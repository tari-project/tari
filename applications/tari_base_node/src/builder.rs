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

use crate::{
    bootstrap::{BaseNodeBootstrapper, WalletBootstrapper},
    miner,
    tasks,
};
use futures::{future, StreamExt};
use log::*;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tari_common::{DatabaseType, GlobalConfig};
use tari_comms::{peer_manager::NodeIdentity, CommsNode};
use tari_comms_dht::Dht;
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface, StateMachineHandle},
    chain_storage::{create_lmdb_database, BlockchainDatabase, BlockchainDatabaseConfig, LMDBDatabase, Validators},
    consensus::ConsensusManagerBuilder,
    mempool::{service::LocalMempoolService, Mempool, MempoolConfig},
    mining::{Miner, MinerInstruction},
    proof_of_work::randomx_factory::{RandomXConfig, RandomXFactory},
    transactions::types::CryptoFactories,
    validation::{
        block_validators::{BodyOnlyValidator, OrphanBlockValidator},
        header_validator::HeaderValidator,
        transaction_validators::{MempoolValidator, TxInputAndMaturityValidator, TxInternalConsistencyValidator},
    },
};
use tari_service_framework::ServiceHandles;
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::{handle::OutputManagerHandle, protocols::txo_validation_protocol::TxoValidationType},
    transaction_service::handle::TransactionServiceHandle,
    types::ValidationRetryStrategy,
};
use tokio::{
    sync::{broadcast::Sender as syncSender, watch},
    task,
    time::delay_for,
};

const LOG_TARGET: &str = "c::bn::initialization";

/// The base node context is a container for all the key structural pieces for the base node application, including the
/// communications stack, the node state machine, the miner and handles to the various services that are registered
/// on the comms stack.
pub struct BaseNodeContext {
    blockchain_db: BlockchainDatabase<LMDBDatabase>,
    base_node_comms: CommsNode,
    base_node_dht: Dht,
    wallet_comms: Option<CommsNode>,
    base_node_handles: ServiceHandles,
    wallet_handles: Option<ServiceHandles>,
    miner: Option<Miner>,
    miner_enabled: Arc<AtomicBool>,
    mining_status: Arc<AtomicBool>,
    miner_instruction_events: syncSender<MinerInstruction>,
    pub miner_hashrate: Arc<AtomicU64>,
}

impl BaseNodeContext {
    /// Starts the node container. This entails starting the miner and wallet (if `mining_enabled` is true) and then
    /// starting the base node state machine. This call consumes the NodeContainer instance.
    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "Tari base node has STARTED");
        // Start wallet & miner
        if self.wallet_handles.is_none() {
            info!(
                target: LOG_TARGET,
                "Miner and Wallet are not starting due to config setting disabling embedded Wallet instance"
            );
        } else if let Some(mut wallet_output_handle) = self.output_manager() {
            let mut miner = self.miner.take().expect("Miner was not constructed");
            let mut rx = miner.get_utxo_receiver_channel();
            task::spawn(async move {
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
                            task::spawn(async move {
                                delay_for(Duration::from_secs(240)).await;
                                let _ = oms_handle_clone
                                    .validate_txos(TxoValidationType::Unspent, ValidationRetryStrategy::UntilSuccess)
                                    .await;
                            });
                        },
                        Err(e) => warn!(target: LOG_TARGET, "Error adding output: {}", e),
                    }
                }
            });
            task::spawn(async move {
                info!(target: LOG_TARGET, "⚒️ Starting miner");
                miner.mine().await;
                info!(target: LOG_TARGET, "⚒️ Miner has shutdown");
            });
        }

        if let Err(e) = self.state_machine().shutdown_signal().await {
            warn!(target: LOG_TARGET, "Error shutting down Base Node State Machine: {}", e);
        }
        info!(target: LOG_TARGET, "Initiating communications stack shutdown");

        if let Some(wallet_comms) = self.wallet_comms {
            future::join(
                self.base_node_comms.wait_until_shutdown(),
                wallet_comms.wait_until_shutdown(),
            )
            .await;
        } else {
            self.base_node_comms.wait_until_shutdown().await
        }
        info!(target: LOG_TARGET, "Communications stack has shutdown");
    }

    /// Returns a handle to the Output Manager
    pub fn output_manager(&self) -> Option<OutputManagerHandle> {
        self.wallet_handles
            .as_ref()
            .map(|wh| wh.expect_handle::<OutputManagerHandle>())
    }

    /// Returns the handle to the Comms Interface
    pub fn local_node(&self) -> LocalNodeCommsInterface {
        self.base_node_handles.expect_handle::<LocalNodeCommsInterface>()
    }

    /// Returns the handle to the Mempool
    pub fn local_mempool(&self) -> LocalMempoolService {
        self.base_node_handles.expect_handle::<LocalMempoolService>()
    }

    /// Returns the CommsNode.
    pub fn base_node_comms(&self) -> &CommsNode {
        &self.base_node_comms
    }

    /// Returns the wallet CommsNode.
    pub fn wallet_comms(&self) -> Option<&CommsNode> {
        self.wallet_comms.as_ref()
    }

    /// Returns the wallet CommsNode.
    pub fn state_machine(&self) -> StateMachineHandle {
        self.base_node_handles.expect_handle::<StateMachineHandle>()
    }

    /// Returns this node's identity.
    pub fn base_node_identity(&self) -> Arc<NodeIdentity> {
        self.base_node_comms.node_identity()
    }

    /// Returns the base node DHT
    pub fn base_node_dht(&self) -> &Dht {
        &self.base_node_dht
    }

    /// Returns a BlockchainDatabase handle
    pub fn blockchain_db(&self) -> BlockchainDatabase<LMDBDatabase> {
        self.blockchain_db.clone()
    }

    /// Returns this node's wallet identity.
    pub fn wallet_node_identity(&self) -> Option<Arc<NodeIdentity>> {
        self.wallet_comms.as_ref().map(|wc| wc.node_identity())
    }

    /// Returns this node's miner enabled flag.
    pub fn miner_enabled(&self) -> Arc<AtomicBool> {
        self.miner_enabled.clone()
    }

    /// Returns this node's mining status.
    pub fn mining_status(&self) -> Arc<AtomicBool> {
        self.mining_status.clone()
    }

    /// Returns this node's miner atomic hash rate.
    pub fn miner_hashrate(&self) -> Arc<AtomicU64> {
        self.miner_hashrate.clone()
    }

    /// Returns this node's miner instruction event channel.
    pub fn miner_instruction_events(&self) -> syncSender<MinerInstruction> {
        self.miner_instruction_events.clone()
    }

    /// Return the handle to the Transaction Service
    pub fn wallet_transaction_service(&self) -> Option<TransactionServiceHandle> {
        self.wallet_handles
            .as_ref()
            .map(|wh| wh.expect_handle::<TransactionServiceHandle>())
    }

    /// Return the state machine channel to provide info updates
    pub fn get_state_machine_info_channel(&self) -> watch::Receiver<StatusInfo> {
        self.base_node_handles
            .expect_handle::<StateMachineHandle>()
            .get_status_info_watch()
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
    cleanup_orphans_at_startup: bool,
) -> Result<BaseNodeContext, anyhow::Error>
{
    let result = match &config.db_type {
        DatabaseType::Memory => {
            // let backend = MemoryDatabase::<HashDigest>::default();
            // build_node_context(
            //     backend,
            //     node_identity,
            //     wallet_node_identity,
            //     config,
            //     interrupt_signal,
            //     cleanup_orphans_at_startup,
            // )
            // .await?
            unimplemented!();
        },
        DatabaseType::LMDB(p) => {
            let backend = create_lmdb_database(&p, config.db_config.clone())?;
            build_node_context(
                backend,
                node_identity,
                wallet_node_identity,
                config,
                interrupt_signal,
                cleanup_orphans_at_startup,
            )
            .await?
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
async fn build_node_context(
    backend: LMDBDatabase,
    base_node_identity: Arc<NodeIdentity>,
    wallet_node_identity: Arc<NodeIdentity>,
    config: &GlobalConfig,
    interrupt_signal: ShutdownSignal,
    cleanup_orphans_at_startup: bool,
) -> Result<BaseNodeContext, anyhow::Error>
{
    //---------------------------------- Blockchain --------------------------------------------//

    let rules = ConsensusManagerBuilder::new(config.network.into()).build();
    let factories = CryptoFactories::default();
    let randomx_factory = RandomXFactory::new(RandomXConfig::default(), config.max_randomx_vms);
    let validators = Validators::new(
        BodyOnlyValidator::default(),
        HeaderValidator::new(rules.clone(), randomx_factory),
        OrphanBlockValidator::new(rules.clone(), factories.clone()),
    );
    let db_config = BlockchainDatabaseConfig {
        orphan_storage_capacity: config.orphan_storage_capacity,
        pruning_horizon: config.pruning_horizon,
        pruning_interval: config.pruned_mode_cleanup_interval,
    };
    let blockchain_db = BlockchainDatabase::new(backend, &rules, validators, db_config, cleanup_orphans_at_startup)?;
    let mempool_validator = MempoolValidator::new(vec![
        Box::new(TxInternalConsistencyValidator::new(factories.clone())),
        Box::new(TxInputAndMaturityValidator::new(blockchain_db.clone())),
    ]);
    let mempool = Mempool::new(MempoolConfig::default(), Arc::new(mempool_validator));

    //---------------------------------- Base Node  --------------------------------------------//
    debug!(target: LOG_TARGET, "Creating base node state machine.");

    let base_node_handles = BaseNodeBootstrapper {
        config,
        node_identity: base_node_identity,
        db: blockchain_db.clone(),
        mempool,
        rules: rules.clone(),
        factories: factories.clone(),
        interrupt_signal: interrupt_signal.clone(),
    }
    .bootstrap()
    .await?;

    let base_node_comms = base_node_handles.expect_handle::<CommsNode>();
    let base_node_dht = base_node_handles.expect_handle::<Dht>();

    //---------------------------------- Wallet --------------------------------------------//
    let wallet_handles = if config.enable_wallet {
        let wallet_handles = WalletBootstrapper {
            node_identity: wallet_node_identity,
            config: config.clone(),
            interrupt_signal: interrupt_signal.clone(),
            base_node_peer: base_node_comms.node_identity().to_peer(),
            factories,
        }
        .bootstrap()
        .await?;
        let wallet_comms = wallet_handles.expect_handle::<CommsNode>();

        task::spawn(tasks::sync_peers(
            base_node_comms.subscribe_connection_manager_events(),
            base_node_comms.peer_manager(),
            wallet_comms.peer_manager(),
        ));

        // Set the base node for the wallet to the 'local' base node
        let base_node_public_key = base_node_comms.node_identity().public_key().clone();
        let mut transaction_service_handle = wallet_handles.expect_handle::<TransactionServiceHandle>();
        transaction_service_handle
            .set_base_node_public_key(base_node_public_key.clone())
            .await
            .expect("Problem setting local base node public key for transaction service.");
        let oms_handle = wallet_handles.expect_handle::<OutputManagerHandle>();
        let state_machine = base_node_handles.expect_handle::<StateMachineHandle>();
        tasks::spawn_transaction_protocols_and_utxo_validation(
            state_machine,
            transaction_service_handle,
            oms_handle,
            config.base_node_query_timeout,
            base_node_public_key.clone(),
            interrupt_signal.clone(),
        );

        Some(wallet_handles)
    } else {
        None
    };

    //---------------------------------- Mining --------------------------------------------//

    let local_mp_interface = base_node_handles.expect_handle::<LocalMempoolService>();
    let node_event_stream = base_node_handles
        .expect_handle::<StateMachineHandle>()
        .get_state_change_event_stream();
    let mempool_event_stream = local_mp_interface.get_mempool_state_event_stream();
    let miner = miner::build_miner(
        &base_node_handles,
        interrupt_signal,
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
        blockchain_db,
        base_node_comms,
        base_node_dht,
        wallet_comms: wallet_handles.as_ref().map(|h| h.expect_handle::<CommsNode>()),
        base_node_handles,
        wallet_handles,
        miner: Some(miner),
        miner_enabled,
        mining_status,
        miner_instruction_events,
        miner_hashrate,
    })
}
