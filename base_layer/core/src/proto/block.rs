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

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::{FixedHash, PrivateKey};
use tari_utilities::ByteArray;

use super::core as proto;
use crate::{
    blocks::{Block, BlockHeaderAccumulatedData, HistoricalBlock, NewBlock, NewBlockHeaderTemplate, NewBlockTemplate},
    proof_of_work::{Difficulty, ProofOfWork},
};

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

impl TryFrom<Block> for proto::Block {
    type Error = String;

    fn try_from(block: Block) -> Result<Self, Self::Error> {
        Ok(Self {
            header: Some(block.header.into()),
            body: Some(block.body.try_into()?),
        })
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

        let output_hashes: Vec<FixedHash> = historical_block
            .pruned_output_hashes
            .into_iter()
            .map(|hash| hash.try_into().map_err(|_| "Malformed pruned hash".to_string()))
            .collect::<Result<_, _>>()?;

        Ok(HistoricalBlock::new(
            block,
            historical_block.confirmations,
            accumulated_data,
            output_hashes,
            historical_block.pruned_input_count,
        ))
    }
}

impl TryFrom<HistoricalBlock> for proto::HistoricalBlock {
    type Error = String;

    fn try_from(block: HistoricalBlock) -> Result<Self, Self::Error> {
        let pruned_output_hashes = block.pruned_outputs().iter().map(|x| x.to_vec()).collect();
        let (block, accumulated_data, confirmations, pruned_input_count) = block.dissolve();
        Ok(Self {
            confirmations,
            accumulated_data: Some(accumulated_data.into()),
            block: Some(block.try_into()?),
            pruned_output_hashes,
            pruned_input_count,
        })
    }
}

impl From<BlockHeaderAccumulatedData> for proto::BlockHeaderAccumulatedData {
    fn from(source: BlockHeaderAccumulatedData) -> Self {
        Self {
            achieved_difficulty: source.achieved_difficulty.into(),
            accumulated_randomx_difficulty: source.accumulated_randomx_difficulty.into(),
            accumulated_sha_difficulty: source.accumulated_sha_difficulty.into(),
            target_difficulty: source.target_difficulty.into(),
            total_kernel_offset: source.total_kernel_offset.to_vec(),
            hash: source.hash.to_vec(),
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
        let hash = source.hash.try_into().map_err(|_| "Malformed hash".to_string())?;
        Ok(Self {
            hash,
            achieved_difficulty: Difficulty::from_u64(source.achieved_difficulty).map_err(|e| e.to_string())?,
            total_accumulated_difficulty: accumulated_difficulty,
            accumulated_randomx_difficulty: Difficulty::from_u64(source.accumulated_randomx_difficulty)
                .map_err(|e| e.to_string())?,
            accumulated_sha_difficulty: Difficulty::from_u64(source.accumulated_sha_difficulty)
                .map_err(|e| e.to_string())?,
            target_difficulty: Difficulty::from_u64(source.target_difficulty).map_err(|e| e.to_string())?,
            total_kernel_offset: PrivateKey::from_bytes(source.total_kernel_offset.as_slice())
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
            target_difficulty: Difficulty::from_u64(block_template.target_difficulty).map_err(|e| e.to_string())?,
            reward: block_template.reward.into(),
            total_fees: block_template.total_fees.into(),
        })
    }
}

impl TryFrom<NewBlockTemplate> for proto::NewBlockTemplate {
    type Error = String;

    fn try_from(block_template: NewBlockTemplate) -> Result<Self, Self::Error> {
        Ok(Self {
            header: Some(block_template.header.into()),
            body: Some(block_template.body.try_into()?),
            target_difficulty: block_template.target_difficulty.as_u64(),
            reward: block_template.reward.0,
            total_fees: block_template.total_fees.0,
        })
    }
}

//------------------------------ NewBlockHeaderTemplate ----------------------------------------//

impl TryFrom<proto::NewBlockHeaderTemplate> for NewBlockHeaderTemplate {
    type Error = String;

    fn try_from(header: proto::NewBlockHeaderTemplate) -> Result<Self, Self::Error> {
        let total_kernel_offset = PrivateKey::from_bytes(&header.total_kernel_offset).map_err(|err| err.to_string())?;
        let total_script_offset = PrivateKey::from_bytes(&header.total_script_offset).map_err(|err| err.to_string())?;
        let pow = match header.pow {
            Some(p) => ProofOfWork::try_from(p)?,
            None => return Err("No proof of work provided".into()),
        };
        let prev_hash = header
            .prev_hash
            .try_into()
            .map_err(|_| "Malformed prev block hash".to_string())?;
        Ok(Self {
            version: u16::try_from(header.version).map_err(|err| err.to_string())?,
            height: header.height,
            prev_hash,
            total_kernel_offset,
            total_script_offset,
            pow,
        })
    }
}

impl From<NewBlockHeaderTemplate> for proto::NewBlockHeaderTemplate {
    fn from(header: NewBlockHeaderTemplate) -> Self {
        Self {
            version: u32::try_from(header.version).unwrap(),
            height: header.height,
            prev_hash: header.prev_hash.to_vec(),
            total_kernel_offset: header.total_kernel_offset.to_vec(),
            total_script_offset: header.total_script_offset.to_vec(),
            pow: Some(proto::ProofOfWork::from(header.pow)),
        }
    }
}

//---------------------------------- NewBlock --------------------------------------------//

impl TryFrom<proto::NewBlock> for NewBlock {
    type Error = String;

    fn try_from(new_block: proto::NewBlock) -> Result<Self, Self::Error> {
        Ok(Self {
            header: new_block.header.ok_or("No new block header provided")?.try_into()?,
            coinbase_kernel: new_block
                .coinbase_kernel
                .ok_or("No coinbase kernel given")?
                .try_into()?,
            coinbase_output: new_block
                .coinbase_output
                .ok_or("No coinbase kernel given")?
                .try_into()?,
            kernel_excess_sigs: new_block
                .kernel_excess_sigs
                .iter()
                .map(|bytes| PrivateKey::from_bytes(bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Invalid excess signature scalar")?,
        })
    }
}

impl TryFrom<NewBlock> for proto::NewBlock {
    type Error = String;

    fn try_from(new_block: NewBlock) -> Result<Self, Self::Error> {
        Ok(Self {
            header: Some(new_block.header.into()),
            coinbase_kernel: Some(new_block.coinbase_kernel.into()),
            coinbase_output: Some(new_block.coinbase_output.try_into()?),
            kernel_excess_sigs: new_block.kernel_excess_sigs.into_iter().map(|s| s.to_vec()).collect(),
        })
    }
}
