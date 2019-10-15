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
    actor::{DhtActor, DhtRequest, DhtRequester},
    envelope::DhtMessageType,
    inbound,
    inbound::{DecryptedDhtMessage, DhtInboundMessage},
    outbound,
    outbound::DhtOutboundRequest,
    store_forward,
    DhtConfig,
};
use futures::{channel::mpsc, future, Future};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::InboundMessage,
    outbound_message_service::OutboundMessage,
    peer_manager::{NodeIdentity, PeerFeature, PeerManager},
};
use tari_comms_middleware::MiddlewareError;
use tari_shutdown::ShutdownSignal;
use tokio::runtime::TaskExecutor;
use tower::{layer::Layer, Service, ServiceBuilder};
use tower_filter::error::Error as FilterError;

/// Responsible for starting the DHT actor, building the DHT middleware stack and as a factory
/// for producing DHT requesters.
pub struct Dht {
    /// Node identity of this node
    node_identity: Arc<NodeIdentity>,
    /// Comms peer manager
    peer_manager: Arc<PeerManager>,
    /// Dht configuration
    config: DhtConfig,
    /// Used to create a OutboundMessageRequester.
    outbound_sender: mpsc::Sender<DhtOutboundRequest>,
    /// Receiver for DHT outbound requests.
    outbound_receiver: Option<mpsc::Receiver<DhtOutboundRequest>>,
    /// Sender for DHT requests
    dht_sender: mpsc::Sender<DhtRequest>,
}

impl Dht {
    pub fn new(
        config: DhtConfig,
        executor: TaskExecutor,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (outbound_sender, outbound_receiver) = mpsc::channel(config.outbound_buffer_size);
        let (dht_sender, dht_receiver) = mpsc::channel(10);

        let dht = Self {
            node_identity,
            peer_manager,
            config,
            outbound_sender,
            outbound_receiver: Some(outbound_receiver),
            dht_sender,
        };

        executor.spawn(dht.actor(dht_receiver, shutdown_signal).start());

        dht
    }

    /// Create an actor
    fn actor(&self, request_receiver: mpsc::Receiver<DhtRequest>, shutdown_signal: ShutdownSignal) -> DhtActor {
        DhtActor::new(
            self.config.clone(),
            Arc::clone(&self.node_identity),
            self.outbound_requester(),
            request_receiver,
            shutdown_signal,
        )
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
            .layer(tower_filter::FilterLayer::new(self.unsupported_saf_messages_filter()))
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

    /// Produces a filter predicate which disallows store and forward messages if that feature is not
    /// supported by the node.
    fn unsupported_saf_messages_filter(
        &self,
    ) -> impl tower_filter::Predicate<DhtInboundMessage, Future = future::Ready<Result<(), FilterError>>> + Clone + Send
    {
        let node_identity = Arc::clone(&self.node_identity);
        move |msg: &DhtInboundMessage| {
            if node_identity.has_peer_feature(&PeerFeature::DhtStoreForward) {
                return future::ready(Ok(()));
            }

            match msg.dht_header.message_type {
                DhtMessageType::SAFRequestMessages | DhtMessageType::SAFStoredMessages => {
                    // TODO: This is an indication of node misbehaviour
                    warn!(
                        "Received store and forward message from PublicKey={}. Store and forward feature is not \
                         supported by this node. Discarding message.",
                        msg.dht_header.origin_public_key
                    );
                    future::ready(Err(FilterError::rejected()))
                },
                _ => future::ready(Ok(())),
            }
        }
    }

    pub fn dht_requester(&self) -> DhtRequester {
        DhtRequester::new(self.dht_sender.clone())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        envelope::{DhtMessageFlags, DhtMessageType},
        outbound::DhtOutboundRequest,
        test_utils::{
            make_client_identity,
            make_comms_inbound_message,
            make_dht_envelope,
            make_node_identity,
            make_peer_manager,
        },
        DhtBuilder,
        DhtConfig,
    };
    use futures::{channel::mpsc, StreamExt};
    use std::sync::Arc;
    use tari_comms::{
        message::{Message, MessageFlags, MessageHeader},
        utils::crypt::{encrypt, generate_ecdh_secret},
    };
    use tari_comms_middleware::sink::SinkMiddleware;
    use tari_shutdown::Shutdown;
    use tari_utilities::message_format::MessageFormat;
    use tokio::runtime::Runtime;
    use tower::{layer::Layer, Service};

    #[test]
    fn stack_unencrypted() {
        let node_identity = make_node_identity();
        let peer_manager = make_peer_manager();
        let rt = Runtime::new().unwrap();

        let shutdown = Shutdown::new();
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            rt.executor(),
            shutdown.to_signal(),
        )
        .finish();

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

        let rt = Runtime::new().unwrap();
        let shutdown = Shutdown::new();
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            rt.executor(),
            shutdown.to_signal(),
        )
        .finish();

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

        let rt = Runtime::new().unwrap();
        let shutdown = Shutdown::new();
        let mut dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            rt.executor(),
            shutdown.to_signal(),
        )
        .with_config(DhtConfig {
            // Do not want to have the auto join interfering by sending on the outbound requester
            enable_auto_join: false,
            enable_auto_stored_message_request: false,
            ..Default::default()
        })
        .finish();

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

    #[test]
    fn stack_filter_saf_message() {
        let node_identity = make_client_identity();
        let peer_manager = make_peer_manager();

        let rt = Runtime::new().unwrap();
        let shutdown = Shutdown::new();
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            rt.executor(),
            shutdown.to_signal(),
        )
        .finish();

        let (next_service_tx, mut next_service_rx) = mpsc::channel(10);

        let mut service = dht
            .inbound_middleware_layer()
            .layer(SinkMiddleware::new(next_service_tx));

        let msg = Message::from_message_format((), "secret".to_string()).unwrap();
        let mut dht_envelope = make_dht_envelope(&node_identity, msg.to_binary().unwrap(), DhtMessageFlags::empty());
        dht_envelope.header.message_type = DhtMessageType::SAFStoredMessages;
        let inbound_message =
            make_comms_inbound_message(&node_identity, dht_envelope.to_binary().unwrap(), MessageFlags::empty());

        let err = rt.block_on(service.call(inbound_message));
        assert!(err.is_err());
        // This seems like the best way to tell that an open channel is empty without the test blocking indefinitely
        assert_eq!(
            format!("{}", next_service_rx.try_next().unwrap_err()),
            "receiver channel is empty"
        );
    }
}
