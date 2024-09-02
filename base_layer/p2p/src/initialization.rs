//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    fs,
    fs::File,
    iter,
    path::Path,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use fs2::FileExt;
use futures::future;
use lmdb_zero::open;
use log::*;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use tari_common::{
    configuration::Network,
    exit_codes::{ExitCode, ExitError},
};
use tari_comms::{
    backoff::ConstantBackoff,
    multiaddr::multiaddr,
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerFlags, PeerManagerError},
    pipeline,
    protocol::{
        messaging::{MessagingEventSender, MessagingProtocolExtension},
        rpc::RpcServer,
        NodeNetworkInfo,
        ProtocolId,
    },
    tor,
    tor::{HiddenServiceControllerError, TorIdentity},
    transports::{
        predicate::FalsePredicate,
        HiddenServiceTransport,
        MemoryTransport,
        SocksConfig,
        SocksTransport,
        TcpWithTorTransport,
    },
    utils::cidr::parse_cidrs,
    CommsBuilder,
    CommsBuilderError,
    CommsNode,
    PeerManager,
    UnspawnedCommsNode,
};
use tari_comms_dht::{Dht, DhtInitializationError};
use tari_service_framework::{async_trait, ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};
use tari_shutdown::ShutdownSignal;
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBConfig},
    LMDBWrapper,
};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceBuilder;

use crate::{
    comms_connector::{InboundDomainConnector, PubsubDomainConnector},
    config::{P2pConfig, PeerSeedsConfig},
    get_network_wire_byte,
    peer_seeds::{DnsSeedResolver, SeedPeer},
    transport::{TorTransportConfig, TransportType},
    TransportConfig,
    MAJOR_NETWORK_VERSION,
    MINOR_NETWORK_VERSION,
};
const LOG_TARGET: &str = "p2p::initialization";

/// ProtocolId for minotari messaging protocol
pub static MESSAGING_PROTOCOL_ID: ProtocolId = ProtocolId::from_static(b"t/msg/0.1");

#[derive(Debug, Error)]
pub enum CommsInitializationError {
    #[error("Comms builder error: `{0}`")]
    CommsBuilderError(#[from] CommsBuilderError),
    #[error("Failed to initialize tor hidden service: {0}")]
    HiddenServiceControllerError(#[from] HiddenServiceControllerError),
    #[error("DHT initialization error: `{0}`")]
    DhtInitializationError(#[from] DhtInitializationError),
    #[error("Hidden service builder error: `{0}`")]
    HiddenServiceBuilderError(#[from] tor::HiddenServiceBuilderError),
    #[error("Invalid liveness CIDRs error: `{0}`")]
    InvalidLivenessCidrs(String),
    #[error("Could not add seed peers to comms layer: `{0}`")]
    FailedToAddSeedPeer(#[from] PeerManagerError),
    #[error("Cannot acquire exclusive file lock, another instance of the application is already running")]
    CannotAcquireFileLock,
    #[error("Invalid tor forward address: `{0}`")]
    InvalidTorForwardAddress(std::io::Error),
    #[error("IO Error: `{0}`")]
    IoError(#[from] std::io::Error),
}

impl CommsInitializationError {
    pub fn to_exit_error(&self) -> ExitError {
        #[allow(clippy::enum_glob_use)]
        use HiddenServiceControllerError::*;
        match self {
            CommsInitializationError::HiddenServiceControllerError(TorControlPortOffline) => {
                ExitError::new(ExitCode::TorOffline, self)
            },
            CommsInitializationError::HiddenServiceControllerError(HashedPasswordAuthAutoNotSupported) => {
                ExitError::new(ExitCode::TorAuthConfiguration, self)
            },
            CommsInitializationError::HiddenServiceControllerError(FailedToLoadCookieFile(_)) => {
                ExitError::new(ExitCode::TorAuthUnreadableCookie, self)
            },

            _ => ExitError::new(ExitCode::NetworkError, self),
        }
    }
}

/// Initialize Tari Comms configured for tests
pub async fn initialize_local_test_comms<P: AsRef<Path>>(
    node_identity: Arc<NodeIdentity>,
    connector: InboundDomainConnector,
    data_path: P,
    discovery_request_timeout: Duration,
    seed_peers: Vec<Peer>,
    shutdown_signal: ShutdownSignal,
) -> Result<(UnspawnedCommsNode, Dht, MessagingEventSender), CommsInitializationError> {
    let peer_database_name = {
        let mut rng = thread_rng();
        iter::repeat(())
            .map(|_| rng.sample(Alphanumeric) as char)
            .take(8)
            .collect::<String>()
    };
    std::fs::create_dir_all(&data_path).unwrap();
    let datastore = LMDBBuilder::new()
        .set_path(&data_path)
        .set_env_flags(open::NOLOCK)
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database(&peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    //---------------------------------- Comms --------------------------------------------//

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        .with_listener_address(node_identity.first_public_address().unwrap())
        .with_listener_liveness_max_sessions(1)
        .with_node_identity(node_identity)
        .with_user_agent(&"/test/1.0")
        .with_peer_storage(peer_database, None)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_min_connectivity(1)
        .with_network_byte(Network::LocalNet.as_byte())
        .with_shutdown_signal(shutdown_signal)
        .build()?;

    add_seed_peers(&comms.peer_manager(), &comms.node_identity(), seed_peers).await?;

    // Create outbound channel
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let dht = Dht::builder()
        .local_test()
        .with_outbound_sender(outbound_tx)
        .with_discovery_timeout(discovery_request_timeout)
        .build(
            comms.node_identity(),
            comms.peer_manager(),
            comms.connectivity(),
            comms.shutdown_signal(),
        )
        .await?;

    let dht_outbound_layer = dht.outbound_middleware_layer();
    let (event_sender, _) = broadcast::channel(100);
    let pipeline = pipeline::Builder::new()
        .with_outbound_pipeline(outbound_rx, |sink| {
            ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
        })
        .max_concurrent_inbound_tasks(10)
        .with_inbound_pipeline(
            ServiceBuilder::new()
                .layer(dht.inbound_middleware_layer())
                .service(connector),
        )
        .build();

    let comms = comms.add_protocol_extension(
        MessagingProtocolExtension::new(MESSAGING_PROTOCOL_ID.clone(), event_sender.clone(), pipeline)
            .enable_message_received_event(),
    );

    Ok((comms, dht, event_sender))
}

pub async fn spawn_comms_using_transport<F: Fn(TorIdentity) + Send + Sync + Unpin + Clone + 'static>(
    comms: UnspawnedCommsNode,
    transport_config: TransportConfig,
    after_comms: F,
) -> Result<CommsNode, CommsInitializationError> {
    let comms = match transport_config.transport_type {
        TransportType::Memory => {
            debug!(target: LOG_TARGET, "Building in-memory comms stack");
            comms
                .with_listener_address(transport_config.memory.listener_address.clone())
                .spawn_with_transport(MemoryTransport)
                .await?
        },
        TransportType::Tcp => {
            let config = transport_config.tcp;
            debug!(
                target: LOG_TARGET,
                "Building TCP comms stack{}",
                config
                    .tor_socks_address
                    .as_ref()
                    .map(|_| " with Tor support")
                    .unwrap_or("")
            );
            let mut transport = TcpWithTorTransport::new();
            if let Some(addr) = config.tor_socks_address {
                transport.set_tor_socks_proxy(SocksConfig {
                    proxy_address: addr,
                    authentication: config.tor_socks_auth.into(),
                    proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
                });
            }
            comms
                .with_listener_address(config.listener_address)
                .spawn_with_transport(transport)
                .await?
        },
        TransportType::Tor => {
            let tor_config = transport_config.tor;
            debug!(target: LOG_TARGET, "Building TOR comms stack ({:?})", tor_config);
            let listener_address_override = tor_config.listener_address_override.clone();
            let hidden_service_ctl = initialize_hidden_service(tor_config)?;
            // Set the listener address to be the address (usually local) to which tor will forward all traffic
            let instant = Instant::now();
            let transport = HiddenServiceTransport::new(hidden_service_ctl, after_comms);
            debug!(target: LOG_TARGET, "TOR transport initialized in {:.0?}", instant.elapsed());

            comms
                .with_listener_address(
                    listener_address_override.unwrap_or_else(|| multiaddr![Ip4([127, 0, 0, 1]), Tcp(0u16)]),
                )
                .spawn_with_transport(transport)
                .await?
        },
        TransportType::Socks5 => {
            debug!(target: LOG_TARGET, "Building SOCKS5 comms stack");
            let transport = SocksTransport::new(transport_config.socks.into());
            comms
                .with_listener_address(transport_config.tcp.listener_address)
                .spawn_with_transport(transport)
                .await?
        },
    };

    Ok(comms)
}

fn initialize_hidden_service(
    mut config: TorTransportConfig,
) -> Result<tor::HiddenServiceController, CommsInitializationError> {
    let mut builder = tor::HiddenServiceBuilder::new()
        .with_port_mapping(config.to_port_mapping()?)
        .with_socks_authentication(config.to_socks_auth())
        .with_control_server_auth(config.to_control_auth()?)
        .with_socks_address_override(config.socks_address_override)
        .with_control_server_address(config.control_address)
        .with_bypass_proxy_addresses(config.proxy_bypass_addresses.into());

    if config.proxy_bypass_for_outbound_tcp {
        builder = builder.bypass_tor_for_tcp_addresses();
    }

    if let Some(identity) = config.identity.take() {
        builder = builder.with_tor_identity(identity);
    }

    let hidden_svc_ctl = builder.build()?;
    Ok(hidden_svc_ctl)
}

async fn configure_comms_and_dht(
    builder: CommsBuilder,
    config: &P2pConfig,
    connector: InboundDomainConnector,
) -> Result<(UnspawnedCommsNode, Dht), CommsInitializationError> {
    let file_lock = acquire_exclusive_file_lock(&config.datastore_path)?;

    let datastore = LMDBBuilder::new()
        .set_path(&config.datastore_path)
        .set_env_flags(open::NOLOCK)
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database(&config.peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&config.peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    let listener_liveness_allowlist_cidrs = parse_cidrs(&config.listener_liveness_allowlist_cidrs)
        .map_err(CommsInitializationError::InvalidLivenessCidrs)?;

    let builder = builder
        .with_listener_liveness_max_sessions(config.listener_liveness_max_sessions)
        .with_listener_liveness_allowlist_cidrs(listener_liveness_allowlist_cidrs)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_peer_storage(peer_database, Some(file_lock));

    let mut comms = match config.auxiliary_tcp_listener_address {
        Some(ref addr) => builder.with_auxiliary_tcp_listener_address(addr.clone()).build()?,
        None => builder.build()?,
    };

    let peer_manager = comms.peer_manager();
    let connectivity = comms.connectivity();
    let node_identity = comms.node_identity();
    let shutdown_signal = comms.shutdown_signal();
    // Create outbound channel
    let (outbound_tx, outbound_rx) = mpsc::channel(config.dht.outbound_buffer_size);

    let mut dht = Dht::builder();
    dht.with_config(config.dht.clone()).with_outbound_sender(outbound_tx);
    let dht = dht
        .build(node_identity.clone(), peer_manager, connectivity, shutdown_signal)
        .await?;

    let dht_outbound_layer = dht.outbound_middleware_layer();

    // DHT RPC service is only available for communication nodes
    if node_identity.has_peer_features(PeerFeatures::COMMUNICATION_NODE) {
        comms = comms.add_rpc_server(RpcServer::new().add_service(dht.rpc_service()));
    }

    // Hook up DHT messaging middlewares
    let messaging_pipeline = pipeline::Builder::new()
        .with_outbound_pipeline(outbound_rx, |sink| {
            ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
        })
        .max_concurrent_inbound_tasks(config.max_concurrent_inbound_tasks)
        .max_concurrent_outbound_tasks(config.max_concurrent_outbound_tasks)
        .with_inbound_pipeline(
            ServiceBuilder::new()
                .layer(dht.inbound_middleware_layer())
                .service(connector),
        )
        .build();

    let (messaging_events_sender, _) = broadcast::channel(1);
    comms = comms.add_protocol_extension(
        MessagingProtocolExtension::new(
            MESSAGING_PROTOCOL_ID.clone(),
            messaging_events_sender,
            messaging_pipeline,
        )
        .with_ban_duration(config.dht.ban_duration_short),
    );

    Ok((comms, dht))
}

/// Acquire an exclusive OS level write lock on a file in the provided path. This is used to check if another instance
/// of this database has already been initialized in order to prevent two process from using it simultaneously
/// ## Parameters
/// `db_path` - Path where the db will be initialized
///
/// ## Returns
/// Returns a File handle that must be retained to keep the file lock active.
fn acquire_exclusive_file_lock(db_path: &Path) -> Result<File, CommsInitializationError> {
    let lock_file_path = db_path.join(".p2p_file.lock");

    if let Some(parent) = lock_file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(lock_file_path)?;
    // Attempt to acquire exclusive OS level Write Lock
    if let Err(e) = file.try_lock_exclusive() {
        error!(
            target: LOG_TARGET,
            "Could not acquire exclusive write lock on database lock file: {:?}", e
        );
        return Err(CommsInitializationError::CannotAcquireFileLock);
    }

    Ok(file)
}

/// Adds a new peer to the base node
/// ## Parameters
/// `comms_node` - A reference to the comms node. This is the communications stack
/// `peers` - A list of peers to be added to the comms node, the current node identity of the comms stack is excluded if
/// found in the list.
///
/// ## Returns
/// A Result to determine if the call was successful or not, string will indicate the reason on error
pub async fn add_seed_peers(
    peer_manager: &PeerManager,
    node_identity: &NodeIdentity,
    peers: Vec<Peer>,
) -> Result<(), CommsInitializationError> {
    for mut peer in peers {
        if &peer.public_key == node_identity.public_key() {
            debug!(
                target: LOG_TARGET,
                "Attempting to add yourself [{}] as a seed peer to comms layer, ignoring request", peer
            );
            continue;
        }
        peer.add_flags(PeerFlags::SEED);

        debug!(target: LOG_TARGET, "Adding seed peer [{}]", peer);
        peer_manager
            .add_peer(peer)
            .await
            .map_err(CommsInitializationError::FailedToAddSeedPeer)?;
    }
    Ok(())
}

pub struct P2pInitializer {
    config: P2pConfig,
    user_agent: String,
    seed_config: PeerSeedsConfig,
    network: Network,
    node_identity: Arc<NodeIdentity>,
    connector: Option<PubsubDomainConnector>,
}

impl P2pInitializer {
    pub fn new(
        config: P2pConfig,
        user_agent: String,
        seed_config: PeerSeedsConfig,
        network: Network,
        node_identity: Arc<NodeIdentity>,
        connector: PubsubDomainConnector,
    ) -> Self {
        Self {
            config,
            user_agent,
            seed_config,
            network,
            node_identity,
            connector: Some(connector),
        }
    }

    // Following are inlined due to Rust ICE: https://github.com/rust-lang/rust/issues/73537
    fn try_parse_seed_peers(peer_seeds_str: &[String]) -> Result<Vec<Peer>, ServiceInitializationError> {
        peer_seeds_str
            .iter()
            .map(|s| SeedPeer::from_str(s))
            .map(|r| r.map(Peer::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    async fn try_resolve_dns_seeds(config: &PeerSeedsConfig) -> Result<Vec<Peer>, ServiceInitializationError> {
        if config.dns_seeds.is_empty() {
            debug!(target: LOG_TARGET, "No DNS Seeds configured");
            return Ok(Vec::new());
        }

        debug!(
            target: LOG_TARGET,
            "Resolving DNS seeds (NS:{}, addresses: {})...",
            config.dns_seeds_name_server,
            config
                .dns_seeds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join(",")
        );
        let start = Instant::now();

        let resolver = if config.dns_seeds_use_dnssec {
            debug!(
                target: LOG_TARGET,
                "Using {} to resolve DNS seeds. DNSSEC is enabled", config.dns_seeds_name_server
            );
            DnsSeedResolver::connect_secure(config.dns_seeds_name_server.clone()).await?
        } else {
            debug!(
                target: LOG_TARGET,
                "Using {} to resolve DNS seeds. DNSSEC is disabled", config.dns_seeds_name_server
            );
            DnsSeedResolver::connect(config.dns_seeds_name_server.clone()).await?
        };
        let resolving = config.dns_seeds.iter().map(|addr| {
            let mut resolver = resolver.clone();
            async move { (resolver.resolve(addr).await, addr) }
        });

        let peers = future::join_all(resolving)
            .await
            .into_iter()
            // Log and ignore errors
            .filter_map(|(result, addr)| match result {
                Ok(peers) => {
                    debug!(
                        target: LOG_TARGET,
                        "Found {} peer(s) from `{}` in {:.0?}",
                        peers.len(),
                        addr,
                        start.elapsed()
                    );
                    Some(peers)
                },
                Err(err) => {
                    warn!(target: LOG_TARGET, "DNS seed `{}` failed to resolve: {}", addr, err);
                    None
                },
            })
            .flatten()
            .map(Into::into)
            .collect::<Vec<_>>();

        Ok(peers)
    }
}

#[async_trait]
impl ServiceInitializer for P2pInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing P2P");
        let mut config = self.config.clone();
        let connector = self.connector.take().expect("P2pInitializer called more than once");

        let mut builder = CommsBuilder::new()
            .with_shutdown_signal(context.get_shutdown_signal())
            .with_node_identity(self.node_identity.clone())
            .with_node_info(NodeNetworkInfo {
                major_version: MAJOR_NETWORK_VERSION,
                minor_version: MINOR_NETWORK_VERSION,
                network_wire_byte: get_network_wire_byte(self.network)?,
                user_agent: self.user_agent.clone(),
            })
            .with_minimize_connections(if self.config.dht.minimize_connections {
                Some(self.config.dht.num_neighbouring_nodes + self.config.dht.num_random_nodes)
            } else {
                None
            })
            .set_self_liveness_check(config.listener_self_liveness_check_interval);

        if config.allow_test_addresses || config.dht.peer_validator_config.allow_test_addresses {
            // The default is false, so ensure that both settings are true in this case
            config.allow_test_addresses = true;
            builder = builder.allow_test_addresses();
            config.dht.peer_validator_config = builder.peer_validator_config().clone();
        }

        let (comms, dht) = configure_comms_and_dht(builder, &config, connector).await?;

        let peer_manager = comms.peer_manager();
        let node_identity = comms.node_identity();

        let peers = match Self::try_resolve_dns_seeds(&self.seed_config).await {
            Ok(peers) => peers,
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to resolve DNS seeds: {}", err);
                Vec::new()
            },
        };
        add_seed_peers(&peer_manager, &node_identity, peers).await?;

        let peers = Self::try_parse_seed_peers(&self.seed_config.peer_seeds)?;

        add_seed_peers(&peer_manager, &node_identity, peers).await?;

        context.register_handle(comms.connectivity());
        context.register_handle(peer_manager);
        context.register_handle(comms);
        context.register_handle(dht);
        debug!(target: LOG_TARGET, "P2P Initialized");
        Ok(())
    }
}
