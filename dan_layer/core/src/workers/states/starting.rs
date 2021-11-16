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
    digital_assets_error::DigitalAssetError,
    models::{AssetDefinition, HotStuffTreeNode, Payload, QuorumCertificate, TariDanPayload},
    services::{
        infrastructure_services::NodeAddressable,
        AssetProcessor,
        BaseNodeClient,
        CommitteeManager,
        PayloadProcessor,
        PayloadProvider,
    },
    storage::{BackendAdapter, ChainStorageService, DbFactory, StateDbUnitOfWork, UnitOfWork},
    workers::states::ConsensusWorkerStateEvent,
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
        TBackendAdapter: BackendAdapter,
        TDbFactory: DbFactory<TBackendAdapter>,
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
        info!(target: LOG_TARGET, "Creating DB");
        let chain_db = db_factory.create()?;
        if chain_db.is_empty()? {
            info!(target: LOG_TARGET, "DB is empty, initializing");
            let mut tx = chain_db.new_unit_of_work();

            let state_db = db_factory.create_state_db()?;
            let mut state_tx = state_db.new_unit_of_work();

            info!(target: LOG_TARGET, "Loading initial state");
            let initial_state = asset_definition.initial_state();
            for schema in &initial_state.schemas {
                debug!(target: LOG_TARGET, "Setting initial state for {}", schema.name);
                for key_value in &schema.items {
                    debug!(
                        target: LOG_TARGET,
                        "Setting {:?} = {:?}", key_value.key, key_value.value
                    );
                    state_tx.set_value(schema.name.clone(), key_value.key.clone(), key_value.value.clone());
                }
            }
            for template in &asset_definition.template_parameters {
                debug!(
                    target: LOG_TARGET,
                    "Setting template parameters for: {}", template.template_id
                );
                payload_processor.init_template(template, &mut state_tx);
            }
            info!(target: LOG_TARGET, "Saving genesis node");
            let node = HotStuffTreeNode::genesis(payload_provider.create_genesis_payload());
            let genesis_qc = QuorumCertificate::genesis(node.hash().clone());
            chain_storage_service.add_node(&node, tx.clone()).await?;
            tx.commit_node(node.hash())?;
            debug!(target: LOG_TARGET, "Setting locked QC");
            chain_storage_service.set_locked_qc(genesis_qc, tx.clone()).await?;
            debug!(target: LOG_TARGET, "Committing state");
            state_tx.commit()?;
            debug!(target: LOG_TARGET, "Committing node");
            tx.commit()?;
        }

        Ok(ConsensusWorkerStateEvent::Initialized)
    }
}
