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

use super::{broadcast_strategy::BroadcastStrategy, error::DhtOutboundError, message::DhtOutboundRequest};
use crate::{
    message::DhtHeader,
    outbound::message::{DhtOutboundMessage, ForwardRequest, SendMessageRequest},
};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::peer_manager::{NodeIdentity, PeerManager, PeerNodeIdentity};
use tari_comms_middleware::error::MiddlewareError;
use tower::{layer::Layer, Service, ServiceExt};

pub struct BroadcastLayer {
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
}

impl BroadcastLayer {
    pub fn new(node_identity: Arc<NodeIdentity>, peer_manager: Arc<PeerManager>) -> Self {
        BroadcastLayer {
            node_identity,
            peer_manager,
        }
    }
}

impl<S> Layer<S> for BroadcastLayer {
    type Service = BroadcastMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        BroadcastMiddleware::new(service, Arc::clone(&self.peer_manager), Arc::clone(&self.node_identity))
    }
}
const LOG_TARGET: &'static str = "comms::dht::outbound::broadcast_middleware";

/// Responsible for constructing messages using a broadcast strategy and passing them on to
/// the worker task.
#[derive(Clone)]
pub struct BroadcastMiddleware<S> {
    next: S,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
}

impl<S> BroadcastMiddleware<S> {
    pub fn new(service: S, peer_manager: Arc<PeerManager>, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            next: service,
            peer_manager,
            node_identity,
        }
    }
}

impl<S> Service<DhtOutboundRequest> for BroadcastMiddleware<S>
where
    S: Service<DhtOutboundMessage> + Clone,
    S::Error: Into<MiddlewareError>,
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
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
            msg,
        )
        .handle()
    }
}

struct BroadcastTask<S> {
    service: S,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    request: Option<DhtOutboundRequest>,
}

impl<S> BroadcastTask<S>
where
    S: Service<DhtOutboundMessage>,
    S::Error: Into<MiddlewareError>,
{
    pub fn new(
        service: S,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        request: DhtOutboundRequest,
    ) -> Self
    {
        Self {
            service,
            peer_manager,
            node_identity,
            request: Some(request),
        }
    }

    pub async fn handle(mut self) -> Result<(), MiddlewareError> {
        let request = self.request.take().expect("request cannot be None");
        // TODO: use blocking threadpool to generate messages
        debug!(target: LOG_TARGET, "Processing outbound request {}", request);
        let messages = self.generate_outbound_messages(request).map_err(Into::into)?;
        debug!(target: LOG_TARGET, "Sending {} message(s)", messages.len());

        for message in messages {
            self.service.ready().await.map_err(Into::into)?;
            self.service.call(message).await.map_err(Into::into)?;
        }

        Ok(())
    }

    pub fn generate_outbound_messages(
        &self,
        msg: DhtOutboundRequest,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        match msg {
            DhtOutboundRequest::SendMsg(request) => self.generate_send_messages(*request),
            DhtOutboundRequest::Forward(request) => self.generate_forward_messages(*request),
        }
    }

    fn get_broadcast_identities(
        &self,
        broadcast_strategy: &BroadcastStrategy,
    ) -> Result<Vec<PeerNodeIdentity>, DhtOutboundError>
    {
        match broadcast_strategy {
            BroadcastStrategy::DirectNodeId(node_id) => {
                // Send to a particular peer matching the given node ID
                self.peer_manager
                    .direct_identity_node_id(node_id)
                    .map(|peer| vec![peer])
                    .map_err(Into::into)
            },
            BroadcastStrategy::DirectPublicKey(public_key) => {
                // Send to a particular peer matching the given node ID
                self.peer_manager
                    .direct_identity_public_key(public_key)
                    .map(|peer| vec![peer])
                    .map_err(Into::into)
            },
            BroadcastStrategy::Flood => {
                // Send to all known Communication Node peers
                self.peer_manager.flood_identities().map_err(Into::into)
            },
            BroadcastStrategy::Closest(closest_request) => {
                // Send to all n nearest neighbour Communication Nodes
                self.peer_manager
                    .closest_identities(
                        &closest_request.node_id,
                        closest_request.n,
                        &closest_request.excluded_peers,
                    )
                    .map_err(Into::into)
            },
            BroadcastStrategy::Random(n) => {
                // Send to a random set of peers of size n that are Communication Nodes
                self.peer_manager.random_identities(*n).map_err(Into::into)
            },
        }
    }

    fn generate_send_messages(
        &self,
        send_message_request: SendMessageRequest,
    ) -> Result<Vec<DhtOutboundMessage>, DhtOutboundError>
    {
        let SendMessageRequest {
            broadcast_strategy,
            destination,
            comms_flags,
            dht_flags,
            dht_message_type,
            body,
        } = send_message_request;

        // Use the BroadcastStrategy to select appropriate peer(s) from PeerManager and then construct and send a
        // individually wrapped MessageEnvelope to each selected peer
        let selected_node_identities = self.get_broadcast_identities(&broadcast_strategy)?;

        // Create a DHT header
        let dht_header = DhtHeader::new(
            // Final destination for this message
            destination,
            // Origin public key used to identify the origin and verify the signature
            self.node_identity.identity.public_key.clone(),
            // Signing will happen later in the pipeline (SerializeMiddleware) to prevent double work
            Vec::default(),
            dht_message_type,
            dht_flags,
        );

        // Construct a MessageEnvelope for each recipient
        let messages = selected_node_identities
            .into_iter()
            .map(|peer_node_identity| {
                let dest_public_key = peer_node_identity.public_key.clone();
                DhtOutboundMessage::new(
                    peer_node_identity,
                    dht_header.clone(),
                    dest_public_key,
                    comms_flags,
                    body.clone(),
                )
            })
            .collect::<Vec<_>>();

        Ok(messages)
    }

    fn generate_forward_messages(
        &self,
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
        let selected_node_identities = self.get_broadcast_identities(&broadcast_strategy)?;

        let messages = selected_node_identities
            .into_iter()
            .map(|peer_node_identity| {
                DhtOutboundMessage::new(
                    peer_node_identity,
                    dht_header.clone(),
                    dht_header.origin_public_key.clone(),
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
        message::{DhtMessageFlags, DhtMessageType, NodeDestination},
        test_utils::{make_peer_manager, service_fn},
    };
    use futures::future;
    use rand::rngs::OsRng;
    use std::sync::Mutex;
    use tari_comms::{
        connection::NetAddress,
        message::MessageFlags,
        peer_manager::{NodeId, Peer, PeerFlags},
        types::CommsPublicKey,
    };
    use tari_crypto::keys::PublicKey;
    use tokio::runtime::Runtime;

    #[test]
    fn send_message_flood() {
        let rt = Runtime::new().unwrap();

        let peer_manager = make_peer_manager();
        let pk = CommsPublicKey::default();
        let example_peer = Peer::new(
            pk.clone(),
            NodeId::from_key(&pk).unwrap(),
            vec!["127.0.0.1:9999".parse::<NetAddress>().unwrap()].into(),
            PeerFlags::empty(),
        );
        peer_manager.add_peer(example_peer.clone()).unwrap();
        let other_peer = {
            let mut p = example_peer.clone();
            let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng::new().unwrap());
            p.node_id = NodeId::from_key(&pk).unwrap();
            p.public_key = pk;
            p
        };
        peer_manager.add_peer(other_peer.clone()).unwrap();
        let node_identity =
            NodeIdentity::random(&mut OsRng::new().unwrap(), "127.0.0.1:9000".parse().unwrap()).unwrap();

        let response = Arc::new(Mutex::new(Vec::new()));
        let next_service = service_fn(|out_msg: DhtOutboundMessage| {
            response.clone().lock().unwrap().push(out_msg);
            future::ready(Result::<_, MiddlewareError>::Ok(()))
        });

        let mut service = BroadcastMiddleware::new(next_service, peer_manager, Arc::new(node_identity));

        rt.block_on(service.call(DhtOutboundRequest::SendMsg(Box::new(SendMessageRequest {
            broadcast_strategy: BroadcastStrategy::Flood,
            comms_flags: MessageFlags::NONE,
            destination: NodeDestination::Undisclosed,
            dht_message_type: DhtMessageType::None,
            dht_flags: DhtMessageFlags::NONE,
            body: "custom_msg".as_bytes().to_vec(),
        }))))
        .unwrap();

        {
            let lock = response.lock().unwrap();
            assert_eq!(lock.len(), 2);
            assert!(lock
                .iter()
                .any(|msg| msg.peer_node_identity.node_id == example_peer.node_id));
            assert!(lock
                .iter()
                .any(|msg| msg.peer_node_identity.node_id == other_peer.node_id));
        }
    }
}
