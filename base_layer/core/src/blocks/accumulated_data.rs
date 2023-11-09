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
use std::{
    fmt,
    fmt::{Display, Formatter},
    sync::Arc,
};

use log::*;
use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, HashOutput, PrivateKey};
use tari_mmr::{pruned_hashset::PrunedHashSet, ArrayLike};
use tari_utilities::hex::Hex;

use crate::{
    blocks::{error::BlockError, Block, BlockHeader},
    proof_of_work::{AccumulatedDifficulty, AchievedTargetDifficulty, Difficulty, PowAlgorithm},
    transactions::aggregated_body::AggregateBody,
};

const LOG_TARGET: &str = "c::bn::acc_data";

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct BlockAccumulatedData {
    pub(crate) kernels: PrunedHashSet,
    pub(crate) kernel_sum: Commitment,
}

impl BlockAccumulatedData {
    pub fn new(kernels: PrunedHashSet, total_kernel_sum: Commitment) -> Self {
        Self {
            kernels,
            kernel_sum: total_kernel_sum,
        }
    }

    pub fn dissolve(self) -> PrunedHashSet {
        self.kernels
    }

    pub fn kernel_sum(&self) -> &Commitment {
        &self.kernel_sum
    }
}

impl Display for BlockAccumulatedData {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{} hashes in kernel MMR,", self.kernels.len().unwrap_or(0))
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateBlockAccumulatedData {
    pub kernel_hash_set: Option<PrunedHashSet>,
    pub kernel_sum: Option<Commitment>,
}

pub struct BlockHeaderAccumulatedDataBuilder<'a> {
    previous_accum: &'a BlockHeaderAccumulatedData,
    hash: Option<HashOutput>,
    current_total_kernel_offset: Option<PrivateKey>,
    current_achieved_target: Option<AchievedTargetDifficulty>,
}

impl<'a> BlockHeaderAccumulatedDataBuilder<'a> {
    pub fn from_previous(previous_accum: &'a BlockHeaderAccumulatedData) -> Self {
        Self {
            previous_accum,
            hash: None,
            current_total_kernel_offset: None,
            current_achieved_target: None,
        }
    }
}

impl BlockHeaderAccumulatedDataBuilder<'_> {
    pub fn with_hash(mut self, hash: HashOutput) -> Self {
        self.hash = Some(hash);
        self
    }

    pub fn with_total_kernel_offset(mut self, current_offset: PrivateKey) -> Self {
        self.current_total_kernel_offset = Some(current_offset);
        self
    }

    pub fn with_achieved_target_difficulty(mut self, achieved_target: AchievedTargetDifficulty) -> Self {
        self.current_achieved_target = Some(achieved_target);
        self
    }

    pub fn build(self) -> Result<BlockHeaderAccumulatedData, BlockError> {
        let previous_accum = self.previous_accum;
        let hash = self.hash.ok_or(BlockError::BuilderMissingField { field: "hash" })?;

        if hash == previous_accum.hash {
            return Err(BlockError::BuilderInvalidValue {
                field: "hash",
                details: "Hash was set to the same hash that is contained in previous accumulated data".to_string(),
            });
        }

        let achieved_target = self.current_achieved_target.ok_or(BlockError::BuilderMissingField {
            field: "Current achieved difficulty",
        })?;

        let (randomx_diff, sha3x_diff) = match achieved_target.pow_algo() {
            PowAlgorithm::RandomX => (
                previous_accum
                    .accumulated_randomx_difficulty
                    .checked_add_difficulty(achieved_target.achieved())
                    .ok_or(BlockError::DifficultyOverflow)?,
                previous_accum.accumulated_sha3x_difficulty,
            ),
            PowAlgorithm::Sha3x => (
                previous_accum.accumulated_randomx_difficulty,
                previous_accum
                    .accumulated_sha3x_difficulty
                    .checked_add_difficulty(achieved_target.achieved())
                    .ok_or(BlockError::DifficultyOverflow)?,
            ),
        };

        let total_kernel_offset = self
            .current_total_kernel_offset
            .map(|offset| &previous_accum.total_kernel_offset + offset)
            .ok_or(BlockError::BuilderMissingField {
                field: "total_kernel_offset",
            })?;

        let result = BlockHeaderAccumulatedData {
            hash,
            total_kernel_offset,
            achieved_difficulty: achieved_target.achieved(),
            total_accumulated_difficulty: U512::from(randomx_diff.as_u256()) * U512::from(sha3x_diff.as_u256()),
            accumulated_randomx_difficulty: randomx_diff,
            accumulated_sha3x_difficulty: sha3x_diff,
            target_difficulty: achieved_target.target(),
        };
        trace!(
            target: LOG_TARGET,
            "Calculated: Tot_acc_diff {}, RandomX {}, SHA3 {}",
            result.total_accumulated_difficulty,
            result.accumulated_randomx_difficulty,
            result.accumulated_sha3x_difficulty,
        );
        Ok(result)
    }
}

/// Accumulated and other pertinent data in the block header acting as a "condensed blockchain snapshot" for the block
#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
pub struct BlockHeaderAccumulatedData {
    /// The block hash.
    pub hash: HashOutput,
    /// The total accumulated offset for all kernels in the block.
    pub total_kernel_offset: PrivateKey,
    /// The achieved difficulty for solving the current block using the specified proof of work algorithm.
    pub achieved_difficulty: Difficulty,
    /// The total accumulated difficulty for all blocks since Genesis, but not including this block, tracked
    /// separately.
    pub total_accumulated_difficulty: U512,
    /// The total accumulated difficulty for RandomX proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_randomx_difficulty: AccumulatedDifficulty,
    /// The total accumulated difficulty for SHA3 proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    /// The target difficulty for solving the current block using the specified proof of work algorithm.
    pub target_difficulty: Difficulty,
}

impl BlockHeaderAccumulatedData {
    pub fn builder(previous: &BlockHeaderAccumulatedData) -> BlockHeaderAccumulatedDataBuilder<'_> {
        BlockHeaderAccumulatedDataBuilder::from_previous(previous)
    }
}

impl Display for BlockHeaderAccumulatedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Hash: {}", self.hash.to_hex())?;
        writeln!(f, "Achieved difficulty: {}", self.achieved_difficulty)?;
        writeln!(f, "Total accumulated difficulty: {}", self.total_accumulated_difficulty)?;
        writeln!(
            f,
            "Accumulated RandomX difficulty: {}",
            self.accumulated_randomx_difficulty
        )?;
        writeln!(f, "Accumulated sha3 difficulty: {}", self.accumulated_sha3x_difficulty)?;
        writeln!(f, "Target difficulty: {}", self.target_difficulty)?;
        Ok(())
    }
}

/// A block linked to a chain.
/// A ChainHeader guarantees (i.e cannot be constructed) that the block and accumulated data correspond by hash.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ChainHeader {
    header: BlockHeader,
    accumulated_data: BlockHeaderAccumulatedData,
}

impl Display for ChainHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.header)?;
        writeln!(f, "{}", self.accumulated_data)?;
        Ok(())
    }
}

impl ChainHeader {
    /// Attempts to construct a `ChainHeader` from a `BlockHeader` and associate `BlockHeaderAccumulatedData`. Returns
    /// None if the Block and the BlockHeaderAccumulatedData do not correspond (i.e have different hashes)
    pub fn try_construct(header: BlockHeader, accumulated_data: BlockHeaderAccumulatedData) -> Option<Self> {
        if accumulated_data.hash != header.hash() {
            return None;
        }

        Some(Self {
            header,
            accumulated_data,
        })
    }

    pub fn height(&self) -> u64 {
        self.header.height
    }

    pub fn timestamp(&self) -> u64 {
        self.header.timestamp.as_u64()
    }

    pub fn hash(&self) -> &HashOutput {
        &self.accumulated_data.hash
    }

    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    pub fn accumulated_data(&self) -> &BlockHeaderAccumulatedData {
        &self.accumulated_data
    }

    pub fn into_parts(self) -> (BlockHeader, BlockHeaderAccumulatedData) {
        (self.header, self.accumulated_data)
    }

    pub fn into_header(self) -> BlockHeader {
        self.header
    }

    pub fn upgrade_to_chain_block(self, body: AggregateBody) -> ChainBlock {
        // NOTE: Panic cannot occur because a ChainBlock has the same guarantees as ChainHeader
        ChainBlock::try_construct(Arc::new(Block::new(self.header, body)), self.accumulated_data).unwrap()
    }
}

/// A block linked to a chain.
/// A ChainBlock MUST have the same or stronger guarantees than `ChainHeader`
#[derive(Debug, Clone, PartialEq)]
pub struct ChainBlock {
    accumulated_data: BlockHeaderAccumulatedData,
    block: Arc<Block>,
}

impl Display for ChainBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.accumulated_data)?;
        writeln!(f, "{}", self.block)?;
        Ok(())
    }
}

impl ChainBlock {
    /// Attempts to construct a `ChainBlock` from a `Block` and associate `BlockHeaderAccumulatedData`. Returns None if
    /// the Block and the BlockHeaderAccumulatedData do not correspond (i.e have different hashes)
    pub fn try_construct(block: Arc<Block>, accumulated_data: BlockHeaderAccumulatedData) -> Option<Self> {
        if accumulated_data.hash != block.hash() {
            return None;
        }

        Some(Self {
            accumulated_data,
            block,
        })
    }

    pub fn height(&self) -> u64 {
        self.block.header.height
    }

    pub fn hash(&self) -> &HashOutput {
        &self.accumulated_data.hash
    }

    /// Returns a reference to the inner block
    pub fn block(&self) -> &Block {
        &self.block
    }

    /// Returns a reference to the inner block's header
    pub fn header(&self) -> &BlockHeader {
        &self.block.header
    }

    /// Returns the inner block wrapped in an atomically reference counted (ARC) pointer. This call is cheap and does
    /// not copy the block in memory.
    pub fn to_arc_block(&self) -> Arc<Block> {
        self.block.clone()
    }

    pub fn accumulated_data(&self) -> &BlockHeaderAccumulatedData {
        &self.accumulated_data
    }

    pub fn to_chain_header(&self) -> ChainHeader {
        // NOTE: Panic is impossible, a ChainBlock cannot be constructed if inconsistencies between the header and
        // accum data exist
        ChainHeader::try_construct(self.block.header.clone(), self.accumulated_data.clone()).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod chain_block {
        use super::*;
        use crate::blocks::genesis_block::get_esmeralda_genesis_block;

        #[test]
        fn it_converts_to_a_chain_header() {
            let genesis = get_esmeralda_genesis_block();
            let header = genesis.to_chain_header();
            assert_eq!(header.header(), genesis.header());
            assert_eq!(header.accumulated_data(), genesis.accumulated_data());
        }

        #[test]
        fn it_provides_guarantees_about_data_integrity() {
            let mut genesis = get_esmeralda_genesis_block();
            // Mess with the header, only possible using the non-public fields
            genesis.block = Arc::new({
                let mut b = (*genesis.block).clone();
                b.header.height = 1;
                b
            });
            assert!(ChainBlock::try_construct(genesis.to_arc_block(), genesis.accumulated_data().clone()).is_none());
            assert!(ChainHeader::try_construct(genesis.header().clone(), genesis.accumulated_data().clone()).is_none());

            genesis.block = Arc::new({
                let mut b = (*genesis.block).clone();
                b.header.height = 0;
                b
            });
            ChainBlock::try_construct(genesis.to_arc_block(), genesis.accumulated_data().clone()).unwrap();
            ChainHeader::try_construct(genesis.header().clone(), genesis.accumulated_data().clone()).unwrap();
        }
    }
}
