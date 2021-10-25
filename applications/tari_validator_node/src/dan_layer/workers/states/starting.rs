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
    dan_layer::{models::AssetDefinition, services::BaseNodeClient, workers::states::ConsensusWorkerStateEvent},
    digital_assets_error::DigitalAssetError,
    types::PublicKey,
};
use std::marker::PhantomData;

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

    pub async fn next_event(
        &self,
        base_node_client: &mut TBaseNodeClient,
        asset_definition: &AssetDefinition,
    ) -> Result<ConsensusWorkerStateEvent, DigitalAssetError> {
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

        // Get committee
        let committee = last_checkpoint.get_side_chain_committee();
        todo!();

        Ok(ConsensusWorkerStateEvent::Initialized)
    }
}
