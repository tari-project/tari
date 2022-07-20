//  Copyright 2022, The Tari Project
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
    collections::HashMap,
    convert::{TryFrom, TryInto},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use log::*;
use tari_common_types::types::{FixedHash, FixedHashSizeError};
use tari_comms::{types::CommsPublicKey, NodeIdentity};
use tari_comms_dht::Dht;
use tari_core::transactions::transaction_components::{ContractConstitution, OutputType};
use tari_crypto::tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray};
use tari_dan_core::{
    models::{AssetDefinition, BaseLayerMetadata, Committee},
    services::{
        AcceptanceManager,
        BaseNodeClient,
        ConcreteAcceptanceManager,
        ConcreteAssetProcessor,
        ConcreteCheckpointManager,
        ConcreteCommitteeManager,
        LoggingEventsPublisher,
        MempoolServiceHandle,
        NodeIdentitySigningService,
        TariDanPayloadProcessor,
        TariDanPayloadProvider,
    },
    storage::{
        global::{ContractState, GlobalDb, GlobalDbMetadataKey},
        StorageError,
    },
    workers::ConsensusWorker,
    DigitalAssetError,
};
use tari_dan_storage_sqlite::{
    global::{models::contract::NewContract, SqliteGlobalDbBackendAdapter},
    SqliteDbFactory,
    SqliteStorageService,
};
use tari_p2p::{comms_connector::SubscriptionFactory, tari_message::TariMessageType};
use tari_service_framework::ServiceHandles;
use tari_shutdown::ShutdownSignal;
use tokio::{task, time};

use crate::{
    p2p::services::{
        inbound_connection_service::TariCommsInboundConnectionService,
        outbound_connection_service::TariCommsOutboundService,
    },
    DefaultServiceSpecification,
    GrpcBaseNodeClient,
    GrpcWalletClient,
    TariCommsValidatorNodeClientFactory,
    ValidatorNodeConfig,
};

const LOG_TARGET: &str = "tari::validator_node::asset_worker_manager";

pub struct ContractWorkerManager {
    config: ValidatorNodeConfig,
    global_db: GlobalDb<SqliteGlobalDbBackendAdapter>,
    last_scanned_height: u64,
    last_scanned_hash: Option<FixedHash>,
    base_node_client: GrpcBaseNodeClient,
    acceptance_manager: ConcreteAcceptanceManager<GrpcWalletClient, GrpcBaseNodeClient>,
    identity: Arc<NodeIdentity>,
    active_workers: HashMap<FixedHash, Arc<AtomicBool>>,
    mempool: MempoolServiceHandle,
    handles: ServiceHandles,
    subscription_factory: SubscriptionFactory,
    db_factory: SqliteDbFactory,
    shutdown: ShutdownSignal,
}

macro_rules! some_or_continue {
    ($expr:expr) => {
        match $expr {
            Some(x) => x,
            None => continue,
        }
    };
}

impl ContractWorkerManager {
    pub fn new(
        config: ValidatorNodeConfig,
        identity: Arc<NodeIdentity>,
        global_db: GlobalDb<SqliteGlobalDbBackendAdapter>,
        base_node_client: GrpcBaseNodeClient,
        acceptance_manager: ConcreteAcceptanceManager<GrpcWalletClient, GrpcBaseNodeClient>,
        mempool: MempoolServiceHandle,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
        db_factory: SqliteDbFactory,
        shutdown: ShutdownSignal,
    ) -> Self {
        Self {
            config,
            global_db,
            last_scanned_height: 0,
            last_scanned_hash: None,
            base_node_client,
            acceptance_manager,
            identity,
            mempool,
            handles,
            subscription_factory,
            db_factory,
            active_workers: HashMap::new(),
            shutdown,
        }
    }

    pub async fn start(mut self) -> Result<(), WorkerManagerError> {
        self.load_initial_state()?;

        info!(
            target: LOG_TARGET,
            "ℹ️ constitution_auto_accept is {}", self.config.constitution_auto_accept
        );

        if !self.config.scan_for_assets {
            info!(
                target: LOG_TARGET,
                "⚠️ scan_for_assets turned OFF. Contract scanner is shutting down."
            );
            self.shutdown.await;
            return Ok(());
        }

        self.start_active_contracts().await?;

        loop {
            // TODO: Get statuses of Accepted contracts to see if quorum is met if quorum is met, start the chain and
            // create a checkpoint

            let tip = self.base_node_client.get_tip_info().await?;
            let new_contracts = self.scan_for_new_contracts(&tip).await?;

            if !new_contracts.is_empty() {
                if self.config.constitution_auto_accept {
                    info!(
                        target: LOG_TARGET,
                        "ℹ️ Auto accepting {} new contract(s).",
                        new_contracts.len()
                    );
                    self.accept_contracts(new_contracts).await?;
                } else {
                    info!(
                        target: LOG_TARGET,
                        "ℹ️ Auto accept is OFF. {} new contract(s) will require manual acceptance.",
                        new_contracts.len()
                    );
                }
            }

            self.validate_contract_activity(&tip).await?;

            self.set_last_scanned_block(&tip)?;
            tokio::select! {
                _ = time::sleep(Duration::from_secs(self.config.constitution_management_polling_interval_in_seconds)) => {},
                _ = &mut self.shutdown => break
            }
        }

        Ok(())
    }

    async fn validate_contract_activity(&mut self, tip: &BaseLayerMetadata) -> Result<(), WorkerManagerError> {
        let active_contracts = self.global_db.get_contracts_with_state(ContractState::Active)?;

        for contract in active_contracts {
            let contract_id = FixedHash::try_from(contract.contract_id)?;
            info!("Validating contract={} activity", contract_id.to_hex());

            if let Some(checkpoint) = self.scan_for_last_checkpoint(tip, &contract_id).await? {
                let constitution = ContractConstitution::from_binary(&*contract.constitution).map_err(|error| {
                    WorkerManagerError::DataCorruption {
                        details: error.to_string(),
                    }
                })?;

                if tip.height_of_longest_chain >
                    checkpoint.mined_height + constitution.checkpoint_params.abandoned_interval
                {
                    self.global_db
                        .update_contract_state(contract_id, ContractState::Abandoned)?;

                    info!(
                        target: LOG_TARGET,
                        "Contract={} has missed checkpoints and has been marked Abandoned",
                        contract_id.to_hex()
                    );
                }
            }
        }

        Ok(())
    }

    async fn start_active_contracts(&mut self) -> Result<(), WorkerManagerError> {
        // Abandoned contracts can be revived by the VNC so they should continue to monitor them
        let mut active_contracts = self.global_db.get_contracts_with_state(ContractState::Active)?;
        active_contracts.append(&mut self.global_db.get_contracts_with_state(ContractState::Abandoned)?);
        info!(
            target: LOG_TARGET,
            "ℹ️ ready to work on {} active contract(s)",
            active_contracts.len()
        );

        for contract in active_contracts {
            let contract_id = FixedHash::try_from(contract.contract_id)?;

            let constitution = ContractConstitution::from_binary(&*contract.constitution).map_err(|error| {
                WorkerManagerError::DataCorruption {
                    details: error.to_string(),
                }
            })?;

            let kill = self.spawn_asset_worker(contract_id, &constitution);
            self.active_workers.insert(contract_id, kill);
        }

        Ok(())
    }

    async fn accept_contracts(&mut self, new_contracts: Vec<ActiveContract>) -> Result<(), WorkerManagerError> {
        for contract in new_contracts {
            info!(
                target: LOG_TARGET,
                "ℹ️ Posting acceptance transaction for contract {}", contract.contract_id
            );
            self.post_contract_acceptance(&contract).await?;

            // TODO: This should only be set to Accepted but we don't have steps for checking quorums yet.
            self.global_db
                .update_contract_state(contract.contract_id, ContractState::Active)?;

            // TODO: Scan for acceptances and once enough are present, start working on the contract
            //       for now, we start working immediately.
            let kill = self.spawn_asset_worker(contract.contract_id, &contract.constitution);
            self.active_workers.insert(contract.contract_id, kill);
        }

        Ok(())
    }

    fn load_initial_state(&mut self) -> Result<(), WorkerManagerError> {
        self.last_scanned_hash = self
            .global_db
            .get_data(GlobalDbMetadataKey::LastScannedConstitutionHash)?
            .map(TryInto::try_into)
            .transpose()?;
        self.last_scanned_height = self
            .global_db
            .get_data(GlobalDbMetadataKey::LastScannedConstitutionHeight)?
            .map(|data| {
                if data.len() == 8 {
                    let mut buf = [0u8; 8];
                    buf.copy_from_slice(&data);
                    Ok(u64::from_le_bytes(buf))
                } else {
                    Err(WorkerManagerError::DataCorruption {
                        details: "LastScannedConstitutionHeight did not contain little-endian u64 data".to_string(),
                    })
                }
            })
            .transpose()?
            .unwrap_or(0);
        Ok(())
    }

    async fn scan_for_last_checkpoint(
        &mut self,
        tip: &BaseLayerMetadata,
        contract_id: &FixedHash,
    ) -> Result<Option<Checkpoint>, WorkerManagerError> {
        info!(
            target: LOG_TARGET,
            "Scanning base layer (tip: {}) for last checkpoint of contract={}",
            tip.height_of_longest_chain,
            contract_id
        );

        let outputs = self
            .base_node_client
            .get_current_contract_outputs(
                tip.height_of_longest_chain,
                *contract_id,
                OutputType::ContractCheckpoint,
            )
            .await?;

        let mut outputs = outputs
            .iter()
            .map({
                |output| Checkpoint {
                    mined_height: output.mined_height,
                }
            })
            .collect::<Vec<Checkpoint>>();
        outputs.sort_by(|l, r| l.mined_height.partial_cmp(&r.mined_height).unwrap());

        Ok(outputs.pop())
    }

    async fn scan_for_new_contracts(
        &mut self,
        tip: &BaseLayerMetadata,
    ) -> Result<Vec<ActiveContract>, WorkerManagerError> {
        info!(
            target: LOG_TARGET,
            "🔍 Scanning base layer (tip: {}) for new assets", tip.height_of_longest_chain
        );

        let outputs = self
            .base_node_client
            .get_constitutions(self.last_scanned_hash, self.identity.public_key())
            .await?;

        let mut new_contracts = vec![];
        for utxo in outputs {
            let output = some_or_continue!(utxo.output.into_unpruned_output());
            let mined_height = utxo.mined_height;
            let sidechain_features = some_or_continue!(output.features.sidechain_features);
            let contract_id = sidechain_features.contract_id;
            let constitution = some_or_continue!(sidechain_features.constitution);
            if !constitution.validator_committee.contains(self.identity.public_key()) {
                warn!(
                    target: LOG_TARGET,
                    "Base node returned constitution for contract {} that this node is not part of", contract_id
                );
                continue;
            }

            if self.active_workers.contains_key(&contract_id) {
                warn!(target: LOG_TARGET, "Contract {} is already in active list", contract_id);
                continue;
            }

            if constitution.acceptance_requirements.acceptance_period_expiry < tip.height_of_longest_chain {
                warn!(
                    target: LOG_TARGET,
                    "Constitution acceptance period for contract {} has expired. Expires at {} but tip is {}",
                    contract_id,
                    constitution.acceptance_requirements.acceptance_period_expiry,
                    tip.height_of_longest_chain
                );

                let contract = ActiveContract {
                    constitution,
                    contract_id,
                    mined_height,
                };

                match self.global_db.save_contract(contract.into(), ContractState::Expired) {
                    Ok(_) => info!(
                        target: LOG_TARGET,
                        "Saving expired contract data id={}",
                        contract_id.to_hex()
                    ),
                    Err(error) => error!(
                        target: LOG_TARGET,
                        "Couldn't save expired contract data id={} received error={}",
                        contract_id.to_hex(),
                        error.to_string()
                    ),
                }

                continue;
            }

            let contract = ActiveContract {
                constitution,
                contract_id,
                mined_height,
            };

            match self
                .global_db
                .save_contract(contract.clone().into(), ContractState::Pending)
            {
                Ok(_) => info!(
                    target: LOG_TARGET,
                    "Saving contract data id={}",
                    contract.contract_id.to_hex()
                ),
                Err(error) => error!(
                    target: LOG_TARGET,
                    "Couldn't save contract data id={} received error={}",
                    contract.contract_id.to_hex(),
                    error.to_string()
                ),
            }

            new_contracts.push(contract);
        }

        info!(target: LOG_TARGET, "{} new contract(s) found", new_contracts.len());

        Ok(new_contracts)
    }

    fn spawn_asset_worker(&self, contract_id: FixedHash, constitution: &ContractConstitution) -> Arc<AtomicBool> {
        info!(target: LOG_TARGET, "🚀 starting work on contract {}", contract_id);
        let node_identity = self.identity.clone();
        let mempool = self.mempool.clone();
        let handles = self.handles.clone();
        let subscription_factory = self.subscription_factory.clone();
        let db_factory = self.db_factory.clone();
        let shutdown = self.shutdown.clone();
        // Create a kill signal for each asset
        let kill = Arc::new(AtomicBool::new(false));
        let dan_config = self.config.clone();
        task::spawn(Self::start_asset_worker(
            AssetDefinition {
                contract_id,
                committee: constitution
                    .validator_committee
                    .members()
                    .iter()
                    .map(|pk| pk.to_hex())
                    .collect(),
                phase_timeout: self.config.phase_timeout,
                base_layer_confirmation_time: 0,
                checkpoint_unique_id: vec![],
                initial_state: Default::default(),
                template_parameters: vec![],
                wasm_modules: vec![],
                wasm_functions: vec![],
                flow_functions: vec![],
            },
            node_identity,
            mempool,
            handles,
            subscription_factory,
            shutdown,
            dan_config,
            db_factory,
            kill.clone(),
        ));
        kill
    }

    async fn start_asset_worker(
        asset_definition: AssetDefinition,
        node_identity: Arc<NodeIdentity>,
        mempool_service: MempoolServiceHandle,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
        shutdown: ShutdownSignal,
        config: ValidatorNodeConfig,
        db_factory: SqliteDbFactory,
        kill: Arc<AtomicBool>,
    ) -> Result<(), DigitalAssetError> {
        let timeout = Duration::from_secs(asset_definition.phase_timeout);
        let committee = asset_definition
            .committee
            .iter()
            .map(|s| CommsPublicKey::from_hex(s).map_err(|_| DigitalAssetError::InvalidCommitteePublicKeyHex))
            .collect::<Result<Vec<_>, _>>()?;

        let committee = Committee::new(committee);
        let committee_service = ConcreteCommitteeManager::new(committee);

        let payload_provider = TariDanPayloadProvider::new(mempool_service.clone());

        let events_publisher = LoggingEventsPublisher::default();
        let signing_service = NodeIdentitySigningService::new(node_identity.clone());

        // let _backend = LmdbAssetStore::initialize(data_dir.join("asset_data"), Default::default())
        //     .map_err(|err| ExitCodes::DatabaseError(err.to_string()))?;
        // let data_store = AssetDataStore::new(backend);
        let asset_processor = ConcreteAssetProcessor::default();

        let payload_processor = TariDanPayloadProcessor::new(asset_processor);
        let mut inbound = TariCommsInboundConnectionService::new(asset_definition.contract_id);
        let receiver = inbound.get_receiver();

        let loopback = inbound.clone_sender();
        let shutdown_2 = shutdown.clone();
        task::spawn(async move {
            let topic_subscription =
                subscription_factory.get_subscription(TariMessageType::DanConsensusMessage, "HotStuffMessages");
            inbound.run(shutdown_2, topic_subscription).await
        });
        let dht = handles.expect_handle::<Dht>();
        let outbound = TariCommsOutboundService::new(dht.outbound_requester(), loopback, asset_definition.contract_id);
        let base_node_client = GrpcBaseNodeClient::new(config.base_node_grpc_address);
        let chain_storage = SqliteStorageService {};
        let wallet_client = GrpcWalletClient::new(config.wallet_grpc_address);
        let checkpoint_manager = ConcreteCheckpointManager::new(asset_definition.clone(), wallet_client);
        let validator_node_client_factory = TariCommsValidatorNodeClientFactory::new(dht.dht_requester());
        let mut consensus_worker = ConsensusWorker::<DefaultServiceSpecification>::new(
            receiver,
            outbound,
            committee_service,
            node_identity.public_key().clone(),
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
            asset_definition,
            base_node_client,
            timeout,
            db_factory,
            chain_storage,
            checkpoint_manager,
            validator_node_client_factory,
        );

        if let Err(err) = consensus_worker.run(shutdown, None, kill).await {
            error!(target: LOG_TARGET, "Consensus worker failed with error: {}", err);
            return Err(err);
        }

        Ok(())
    }

    async fn post_contract_acceptance(&mut self, contract: &ActiveContract) -> Result<(), WorkerManagerError> {
        let mut acceptance_manager = self.acceptance_manager.clone();

        let tx_id = acceptance_manager
            .publish_constitution_acceptance(&self.identity, &contract.contract_id)
            .await?;
        info!(
            target: LOG_TARGET,
            "Contract {} acceptance submitted with id={}", contract.contract_id, tx_id
        );
        Ok(())
    }

    fn set_last_scanned_block(&mut self, tip: &BaseLayerMetadata) -> Result<(), WorkerManagerError> {
        self.global_db.set_data(
            GlobalDbMetadataKey::LastScannedConstitutionHash,
            tip.tip_hash.as_bytes(),
        )?;
        self.global_db.set_data(
            GlobalDbMetadataKey::LastScannedConstitutionHeight,
            &tip.height_of_longest_chain.to_le_bytes(),
        )?;
        self.last_scanned_hash = Some(tip.tip_hash);
        self.last_scanned_height = tip.height_of_longest_chain;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerManagerError {
    #[error(transparent)]
    FixedHashSizeError(#[from] FixedHashSizeError),
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("DigitalAsset error: {0}")]
    DigitalAssetError(#[from] DigitalAssetError),
    // TODO: remove dead_code
    #[allow(dead_code)]
    #[error("Data corruption: {details}")]
    DataCorruption { details: String },
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct Checkpoint {
    pub mined_height: u64,
}

#[derive(Debug, Clone)]
struct ActiveContract {
    pub constitution: ContractConstitution,
    pub contract_id: FixedHash,
    pub mined_height: u64,
}

impl From<ActiveContract> for NewContract {
    fn from(value: ActiveContract) -> Self {
        Self {
            height: value.mined_height as i64,
            contract_id: value.contract_id.to_vec(),
            constitution: value.constitution.to_binary().unwrap(),
            state: 0,
        }
    }
}
