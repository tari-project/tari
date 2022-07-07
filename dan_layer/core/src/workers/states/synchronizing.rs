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

use std::{convert::TryFrom, marker::PhantomData};

use log::*;
use tari_comms::types::CommsPublicKey;
use tari_core::transactions::transaction_components::OutputType;
use tari_dan_engine::state::StateDbUnitOfWorkReader;

use crate::{
    models::{AssetDefinition, BaseLayerOutput, CheckpointOutput},
    services::{BaseNodeClient, ServiceSpecification},
    storage::DbFactory,
    workers::{state_sync::StateSynchronizer, states::ConsensusWorkerStateEvent},
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::dan::workers::states::starting";

#[derive(Debug, Default, Clone)]
pub struct Synchronizing<TSpecification> {
    _spec: PhantomData<TSpecification>,
}

impl<TSpecification: ServiceSpecification<Addr = CommsPublicKey>> Synchronizing<TSpecification> {
    pub fn new() -> Self {
        Default::default()
    }

    #[allow(unreachable_code, unused_variables)]
    pub async fn next_event(
        &mut self,
        base_node_client: &mut TSpecification::BaseNodeClient,
        asset_definition: &AssetDefinition,
        db_factory: &TSpecification::DbFactory,
        validator_node_client_factory: &TSpecification::ValidatorNodeClientFactory,
        our_address: &TSpecification::Addr,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
        // TODO: The collectibles app does not post a valid initial merkle root for the initial asset checkpoint. So
        // this is always out-of-sync.
        // return Ok(ConsensusWorkerStateEvent::Synchronized);

        let tip = base_node_client.get_tip_info().await?;
        let mut last_checkpoint = base_node_client
            .get_current_contract_outputs(
                tip.height_of_longest_chain
                    .saturating_sub(asset_definition.base_layer_confirmation_time),
                asset_definition.contract_id,
                OutputType::ContractCheckpoint,
            )
            .await?;

        let last_checkpoint = match last_checkpoint.pop() {
            Some(utxo) => {
                let output = BaseLayerOutput::try_from(utxo)?;
                CheckpointOutput::try_from(output)?
            },
            None => return Ok(ConsensusWorkerStateEvent::BaseLayerCheckpointNotFound),
        };

        let mut constitution = base_node_client
            .get_current_contract_outputs(
                tip.height_of_longest_chain
                    .saturating_sub(asset_definition.base_layer_confirmation_time),
                asset_definition.contract_id,
                OutputType::ContractConstitution,
            )
            .await?;

        let current_constitution = match constitution.pop() {
            Some(o) => BaseLayerOutput::try_from(o)?,
            None => return Ok(ConsensusWorkerStateEvent::BaseLayerCheckopintNotFound),
        };

        let mut state_db = db_factory.get_or_create_state_db(&asset_definition.contract_id)?;
        {
            let state_reader = state_db.reader();
            let our_merkle_root = state_reader.calculate_root()?;
            if our_merkle_root.as_bytes() == last_checkpoint.merkle_root.as_slice() {
                info!(target: LOG_TARGET, "Our state database is up-to-date.");
                return Ok(ConsensusWorkerStateEvent::Synchronized);
            }
        }

        let committee = current_constitution
            .features
            .constitution_committee()
            .map(|committee| committee.members().to_vec())
            .unwrap_or_default();

        info!(
            target: LOG_TARGET,
            "Our state database for asset '{}' is out of sync. Attempting to contact a committee member to synchronize",
            asset_definition.contract_id
        );

        let synchronizer = StateSynchronizer::new(
            &last_checkpoint,
            &mut state_db,
            validator_node_client_factory,
            our_address,
            &committee,
        );
        synchronizer.sync().await?;

        Ok(ConsensusWorkerStateEvent::Synchronized)
    }
}
