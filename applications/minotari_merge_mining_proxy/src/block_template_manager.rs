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
use std::{cmp, convert::TryFrom, sync::Arc};

use log::*;
use minotari_app_grpc::tari_rpc::{pow_algo::PowAlgos, GetNewBlockRequest, MinerData, NewBlockTemplate, PowAlgo};
use minotari_app_utilities::parse_miner_input::{BaseNodeGrpcClient, ShaP2PoolGrpcClient};
use minotari_node_grpc_client::grpc;
use tari_common_types::{tari_address::TariAddress, types::FixedHash};
use tari_core::{
    consensus::ConsensusManager,
    proof_of_work::{monero_rx, monero_rx::FixedByteArray, Difficulty},
    transactions::{
        generate_coinbase,
        transaction_components::{
            payment_id::{PaymentId, TxType},
            CoinBaseExtra,
            TransactionKernel,
            TransactionOutput,
        },
        transaction_key_manager::{create_memory_db_key_manager, MemoryDbKeyManager},
    },
    AuxChainHashes,
};
use tari_max_size::MaxSizeBytes;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    block_template_data::{BlockTemplateData, BlockTemplateDataBuilder},
    common::merge_mining,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::block_template_protocol";

/// Structure holding grpc connections.
pub(crate) struct BlockTemplateManager<'a> {
    config: Arc<MergeMiningProxyConfig>,
    base_node_client: &'a mut BaseNodeGrpcClient,
    p2pool_client: Option<ShaP2PoolGrpcClient>,
    key_manager: MemoryDbKeyManager,
    wallet_payment_address: TariAddress,
    consensus_manager: ConsensusManager,
}

impl<'a> BlockTemplateManager<'a> {
    pub fn try_create(
        base_node_client: &'a mut BaseNodeGrpcClient,
        p2pool_client: Option<ShaP2PoolGrpcClient>,
        config: Arc<MergeMiningProxyConfig>,
        consensus_manager: ConsensusManager,
        wallet_payment_address: TariAddress,
    ) -> Result<BlockTemplateManager<'a>, MmProxyError> {
        let key_manager = create_memory_db_key_manager()?;
        Ok(Self {
            config,
            base_node_client,
            p2pool_client,
            key_manager,
            wallet_payment_address,
            consensus_manager,
        })
    }
}

impl BlockTemplateManager<'_> {
    /// Create [FinalBlockTemplateData] with [MoneroMiningData].
    pub async fn get_next_tari_block_template(
        mut self,
        monero_mining_data: MoneroMiningData,
    ) -> Result<FinalBlockTemplateData, MmProxyError> {
        let block = match self.p2pool_client.as_mut() {
            Some(client) => {
                let pow_algo = PowAlgo {
                    pow_algo: PowAlgos::Randomxm.into(),
                };
                let coinbase_extra = if self.config.coinbase_extra.trim().is_empty() {
                    String::new()
                } else {
                    self.config.coinbase_extra.clone()
                };
                let block_result = client
                    .get_new_block(GetNewBlockRequest {
                        pow: Some(pow_algo),
                        coinbase_extra,
                        wallet_payment_address: self.wallet_payment_address.to_base58(),
                    })
                    .await?
                    .into_inner();
                block_result
                    .block
                    .ok_or_else(|| MmProxyError::FailedToGetBlockTemplate("block result".to_string()))?
            },
            None => {
                let (block_template_with_coinbase, height) = self.create_new_block_template().await?;

                match self.get_new_block(block_template_with_coinbase).await {
                    Ok(b) => {
                        debug!(
                            target: LOG_TARGET,
                            "Requested new block at height: #{} , block hash: `{}`",
                            height,
                            {
                                let block_header = b.block.as_ref().map(|b| b.header.as_ref()).unwrap_or_default();
                                block_header.map(|h| h.hash.clone()).unwrap_or_default().to_hex()
                            },
                        );
                        b
                    },
                    Err(err) => {
                        error!(target: LOG_TARGET, "grpc get_new_block ({})", err.to_string());
                        return Err(err);
                    },
                }
            },
        };

        let miner_data = block
            .miner_data
            .as_ref()
            .copied()
            .ok_or_else(|| MmProxyError::GrpcResponseMissingField("miner_data"))?;

        add_monero_data(block, monero_mining_data.clone(), miner_data)
    }

    async fn create_new_block_template(&mut self) -> Result<(NewBlockTemplate, u64), MmProxyError> {
        let mut loop_count = 0;
        loop {
            let new_template = match self.get_new_block_template().await {
                Ok(val) => {
                    if !val.template.is_mempool_in_sync {
                        warn!(target: LOG_TARGET, "Mempool is not in sync.");

                        if loop_count > 3 {
                            return Err(MmProxyError::FailedToGetBlockTemplate(
                                "mempool not in sync".to_string(),
                            ));
                        } else {
                            loop_count += 1;
                            continue;
                        }
                    }
                    val
                },
                Err(err) => {
                    error!(target: LOG_TARGET, "grpc get_new_block_template ({})", err.to_string());
                    return Err(err);
                },
            };
            let height = new_template
                .template
                .header
                .as_ref()
                .map(|h| h.height)
                .unwrap_or_default();
            debug!(target: LOG_TARGET, "Requested new block template at height: #{} (try {})", height, loop_count);
            let (coinbase_output, coinbase_kernel) = self.get_coinbase(&new_template).await?;

            let template_with_coinbase =
                merge_mining::add_coinbase(&coinbase_output, &coinbase_kernel, new_template.template.clone())?;
            debug!(target: LOG_TARGET, "Added coinbase to new block template (try {})", loop_count);

            return Ok((template_with_coinbase, height));
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
            initial_sync_achieved: _,
        } = self
            .base_node_client
            .get_new_block_template(grpc::NewBlockTemplateRequest {
                algo: Some(grpc::PowAlgo {
                    pow_algo: grpc::pow_algo::PowAlgos::Randomxm.into(),
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
        Ok(NewBlockTemplateData { template, miner_data })
    }

    /// Get coinbase transaction for the [template](NewBlockTemplateData).
    async fn get_coinbase(
        &mut self,
        template: &NewBlockTemplateData,
    ) -> Result<(TransactionOutput, TransactionKernel), MmProxyError> {
        let miner_data = &template.miner_data;
        let tari_height = template.height();
        let block_reward = miner_data.reward;
        let total_fees = miner_data.total_fees;

        let (coinbase_output, coinbase_kernel) = generate_coinbase(
            total_fees.into(),
            block_reward.into(),
            tari_height,
            &CoinBaseExtra::try_from(self.config.coinbase_extra.as_bytes().to_vec())?,
            &self.key_manager,
            &self.wallet_payment_address,
            true,
            self.consensus_manager.consensus_constants(tari_height),
            self.config.range_proof_type,
            PaymentId::Open {
                user_data: vec![],
                tx_type: TxType::Coinbase,
            },
        )
        .await?;
        Ok((coinbase_output, coinbase_kernel))
    }
}

/// This is an interim solution to calculate the merkle root for the aux chains when multiple aux chains will be
/// merge mined with Monero. It needs to be replaced with a more general solution in the future.
pub fn calculate_aux_chain_merkle_root(hashes: AuxChainHashes) -> Result<(monero::Hash, u32), MmProxyError> {
    if hashes.is_empty() {
        Err(MmProxyError::MissingDataError(
            "No aux chain hashes provided".to_string(),
        ))
    } else if hashes.len() == 1 {
        Ok((hashes[0], 0))
    } else {
        unimplemented!("Multiple aux chains for Monero is not supported yet, only Tari.");
    }
}

/// Build the [FinalBlockTemplateData] from [template](NewBlockTemplateData) and with
/// [tari](grpc::GetNewBlockResult) and [monero data](MoneroMiningData).
fn add_monero_data(
    tari_block_result: grpc::GetNewBlockResult,
    monero_mining_data: MoneroMiningData,
    miner_data: MinerData,
) -> Result<FinalBlockTemplateData, MmProxyError> {
    let merge_mining_hash = FixedHash::try_from(tari_block_result.merge_mining_hash.clone())
        .map_err(|e| MmProxyError::ConversionError(e.to_string()))?;

    let aux_chain_hashes = AuxChainHashes::try_from(vec![monero::Hash::from_slice(merge_mining_hash.as_slice())])?;
    let tari_difficulty = miner_data.target_difficulty;
    let block_template_data = BlockTemplateDataBuilder::new()
        .tari_block(
            tari_block_result
                .block
                .ok_or(MmProxyError::GrpcResponseMissingField("block"))?,
        )
        .tari_miner_data(miner_data)
        .monero_seed(monero_mining_data.seed_hash)
        .monero_difficulty(monero_mining_data.difficulty)
        .tari_difficulty(tari_difficulty)
        .tari_merge_mining_hash(merge_mining_hash)
        .aux_hashes(aux_chain_hashes.clone())
        .build()?;

    // Deserialize the block template blob
    debug!(target: LOG_TARGET, "Deseriale Monero block template blob into Monero block",);
    let mut monero_block = monero_rx::deserialize_monero_block_from_hex(&monero_mining_data.blocktemplate_blob)?;

    debug!(target: LOG_TARGET, "Insert aux chain merkle root (merge_mining_hash) into Monero block");
    let aux_chain_mr = calculate_aux_chain_merkle_root(aux_chain_hashes.clone())?.0;
    monero_rx::insert_aux_chain_mr_and_info_into_block(&mut monero_block, aux_chain_mr.to_bytes(), 1, 0)?;

    debug!(target: LOG_TARGET, "Create blockhashing blob from blocktemplate blob",);
    // Must be done after the aux_chain_mr is inserted since it will affect the hash of the miner tx
    let blockhashing_blob = monero_rx::create_blockhashing_blob_from_block(&monero_block)?;
    let blocktemplate_blob = monero_rx::serialize_monero_block_to_hex(&monero_block)?;

    let monero_difficulty = monero_mining_data.difficulty;
    let mining_difficulty = cmp::min(monero_difficulty, tari_difficulty);
    info!(
        target: LOG_TARGET,
        "Difficulties: Minotari ({}), Monero({}), Selected({})",
        tari_difficulty,
        monero_mining_data.difficulty,
        mining_difficulty
    );

    Ok(FinalBlockTemplateData {
        template: block_template_data,
        target_difficulty: Difficulty::from_u64(mining_difficulty)?,
        blockhashing_blob,
        blocktemplate_blob,
        aux_chain_hashes,
        aux_chain_mr: AuxChainMr::try_from(aux_chain_mr.to_bytes().to_vec())
            .map_err(|e| MmProxyError::ConversionError(e.to_string()))?,
    })
}

/// Private convenience container struct for new template data
#[derive(Debug, Clone)]
pub(crate) struct NewBlockTemplateData {
    pub template: grpc::NewBlockTemplate,
    pub miner_data: grpc::MinerData,
}

impl NewBlockTemplateData {
    pub fn height(&self) -> u64 {
        self.template.header.as_ref().map(|h| h.height).unwrap_or(0)
    }
}

/// The AuxChainMerkleRoot is a 32 byte hash
pub(crate) type AuxChainMr = MaxSizeBytes<32>;
/// Final outputs for required for merge mining
#[derive(Debug, Clone)]
pub(crate) struct FinalBlockTemplateData {
    pub template: BlockTemplateData,
    pub target_difficulty: Difficulty,
    pub blockhashing_blob: String,
    pub blocktemplate_blob: String,
    pub aux_chain_hashes: AuxChainHashes,
    pub aux_chain_mr: AuxChainMr,
}

/// Container struct for monero mining data inputs obtained from monerod
#[derive(Clone)]
pub(crate) struct MoneroMiningData {
    pub seed_hash: FixedByteArray,
    pub blocktemplate_blob: String,
    pub difficulty: u64,
}
