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

use crate::bootstrap::BaseNodeBootstrapper;
use log::*;
use std::sync::Arc;
use tari_common::{DatabaseType, GlobalConfig};
use tari_comms::{peer_manager::NodeIdentity, protocol::rpc::RpcServerHandle, CommsNode};
use tari_comms_dht::Dht;
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface, StateMachineHandle},
    chain_storage::{create_lmdb_database, BlockchainDatabase, BlockchainDatabaseConfig, LMDBDatabase, Validators},
    consensus::ConsensusManagerBuilder,
    mempool::{service::LocalMempoolService, Mempool, MempoolConfig},
    proof_of_work::randomx_factory::{RandomXConfig, RandomXFactory},
    transactions::types::CryptoFactories,
    validation::{
        block_validators::{BodyOnlyValidator, OrphanBlockValidator},
        header_validator::HeaderValidator,
        transaction_validators::{
            MempoolValidator,
            TxConsensusValidator,
            TxInputAndMaturityValidator,
            TxInternalConsistencyValidator,
        },
    },
};
use tari_service_framework::ServiceHandles;
use tari_shutdown::ShutdownSignal;
use tokio::sync::watch;

const LOG_TARGET: &str = "c::bn::initialization";

/// The base node context is a container for all the key structural pieces for the base node application, including the
/// communications stack, the node state machine and handles to the various services that are registered
/// on the comms stack.
pub struct BaseNodeContext {
    config: Arc<GlobalConfig>,
    blockchain_db: BlockchainDatabase<LMDBDatabase>,
    base_node_comms: CommsNode,
    base_node_dht: Dht,
    base_node_handles: ServiceHandles,
}

impl BaseNodeContext {
    /// Starts the node container. This entails the base node state machine.
    /// This call consumes the NodeContainer instance.
    pub async fn run(self) {
        info!(target: LOG_TARGET, "Tari base node has STARTED");

        if let Err(e) = self.state_machine().shutdown_signal().await {
            warn!(target: LOG_TARGET, "Error shutting down Base Node State Machine: {}", e);
        }
        info!(target: LOG_TARGET, "Initiating communications stack shutdown");

        self.base_node_comms.wait_until_shutdown().await;
        info!(target: LOG_TARGET, "Communications stack has shutdown");
    }

    /// Return the node config
    pub fn config(&self) -> Arc<GlobalConfig> {
        self.config.clone()
    }

    /// Returns the handle to the Comms Interface
    pub fn local_node(&self) -> LocalNodeCommsInterface {
        self.base_node_handles.expect_handle()
    }

    /// Returns the handle to the Mempool
    pub fn local_mempool(&self) -> LocalMempoolService {
        self.base_node_handles.expect_handle()
    }

    /// Returns the CommsNode.
    pub fn base_node_comms(&self) -> &CommsNode {
        &self.base_node_comms
    }

    /// Returns the wallet CommsNode.
    pub fn state_machine(&self) -> StateMachineHandle {
        self.base_node_handles.expect_handle()
    }

    /// Returns this node's identity.
    pub fn base_node_identity(&self) -> Arc<NodeIdentity> {
        self.base_node_comms.node_identity()
    }

    /// Returns the base node DHT
    pub fn base_node_dht(&self) -> &Dht {
        &self.base_node_dht
    }

    /// Returns a handle to the comms RPC server
    pub fn rpc_server(&self) -> RpcServerHandle {
        self.base_node_handles.expect_handle()
    }

    /// Returns a BlockchainDatabase handle
    pub fn blockchain_db(&self) -> BlockchainDatabase<LMDBDatabase> {
        self.blockchain_db.clone()
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
    config: Arc<GlobalConfig>,
    node_identity: Arc<NodeIdentity>,
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
                config,
                interrupt_signal,
                cleanup_orphans_at_startup,
            )
            .await?
        },
    };
    Ok(result)
}

/// Constructs the base node context, this includes setting up the consensus manager, mempool, base node
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
    config: Arc<GlobalConfig>,
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
        Box::new(TxConsensusValidator::new(blockchain_db.clone())),
    ]);
    let mempool = Mempool::new(MempoolConfig::default(), Arc::new(mempool_validator));

    //---------------------------------- Base Node  --------------------------------------------//
    debug!(target: LOG_TARGET, "Creating base node state machine.");

    let base_node_handles = BaseNodeBootstrapper {
        config: &config,
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

    Ok(BaseNodeContext {
        config,
        blockchain_db,
        base_node_comms,
        base_node_dht,
        base_node_handles,
    })
}
