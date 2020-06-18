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

use crate::{
    comms_connector::{InboundDomainConnector, PeerMessage},
    transport::{TorConfig, TransportType},
};
use derive_error::Error;
use futures::{channel::mpsc, AsyncRead, AsyncWrite, Sink};
use log::*;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use std::{error::Error, iter, path::PathBuf, sync::Arc, time::Duration};
use tari_comms::{
    backoff::ConstantBackoff,
    peer_manager::{NodeIdentity, Peer, PeerManagerError},
    pipeline,
    pipeline::SinkService,
    protocol::Protocols,
    tor,
    transports::{MemoryTransport, SocksTransport, TcpWithTorTransport, Transport},
    utils::cidr::parse_cidrs,
    CommsBuilder,
    CommsBuilderError,
    CommsNode,
    PeerManager,
    Substream,
};
use tari_comms_dht::{Dht, DhtBuilder, DhtConfig, DhtInitializationError};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tower::ServiceBuilder;

const LOG_TARGET: &str = "p2p::initialization";

#[derive(Debug, Error)]
pub enum CommsInitializationError {
    CommsBuilderError(CommsBuilderError),
    DhtInitializationError(DhtInitializationError),
    HiddenServiceBuilderError(tor::HiddenServiceBuilderError),
    #[error(non_std, no_from, msg_embedded)]
    InvalidLivenessCidrs(String),
    /// Could not add seed peers to comms layer
    FailedToAddSeedPeer(PeerManagerError),
}

impl CommsInitializationError {
    pub fn to_friendly_string(&self) -> String {
        // Add any helpful user-facing messages here
        match self {
            CommsInitializationError::HiddenServiceBuilderError(
                tor::HiddenServiceBuilderError::HiddenServiceControllerError(
                    tor::HiddenServiceControllerError::TorControlPortOffline,
                ),
            ) => r#"Unable to connect to the Tor control port. 
Please check that you have the Tor proxy running and that access to the Tor control port is turned on.
If you are unsure of what to do, use the following command to start the Tor proxy:
tor --allow-missing-torrc --ignore-missing-torrc --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 --log "notice stdout" --clientuseipv6 1"#
                .to_string(),
            err => format!("Failed to initialize comms: {:?}", err),
        }
    }
}

/// Configuration for a comms node
#[derive(Clone)]
pub struct CommsConfig {
    /// Path to the LMDB data files.
    pub datastore_path: PathBuf,
    /// Name to use for the peer database
    pub peer_database_name: String,
    /// The maximum number of concurrent Inbound tasks allowed before back-pressure is applied to peers
    pub max_concurrent_inbound_tasks: usize,
    /// The size of the buffer (channel) which holds pending outbound message requests
    pub outbound_buffer_size: usize,
    /// Configuration for DHT
    pub dht: DhtConfig,
    /// The identity of this node on the network
    pub node_identity: Arc<NodeIdentity>,
    /// The type of transport to use
    pub transport_type: TransportType,
    /// Set to true to allow peers to provide test addresses (loopback, memory etc.). If set to false, memory
    /// addresses, loopback, local-link (i.e addresses used in local tests) will not be accepted from peers. This
    /// should always be false for non-test nodes.
    pub allow_test_addresses: bool,
    /// The maximum number of liveness sessions allowed for the connection listener.
    /// Liveness sessions can be used by third party tooling to determine node liveness.
    /// A value of 0 will disallow any liveness sessions.
    pub listener_liveness_max_sessions: usize,
    /// CIDR for addresses allowed to enter into liveness check mode on the listener.
    pub listener_liveness_allowlist_cidrs: Vec<String>,
}

/// Initialize Tari Comms configured for tests
pub async fn initialize_local_test_comms<TSink>(
    node_identity: Arc<NodeIdentity>,
    connector: InboundDomainConnector<TSink>,
    data_path: &str,
    discovery_request_timeout: Duration,
    seed_peers: Vec<Peer>,
) -> Result<(CommsNode, Dht), CommsInitializationError>
where
    TSink: Sink<Arc<PeerMessage>> + Unpin + Clone + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let peer_database_name = {
        let mut rng = thread_rng();
        iter::repeat(())
            .map(|_| rng.sample(Alphanumeric))
            .take(8)
            .collect::<String>()
    };
    std::fs::create_dir_all(data_path).unwrap();
    let datastore = LMDBBuilder::new()
        .set_path(data_path)
        .set_environment_size(50)
        .set_max_number_of_databases(1)
        .add_database(&peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    //---------------------------------- Comms --------------------------------------------//

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        .with_transport(MemoryTransport)
        .with_listener_address(node_identity.public_address())
        .with_listener_liveness_max_sessions(1)
        .with_node_identity(node_identity)
        .with_peer_storage(peer_database)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_min_connectivity(1.0)
        .build()?;

    add_peers_to_comms(&comms.peer_manager, &comms.node_identity, seed_peers).await?;

    // Create outbound channel
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let dht = DhtBuilder::new(
        comms.node_identity(),
        comms.peer_manager(),
        outbound_tx,
        comms.connectivity(),
        comms.shutdown_signal(),
    )
    .local_test()
    .with_discovery_timeout(discovery_request_timeout)
    .finish()
    .await?;

    let dht_outbound_layer = dht.outbound_middleware_layer();

    let comms = comms
        .with_messaging_pipeline(
            pipeline::Builder::new()
                .outbound_buffer_size(10)
                .with_outbound_pipeline(outbound_rx, |sink| {
                    ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
                })
                .max_concurrent_inbound_tasks(10)
                .with_inbound_pipeline(
                    ServiceBuilder::new()
                        .layer(dht.inbound_middleware_layer())
                        .service(SinkService::new(connector)),
                )
                .finish(),
        )
        .spawn()
        .await?;

    Ok((comms, dht))
}

/// Initialize Tari Comms
pub async fn initialize_comms<TSink>(
    config: CommsConfig,
    connector: InboundDomainConnector<TSink>,
    seed_peers: Vec<Peer>,
    protocols: Protocols<Substream>,
) -> Result<(CommsNode, Dht), CommsInitializationError>
where
    TSink: Sink<Arc<PeerMessage>> + Unpin + Clone + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let mut builder = CommsBuilder::new().with_node_identity(config.node_identity.clone());

    if config.allow_test_addresses {
        builder = builder.allow_test_addresses();
    }

    match &config.transport_type {
        TransportType::Memory { listener_address } => {
            debug!(target: LOG_TARGET, "Building in-memory comms stack");
            let comms = builder
                .with_protocols(protocols)
                .with_transport(MemoryTransport)
                .with_listener_address(listener_address.clone());
            configure_comms_and_dht(comms, config, connector, seed_peers).await
        },
        TransportType::Tcp {
            listener_address,
            tor_socks_config,
        } => {
            debug!(
                target: LOG_TARGET,
                "Building TCP comms stack{}",
                tor_socks_config.as_ref().map(|_| " with Tor support").unwrap_or("")
            );
            let mut transport = TcpWithTorTransport::new();
            if let Some(config) = tor_socks_config {
                transport.set_tor_socks_proxy(config.clone());
            }
            let comms = builder
                .with_transport(transport)
                .with_listener_address(listener_address.clone());
            configure_comms_and_dht(comms, config, connector, seed_peers).await
        },
        TransportType::Tor(tor_config) => {
            debug!(target: LOG_TARGET, "Building TOR comms stack ({})", tor_config);
            let hidden_service = initialize_hidden_service(tor_config.clone()).await?;
            debug!(
                target: LOG_TARGET,
                "Created hidden service {}",
                hidden_service.get_onion_address()
            );
            let comms = builder.configure_from_hidden_service(hidden_service);
            debug!(target: LOG_TARGET, "Comms stack configured");

            let (comms, dht) = configure_comms_and_dht(comms, config, connector, seed_peers).await?;
            debug!(target: LOG_TARGET, "DHT configured");
            // Set the public address to the onion address that comms is using
            comms.node_identity().set_public_address(
                comms
                    .hidden_service()
                    .expect("hidden_service must be set because a tor hidden service is set")
                    .get_onion_address(),
            );
            Ok((comms, dht))
        },
        TransportType::Socks {
            socks_config,
            listener_address,
        } => {
            debug!(target: LOG_TARGET, "Building SOCKS5 comms stack");
            let comms = builder
                .with_transport(SocksTransport::new(socks_config.clone()))
                .with_listener_address(listener_address.clone());
            configure_comms_and_dht(comms, config, connector, seed_peers).await
        },
    }
}

async fn initialize_hidden_service(config: TorConfig) -> Result<tor::HiddenService, tor::HiddenServiceBuilderError> {
    let mut builder = tor::HiddenServiceBuilder::new()
        .with_hs_flags(tor::HsFlags::DETACH)
        .with_port_mapping(config.port_mapping)
        .with_socks_address_override(config.socks_address_override)
        .with_socks_authentication(config.socks_auth)
        .with_control_server_auth(config.control_server_auth)
        .with_control_server_address(config.control_server_addr);

    if let Some(identity) = config.identity {
        builder = builder.with_tor_identity(*identity);
    }

    builder.finish().await
}

async fn configure_comms_and_dht<TTransport, TSink>(
    builder: CommsBuilder<TTransport>,
    config: CommsConfig,
    connector: InboundDomainConnector<TSink>,
    seed_peers: Vec<Peer>,
) -> Result<(CommsNode, Dht), CommsInitializationError>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TSink: Sink<Arc<PeerMessage>> + Unpin + Clone + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let datastore = LMDBBuilder::new()
        .set_path(&config.datastore_path)
        .set_environment_size(50)
        .set_max_number_of_databases(1)
        .add_database(&config.peer_database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();
    let peer_database = datastore.get_handle(&config.peer_database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));

    let listener_liveness_allowlist_cidrs = parse_cidrs(&config.listener_liveness_allowlist_cidrs)
        .map_err(CommsInitializationError::InvalidLivenessCidrs)?;

    let comms = builder
        .with_listener_liveness_max_sessions(config.listener_liveness_max_sessions)
        .with_listener_liveness_allowlist_cidrs(listener_liveness_allowlist_cidrs)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(500)))
        .with_peer_storage(peer_database)
        .build()?;

    add_peers_to_comms(&comms.peer_manager, &comms.node_identity, seed_peers).await?;

    // Create outbound channel
    let (outbound_tx, outbound_rx) = mpsc::channel(config.outbound_buffer_size);

    let dht = DhtBuilder::new(
        comms.node_identity(),
        comms.peer_manager(),
        outbound_tx,
        comms.connectivity(),
        comms.shutdown_signal(),
    )
    .with_config(config.dht)
    .finish()
    .await?;

    let dht_outbound_layer = dht.outbound_middleware_layer();

    let comms = comms
        .with_messaging_pipeline(
            pipeline::Builder::new()
                .outbound_buffer_size(config.outbound_buffer_size)
                .with_outbound_pipeline(outbound_rx, |sink| {
                    ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
                })
                .max_concurrent_inbound_tasks(config.max_concurrent_inbound_tasks)
                .with_inbound_pipeline(
                    ServiceBuilder::new()
                        .layer(dht.inbound_middleware_layer())
                        .service(SinkService::new(connector)),
                )
                .finish(),
        )
        .spawn()
        .await?;

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
async fn add_peers_to_comms(
    peer_manager: &PeerManager,
    node_identity: &NodeIdentity,
    peers: Vec<Peer>,
) -> Result<(), CommsInitializationError>
{
    for peer in peers {
        let peer_desc = peer.to_string();
        debug!(target: LOG_TARGET, "Adding seed peer [{}]", peer);

        if &peer.public_key == node_identity.public_key() {
            debug!(
                target: LOG_TARGET,
                "Attempting to add yourself [{}] as a seed peer to comms layer, ignoring request", peer_desc
            );
            continue;
        }

        peer_manager
            .add_peer(peer)
            .await
            .map_err(CommsInitializationError::FailedToAddSeedPeer)?;
    }
    Ok(())
}
