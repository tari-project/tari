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
use tari_common_sqlite::{
    DbProfile,
    connection::{DbConnection, DbConnectionUrl},
    error::StorageError,
};
use tari_comms::{
    connectivity::ConnectivityRequester,
    message::{InboundMessage, OutboundMessage},
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    pipeline::PipelineError,
};
use tari_shutdown::ShutdownSignal;
use tari_utilities::epoch_time::EpochTime;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tower::{Service, ServiceBuilder, layer::Layer};

use self::outbound::OutboundMessageRequester;
use crate::{
    DedupLayer,
    DhtActorError,
    DhtBuilder,
    DhtConfig,
    SeedPeerProvider,
    actor::{DhtActor, DhtRequest, DhtRequester},
    connectivity::{DhtConnectivity, MetricsCollector, MetricsCollectorHandle},
    discovery::{DhtDiscoveryRequest, DhtDiscoveryRequester, DhtDiscoveryService},
    envelope::DhtMessageType,
    event::{DhtEventReceiver, DhtEventSender},
    filter,
    inbound,
    inbound::{DecryptedDhtMessage, DhtInboundMessage, ForwardLayer, MetricsLayer, metrics},
    logging_middleware::MessageLoggingLayer,
    network_discovery::DhtNetworkDiscovery,
    outbound,
    outbound::DhtOutboundRequest,
    rpc,
    storage::MIGRATIONS,
};

const LOG_TARGET: &str = "comms::dht";

const DHT_DISCOVERY_CHANNEL_SIZE: usize = 100;
const DHT_EVENT_BROADCAST_CHANNEL_SIZE: usize = 100;

#[derive(Debug, Error)]
pub enum DhtInitializationError {
    #[error("Database initialization failed: {0}")]
    DatabaseMigrationFailed(#[from] StorageError),
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
    outbound_tx: mpsc::UnboundedSender<DhtOutboundRequest>,
    /// Sender for DHT requests
    dht_sender: mpsc::UnboundedSender<DhtRequest>,
    /// Sender for DHT discovery requests
    discovery_sender: mpsc::Sender<DhtDiscoveryRequest>,
    /// Connectivity actor requester
    connectivity: ConnectivityRequester,
    /// Event stream sender
    event_publisher: DhtEventSender,
    /// Used by MetricsLayer to collect metrics and to inform heuristics for peer banning
    metrics_collector: MetricsCollectorHandle,
    /// Re-resolves seed peers for a rebootstrap
    seed_peer_provider: Option<Arc<dyn SeedPeerProvider>>,
}

impl Dht {
    pub(crate) async fn initialize(
        config: DhtConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_tx: mpsc::UnboundedSender<DhtOutboundRequest>,
        connectivity: ConnectivityRequester,
        seed_peer_provider: Option<Arc<dyn SeedPeerProvider>>,
        shutdown_signal: ShutdownSignal,
    ) -> Result<Self, DhtInitializationError> {
        let mut config = config;
        config.clamp_rebootstrap_settings();
        let (dht_sender, dht_receiver) = mpsc::unbounded_channel();
        let (discovery_sender, discovery_receiver) = mpsc::channel(DHT_DISCOVERY_CHANNEL_SIZE);
        let (event_publisher, _) = broadcast::channel(DHT_EVENT_BROADCAST_CHANNEL_SIZE);

        // One sample more than the flood-ban count is all the flood-ban check needs to see a flooding peer
        let metrics_collector = MetricsCollector::spawn(
            config.flood_ban_max_msg_count.saturating_add(1),
            config.flood_ban_timespan,
        );

        let dht = Self {
            node_identity,
            peer_manager,
            metrics_collector,
            config: Arc::new(config.clone()),
            outbound_tx,
            dht_sender,
            connectivity,
            discovery_sender,
            event_publisher,
            seed_peer_provider,
        };

        // Each `:memory:` connection is a database of its own, so an in-memory DHT database gets exactly one. A
        // shared-cache in-memory database locks whole tables and fails a reader at once (SQLITE_LOCKED, no busy
        // wait) while another connection writes, so it gets one too.
        let pool_size = match dht.config.database_url {
            DbConnectionUrl::Memory | DbConnectionUrl::MemoryShared(_) => 1,
            DbConnectionUrl::File(_) => 16,
        };
        let conn = DbConnection::connect_and_migrate(
            &dht.config.database_url.clone(),
            MIGRATIONS,
            Some(pool_size),
            DbProfile::Cache,
        )
        .map_err(DhtInitializationError::DatabaseMigrationFailed)?;

        let connectivity = dht.connectivity_service(shutdown_signal.clone());
        dht.network_discovery_service(shutdown_signal.clone())
            .with_pool_peers(connectivity.pool_peers())
            .spawn();
        connectivity.spawn();
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
        rpc::DhtService::new(rpc::DhtRpcServiceImpl::new(
            self.peer_manager.clone(),
            self.config.clone(),
        ))
    }

    /// Create a DHT actor
    fn actor(
        &self,
        conn: DbConnection,
        request_receiver: mpsc::UnboundedReceiver<DhtRequest>,
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
            self.dht_requester(),
            self.outbound_requester(),
            request_receiver,
            shutdown_signal,
        )
    }

    fn connectivity_service(&self, shutdown_signal: ShutdownSignal) -> DhtConnectivity {
        DhtConnectivity::new(
            self.config.clone(),
            self.peer_manager.clone(),
            self.connectivity.clone(),
            self.dht_requester(),
            self.event_publisher.clone(),
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
            self.seed_peer_provider.clone(),
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
            Future = impl Future<Output = Result<(), PipelineError>> + use<S>,
        > + Clone
                  + use<S>,
    > + use<S>
    where
        S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Send + Sync + 'static,
        S::Future: Send,
    {
        ServiceBuilder::new()
            .layer(MetricsLayer::new(self.metrics_collector.clone()))
            .layer(inbound::DeserializeLayer::new(
                self.peer_manager.clone(),
                self.connectivity.clone(),
            ))
            .layer(filter::FilterLayer::new(self.unsupported_saf_messages_filter()))
            .layer(filter::FilterLayer::new(discard_expired_messages))
            .layer(inbound::DecryptionLayer::new(self.node_identity.clone()))
            .layer(DedupLayer::new(
                self.dht_requester(),
                self.config.dedup_allowed_message_occurrences,
            ))
            .layer(filter::FilterLayer::new(filter_messages_to_rebroadcast))
            .layer(MessageLoggingLayer::new(format!(
                "Inbound [{}]",
                self.node_identity.node_id().short_str()
            )))
            .layer(ForwardLayer::new(
                self.dht_requester(),
                self.outbound_requester(),
                self.node_identity.features().contains(PeerFeatures::DHT_STORE_FORWARD) &&
                    self.config.enable_forwarding,
            ))
            .layer(inbound::DhtHandlerLayer::new(
                self.config.clone(),
                self.node_identity.clone(),
                self.peer_manager.clone(),
                self.dht_requester(),
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
            Future = impl Future<Output = Result<(), PipelineError>> + use<S>,
        > + Clone
                  + use<S>,
    > + use<S>
    where
        S: Service<OutboundMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
        S::Future: Send,
    {
        ServiceBuilder::new()
            .layer(MessageLoggingLayer::new(format!(
                "Pre Broadcast [{}]",
                self.node_identity.node_id().short_str()
            )))
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
    fn unsupported_saf_messages_filter(
        &self,
    ) -> impl filter::Predicate<DhtInboundMessage> + Clone + Send + 'static + use<> {
        move |msg: &DhtInboundMessage| match msg.dht_header.message_type {
            DhtMessageType::SafRequestMessages => {
                debug!(
                    target: LOG_TARGET,
                    "Received store and forward message from PublicKey={}. Store and forward feature is not supported \
                     by this node. Discarding message.",
                    msg.source_peer.public_key
                );
                false
            },
            _ => true,
        }
    }
}

/// Provides the gossip filtering rules for an inbound message
fn filter_messages_to_rebroadcast(msg: &DecryptedDhtMessage) -> bool {
    // Let the message through if:
    // it isn't a duplicate (normal message), or
    let should_continue = !msg.is_duplicate() ||
        (
            // it is a duplicate domain message (i.e. not DHT protocol message), and
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
            "[filter_messages_to_rebroadcast] Discarding duplicate message {msg}"
        );
        false
    }
}

/// Check message expiry and immediately discard if expired
fn discard_expired_messages(msg: &DhtInboundMessage) -> bool {
    if let Some(expires) = msg.dht_header.expires &&
        expires < EpochTime::now()
    {
        // The signature of an expired message is not verified, so it counts by the signer it claims
        metrics::set_flood_series(metrics::claimed_flood_series(msg));
        debug!(
            target: LOG_TARGET,
            "[discard_expired_messages] Discarding expired message {msg}"
        );
        return false;
    }
    true
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use futures::future;
    use tari_comms::{
        BytesMut,
        message::{MessageExt, MessageTag, OutboundMessage},
        peer_manager::NodeId,
        pipeline::SinkService,
        test_utils::mocks::{ConnectivityManagerMockState, create_connectivity_mock},
        types::{CommsDHKE, CommsPublicKey},
        wrap_in_envelope_body,
    };
    use tari_shutdown::Shutdown;
    use tari_utilities::ByteArray;
    use tokio::{task, time};
    use tower::service_fn;

    use super::*;
    use crate::{
        connectivity::FloodSeries,
        crypt,
        envelope::{DhtEnvelope, DhtMessageFlags, DhtMessageHeader, NodeDestination},
        outbound::{
            SendMessageParams,
            mock::{OutboundServiceMockState, create_outbound_service_mock},
        },
        proto::dht::{DiscoveryMessage, JoinMessage},
        test_utils::{
            build_peer_manager,
            make_comms_inbound_message,
            make_dht_envelope,
            make_node_identity,
            make_valid_message_signature,
            service_spy,
        },
        version::DhtProtocolVersion,
    };

    /// An in-memory database is private to one connection, so the default (`:memory:`) configuration must get a
    /// single-connection pool; a larger one is refused at startup.
    #[tokio::test]
    async fn dht_initialize_with_memory_url_uses_single_connection() {
        let config = DhtConfig::default_local_test();
        assert!(matches!(config.database_url, DbConnectionUrl::Memory));

        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();
        let (out_tx, _) = mpsc::unbounded_channel();
        let shutdown = Shutdown::new();
        let dht = Dht::builder()
            .with_config(config)
            .with_outbound_sender(out_tx)
            .build(node_identity, peer_manager, connectivity, shutdown.to_signal())
            .await;
        assert!(dht.is_ok(), "{:?}", dht.err());
    }

    #[tokio::test]
    async fn test_stack_unencrypted() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _) = mpsc::unbounded_channel();

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
        // Don't encrypt
        let dht_envelope = make_dht_envelope(
            &node_identity,
            &msg,
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
            false,
        )
        .unwrap();
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        let msg = {
            service.call(inbound_message).await.unwrap();
            let msg = time::timeout(Duration::from_secs(10), out_rx.recv())
                .await
                .unwrap()
                .unwrap();
            msg.success()
                .unwrap()
                .decode_part_with_max_items::<Vec<u8>>(0, 0)
                .unwrap()
                .unwrap()
        };

        assert_eq!(msg, b"secret");
    }

    /// A re-encoded signature (here: an appended unknown field) still verifies. The decryption layer drops it as
    /// non-canonical, and the dedup key is canonical in any case (see `dedup_layer_dedups_reencoded_signature` in
    /// actor.rs for the layer below that check), so the application sees the message once and nobody is banned.
    #[tokio::test]
    async fn test_stack_dedups_reencoded_signature() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, mock) = create_connectivity_mock();
        let connectivity_state = mock.spawn();
        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();
        let (out_tx, _) = mpsc::unbounded_channel();
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

        let body = wrap_in_envelope_body!(b"signed".to_vec()).to_encoded_bytes();
        let mut header = DhtMessageHeader {
            version: DhtProtocolVersion::latest(),
            destination: NodeDestination::Unknown,
            ephemeral_public_key: None,
            message_signature: Vec::new(),
            message_type: DhtMessageType::None,
            flags: DhtMessageFlags::empty(),
            message_tag: MessageTag::new(),
            expires: None,
        };
        let binding_hash = crypt::create_message_domain_separated_hash(&header, &body);
        header.message_signature = make_valid_message_signature(node_identity.secret_key(), &binding_hash);
        let honest = DhtEnvelope::new(header.into(), body);
        let mut reencoded = honest.clone();
        reencoded
            .header
            .as_mut()
            .unwrap()
            .message_signature
            .extend_from_slice(&[0x78, 0x01]);

        service
            .call(make_comms_inbound_message(
                &node_identity,
                honest.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        let received = time::timeout(Duration::from_secs(10), out_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.authenticated_origin(), Some(node_identity.public_key()));

        // The re-encoded copy does not reach the application
        service
            .call(make_comms_inbound_message(
                &node_identity,
                reencoded.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        assert!(out_rx.try_recv().is_err());

        // Accepted: with the signature stripped it is an unsigned message with its own key, and has no origin
        let mut stripped = honest;
        stripped.header.as_mut().unwrap().message_signature.clear();
        service
            .call(make_comms_inbound_message(
                &node_identity,
                stripped.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        let received = out_rx.try_recv().unwrap();
        assert!(received.authenticated_origin().is_none());

        time::sleep(Duration::from_millis(200)).await;
        assert_eq!(connectivity_state.count_calls_containing("BanPeer").await, 0);
    }

    /// An unsigned body is not bound by a signature, and nothing checks that it is canonically encoded, so re-encoding
    /// it (here: an appended unknown field) reaches dedup and must not give the message a new dedup key.
    #[tokio::test]
    async fn test_stack_dedups_reencoded_unsigned_body() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, mock) = create_connectivity_mock();
        let connectivity_state = mock.spawn();
        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();
        let (out_tx, _) = mpsc::unbounded_channel();
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

        let honest = make_dht_envelope(
            &node_identity,
            &wrap_in_envelope_body!(b"unsigned".to_vec()),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
            false,
        )
        .unwrap();
        let mut reencoded = honest.clone();
        reencoded.body.extend_from_slice(&[0x78, 0x01]);

        service
            .call(make_comms_inbound_message(
                &node_identity,
                honest.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        time::timeout(Duration::from_secs(10), out_rx.recv())
            .await
            .unwrap()
            .unwrap();

        service
            .call(make_comms_inbound_message(
                &node_identity,
                reencoded.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        assert!(out_rx.try_recv().is_err());

        time::sleep(Duration::from_millis(200)).await;
        assert_eq!(connectivity_state.count_calls_containing("BanPeer").await, 0);
    }

    /// A node that broadcasts an unsigned body that is not canonically encoded must record the key its echo will
    /// have, or it would deliver and propagate its own message again.
    #[tokio::test]
    async fn test_stack_dedups_echo_of_non_canonical_unsigned_broadcast() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();
        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();
        let (oms_tx, mut oms_rx) = mpsc::unbounded_channel();
        let shutdown = Shutdown::new();
        let dht = Dht::builder()
            .local_test()
            .with_outbound_sender(oms_tx)
            .build(
                Arc::clone(&node_identity),
                peer_manager,
                connectivity,
                shutdown.to_signal(),
            )
            .await
            .unwrap();
        let (out_tx, mut out_rx) = mpsc::channel(10);
        let mut inbound = dht.inbound_middleware_layer().layer(SinkService::new(out_tx));
        let mut outbound = dht.outbound_middleware_layer().layer(service_fn(|_: OutboundMessage| {
            future::ready(Ok::<_, PipelineError>(()))
        }));

        let canonical = wrap_in_envelope_body!(b"own message".to_vec()).to_encoded_bytes();
        let mut body = canonical.clone();
        body.extend_from_slice(&[0x78, 0x01]);

        dht.outbound_requester()
            .send_raw_no_wait(
                SendMessageParams::new().flood(vec![]).finish(),
                BytesMut::from(body.as_slice()),
            )
            .await
            .unwrap();
        let request = time::timeout(Duration::from_secs(10), oms_rx.recv())
            .await
            .unwrap()
            .unwrap();
        outbound.call(request).await.unwrap();

        // The key recorded is the one the inbound path computes from the decoded body
        let inbound_key = crate::dedup::create_message_hash(&[], &canonical);
        let hit_count = dht
            .dht_requester()
            .get_message_cache_hit_count(inbound_key.to_vec())
            .await
            .unwrap();
        assert_eq!(hit_count, 1);

        // The echo is a duplicate
        let echo = DhtEnvelope::new(
            DhtMessageHeader {
                version: DhtProtocolVersion::latest(),
                destination: NodeDestination::Unknown,
                ephemeral_public_key: None,
                message_signature: Vec::new(),
                message_type: DhtMessageType::None,
                flags: DhtMessageFlags::empty(),
                message_tag: MessageTag::new(),
                expires: None,
            }
            .into(),
            body,
        );
        inbound
            .call(make_comms_inbound_message(
                &node_identity,
                echo.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        assert!(out_rx.try_recv().is_err());
    }

    /// Forwarding a message this node cannot decrypt must not record a key for its ciphertext as if it were an
    /// unsigned body: an attacker could make that ciphertext equal an unsigned message B and have B dropped.
    #[tokio::test]
    async fn test_stack_forward_does_not_shadow_unsigned_message() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();
        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();
        let (oms_tx, mut oms_rx) = mpsc::unbounded_channel();
        let shutdown = Shutdown::new();
        let dht = Dht::builder()
            .local_test()
            .with_outbound_sender(oms_tx)
            .build(
                Arc::clone(&node_identity),
                peer_manager,
                connectivity,
                shutdown.to_signal(),
            )
            .await
            .unwrap();
        let (out_tx, mut out_rx) = mpsc::channel(10);
        let mut inbound = dht.inbound_middleware_layer().layer(SinkService::new(out_tx));
        let mut outbound = dht.outbound_middleware_layer().layer(service_fn(|_: OutboundMessage| {
            future::ready(Ok::<_, PipelineError>(()))
        }));

        // B: an unsigned cleartext message
        let b_body = wrap_in_envelope_body!(b"pending transaction".to_vec()).to_encoded_bytes();
        let b = DhtEnvelope::new(
            DhtMessageHeader {
                version: DhtProtocolVersion::latest(),
                destination: NodeDestination::Unknown,
                ephemeral_public_key: None,
                message_signature: Vec::new(),
                message_type: DhtMessageType::None,
                flags: DhtMessageFlags::empty(),
                message_tag: MessageTag::new(),
                expires: None,
            }
            .into(),
            b_body.clone(),
        );

        // A throwaway-signed "encrypted" message for another node whose body is B's body
        let (throwaway_key, ephemeral_public_key) = CommsPublicKey::random_keypair(&mut rand::rng());
        let mut header = DhtMessageHeader {
            version: DhtProtocolVersion::latest(),
            destination: make_node_identity().public_key().clone().into(),
            ephemeral_public_key: Some(ephemeral_public_key),
            message_signature: Vec::new(),
            message_type: DhtMessageType::None,
            flags: DhtMessageFlags::ENCRYPTED,
            message_tag: MessageTag::new(),
            expires: None,
        };
        let binding_hash = crypt::create_message_domain_separated_hash(&header, &b_body);
        header.message_signature = make_valid_message_signature(&throwaway_key, &binding_hash);
        let decoy = DhtEnvelope::new(header.into(), b_body.clone());

        // The node forwards the decoy through its real outbound pipeline
        inbound
            .call(make_comms_inbound_message(
                &node_identity,
                decoy.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        let forward = time::timeout(Duration::from_secs(10), oms_rx.recv())
            .await
            .unwrap()
            .unwrap();
        outbound.call(forward).await.unwrap();

        // No key was recorded for the ciphertext as an unsigned body
        let unsigned_key = crate::dedup::create_message_hash(&[], &b_body);
        let hit_count = dht
            .dht_requester()
            .get_message_cache_hit_count(unsigned_key.to_vec())
            .await
            .unwrap();
        assert_eq!(hit_count, 0);

        // B still reaches the application
        inbound
            .call(make_comms_inbound_message(&node_identity, b.to_encoded_bytes().into()))
            .await
            .unwrap();
        let received = time::timeout(Duration::from_secs(10), out_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(received.authenticated_origin().is_none());
        assert!(received.decryption_succeeded());
    }

    #[tokio::test]
    async fn test_stack_encrypted() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let (connectivity, _) = create_connectivity_mock();

        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();

        // Dummy out channel, we are not testing outbound here.
        let (out_tx, _) = mpsc::unbounded_channel();

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
        // Encrypt for self
        let dht_envelope = make_dht_envelope(
            &node_identity,
            &msg,
            DhtMessageFlags::ENCRYPTED,
            true,
            MessageTag::new(),
            true,
        )
        .unwrap();

        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        let msg = {
            service.call(inbound_message).await.unwrap();
            let msg = time::timeout(Duration::from_secs(10), out_rx.recv())
                .await
                .unwrap()
                .unwrap();
            msg.success()
                .unwrap()
                .decode_part_with_max_items::<Vec<u8>>(0, 0)
                .unwrap()
                .unwrap()
        };

        assert_eq!(msg, b"secret");
    }

    #[tokio::test]
    async fn test_stack_forward() {
        let node_identity = make_node_identity();
        let peer_manager = build_peer_manager();
        let shutdown = Shutdown::new();

        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();

        let (connectivity, _) = create_connectivity_mock();
        let (oms_requester, oms_mock) = create_outbound_service_mock();

        // Send all outbound requests to the mock
        let dht = Dht::builder()
            .with_outbound_sender(oms_requester.get_mpsc_sender())
            .with_config(DhtConfig::default_local_test())
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
        let ecdh_key = CommsDHKE::new(
            node_identity2.secret_key(),
            &node_identity2.public_key().to_public_key().unwrap(),
        );
        let key_message = crypt::generate_key_message(&ecdh_key);
        let mut encrypted_bytes = msg.encode_into_bytes_mut();
        crypt::encrypt_message(&key_message, &mut encrypted_bytes, b"test associated data").unwrap();
        let dht_envelope = make_dht_envelope(
            &node_identity2,
            &encrypted_bytes.to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
            MessageTag::new(),
            true,
        )
        .unwrap();

        let message_signature = dht_envelope.header.as_ref().unwrap().message_signature.clone();
        assert!(!message_signature.is_empty());
        let inbound_message = make_comms_inbound_message(&node_identity, dht_envelope.to_encoded_bytes().into());

        service.call(inbound_message).await.unwrap();

        oms_mock_state
            .wait_call_count(1, Duration::from_secs(10))
            .await
            .unwrap();
        let (params, _) = oms_mock_state.pop_call().await.unwrap();

        // Check that OMS got a request to forward with the original Dht Header
        assert_eq!(params.dht_header.unwrap().message_signature, message_signature);

        // Check the next service was not called
        assert_eq!(spy.call_count(), 0);
    }

    /// Builds a DHT that knows `peers`, whose bans go to `connectivity` and whose outbound requests go to `out_tx`
    async fn build_dht(
        node_identity: Arc<NodeIdentity>,
        peers: &[&NodeIdentity],
        connectivity: ConnectivityRequester,
        out_tx: mpsc::UnboundedSender<DhtOutboundRequest>,
        shutdown: &Shutdown,
    ) -> Dht {
        let peer_manager = build_peer_manager();
        for peer in peers {
            peer_manager.add_or_update_peer(peer.to_peer()).await.unwrap();
        }
        Dht::builder()
            .with_config(DhtConfig::default_local_test())
            .with_outbound_sender(out_tx)
            .build(node_identity, peer_manager, connectivity, shutdown.to_signal())
            .await
            .unwrap()
    }

    /// A clear text (unless `ephemeral_public_key` is given) envelope carrying `body` as is, validly signed by `signer`
    fn signed_envelope(
        signer: &NodeIdentity,
        message_type: DhtMessageType,
        destination: NodeDestination,
        ephemeral_public_key: Option<CommsPublicKey>,
        body: Vec<u8>,
    ) -> DhtEnvelope {
        let flags = if ephemeral_public_key.is_some() {
            DhtMessageFlags::ENCRYPTED
        } else {
            DhtMessageFlags::NONE
        };
        let mut header = DhtMessageHeader {
            version: DhtProtocolVersion::latest(),
            destination,
            message_signature: Vec::new(),
            ephemeral_public_key,
            message_type,
            flags,
            message_tag: MessageTag::new(),
            expires: None,
        };
        let binding_hash = crypt::create_message_domain_separated_hash(&header, &body);
        header.message_signature = make_valid_message_signature(signer.secret_key(), &binding_hash);
        DhtEnvelope::new(header.into(), body)
    }

    /// A clear text domain message signed by `signer`
    fn signed_domain_envelope(signer: &NodeIdentity) -> DhtEnvelope {
        signed_envelope(
            signer,
            DhtMessageType::None,
            NodeDestination::Unknown,
            None,
            wrap_in_envelope_body!(b"flood".to_vec()).to_encoded_bytes(),
        )
    }

    const FLOOD_TEST_MAX_COUNT: usize = 5;

    /// Builds a DHT that knows `peers` and flags a peer for flooding after `FLOOD_TEST_MAX_COUNT` messages
    async fn build_flood_test_dht(peers: &[&NodeIdentity], shutdown: &Shutdown) -> Dht {
        let peer_manager = build_peer_manager();
        for peer in peers {
            peer_manager.add_or_update_peer(peer.to_peer()).await.unwrap();
        }
        let (connectivity, _) = create_connectivity_mock();
        let (out_tx, _) = mpsc::unbounded_channel();
        Dht::builder()
            .with_config(DhtConfig {
                flood_ban_max_msg_count: FLOOD_TEST_MAX_COUNT,
                ..DhtConfig::default_local_test()
            })
            .with_outbound_sender(out_tx)
            .build(make_node_identity(), peer_manager, connectivity, shutdown.to_signal())
            .await
            .unwrap()
    }

    /// The peers the flood check would ban, and the series that got them banned
    async fn flooding_peers(dht: &Dht) -> Vec<(NodeId, FloodSeries)> {
        dht.metrics_collector()
            .take_flooding_peers()
            .await
            .unwrap()
            .into_iter()
            .map(|(node_id, _, series)| (node_id, series))
            .collect()
    }

    /// Two sybils, each under the limit, send messages through relay H. H's neighbour does not ban H at the own
    /// limit: the messages are signed by the sybils, so they count in H's forwarded series, which bans only at
    /// `FORWARDED_FLOOD_BAN_FACTOR` x the limit.
    #[tokio::test]
    async fn relayed_messages_count_in_the_forwarded_series() {
        let (sybil_a, sybil_b) = (make_node_identity(), make_node_identity());
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sybil_a, &sybil_b, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let forwarded_limit = FLOOD_TEST_MAX_COUNT * crate::config::FORWARDED_FLOOD_BAN_FACTOR;
        let relay_signed = |i: usize| {
            let sybil = if i.is_multiple_of(2) { &sybil_a } else { &sybil_b };
            signed_domain_envelope(sybil).to_encoded_bytes()
        };
        for i in 0..forwarded_limit {
            service
                .call(make_comms_inbound_message(&relay, relay_signed(i).into()))
                .await
                .unwrap();
        }
        assert!(flooding_peers(&dht).await.is_empty());
        let mut metrics = dht.metrics_collector();
        let own = metrics
            .get_messages_received_timeseries(relay.node_id().clone(), FloodSeries::Own)
            .await
            .unwrap();
        assert_eq!(own.count(), 0);
        let forwarded = metrics
            .get_messages_received_timeseries(relay.node_id().clone(), FloodSeries::Forwarded)
            .await
            .unwrap();
        assert_eq!(forwarded.count(), forwarded_limit);

        // A relay that forwards more than that is banned for it
        service
            .call(make_comms_inbound_message(&relay, relay_signed(0).into()))
            .await
            .unwrap();
        assert_eq!(flooding_peers(&dht).await, vec![(
            relay.node_id().clone(),
            FloodSeries::Forwarded
        )]);
    }

    /// The flood-ban series counts of `peer`: (own, forwarded)
    async fn series_counts(dht: &Dht, peer: &NodeIdentity) -> (usize, usize) {
        let mut metrics = dht.metrics_collector();
        let node_id = peer.node_id().clone();
        let own = metrics
            .get_messages_received_timeseries(node_id.clone(), FloodSeries::Own)
            .await
            .unwrap();
        let forwarded = metrics
            .get_messages_received_timeseries(node_id, FloodSeries::Forwarded)
            .await
            .unwrap();
        (own.count(), forwarded.count())
    }

    /// An expired message is dropped before its signature is verified. Relay H is charged on its own series only when
    /// the message names H as its signer; one naming another signer counts in H's forwarded series.
    #[tokio::test]
    async fn an_expired_message_counts_by_the_signer_it_names() {
        let sybil = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sybil, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let expired = |signer: &NodeIdentity| {
            let body = wrap_in_envelope_body!(b"expired".to_vec()).to_encoded_bytes();
            let mut header = DhtMessageHeader {
                version: DhtProtocolVersion::latest(),
                destination: NodeDestination::Unknown,
                message_signature: Vec::new(),
                ephemeral_public_key: None,
                message_type: DhtMessageType::None,
                flags: DhtMessageFlags::NONE,
                message_tag: MessageTag::new(),
                expires: Some(EpochTime::from_secs_since_epoch(1)),
            };
            let binding_hash = crypt::create_message_domain_separated_hash(&header, &body);
            header.message_signature = make_valid_message_signature(signer.secret_key(), &binding_hash);
            DhtEnvelope::new(header.into(), body).to_encoded_bytes()
        };
        for signer in [&sybil, &relay] {
            service
                .call(make_comms_inbound_message(&relay, expired(signer).into()))
                .await
                .unwrap();
        }
        assert_eq!(series_counts(&dht, &relay).await, (1, 1));
    }

    /// A validly signed cleartext message whose body does not decode counts by the signer it names: relay H's own if H
    /// signed it, its forwarded series if another peer did
    #[tokio::test]
    async fn an_undecodable_signed_body_counts_by_its_signer() {
        let sybil = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sybil, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        for signer in [&sybil, &relay] {
            let envelope = signed_envelope(signer, DhtMessageType::None, NodeDestination::Unknown, None, vec![
                0xff;
                8
            ]);
            service
                .call(make_comms_inbound_message(&relay, envelope.to_encoded_bytes().into()))
                .await
                .unwrap();
        }
        assert_eq!(series_counts(&dht, &relay).await, (1, 1));
    }

    /// A non-canonically encoded signature is rejected before it is verified. One naming another signer, delivered by
    /// relay H (which an older relay without the canonical check would pass on), counts in H's forwarded series.
    #[tokio::test]
    async fn a_non_canonical_signature_from_another_signer_counts_as_forwarded() {
        let sybil = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sybil, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let mut envelope = signed_domain_envelope(&sybil);
        // Unknown field 15 (varint) = 1: the signature decodes to the same value
        envelope
            .header
            .as_mut()
            .unwrap()
            .message_signature
            .extend_from_slice(&[0x78, 0x01]);
        service
            .call(make_comms_inbound_message(&relay, envelope.to_encoded_bytes().into()))
            .await
            .unwrap();
        assert_eq!(series_counts(&dht, &relay).await, (0, 1));
    }

    /// A canonically encoded signature that fails to verify is rejected by relays of every version, so even one naming
    /// another signer counts as relay H's own
    #[tokio::test]
    async fn a_signature_that_fails_to_verify_counts_as_own() {
        let sybil = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sybil, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let mut envelope = signed_domain_envelope(&sybil);
        envelope.body = wrap_in_envelope_body!(b"not what was signed".to_vec()).to_encoded_bytes();
        service
            .call(make_comms_inbound_message(&relay, envelope.to_encoded_bytes().into()))
            .await
            .unwrap();
        assert_eq!(series_counts(&dht, &relay).await, (1, 0));
    }

    /// v6.2 and older relays re-encode the Join body they propagate, which can break its signature, so a Join from
    /// another signer that fails to verify counts in relay H's forwarded series
    #[tokio::test]
    async fn a_join_that_fails_to_verify_counts_as_forwarded() {
        let sybil = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sybil, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let mut envelope = signed_envelope(
            &sybil,
            DhtMessageType::Join,
            sybil.public_key().clone().into(),
            None,
            wrap_in_envelope_body!(JoinMessage::from(&sybil)).to_encoded_bytes(),
        );
        // A trailing unknown field, as an older relay's re-encoding could differ from the signed bytes
        envelope.body.extend_from_slice(&[0x78, 0x01]);
        service
            .call(make_comms_inbound_message(&relay, envelope.to_encoded_bytes().into()))
            .await
            .unwrap();
        assert_eq!(series_counts(&dht, &relay).await, (0, 1));
    }

    /// An encrypted message for another node fails to decrypt here and is passed on: relay H may only be forwarding
    /// it, so it counts in H's forwarded series
    #[tokio::test]
    async fn an_encrypted_message_for_another_node_counts_as_forwarded() {
        let sender = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&sender, &relay], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        // Encrypted for, and addressed to, `sender`
        let envelope = make_dht_envelope(
            &sender,
            &wrap_in_envelope_body!(b"not for us".to_vec()),
            DhtMessageFlags::ENCRYPTED,
            true,
            MessageTag::new(),
            true,
        )
        .unwrap();
        // Forwarding it on fails here (no outbound service), which does not matter for the count
        let _result = service
            .call(make_comms_inbound_message(&relay, envelope.to_encoded_bytes().into()))
            .await;
        assert_eq!(series_counts(&dht, &relay).await, (0, 1));
    }

    /// A peer that sends more than the limit of messages it signed itself is banned on its own series
    #[tokio::test]
    async fn a_peer_flooding_messages_it_signed_is_banned_on_its_own_series() {
        let peer = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&peer], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        for _ in 0..=FLOOD_TEST_MAX_COUNT {
            let bytes = signed_domain_envelope(&peer).to_encoded_bytes();
            service
                .call(make_comms_inbound_message(&peer, bytes.into()))
                .await
                .unwrap();
        }
        assert_eq!(flooding_peers(&dht).await, vec![(
            peer.node_id().clone(),
            FloodSeries::Own
        )]);
    }

    /// A duplicate first delivered by another peer does not count against the relay that delivers it again, but a
    /// peer's own repeats do.
    #[tokio::test]
    async fn flood_ban_skips_only_duplicates_first_delivered_by_another_peer() {
        let signer = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();
        let (connectivity, _) = create_connectivity_mock();
        let (out_tx, _) = mpsc::unbounded_channel();
        let dht = build_dht(
            make_node_identity(),
            &[&signer, &relay],
            connectivity,
            out_tx,
            &shutdown,
        )
        .await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let bytes = signed_domain_envelope(&signer).to_encoded_bytes();
        for peer in [&signer, &signer, &relay, &relay] {
            service
                .call(make_comms_inbound_message(peer, bytes.clone().into()))
                .await
                .unwrap();
        }

        let mut metrics = dht.metrics_collector();
        let ts = metrics
            .get_messages_received_timeseries(signer.node_id().clone(), FloodSeries::Own)
            .await
            .unwrap();
        assert_eq!(ts.count(), 2);
        let ts = metrics
            .get_messages_received_timeseries(relay.node_id().clone(), FloodSeries::Own)
            .await
            .unwrap();
        assert_eq!(ts.count(), 0);
        // The node-wide total still counts every message
        let total = metrics
            .get_total_message_count_in_timespan(Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(total, 4);
    }

    /// A peer replaying its own signed message is caught by the flood check even though dedup drops the replays
    #[tokio::test]
    async fn a_peer_replaying_its_own_message_is_flagged_for_flooding() {
        let peer = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&peer], &shutdown).await;
        let mut service = dht.inbound_middleware_layer().layer(service_spy().to_service());

        let bytes = signed_domain_envelope(&peer).to_encoded_bytes();
        for _ in 0..=FLOOD_TEST_MAX_COUNT {
            service
                .call(make_comms_inbound_message(&peer, bytes.clone().into()))
                .await
                .unwrap();
        }
        assert_eq!(flooding_peers(&dht).await, vec![(
            peer.node_id().clone(),
            FloodSeries::Own
        )]);
    }

    /// Messages dropped before dedup (here: undecodable envelopes and bad signatures) count towards the flood ban
    #[tokio::test]
    async fn dropped_messages_count_towards_the_flood_ban() {
        let undecodable_sender = make_node_identity();
        let bad_signature_sender = make_node_identity();
        let shutdown = Shutdown::new();
        let dht = build_flood_test_dht(&[&undecodable_sender, &bad_signature_sender], &shutdown).await;
        let spy = service_spy();
        let mut service = dht.inbound_middleware_layer().layer(spy.to_service());

        for i in 0..=FLOOD_TEST_MAX_COUNT {
            let result = service
                .call(make_comms_inbound_message(&undecodable_sender, vec![0xff; 8].into()))
                .await;
            assert!(result.is_err());

            // Each envelope is distinct, so dedup is not what drops them
            let mut envelope = signed_domain_envelope(&bad_signature_sender);
            envelope.body = wrap_in_envelope_body!(vec![u8::try_from(i).unwrap()]).to_encoded_bytes();
            service
                .call(make_comms_inbound_message(
                    &bad_signature_sender,
                    envelope.to_encoded_bytes().into(),
                ))
                .await
                .unwrap();
        }
        assert_eq!(spy.call_count(), 0);
        let flooding = flooding_peers(&dht).await;
        assert!(flooding.contains(&(undecodable_sender.node_id().clone(), FloodSeries::Own)));
        assert!(flooding.contains(&(bad_signature_sender.node_id().clone(), FloodSeries::Own)));
    }

    /// Waits a little for bans to reach the connectivity mock, and returns the banned node ids
    async fn banned_peers(mock_state: &ConnectivityManagerMockState) -> Vec<NodeId> {
        time::sleep(Duration::from_millis(500)).await;
        mock_state
            .take_banned_peers()
            .await
            .into_iter()
            .map(|(node_id, _, _)| node_id)
            .collect()
    }

    /// Builds relay H, whose outbound requests go to a mock so propagated messages can be captured
    async fn build_relay(
        relay: Arc<NodeIdentity>,
        peers: &[&NodeIdentity],
        shutdown: &Shutdown,
    ) -> (Dht, OutboundServiceMockState) {
        let (connectivity, _) = create_connectivity_mock();
        let (oms_requester, oms_mock) = create_outbound_service_mock();
        let oms_mock_state = oms_mock.get_state();
        task::spawn(oms_mock.run());
        let dht = build_dht(relay, peers, connectivity, oms_requester.get_mpsc_sender(), shutdown).await;
        (dht, oms_mock_state)
    }

    /// An attacker X sends relay H a Join whose signed body carries an unknown field. H propagates the body bytes it
    /// received, so N verifies the Join and accepts it. N bans no one.
    #[tokio::test]
    async fn a_relayed_join_with_a_non_canonical_body_verifies_at_the_next_hop() {
        let attacker = make_node_identity();
        let relay = make_node_identity();
        let node = make_node_identity();
        let shutdown = Shutdown::new();

        let (dht_h, oms_mock_state) = build_relay(relay.clone(), &[&attacker], &shutdown).await;
        let mut service_h = dht_h.inbound_middleware_layer().layer(service_spy().to_service());

        let mut body = wrap_in_envelope_body!(JoinMessage::from(&attacker)).to_encoded_bytes();
        body.extend_from_slice(&[0x10, 0x01]);
        let envelope = signed_envelope(
            &attacker,
            DhtMessageType::Join,
            attacker.public_key().clone().into(),
            None,
            body.clone(),
        );
        service_h
            .call(make_comms_inbound_message(
                &attacker,
                envelope.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();

        oms_mock_state
            .wait_call_count(1, Duration::from_secs(10))
            .await
            .unwrap();
        let (params, propagated_body) = oms_mock_state.pop_call().await.unwrap();
        assert_eq!(propagated_body.to_vec(), body);

        // N receives H's propagation
        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let (out_tx, _out_rx) = mpsc::unbounded_channel();
        let dht_n = build_dht(node.clone(), &[&relay], connectivity, out_tx, &shutdown).await;
        let mut service_n = dht_n.inbound_middleware_layer().layer(service_spy().to_service());
        let propagated = DhtEnvelope::new(params.dht_header.unwrap().into(), propagated_body.to_vec());
        service_n
            .call(make_comms_inbound_message(&relay, propagated.to_encoded_bytes().into()))
            .await
            .unwrap();

        // N verified and accepted the Join, and banned no one
        assert!(banned_peers(&mock_state).await.is_empty());
        assert!(dht_n.peer_manager.exists(attacker.public_key()).await.unwrap());
    }

    /// An envelope that is not canonically encoded (here an appended unknown field) is dropped by H at deserialize:
    /// it is neither accepted nor propagated, and no one is banned.
    #[tokio::test]
    async fn a_non_canonical_envelope_is_dropped_by_the_relay() {
        let attacker = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();

        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let (oms_requester, oms_mock) = create_outbound_service_mock();
        let oms_mock_state = oms_mock.get_state();
        task::spawn(oms_mock.run());
        let dht_h = build_dht(
            relay.clone(),
            &[&attacker],
            connectivity,
            oms_requester.get_mpsc_sender(),
            &shutdown,
        )
        .await;
        let spy = service_spy();
        let mut service_h = dht_h.inbound_middleware_layer().layer(spy.to_service());

        let envelope = signed_envelope(
            &attacker,
            DhtMessageType::Join,
            attacker.public_key().clone().into(),
            None,
            wrap_in_envelope_body!(JoinMessage::from(&attacker)).to_encoded_bytes(),
        );
        let mut bytes = envelope.to_encoded_bytes();
        bytes.extend_from_slice(&[0x78, 0x01]);
        service_h
            .call(make_comms_inbound_message(&attacker, bytes.into()))
            .await
            .unwrap();

        assert!(banned_peers(&mock_state).await.is_empty());
        assert_eq!(oms_mock_state.call_count().await, 0);
        assert_eq!(spy.call_count(), 0);
    }

    /// The same signed Join delivered twice, the second time with a re-encoded (non-canonical) `message_signature`,
    /// is only accepted and propagated once.
    #[tokio::test]
    async fn a_join_with_a_re_encoded_message_signature_is_rejected() {
        let attacker = make_node_identity();
        let relay = make_node_identity();
        let shutdown = Shutdown::new();

        let (dht_h, oms_mock_state) = build_relay(relay.clone(), &[&attacker], &shutdown).await;
        let mut service_h = dht_h.inbound_middleware_layer().layer(service_spy().to_service());

        let mut envelope = signed_envelope(
            &attacker,
            DhtMessageType::Join,
            attacker.public_key().clone().into(),
            None,
            wrap_in_envelope_body!(JoinMessage::from(&attacker)).to_encoded_bytes(),
        );
        service_h
            .call(make_comms_inbound_message(
                &attacker,
                envelope.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        oms_mock_state
            .wait_call_count(1, Duration::from_secs(10))
            .await
            .unwrap();

        // Unknown field 15 (varint) = 1: the signature decodes to the same value
        let header = envelope.header.as_mut().unwrap();
        header.message_signature.extend_from_slice(&[0x78, 0x01]);
        service_h
            .call(make_comms_inbound_message(
                &attacker,
                envelope.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();

        time::sleep(Duration::from_millis(500)).await;
        assert_eq!(oms_mock_state.call_count().await, 1);
    }

    /// An attacker A asks target T for a DiscoveryResponse and replays T's signed response to victim V. V bans A, who
    /// delivered a response it did not sign, and does not ban T.
    #[tokio::test]
    async fn a_replayed_discovery_response_bans_the_replayer_not_the_signer() {
        let attacker = make_node_identity();
        let target = make_node_identity();
        let victim = make_node_identity();
        let shutdown = Shutdown::new();

        // T, with its real outbound pipeline so that its response is signed as it would be on the wire
        let (connectivity, _) = create_connectivity_mock();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let dht_t = build_dht(target.clone(), &[&attacker], connectivity, out_tx, &shutdown).await;
        let mut service_t = dht_t.inbound_middleware_layer().layer(service_spy().to_service());
        let (wire_tx, mut wire_rx) = mpsc::channel(10);
        let mut outbound_t = dht_t.outbound_middleware_layer().layer(SinkService::new(wire_tx));

        let discovery = DiscoveryMessage {
            public_key: attacker.public_key().to_vec(),
            addresses: attacker.public_addresses().iter().map(|a| a.to_vec()).collect(),
            peer_features: attacker.features().bits(),
            nonce: 1,
            identity_signature: attacker.identity_signature_read().as_ref().map(Into::into),
        };
        let envelope = signed_envelope(
            &attacker,
            DhtMessageType::Discovery,
            NodeDestination::Unknown,
            None,
            wrap_in_envelope_body!(discovery).to_encoded_bytes(),
        );
        service_t
            .call(make_comms_inbound_message(
                &attacker,
                envelope.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();
        let request = time::timeout(Duration::from_secs(10), out_rx.recv())
            .await
            .unwrap()
            .unwrap();
        outbound_t.call(request).await.unwrap();
        let response: OutboundMessage = time::timeout(Duration::from_secs(10), wire_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.peer_node_id, *attacker.node_id());

        // A replays it to V
        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let (out_tx, _out_rx) = mpsc::unbounded_channel();
        let dht_v = build_dht(victim.clone(), &[&attacker, &target], connectivity, out_tx, &shutdown).await;
        let mut service_v = dht_v.inbound_middleware_layer().layer(service_spy().to_service());
        service_v
            .call(make_comms_inbound_message(&attacker, response.body))
            .await
            .unwrap();

        assert_eq!(banned_peers(&mock_state).await, vec![attacker.node_id().clone()]);
    }

    /// A validly signed encrypted message for V whose ephemeral key is not a curve point, delivered by relay R, is
    /// dropped by V without banning R.
    #[tokio::test]
    async fn an_ephemeral_key_that_is_not_a_point_does_not_ban_the_relay() {
        let attacker = make_node_identity();
        let relay = make_node_identity();
        let victim = make_node_identity();
        let shutdown = Shutdown::new();

        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let (out_tx, _out_rx) = mpsc::unbounded_channel();
        let dht_v = build_dht(victim.clone(), &[&relay], connectivity, out_tx, &shutdown).await;
        let spy = service_spy();
        let mut service_v = dht_v.inbound_middleware_layer().layer(spy.to_service());

        let not_a_point = CommsPublicKey::from_canonical_bytes(&[0xffu8; 32]).unwrap();
        assert!(not_a_point.to_public_key().is_err());
        let envelope = signed_envelope(
            &attacker,
            DhtMessageType::None,
            NodeDestination::PublicKey(Box::new(victim.public_key().clone())),
            Some(not_a_point),
            vec![1u8; 64],
        );
        // Rejected when the header is parsed, which fails the pipeline call without a ban
        assert!(
            service_v
                .call(make_comms_inbound_message(&relay, envelope.to_encoded_bytes().into()))
                .await
                .is_err()
        );

        assert!(banned_peers(&mock_state).await.is_empty());
        assert_eq!(spy.call_count(), 0);
    }
}
