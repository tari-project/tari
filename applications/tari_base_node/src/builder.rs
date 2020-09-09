// Copyright 2019. The Tari Project
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

use crate::miner;
use futures::future;
use log::*;
use std::{
    fs,
    net::SocketAddr,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tari_app_utilities::{
    identity_management::{load_from_json, save_as_json},
    utilities::{
        create_peer_db_folder,
        create_wallet_folder,
        into_socks_authentication,
        parse_peer_seeds,
        setup_wallet_transport_type,
    },
};
use tari_broadcast_channel::Subscriber;
use tari_common::{CommsTransport, DatabaseType, GlobalConfig, Network, SocksAuthentication, TorControlAuthentication};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    protocol::ProtocolExtensions,
    socks,
    tor,
    tor::TorIdentity,
    transports::SocksConfig,
    utils::multiaddr::multiaddr_to_socketaddr,
    CommsNode,
    ConnectionManagerEvent,
    PeerManager,
};
use tari_comms_dht::{DbConnectionUrl, Dht, DhtConfig};
use tari_core::{
    base_node::{
        chain_metadata_service::{ChainMetadataHandle, ChainMetadataServiceInitializer},
        service::{BaseNodeServiceConfig, BaseNodeServiceInitializer},
        states::StatusInfo,
        BaseNodeStateMachine,
        BaseNodeStateMachineConfig,
        LocalNodeCommsInterface,
        OutboundNodeCommsInterface,
        SyncValidators,
    },
    chain_storage::{
        create_lmdb_database,
        BlockchainBackend,
        BlockchainDatabase,
        BlockchainDatabaseConfig,
        LMDBDatabase,
        MemoryDatabase,
        Validators,
    },
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network as NetworkType},
    mempool::{
        service::LocalMempoolService,
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        MempoolSyncProtocolExtension,
        MempoolValidators,
    },
    mining::{Miner, MinerInstruction},
    tari_utilities::hex::Hex,
    transactions::types::{CryptoFactories, HashDigest, PublicKey},
    validation::{
        accum_difficulty_validators::AccumDifficultyValidator,
        block_validators::{FullConsensusValidator, StatelessBlockValidator},
        transaction_validators::{FullTxValidator, TxInputAndMaturityValidator},
    },
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::{
    comms_connector::{pubsub_connector, PubsubDomainConnector, SubscriptionFactory},
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessConfig, LivenessInitializer},
    },
    transport::{TorConfig, TransportType},
};
use tari_service_framework::{handles::ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::OutputManagerHandle,
        protocols::utxo_validation_protocol::UtxoValidationRetry,
        storage::sqlite_db::OutputManagerSqliteDatabase,
        OutputManagerServiceInitializer,
    },
    storage::sqlite_utilities::{run_migration_and_create_sqlite_connection, WalletDbConnection},
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionServiceHandle,
        storage::sqlite_db::TransactionServiceSqliteDatabase,
        TransactionServiceInitializer,
    },
};
use tokio::{
    runtime,
    stream::StreamExt,
    sync::{broadcast, broadcast::Sender as syncSender},
    task,
    time::delay_for,
};

const LOG_TARGET: &str = "c::bn::initialization";
/// The minimum buffer size for the base node pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;
/// The minimum buffer size for the base node wallet pubsub_connector channel
const BASE_NODE_WALLET_BUFFER_MIN_SIZE: usize = 300;

#[macro_export]
macro_rules! using_backend {
    ($self:expr, $i: ident, $cmd: expr) => {
        match $self {
            NodeContainer::LMDB($i) => $cmd,
            NodeContainer::Memory($i) => $cmd,
        }
    };
}

/// The type of DB is configured dynamically in the config file, but the state machine struct has static dispatch;
/// and so we have to use an enum wrapper to hold the various acceptable types.
pub enum NodeContainer {
    LMDB(BaseNodeContext<LMDBDatabase<HashDigest>>),
    Memory(BaseNodeContext<MemoryDatabase<HashDigest>>),
}

impl NodeContainer {
    /// Starts the node container. This entails starting the miner and wallet (if `mining_enabled` is true) and then
    /// starting the base node state machine. This call consumes the NodeContainer instance.
    pub async fn run(self, rt: runtime::Handle) {
        using_backend!(self, ctx, NodeContainer::run_impl(ctx, rt).await)
    }

    /// Returns a handle to the wallet output manager service. This function panics if it has not been registered
    /// with the comms service
    pub fn output_manager(&self) -> OutputManagerHandle {
        using_backend!(self, ctx, ctx.output_manager())
    }

    /// Returns a handle to the local node communication service. This function panics if it has not been registered
    /// with the comms service
    pub fn local_node(&self) -> LocalNodeCommsInterface {
        using_backend!(self, ctx, ctx.local_node())
    }

    /// Returns a handle to the local mempool service. This function panics if it has not been registered
    /// with the comms service
    pub fn local_mempool(&self) -> LocalMempoolService {
        using_backend!(self, ctx, ctx.local_mempool())
    }

    /// Returns the CommsNode.
    pub fn base_node_comms(&self) -> &CommsNode {
        using_backend!(self, ctx, &ctx.base_node_comms)
    }

    /// Returns the wallet CommsNode.
    pub fn wallet_comms(&self) -> &CommsNode {
        using_backend!(self, ctx, &ctx.wallet_comms)
    }

    /// Returns this node's identity.
    pub fn base_node_identity(&self) -> Arc<NodeIdentity> {
        using_backend!(self, ctx, ctx.base_node_comms.node_identity())
    }

    /// Returns the base node DHT
    pub fn base_node_dht(&self) -> &Dht {
        using_backend!(self, ctx, &ctx.base_node_dht)
    }

    /// Returns this node's wallet identity.
    pub fn wallet_node_identity(&self) -> Arc<NodeIdentity> {
        using_backend!(self, ctx, ctx.wallet_comms.node_identity())
    }

    /// Returns this node's miner enabled flag.
    pub fn miner_enabled(&self) -> Arc<AtomicBool> {
        using_backend!(self, ctx, ctx.miner_enabled.clone())
    }

    /// Returns this node's mining status.
    pub fn mining_status(&self) -> Arc<AtomicBool> {
        using_backend!(self, ctx, ctx.mining_status.clone())
    }

    /// Returns this node's miner atomic hash rate.
    pub fn miner_hashrate(&self) -> Arc<AtomicU64> {
        using_backend!(self, ctx, ctx.miner_hashrate.clone())
    }

    /// Returns this node's miner instruction event channel.
    pub fn miner_instruction_events(&self) -> syncSender<MinerInstruction> {
        using_backend!(self, ctx, ctx.miner_instruction_events.clone())
    }

    /// Returns a handle to the wallet transaction service. This function panics if it has not been registered
    /// with the comms service
    pub fn wallet_transaction_service(&self) -> TransactionServiceHandle {
        using_backend!(self, ctx, ctx.wallet_transaction_service())
    }

    pub fn get_state_machine_info_channel(&self) -> Subscriber<StatusInfo> {
        using_backend!(self, ctx, ctx.get_status_event_stream())
    }

    async fn run_impl<B: BlockchainBackend + 'static>(mut ctx: BaseNodeContext<B>, rt: runtime::Handle) {
        info!(target: LOG_TARGET, "Tari base node has STARTED");
        let mut wallet_output_handle = ctx.output_manager();
        // Start wallet & miner
        let mut miner = ctx.miner.take().expect("Miner was not constructed");
        let mut rx = miner.get_utxo_receiver_channel();
        rt.spawn(async move {
            info!(target: LOG_TARGET, " ⚒️ Mining wallet ready to receive coins.");
            while let Some(utxo) = rx.next().await {
                match wallet_output_handle.add_output(utxo).await {
                    Ok(_) => {
                        info!(
                            target: LOG_TARGET,
                            "🤑💰🤑 Newly mined coinbase output added to wallet 🤑💰🤑"
                        );
                        // TODO Remove this when the wallet monitors the UTXO's more intelligently
                        let mut oms_handle_clone = wallet_output_handle.clone();
                        tokio::spawn(async move {
                            delay_for(Duration::from_secs(240)).await;
                            let _ = oms_handle_clone.validate_utxos(UtxoValidationRetry::UntilSuccess).await;
                        });
                    },
                    Err(e) => warn!(target: LOG_TARGET, "Error adding output: {}", e),
                }
            }
        });
        rt.spawn(async move {
            info!(target: LOG_TARGET, "⚒️ Starting miner");
            miner.mine().await;
            info!(target: LOG_TARGET, "⚒️ Miner has shutdown");
        });
        info!(target: LOG_TARGET, "Starting main base node event loop");
        ctx.node.run().await;
        info!(target: LOG_TARGET, "Initiating communications stack shutdown");
        future::join(ctx.base_node_comms.shutdown(), ctx.wallet_comms.shutdown()).await;
    }
}

/// The base node context is a container for all the key structural pieces for the base node application, including the
/// communications stack, the node state machine, the miner and handles to the various services that are registered
/// on the comms stack.
///
/// `BaseNodeContext` is not intended to be ever used directly, so is a private struct. It is only ever created in the
/// [NodeContainer] enum, which serves the purpose  of abstracting the specific `BlockchainBackend` instance away
/// from users of the full base node stack.
pub struct BaseNodeContext<B: BlockchainBackend> {
    base_node_comms: CommsNode,
    base_node_dht: Dht,
    wallet_comms: CommsNode,
    base_node_handles: Arc<ServiceHandles>,
    wallet_handles: Arc<ServiceHandles>,
    node: BaseNodeStateMachine<B>,
    miner: Option<Miner>,
    miner_enabled: Arc<AtomicBool>,
    mining_status: Arc<AtomicBool>,
    miner_instruction_events: syncSender<MinerInstruction>,
    pub miner_hashrate: Arc<AtomicU64>,
}

impl<B: BlockchainBackend> BaseNodeContext<B>
where B: 'static
{
    /// Returns a handle to the Output Manager
    pub fn output_manager(&self) -> OutputManagerHandle {
        self.wallet_handles
            .get_handle::<OutputManagerHandle>()
            .expect("Problem getting wallet output manager handle")
    }

    /// Returns the handle to the Comms Interface
    pub fn local_node(&self) -> LocalNodeCommsInterface {
        self.base_node_handles
            .get_handle::<LocalNodeCommsInterface>()
            .expect("Could not get local node interface handle")
    }

    /// Returns the handle to the Mempool
    pub fn local_mempool(&self) -> LocalMempoolService {
        self.base_node_handles
            .get_handle::<LocalMempoolService>()
            .expect("Could not get local mempool interface handle")
    }

    /// Return the handle to the Transaction Service
    pub fn wallet_transaction_service(&self) -> TransactionServiceHandle {
        self.wallet_handles
            .get_handle::<TransactionServiceHandle>()
            .expect("Could not get wallet transaction service handle")
    }

    // /// Return the state machine channel to provide info updates
    pub fn get_status_event_stream(&self) -> Subscriber<StatusInfo> {
        self.node.get_status_event_stream()
    }
}

/// Sets up and initializes the base node, creating the context and database
/// ## Parameters
/// `config` - The configuration for the base node
/// `node_identity` - The node identity information of the base node
/// `wallet_node_identity` - The node identity information of the base node's wallet
/// `interrupt_signal` - The signal used to stop the application
/// ## Returns
/// Result containing the NodeContainer, String will contain the reason on error
pub async fn configure_and_initialize_node(
    config: &GlobalConfig,
    node_identity: Arc<NodeIdentity>,
    wallet_node_identity: Arc<NodeIdentity>,
    interrupt_signal: ShutdownSignal,
) -> Result<NodeContainer, String>
{
    let network = match &config.network {
        Network::MainNet => NetworkType::MainNet,
        Network::Rincewind => NetworkType::Rincewind,
    };
    let result = match &config.db_type {
        DatabaseType::Memory => {
            let backend = MemoryDatabase::<HashDigest>::default();
            let ctx = build_node_context(
                backend,
                network,
                node_identity,
                wallet_node_identity,
                config,
                interrupt_signal,
            )
            .await?;
            NodeContainer::Memory(ctx)
        },
        DatabaseType::LMDB(p) => {
            let backend = create_lmdb_database(&p, config.db_config.clone(), MmrCacheConfig::default())
                .map_err(|e| e.to_string())?;
            let ctx = build_node_context(
                backend,
                network,
                node_identity,
                wallet_node_identity,
                config,
                interrupt_signal,
            )
            .await?;
            NodeContainer::LMDB(ctx)
        },
    };
    Ok(result)
}

/// Constructs the base node context, this includes setting up the consensus manager, mempool, base node, wallet, miner
/// and state machine
/// ## Parameters
/// `backend` - Backend interface
/// `network` - The NetworkType (rincewind, mainnet, local)
/// `base_node_identity` - The node identity information of the base node
/// `wallet_node_identity` - The node identity information of the base node's wallet
/// `config` - The configuration for the base node
/// `interrupt_signal` - The signal used to stop the application
/// ## Returns
/// Result containing the BaseNodeContext, String will contain the reason on error
async fn build_node_context<B>(
    backend: B,
    network: NetworkType,
    base_node_identity: Arc<NodeIdentity>,
    wallet_node_identity: Arc<NodeIdentity>,
    config: &GlobalConfig,
    interrupt_signal: ShutdownSignal,
) -> Result<BaseNodeContext<B>, String>
where
    B: BlockchainBackend + 'static,
{
    //---------------------------------- Blockchain --------------------------------------------//

    let rules = ConsensusManagerBuilder::new(network).build();
    let factories = CryptoFactories::default();
    let validators = Validators::new(
        FullConsensusValidator::new(rules.clone(), factories.clone()),
        StatelessBlockValidator::new(rules.clone(), factories.clone()),
        AccumDifficultyValidator {},
    );
    let db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: config.orphan_storage_capacity,
        pruning_horizon: config.pruning_horizon,
        pruning_interval: config.pruned_mode_cleanup_interval,
    };
    let db = BlockchainDatabase::new(backend, &rules, validators, db_config).map_err(|e| e.to_string())?;
    let mempool_validator =
        MempoolValidators::new(FullTxValidator::new(factories.clone()), TxInputAndMaturityValidator {});
    let mempool = Mempool::new(db.clone(), MempoolConfig::default(), mempool_validator);
    let handle = runtime::Handle::current();

    //---------------------------------- Base Node --------------------------------------------//

    let buf_size = std::cmp::max(BASE_NODE_BUFFER_MIN_SIZE, config.buffer_size_base_node);
    let (publisher, base_node_subscriptions) =
        pubsub_connector(handle.clone(), buf_size, config.buffer_rate_limit_base_node);
    let base_node_subscriptions = Arc::new(base_node_subscriptions);
    create_peer_db_folder(&config.peer_db_path)?;

    let mut protocols = ProtocolExtensions::new();
    protocols.add(MempoolSyncProtocolExtension::new(Default::default(), mempool.clone()));

    let (base_node_comms, base_node_dht) =
        setup_base_node_comms(base_node_identity, config, publisher, protocols).await?;
    base_node_comms
        .peer_manager()
        .add_peer(wallet_node_identity.to_peer())
        .await
        .map_err(|err| err.to_string())?;
    base_node_comms
        .connectivity()
        .add_managed_peers(vec![wallet_node_identity.node_id().clone()])
        .await
        .map_err(|err| err.to_string())?;

    debug!(target: LOG_TARGET, "Registering base node services");
    let base_node_handles = register_base_node_services(
        &base_node_comms,
        &base_node_dht,
        db.clone(),
        base_node_subscriptions.clone(),
        mempool,
        rules.clone(),
    )
    .await;
    debug!(target: LOG_TARGET, "Base node service registration complete.");

    //---------------------------------- Wallet --------------------------------------------//
    let buf_size = std::cmp::max(BASE_NODE_WALLET_BUFFER_MIN_SIZE, config.buffer_size_base_node_wallet);
    let (publisher, wallet_subscriptions) =
        pubsub_connector(handle.clone(), buf_size, config.buffer_rate_limit_base_node_wallet);
    let wallet_subscriptions = Arc::new(wallet_subscriptions);
    create_peer_db_folder(&config.wallet_peer_db_path)?;
    let (wallet_comms, wallet_dht) = setup_wallet_comms(
        wallet_node_identity,
        config,
        publisher,
        base_node_comms.node_identity().to_peer(),
    )
    .await?;
    wallet_comms
        .connectivity()
        .add_managed_peers(vec![base_node_comms.node_identity().node_id().clone()])
        .await
        .map_err(|err| err.to_string())?;

    task::spawn(sync_peers(
        base_node_comms.subscribe_connection_manager_events(),
        base_node_comms.peer_manager(),
        wallet_comms.peer_manager(),
    ));

    create_wallet_folder(
        &config
            .wallet_db_file
            .parent()
            .expect("wallet_db_file cannot be set to a root directory"),
    )?;
    let wallet_conn = run_migration_and_create_sqlite_connection(&config.wallet_db_file)
        .map_err(|e| format!("Could not create wallet: {:?}", e))?;

    let network = match &config.network {
        Network::MainNet => NetworkType::MainNet,
        Network::Rincewind => NetworkType::Rincewind,
    };

    let wallet_handles = register_wallet_services(
        &wallet_comms,
        &wallet_dht,
        &wallet_conn,
        wallet_subscriptions,
        factories.clone(),
        config.base_node_query_timeout,
        config.transaction_broadcast_monitoring_timeout,
        config.transaction_chain_monitoring_timeout,
        config.transaction_direct_send_timeout,
        config.transaction_broadcast_send_timeout,
        network,
    )
    .await;

    // Set the base node for the wallet to the 'local' base node
    let base_node_public_key = base_node_comms.node_identity().public_key().clone();
    wallet_handles
        .get_handle::<TransactionServiceHandle>()
        .expect("TransactionService is not registered")
        .set_base_node_public_key(base_node_public_key.clone())
        .await
        .expect("Problem setting local base node public key for transaction service.");
    let mut oms_handle = wallet_handles
        .get_handle::<OutputManagerHandle>()
        .expect("OutputManagerService is not registered");
    oms_handle
        .set_base_node_public_key(base_node_public_key)
        .await
        .expect("Problem setting local base node public key for output manager service.");
    // Start the Output Manager UTXO Validation
    oms_handle
        .validate_utxos(UtxoValidationRetry::UntilSuccess)
        .await
        .expect("Problem starting the Output Manager Service Utxo Valdation process");

    //---------------------------------- Base Node State Machine --------------------------------------------//
    let outbound_interface = base_node_handles
        .get_handle::<OutboundNodeCommsInterface>()
        .expect("Problem getting node interface handle.");
    let chain_metadata_service = base_node_handles
        .get_handle::<ChainMetadataHandle>()
        .expect("Problem getting chain metadata interface handle.");
    debug!(target: LOG_TARGET, "Creating base node state machine.");
    let mut state_machine_config = BaseNodeStateMachineConfig::default();
    state_machine_config.block_sync_config.sync_strategy = config
        .block_sync_strategy
        .parse()
        .expect("Problem reading block sync strategy from config");

    state_machine_config.horizon_sync_config.horizon_sync_height_offset =
        rules.consensus_constants().coinbase_lock_height() + 50;
    let node_local_interface = base_node_handles
        .get_handle::<LocalNodeCommsInterface>()
        .expect("Problem getting node local interface handle.");
    let sync_validators = SyncValidators::full_consensus(db.clone(), rules.clone(), factories.clone());
    let node = BaseNodeStateMachine::new(
        &db,
        &node_local_interface,
        &outbound_interface,
        base_node_comms.peer_manager(),
        base_node_comms.connectivity(),
        chain_metadata_service.get_event_stream(),
        state_machine_config,
        sync_validators,
        interrupt_signal,
    );

    //---------------------------------- Mining --------------------------------------------//

    let local_mp_interface = base_node_handles
        .get_handle::<LocalMempoolService>()
        .expect("Problem getting mempool interface handle.");
    let node_event_stream = node.get_state_change_event_stream();
    let mempool_event_stream = local_mp_interface.get_mempool_state_event_stream();
    let miner = miner::build_miner(
        &base_node_handles,
        node.get_interrupt_signal(),
        node_event_stream,
        mempool_event_stream,
        rules,
        config.num_mining_threads,
    );
    if config.enable_mining {
        info!(target: LOG_TARGET, "Enabling solo miner");
        miner.enable_mining_flag().store(true, Ordering::Relaxed);
    } else {
        info!(
            target: LOG_TARGET,
            "Mining is disabled in the config file. This node will not mine for Tari unless enabled in the UI"
        );
    };

    let miner_enabled = miner.enable_mining_flag();
    let mining_status = miner.mining_status_flag();
    let miner_hashrate = miner.get_hashrate_u64();
    let miner_instruction_events = miner.get_miner_instruction_events_sender_channel();
    Ok(BaseNodeContext {
        base_node_comms,
        base_node_dht,
        wallet_comms,
        base_node_handles,
        wallet_handles,
        node,
        miner: Some(miner),
        miner_enabled,
        mining_status,
        miner_instruction_events,
        miner_hashrate,
    })
}

/// Asynchronously syncs peers with base node, adding peers if the peer is not already known
/// ## Parameters
/// `events_rx` - The event stream
/// `base_node_peer_manager` - The peer manager for the base node wrapped in an atomic reference counter
/// `wallet_peer_manager` - The peer manager for the base node's wallet wrapped in an atomic reference counter
///
/// ## Returns
/// Nothing is returned
async fn sync_peers(
    mut events_rx: broadcast::Receiver<Arc<ConnectionManagerEvent>>,
    base_node_peer_manager: Arc<PeerManager>,
    wallet_peer_manager: Arc<PeerManager>,
)
{
    while let Some(Ok(event)) = events_rx.next().await {
        if let ConnectionManagerEvent::PeerConnected(conn) = &*event {
            if !wallet_peer_manager.exists_node_id(conn.peer_node_id()).await {
                match base_node_peer_manager.find_by_node_id(conn.peer_node_id()).await {
                    Ok(mut peer) => {
                        peer.unset_id();
                        if let Err(err) = wallet_peer_manager.add_peer(peer).await {
                            warn!(target: LOG_TARGET, "Failed to add peer to wallet: {:?}", err);
                        }
                    },
                    Err(err) => {
                        warn!(target: LOG_TARGET, "Failed to find peer in base node: {:?}", err);
                    },
                }
            }
        }
    }
}

/// Creates a transport type from the given configuration
/// /// ## Paramters
/// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
///
/// ##Returns
/// TransportType based on the configuration
fn setup_transport_type(config: &GlobalConfig) -> TransportType {
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
                authentication: tor_socks_auth.map(into_socks_authentication).unwrap_or_default(),
            }),
        },
        CommsTransport::TorHiddenService {
            control_server_address,
            socks_address_override,
            forward_address,
            auth,
            onion_port,
        } => {
            let tor_identity_path = Path::new(&config.tor_identity_file);
            let identity = if tor_identity_path.exists() {
                // If this fails, we can just use another address
                load_from_json::<_, TorIdentity>(&tor_identity_path).ok()
            } else {
                None
            };
            info!(
                target: LOG_TARGET,
                "Tor identity at path '{}' {:?}",
                tor_identity_path.to_string_lossy(),
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
                        TorControlAuthentication::Password(password) => tor::Authentication::HashedPassword(password),
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
                authentication: into_socks_authentication(auth),
            },
            listener_address,
        },
    }
}

/// Asynchronously initializes comms for the base node
/// ## Parameters
/// `node_identity` - The node identity to initialize the comms stack with, see [NodeIdentity]
/// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
/// `publisher` - The publisher for the publish-subscribe messaging system
/// ## Returns
/// A Result containing the commsnode and dht on success, string will indicate the reason on error
async fn setup_base_node_comms(
    node_identity: Arc<NodeIdentity>,
    config: &GlobalConfig,
    publisher: PubsubDomainConnector,
    protocols: ProtocolExtensions,
) -> Result<(CommsNode, Dht), String>
{
    // Ensure that the node identity has the correct public address
    node_identity.set_public_address(config.public_address.clone());
    let comms_config = CommsConfig {
        node_identity,
        transport_type: setup_transport_type(&config),
        datastore_path: config.peer_db_path.clone(),
        peer_database_name: "peers".to_string(),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        // TODO - make this configurable
        dht: DhtConfig {
            database_url: DbConnectionUrl::File(config.data_dir.join("dht.db")),
            auto_join: true,
            ..Default::default()
        },
        // TODO: This should be false unless testing locally - make this configurable
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: config.listener_liveness_allowlist_cidrs.clone(),
        listener_liveness_max_sessions: config.listnener_liveness_max_sessions,
        user_agent: format!("tari/basenode/{}", env!("CARGO_PKG_VERSION")),
    };

    let seed_peers = parse_peer_seeds(&config.peer_seeds);
    let (comms, dht) = initialize_comms(comms_config, publisher, seed_peers, protocols)
        .await
        .map_err(|e| e.to_friendly_string())?;

    // Save final node identity after comms has initialized. This is required because the public_address can be changed
    // by comms during initialization when using tor.
    save_as_json(&config.identity_file, &*comms.node_identity())
        .map_err(|e| format!("Failed to save node identity: {:?}", e))?;
    if let Some(hs) = comms.hidden_service() {
        save_as_json(&config.tor_identity_file, hs.tor_identity())
            .map_err(|e| format!("Failed to save tor identity: {:?}", e))?;
    }

    Ok((comms, dht))
}

/// Asynchronously initializes comms for the base node's wallet
/// ## Parameters
/// `node_identity` - The node identity to initialize the comms stack with, see [NodeIdentity]
/// `config` - The configuration in which to set up the comms stack, see [GlobalConfig]
/// `publisher` - The publisher for the publish-subscribe messaging system
/// `base_node_peer` - The base node for the wallet to connect to
/// `peers` - A list of peers to be added to the comms node, the current node identity of the comms stack is excluded if
/// found in the list. ## Returns
/// A Result containing the commsnode and dht on success, string will indicate the reason on error
async fn setup_wallet_comms(
    node_identity: Arc<NodeIdentity>,
    config: &GlobalConfig,
    publisher: PubsubDomainConnector,
    base_node_peer: Peer,
) -> Result<(CommsNode, Dht), String>
{
    let comms_config = CommsConfig {
        node_identity,
        user_agent: format!("tari/wallet/{}", env!("CARGO_PKG_VERSION")),
        transport_type: setup_wallet_transport_type(&config),
        datastore_path: config.wallet_peer_db_path.clone(),
        peer_database_name: "peers".to_string(),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        // TODO - make this configurable
        dht: DhtConfig {
            database_url: DbConnectionUrl::File(config.data_dir.join("dht-wallet.db")),
            auto_join: true,
            ..Default::default()
        },
        // TODO: This should be false unless testing locally - make this configurable
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
    };

    let mut seed_peers = parse_peer_seeds(&config.peer_seeds);
    seed_peers.push(base_node_peer);
    let (comms, dht) = initialize_comms(comms_config, publisher, seed_peers, Default::default())
        .await
        .map_err(|e| format!("Could not create comms layer: {:?}", e))?;

    // Save final node identity after comms has initialized. This is required because the public_address can be changed
    // by comms during initialization when using tor.
    save_as_json(&config.wallet_identity_file, &*comms.node_identity())
        .map_err(|e| format!("Failed to save node identity: {:?}", e))?;
    if let Some(hs) = comms.hidden_service() {
        save_as_json(&config.wallet_tor_identity_file, hs.tor_identity())
            .map_err(|e| format!("Failed to save tor identity: {:?}", e))?;
    }

    Ok((comms, dht))
}

/// Asynchronously registers services of the base node
///
/// ## Parameters
/// `comms` - A reference to the comms node. This is the communications stack
/// `db` - The interface to the blockchain database, for all transactions stored in a block
/// `dht` - A reference to the peer discovery service
/// `subscription_factory` - The publish-subscribe messaging system, wrapped in an atomic reference counter
/// `mempool` - The mempool interface, for all transactions not yet included or recently included in a block
/// `consensus_manager` - The consensus manager for the blockchain
/// `factories` -  Cryptographic factory based on Pederson Commitments
///
/// ## Returns
/// A hashmap of handles wrapped in an atomic reference counter
async fn register_base_node_services<B>(
    comms: &CommsNode,
    dht: &Dht,
    db: BlockchainDatabase<B>,
    subscription_factory: Arc<SubscriptionFactory>,
    mempool: Mempool<B>,
    consensus_manager: ConsensusManager,
) -> Arc<ServiceHandles>
where
    B: BlockchainBackend + 'static,
{
    let node_config = BaseNodeServiceConfig::default(); // TODO - make this configurable
    let mempool_config = MempoolServiceConfig::default(); // TODO - make this configurable
    StackBuilder::new(runtime::Handle::current(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(BaseNodeServiceInitializer::new(
            subscription_factory.clone(),
            db,
            mempool.clone(),
            consensus_manager,
            node_config,
        ))
        .add_initializer(MempoolServiceInitializer::new(
            subscription_factory.clone(),
            mempool,
            mempool_config,
        ))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_secs(30)),
                refresh_neighbours_interval: Duration::from_secs(3 * 60),
                random_peer_selection_ratio: 0.4,
                ..Default::default()
            },
            subscription_factory,
            dht.dht_requester(),
        ))
        .add_initializer(ChainMetadataServiceInitializer)
        .finish()
        .await
        .expect("Service initialization failed")
}

/// Asynchronously registers services for the base node's wallet
/// ## Parameters
/// `wallet_comms` - A reference to the comms node. This is the communications stack
/// `wallet_dht` - A reference to the peer discovery service
/// `wallet_db_conn` - A reference to the sqlite database connection for the transaction and output manager services
/// `subscription_factory` - The publish-subscribe messaging system, wrapped in an atomic reference counter
/// `factories` -  Cryptographic factory based on Pederson Commitments
///
/// ## Returns
/// A hashmap of handles wrapped in an atomic reference counter
async fn register_wallet_services(
    wallet_comms: &CommsNode,
    wallet_dht: &Dht,
    wallet_db_conn: &WalletDbConnection,
    subscription_factory: Arc<SubscriptionFactory>,
    factories: CryptoFactories,
    base_node_query_timeout: Duration,
    broadcast_monitoring_timeout: Duration,
    chain_monitoring_timeout: Duration,
    direct_send_timeout: Duration,
    broadcast_send_timeout: Duration,
    network: NetworkType,
) -> Arc<ServiceHandles>
{
    let transaction_db = TransactionServiceSqliteDatabase::new(wallet_db_conn.clone(), None);
    transaction_db.migrate(wallet_comms.node_identity().public_key().clone());

    StackBuilder::new(runtime::Handle::current(), wallet_comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(wallet_dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig{
                auto_ping_interval: None,
                ..Default::default()
            },
            subscription_factory.clone(),
            wallet_dht.dht_requester(),
    ))
        // Wallet services
        .add_initializer(OutputManagerServiceInitializer::new(
            OutputManagerServiceConfig::new(base_node_query_timeout),
            subscription_factory.clone(),
            OutputManagerSqliteDatabase::new(wallet_db_conn.clone(),None),
            factories.clone(),
            network
        ))
        .add_initializer(TransactionServiceInitializer::new(
            TransactionServiceConfig::new(broadcast_monitoring_timeout,
                                          chain_monitoring_timeout,
                                          direct_send_timeout,
                                          broadcast_send_timeout,),
            subscription_factory,
            transaction_db,
            wallet_comms.node_identity(),
            factories,network
        ))
        .finish()
        .await
        .expect("Service initialization failed")
}
