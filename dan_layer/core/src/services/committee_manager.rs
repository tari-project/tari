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

use std::collections::HashMap;

use async_trait::async_trait;
use log::*;
use tari_common_types::types::{PublicKey, ASSET_CHECKPOINT_ID};
use tari_utilities::hex::Hex;

use crate::{
    digital_assets_error::DigitalAssetError,
    models::{AssetDefinition, BaseLayerOutput, Committee},
    services::{infrastructure_services::NodeAddressable, BaseNodeClient},
};

const LOG_TARGET: &str = "tari::dan_layer::core::services::committee_manager";

#[async_trait]
pub trait CommitteeManager: Send + Sync + 'static {
    type Addr: NodeAddressable;
    async fn get_all_committees(&self) -> Result<Vec<AssetDefinition>, DigitalAssetError>;
    fn current_committee(&self, asset_public_key: &PublicKey) -> Result<&Committee<Self::Addr>, DigitalAssetError>;

    // fn read_from_checkpoint(&mut self, output: BaseLayerOutput) -> Result<(), DigitalAssetError>;
    async fn check_for_changes(&mut self, asset_public_key: &PublicKey) -> Result<(), DigitalAssetError>;
}

#[derive(Clone)]
pub struct StaticListCommitteeManager {
    committee: Committee<PublicKey>,
    asset_definition: AssetDefinition,
}

impl StaticListCommitteeManager {
    pub fn new(committee: Vec<PublicKey>, asset_definition: AssetDefinition) -> Self {
        Self {
            committee: Committee::new(committee),
            asset_definition,
        }
    }
}

#[async_trait]
impl CommitteeManager for StaticListCommitteeManager {
    type Addr = PublicKey;

    async fn get_all_committees(&self) -> Result<Vec<AssetDefinition>, DigitalAssetError> {
        Ok(vec![self.asset_definition.clone()])
    }

    fn current_committee(&self, _asset_public_key: &PublicKey) -> Result<&Committee<PublicKey>, DigitalAssetError> {
        Ok(&self.committee)
    }

    async fn check_for_changes(&mut self, _asset_public_key: &PublicKey) -> Result<(), DigitalAssetError> {
        Ok(())
    }
}

/// A committee manager that uses the base layer as the source of truth
#[derive(Clone)]
pub struct BaseLayerCommitteeManager<TBaseNodeClient: BaseNodeClient> {
    committee: HashMap<PublicKey, Committee<PublicKey>>,
    base_node_client: TBaseNodeClient,
}

impl<TBaseNodeClient: BaseNodeClient> BaseLayerCommitteeManager<TBaseNodeClient> {
    pub fn new(base_node_client: TBaseNodeClient) -> Self {
        Self {
            committee: Default::default(),
            base_node_client,
        }
    }
}

#[async_trait]
impl<TBaseNodeClient: BaseNodeClient + 'static> CommitteeManager for BaseLayerCommitteeManager<TBaseNodeClient> {
    type Addr = PublicKey;

    async fn get_all_committees(&self) -> Result<Vec<AssetDefinition>, DigitalAssetError> {
        todo!()
    }

    fn current_committee(&self, asset_public_key: &PublicKey) -> Result<&Committee<PublicKey>, DigitalAssetError> {
        todo!()
        // match self.committee.get(&asset_public_key) {
        //     Some(c) => Ok(c),
        //     None => {
        //         self.check_for_changes(asset_public_key)?;
        //         self.committee
        //             .get(&asset_public_key)
        //             .ok_or_else(|| DigitalAssetError::NotFound {
        //                 entity: "Checkpoint",
        //                 id: asset_public_key.to_string(),
        //             })
        //     },
        // }
    }

    async fn check_for_changes(&mut self, asset_public_key: &PublicKey) -> Result<(), DigitalAssetError> {
        // let mut next_scanned_height = 0u64;
        // let mut last_tip = 0u64;
        // let mut monitoring = Monitoring::new(self.config.committee_management_confirmation_time);
        // loop {
        //     let tip = base_node_client
        //         .get_tip_info()
        //         .await
        //         .map_err(|e| ExitError::new(ExitCode::DigitalAssetError, e))?;
        //     if tip.height_of_longest_chain >= next_scanned_height {
        //         info!(
        //             target: LOG_TARGET,
        //             "Scanning base layer (tip : {}) for new assets", tip.height_of_longest_chain
        //         );
        //         if self.config.scan_for_assets {
        //             next_scanned_height =
        //                 tip.height_of_longest_chain + self.config.committee_management_polling_interval;
        //             info!(target: LOG_TARGET, "Next scanning height {}", next_scanned_height);
        //         } else {
        //             next_scanned_height = u64::MAX; // Never run again.
        //         }
        //         let mut assets = base_node_client
        //             .get_assets_for_dan_node(node_identity.public_key().clone())
        //             .await
        //             .map_err(|e| ExitError::new(ExitCode::DigitalAssetError, e))?;
        //         info!(
        //             target: LOG_TARGET,
        //             "Base node returned {} asset(s) to process",
        //             assets.len()
        //         );
        //         if let Some(allow_list) = &self.config.assets_allow_list {
        //             assets.retain(|(asset, _)| allow_list.contains(&asset.public_key.to_hex()));
        //         }
        //         for (asset, mined_height) in assets.clone() {
        //             monitoring.add_if_unmonitored(asset.clone());
        //             monitoring.add_state(asset.public_key, mined_height, true);
        //         }
        //         let mut known_active_public_keys = assets.into_iter().map(|(asset, _)| asset.public_key);
        //         let active_public_keys = monitoring
        //             .get_active_public_keys()
        //             .into_iter()
        //             .cloned()
        //             .collect::<Vec<PublicKey>>();
        //         for public_key in active_public_keys {
        //             if !known_active_public_keys.any(|pk| pk == public_key) {
        //                 // Active asset is not part of the newly known active assets, maybe there were no checkpoint
        // for                 // the asset. Are we still part of the committee?
        //                 if let (false, height) = base_node_client
        //                     .check_if_in_committee(public_key.clone(), node_identity.public_key().clone())
        //                     .await
        //                     .unwrap()
        //                 {
        //                     // We are not part of the latest committee, set the state to false
        //                     monitoring.add_state(public_key.clone(), height, false)
        //                 }
        //             }
        //         }
        //     }
        //
        //     if tip.height_of_longest_chain > last_tip {
        //         last_tip = tip.height_of_longest_chain;
        //         monitoring.update_height(last_tip, |asset| {}
        //         }
        todo!()
        // let mut base_node_client = self.base_node_client;
        // let tip = base_node_client.get_tip_info().await?;
        // let last_checkpoint = base_node_client
        //     .get_current_checkpoint(
        //         tip.height_of_longest_chain,
        //         asset_public_key.clone(),
        //         // TODO: read this from the chain maybe?
        //         ASSET_CHECKPOINT_ID.into(),
        //     )
        //     .await?;
        //
        // let last_checkpoint = match last_checkpoint {
        //     None => {
        //         return Err(DigitalAssetError::NotFound {
        //             entity: "checkpoint",
        //             id: asset_public_key.to_hex(),
        //         })
        //     },
        //     Some(chk) => chk,
        // };
        //
        // let committee = last_checkpoint
        //     .get_side_chain_committee()
        //     .ok_or(DigitalAssetError::NoCommitteeForAsset)?;
        //
        // debug!(
        //     target: LOG_TARGET,
        //     "Found {} committee member(s): {}",
        //     committee.len(),
        //     committee.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
        // );
        // Ok(())
    }

    // fn read_from_checkpoint(&mut self, output: BaseLayerOutput) -> Result<(), DigitalAssetError> {
    //     // TODO: better error
    //     let committee = output.get_side_chain_committee().unwrap();
    //     self.committee = Committee::new(committee.to_vec());
    //     Ok(())
    // }
}
