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

use std::sync::{Arc, RwLock};

use log::*;
use tari_common::{
    configuration::Network,
    exit_codes::{ExitCode, ExitError},
};
use tari_comms::{peer_manager::NodeIdentity, protocol::rpc::RpcServerHandle, CommsNode};
use tari_comms_dht::Dht;
use tari_core::{
    base_node::{state_machine_service::states::StatusInfo, LocalNodeCommsInterface, StateMachineHandle},
    chain_storage::{create_lmdb_database, BlockchainDatabase, ChainStorageError, LMDBDatabase, Validators},
    consensus::ConsensusManager,
    mempool::{service::LocalMempoolService, Mempool},
    proof_of_work::randomx_factory::RandomXFactory,
    transactions::CryptoFactories,
    validation::{
        block_body::{BlockBodyFullValidator, BlockBodyInternalConsistencyValidator},
        header::HeaderFullValidator,
        transaction::TransactionFullValidator,
        DifficultyCalculator,
    },
    OutputSmt,
};
use tari_p2p::{auto_update::SoftwareUpdaterHandle, services::liveness::LivenessHandle};
use tari_service_framework::ServiceHandles;
use tari_shutdown::ShutdownSignal;
use tokio::sync::watch;

use crate::{bootstrap::BaseNodeBootstrapper, ApplicationConfig, DatabaseType};

const LOG_TARGET: &str = "c::bn::initialization";

/// The base node context is a container for all the key structural pieces for the base node application, including the
/// communications stack, the node state machine and handles to the various services that are registered
/// on the comms stack.
pub struct BaseNodeContext {
    config: Arc<ApplicationConfig>,
    consensus_rules: ConsensusManager,
    blockchain_db: BlockchainDatabase<LMDBDatabase>,
    base_node_comms: CommsNode,
    base_node_dht: Dht,
    base_node_handles: ServiceHandles,
}

impl BaseNodeContext {
    /// Waits for shutdown of the base node state machine and comms.
    /// This call consumes the NodeContainer instance.
    pub async fn wait_for_shutdown(self) {
        self.state_machine().shutdown_signal().wait().await;
        info!(target: LOG_TARGET, "Waiting for communications stack shutdown");

        self.base_node_comms.wait_until_shutdown().await;
        info!(target: LOG_TARGET, "Communications stack has shutdown");
    }

    /// Return the node config
    pub fn config(&self) -> Arc<ApplicationConfig> {
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

    /// Returns the liveness service handle
    pub fn liveness(&self) -> LivenessHandle {
        self.base_node_handles.expect_handle()
    }

    /// Returns the base node state machine
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

    /// Returns a software update handle
    pub fn software_updater(&self) -> SoftwareUpdaterHandle {
        self.base_node_handles.expect_handle()
    }

    /// Returns a handle to the comms RPC server
    pub fn rpc_server(&self) -> RpcServerHandle {
        self.base_node_handles.expect_handle()
    }

    /// Returns a BlockchainDatabase handle
    pub fn blockchain_db(&self) -> BlockchainDatabase<LMDBDatabase> {
        self.blockchain_db.clone()
    }

    /// Returns the configured network
    pub fn network(&self) -> Network {
        self.config.base_node.network
    }

    /// Returns the consensus rules
    pub fn consensus_rules(&self) -> &ConsensusManager {
        &self.consensus_rules
    }

    /// Return the state machine channel to provide info updates
    pub fn get_state_machine_info_channel(&self) -> watch::Receiver<StatusInfo> {
        self.base_node_handles
            .expect_handle::<StateMachineHandle>()
            .get_status_info_watch()
    }

    pub fn get_report_grpc_error(&self) -> bool {
        self.config.base_node.report_grpc_error
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
    app_config: Arc<ApplicationConfig>,
    node_identity: Arc<NodeIdentity>,
    interrupt_signal: ShutdownSignal,
) -> Result<BaseNodeContext, ExitError> {
    let result = match &app_config.base_node.db_type {
        DatabaseType::Lmdb => {
            let rules = ConsensusManager::builder(app_config.base_node.network)
                .build()
                .map_err(|e| ExitError::new(ExitCode::UnknownError, e))?;
            let backend = create_lmdb_database(
                app_config.base_node.lmdb_path.as_path(),
                app_config.base_node.lmdb.clone(),
                rules,
            )
            .map_err(|e| ExitError::new(ExitCode::DatabaseError, e))?;
            build_node_context(backend, app_config, node_identity, interrupt_signal).await?
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
    app_config: Arc<ApplicationConfig>,
    base_node_identity: Arc<NodeIdentity>,
    interrupt_signal: ShutdownSignal,
) -> Result<BaseNodeContext, ExitError> {
    //---------------------------------- Blockchain --------------------------------------------//
    debug!(
        target: LOG_TARGET,
        "Building base node context for {}  network", app_config.base_node.network
    );
    let rules = ConsensusManager::builder(app_config.base_node.network)
        .build()
        .map_err(|e| ExitError::new(ExitCode::UnknownError, e))?;
    let factories = CryptoFactories::default();
    let randomx_factory = RandomXFactory::new(app_config.base_node.max_randomx_vms);
    let difficulty_calculator = DifficultyCalculator::new(rules.clone(), randomx_factory.clone());
    let smt = Arc::new(RwLock::new(OutputSmt::new()));
    let validators = Validators::new(
        BlockBodyFullValidator::new(rules.clone(), true),
        HeaderFullValidator::new(rules.clone(), difficulty_calculator.clone()),
        BlockBodyInternalConsistencyValidator::new(
            rules.clone(),
            app_config.base_node.bypass_range_proof_verification,
            factories.clone(),
        ),
    );

    let blockchain_db = BlockchainDatabase::new(
        backend,
        rules.clone(),
        validators,
        app_config.base_node.storage,
        difficulty_calculator,
        smt.clone(),
    )
    .map_err(|err| {
        if let ChainStorageError::DatabaseResyncRequired(reason) = err {
            ExitError::new(
                ExitCode::DbInconsistentState,
                format!("You may need to re-sync your database because {}", reason),
            )
        } else {
            ExitError::new(ExitCode::DatabaseError, err)
        }
    })?;

    let mempool_validator = TransactionFullValidator::new(
        factories.clone(),
        app_config.base_node.bypass_range_proof_verification,
        blockchain_db.clone(),
        rules.clone(),
    );
    let mempool = Mempool::new(
        app_config.base_node.mempool.clone(),
        rules.clone(),
        Box::new(mempool_validator),
    );

    //---------------------------------- Base Node  --------------------------------------------//
    debug!(target: LOG_TARGET, "Creating base node state machine.");

    let base_node_handles = BaseNodeBootstrapper {
        app_config: &app_config,
        node_identity: base_node_identity,
        db: blockchain_db.clone(),
        mempool,
        rules: rules.clone(),
        factories: factories.clone(),
        randomx_factory,
        interrupt_signal: interrupt_signal.clone(),
        smt,
    }
    .bootstrap()
    .await?;

    let base_node_comms = base_node_handles.expect_handle::<CommsNode>();
    let base_node_dht = base_node_handles.expect_handle::<Dht>();

    Ok(BaseNodeContext {
        config: app_config,
        consensus_rules: rules,
        blockchain_db,
        base_node_comms,
        base_node_dht,
        base_node_handles,
    })
}
