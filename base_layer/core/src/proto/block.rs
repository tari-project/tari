// Copyright 2019, The Tari Project
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

use super::core as proto;
use crate::{
    blocks::{Block, BlockHeader, NewBlock, NewBlockHeaderTemplate, NewBlockTemplate},
    chain_storage::{BlockHeaderAccumulatedData, HistoricalBlock},
    proof_of_work::{PowAlgorithm, ProofOfWork},
    transactions::types::BlindingFactor,
};
use prost_types::Timestamp;
use std::convert::{TryFrom, TryInto};
use tari_common_types::types::BLOCK_HASH_LENGTH;
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray};

/// Utility function that converts a `prost::Timestamp` to a `chrono::DateTime`
pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> EpochTime {
    (timestamp.seconds as u64).into()
}

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
pub(crate) fn datetime_to_timestamp(datetime: EpochTime) -> Timestamp {
    Timestamp {
        seconds: datetime.as_u64() as i64,
        nanos: 0,
    }
}

//---------------------------------- Block --------------------------------------------//

impl TryFrom<proto::Block> for Block {
    type Error = String;

    fn try_from(block: proto::Block) -> Result<Self, Self::Error> {
        let header = block
            .header
            .map(TryInto::try_into)
            .ok_or_else(|| "Block header not provided".to_string())??;

        let body = block
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Block body not provided".to_string())??;

        Ok(Self { header, body })
    }
}

impl From<Block> for proto::Block {
    fn from(block: Block) -> Self {
        Self {
            header: Some(block.header.into()),
            body: Some(block.body.into()),
        }
    }
}

//---------------------------------- BlockHeader --------------------------------------------//

impl TryFrom<proto::BlockHeader> for BlockHeader {
    type Error = String;

    fn try_from(header: proto::BlockHeader) -> Result<Self, Self::Error> {
        let total_kernel_offset =
            BlindingFactor::from_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;

        let timestamp = header
            .timestamp
            .map(timestamp_to_datetime)
            .ok_or_else(|| "timestamp not provided".to_string())?;

        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        Ok(Self {
            version: header.version as u16,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp,
            output_mr: header.output_mr,
            range_proof_mr: header.range_proof_mr,
            output_mmr_size: header.output_mmr_size,
            kernel_mr: header.kernel_mr,
            kernel_mmr_size: header.kernel_mmr_size,
            total_kernel_offset,
            nonce: header.nonce,
            pow,
        })
    }
}

impl From<BlockHeader> for proto::BlockHeader {
    fn from(header: BlockHeader) -> Self {
        Self {
            version: header.version as u32,
            height: header.height,
            prev_hash: header.prev_hash,
            timestamp: Some(datetime_to_timestamp(header.timestamp)),
            output_mr: header.output_mr,
            range_proof_mr: header.range_proof_mr,
            kernel_mr: header.kernel_mr,
            total_kernel_offset: header.total_kernel_offset.to_vec(),
            nonce: header.nonce,
            pow: Some(proto::ProofOfWork::from(header.pow)),
            kernel_mmr_size: header.kernel_mmr_size,
            output_mmr_size: header.output_mmr_size,
        }
    }
}

//---------------------------------- ProofOfWork --------------------------------------------//
#[allow(deprecated)]
impl TryFrom<proto::ProofOfWork> for ProofOfWork {
    type Error = String;

    fn try_from(pow: proto::ProofOfWork) -> Result<Self, Self::Error> {
        Ok(Self {
            pow_algo: PowAlgorithm::try_from(pow.pow_algo)?,
            pow_data: pow.pow_data,
        })
    }
}

#[allow(deprecated)]
impl From<ProofOfWork> for proto::ProofOfWork {
    fn from(pow: ProofOfWork) -> Self {
        Self {
            pow_algo: pow.pow_algo as u64,
            pow_data: pow.pow_data,
        }
    }
}

//---------------------------------- HistoricalBlock --------------------------------------------//

impl TryFrom<proto::HistoricalBlock> for HistoricalBlock {
    type Error = String;

    fn try_from(historical_block: proto::HistoricalBlock) -> Result<Self, Self::Error> {
        let block = historical_block
            .block
            .map(TryInto::try_into)
            .ok_or_else(|| "block in historical block not provided".to_string())??;

        let accumulated_data = historical_block
            .accumulated_data
            .map(TryInto::try_into)
            .ok_or_else(|| "accumulated_data in historical block not provided".to_string())??;

        let pruned = historical_block
            .pruned_output_hashes
            .into_iter()
            .zip(historical_block.pruned_proof_hashes)
            .collect();

        Ok(HistoricalBlock::new(
            block,
            historical_block.confirmations,
            accumulated_data,
            pruned,
            historical_block.pruned_input_count,
        ))
    }
}

impl From<HistoricalBlock> for proto::HistoricalBlock {
    fn from(block: HistoricalBlock) -> Self {
        let pruned_output_hashes = block.pruned_outputs().iter().map(|x| x.0.clone()).collect();
        let pruned_proof_hashes = block.pruned_outputs().iter().map(|x| x.1.clone()).collect();
        let (block, accumulated_data, confirmations, pruned_input_count) = block.dissolve();
        Self {
            confirmations,
            accumulated_data: Some(accumulated_data.into()),
            block: Some(block.into()),
            pruned_output_hashes,
            pruned_proof_hashes,
            pruned_input_count,
        }
    }
}

impl From<BlockHeaderAccumulatedData> for proto::BlockHeaderAccumulatedData {
    fn from(source: BlockHeaderAccumulatedData) -> Self {
        Self {
            achieved_difficulty: source.achieved_difficulty.into(),
            accumulated_monero_difficulty: source.accumulated_monero_difficulty.into(),
            accumulated_blake_difficulty: source.accumulated_blake_difficulty.into(),
            target_difficulty: source.target_difficulty.into(),
            total_kernel_offset: source.total_kernel_offset.to_vec(),
            hash: source.hash,
            total_accumulated_difficulty: Vec::from(source.total_accumulated_difficulty.to_le_bytes()),
        }
    }
}

impl TryFrom<proto::BlockHeaderAccumulatedData> for BlockHeaderAccumulatedData {
    type Error = String;

    fn try_from(source: proto::BlockHeaderAccumulatedData) -> Result<Self, Self::Error> {
        let mut acc_diff = [0; 16];
        acc_diff.copy_from_slice(&source.total_accumulated_difficulty[0..16]);
        let accumulated_difficulty = u128::from_le_bytes(acc_diff);

        Ok(Self {
            hash: source.hash,
            achieved_difficulty: source.achieved_difficulty.into(),
            total_accumulated_difficulty: accumulated_difficulty,
            accumulated_monero_difficulty: source.accumulated_monero_difficulty.into(),
            accumulated_blake_difficulty: source.accumulated_blake_difficulty.into(),
            target_difficulty: source.target_difficulty.into(),
            total_kernel_offset: BlindingFactor::from_bytes(source.total_kernel_offset.as_slice())
                .map_err(|err| format!("Invalid value for total_kernel_offset: {}", err))?,
        })
    }
}

//--------------------------------- NewBlockTemplate -------------------------------------------//

impl TryFrom<proto::NewBlockTemplate> for NewBlockTemplate {
    type Error = String;

    fn try_from(block_template: proto::NewBlockTemplate) -> Result<Self, Self::Error> {
        let header = block_template
            .header
            .map(TryInto::try_into)
            .ok_or_else(|| "Block header template not provided".to_string())??;

        let body = block_template
            .body
            .map(TryInto::try_into)
            .ok_or_else(|| "Block body not provided".to_string())??;

        Ok(Self {
            header,
            body,
            target_difficulty: block_template.target_difficulty.into(),
            reward: block_template.reward.into(),
            total_fees: block_template.total_fees.into(),
        })
    }
}

impl From<NewBlockTemplate> for proto::NewBlockTemplate {
    fn from(block_template: NewBlockTemplate) -> Self {
        Self {
            header: Some(block_template.header.into()),
            body: Some(block_template.body.into()),
            target_difficulty: block_template.target_difficulty.as_u64(),
            reward: block_template.reward.0,
            total_fees: block_template.total_fees.0,
        }
    }
}

//------------------------------ NewBlockHeaderTemplate ----------------------------------------//

impl TryFrom<proto::NewBlockHeaderTemplate> for NewBlockHeaderTemplate {
    type Error = String;

    fn try_from(header: proto::NewBlockHeaderTemplate) -> Result<Self, Self::Error> {
        let total_kernel_offset =
            BlindingFactor::from_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;
        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        Ok(Self {
            version: header.version as u16,
            height: header.height,
            prev_hash: header.prev_hash,
            total_kernel_offset,
            pow,
        })
    }
}

impl From<NewBlockHeaderTemplate> for proto::NewBlockHeaderTemplate {
    fn from(header: NewBlockHeaderTemplate) -> Self {
        Self {
            version: header.version as u32,
            height: header.height,
            prev_hash: header.prev_hash,
            total_kernel_offset: header.total_kernel_offset.to_vec(),
            pow: Some(proto::ProofOfWork::from(header.pow)),
        }
    }
}

//---------------------------------- NewBlock --------------------------------------------//

impl TryFrom<proto::NewBlock> for NewBlock {
    type Error = String;

    fn try_from(new_block: proto::NewBlock) -> Result<Self, Self::Error> {
        let block_hash = new_block.block_hash;
        if block_hash.len() != BLOCK_HASH_LENGTH {
            return Err(format!(
                "Block hash has an incorrect length. (len={}, expected={})",
                block_hash.len(),
                BLOCK_HASH_LENGTH
            ));
        }

        Ok(Self { block_hash })
    }
}

impl From<NewBlock> for proto::NewBlock {
    fn from(new_block: NewBlock) -> Self {
        Self {
            block_hash: new_block.block_hash,
        }
    }
}
