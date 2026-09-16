//  Copyright 2025, The Tari Project
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
use std::{fmt, fmt::Display};

use primitive_types::U512;
use tari_common_types::types::{HashOutput, PrivateKey};
use tari_transaction_components::tari_proof_of_work::{AccumulatedDifficulty, Difficulty};

/// Accumulated and other pertinent data in the block header acting as a "condensed blockchain snapshot" for the block
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The total accumulated difficulty for Merged mined monero RandomX proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_monero_randomx_difficulty: AccumulatedDifficulty,
    /// The total accumulated difficulty for Tari RandomX proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_tari_randomx_difficulty: AccumulatedDifficulty,
    /// The total accumulated difficulty for SHA3 proof of work for all blocks since Genesis,
    /// but not including this block, tracked separately.
    pub accumulated_sha3x_difficulty: AccumulatedDifficulty,
    /// The total accumulated difficulty for Cuckaroo proof of work for all blocks since Genesis,
    pub accumulated_cuckaroo_difficulty: AccumulatedDifficulty,
    /// The target difficulty for solving the current block using the specified proof of work algorithm.
    pub target_difficulty: Difficulty,
}

impl BlockHeaderAccumulatedData {
    pub fn genesis(hash: HashOutput, total_kernel_offset: PrivateKey) -> Self {
        Self {
            hash,
            total_kernel_offset,
            achieved_difficulty: Difficulty::min(),
            total_accumulated_difficulty: 1.into(),
            accumulated_monero_randomx_difficulty: AccumulatedDifficulty::min(),
            accumulated_tari_randomx_difficulty: AccumulatedDifficulty::min(),
            accumulated_sha3x_difficulty: AccumulatedDifficulty::min(),
            accumulated_cuckaroo_difficulty: AccumulatedDifficulty::min(),
            target_difficulty: Difficulty::min(),
        }
    }

    pub fn accumulated_monero_randomx_difficulty(&self) -> AccumulatedDifficulty {
        self.accumulated_monero_randomx_difficulty
    }

    pub fn accumulated_tari_randomx_difficulty(&self) -> AccumulatedDifficulty {
        self.accumulated_tari_randomx_difficulty
    }

    pub fn accumulated_sha3x_difficulty(&self) -> AccumulatedDifficulty {
        self.accumulated_sha3x_difficulty
    }

    pub fn accumulated_cuckaroo_difficulty(&self) -> AccumulatedDifficulty {
        self.accumulated_cuckaroo_difficulty
    }
}

impl Display for BlockHeaderAccumulatedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Hash: {}", self.hash)?;
        writeln!(f, "Achieved difficulty: {}", self.achieved_difficulty)?;
        writeln!(f, "Total accumulated difficulty: {}", self.total_accumulated_difficulty)?;
        writeln!(
            f,
            "Accumulated Monero RandomX difficulty: {}",
            self.accumulated_monero_randomx_difficulty
        )?;
        writeln!(
            f,
            "Accumulated Tari RandomX difficulty: {}",
            self.accumulated_tari_randomx_difficulty
        )?;
        writeln!(f, "Accumulated sha3 difficulty: {}", self.accumulated_sha3x_difficulty)?;
        writeln!(
            f,
            "Accumulated cuckaroo difficulty: {}",
            self.accumulated_cuckaroo_difficulty
        )?;
        writeln!(f, "Target difficulty: {}", self.target_difficulty)?;
        Ok(())
    }
}
