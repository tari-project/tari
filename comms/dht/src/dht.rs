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
    connectivity::DhtConnectivity,
    discovery::{DhtDiscoveryRequest, DhtDiscoveryRequester, DhtDiscoveryService},
    event::{DhtEventReceiver, DhtEventSender},
    inbound,
    inbound::{DecryptedDhtMessage, DhtInboundMessage},
    logging_middleware::MessageLoggingLayer,
    network_discovery::DhtNetworkDiscovery,
    outbound,
    outbound::DhtOutboundRequest,
    proto::envelope::DhtMessageType,
    rpc,
    storage::{DbConnection, StorageError},
    store_forward,
    store_forward::{StoreAndForwardError, StoreAndForwardRequest, StoreAndForwardRequester, StoreAndForwardService},
    tower_filter,
    DedupLayer,
    DhtActorError,
    DhtConfig,
};
use futures::{channel::mpsc, future, Future};
use log::*;
use std::sync::Arc;
use tari_comms::{
    connectivity::ConnectivityRequester,
    message::{InboundMessage, OutboundMessage},
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    pipeline::PipelineError,
};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::sync::broadcast;
use tower::{layer::Layer, Service, ServiceBuilder};

const LOG_TARGET: &str = "comms::dht";

const DHT_ACTOR_CHANNEL_SIZE: usize = 100;
const DHT_DISCOVERY_CHANNEL_SIZE: usize = 100;
const DHT_SAF_SERVICE_CHANNEL_SIZE: usize = 100;
const DHT_EVENT_BROADCAST_CHANNEL_SIZE: usize = 100;

#[derive(Debug, Error)]
pub enum DhtInitializationError {
    #[error("Database initialization failed: {0}")]
    DatabaseMigrationFailed(#[from] StorageError),
    #[error("StoreAndForwardInitializationError: {0}")]
    StoreAndForwardInitializationError(#[from] StoreAndForwardError),
    #[error("DhtActorInitializationError: {0}")]
    DhtActorInitializationError(#[from] DhtActorError),
}

/// Responsible for starting the DHT actor, building the DHT middleware stack and as a factory
/// for producing DHT requesters.
#[derive(Clone)]
pub struct Dht {
    /// Node identity of this node
    node_identity: Arc<NodeIdentity>,
    /// Comms peer manager
    peer_manager: Arc<PeerManager>,
    /// Dht configuration
    config: DhtConfig,
    /// Used to create a OutboundMessageRequester.
    outbound_tx: mpsc::Sender<DhtOutboundRequest>,
    /// Sender for DHT requests
    dht_sender: mpsc::Sender<DhtRequest>,
    /// Sender for SAF requests
    saf_sender: mpsc::Sender<StoreAndForwardRequest>,
    /// Sender for SAF repsonse signals
    saf_response_signal_sender: mpsc::Sender<()>,
    /// Sender for DHT discovery requests
    discovery_sender: mpsc::Sender<DhtDiscoveryRequest>,
    /// Connectivity actor requester
    connectivity: ConnectivityRequester,
    /// Event stream sender
    event_publisher: DhtEventSender,
}

impl Dht {
    pub async fn initialize(
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_tx: mpsc::Sender<DhtOutboundRequest>,
        connectivity: ConnectivityRequester,
        shutdown_signal: ShutdownSignal,
    ) -> Result<Self, DhtInitializationError>
    {
        let (dht_sender, dht_receiver) = mpsc::channel(DHT_ACTOR_CHANNEL_SIZE);
        let (discovery_sender, discovery_receiver) = mpsc::channel(DHT_DISCOVERY_CHANNEL_SIZE);
        let (saf_sender, saf_receiver) = mpsc::channel(DHT_SAF_SERVICE_CHANNEL_SIZE);
        let (saf_response_signal_sender, saf_response_signal_receiver) = mpsc::channel(DHT_SAF_SERVICE_CHANNEL_SIZE);
        let (event_publisher, _) = broadcast::channel(DHT_EVENT_BROADCAST_CHANNEL_SIZE);

        let dht = Self {
            node_identity,
            peer_manager,
            config,
            outbound_tx,
            dht_sender,
            saf_sender,
            saf_response_signal_sender,
            connectivity,
            discovery_sender,
            event_publisher: event_publisher.clone(),
        };

        let conn = DbConnection::connect_and_migrate(dht.config.database_url.clone())
            .await
            .map_err(DhtInitializationError::DatabaseMigrationFailed)?;

        dht.network_discovery_service(shutdown_signal.clone()).spawn();
        dht.connectivity_service(shutdown_signal.clone()).spawn();
        dht.store_and_forward_service(
            conn.clone(),
            saf_receiver,
            shutdown_signal.clone(),
            saf_response_signal_receiver,
        )
        .spawn();
        dht.actor(conn, dht_receiver, shutdown_signal.clone()).spawn();
        dht.discovery_service(discovery_receiver, shutdown_signal).spawn();

        debug!(target: LOG_TARGET, "Dht initialization complete.");

        Ok(dht)
    }

    /// Create a DHT RPC service
    pub fn rpc_service(&self) -> rpc::DhtService<rpc::DhtRpcServiceImpl> {
        rpc::DhtService::new(rpc::DhtRpcServiceImpl::new(self.peer_manager.clone()))
    }

    /// Create a DHT actor
    fn actor(
        &self,
        conn: DbConnection,
        request_receiver: mpsc::Receiver<DhtRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> DhtActor
    {
        DhtActor::new(
            self.config.clone(),
            conn,
            Arc::clone(&self.node_identity),
            Arc::clone(&self.peer_manager),
            self.connectivity.clone(),
            self.outbound_requester(),
            request_receiver,
            shutdown_signal,
        )
    }

    /// Create the discovery service
    fn discovery_service(
        &self,
        request_receiver: mpsc::Receiver<DhtDiscoveryRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> DhtDiscoveryService
    {
        DhtDiscoveryService::new(
            self.config.clone(),
            Arc::clone(&self.node_identity),
            Arc::clone(&self.peer_manager),
            self.outbound_requester(),
            request_receiver,
            shutdown_signal,
        )
    }

    fn connectivity_service(&self, shutdown_signal: ShutdownSignal) -> DhtConnectivity {
        DhtConnectivity::new(
            self.config.clone(),
            self.peer_manager.clone(),
            self.node_identity.clone(),
            self.connectivity.clone(),
            self.dht_requester(),
            self.event_publisher.subscribe(),
            shutdown_signal,
        )
    }

    /// Create the network discovery service
    fn network_discovery_service(&self, shutdown_signal: ShutdownSignal) -> DhtNetworkDiscovery {
        DhtNetworkDiscovery::new(
            self.config.clone(),
            Arc::clone(&self.node_identity),
            Arc::clone(&self.peer_manager),
            self.connectivity.clone(),
            self.event_publisher.clone(),
            shutdown_signal,
        )
    }

    fn store_and_forward_service(
        &self,
        conn: DbConnection,
        request_rx: mpsc::Receiver<StoreAndForwardRequest>,
        shutdown_signal: ShutdownSignal,
        saf_response_signal_rx: mpsc::Receiver<()>,
    ) -> StoreAndForwardService
    {
        StoreAndForwardService::new(
            self.config.clone(),
            conn,
            self.peer_manager.clone(),
            self.dht_requester(),
            self.connectivity.clone(),
            self.outbound_requester(),
            request_rx,
            saf_response_signal_rx,
            self.event_publisher.clone(),
            shutdown_signal,
        )
    }

    /// Return a new OutboundMessageRequester connected to the receiver
    pub fn outbound_requester(&self) -> OutboundMessageRequester {
        OutboundMessageRequester::new(self.outbound_tx.clone())
    }

    /// Returns a requester for the DhtActor associated with this instance
    pub fn dht_requester(&self) -> DhtRequester {
        DhtRequester::new(self.dht_sender.clone())
    }

    /// Returns a requester for the DhtDiscoveryService associated with this instance
    pub fn discovery_service_requester(&self) -> DhtDiscoveryRequester {
        DhtDiscoveryRequester::new(self.discovery_sender.clone(), self.config.discovery_request_timeout)
    }

    /// Returns a requester for the StoreAndForwardService associated with this instance
    pub fn store_and_forward_requester(&self) -> StoreAndForwardRequester {
        StoreAndForwardRequester::new(self.saf_sender.clone())
    }

    /// Get a subscription to `DhtEvents`
    pub fn subscribe_dht_events(&self) -> DhtEventReceiver {
        self.event_publisher.subscribe()
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
            Error = PipelineError,
            Future = impl Future<Output = Result<(), PipelineError>> + Send,
        > + Clone
                      + Send,
    >
    where
        S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Send + Sync + 'static,
        S::Future: Send,
    {
        // FIXME: There is an unresolved stack overflow issue on windows in debug mode during runtime, but not in
        //        release mode, related to the amount of layers. (issue #1416)
        ServiceBuilder::new()
            .layer(inbound::DeserializeLayer::new(self.peer_manager.clone()))
            .layer(inbound::ValidateLayer::new(self.config.network))
            .layer(DedupLayer::new(self.dht_requester()))
            .layer(tower_filter::FilterLayer::new(self.unsupported_saf_messages_filter()))
            .layer(MessageLoggingLayer::new(format!(
                "Inbound [{}]",
                self.node_identity.node_id().short_str()
            )))
            .layer(inbound::DecryptionLayer::new(Arc::clone(&self.node_identity)))
            .layer(store_forward::StoreLayer::new(
                self.config.clone(),
                Arc::clone(&self.peer_manager),
                Arc::clone(&self.node_identity),
                self.store_and_forward_requester(),
            ))
            .layer(store_forward::ForwardLayer::new(
                self.outbound_requester(),
                self.node_identity.features().contains(PeerFeatures::DHT_STORE_FORWARD),
            ))
            .layer(store_forward::MessageHandlerLayer::new(
                self.config.clone(),
                self.store_and_forward_requester(),
                self.dht_requester(),
                Arc::clone(&self.node_identity),
                Arc::clone(&self.peer_manager),
                self.outbound_requester(),
                self.saf_response_signal_sender.clone(),
            ))
            .layer(inbound::DhtHandlerLayer::new(
                Arc::clone(&self.node_identity),
                Arc::clone(&self.peer_manager),
                self.discovery_service_requester(),
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
            Error = PipelineError,
            Future = impl Future<Output = Result<(), PipelineError>> + Send,
        > + Clone
                      + Send,
    >
    where
        S: Service<OutboundMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
        S::Future: Send,
    {
        ServiceBuilder::new()
            .layer(outbound::BroadcastLayer::new(
                Arc::clone(&self.node_identity),
                self.dht_requester(),
                self.discovery_service_requester(),
                self.config.network,
            ))
            .layer(MessageLoggingLayer::new(format!(
                "Outbound [{}]",
                self.node_identity.node_id().short_str()
            )))
            .layer(outbound::SerializeLayer)
            .into_inner()
    }

    /// Produces a filter predicate which disallows store and forward messages if that feature is not
    /// supported by the node.
    fn unsupported_saf_messages_filter(
        &self,
    ) -> impl tower_filter::Predicate<DhtInboundMessage, Future = future::Ready<Result<(), PipelineError>>> + Clone + Send
    {
        let node_identity = Arc::clone(&self.node_identity);
        move |msg: &DhtInboundMessage| {
            if node_identity.has_peer_features(PeerFeatures::DHT_STORE_FORWARD) {
                return future::ready(Ok(()));
            }

            match msg.dht_header.message_type {
                DhtMessageType::SafRequestMessages => {
                    // TODO: #banheuristic This is an indication of node misbehaviour
                    debug!(
                        "Received store and forward message from PublicKey={}. Store and forward feature is not \
                         supported by this node. Discarding message.",
                        msg.source_peer.public_key
                    );
                    future::ready(Err(anyhow::anyhow!(
                        "Message filtered out because store and forward is not supported by this node",
                    )))
                },
                _ => future::ready(Ok(())),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        crypt,
        envelope::DhtMessageFlags,
        outbound::mock::create_outbound_service_mock,
        proto::envelope::DhtMessageType,
        test_utils::{
            build_peer_manager,
            make_client_identity,
            make_comms_inbound_message,
            make_dht_envelope,
            make_node_identity,
        },
        DhtBuilder,
    };
    use futures::{channel::mpsc, StreamExt};
    use std::{sync::Arc, time::Duration};
    use tari_comms::{
        message::{MessageExt, MessageTag},
        pipeline::SinkService,
        test_utils::mocks::create_connectivity_mock,
        wrap_in_envelope_body,
    };
    use tari_shutdown::Shutdown;
    use tokio::{task, time};
    use tower::{layer::Layer, Service};

    #[tokio_macros::test_basic]
    async fn stack_unencrypted() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _) = mpsc::channel(10);

        let shutdown = Shutdown::new();
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            out_tx,
            connectivity,
            shutdown.to_signal(),
        )
        .local_test()
        .build()
        .await
        .unwrap();

        let (out_tx, mut out_rx) = mpsc::channel(10);

        let mut service = dht.inbound_middleware_layer().layer(SinkService::new(out_tx));

        let msg = wrap_in_envelope_body!(b"secret".to_vec());
        let dht_envelope = make_dht_envelope(
            &node_identity,
            msg.to_encoded_bytes(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
        );
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        let msg = {
            service.call(inbound_message).await.unwrap();
            let msg = time::timeout(Duration::from_secs(10), out_rx.next())
                .await
                .unwrap()
                .unwrap();
            msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap()
        };

        assert_eq!(msg, b"secret");
    }

    #[tokio_macros::test_basic]
    async fn stack_encrypted() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _out_rx) = mpsc::channel(10);

        let shutdown = Shutdown::new();
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            out_tx,
            connectivity,
            shutdown.to_signal(),
        )
        .build()
        .await
        .unwrap();

        let (out_tx, mut out_rx) = mpsc::channel(10);

        let mut service = dht.inbound_middleware_layer().layer(SinkService::new(out_tx));

        let msg = wrap_in_envelope_body!(b"secret".to_vec());
        // Encrypt for self
        let dht_envelope = make_dht_envelope(
            &node_identity,
            msg.to_encoded_bytes(),
            DhtMessageFlags::ENCRYPTED,
            true,
            MessageTag::new(),
        );
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        let msg = {
            service.call(inbound_message).await.unwrap();
            let msg = time::timeout(Duration::from_secs(10), out_rx.next())
                .await
                .unwrap()
                .unwrap();
            msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap()
        };

        assert_eq!(msg, b"secret");
    }

    #[tokio_macros::test_basic]
    async fn stack_forward() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let shutdown = Shutdown::new();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        let (connectivity, _) = create_connectivity_mock();
        let (next_service_tx, mut next_service_rx) = mpsc::channel(10);
        let (oms_requester, oms_mock) = create_outbound_service_mock(1);

        // Send all outbound requests to the mock
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            oms_requester.get_mpsc_sender(),
            connectivity,
            shutdown.to_signal(),
        )
        .build()
        .await
        .unwrap();
        let oms_mock_state = oms_mock.get_state();
        task::spawn(oms_mock.run());

        let mut service = dht.inbound_middleware_layer().layer(SinkService::new(next_service_tx));

        let msg = wrap_in_envelope_body!(b"unencrypteable".to_vec());

        // Encrypt for someone else
        let node_identity2 = make_node_identity();
        let ecdh_key = crypt::generate_ecdh_secret(node_identity2.secret_key(), node_identity2.public_key());
        let encrypted_bytes = crypt::encrypt(&ecdh_key, &msg.to_encoded_bytes()).unwrap();
        let dht_envelope = make_dht_envelope(
            &node_identity,
            encrypted_bytes,
            DhtMessageFlags::ENCRYPTED,
            true,
            MessageTag::new(),
        );

        let origin_mac = dht_envelope.header.as_ref().unwrap().origin_mac.clone();
        assert_eq!(origin_mac.is_empty(), false);
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        service.call(inbound_message).await.unwrap();

        assert_eq!(oms_mock_state.call_count(), 1);
        let (params, _) = oms_mock_state.pop_call().unwrap();

        // Check that OMS got a request to forward with the original Dht Header
        assert_eq!(params.dht_header.unwrap().origin_mac, origin_mac);

        // Check the next service was not called
        assert!(next_service_rx.try_next().is_err());
    }

    #[tokio_macros::test_basic]
    async fn stack_filter_saf_message() {
        let node_identity = make_client_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _) = mpsc::channel(10);

        let shutdown = Shutdown::new();
        let dht = DhtBuilder::new(
            Arc::clone(&node_identity),
            peer_manager,
            out_tx,
            connectivity,
            shutdown.to_signal(),
        )
        .build()
        .await
        .unwrap();

        let (next_service_tx, mut next_service_rx) = mpsc::channel(10);

        let mut service = dht.inbound_middleware_layer().layer(SinkService::new(next_service_tx));

        let msg = wrap_in_envelope_body!(b"secret".to_vec());
        let mut dht_envelope = make_dht_envelope(
            &node_identity,
            msg.to_encoded_bytes(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
        );
        dht_envelope.header.as_mut().and_then(|header| {
            header.message_type = DhtMessageType::SafStoredMessages as i32;
            Some(header)
        });
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        service.call(inbound_message).await.unwrap_err();
        // This seems like the best way to tell that an open channel is empty without the test blocking indefinitely
        assert_eq!(
            format!("{}", next_service_rx.try_next().unwrap_err()),
            "receiver channel is empty"
        );
    }
}
