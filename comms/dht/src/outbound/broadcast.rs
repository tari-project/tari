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

use super::{error::DhtOutboundError, message::DhtOutboundRequest};
use crate::{
    actor::DhtRequester,
    broadcast_strategy::BroadcastStrategy,
    discovery::DhtDiscoveryRequester,
    envelope::{DhtMessageHeader, NodeDestination},
    outbound::message::{DhtOutboundMessage, ForwardRequest, OutboundEncryption, SendMessageRequest},
};
use futures::{
    future,
    stream::{self, StreamExt},
    task::Context,
    Future,
    Poll,
};
use log::*;
use std::sync::Arc;
use tari_comms::{
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures},
    types::CommsPublicKey,
};
use tari_comms_middleware::MiddlewareError;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::dht::outbound::broadcast_middleware";

pub struct BroadcastLayer {
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    node_identity: Arc<NodeIdentity>,
}

impl BroadcastLayer {
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
    ) -> Self
    {
        BroadcastLayer {
            node_identity,
            dht_requester,
            dht_discovery_requester,
        }
    }
}

impl<S> Layer<S> for BroadcastLayer {
    type Service = BroadcastMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        BroadcastMiddleware::new(
            service,
            Arc::clone(&self.node_identity),
            self.dht_requester.clone(),
            self.dht_discovery_requester.clone(),
        )
    }
}

/// Responsible for constructing messages using a broadcast strategy and passing them on to
/// the worker task.
#[derive(Clone)]
pub struct BroadcastMiddleware<S> {
    next: S,
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    node_identity: Arc<NodeIdentity>,
}

impl<S> BroadcastMiddleware<S> {
    pub fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
    ) -> Self
    {
        Self {
            next: service,
            dht_requester,
            dht_discovery_requester,
            node_identity,
        }
    }
}

impl<S> Service<DhtOutboundRequest> for BroadcastMiddleware<S>
where S: Service<DhtOutboundMessage, Response = (), Error = MiddlewareError> + Clone
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<(), Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtOutboundRequest) -> Self::Future {
        BroadcastTask::new(
            self.next.clone(),
            Arc::clone(&self.node_identity),
            self.dht_requester.clone(),
            self.dht_discovery_requester.clone(),
            msg,
        )
        .handle()
    }
}

struct BroadcastTask<S> {
    service: S,
    node_identity: Arc<NodeIdentity>,
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    request: Option<DhtOutboundRequest>,
}

impl<S> BroadcastTask<S>
where S: Service<DhtOutboundMessage, Response = (), Error = MiddlewareError>
{
    pub fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        request: DhtOutboundRequest,
    ) -> Self
    {
        Self {
            service,
            node_identity,
            dht_requester,
            dht_discovery_requester,
            request: Some(request),
        }
    }

    pub async fn handle(mut self) -> Result<(), MiddlewareError> {
        let request = self.request.take().expect("request cannot be None");
        debug!(target: LOG_TARGET, "Processing outbound request {}", request);
        let messages = self.generate_outbound_messages(request).await?;
        debug!(
            target: LOG_TARGET,
            "Passing {} message(s) to next_service",
            messages.len()
        );

        self.service
            .call_all(stream::iter(messages))
            .unordered()
            .filter_map(|result| future::ready(result.err()))
            .for_each(|err| {
                error!(target: LOG_TARGET, "Error when sending broadcast messages: {}", err);
                future::ready(())
            })
            .await;

        Ok(())
    }

    pub async fn generate_outbound_messages(
        &mut self,
        msg: DhtOutboundRequest,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        match msg {
            DhtOutboundRequest::SendMsg(request, reply_tx) => {
                match self.generate_send_messages(*request).await {
                    Ok(msgs) => {
                        // Reply with the number of messages to be sent
                        let _ = reply_tx.send(msgs.len());
                        Ok(msgs)
                    },
                    Err(err) => {
                        // Reply 0 messages sent
                        let _ = reply_tx.send(0);
                        Err(err)
                    },
                }
            },
            DhtOutboundRequest::Forward(request) => {
                if self.node_identity.has_peer_features(PeerFeatures::MESSAGE_PROPAGATION) {
                    self.generate_forward_messages(*request).await
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Message propagation is not enabled on this node. Discarding request to propagate message"
                    );
                    Ok(Vec::new())
                }
            },
        }
    }

    async fn select_or_discover_peer(
        &mut self,
        dest_public_key: CommsPublicKey,
    ) -> Result<Option<Peer>, DhtOutboundError>
    {
        let mut peers = self
            .dht_requester
            .select_peers(BroadcastStrategy::DirectPublicKey(dest_public_key.clone()))
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                DhtOutboundError::PeerSelectionFailed
            })?;

        if peers.len() > 0 {
            return Ok(Some(peers.remove(0)));
        }

        trace!(
            target: LOG_TARGET,
            "Initiating peer discovery for public key '{}'",
            dest_public_key
        );

        // TODO: This works because we know that all non-DAN node IDs are/should be derived from the public key.
        //       Once the DAN launches, this may not be the case.
        let derived_node_id = NodeId::from_key(&dest_public_key).ok();

        // Peer not found, let's try and discover it
        match self
            .dht_discovery_requester
            .discover_peer(dest_public_key, derived_node_id, NodeDestination::Unknown)
            .await
        {
            // Peer found!
            Ok(peer) => {
                if peer.is_banned() {
                    warn!(
                        target: LOG_TARGET,
                        "Peer discovery succeeded however peer with public key '{}' is marked as banned.",
                        peer.public_key
                    );
                    return Ok(None);
                }

                debug!(
                    target: LOG_TARGET,
                    "Peer discovery succeeded for public key '{}'.", peer.public_key
                );
                Ok(Some(peer))
            },
            // Error during discovery
            Err(err) => {
                debug!(target: LOG_TARGET, "Peer discovery failed because '{}'.", err);
                Ok(None)
            },
        }
    }

    async fn generate_send_messages(
        &mut self,
        send_message_request: SendMessageRequest,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        let SendMessageRequest {
            broadcast_strategy,
            destination,
            encryption,
            comms_flags,
            dht_flags,
            dht_message_type,
            body,
        } = send_message_request;

        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // individually wrapped MessageEnvelope to each selected peer
        // If the broadcast strategy is DirectPublicKey and the peer is not known, peer discovery will be initiated.
        let selected_peers = match broadcast_strategy.direct_public_key() {
            Some(_) => {
                let dest_public_key = broadcast_strategy.take_direct_public_key().expect("already checked");
                self.select_or_discover_peer(dest_public_key)
                    .await
                    .map(|peer| peer.map(|p| vec![p]).unwrap_or_default())?
            },
            None => self
                .dht_requester
                .select_peers(broadcast_strategy)
                .await
                .map_err(|err| {
                    error!(target: LOG_TARGET, "{}", err);
                    DhtOutboundError::PeerSelectionFailed
                })?,
        };

        // Create a DHT header
        let dht_header = DhtMessageHeader::new(
            // Final destination for this message
            destination,
            // Origin public key used to identify the origin and verify the signature
            self.node_identity.public_key().clone(),
            // Signing will happen later in the pipeline (SerializeMiddleware), left empty to prevent double work
            Vec::new(),
            dht_message_type,
            dht_flags,
        );

        // Construct a MessageEnvelope for each recipient
        let messages = selected_peers
            .into_iter()
            .map(|peer| {
                DhtOutboundMessage::new(peer, dht_header.clone(), encryption.clone(), comms_flags, body.clone())
            })
            .collect::<Vec<_>>();

        Ok(messages)
    }

    async fn generate_forward_messages(
        &mut self,
        forward_request: ForwardRequest,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        let ForwardRequest {
            broadcast_strategy,
            dht_header,
            comms_flags,
            body,
        } = forward_request;
        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then forward the
        // received message to each selected peer
        let selected_node_identities = self
            .dht_requester
            .select_peers(broadcast_strategy)
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                DhtOutboundError::PeerSelectionFailed
            })?;

        let messages = selected_node_identities
            .into_iter()
            .map(|peer| {
                DhtOutboundMessage::new(
                    peer,
                    dht_header.clone(),
                    // Forwarding the message as is, no encryption
                    OutboundEncryption::None,
                    comms_flags,
                    body.clone(),
                )
            })
            .collect();

        Ok(messages)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        broadcast_strategy::BroadcastStrategy,
        envelope::{DhtMessageFlags, NodeDestination},
        outbound::message::OutboundEncryption,
        proto::envelope::DhtMessageType,
        test_utils::{
            create_dht_actor_mock,
            create_dht_discovery_mock,
            make_peer,
            service_spy,
            DhtDiscoveryMockState,
            DhtMockState,
        },
    };
    use futures::channel::oneshot;
    use rand::rngs::OsRng;
    use std::time::Duration;
    use tari_comms::{
        connection::NetAddress,
        message::MessageFlags,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tokio::runtime::Runtime;

    #[test]
    fn send_message_flood() {
        let rt = Runtime::new().unwrap();

        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            vec!["127.0.0.1:9999".parse::<NetAddress>().unwrap()].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
        );

        let other_peer = {
            let mut p = example_peer.clone();
            let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng::new().unwrap());
            p.node_id = NodeId::from_key(&pk).unwrap();
            p.public_key = pk;
            p
        };

        let node_identity = Arc::new(
            NodeIdentity::random(
                &mut OsRng::new().unwrap(),
                "127.0.0.1:9000".parse().unwrap(),
                PeerFeatures::COMMUNICATION_NODE,
            )
            .unwrap(),
        );

        let (dht_requester, mut dht_mock) = create_dht_actor_mock(10);
        let (dht_discover_requester, _) = create_dht_discovery_mock(10, Duration::from_secs(10));

        let mock_state = DhtMockState::new();
        mock_state.set_select_peers_response(vec![example_peer.clone(), other_peer.clone()]);
        dht_mock.set_shared_state(mock_state);

        rt.spawn(dht_mock.run());

        let spy = service_spy();

        let mut service =
            BroadcastMiddleware::new(spy.to_service(), node_identity, dht_requester, dht_discover_requester);
        let (reply_tx, _reply_rx) = oneshot::channel();

        rt.block_on(service.call(DhtOutboundRequest::SendMsg(
            Box::new(SendMessageRequest {
                broadcast_strategy: BroadcastStrategy::Flood,
                comms_flags: MessageFlags::NONE,
                destination: NodeDestination::Unknown,
                encryption: OutboundEncryption::None,
                dht_message_type: DhtMessageType::None,
                dht_flags: DhtMessageFlags::NONE,
                body: "custom_msg".as_bytes().to_vec(),
            }),
            reply_tx,
        )))
        .unwrap();

        assert_eq!(spy.call_count(), 2);
        let requests = spy.take_requests();
        assert!(requests
            .iter()
            .any(|msg| msg.destination_peer.node_id == example_peer.node_id));
        assert!(requests
            .iter()
            .any(|msg| msg.destination_peer.node_id == other_peer.node_id));
    }

    #[test]
    fn send_message_direct_not_found() {
        // Test for issue https://github.com/tari-project/tari/issues/959
        let rt = Runtime::new().unwrap();

        let pk = CommsPublicKey::default();
        let node_identity = NodeIdentity::random(
            &mut OsRng::new().unwrap(),
            "127.0.0.1:9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap();

        let (dht_requester, dht_mock) = create_dht_actor_mock(10);
        rt.spawn(dht_mock.run());
        let (dht_discover_requester, _) = create_dht_discovery_mock(10, Duration::from_secs(10));
        let spy = service_spy();

        let mut service = BroadcastMiddleware::new(
            spy.to_service(),
            Arc::new(node_identity),
            dht_requester,
            dht_discover_requester,
        );
        let (reply_tx, reply_rx) = oneshot::channel();

        rt.block_on(service.call(DhtOutboundRequest::SendMsg(
            Box::new(SendMessageRequest {
                broadcast_strategy: BroadcastStrategy::DirectPublicKey(pk),
                comms_flags: MessageFlags::NONE,
                destination: NodeDestination::Unknown,
                encryption: OutboundEncryption::None,
                dht_message_type: DhtMessageType::None,
                dht_flags: DhtMessageFlags::NONE,
                body: "custom_msg".as_bytes().to_vec(),
            }),
            reply_tx,
        )))
        .unwrap();

        let num_peers_selected = rt.block_on(reply_rx).unwrap();
        assert_eq!(num_peers_selected, 0);

        assert_eq!(spy.call_count(), 0);
    }

    #[test]
    fn send_message_direct_dht_discovery() {
        let rt = Runtime::new().unwrap();

        let node_identity = NodeIdentity::random(
            &mut OsRng::new().unwrap(),
            "127.0.0.1:9000".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap();

        let (dht_requester, dht_mock) = create_dht_actor_mock(10);
        rt.spawn(dht_mock.run());
        let (dht_discover_requester, mut discovery_mock) = create_dht_discovery_mock(10, Duration::from_secs(10));
        let dht_discovery_state = DhtDiscoveryMockState::new();
        discovery_mock.set_shared_state(dht_discovery_state.clone());
        rt.spawn(discovery_mock.run());

        let peer_to_discover = make_peer();
        dht_discovery_state.set_discover_peer_response(peer_to_discover.clone());

        let spy = service_spy();

        let mut service = BroadcastMiddleware::new(
            spy.to_service(),
            Arc::new(node_identity),
            dht_requester,
            dht_discover_requester,
        );
        let (reply_tx, reply_rx) = oneshot::channel();

        rt.block_on(service.call(DhtOutboundRequest::SendMsg(
            Box::new(SendMessageRequest {
                broadcast_strategy: BroadcastStrategy::DirectPublicKey(peer_to_discover.public_key.clone()),
                comms_flags: MessageFlags::NONE,
                destination: NodeDestination::Unknown,
                encryption: OutboundEncryption::None,
                dht_message_type: DhtMessageType::None,
                dht_flags: DhtMessageFlags::NONE,
                body: "custom_msg".as_bytes().to_vec(),
            }),
            reply_tx,
        )))
        .unwrap();

        let num_peers_selected = rt.block_on(reply_rx).unwrap();
        assert_eq!(num_peers_selected, 1);
        assert_eq!(dht_discovery_state.call_count(), 1);

        assert_eq!(spy.call_count(), 1);
    }
}
