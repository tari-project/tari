//  Copyright 2020, The Tari Project
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

use anyhow::anyhow;
use log::*;
use std::{cmp, fs, str::FromStr, sync::Arc, time::Duration};
use tari_app_utilities::{identity_management, utilities};
use tari_common::{CommsTransport, GlobalConfig, TorControlAuthentication};
use tari_comms::{
    peer_manager::Peer,
    protocol::rpc::RpcServer,
    socks,
    tor,
    tor::TorIdentity,
    transports::SocksConfig,
    utils::multiaddr::multiaddr_to_socketaddr,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_comms_dht::{DbConnectionUrl, Dht, DhtConfig};
use tari_core::{
    base_node,
    base_node::{
        chain_metadata_service::ChainMetadataServiceInitializer,
        service::{BaseNodeServiceConfig, BaseNodeServiceInitializer},
        state_machine_service::{initializer::BaseNodeStateMachineInitializer, states::HorizonSyncConfig},
        BaseNodeStateMachineConfig,
        BlockSyncConfig,
        StateMachineHandle,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    mempool,
    mempool::{
        service::MempoolHandle,
        Mempool,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        MempoolSyncInitializer,
    },
    transactions::types::CryptoFactories,
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization,
    initialization::{CommsConfig, P2pInitializer},
    seed_peer::SeedPeer,
    services::liveness::{LivenessConfig, LivenessInitializer},
    transport::{TorConfig, TransportType},
};
use tari_service_framework::{ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;
use tokio::runtime;

const LOG_TARGET: &str = "c::bn::initialization";
/// The minimum buffer size for the base node pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;

pub struct BaseNodeBootstrapper<'a, B> {
    pub config: &'a GlobalConfig,
    pub node_identity: Arc<NodeIdentity>,
    pub db: BlockchainDatabase<B>,
    pub mempool: Mempool,
    pub rules: ConsensusManager,
    pub factories: CryptoFactories,
    pub interrupt_signal: ShutdownSignal,
}

impl<B> BaseNodeBootstrapper<'_, B>
where B: BlockchainBackend + 'static
{
    pub async fn bootstrap(self) -> Result<ServiceHandles, anyhow::Error> {
        let config = self.config;

        fs::create_dir_all(&config.peer_db_path)?;

        let buf_size = cmp::max(BASE_NODE_BUFFER_MIN_SIZE, config.buffer_size_base_node);
        let (publisher, peer_message_subscriptions) =
            pubsub_connector(runtime::Handle::current(), buf_size, config.buffer_rate_limit_base_node);
        let peer_message_subscriptions = Arc::new(peer_message_subscriptions);

        let node_config = BaseNodeServiceConfig::default(); // TODO - make this configurable
        let mempool_config = MempoolServiceConfig::default(); // TODO - make this configurable

        let comms_config = self.create_comms_config();
        let transport_type = comms_config.transport_type.clone();

        let sync_peers = config
            .force_sync_peers
            .iter()
            .map(|s| SeedPeer::from_str(s))
            .map(|r| r.map(Peer::from).map(|p| p.node_id))
            .collect::<Result<Vec<_>, _>>()?;

        debug!(target: LOG_TARGET, "{} sync peer(s) configured", sync_peers.len());

        let rules = self.rules.clone();

        let mempool_sync = MempoolSyncInitializer::new(mempool_config, self.mempool.clone());
        let mempool_protocol = mempool_sync.get_protocol_extension();

        let mut handles = StackBuilder::new(self.interrupt_signal)
            .add_initializer(P2pInitializer::new(comms_config, publisher))
            .add_initializer(BaseNodeServiceInitializer::new(
                peer_message_subscriptions.clone(),
                self.db.clone().into(),
                self.mempool.clone(),
                self.rules.clone(),
                node_config,
            ))
            .add_initializer(MempoolServiceInitializer::new(
                mempool_config,
                self.mempool.clone(),
                peer_message_subscriptions.clone(),
            ))
            .add_initializer(mempool_sync)
            .add_initializer(LivenessInitializer::new(
                LivenessConfig {
                    auto_ping_interval: Some(Duration::from_secs(config.auto_ping_interval)),
                    refresh_neighbours_interval: Duration::from_secs(3 * 60),
                    monitored_peers: sync_peers.clone(),
                    ..Default::default()
                },
                peer_message_subscriptions,
            ))
            .add_initializer(ChainMetadataServiceInitializer)
            .add_initializer(BaseNodeStateMachineInitializer::new(
                self.db.clone().into(),
                BaseNodeStateMachineConfig {
                    block_sync_config: BlockSyncConfig {
                        sync_peers,
                        ..Default::default()
                    },
                    horizon_sync_config: HorizonSyncConfig {
                        horizon_sync_height_offset: rules.consensus_constants(0).coinbase_lock_height() + 50,
                        ..Default::default()
                    },
                    pruning_horizon: config.pruning_horizon,
                    orphan_db_clean_out_threshold: config.orphan_db_clean_out_threshold,
                    max_randomx_vms: config.max_randomx_vms,
                    blocks_behind_before_considered_lagging: self.config.blocks_behind_before_considered_lagging,
                    ..Default::default()
                },
                self.rules,
                self.factories,
            ))
            .build()
            .await?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

        let comms = comms.add_protocol_extension(mempool_protocol);
        let comms = Self::setup_rpc_services(comms, &handles, self.db.into());
        let comms = initialization::spawn_comms_using_transport(comms, transport_type).await?;
        // Save final node identity after comms has initialized. This is required because the public_address can be
        // changed by comms during initialization when using tor.
        identity_management::save_as_json(&config.base_node_identity_file, &*comms.node_identity())
            .map_err(|e| anyhow!("Failed to save node identity: {:?}", e))?;
        if let Some(hs) = comms.hidden_service() {
            identity_management::save_as_json(&config.base_node_tor_identity_file, hs.tor_identity())
                .map_err(|e| anyhow!("Failed to save tor identity: {:?}", e))?;
        }

        handles.register(comms);

        Ok(handles)
    }

    fn setup_rpc_services(
        comms: UnspawnedCommsNode,
        handles: &ServiceHandles,
        db: AsyncBlockchainDb<B>,
    ) -> UnspawnedCommsNode
    {
        let dht = handles.expect_handle::<Dht>();

        // Add your RPC services here ‍🏴‍☠️️☮️🌊
        let rpc_server = RpcServer::new()
            .add_service(dht.rpc_service())
            .add_service(base_node::create_base_node_sync_rpc_service(db.clone()))
            .add_service(mempool::create_mempool_rpc_service(
                handles.expect_handle::<MempoolHandle>(),
            ))
            .add_service(base_node::rpc::create_base_node_wallet_rpc_service(
                db,
                handles.expect_handle::<MempoolHandle>(),
                handles.expect_handle::<StateMachineHandle>(),
            ));

        comms.add_protocol_extension(rpc_server)
    }

    fn create_comms_config(&self) -> CommsConfig {
        CommsConfig {
            node_identity: self.node_identity.clone(),
            transport_type: self.create_transport_type(),
            datastore_path: self.config.peer_db_path.clone(),
            peer_database_name: "peers".to_string(),
            max_concurrent_inbound_tasks: 100,
            outbound_buffer_size: 100,
            dht: DhtConfig {
                database_url: DbConnectionUrl::File(self.config.data_dir.join("dht.db")),
                auto_join: true,
                allow_test_addresses: self.config.allow_test_addresses,
                network: self.config.network.into(),
                flood_ban_max_msg_count: self.config.flood_ban_max_msg_count,
                ..Default::default()
            },
            allow_test_addresses: self.config.allow_test_addresses,
            listener_liveness_allowlist_cidrs: self.config.listener_liveness_allowlist_cidrs.clone(),
            listener_liveness_max_sessions: self.config.listnener_liveness_max_sessions,
            user_agent: format!("tari/basenode/{}", env!("CARGO_PKG_VERSION")),
            // Also add sync peers to the peer seed list. Duplicates are acceptable.
            peer_seeds: self
                .config
                .peer_seeds
                .iter()
                .cloned()
                .chain(self.config.force_sync_peers.clone())
                .collect(),
            dns_seeds: self.config.dns_seeds.clone(),
            dns_seeds_name_server: self.config.dns_seeds_name_server,
            dns_seeds_use_dnssec: self.config.dns_seeds_use_dnssec,
        }
    }

    /// Creates a transport type from the given configuration
    ///
    /// ## Paramters
    /// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
    ///
    /// ##Returns
    /// TransportType based on the configuration
    fn create_transport_type(&self) -> TransportType {
        let config = self.config;
        debug!(target: LOG_TARGET, "Transport is set to '{:?}'", config.comms_transport);

        match config.comms_transport.clone() {
            CommsTransport::Tcp {
                listener_address,
                tor_socks_address,
                tor_socks_auth,
            } => TransportType::Tcp {
                listener_address,
                tor_socks_config: tor_socks_address.map(|proxy_address| SocksConfig {
                    proxy_address,
                    authentication: tor_socks_auth
                        .map(utilities::convert_socks_authentication)
                        .unwrap_or_default(),
                }),
            },
            CommsTransport::TorHiddenService {
                control_server_address,
                socks_address_override,
                forward_address,
                auth,
                onion_port,
            } => {
                let identity = Some(&config.base_node_tor_identity_file)
                    .filter(|p| p.exists())
                    .and_then(|p| {
                        // If this fails, we can just use another address
                        identity_management::load_from_json::<_, TorIdentity>(p).ok()
                    });
                info!(
                    target: LOG_TARGET,
                    "Tor identity at path '{}' {:?}",
                    config.base_node_tor_identity_file.to_string_lossy(),
                    identity
                        .as_ref()
                        .map(|ident| format!("loaded for address '{}.onion'", ident.service_id))
                        .or_else(|| Some("not found".to_string()))
                        .unwrap()
                );

                let forward_addr = multiaddr_to_socketaddr(&forward_address).expect("Invalid tor forward address");
                TransportType::Tor(TorConfig {
                    control_server_addr: control_server_address,
                    control_server_auth: {
                        match auth {
                            TorControlAuthentication::None => tor::Authentication::None,
                            TorControlAuthentication::Password(password) => {
                                tor::Authentication::HashedPassword(password)
                            },
                        }
                    },
                    identity: identity.map(Box::new),
                    port_mapping: (onion_port, forward_addr).into(),
                    // TODO: make configurable
                    socks_address_override,
                    socks_auth: socks::Authentication::None,
                })
            },
            CommsTransport::Socks5 {
                proxy_address,
                listener_address,
                auth,
            } => TransportType::Socks {
                socks_config: SocksConfig {
                    proxy_address,
                    authentication: utilities::convert_socks_authentication(auth),
                },
                listener_address,
            },
        }
    }
}
