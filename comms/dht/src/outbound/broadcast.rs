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
    envelope::{DhtMessageHeader, DhtMessageOrigin, NodeDestination},
    outbound::{
        message::{DhtOutboundMessage, OutboundEncryption},
        message_params::FinalSendMessageParams,
        SendMessageResponse,
    },
    proto::envelope::{DhtMessageType, Network},
    PipelineError,
};
use futures::{
    channel::oneshot,
    future,
    stream::{self, StreamExt},
    task::Context,
    Future,
};
use log::*;
use std::{sync::Arc, task::Poll};
use tari_comms::{
    message::MessageFlags,
    peer_manager::{NodeId, NodeIdentity, Peer},
    types::CommsPublicKey,
};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::outbound::broadcast_middleware";

pub struct BroadcastLayer {
    dht_requester: DhtRequester,
    dht_discovery_requester: DhtDiscoveryRequester,
    node_identity: Arc<NodeIdentity>,
    target_network: Network,
}

impl BroadcastLayer {
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        target_network: Network,
    ) -> Self
    {
        BroadcastLayer {
            node_identity,
            dht_requester,
            dht_discovery_requester,
            target_network,
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
            self.target_network,
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
    target_network: Network,
}

impl<S> BroadcastMiddleware<S> {
    pub fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        target_network: Network,
    ) -> Self
    {
        Self {
            next: service,
            dht_requester,
            dht_discovery_requester,
            node_identity,
            target_network,
        }
    }
}

impl<S> Service<DhtOutboundRequest> for BroadcastMiddleware<S>
where S: Service<DhtOutboundMessage, Response = (), Error = PipelineError> + Clone
{
    type Error = PipelineError;
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
            self.target_network,
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
    target_network: Network,
}

impl<S> BroadcastTask<S>
where S: Service<DhtOutboundMessage, Response = (), Error = PipelineError>
{
    pub fn new(
        service: S,
        node_identity: Arc<NodeIdentity>,
        dht_requester: DhtRequester,
        dht_discovery_requester: DhtDiscoveryRequester,
        target_network: Network,
        request: DhtOutboundRequest,
    ) -> Self
    {
        Self {
            service,
            node_identity,
            dht_requester,
            dht_discovery_requester,
            target_network,
            request: Some(request),
        }
    }

    pub async fn handle(mut self) -> Result<(), PipelineError> {
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
            DhtOutboundRequest::SendMessage(params, body, reply_tx) => {
                self.handle_send_message(*params, body, reply_tx).await
            },
        }
    }

    async fn handle_send_message(
        &mut self,
        params: FinalSendMessageParams,
        body: Vec<u8>,
        reply_tx: oneshot::Sender<SendMessageResponse>,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        let FinalSendMessageParams {
            broadcast_strategy,
            destination,
            dht_message_type,
            encryption,
            is_discovery_enabled,
            force_origin,
            dht_header,
        } = params;

        if broadcast_strategy
            .direct_public_key()
            .filter(|pk| *pk == self.node_identity.public_key())
            .is_some()
        {
            warn!(target: LOG_TARGET, "Attempt to send to own peer");
            return Err(DhtOutboundError::SendToOurselves);
        }

        match self.select_peers(broadcast_strategy.clone()).await {
            Ok(mut peers) => {
                if reply_tx.is_canceled() {
                    return Err(DhtOutboundError::ReplyChannelCanceled);
                }

                let mut reply_tx = Some(reply_tx);

                trace!(
                    target: LOG_TARGET,
                    "Number of peers selected = {}, is_discovery_enabled = {}",
                    peers.len(),
                    is_discovery_enabled,
                );

                // Discovery is required if:
                //  - Discovery is enabled for this request
                //  - There where no peers returned
                //  - A direct public key broadcast strategy is used
                if is_discovery_enabled && peers.is_empty() && broadcast_strategy.direct_public_key().is_some() {
                    let (discovery_reply_tx, discovery_reply_rx) = oneshot::channel();
                    let target_public_key = broadcast_strategy.into_direct_public_key().expect("already checked");

                    let _ = reply_tx
                        .take()
                        .expect("cannot fail")
                        .send(SendMessageResponse::PendingDiscovery(discovery_reply_rx));

                    match self.initiate_peer_discovery(target_public_key).await {
                        Ok(Some(peer)) => {
                            // Set the reply_tx so that it can be used later
                            reply_tx = Some(discovery_reply_tx);
                            peers = vec![peer];
                        },
                        Ok(None) => {
                            // Message sent to 0 peers
                            let _ = discovery_reply_tx.send(SendMessageResponse::Queued(vec![]));
                            return Ok(Vec::new());
                        },
                        Err(err) => {
                            let _ = discovery_reply_tx.send(SendMessageResponse::Failed);
                            return Err(err);
                        },
                    }
                }

                match self
                    .generate_send_messages(
                        peers,
                        destination,
                        dht_message_type,
                        encryption,
                        dht_header,
                        force_origin,
                        body,
                    )
                    .await
                {
                    Ok(msgs) => {
                        // Reply with the number of messages to be sent
                        let _ = reply_tx
                            .take()
                            .expect("cannot fail")
                            .send(SendMessageResponse::Queued(msgs.iter().map(|m| m.tag).collect()));
                        Ok(msgs)
                    },
                    Err(err) => {
                        // Reply 0 messages sent
                        let _ = reply_tx.take().expect("cannot fail").send(SendMessageResponse::Failed);
                        Err(err)
                    },
                }
            },
            Err(err) => {
                let _ = reply_tx.send(SendMessageResponse::Failed);
                Err(err)
            },
        }
    }

    async fn select_peers(&mut self, broadcast_strategy: BroadcastStrategy) -> Result<Vec<Peer>, DhtOutboundError> {
        self.dht_requester
            .select_peers(broadcast_strategy)
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                DhtOutboundError::PeerSelectionFailed
            })
    }

    async fn initiate_peer_discovery(
        &mut self,
        dest_public_key: CommsPublicKey,
    ) -> Result<Option<Peer>, DhtOutboundError>
    {
        trace!(
            target: LOG_TARGET,
            "Initiating peer discovery for public key '{}'",
            dest_public_key
        );

        // TODO: This works because we know that all non-DAN node IDs are/should be derived from the public key.
        //       Once the DAN launches, this may not be the case and we'll need to query the blockchain for the node id
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

    #[allow(clippy::too_many_arguments)]
    async fn generate_send_messages(
        &mut self,
        selected_peers: Vec<Peer>,
        destination: NodeDestination,
        dht_message_type: DhtMessageType,
        encryption: OutboundEncryption,
        custom_header: Option<DhtMessageHeader>,
        force_origin: bool,
        body: Vec<u8>,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        let dht_flags = encryption.flags();

        // Create a DHT header
        let dht_header = custom_header
            .or_else(|| {
                // The origin is specified if encryption is turned on, otherwise it is not
                let origin = if force_origin || encryption.is_encrypt() {
                    Some(DhtMessageOrigin {
                        // Origin public key used to identify the origin and verify the signature
                        public_key: self.node_identity.public_key().clone(),
                        // Signing will happen later in the pipeline (SerializeMiddleware), left empty to prevent double
                        // work
                        signature: Vec::new(),
                    })
                } else {
                    None
                };

                Some(DhtMessageHeader::new(
                    // Final destination for this message
                    destination,
                    dht_message_type,
                    origin,
                    self.target_network,
                    dht_flags,
                ))
            })
            .expect("always Some");

        // Construct a MessageEnvelope for each recipient
        let messages = selected_peers
            .into_iter()
            .map(|peer| {
                DhtOutboundMessage::new(
                    peer,
                    dht_header.clone(),
                    encryption.clone(),
                    MessageFlags::NONE,
                    body.clone(),
                )
            })
            .collect::<Vec<_>>();

        Ok(messages)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        outbound::SendMessageParams,
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
        multiaddr::Multiaddr,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tari_test_utils::unpack_enum;
    use tokio::runtime::Runtime;

    #[test]
    fn send_message_flood() {
        let mut rt = Runtime::new().unwrap();

        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            vec!["/ip4/127.0.0.1/tcp/9999".parse::<Multiaddr>().unwrap()].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            &[],
        );

        let other_peer = {
            let mut p = example_peer.clone();
            let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
            p.node_id = NodeId::from_key(&pk).unwrap();
            p.public_key = pk;
            p
        };

        let node_identity = Arc::new(
            NodeIdentity::random(
                &mut OsRng,
                "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
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

        let mut service = BroadcastMiddleware::new(
            spy.to_service(),
            node_identity,
            dht_requester,
            dht_discover_requester,
            Network::LocalTest,
        );
        let (reply_tx, _reply_rx) = oneshot::channel();

        rt.block_on(service.call(DhtOutboundRequest::SendMessage(
            Box::new(SendMessageParams::new().flood().finish()),
            "custom_msg".as_bytes().to_vec(),
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
        let mut rt = Runtime::new().unwrap();

        let pk = CommsPublicKey::default();
        let node_identity = NodeIdentity::random(
            &mut OsRng,
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
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
            Network::LocalTest,
        );
        let (reply_tx, reply_rx) = oneshot::channel();

        rt.block_on(
            service.call(DhtOutboundRequest::SendMessage(
                Box::new(
                    SendMessageParams::new()
                        .direct_public_key(pk)
                        .with_discovery(false)
                        .finish(),
                ),
                "custom_msg".as_bytes().to_vec(),
                reply_tx,
            )),
        )
        .unwrap();

        let send_message_response = rt.block_on(reply_rx).unwrap();
        unpack_enum!(SendMessageResponse::Queued(tags) = send_message_response);
        assert_eq!(tags.len(), 0);
        assert_eq!(spy.call_count(), 0);
    }

    #[test]
    fn send_message_direct_dht_discovery() {
        let mut rt = Runtime::new().unwrap();

        let node_identity = NodeIdentity::random(
            &mut OsRng,
            "/ip4/127.0.0.1/tcp/9000".parse().unwrap(),
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
            Network::LocalTest,
        );
        let (reply_tx, reply_rx) = oneshot::channel();

        rt.block_on(
            service.call(DhtOutboundRequest::SendMessage(
                Box::new(
                    SendMessageParams::new()
                        .direct_public_key(peer_to_discover.public_key.clone())
                        .finish(),
                ),
                "custom_msg".as_bytes().to_vec(),
                reply_tx,
            )),
        )
        .unwrap();

        let send_message_response = rt.block_on(reply_rx).unwrap();

        unpack_enum!(SendMessageResponse::PendingDiscovery(await_discovery) = send_message_response);
        let discovery_reply = rt.block_on(await_discovery).unwrap();
        assert_eq!(dht_discovery_state.call_count(), 1);
        unpack_enum!(SendMessageResponse::Queued(tags) = discovery_reply);
        assert_eq!(tags.len(), 1);
        assert_eq!(spy.call_count(), 1);
    }
}
