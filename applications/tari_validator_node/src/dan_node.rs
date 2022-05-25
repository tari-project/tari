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

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use log::*;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_types::types::PublicKey;
use tari_comms::{types::CommsPublicKey, NodeIdentity};
use tari_comms_dht::Dht;
use tari_crypto::tari_utilities::hex::Hex;
use tari_dan_core::{
    models::{domain_events::ConsensusWorkerDomainEvent, AssetDefinition, Committee, TariDanPayload},
    services::{
        infrastructure_services::NodeAddressable,
        BaseLayerCheckpointManager,
        BaseLayerCommitteeManager,
        BaseNodeClient,
        CommitteeManager,
        ConcreteAssetProcessor,
        LoggingEventsPublisher,
        MempoolServiceHandle,
        NodeIdentitySigningService,
        ServiceSpecification,
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
    monitoring::Monitoring,
    p2p::services::{
        inbound_connection_service::{TariCommsInboundConnectionService, TariCommsInboundReceiverHandle},
        outbound_connection_service::TariCommsOutboundService,
    },
    TariCommsValidatorNodeClientFactory,
};

const LOG_TARGET: &str = "tari::validator_node::app";

pub trait RunningServiceSpecification:
    ServiceSpecification<
    Addr = PublicKey,
    EventsPublisher = LoggingEventsPublisher<ConsensusWorkerDomainEvent>,
    InboundConnectionService = TariCommsInboundReceiverHandle,
    OutboundService = TariCommsOutboundService<TariDanPayload>,
    PayloadProvider = TariDanPayloadProvider<MempoolServiceHandle>,
    SigningService = NodeIdentitySigningService,
    PayloadProcessor = TariDanPayloadProcessor<ConcreteAssetProcessor>,
    BaseNodeClient = GrpcBaseNodeClient,
    DbFactory = SqliteDbFactory,
    ChainStorageService = SqliteStorageService,
    ValidatorNodeClientFactory = TariCommsValidatorNodeClientFactory,
>
{
}

pub struct DanNode {
    config: ValidatorNodeConfig,
}

impl DanNode {
    pub fn new(config: ValidatorNodeConfig) -> Self {
        Self { config }
    }

    pub async fn start<TSpecification: RunningServiceSpecification>(
        &self,
        shutdown: ShutdownSignal,
        node_identity: Arc<NodeIdentity>,
        mempool_service: MempoolServiceHandle,
        committee_manager: TSpecification::CommitteeManager,
        checkpoint_manager: TSpecification::CheckpointManager,
        db_factory: SqliteDbFactory,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
    ) -> Result<(), ExitError> {
        for asset in committee_manager
            .get_all_committees()
            .await
            .map_err(|de| ExitCode::DigitalAssetError)?
        {
            println!("Starting committee for asset:{}", asset.public_key);
            let node_identity = node_identity.as_ref().clone();
            let mempool = mempool_service.clone();
            let handles = handles.clone();
            let subscription_factory = subscription_factory.clone();
            let shutdown = shutdown.clone();
            // Create a kill signal for each asset
            let kill = Arc::new(AtomicBool::new(false));
            let dan_config = self.config.clone();
            let db_factory = db_factory.clone();
            DanNode::start_asset_worker::<TSpecification>(
                asset,
                node_identity,
                mempool,
                handles,
                subscription_factory,
                shutdown,
                dan_config,
                committee_manager.clone(),
                checkpoint_manager.clone(),
                db_factory,
                kill.clone(),
            )
            .await?;
        }

        // TODO: Loop and look for more committees

        //     }
        //     time::sleep(Duration::from_secs(120)).await;
        // }
        Ok(())
    }

    pub async fn start_asset_worker<TSpecification: RunningServiceSpecification>(
        asset_definition: AssetDefinition,
        node_identity: NodeIdentity,
        mempool_service: MempoolServiceHandle,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
        shutdown: ShutdownSignal,
        config: ValidatorNodeConfig,
        committee_service: TSpecification::CommitteeManager,
        checkpoint_manager: TSpecification::CheckpointManager,
        db_factory: SqliteDbFactory,
        kill: Arc<AtomicBool>,
    ) -> Result<(), ExitError> {
        let timeout = Duration::from_secs(asset_definition.phase_timeout);

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
        let chain_storage = SqliteStorageService {};
        let validator_node_client_factory = TariCommsValidatorNodeClientFactory::new(dht.dht_requester());
        let mut consensus_worker = ConsensusWorker::<TSpecification>::new(
            receiver,
            outbound,
            committee_service,
            node_identity.public_key().clone(),
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
            asset_definition,
            timeout,
            db_factory,
            chain_storage,
            checkpoint_manager,
            validator_node_client_factory,
        );

        if let Err(err) = consensus_worker.run(shutdown.clone(), None, kill).await {
            error!(target: LOG_TARGET, "Consensus worker failed with error: {}", err);
            return Err(ExitError::new(ExitCode::UnknownError, err));
        }

        Ok(())
    }

    // async fn start_asset_proxy(&self) -> Result<(), ExitCodes> {
    //     todo!()
    // }
}
