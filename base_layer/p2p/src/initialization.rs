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
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use futures::future;
use log::*;
use serde::Deserialize;
use serde_json;
use tari_common::{
    configuration::{DnsNameServerList, Network},
    exit_codes::{ExitCode, ExitError},
    DnsNameServer,
};
use tari_common_sqlite::{
    connection::{DbConnection, DbConnectionUrl},
    error::StorageError,
};
use tari_comms::{
    backoff::ConstantBackoff,
    multiaddr::multiaddr,
    peer_manager::{
        database::{PeerDatabaseSql, MIGRATIONS},
        NodeIdentity,
        Peer,
        PeerFeatures,
        PeerFlags,
        PeerManagerError,
    },
    pipeline,
    protocol::{
        messaging::{MessagingEventSender, MessagingProtocolExtension},
        rpc::RpcServer,
        NodeNetworkInfo,
        ProtocolId,
    },
    tor::{self, HiddenServiceControllerError, TorIdentity},
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
use tari_utilities::hex::Hex;
use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc},
    time::timeout,
};
use tower::ServiceBuilder;

use crate::{
    comms_connector::{InboundDomainConnector, PubsubDomainConnector},
    config::{P2pConfig, PeerSeedsConfig},
    dns::DnsClientError,
    peer_seeds::{DnsSeedResolver, SeedPeer},
    signature_verification::verify_signed_file,
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
    #[error("StorageError `{0}`")]
    StorageError(#[from] StorageError),
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
) -> Result<(UnspawnedCommsNode, Dht, MessagingEventSender), CommsInitializationError>
where
    PathBuf: From<P>,
{
    fs::create_dir_all(&data_path)?;
    let database_url = DbConnectionUrl::File(PathBuf::from(data_path).join("peers.db"));
    debug!(target: LOG_TARGET, "initialize_local_test_comms - node_identity: {}, database URL: {}", node_identity.node_id().to_hex(), database_url.to_url_string());
    let db_connection = DbConnection::connect_and_migrate(&database_url, MIGRATIONS, Some(5))?;
    let peer_database = PeerDatabaseSql::new(db_connection, &node_identity.to_peer())?;

    //---------------------------------- Comms --------------------------------------------//

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        .with_listener_address(node_identity.first_public_address().unwrap())
        .with_listener_liveness_max_sessions(1)
        .with_node_identity(node_identity)
        .with_user_agent(&"/test/1.0")
        .with_peer_storage(peer_database)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_min_connectivity(1)
        .with_network_byte(Network::LocalNet.as_byte())
        .with_shutdown_signal(shutdown_signal)
        .build()?;

    add_seed_peers(&comms.peer_manager(), &comms.node_identity(), seed_peers).await?;

    // Create outbound channel
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();

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
            debug!(target: LOG_TARGET, "Building TOR comms stack ({tor_config:?})");
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
    let database_url = DbConnectionUrl::File(
        PathBuf::from(&config.datastore_path)
            .join(&config.peer_database_name)
            .with_extension("db"),
    );
    let db_connection = DbConnection::connect_and_migrate(&database_url, MIGRATIONS, Some(16))?;
    let this_node = builder
        .node_identity()
        .as_deref()
        .ok_or(CommsBuilderError::NodeIdentityNotSet)?
        .to_peer();
    let peer_database = PeerDatabaseSql::new(db_connection, &this_node)?;

    let listener_liveness_allowlist_cidrs = parse_cidrs(&config.listener_liveness_allowlist_cidrs)
        .map_err(CommsInitializationError::InvalidLivenessCidrs)?;

    let builder = builder
        .with_listener_liveness_max_sessions(config.listener_liveness_max_sessions)
        .with_listener_liveness_allowlist_cidrs(listener_liveness_allowlist_cidrs)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_transport_protocols(config.transport.transport_type.get_supported_protocols())
        .with_peer_storage(peer_database)
        .with_excluded_dial_addresses(config.dht.excluded_dial_addresses.clone().into_vec());

    let mut comms = match config.auxiliary_tcp_listener_address {
        Some(ref addr) => builder.with_auxiliary_tcp_listener_address(addr.clone()).build()?,
        None => builder.build()?,
    };

    let peer_manager = comms.peer_manager();
    let connectivity = comms.connectivity();
    let node_identity = comms.node_identity();
    let shutdown_signal = comms.shutdown_signal();
    // Create outbound channel
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();

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
                "Attempting to add yourself [{peer}] as a seed peer to comms layer, ignoring request"
            );
            continue;
        }
        peer.add_flags(PeerFlags::SEED);

        debug!(target: LOG_TARGET, "Adding seed peer [{peer}]");
        peer_manager
            .add_or_update_peer(peer)
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
    }

    async fn get_url_from_dns(resolver: &mut DnsSeedResolver, addr: &str) -> Result<(String, String), DnsClientError> {
        let timer = Instant::now();
        let download_url_res = match timeout(Duration::from_secs(5), resolver.resolve_download_url(addr)).await {
            Ok(res) => res,
            Err(_) => {
                warn!(target: LOG_TARGET, "Timeout resolving DNS download URL `{addr}`");
                Err(DnsClientError::Timeout)
            },
        }?;
        let res = (download_url_res, addr.to_string());
        info!(target: LOG_TARGET, "Resolved DNS download URL `{}` in {:.0?}", addr, timer.elapsed());
        Ok(res)
    }

    /// downloads seed peers files - json with peers and .asc for verification
    async fn download_seed_peers_files(
        (url, addr): (String, String),
    ) -> Result<Vec<SeedPeer>, ServiceInitializationError> {
        #[derive(Deserialize)]
        struct SeedNodesJson {
            peer_seeds: Vec<String>,
        }

        let timer = Instant::now();

        let content = verify_signed_file(&url, &format!("{}.asc", url)).await.map_err(|e| {
            warn!(target: LOG_TARGET, "Failed to verify seed nodes file from {}: {}", url, e);
            anyhow!("Signature verification failed: {}", e)
        })?;

        let seed_nodes: SeedNodesJson = serde_json::from_str(&content).map_err(|e: serde_json::Error| {
            warn!(target: LOG_TARGET, "Failed to parse seed nodes JSON from {}: {}", url, e);
            anyhow!("Invalid JSON: {}", e)
        })?;

        let mut peers = Vec::new();
        for peer_str in seed_nodes.peer_seeds {
            match peer_str.parse::<SeedPeer>() {
                Ok(peer) => peers.push(peer),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to parse peer '{}': {}", peer_str, e);
                    // Continue with other peers even if one fails
                },
            }
        }

        info!(
            target: LOG_TARGET,
            "Downloaded and verified {} seed peers from {} in {:.0?}",
            peers.len(),
            addr,
            timer.elapsed()
        );

        Ok(peers)
    }

    async fn resolve_http_download_seeds(config: &PeerSeedsConfig) -> Result<Vec<Peer>, ServiceInitializationError> {
        if config.dns_seeds.is_empty() {
            debug!(target: LOG_TARGET, "No DNS Seeds configured");
            return Ok(Vec::new());
        }

        debug!(
            target: LOG_TARGET,
            "Resolving DNS seeds (DNSSEC is enabled: {}, name servers: {}, addresses: {}) ...",
            config.dns_seeds_use_dnssec,
            config.dns_seed_name_servers,
            config
                .dns_seeds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join(",")
        );
        let start = Instant::now();

        let resolver =
            P2pInitializer::get_dns_seed_resolver(config.dns_seeds_use_dnssec, &config.dns_seed_name_servers).await?;

        // First, resolve all DNS records to get download URLs
        let resolving = config.dns_seeds.iter().map(|addr| {
            let mut resolver = resolver.clone();
            let addr = addr.clone();
            async move { P2pInitializer::get_url_from_dns(&mut resolver, &addr).await }
        });

        let resolved_urls: Vec<(String, String)> = future::join_all(resolving)
            .await
            .into_iter()
            .filter_map(|result| match result {
                Ok(url_pair) => Some(url_pair),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to resolve DNS seed: {}", e);
                    None
                },
            })
            .collect();

        // Download and verify seed peer files
        let downloading = resolved_urls
            .into_iter()
            .map(|url_pair| async move { P2pInitializer::download_seed_peers_files(url_pair).await });

        let seed_peers = future::join_all(downloading).await;
        if seed_peers.iter().all(|downlaod_res| downlaod_res.is_err()) {
            return Err(anyhow!("Failed to download and verify seed peer files"));
        }

        let all_seed_peers: Vec<SeedPeer> = seed_peers
            .into_iter()
            .filter_map(|result| match result {
                Ok(peers) => Some(peers),
                Err(e) => {
                    warn!(target: LOG_TARGET, "Failed to download/verify seed peers: {}", e);
                    None
                },
            })
            .flatten()
            .collect();

        let peers: Vec<Peer> = all_seed_peers.into_iter().map(Peer::from).collect();
        info!(
            target: LOG_TARGET,
            "Resolved {} seed peers from DNS in {:.0?}",
            peers.len(),
            start.elapsed()
        );

        Ok(peers)
    }

    async fn try_resolve_dns_seeds(config: &PeerSeedsConfig) -> Result<Vec<Peer>, ServiceInitializationError> {
        if config.dns_seeds.is_empty() {
            debug!(target: LOG_TARGET, "No DNS Seeds configured");
            return Ok(Vec::new());
        }

        debug!(
            target: LOG_TARGET,
            "Resolving DNS seeds (DNSSEC is enabled: {}, name servers: {}, addresses: {}) ...",
            config.dns_seeds_use_dnssec,
            config.dns_seed_name_servers,
            config
                .dns_seeds
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join(",")
        );
        let start = Instant::now();

        let resolver =
            P2pInitializer::get_dns_seed_resolver(config.dns_seeds_use_dnssec, &config.dns_seed_name_servers).await?;
        let resolving = config.dns_seeds.iter().map(|addr| {
            let mut resolver = resolver.clone();
            async move {
                let timer = Instant::now();
                let seeds_res = match timeout(Duration::from_secs(5), resolver.resolve(addr)).await {
                    Ok(res) => res,
                    Err(_) => {
                        warn!(target: LOG_TARGET, "Timeout resolving DNS seed `{addr}`");
                        Err(DnsClientError::Timeout)
                    },
                };
                // let res = (resolver.resolve(addr).await, addr);
                let res = (seeds_res, addr.clone());
                info!(target: LOG_TARGET, "Resolved DNS seed `{}` in {:.0?}", addr, timer.elapsed());
                res
            }
        });

        let peers = future::join_all(resolving)
                .await
                .into_iter()
                // Log and ignore errors
                .filter_map(|(result, addr)| match result {
                    Ok(peers) => {
                        info!(
                            target: LOG_TARGET,
                            "Found {} peer(s) from `{}` in {:.0?}",
                            peers.len(),
                            addr,
                            start.elapsed()
                        );
                        Some(peers)
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "DNS seed `{addr}` failed to resolve: {err}");
                        None
                    },
                })
                .flatten()
                .map(Into::into)
                .collect::<Vec<_>>();

        Ok(peers)
    }

    async fn get_dns_seed_resolver(
        dns_seeds_use_dnssec: bool,
        dns_seed_name_servers: &DnsNameServerList,
    ) -> Result<DnsSeedResolver, ServiceInitializationError> {
        if dns_seed_name_servers.is_empty() {
            return Err(ServiceInitializationError::from(DnsClientError::Connection(
                "No DNS name servers configured!".to_string(),
            )));
        }
        let mut dns_errors = Vec::new();
        for dns in dns_seed_name_servers {
            info!(target: LOG_TARGET, "Connecting to DNS name server: {dns}");
            let res = match (dns_seeds_use_dnssec, dns == &DnsNameServer::System) {
                (true, false) => DnsSeedResolver::connect_secure(dns.clone()),
                (_, _) => DnsSeedResolver::connect(dns.clone()),
            };
            match res {
                Ok(resolver) => return Ok(resolver),
                Err(err) => {
                    warn!(target: LOG_TARGET, "Failed to connect to DNS name server: {err}");
                    dns_errors.push(err.to_string())
                },
            }
        }
        Err(ServiceInitializationError::from(DnsClientError::Connection(format!(
            "{dns_errors:?}"
        ))))
    }
}

#[async_trait]
impl ServiceInitializer for P2pInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        info!(target: LOG_TARGET, "Initializing P2P");
        let mut config = self.config.clone();
        let connector = self.connector.take().expect("P2pInitializer called more than once");

        let mut builder = CommsBuilder::new()
            .with_shutdown_signal(context.get_shutdown_signal())
            .with_node_identity(self.node_identity.clone())
            .with_node_info(NodeNetworkInfo {
                major_version: MAJOR_NETWORK_VERSION,
                minor_version: MINOR_NETWORK_VERSION,
                network_wire_byte: self.network.as_wire_byte(),
                user_agent: self.user_agent.clone(),
            })
            .with_peer_validator_config(config.dht.peer_validator_config.clone())
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

        let peers = match Self::resolve_http_download_seeds(&self.seed_config).await {
            Ok(peers) => peers,
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to resolve seeds through HTTP, fallback to DNS: {err}");
                Self::try_resolve_dns_seeds(&self.seed_config).await.unwrap_or_default()
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

#[cfg(test)]
mod test {
    use tari_common::configuration::Network;
    use tari_comms::connection_manager::WireMode;

    use super::*;

    #[test]
    fn self_liveness_network_wire_byte_is_consistent() {
        let wire_mode = WireMode::Liveness;
        assert_eq!(wire_mode.as_byte(), Network::RESERVED_WIRE_BYTE);
    }

    #[tokio::test]
    async fn test_parse_seed_peers_from_json() {
        // Test JSON content that matches the expected format from cdn-universe.tari.com
        let json_content = r#"{
            "peer_seeds": [
                "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip4/51.83.4.85/tcp/18189",
                "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/ip4/51.83.102.25/tcp/18189",
                "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/ip6/2001:41d0:303:a619::1/tcp/18189",
                "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip6/2001:41d0:303:9a55::1/tcp/18189",
                "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/onion3/tadnxyokalnqjtvu6mlhxndcq4v2tlolotpvrflscdmi7lcautao3had:18141",
                "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/onion3/mhfptgpcj6htjkr5zwurom32wvt7x76ovqzn2ttnwo2bnku6baeaaiyd:18141"
            ]
        }"#;

        // Parse the JSON
        #[derive(serde::Deserialize)]
        struct SeedNodesJson {
            peer_seeds: Vec<String>,
        }

        let seed_nodes: SeedNodesJson = serde_json::from_str(json_content).unwrap();
        assert_eq!(seed_nodes.peer_seeds.len(), 6);

        // Parse each peer string into SeedPeer
        let mut peers = Vec::new();
        for peer_str in seed_nodes.peer_seeds {
            let peer = peer_str.parse::<SeedPeer>().unwrap();
            peers.push(peer);
        }

        assert_eq!(peers.len(), 6);

        // Verify the first peer (IPv4)
        let first_peer = &peers.first().unwrap();
        assert_eq!(
            first_peer.public_key.to_hex(),
            "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409"
        );
        assert_eq!(first_peer.addresses.len(), 1);
        assert_eq!(
            first_peer.addresses.first().unwrap().to_string(),
            "/ip4/51.83.4.85/tcp/18189"
        );

        // Verify an IPv6 peer
        let ipv6_peer = &peers.get(2).unwrap();
        assert_eq!(
            ipv6_peer.public_key.to_hex(),
            "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76"
        );
        assert_eq!(
            ipv6_peer.addresses.first().unwrap().to_string(),
            "/ip6/2001:41d0:303:a619::1/tcp/18189"
        );

        // Verify an onion peer
        let onion_peer = &peers.get(4).unwrap();
        assert_eq!(
            onion_peer.addresses.first().unwrap().to_string(),
            "/onion3/tadnxyokalnqjtvu6mlhxndcq4v2tlolotpvrflscdmi7lcautao3had:18141"
        );
    }

    #[tokio::test]
    async fn test_try_parse_seed_peers() {
        let peer_seeds = vec![
            "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip4/51.83.4.85/tcp/18189".to_string(),
            "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/ip4/51.83.102.25/tcp/18189".to_string(),
        ];

        let peers = P2pInitializer::try_parse_seed_peers(&peer_seeds).unwrap();
        assert_eq!(peers.len(), 2);

        // Verify conversion to Peer works
        let first_peer = &peers.first().unwrap();
        assert_eq!(
            first_peer.public_key.to_hex(),
            "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409"
        );
    }

    #[tokio::test]
    async fn test_parse_invalid_seed_peers() {
        // Test JSON with some invalid peers
        let json_content = r#"{
            "peer_seeds": [
                "4cdfb70e0b38b60c6a3573b2870e32bc3d846419c606ea379f43650b80f38409::/ip4/51.83.4.85/tcp/18189",
                "invalid_public_key::/ip4/1.2.3.4/tcp/12345",
                "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/invalid_address",
                "1e08628960f75b7e324f010b2ee609a9e28097e9101f4d769d474a38b6ee2d76::/ip4/51.83.102.25/tcp/18189"
            ]
        }"#;

        #[derive(serde::Deserialize)]
        struct SeedNodesJson {
            peer_seeds: Vec<String>,
        }

        let seed_nodes: SeedNodesJson = serde_json::from_str(json_content).unwrap();
        assert_eq!(seed_nodes.peer_seeds.len(), 4);

        // Parse peers, skipping invalid ones
        let mut peers = Vec::new();
        let mut invalid_count = 0;
        for peer_str in seed_nodes.peer_seeds {
            match peer_str.parse::<SeedPeer>() {
                Ok(peer) => peers.push(peer),
                Err(_) => invalid_count += 1,
            }
        }

        // Should have 2 valid peers and 2 invalid ones
        assert_eq!(peers.len(), 2);
        assert_eq!(invalid_count, 2);
    }

    #[tokio::test]
    async fn test_signature_verification_with_actual_key() {
        // Test with the actual public key for seed peers HTTP download
        const SEED_PEERS_PUBLIC_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

mDMEaLl/nhYJKwYBBAHaRw8BAQdAocXM74pI54REY9Y0fESxir/iq8We9wp6JHFP
z8vcdm20Sk1hY2llaiAoVGVzdCBmb3Igc2VlZCBwZWVycyBIVFRQIGRvd25sb2Fk
KSA8bWFjaWVqLmtvenVzemVrQHNwYWNlaW5jaC5jb20+iJMEExYKADsWIQTaz8Pe
9KT58ia7xIFrHRtevPqxvwUCaLl/ngIbAwULCQgHAgIiAgYVCgkICwIEFgIDAQIe
BwIXgAAKCRBrHRtevPqxv5cOAQDR1jrEiLxlsEFLsI6DLd0I7SRQDw+tziT/02ed
7E8wMQD/ZzdO7ZO8oLfneJrrwoWiGk241+yq7ym5uEcBuhnKyQ8=
=rjiS
-----END PGP PUBLIC KEY BLOCK-----"#;

        use pgp::{types::PublicKeyTrait, Deserializable};

        // Parse the public key
        let (key, _) = pgp::SignedPublicKey::from_string(SEED_PEERS_PUBLIC_KEY).unwrap();

        // The key should be valid - just verify it can be parsed
        assert!(!key.primary_key.key_id().to_vec().is_empty());
    }

    #[tokio::test]
    async fn test_empty_seed_peers_json() {
        let json_content = r#"{
            "peer_seeds": []
        }"#;

        #[derive(serde::Deserialize)]
        struct SeedNodesJson {
            peer_seeds: Vec<String>,
        }

        let seed_nodes: SeedNodesJson = serde_json::from_str(json_content).unwrap();
        assert_eq!(seed_nodes.peer_seeds.len(), 0);

        let peers: Vec<SeedPeer> = seed_nodes
            .peer_seeds
            .into_iter()
            .filter_map(|s| s.parse::<SeedPeer>().ok())
            .collect();

        assert_eq!(peers.len(), 0);
    }
}
