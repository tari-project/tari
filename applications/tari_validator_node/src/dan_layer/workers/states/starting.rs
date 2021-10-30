// Copyright 2021. The Tari Project
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
    dan_layer::{
        models::{AssetDefinition, Payload, QuorumCertificate, TariDanPayload},
        services::{
            infrastructure_services::NodeAddressable,
            BaseNodeClient,
            CommitteeManager,
            PayloadProcessor,
            PayloadProvider,
        },
        storage::{ChainStorageService, DbFactory},
        workers::states::ConsensusWorkerStateEvent,
    },
    digital_assets_error::DigitalAssetError,
};
use log::*;
use std::marker::PhantomData;

const LOG_TARGET: &str = "tari::dan::workers::states::starting";

pub struct Starting<TBaseNodeClient: BaseNodeClient> {
    base_node_client: PhantomData<TBaseNodeClient>,
}

impl<TBaseNodeClient> Starting<TBaseNodeClient>
where TBaseNodeClient: BaseNodeClient
{
    pub fn new() -> Self {
        Self {
            base_node_client: Default::default(),
        }
    }

    pub async fn next_event<
        TAddr: NodeAddressable,
        TCommitteeManager: CommitteeManager<TAddr>,
        TPayload: Payload,
        TPayloadProvider: PayloadProvider<TPayload>,
        TPayloadProcessor: PayloadProcessor<TPayload>,
        TDbFactory: DbFactory,
        TChainStorageService: ChainStorageService<TPayload>,
    >(
        &self,
        base_node_client: &mut TBaseNodeClient,
        asset_definition: &AssetDefinition,
        committee_manager: &mut TCommitteeManager,
        db_factory: &TDbFactory,
        payload_provider: &TPayloadProvider,
        payload_processor: &TPayloadProcessor,
        chain_storage_service: &TChainStorageService,
        node_id: &TAddr,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        info!(
            target: LOG_TARGET,
            "Checking base layer to see if we are part of the committee"
        );
        let tip = base_node_client.get_tip_info().await?;
        // committee service.get latest committee
        // get latest checkpoint on the base layer
        let last_checkpoint = base_node_client
            .get_current_checkpoint(
                tip.height_of_longest_chain - asset_definition.base_layer_confirmation_time,
                asset_definition.public_key.clone(),
                asset_definition.checkpoint_unique_id.clone(),
            )
            .await?;

        let last_checkpoint = match last_checkpoint {
            None => return Ok(ConsensusWorkerStateEvent::BaseLayerCheckpointNotFound),
            Some(chk) => chk,
        };

        committee_manager.read_from_checkpoint(last_checkpoint)?;

        if !committee_manager.current_committee()?.contains(node_id) {
            return Ok(ConsensusWorkerStateEvent::NotPartOfCommittee);
        }

        // read and create the genesis block
        let chain_db = db_factory.create();
        if chain_db.is_empty() {
            let mut tx = chain_db.new_unit_of_work();
            // let metadata = chain_db.metadata.read(&mut tx);
            let payload = payload_provider.create_genesis_payload();

            payload_processor.process_payload(&payload, &mut tx).await?;
            let genesis_qc = QuorumCertificate::genesis(payload);
            let mut tx = chain_storage_service.save_qc(&genesis_qc, tx).await?;
            tx.commit()?;
        }

        Ok(ConsensusWorkerStateEvent::Initialized)
    }
}
