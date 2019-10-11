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

use self::outbound::OutboundMessageRequester;
use crate::{
    envelope::{DhtMessageType, NodeDestination},
    inbound,
    inbound::{DecryptedDhtMessage, DiscoverMessage, JoinMessage},
    outbound,
    outbound::{BroadcastClosestRequest, BroadcastStrategy, DhtOutboundError, DhtOutboundRequest, OutboundEncryption},
    store_forward,
    DhtConfig,
};
use futures::{channel::mpsc, Future};
use log::debug;
use std::sync::Arc;
use tari_comms::{
    message::InboundMessage,
    outbound_message_service::OutboundMessage,
    peer_manager::{NodeId, NodeIdentity, PeerManager},
    types::CommsPublicKey,
};
use tari_comms_middleware::MiddlewareError;
use tower::{layer::Layer, Service, ServiceBuilder};

const LOG_TARGET: &'static str = "comms::dht";

pub struct Dht {
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    outbound_sender: mpsc::Sender<DhtOutboundRequest>,
    outbound_receiver: Option<mpsc::Receiver<DhtOutboundRequest>>,
}

impl Dht {
    pub fn new(config: DhtConfig, node_identity: Arc<NodeIdentity>, peer_manager: Arc<PeerManager>) -> Self {
        let (tx, rx) = mpsc::channel(config.outbound_buffer_size);
        Self {
            node_identity,
            peer_manager,
            config,
            outbound_sender: tx,
            outbound_receiver: Some(rx),
        }
    }

    /// Return a new OutboundMessageRequester connected to the receiver
    pub fn outbound_requester(&self) -> OutboundMessageRequester {
        OutboundMessageRequester::new(self.outbound_sender.clone())
    }

    /// Takes ownership of the receiver for DhtOutboundRequest. Will return None if ownership
    /// has already been taken.
    pub fn take_outbound_receiver(&mut self) -> Option<mpsc::Receiver<DhtOutboundRequest>> {
        self.outbound_receiver.take()
    }

    /// Returns an the full DHT stack as a `tower::layer::Layer`. This can be composed with
    /// other inbound middleware services which expect an DecryptedDhtMessage
    pub fn inbound_middleware_layer<S>(
        &self,
    ) -> impl Layer<
        S,
        Service = impl Service<
            InboundMessage,
            Response = (),
            Error = MiddlewareError,
            Future = impl Future<Output = Result<(), MiddlewareError>> + Send,
        > + Clone
                      + Send,
    >
    where
        S: Service<DecryptedDhtMessage, Response = (), Error = MiddlewareError> + Clone + Send + Sync + 'static,
        S::Future: Send,
    {
        let saf_storage = Arc::new(store_forward::SAFStorage::new(
            self.config.saf_msg_cache_storage_capacity,
        ));

        ServiceBuilder::new()
            .layer(inbound::DeserializeLayer::new())
            .layer(inbound::DecryptionLayer::new(Arc::clone(&self.node_identity)))
            .layer(store_forward::ForwardLayer::new(
                Arc::clone(&self.peer_manager),
                self.config.clone(),
                Arc::clone(&self.node_identity),
                self.outbound_requester(),
            ))
            .layer(store_forward::StoreLayer::new(
                self.config.clone(),
                Arc::clone(&self.peer_manager),
                Arc::clone(&self.node_identity),
                Arc::clone(&saf_storage),
            ))
            .layer(store_forward::MessageHandlerLayer::new(
                self.config.clone(),
                saf_storage,
                Arc::clone(&self.node_identity),
                Arc::clone(&self.peer_manager),
                self.outbound_requester(),
            ))
            .layer(inbound::DhtHandlerLayer::new(
                self.config.clone(),
                Arc::clone(&self.node_identity),
                Arc::clone(&self.peer_manager),
                self.outbound_requester(),
            ))
            .into_inner()
    }

    /// Returns an the full DHT stack as a `tower::layer::Layer`. This can be composed with
    /// other outbound middleware services which expect an OutboundMessage
    pub fn outbound_middleware_layer<S>(
        &self,
    ) -> impl Layer<
        S,
        Service = impl Service<
            DhtOutboundRequest,
            Response = (),
            Error = MiddlewareError,
            Future = impl Future<Output = Result<(), MiddlewareError>> + Send,
        > + Clone
                      + Send,
    >
    where
        S: Service<OutboundMessage, Response = (), Error = MiddlewareError> + Clone + Send + 'static,
        S::Future: Send,
    {
        ServiceBuilder::new()
            .layer(outbound::BroadcastLayer::new(
                Arc::clone(&self.node_identity),
                Arc::clone(&self.peer_manager),
            ))
            .layer(outbound::EncryptionLayer::new(Arc::clone(&self.node_identity)))
            .layer(outbound::SerializeLayer::new(Arc::clone(&self.node_identity)))
            .into_inner()
    }

    pub async fn send_join(&self) -> Result<(), DhtOutboundError> {
        let message = JoinMessage {
            node_id: self.node_identity.identity.node_id.clone(),
            net_addresses: vec![self.node_identity.control_service_address()],
            peer_features: self.node_identity.features().clone(),
        };

        debug!(
            target: LOG_TARGET,
            "Sending Join message to (at most) {} closest peers", self.config.num_regional_nodes
        );

        self.outbound_requester()
            .send_dht_message(
                BroadcastStrategy::Closest(BroadcastClosestRequest {
                    n: self.config.num_regional_nodes,
                    node_id: self.node_identity.identity.node_id.clone(),
                    excluded_peers: Vec::new(),
                }),
                NodeDestination::Undisclosed,
                OutboundEncryption::None,
                DhtMessageType::Join,
                message,
            )
            .await?;

        Ok(())
    }

    pub async fn send_discover(
        &self,
        dest_public_key: CommsPublicKey,
        dest_node_id: Option<NodeId>,
        destination: NodeDestination,
    ) -> Result<(), DhtOutboundError>
    {
        let discover_msg = DiscoverMessage {
            node_id: self.node_identity.identity.node_id.clone(),
            net_addresses: vec![self.node_identity.control_service_address()],
            peer_features: self.node_identity.features().clone(),
        };
        debug!(
            target: LOG_TARGET,
            "Sending Discover message to (at most) {} closest peers", self.config.num_regional_nodes
        );

        // If the destination node is is known, send to the closest peers we know. Otherwise...
        let network_location_node_id = dest_node_id.unwrap_or(match &destination {
            // ... if the destination is undisclosed or a public key, send discover to our closest peers
            NodeDestination::Undisclosed | NodeDestination::PublicKey(_) => self.node_identity.node_id().clone(),
            // otherwise, send it to the closest peers to the given NodeId destination we know
            NodeDestination::NodeId(node_id) => node_id.clone(),
        });

        let broadcast_strategy = BroadcastStrategy::Closest(BroadcastClosestRequest {
            n: self.config.num_regional_nodes,
            node_id: network_location_node_id,
            excluded_peers: Vec::new(),
        });

        self.outbound_requester()
            .send_dht_message(
                broadcast_strategy,
                destination,
                OutboundEncryption::EncryptFor(dest_public_key),
                DhtMessageType::Discover,
                discover_msg,
            )
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        envelope::DhtMessageFlags,
        outbound::DhtOutboundRequest,
        test_utils::{make_comms_inbound_message, make_dht_envelope, make_node_identity, make_peer_manager},
        DhtBuilder,
    };
    use futures::{channel::mpsc, StreamExt};
    use std::sync::Arc;
    use tari_comms::{
        message::{Message, MessageFlags, MessageHeader},
        utils::crypt::{encrypt, generate_ecdh_secret},
    };
    use tari_comms_middleware::sink::SinkMiddleware;
    use tari_utilities::message_format::MessageFormat;
    use tokio::runtime::Runtime;
    use tower::{layer::Layer, Service};

    #[test]
    fn stack_unencrypted() {
        let node_identity = make_node_identity();
        let peer_manager = make_peer_manager();

        let dht = DhtBuilder::new(Arc::clone(&node_identity), peer_manager).finish();

        let rt = Runtime::new().unwrap();

        let (out_tx, mut out_rx) = mpsc::channel(10);

        let mut service = dht.inbound_middleware_layer().layer(SinkMiddleware::new(out_tx));

        let header = MessageHeader::new("fake_type".to_string()).unwrap();
        let msg = Message::from_message_format(header, "secret".to_string()).unwrap();
        let dht_envelope = make_dht_envelope(&node_identity, msg.to_binary().unwrap(), DhtMessageFlags::empty());
        let inbound_message =
            make_comms_inbound_message(&node_identity, dht_envelope.to_binary().unwrap(), MessageFlags::empty());

        let msg = rt.block_on(async move {
            service.call(inbound_message).await.unwrap();
            let msg = out_rx.next().await.unwrap();
            msg.success().unwrap().deserialize_message::<String>().unwrap()
        });

        assert_eq!(msg, "secret");
    }

    #[test]
    fn stack_encrypted() {
        let node_identity = make_node_identity();
        let peer_manager = make_peer_manager();

        let dht = DhtBuilder::new(Arc::clone(&node_identity), peer_manager).finish();

        let rt = Runtime::new().unwrap();

        let (out_tx, mut out_rx) = mpsc::channel(10);

        let mut service = dht.inbound_middleware_layer().layer(SinkMiddleware::new(out_tx));

        let header = MessageHeader::new("fake_type".to_string()).unwrap();
        let msg = Message::from_message_format(header, "secret".to_string()).unwrap();
        // Encrypt for self
        let ecdh_key = generate_ecdh_secret(&node_identity.secret_key, &node_identity.identity.public_key);
        let encrypted_bytes = encrypt(&ecdh_key, &msg.to_binary().unwrap()).unwrap();
        let dht_envelope = make_dht_envelope(&node_identity, encrypted_bytes, DhtMessageFlags::ENCRYPTED);
        let inbound_message =
            make_comms_inbound_message(&node_identity, dht_envelope.to_binary().unwrap(), MessageFlags::empty());

        let msg = rt.block_on(async move {
            service.call(inbound_message).await.unwrap();
            let msg = out_rx.next().await.unwrap();
            msg.success().unwrap().deserialize_message::<String>().unwrap()
        });

        assert_eq!(msg, "secret");
    }

    #[test]
    fn stack_forward() {
        let node_identity = make_node_identity();
        let peer_manager = make_peer_manager();

        let mut dht = DhtBuilder::new(Arc::clone(&node_identity), peer_manager).finish();

        let rt = Runtime::new().unwrap();

        let (next_service_tx, mut next_service_rx) = mpsc::channel(10);

        let mut service = dht
            .inbound_middleware_layer()
            .layer(SinkMiddleware::new(next_service_tx));

        let header = MessageHeader::new("fake_type".to_string()).unwrap();
        let msg = Message::from_message_format(header, "unencrypteable".to_string()).unwrap();

        // Encrypt for someone else
        let node_identity2 = make_node_identity();
        let ecdh_key = generate_ecdh_secret(&node_identity2.secret_key, &node_identity2.identity.public_key);
        let encrypted_bytes = encrypt(&ecdh_key, &msg.to_binary().unwrap()).unwrap();
        let dht_envelope = make_dht_envelope(&node_identity, encrypted_bytes, DhtMessageFlags::ENCRYPTED);
        let inbound_message =
            make_comms_inbound_message(&node_identity, dht_envelope.to_binary().unwrap(), MessageFlags::empty());

        let mut oms_receiver = dht.take_outbound_receiver().unwrap();

        let msg = rt.block_on(async move {
            service.call(inbound_message).await.unwrap();
            oms_receiver.next().await.unwrap()
        });

        // Check that OMS got a request to forward
        match msg {
            DhtOutboundRequest::Forward { .. } => {},
            _ => panic!("unexpected message"),
        }
        // Check the next service was not called
        assert!(rt.block_on(next_service_rx.next()).is_none());
    }
}
