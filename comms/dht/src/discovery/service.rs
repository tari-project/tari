// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    discovery::{
        requester::{DhtDiscoveryRequest, DiscoverPeerRequest},
        DhtDiscoveryError,
    },
    envelope::{DhtMessageType, NodeDestination},
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageParams},
    proto::dht::{DiscoveryMessage, DiscoveryResponseMessage},
    DhtConfig,
};
use futures::{
    channel::{mpsc, oneshot},
    future::FutureExt,
    StreamExt,
};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::{collections::HashMap, sync::Arc};
use tari_comms::{
    connection_manager::{ConnectionManagerError, ConnectionManagerRequester},
    log_if_error,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManager},
    types::CommsPublicKey,
    validate_peer_addresses,
    ConnectionManagerEvent,
};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "comms::dht::discovery_service";

/// The number of consecutive times that attempts to connect should
/// fail before marking the peer as offline
const MAX_FAILED_ATTEMPTS_MARK_PEER_OFFLINE: usize = 10;

struct DiscoveryRequestState {
    reply_tx: oneshot::Sender<Result<Peer, DhtDiscoveryError>>,
    public_key: Box<CommsPublicKey>,
}

pub struct DhtDiscoveryService {
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    outbound_requester: OutboundMessageRequester,
    connection_manager: ConnectionManagerRequester,
    peer_manager: Arc<PeerManager>,
    request_rx: Option<mpsc::Receiver<DhtDiscoveryRequest>>,
    shutdown_signal: Option<ShutdownSignal>,
    inflight_discoveries: HashMap<u64, DiscoveryRequestState>,
}

impl DhtDiscoveryService {
    pub fn new(
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_requester: OutboundMessageRequester,
        connection_manager: ConnectionManagerRequester,
        request_rx: mpsc::Receiver<DhtDiscoveryRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            config,
            outbound_requester,
            connection_manager,
            node_identity,
            peer_manager,
            shutdown_signal: Some(shutdown_signal),
            request_rx: Some(request_rx),
            inflight_discoveries: HashMap::new(),
        }
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("DiscoveryService initialized without shutdown_signal")
            .fuse();

        let mut request_rx = self
            .request_rx
            .take()
            .expect("DiscoveryService initialized without request_rx")
            .fuse();

        let mut connection_events = self.connection_manager.get_event_subscription().fuse();

        loop {
            futures::select! {
                request = request_rx.select_next_some() => {
                    trace!(target: LOG_TARGET, "Received request '{}'", request);
                    self.handle_request(request).await;
                },

                event = connection_events.select_next_some() => {
                    if let Ok(event) = event {
                        trace!(target: LOG_TARGET, "Received connection manager event '{}'", event);
                        self.handle_connection_manager_event(&event).await;
                    }
                },

                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Discovery service is shutting down because the shutdown signal was received");
                    break;
                }
            }
        }
    }

    async fn handle_request(&mut self, request: DhtDiscoveryRequest) {
        use DhtDiscoveryRequest::*;
        match request {
            DiscoverPeer(boxed) => {
                let (request, reply_tx) = *boxed;
                log_if_error!(
                    target: LOG_TARGET,
                    self.initiate_peer_discovery(request, reply_tx).await,
                    "Failed to initiate a discovery request because '{error}'",
                );
            },

            NotifyDiscoveryResponseReceived(discovery_msg) => self.handle_discovery_response(*discovery_msg).await,
        }
    }

    async fn handle_connection_manager_event(&mut self, event: &ConnectionManagerEvent) {
        use ConnectionManagerEvent::*;
        // The connection manager could not dial the peer on any address
        if let PeerConnectFailed(node_id, ConnectionManagerError::DialConnectFailedAllAddresses) = event {
            // Send out a discovery for that peer without keeping track of it as an inflight discovery
            match self.peer_manager.find_by_node_id(node_id).await {
                Ok(peer) => {
                    if peer.connection_stats.failed_attempts() > MAX_FAILED_ATTEMPTS_MARK_PEER_OFFLINE {
                        debug!(
                            target: LOG_TARGET,
                            "Deleting stale peer '{}' because this node failed to connect to them {} times",
                            peer.node_id.short_str(),
                            MAX_FAILED_ATTEMPTS_MARK_PEER_OFFLINE
                        );
                        if let Err(err) = self.peer_manager.set_offline(&peer.public_key, true).await {
                            error!(target: LOG_TARGET, "Failed to mark peer as offline because '{:?}'", err);
                        }
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "Attempting to discover peer '{}' because we failed to connect on all addresses for the \
                             peer",
                            peer.node_id.short_str()
                        );
                        // Attempt to discover them
                        let request = DiscoverPeerRequest {
                            dest_public_key: Box::new(peer.public_key),
                            // TODO: This should be the node region, not the node id
                            dest_node_id: Some(peer.node_id),
                            destination: Default::default(),
                        };
                        // Don't need to be notified for this discovery
                        let (reply_tx, _) = oneshot::channel();
                        if let Err(err) = self.initiate_peer_discovery(request, reply_tx).await {
                            error!(target: LOG_TARGET, "Error sending discovery message: {:?}", err);
                        }
                    }
                },
                Err(err) => error!(target: LOG_TARGET, "{:?}", err),
            }
        }
    }

    fn collect_all_discovery_requests(&mut self, public_key: &CommsPublicKey) -> Vec<DiscoveryRequestState> {
        let mut requests = Vec::new();
        let mut remaining_requests = HashMap::new();
        for (nonce, request) in self.inflight_discoveries.drain() {
            // Exclude canceled requests
            if request.reply_tx.is_canceled() {
                continue;
            }

            // Requests for this public key are collected
            if &*request.public_key == public_key {
                requests.push(request);
                continue;
            }

            // Everything else is put back in inflight_discoveries
            remaining_requests.insert(nonce, request);
        }

        self.inflight_discoveries = remaining_requests;
        requests
    }

    async fn handle_discovery_response(&mut self, discovery_msg: DiscoveryResponseMessage) {
        trace!(
            target: LOG_TARGET,
            "Received discovery response message from {}",
            discovery_msg.node_id.to_hex()
        );
        if let Some(request) = log_if_error!(
            target: LOG_TARGET,
            self.inflight_discoveries
                .remove(&discovery_msg.nonce)
                .ok_or_else(|| DhtDiscoveryError::InflightDiscoveryRequestNotFound),
            "{error}",
        ) {
            let DiscoveryRequestState { public_key, reply_tx } = request;

            let result = self.validate_then_add_peer(&public_key, discovery_msg).await;

            // Resolve any other pending discover requests if the peer was found
            if let Ok(peer) = &result {
                for request in self.collect_all_discovery_requests(&public_key) {
                    let _ = request.reply_tx.send(Ok(peer.clone()));
                }

                debug!(
                    target: LOG_TARGET,
                    "Discovery request for Node Id: {} is successfully completed",
                    peer.node_id.to_hex(),
                );
            }

            let _ = reply_tx.send(result);
        }
    }

    async fn validate_then_add_peer(
        &mut self,
        public_key: &CommsPublicKey,
        discovery_msg: DiscoveryResponseMessage,
    ) -> Result<Peer, DhtDiscoveryError>
    {
        let node_id = self.validate_raw_node_id(&public_key, &discovery_msg.node_id)?;

        let addresses = discovery_msg
            .addresses
            .into_iter()
            .filter_map(|addr| addr.parse().ok())
            .collect::<Vec<_>>();

        validate_peer_addresses(&addresses, self.config.network.is_localtest())
            .map_err(|err| DhtDiscoveryError::InvalidPeerMultiaddr(err.to_string()))?;

        let peer = self
            .add_or_update_peer(
                &public_key,
                node_id,
                addresses,
                PeerFeatures::from_bits_truncate(discovery_msg.peer_features),
            )
            .await?;

        Ok(peer)
    }

    fn validate_raw_node_id(
        &self,
        public_key: &CommsPublicKey,
        raw_node_id: &[u8],
    ) -> Result<NodeId, DhtDiscoveryError>
    {
        // The reason that we check the given node id against what we expect instead of just using the given node id
        // is in future the NodeId may not necessarily be derived from the public key (i.e. DAN node is registered on
        // the base layer)
        let expected_node_id = NodeId::from_key(public_key).map_err(|_| DhtDiscoveryError::InvalidNodeId)?;
        let node_id = NodeId::from_bytes(raw_node_id).map_err(|_| DhtDiscoveryError::InvalidNodeId)?;
        if expected_node_id == node_id {
            Ok(expected_node_id)
        } else {
            // TODO: Misbehaviour #banheuristic
            Err(DhtDiscoveryError::InvalidNodeId)
        }
    }

    async fn add_or_update_peer(
        &self,
        pubkey: &CommsPublicKey,
        node_id: NodeId,
        net_addresses: Vec<Multiaddr>,
        peer_features: PeerFeatures,
    ) -> Result<Peer, DhtDiscoveryError>
    {
        let peer_manager = &self.peer_manager;
        if peer_manager.exists(pubkey).await {
            peer_manager
                .update_peer(
                    pubkey,
                    Some(node_id),
                    Some(net_addresses),
                    None,
                    Some(peer_features),
                    None,
                    None,
                )
                .await?;
        } else {
            peer_manager
                .add_peer(Peer::new(
                    pubkey.clone(),
                    node_id,
                    net_addresses.into(),
                    PeerFlags::default(),
                    peer_features,
                    // We don't know which protocols the peer supports. This is ok because:
                    // 1) supported protocols are considered "extra" information and are not needed for p2p comms, and
                    // 2) when a connection is established with this node, supported protocols information is obtained
                    &[],
                ))
                .await?;
        }

        let peer = peer_manager.find_by_public_key(&pubkey).await?;

        Ok(peer)
    }

    async fn initiate_peer_discovery(
        &mut self,
        discovery_request: DiscoverPeerRequest,
        reply_tx: oneshot::Sender<Result<Peer, DhtDiscoveryError>>,
    ) -> Result<(), DhtDiscoveryError>
    {
        let nonce = OsRng.next_u64();
        let public_key = discovery_request.dest_public_key.clone();
        self.send_discover(nonce, discovery_request).await?;

        let inflight_count = self.inflight_discoveries.len();

        // Take this opportunity to clear cancelled discovery requests (e.g if the caller has timed out the request)
        self.inflight_discoveries = self
            .inflight_discoveries
            .drain()
            .filter(|(_, state)| !state.reply_tx.is_canceled())
            .collect();

        trace!(
            target: LOG_TARGET,
            "{} inflight request(s) cleared",
            inflight_count - self.inflight_discoveries.len()
        );

        // Add the new inflight request.
        let key_exists = self
            .inflight_discoveries
            .insert(nonce, DiscoveryRequestState { reply_tx, public_key })
            .is_some();
        // The nonce should never be chosen more than once
        debug_assert!(!key_exists);

        trace!(
            target: LOG_TARGET,
            "Number of inflight discoveries = {}",
            self.inflight_discoveries.len()
        );

        Ok(())
    }

    async fn send_discover(
        &mut self,
        nonce: u64,
        discovery_request: DiscoverPeerRequest,
    ) -> Result<(), DhtDiscoveryError>
    {
        let DiscoverPeerRequest {
            dest_node_id,
            dest_public_key,
            destination,
        } = discovery_request;

        // If the destination node is is known, send to the closest peers we know. Otherwise...
        let network_location_node_id = dest_node_id
            .or_else(|| match &destination {
                // ... if the destination is undisclosed or a public key, send discover to our closest peers
                NodeDestination::Unknown | NodeDestination::PublicKey(_) => Some(self.node_identity.node_id().clone()),
                // otherwise, send it to the closest peers to the given NodeId destination we know
                NodeDestination::NodeId(node_id) => Some(*node_id.clone()),
            })
            .expect("cannot fail");

        let discover_msg = DiscoveryMessage {
            node_id: self.node_identity.node_id().to_vec(),
            addresses: vec![self.node_identity.public_address().to_string()],
            peer_features: self.node_identity.features().bits(),
            nonce,
        };
        debug!(
            target: LOG_TARGET,
            "Sending Discovery message for Node Id: {}", destination
        );

        self.outbound_requester
            .send_message_no_header(
                SendMessageParams::new()
                    .closest(
                        network_location_node_id,
                        self.config.num_neighbouring_nodes,
                        Vec::new(),
                        PeerFeatures::empty(),
                    )
                    .with_destination(destination)
                    .with_encryption(OutboundEncryption::EncryptFor(dest_public_key))
                    .with_dht_message_type(DhtMessageType::Discovery)
                    .finish(),
                discover_msg,
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        discovery::DhtDiscoveryRequester,
        outbound::mock::create_outbound_service_mock,
        test_utils::{make_node_identity, make_peer_manager},
    };
    use std::time::Duration;
    use tari_comms::test_utils::mocks::create_connection_manager_mock;
    use tari_shutdown::Shutdown;
    use tari_test_utils::runtime;

    #[test]
    fn send_discovery() {
        runtime::test_async(|rt| {
            let node_identity = make_node_identity();
            let peer_manager = make_peer_manager();
            let (outbound_requester, outbound_mock) = create_outbound_service_mock(10);
            let oms_mock_state = outbound_mock.get_state();
            rt.spawn(outbound_mock.run());

            let (connection_manager, _) = create_connection_manager_mock(1);
            let (sender, receiver) = mpsc::channel(10);
            // Requester which timeout instantly
            let mut requester = DhtDiscoveryRequester::new(sender, Duration::from_millis(1));
            let mut shutdown = Shutdown::new();

            let service = DhtDiscoveryService::new(
                DhtConfig::default(),
                node_identity,
                peer_manager,
                outbound_requester,
                connection_manager,
                receiver,
                shutdown.to_signal(),
            );

            rt.spawn(service.run());

            let dest_public_key = Box::new(CommsPublicKey::default());
            let result = rt.block_on(requester.discover_peer(dest_public_key.clone(), None, NodeDestination::Unknown));

            assert!(result.unwrap_err().is_timeout());

            oms_mock_state.wait_call_count(1, Duration::from_secs(5)).unwrap();
            let (params, _) = oms_mock_state.pop_call().unwrap();
            assert_eq!(params.dht_message_type, DhtMessageType::Discovery);
            assert_eq!(params.encryption, OutboundEncryption::EncryptFor(dest_public_key));

            shutdown.trigger().unwrap();
        })
    }
}
