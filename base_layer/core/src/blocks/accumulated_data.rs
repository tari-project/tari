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
};

use log::*;
use primitive_types::U512;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedCommitment, HashOutput, PrivateKey};
use tari_mmr::{pruned_hashset::PrunedHashSet, ArrayLike};
use tari_node_components::blocks::{BlockError, BlockHeaderAccumulatedData};
use tari_transaction_components::{consensus::ConsensusConstants, tari_proof_of_work::PowAlgorithm};

use crate::proof_of_work::AchievedTargetDifficulty;

const LOG_TARGET: &str = "c::bn::acc_data";

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct BlockAccumulatedData {
    pub(crate) kernels: PrunedHashSet,
    pub(crate) kernel_sum: CompressedCommitment,
}

impl BlockAccumulatedData {
    pub fn new(kernels: PrunedHashSet, total_kernel_sum: CompressedCommitment) -> Self {
        Self {
            kernels,
            kernel_sum: total_kernel_sum,
        }
    }

    pub fn dissolve(self) -> PrunedHashSet {
        self.kernels
    }

    pub fn kernel_sum(&self) -> &CompressedCommitment {
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
    pub kernel_sum: Option<CompressedCommitment>,
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

    pub fn build(self, consensus_constants: &ConsensusConstants) -> Result<BlockHeaderAccumulatedData, BlockError> {
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

        let (monero_randomx_diff, tari_randomx_diff, sha3x_diff, cuckaroo_diff) = match achieved_target.pow_algo() {
            PowAlgorithm::RandomXM => (
                previous_accum
                    .accumulated_monero_randomx_difficulty
                    .checked_add_difficulty(achieved_target.target())
                    .ok_or(BlockError::DifficultyOverflow)?,
                previous_accum.accumulated_tari_randomx_difficulty,
                previous_accum.accumulated_sha3x_difficulty,
                previous_accum.accumulated_cuckaroo_difficulty,
            ),
            PowAlgorithm::RandomXT => (
                previous_accum.accumulated_monero_randomx_difficulty,
                previous_accum
                    .accumulated_tari_randomx_difficulty
                    .checked_add_difficulty(achieved_target.target())
                    .ok_or(BlockError::DifficultyOverflow)?,
                previous_accum.accumulated_sha3x_difficulty,
                previous_accum.accumulated_cuckaroo_difficulty,
            ),
            PowAlgorithm::Sha3x => (
                previous_accum.accumulated_monero_randomx_difficulty,
                previous_accum.accumulated_tari_randomx_difficulty,
                previous_accum
                    .accumulated_sha3x_difficulty
                    .checked_add_difficulty(achieved_target.target())
                    .ok_or(BlockError::DifficultyOverflow)?,
                previous_accum.accumulated_cuckaroo_difficulty,
            ),
            PowAlgorithm::Cuckaroo => (
                previous_accum.accumulated_monero_randomx_difficulty,
                previous_accum.accumulated_tari_randomx_difficulty,
                previous_accum.accumulated_sha3x_difficulty,
                previous_accum
                    .accumulated_cuckaroo_difficulty
                    .checked_add_difficulty(achieved_target.target())
                    .ok_or(BlockError::DifficultyOverflow)?,
            ),
        };

        let total_kernel_offset = self
            .current_total_kernel_offset
            .map(|offset| &previous_accum.total_kernel_offset + offset)
            .ok_or(BlockError::BuilderMissingField {
                field: "total_kernel_offset",
            })?;
        let mut total_accumulated: U512 = U512::from(monero_randomx_diff.as_u128()) *
            U512::from(tari_randomx_diff.as_u128()) *
            U512::from(sha3x_diff.as_u128());

        if consensus_constants.include_c29_accumulated_difficulty_into_total() {
            total_accumulated *= U512::from(cuckaroo_diff.as_u128());
        }

        let result = BlockHeaderAccumulatedData {
            hash,
            total_kernel_offset,
            achieved_difficulty: achieved_target.achieved(),
            total_accumulated_difficulty: total_accumulated,
            accumulated_monero_randomx_difficulty: monero_randomx_diff,
            accumulated_tari_randomx_difficulty: tari_randomx_diff,
            accumulated_sha3x_difficulty: sha3x_diff,
            accumulated_cuckaroo_difficulty: cuckaroo_diff,
            target_difficulty: achieved_target.target(),
        };
        trace!(
            target: LOG_TARGET,
            "Calculated: Tot_acc_diff {}, Monero RandomX {}, Tari RandomX {}, SHA3 {}, Cuckaroo {}",
            result.total_accumulated_difficulty,
            result.accumulated_monero_randomx_difficulty,
            result.accumulated_tari_randomx_difficulty,
            result.accumulated_sha3x_difficulty,
            result.accumulated_cuckaroo_difficulty,
        );
        Ok(result)
    }
}
