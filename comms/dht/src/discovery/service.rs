// Copyright 2019, The Taiji Project
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

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::Instant,
};

use log::*;
use rand::{rngs::OsRng, RngCore};
use taiji_comms::{
    log_if_error,
    multiaddr::Multiaddr,
    net_address::PeerAddressSource,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerIdentityClaim, PeerManager},
    types::CommsPublicKey,
    validate_addresses,
};
use taiji_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, ByteArray};
use tokio::{
    sync::{mpsc, oneshot},
    task,
};

use crate::{
    discovery::{requester::DhtDiscoveryRequest, DhtDiscoveryError},
    envelope::{DhtMessageType, NodeDestination},
    outbound::{OutboundEncryption, OutboundMessageRequester, SendMessageParams},
    proto::dht::{DiscoveryMessage, DiscoveryResponseMessage},
    DhtConfig,
};

const LOG_TARGET: &str = "comms::dht::discovery_service";

struct DiscoveryRequestState {
    reply_tx: oneshot::Sender<Result<Peer, DhtDiscoveryError>>,
    public_key: Box<CommsPublicKey>,
    start_ts: Instant,
}

impl DiscoveryRequestState {
    pub fn new(public_key: Box<CommsPublicKey>, reply_tx: oneshot::Sender<Result<Peer, DhtDiscoveryError>>) -> Self {
        Self {
            public_key,
            reply_tx,
            start_ts: Instant::now(),
        }
    }
}

pub struct DhtDiscoveryService {
    config: Arc<DhtConfig>,
    node_identity: Arc<NodeIdentity>,
    outbound_requester: OutboundMessageRequester,
    peer_manager: Arc<PeerManager>,
    request_rx: mpsc::Receiver<DhtDiscoveryRequest>,
    shutdown_signal: ShutdownSignal,
    inflight_discoveries: HashMap<u64, DiscoveryRequestState>,
}

impl DhtDiscoveryService {
    pub fn new(
        config: Arc<DhtConfig>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_requester: OutboundMessageRequester,
        request_rx: mpsc::Receiver<DhtDiscoveryRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            config,
            outbound_requester,
            node_identity,
            peer_manager,
            shutdown_signal,
            request_rx,
            inflight_discoveries: HashMap::new(),
        }
    }

    pub fn spawn(self) {
        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        task::spawn(async move {
            log_mdc::extend(mdc);
            info!(target: LOG_TARGET, "Discovery service started");
            self.run().await
        });
    }

    pub async fn run(mut self) {
        debug!(target: LOG_TARGET, "Dht discovery service started");
        loop {
            tokio::select! {
                biased;

                _ = self.shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "Discovery service is shutting down because the shutdown signal was received");
                    break;
                }

                Some(request) = self.request_rx.recv() => {
                    trace!(target: LOG_TARGET, "Received request '{}'", request);
                    self.handle_request(request).await;
                },
            }
        }
    }

    async fn handle_request(&mut self, request: DhtDiscoveryRequest) {
        use DhtDiscoveryRequest::{DiscoverPeer, NotifyDiscoveryResponseReceived};
        match request {
            DiscoverPeer(dest_pubkey, destination, reply_tx) => {
                log_if_error!(
                    target: LOG_TARGET,
                    self.initiate_peer_discovery(dest_pubkey, destination, reply_tx).await,
                    "Failed to initiate a discovery request because '{error}'",
                );
            },

            NotifyDiscoveryResponseReceived(discovery_msg) => self.handle_discovery_response(discovery_msg).await,
        }
    }

    fn collect_all_discovery_requests(&mut self, public_key: &CommsPublicKey) -> Vec<DiscoveryRequestState> {
        let mut requests = Vec::new();
        let mut remaining_requests = HashMap::new();
        for (nonce, request) in self.inflight_discoveries.drain() {
            // Exclude canceled requests
            if request.reply_tx.is_closed() {
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

    async fn handle_discovery_response(&mut self, discovery_msg: Box<DiscoveryResponseMessage>) {
        trace!(
            target: LOG_TARGET,
            "Received discovery response message from {}",
            discovery_msg.public_key.to_hex()
        );

        match self.inflight_discoveries.remove(&discovery_msg.nonce) {
            Some(request) => {
                let DiscoveryRequestState {
                    public_key,
                    reply_tx,
                    start_ts,
                } = request;

                let result = self.validate_then_add_peer(&public_key, discovery_msg).await;

                // Resolve any other pending discover requests if the peer was found
                match &result {
                    Ok(peer) => {
                        debug!(
                            target: LOG_TARGET,
                            "Received discovery response from peer {}. Discovery completed in {}s",
                            peer.node_id,
                            (Instant::now() - start_ts).as_secs_f32()
                        );

                        for request in self.collect_all_discovery_requests(&public_key) {
                            if !reply_tx.is_closed() {
                                let _result = request.reply_tx.send(Ok(peer.clone()));
                            }
                        }

                        debug!(
                            target: LOG_TARGET,
                            "Discovery request for Node Id {} completed successfully",
                            peer.node_id.to_hex(),
                        );
                    },
                    Err(err) => {
                        debug!(
                            target: LOG_TARGET,
                            "Failed to validate and add peer from discovery response from peer. {:?} Discovery \
                             completed in {}s",
                            err,
                            (Instant::now() - start_ts).as_secs_f32()
                        );
                    },
                }

                let _result = reply_tx.send(result);
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Received a discovery response from peer '{}' that this node did not expect. It may have been \
                     cancelled earlier.",
                    discovery_msg.public_key.to_hex()
                );
            },
        }
    }

    async fn validate_then_add_peer(
        &mut self,
        public_key: &CommsPublicKey,
        discovery_msg: Box<DiscoveryResponseMessage>,
    ) -> Result<Peer, DhtDiscoveryError> {
        let node_id = NodeId::from_public_key(public_key);

        let addresses: Vec<Multiaddr> = discovery_msg
            .addresses
            .into_iter()
            .map(Multiaddr::try_from)
            .collect::<Result<_, _>>()
            .map_err(|e| DhtDiscoveryError::InvalidPeerMultiaddr(e.to_string()))?;

        validate_addresses(&addresses, self.config.allow_test_addresses)
            .map_err(|err| DhtDiscoveryError::InvalidPeerMultiaddr(err.to_string()))?;

        let peer_identity_claim = PeerIdentityClaim::new(
            addresses.clone(),
            PeerFeatures::from_bits_truncate(discovery_msg.peer_features),
            discovery_msg
                .identity_signature
                .ok_or(DhtDiscoveryError::NoSignatureProvided)?
                .try_into()
                .map_err(|e: anyhow::Error| DhtDiscoveryError::InvalidSignature(e.to_string()))?,
            None,
        );

        let peer = self
            .peer_manager
            .add_or_update_online_peer(
                public_key,
                node_id,
                addresses,
                PeerFeatures::from_bits_truncate(discovery_msg.peer_features),
                &PeerAddressSource::FromDiscovery { peer_identity_claim },
            )
            .await?;

        Ok(peer)
    }

    async fn initiate_peer_discovery(
        &mut self,
        dest_pubkey: Box<CommsPublicKey>,
        destination: NodeDestination,
        reply_tx: oneshot::Sender<Result<Peer, DhtDiscoveryError>>,
    ) -> Result<(), DhtDiscoveryError> {
        let nonce = OsRng.next_u64();
        if let Err(err) = self.send_discover(nonce, destination, dest_pubkey.clone()).await {
            let _result = reply_tx.send(Err(err));
            return Ok(());
        }

        let inflight_count = self.inflight_discoveries.len();

        // Take this opportunity to clear cancelled discovery requests (e.g if the caller has timed out the request)
        self.inflight_discoveries = self
            .inflight_discoveries
            .drain()
            .filter(|(_, state)| !state.reply_tx.is_closed())
            .collect();

        trace!(
            target: LOG_TARGET,
            "{} inflight request(s) cleared",
            inflight_count - self.inflight_discoveries.len()
        );

        // Add the new inflight request.
        self.inflight_discoveries
            .insert(nonce, DiscoveryRequestState::new(dest_pubkey, reply_tx));

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
        destination: NodeDestination,
        dest_public_key: Box<CommsPublicKey>,
    ) -> Result<(), DhtDiscoveryError> {
        let discover_msg = DiscoveryMessage {
            public_key: self.node_identity.public_key().to_vec(),
            addresses: self
                .node_identity
                .public_addresses()
                .into_iter()
                .map(|a| a.to_vec())
                .collect(),
            peer_features: self.node_identity.features().bits(),
            nonce,
            identity_signature: self.node_identity.identity_signature_read().as_ref().map(Into::into),
        };
        debug!(
            target: LOG_TARGET,
            "Sending Discovery message for peer public key '{}' with destination {}", &dest_public_key, destination
        );

        self.outbound_requester
            .send_message_no_header(
                SendMessageParams::new()
                    .broadcast(Vec::new())
                    .with_destination(destination)
                    .with_debug_info(format!("discover: {}", &dest_public_key))
                    .with_encryption(OutboundEncryption::EncryptFor(dest_public_key))
                    .with_dht_message_type(DhtMessageType::Discovery)
                    .finish(),
                discover_msg,
            )
            .await?
            .resolve()
            .await
            .map_err(DhtDiscoveryError::DiscoverySendFailed)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use taiji_shutdown::Shutdown;

    use super::*;
    use crate::{
        discovery::DhtDiscoveryRequester,
        outbound::mock::create_outbound_service_mock,
        test_utils::{build_peer_manager, make_node_identity},
    };

    #[tokio::test]
    async fn send_discovery() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (outbound_requester, outbound_mock) = create_outbound_service_mock(10);
        let oms_mock_state = outbound_mock.get_state();
        task::spawn(outbound_mock.run());

        let (sender, receiver) = mpsc::channel(10);
        // Requester which timeout instantly
        let mut requester = DhtDiscoveryRequester::new(sender, Duration::from_millis(1));
        let shutdown = Shutdown::new();

        DhtDiscoveryService::new(
            Default::default(),
            node_identity,
            peer_manager,
            outbound_requester,
            receiver,
            shutdown.to_signal(),
        )
        .spawn();

        let dest_public_key = Box::<tari_crypto::ristretto::RistrettoPublicKey>::default();
        let result = requester
            .discover_peer(
                *dest_public_key.clone(),
                NodeDestination::PublicKey(dest_public_key.clone()),
            )
            .await;

        assert!(result.unwrap_err().is_timeout());

        oms_mock_state.wait_call_count(1, Duration::from_secs(5)).await.unwrap();
        let (params, _) = oms_mock_state.pop_call().await.unwrap();
        assert_eq!(params.dht_message_type, DhtMessageType::Discovery);
        assert_eq!(params.encryption, OutboundEncryption::EncryptFor(dest_public_key));
    }
}
