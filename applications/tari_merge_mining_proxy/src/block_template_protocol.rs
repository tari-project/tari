//  Copyright 2021, The Tari Project
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

//! Methods for seting up a new block.

use std::{cmp, sync::Arc};

use log::*;
use tari_base_node_grpc_client::{grpc, BaseNodeGrpcClient};
use tari_core::proof_of_work::{monero_rx, monero_rx::FixedByteArray, Difficulty};
use tari_wallet_grpc_client::WalletGrpcClient;

use crate::{
    block_template_data::{BlockTemplateData, BlockTemplateDataBuilder},
    common::merge_mining,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
};

const LOG_TARGET: &str = "tari_mm_proxy::proxy::block_template_protocol";

/// Structure holding grpc connections.
pub struct BlockTemplateProtocol<'a> {
    config: Arc<MergeMiningProxyConfig>,
    base_node_client: &'a mut BaseNodeGrpcClient<tonic::transport::Channel>,
    wallet_client: &'a mut WalletGrpcClient<tonic::transport::Channel>,
}

impl<'a> BlockTemplateProtocol<'a> {
    pub fn new(
        base_node_client: &'a mut BaseNodeGrpcClient<tonic::transport::Channel>,
        wallet_client: &'a mut WalletGrpcClient<tonic::transport::Channel>,
        config: Arc<MergeMiningProxyConfig>,
    ) -> Self {
        Self {
            base_node_client,
            wallet_client,
            config,
        }
    }
}

impl BlockTemplateProtocol<'_> {
    /// Create [FinalBlockTemplateData] with [MoneroMiningData].
    pub async fn get_next_block_template(
        mut self,
        monero_mining_data: MoneroMiningData,
    ) -> Result<FinalBlockTemplateData, MmProxyError> {
        loop {
            let new_template = self.get_new_block_template().await?;
            let coinbase = self.get_coinbase(&new_template).await?;

            let template_height = new_template.template.header.as_ref().map(|h| h.height).unwrap_or(0);
            if !self.check_expected_tip(template_height).await? {
                debug!(
                    target: LOG_TARGET,
                    "Chain tip has progressed past template height {}. Fetching a new block template.", template_height
                );
                continue;
            }

            debug!(target: LOG_TARGET, "Added coinbase to new block template");
            let block_template_with_coinbase = merge_mining::add_coinbase(coinbase, new_template.template.clone())?;
            info!(
                target: LOG_TARGET,
                "Received new block template from Tari base node for height #{}",
                new_template
                    .template
                    .header
                    .as_ref()
                    .map(|h| h.height)
                    .unwrap_or_default(),
            );
            let block = match self.get_new_block(block_template_with_coinbase).await {
                Ok(b) => b,
                Err(MmProxyError::FailedPreconditionBlockLostRetry) => {
                    debug!(
                        target: LOG_TARGET,
                        "Chain tip has progressed past template height {}. Fetching a new block template.",
                        template_height
                    );
                    continue;
                },
                Err(err) => return Err(err),
            };

            let final_block = self.add_monero_data(block, monero_mining_data, new_template)?;
            return Ok(final_block);
        }
    }

    /// Get new block from base node.
    async fn get_new_block(
        &mut self,
        template: grpc::NewBlockTemplate,
    ) -> Result<grpc::GetNewBlockResult, MmProxyError> {
        let resp = self.base_node_client.get_new_block(template).await;

        match resp {
            Ok(resp) => Ok(resp.into_inner()),
            Err(status) => {
                if status.code() == tonic::Code::FailedPrecondition {
                    return Err(MmProxyError::FailedPreconditionBlockLostRetry);
                }
                Err(status.into())
            },
        }
    }

    /// Get new [block template](NewBlockTemplateData).
    async fn get_new_block_template(&mut self) -> Result<NewBlockTemplateData, MmProxyError> {
        let grpc::NewBlockTemplateResponse {
            miner_data,
            new_block_template: template,
            initial_sync_achieved,
        } = self
            .base_node_client
            .get_new_block_template(grpc::NewBlockTemplateRequest {
                algo: Some(grpc::PowAlgo {
                    pow_algo: grpc::pow_algo::PowAlgos::Randomx.into(),
                }),
                max_weight: 0,
            })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?
            .into_inner();

        let miner_data = miner_data.ok_or(MmProxyError::GrpcResponseMissingField("miner_data"))?;
        let template = template.ok_or(MmProxyError::GrpcResponseMissingField("new_block_template"))?;
        Ok(NewBlockTemplateData {
            template,
            miner_data,
            initial_sync_achieved,
        })
    }

    /// Check if the height is more than the actual tip. So if still makes sense to compute block for that height.
    async fn check_expected_tip(&mut self, height: u64) -> Result<bool, MmProxyError> {
        let tip = self
            .base_node_client
            .clone()
            .get_tip_info(grpc::Empty {})
            .await?
            .into_inner();
        let tip_height = tip.metadata.as_ref().map(|m| m.height_of_longest_chain).unwrap_or(0);

        if height <= tip_height {
            warn!(
                target: LOG_TARGET,
                "Base node received next block (height={}) that has invalidated the block template (height={})",
                tip_height,
                height
            );
            return Ok(false);
        }
        Ok(true)
    }

    /// Get coinbase transaction for the [template](NewBlockTemplateData).
    async fn get_coinbase(&mut self, template: &NewBlockTemplateData) -> Result<grpc::Transaction, MmProxyError> {
        let miner_data = &template.miner_data;
        let tari_height = template.height();
        let block_reward = miner_data.reward;
        let total_fees = miner_data.total_fees;
        let extra = self.config.coinbase_extra.as_bytes().to_vec();

        let coinbase_response = self
            .wallet_client
            .get_coinbase(grpc::GetCoinbaseRequest {
                reward: block_reward,
                fee: total_fees,
                height: tari_height,
                extra,
            })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?;
        let coinbase = coinbase_response
            .into_inner()
            .transaction
            .ok_or_else(|| MmProxyError::MissingDataError("Coinbase Invalid".to_string()))?;
        Ok(coinbase)
    }

    /// Build the [FinalBlockTemplateData] from [template](NewBlockTemplateData) and with
    /// [tari](grpc::GetNewBlockResult) and [monero data](MoneroMiningData).
    fn add_monero_data(
        &self,
        tari_block: grpc::GetNewBlockResult,
        monero_mining_data: MoneroMiningData,
        template_data: NewBlockTemplateData,
    ) -> Result<FinalBlockTemplateData, MmProxyError> {
        debug!(target: LOG_TARGET, "New block received from Tari: {:?}", tari_block);

        let tari_difficulty = template_data.miner_data.target_difficulty;
        let block_template_data = BlockTemplateDataBuilder::new()
            .tari_block(
                tari_block
                    .block
                    .ok_or(MmProxyError::GrpcResponseMissingField("block"))?,
            )
            .tari_miner_data(template_data.miner_data)
            .monero_seed(monero_mining_data.seed_hash)
            .monero_difficulty(monero_mining_data.difficulty)
            .tari_difficulty(tari_difficulty)
            .build()?;

        // Deserialize the block template blob
        debug!(target: LOG_TARGET, "Deserializing Blocktemplate Blob into Monero Block",);
        let mut monero_block = monero_rx::deserialize_monero_block_from_hex(&monero_mining_data.blocktemplate_blob)?;

        debug!(target: LOG_TARGET, "Appending Merged Mining Tag",);
        // Add the Tari merge mining tag to the retrieved block template
        monero_rx::append_merge_mining_tag(&mut monero_block, &tari_block.merge_mining_hash)?;

        debug!(target: LOG_TARGET, "Creating blockhashing blob from blocktemplate blob",);
        // Must be done after the tag is inserted since it will affect the hash of the miner tx
        let blockhashing_blob = monero_rx::create_blockhashing_blob_from_block(&monero_block)?;
        let blocktemplate_blob = monero_rx::serialize_monero_block_to_hex(&monero_block)?;

        let monero_difficulty = monero_mining_data.difficulty;
        let mining_difficulty = cmp::min(monero_difficulty, tari_difficulty);
        info!(
            target: LOG_TARGET,
            "Difficulties: Tari ({}), Monero({}), Selected({})",
            tari_difficulty,
            monero_mining_data.difficulty,
            mining_difficulty
        );
        Ok(FinalBlockTemplateData {
            template: block_template_data,
            target_difficulty: Difficulty::from_u64(mining_difficulty)?,
            blockhashing_blob,
            blocktemplate_blob,
            merge_mining_hash: tari_block.merge_mining_hash,
        })
    }
}

/// Private convenience container struct for new template data
#[allow(dead_code)]
struct NewBlockTemplateData {
    pub template: grpc::NewBlockTemplate,
    pub miner_data: grpc::MinerData,
    pub initial_sync_achieved: bool,
}

impl NewBlockTemplateData {
    pub fn height(&self) -> u64 {
        self.template.header.as_ref().map(|h| h.height).unwrap_or(0)
    }
}

/// Final outputs for required for merge mining
pub struct FinalBlockTemplateData {
    pub template: BlockTemplateData,
    pub target_difficulty: Difficulty,
    pub blockhashing_blob: String,
    pub blocktemplate_blob: String,
    pub merge_mining_hash: Vec<u8>,
}

/// Container struct for monero mining data inputs obtained from monerod
pub struct MoneroMiningData {
    pub seed_hash: FixedByteArray,
    pub blocktemplate_blob: String,
    pub difficulty: u64,
}
