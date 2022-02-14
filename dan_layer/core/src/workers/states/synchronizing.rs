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

use std::convert::TryFrom;

use log::*;
use tari_common_types::types::PublicKey;

use crate::{
    models::{AssetDefinition, CheckpointOutput},
    services::{BaseNodeClient, ValidatorNodeClientFactory},
    storage::{state::StateDbUnitOfWorkReader, DbFactory},
    workers::{state_sync::StateSynchronizer, states::ConsensusWorkerStateEvent},
    DigitalAssetError,
};

const LOG_TARGET: &str = "tari::dan::workers::states::starting";

#[derive(Debug, Clone, Default)]
pub struct Synchronizing;

impl Synchronizing {
    pub fn new() -> Self {
        Self
    }

    #[allow(unreachable_code, unused_variables)]
    pub async fn next_event<TDbFactory, TBaseNodeClient, TValidatorNodeClientFactory>(
        &mut self,
        base_node_client: &mut TBaseNodeClient,
        asset_definition: &AssetDefinition,
        db_factory: &TDbFactory,
        validator_node_client_factory: &TValidatorNodeClientFactory,
        our_address: &TValidatorNodeClientFactory::Addr,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError>
    where
        TDbFactory: DbFactory,
        TBaseNodeClient: BaseNodeClient,
        TValidatorNodeClientFactory: ValidatorNodeClientFactory<Addr = PublicKey>,
    {
        // TODO: The collectibles app does not post a valid initial merkle root for the initial asset checkpoint. So
        // this is always out-of-sync.
        // return Ok(ConsensusWorkerStateEvent::Synchronized);

        let tip = base_node_client.get_tip_info().await?;
        let last_checkpoint = base_node_client
            .get_current_checkpoint(
                tip.height_of_longest_chain - asset_definition.base_layer_confirmation_time,
                asset_definition.public_key.clone(),
                asset_definition.checkpoint_unique_id.clone(),
            )
            .await?;

        let last_checkpoint = match last_checkpoint {
            Some(cp) => CheckpointOutput::try_from(cp)?,
            None => return Ok(ConsensusWorkerStateEvent::BaseLayerCheckpointNotFound),
        };

        let asset_registration = base_node_client
            .get_asset_registration(asset_definition.public_key.clone())
            .await?;

        let mut state_db = db_factory.get_or_create_state_db(&asset_definition.public_key)?;
        {
            let state_reader = state_db.reader();
            let our_merkle_root = state_reader.calculate_root()?;
            if our_merkle_root.as_bytes() == last_checkpoint.merkle_root.as_slice() {
                info!(target: LOG_TARGET, "Our state database is up-to-date.");
                return Ok(ConsensusWorkerStateEvent::Synchronized);
            }
            let registration_merkle_root = asset_registration.and_then(|ar| ar.get_checkpoint_merkle_root());
            if registration_merkle_root
                .map(|mr| our_merkle_root.as_bytes() == mr.as_slice())
                .unwrap_or(false)
            {
                info!(
                    target: LOG_TARGET,
                    "Our state database is up-to-date (at initial state)."
                );
                return Ok(ConsensusWorkerStateEvent::Synchronized);
            }
        }

        info!(
            target: LOG_TARGET,
            "Our state database for asset '{}' is out of sync. Attempting to contact a committee member to synchronize",
            asset_definition.public_key
        );

        let synchronizer = StateSynchronizer::new(
            &last_checkpoint,
            &mut state_db,
            validator_node_client_factory,
            our_address,
        );
        synchronizer.sync().await?;

        Ok(ConsensusWorkerStateEvent::Synchronized)
    }
}
