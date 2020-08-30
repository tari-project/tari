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
    chain_storage::HistoricalBlock,
    proof_of_work::{Difficulty, PowAlgorithm, ProofOfWork},
    proto::utils::try_convert_all,
    transactions::types::{BlindingFactor, BLOCK_HASH_LENGTH},
};
use prost_types::Timestamp;
use std::convert::{TryFrom, TryInto};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, ByteArray, ByteArrayError};

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
            kernel_mr: header.kernel_mr,
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
        }
    }
}

//---------------------------------- ProofOfWork --------------------------------------------//

impl TryFrom<proto::ProofOfWork> for ProofOfWork {
    type Error = String;

    fn try_from(pow: proto::ProofOfWork) -> Result<Self, Self::Error> {
        Ok(Self {
            pow_algo: PowAlgorithm::try_from(pow.pow_algo)?,
            accumulated_monero_difficulty: Difficulty::from(pow.accumulated_monero_difficulty),
            accumulated_blake_difficulty: Difficulty::from(pow.accumulated_blake_difficulty),
            target_difficulty: Difficulty::from(pow.target_difficulty),
            pow_data: pow.pow_data,
        })
    }
}

impl From<ProofOfWork> for proto::ProofOfWork {
    fn from(pow: ProofOfWork) -> Self {
        Self {
            pow_algo: pow.pow_algo as u64,
            accumulated_monero_difficulty: pow.accumulated_monero_difficulty.as_u64(),
            accumulated_blake_difficulty: pow.accumulated_blake_difficulty.as_u64(),
            target_difficulty: pow.target_difficulty.as_u64(),
            pow_data: pow.pow_data,
        }
    }
}

//---------------------------------- HistoricalBlock --------------------------------------------//

impl TryFrom<proto::HistoricalBlock> for HistoricalBlock {
    type Error = String;

    fn try_from(historical_block: proto::HistoricalBlock) -> Result<Self, Self::Error> {
        let spent_commitments =
            try_convert_all(historical_block.spent_commitments).map_err(|err: ByteArrayError| err.to_string())?;

        let block = historical_block
            .block
            .map(TryInto::try_into)
            .ok_or_else(|| "block in historical block not provided".to_string())??;

        Ok(Self {
            confirmations: historical_block.confirmations,
            spent_commitments,
            block,
        })
    }
}

impl From<HistoricalBlock> for proto::HistoricalBlock {
    fn from(block: HistoricalBlock) -> Self {
        Self {
            confirmations: block.confirmations,
            spent_commitments: block.spent_commitments.into_iter().map(Into::into).collect(),
            block: Some(block.block.into()),
        }
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

        Ok(Self { header, body })
    }
}

impl From<NewBlockTemplate> for proto::NewBlockTemplate {
    fn from(block_template: NewBlockTemplate) -> Self {
        Self {
            header: Some(block_template.header.into()),
            body: Some(block_template.body.into()),
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
