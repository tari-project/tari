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

use std::sync::Arc;

use futures::Future;
use log::*;
use tari_comms::{
    connectivity::ConnectivityRequester,
    message::{InboundMessage, OutboundMessage},
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    pipeline::PipelineError,
};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tower::{layer::Layer, Service, ServiceBuilder};

use self::outbound::OutboundMessageRequester;
use crate::{
    actor::{DhtActor, DhtRequest, DhtRequester},
    connectivity::{DhtConnectivity, MetricsCollector, MetricsCollectorHandle},
    discovery::{DhtDiscoveryRequest, DhtDiscoveryRequester, DhtDiscoveryService},
    event::{DhtEventReceiver, DhtEventSender},
    filter,
    inbound,
    inbound::{DecryptedDhtMessage, DhtInboundMessage, MetricsLayer},
    logging_middleware::MessageLoggingLayer,
    network_discovery::DhtNetworkDiscovery,
    outbound,
    outbound::DhtOutboundRequest,
    proto::envelope::DhtMessageType,
    rpc,
    storage::{DbConnection, StorageError},
    store_forward,
    store_forward::{StoreAndForwardError, StoreAndForwardRequest, StoreAndForwardRequester, StoreAndForwardService},
    DedupLayer,
    DhtActorError,
    DhtBuilder,
    DhtConfig,
};

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
    #[error("Builder error: no outbound message sender set")]
    BuilderNoOutboundMessageSender,
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
    config: Arc<DhtConfig>,
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
    /// Used by MetricsLayer to collect metrics and to inform heuristics for peer banning
    metrics_collector: MetricsCollectorHandle,
}

impl Dht {
    pub(crate) async fn initialize(
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_tx: mpsc::Sender<DhtOutboundRequest>,
        connectivity: ConnectivityRequester,
        shutdown_signal: ShutdownSignal,
    ) -> Result<Self, DhtInitializationError> {
        let (dht_sender, dht_receiver) = mpsc::channel(DHT_ACTOR_CHANNEL_SIZE);
        let (discovery_sender, discovery_receiver) = mpsc::channel(DHT_DISCOVERY_CHANNEL_SIZE);
        let (saf_sender, saf_receiver) = mpsc::channel(DHT_SAF_SERVICE_CHANNEL_SIZE);
        let (saf_response_signal_sender, saf_response_signal_receiver) = mpsc::channel(DHT_SAF_SERVICE_CHANNEL_SIZE);
        let (event_publisher, _) = broadcast::channel(DHT_EVENT_BROADCAST_CHANNEL_SIZE);

        let metrics_collector = MetricsCollector::spawn();

        let dht = Self {
            node_identity,
            peer_manager,
            metrics_collector,
            config: Arc::new(config),
            outbound_tx,
            dht_sender,
            saf_sender,
            saf_response_signal_sender,
            connectivity,
            discovery_sender,
            event_publisher,
        };

        let conn = DbConnection::connect_and_migrate(dht.config.database_url.clone())
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

    pub fn builder() -> DhtBuilder {
        DhtBuilder::new()
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
    ) -> DhtActor {
        DhtActor::new(
            self.config.clone(),
            conn,
            Arc::clone(&self.node_identity),
            Arc::clone(&self.peer_manager),
            self.connectivity.clone(),
            self.outbound_requester(),
            request_receiver,
            self.discovery_service_requester(),
            shutdown_signal,
        )
    }

    /// Create the discovery service
    fn discovery_service(
        &self,
        request_receiver: mpsc::Receiver<DhtDiscoveryRequest>,
        shutdown_signal: ShutdownSignal,
    ) -> DhtDiscoveryService {
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
            self.metrics_collector.clone(),
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
    ) -> StoreAndForwardService {
        StoreAndForwardService::new(
            self.config.saf_config.clone(),
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

    pub fn metrics_collector(&self) -> MetricsCollectorHandle {
        self.metrics_collector.clone()
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
            Future = impl Future<Output = Result<(), PipelineError>>,
        > + Clone,
    >
    where
        S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Send + Sync + 'static,
        S::Future: Send,
    {
        ServiceBuilder::new()
            .layer(MetricsLayer::new(self.metrics_collector.clone()))
            .layer(inbound::DeserializeLayer::new(self.peer_manager.clone()))
            .layer(filter::FilterLayer::new(self.unsupported_saf_messages_filter()))
            .layer(inbound::DecryptionLayer::new(
                self.config.clone(),
                self.node_identity.clone(),
                self.connectivity.clone(),
            ))
            .layer(DedupLayer::new(
                self.dht_requester(),
                self.config.dedup_allowed_message_occurrences,
            ))
            .layer(filter::FilterLayer::new(filter_messages_to_rebroadcast))
            .layer(MessageLoggingLayer::new(format!(
                "Inbound [{}]",
                self.node_identity.node_id().short_str()
            )))
            .layer(store_forward::StoreLayer::new(
                self.config.saf_config.clone(),
                Arc::clone(&self.peer_manager),
                Arc::clone(&self.node_identity),
                self.store_and_forward_requester(),
            ))
            .layer(store_forward::ForwardLayer::new(
                self.outbound_requester(),
                self.node_identity.features().contains(PeerFeatures::DHT_STORE_FORWARD),
            ))
            .layer(store_forward::MessageHandlerLayer::new(
                self.config.saf_config.clone(),
                self.store_and_forward_requester(),
                self.dht_requester(),
                Arc::clone(&self.node_identity),
                Arc::clone(&self.peer_manager),
                self.outbound_requester(),
                self.saf_response_signal_sender.clone(),
            ))
            .layer(inbound::DhtHandlerLayer::new(
                self.config.clone(),
                self.node_identity.clone(),
                self.peer_manager.clone(),
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
            Future = impl Future<Output = Result<(), PipelineError>>,
        > + Clone,
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
                &self.config,
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
    fn unsupported_saf_messages_filter(&self) -> impl filter::Predicate<DhtInboundMessage> + Clone + Send {
        let node_identity = Arc::clone(&self.node_identity);
        move |msg: &DhtInboundMessage| {
            if node_identity.has_peer_features(PeerFeatures::DHT_STORE_FORWARD) {
                return true;
            }

            match msg.dht_header.message_type {
                DhtMessageType::SafRequestMessages => {
                    // TODO: #banheuristic This is an indication of node misbehaviour
                    warn!(
                        "Received store and forward message from PublicKey={}. Store and forward feature is not \
                         supported by this node. Discarding message.",
                        msg.source_peer.public_key
                    );
                    false
                },
                _ => true,
            }
        }
    }
}

/// Provides the gossip filtering rules for an inbound message
fn filter_messages_to_rebroadcast(msg: &DecryptedDhtMessage) -> bool {
    // Let the message through if:
    // it isn't a duplicate (normal message), or
    let should_continue = !msg.is_duplicate() ||
        (
            // it is a duplicate domain message (i.e. not DHT or SAF protocol message), and
            msg.dht_header.message_type.is_domain_message() &&
                // it has an unknown destination (e.g complete transactions, blocks, misc. encrypted
                // messages) we allow it to proceed, which in turn, re-propagates it for another round.
                msg.dht_header.destination.is_unknown()
        );

    if should_continue {
        // The message has been forwarded, but downstream middleware may be interested
        debug!(
            target: LOG_TARGET,
            "[filter_messages_to_rebroadcast] Passing message {} to next service (Trace: {})",
            msg.tag,
            msg.dht_header.message_tag
        );
        true
    } else {
        debug!(
            target: LOG_TARGET,
            "[filter_messages_to_rebroadcast] Discarding duplicate message {}", msg
        );
        false
    }
}

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use tari_comms::{
        message::{MessageExt, MessageTag},
        pipeline::SinkService,
        runtime,
        test_utils::mocks::create_connectivity_mock,
        wrap_in_envelope_body,
    };
    use tari_shutdown::Shutdown;
    use tokio::{sync::mpsc, task, time};
    use tower::{layer::Layer, Service};

    use super::*;
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
            service_spy,
        },
    };

    #[runtime::test]
    async fn stack_unencrypted() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _) = mpsc::channel(10);

        let shutdown = Shutdown::new();
        let dht = Dht::builder()
            .local_test()
            .with_outbound_sender(out_tx)
            .build(
                Arc::clone(&node_identity),
                peer_manager,
                connectivity,
                shutdown.to_signal(),
            )
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
            false,
        );
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        let msg = {
            service.call(inbound_message).await.unwrap();
            let msg = time::timeout(Duration::from_secs(10), out_rx.recv())
                .await
                .unwrap()
                .unwrap();
            msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap()
        };

        assert_eq!(msg, b"secret");
    }

    #[runtime::test]
    async fn stack_encrypted() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _out_rx) = mpsc::channel(10);

        let shutdown = Shutdown::new();
        let dht = Dht::builder()
            .with_outbound_sender(out_tx)
            .build(
                Arc::clone(&node_identity),
                peer_manager,
                connectivity,
                shutdown.to_signal(),
            )
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
            true,
        );

        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        let msg = {
            service.call(inbound_message).await.unwrap();
            let msg = time::timeout(Duration::from_secs(10), out_rx.recv())
                .await
                .unwrap()
                .unwrap();
            msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap()
        };

        assert_eq!(msg, b"secret");
    }

    #[runtime::test]
    async fn stack_forward() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let shutdown = Shutdown::new();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        let (connectivity, _) = create_connectivity_mock();
        let (oms_requester, oms_mock) = create_outbound_service_mock(1);

        // Send all outbound requests to the mock
        let dht = Dht::builder()
            .with_outbound_sender(oms_requester.get_mpsc_sender())
            .build(
                Arc::clone(&node_identity),
                peer_manager,
                connectivity,
                shutdown.to_signal(),
            )
            .await
            .unwrap();
        let oms_mock_state = oms_mock.get_state();
        task::spawn(oms_mock.run());

        let spy = service_spy();
        let mut service = dht.inbound_middleware_layer().layer(spy.to_service());

        let msg = wrap_in_envelope_body!(b"unencrypteable".to_vec());

        // Encrypt for someone else
        let node_identity2 = make_node_identity();
        let ecdh_key = crypt::generate_ecdh_secret(node_identity2.secret_key(), node_identity2.public_key());
        let encrypted_bytes = crypt::encrypt(&ecdh_key, &msg.to_encoded_bytes()).unwrap();
        let dht_envelope = make_dht_envelope(
            &node_identity2,
            encrypted_bytes,
            DhtMessageFlags::ENCRYPTED,
            true,
            MessageTag::new(),
            true,
        );

        let origin_mac = dht_envelope.header.as_ref().unwrap().origin_mac.clone();
        assert!(!origin_mac.is_empty());
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        service.call(inbound_message).await.unwrap();

        assert_eq!(oms_mock_state.call_count(), 1);
        let (params, _) = oms_mock_state.pop_call().unwrap();

        // Check that OMS got a request to forward with the original Dht Header
        assert_eq!(params.dht_header.unwrap().origin_mac, origin_mac);

        // Check the next service was not called
        assert_eq!(spy.call_count(), 0);
    }

    #[runtime::test]
    async fn stack_filter_saf_message() {
        let node_identity = make_client_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _) = mpsc::channel(10);

        let shutdown = Shutdown::new();
        let dht = Dht::builder()
            .with_outbound_sender(out_tx)
            .build(
                Arc::clone(&node_identity),
                peer_manager,
                connectivity,
                shutdown.to_signal(),
            )
            .await
            .unwrap();

        // SAF messages need to be requested before any response is accepted
        dht.store_and_forward_requester()
            .request_saf_messages_from_peer(node_identity.node_id().clone())
            .await
            .unwrap();

        let spy = service_spy();
        let mut service = dht.inbound_middleware_layer().layer(spy.to_service());

        let msg = wrap_in_envelope_body!(b"secret".to_vec());
        let mut dht_envelope = make_dht_envelope(
            &node_identity,
            msg.to_encoded_bytes(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
            false,
        );
        dht_envelope.header.as_mut().unwrap().message_type = DhtMessageType::SafStoredMessages as i32;
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        service.call(inbound_message).await.unwrap_err();
        assert_eq!(spy.call_count(), 0);
    }
}
