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

use std::{
    convert::{TryFrom, TryInto},
    mem,
};

use primitive_types::U256;
use tari_common_types::types::PrivateKey;
use tari_utilities::ByteArray;

use super::core as proto;
use crate::{
    blocks::{Block, BlockHeaderAccumulatedData, HistoricalBlock, NewBlock},
    proof_of_work::{AccumulatedDifficulty, Difficulty},
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

        Ok(HistoricalBlock::new(
            block,
            historical_block.confirmations,
            accumulated_data,
        ))
    }
}

impl TryFrom<HistoricalBlock> for proto::HistoricalBlock {
    type Error = String;

    fn try_from(block: HistoricalBlock) -> Result<Self, Self::Error> {
        let (block, accumulated_data, confirmations) = block.dissolve();
        Ok(Self {
            confirmations,
            accumulated_data: Some(accumulated_data.into()),
            block: Some(block.try_into()?),
        })
    }
}

impl From<BlockHeaderAccumulatedData> for proto::BlockHeaderAccumulatedData {
    fn from(source: BlockHeaderAccumulatedData) -> Self {
        let accumulated_randomx_difficulty = source.accumulated_randomx_difficulty.to_be_bytes();
        let accumulated_sha3x_difficulty = source.accumulated_sha3x_difficulty.to_be_bytes();
        let mut total_accumulated_difficulty = [0u8; 32];
        source
            .total_accumulated_difficulty
            .to_big_endian(&mut total_accumulated_difficulty);
        Self {
            achieved_difficulty: source.achieved_difficulty.into(),
            accumulated_randomx_difficulty,
            accumulated_sha3x_difficulty,
            target_difficulty: source.target_difficulty.into(),
            total_kernel_offset: source.total_kernel_offset.to_vec(),
            hash: source.hash.to_vec(),
            total_accumulated_difficulty: total_accumulated_difficulty.to_vec(),
        }
    }
}

impl TryFrom<proto::BlockHeaderAccumulatedData> for BlockHeaderAccumulatedData {
    type Error = String;

    fn try_from(source: proto::BlockHeaderAccumulatedData) -> Result<Self, Self::Error> {
        const TOTAL_ACC_DIFFICULTY_ARRAY_LEN: usize = 32;
        if source.total_accumulated_difficulty.len() != TOTAL_ACC_DIFFICULTY_ARRAY_LEN {
            return Err(format!(
                "Invalid accumulated difficulty byte length. {} was expected but the actual length was {}",
                TOTAL_ACC_DIFFICULTY_ARRAY_LEN,
                source.total_accumulated_difficulty.len()
            ));
        }
        let mut acc_diff = [0u8; TOTAL_ACC_DIFFICULTY_ARRAY_LEN];
        acc_diff.copy_from_slice(&source.total_accumulated_difficulty[0..TOTAL_ACC_DIFFICULTY_ARRAY_LEN]);
        let accumulated_difficulty = U256::from_big_endian(&acc_diff);

        const SINGLE_ACC_DIFFICULTY_ARRAY_LEN: usize = mem::size_of::<u128>();
        if source.accumulated_sha3x_difficulty.len() != SINGLE_ACC_DIFFICULTY_ARRAY_LEN {
            return Err(format!(
                "Invalid accumulated Sha3x difficulty byte length. {} was expected but the actual length was {}",
                SINGLE_ACC_DIFFICULTY_ARRAY_LEN,
                source.accumulated_sha3x_difficulty.len()
            ));
        }
        let mut acc_diff = [0; SINGLE_ACC_DIFFICULTY_ARRAY_LEN];
        acc_diff.copy_from_slice(&source.accumulated_randomx_difficulty[0..SINGLE_ACC_DIFFICULTY_ARRAY_LEN]);
        let accumulated_sha3x_difficulty = u128::from_be_bytes(acc_diff);

        if source.accumulated_randomx_difficulty.len() != SINGLE_ACC_DIFFICULTY_ARRAY_LEN {
            return Err(format!(
                "Invalid accumulated RandomX difficulty byte length. {} was expected but the actual length was {}",
                SINGLE_ACC_DIFFICULTY_ARRAY_LEN,
                source.accumulated_randomx_difficulty.len()
            ));
        }
        let mut acc_diff = [0; SINGLE_ACC_DIFFICULTY_ARRAY_LEN];
        acc_diff.copy_from_slice(&source.accumulated_randomx_difficulty[0..SINGLE_ACC_DIFFICULTY_ARRAY_LEN]);
        let accumulated_randomx_difficulty = u128::from_be_bytes(acc_diff);

        let hash = source.hash.try_into().map_err(|_| "Malformed hash".to_string())?;
        Ok(Self {
            hash,
            achieved_difficulty: Difficulty::from_u64(source.achieved_difficulty).map_err(|e| e.to_string())?,
            total_accumulated_difficulty: accumulated_difficulty,
            accumulated_randomx_difficulty: AccumulatedDifficulty::from_u128(accumulated_randomx_difficulty)
                .map_err(|e| e.to_string())?,
            accumulated_sha3x_difficulty: AccumulatedDifficulty::from_u128(accumulated_sha3x_difficulty)
                .map_err(|e| e.to_string())?,
            target_difficulty: Difficulty::from_u64(source.target_difficulty).map_err(|e| e.to_string())?,
            total_kernel_offset: PrivateKey::from_canonical_bytes(source.total_kernel_offset.as_slice())
                .map_err(|err| format!("Invalid value for total_kernel_offset: {}", err))?,
        })
    }
}

//---------------------------------- NewBlock --------------------------------------------//

impl TryFrom<proto::NewBlock> for NewBlock {
    type Error = String;

    fn try_from(new_block: proto::NewBlock) -> Result<Self, Self::Error> {
        let mut coinbase_kernels = Vec::new();
        for coinbase_kernel in new_block.coinbase_kernels {
            coinbase_kernels.push(coinbase_kernel.try_into()?)
        }
        let mut coinbase_outputs = Vec::new();
        for coinbase_output in new_block.coinbase_outputs {
            coinbase_outputs.push(coinbase_output.try_into()?)
        }
        Ok(Self {
            header: new_block.header.ok_or("No new block header provided")?.try_into()?,
            coinbase_kernels,
            coinbase_outputs,
            kernel_excess_sigs: new_block
                .kernel_excess_sigs
                .iter()
                .map(|bytes| PrivateKey::from_canonical_bytes(bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Invalid excess signature scalar")?,
        })
    }
}

impl TryFrom<NewBlock> for proto::NewBlock {
    type Error = String;

    fn try_from(new_block: NewBlock) -> Result<Self, Self::Error> {
        let mut coinbase_kernels = Vec::new();
        for coinbase_kernel in new_block.coinbase_kernels {
            coinbase_kernels.push(coinbase_kernel.into())
        }
        let mut coinbase_outputs = Vec::new();
        for coinbase_output in new_block.coinbase_outputs {
            coinbase_outputs.push(coinbase_output.try_into()?)
        }
        Ok(Self {
            header: Some(new_block.header.into()),
            coinbase_kernels,
            coinbase_outputs,
            kernel_excess_sigs: new_block.kernel_excess_sigs.into_iter().map(|s| s.to_vec()).collect(),
        })
    }
}
