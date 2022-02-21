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

use std::marker::PhantomData;

use log::*;
use tari_utilities::hex::Hex;

use crate::{
    digital_assets_error::DigitalAssetError,
    models::AssetDefinition,
    services::{infrastructure_services::NodeAddressable, BaseNodeClient, CommitteeManager},
    storage::DbFactory,
    workers::states::ConsensusWorkerStateEvent,
};

const LOG_TARGET: &str = "tari::dan::workers::states::starting";

pub struct Starting<TBaseNodeClient: BaseNodeClient> {
    base_node_client: PhantomData<TBaseNodeClient>,
}

impl<TBaseNodeClient: BaseNodeClient> Default for Starting<TBaseNodeClient> {
    fn default() -> Self {
        Self {
            base_node_client: PhantomData,
        }
    }
}

impl<TBaseNodeClient> Starting<TBaseNodeClient>
where TBaseNodeClient: BaseNodeClient
{
    pub async fn next_event<
        TAddr: NodeAddressable,
        TCommitteeManager: CommitteeManager<TAddr>,
        TDbFactory: DbFactory,
    >(
        &self,
        base_node_client: &mut TBaseNodeClient,
        asset_definition: &AssetDefinition,
        committee_manager: &mut TCommitteeManager,
        db_factory: &TDbFactory,
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
            info!(
                target: LOG_TARGET,
                "Validator node not part of committee for asset public key '{}'",
                asset_definition.public_key.to_hex()
            );
            return Ok(ConsensusWorkerStateEvent::NotPartOfCommittee);
        }

        info!(
            target: LOG_TARGET,
            "Validator node is a committee member for asset public key '{}'",
            asset_definition.public_key.to_hex()
        );
        // read and create the genesis block
        info!(target: LOG_TARGET, "Creating DB");
        let _ = db_factory.get_or_create_chain_db(&asset_definition.public_key)?;

        Ok(ConsensusWorkerStateEvent::Initialized)
    }
}
