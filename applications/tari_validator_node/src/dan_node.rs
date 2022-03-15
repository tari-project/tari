//  Copyright 2021. The Tari Project
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

use std::{collections::HashMap, sync::Arc, time::Duration};

use log::*;
use tari_common::{
    exit_codes::{ExitCode, ExitError},
    GlobalConfig,
};
use tari_comms::{types::CommsPublicKey, NodeIdentity};
use tari_comms_dht::Dht;
use tari_crypto::tari_utilities::hex::Hex;
use tari_dan_core::{
    models::{AssetDefinition, Committee},
    services::{
        BaseNodeClient,
        ConcreteAssetProcessor,
        ConcreteCheckpointManager,
        ConcreteCommitteeManager,
        LoggingEventsPublisher,
        MempoolServiceHandle,
        NodeIdentitySigningService,
        TariDanPayloadProcessor,
        TariDanPayloadProvider,
    },
    workers::ConsensusWorker,
};
use tari_dan_storage_sqlite::{SqliteDbFactory, SqliteStorageService};
use tari_p2p::{comms_connector::SubscriptionFactory, tari_message::TariMessageType};
use tari_service_framework::ServiceHandles;
use tari_shutdown::ShutdownSignal;
use tokio::{task, time};

use crate::{
    config::ValidatorNodeConfig,
    default_service_specification::DefaultServiceSpecification,
    grpc::services::{base_node_client::GrpcBaseNodeClient, wallet_client::GrpcWalletClient},
    p2p::services::{
        inbound_connection_service::TariCommsInboundConnectionService,
        outbound_connection_service::TariCommsOutboundService,
    },
    TariCommsValidatorNodeClientFactory,
};

const LOG_TARGET: &str = "tari::validator_node::app";

pub struct DanNode {
    config: ValidatorNodeConfig,
}

impl DanNode {
    pub fn new(config: ValidatorNodeConfig) -> Self {
        Self { config }
    }

    pub async fn start(
        &self,
        shutdown: ShutdownSignal,
        node_identity: Arc<NodeIdentity>,
        mempool_service: MempoolServiceHandle,
        db_factory: SqliteDbFactory,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
    ) -> Result<(), ExitError> {
        let mut base_node_client = GrpcBaseNodeClient::new(self.config.base_node_grpc_address);
        #[allow(clippy::mutable_key_type)]
        let mut tasks = HashMap::new();
        let mut next_scanned_height = 0u64;
        loop {
            let tip = base_node_client.get_tip_info().await.unwrap();
            if tip.height_of_longest_chain >= next_scanned_height {
                info!(
                    target: LOG_TARGET,
                    "Scanning base layer (tip : {}) for new assets", tip.height_of_longest_chain
                );
                if self.config.scan_for_assets {
                    next_scanned_height = tip.height_of_longest_chain + self.config.new_asset_scanning_interval;
                    info!(target: LOG_TARGET, "Next scanning height {}", next_scanned_height);
                } else {
                    next_scanned_height = u64::MAX; // Never run again.
                }

                let assets = base_node_client
                    .get_assets_for_dan_node(node_identity.public_key().clone())
                    .await
                    .unwrap();
                info!(
                    target: LOG_TARGET,
                    "Base node returned {} asset(s) to process",
                    assets.len()
                );
                for asset in assets {
                    if tasks.contains_key(&asset.public_key) {
                        debug!(
                            target: LOG_TARGET,
                            "Asset task already running for asset '{}'", asset.public_key
                        );
                        continue;
                    }
                    if let Some(allow_list) = &self.config.assets_allow_list {
                        if !allow_list.contains(&asset.public_key.to_hex()) {
                            debug!(
                                target: LOG_TARGET,
                                "Asset '{}' is not allowlisted for processing ", asset.public_key
                            );
                            continue;
                        }
                    }
                    info!(target: LOG_TARGET, "Adding asset '{}'", asset.public_key);
                    let node_identity = node_identity.as_ref().clone();
                    let mempool = mempool_service.clone();
                    let handles = handles.clone();
                    let subscription_factory = subscription_factory.clone();
                    let shutdown = shutdown.clone();
                    let dan_config = self.config.clone();
                    let db_factory = db_factory.clone();
                    tasks.insert(
                        asset.public_key.clone(),
                        task::spawn(DanNode::start_asset_worker(
                            asset,
                            node_identity,
                            mempool,
                            handles,
                            subscription_factory,
                            shutdown,
                            dan_config,
                            db_factory,
                        )),
                    );
                }
            }
            time::sleep(Duration::from_secs(120)).await;
        }
    }

    // async fn start_asset_proxy(&self) -> Result<(), ExitCodes> {
    //     todo!()
    // }

    pub async fn start_asset_worker(
        asset_definition: AssetDefinition,
        node_identity: NodeIdentity,
        mempool_service: MempoolServiceHandle,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
        shutdown: ShutdownSignal,
        config: ValidatorNodeConfig,
        db_factory: SqliteDbFactory,
    ) -> Result<(), ExitError> {
        let timeout = Duration::from_secs(asset_definition.phase_timeout);
        let committee = asset_definition
            .committee
            .iter()
            .map(|s| {
                CommsPublicKey::from_hex(s)
                    .map_err(|e| ExitError::new(ExitCode::ConfigError, format!("could not convert to hex:{}", e)))
            })
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
        let mut inbound = TariCommsInboundConnectionService::new(asset_definition.public_key.clone());
        let receiver = inbound.get_receiver();

        let loopback = inbound.clone_sender();
        let shutdown_2 = shutdown.clone();
        task::spawn(async move {
            let topic_subscription =
                subscription_factory.get_subscription(TariMessageType::DanConsensusMessage, "HotStuffMessages");
            inbound.run(shutdown_2, topic_subscription).await
        });
        let dht = handles.expect_handle::<Dht>();
        let outbound =
            TariCommsOutboundService::new(dht.outbound_requester(), loopback, asset_definition.public_key.clone());
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

        if let Err(err) = consensus_worker.run(shutdown.clone(), None).await {
            error!(target: LOG_TARGET, "Consensus worker failed with error: {}", err);
            return Err(ExitError::new(ExitCode::UnknownError, err));
        }

        Ok(())
    }
}
