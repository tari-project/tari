// Copyright 2022. The Tari Project
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

use std::cmp;

use tari_utilities::epoch_time::EpochTime;

use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    proof_of_work::AchievedTargetDifficulty,
    validation::{
        helpers::{
            check_blockchain_version,
            check_header_timestamp_greater_than_median,
            check_not_bad_block,
            check_pow_data,
            check_timestamp_ftl,
        },
        DifficultyCalculator,
        HeaderChainLinkedValidator,
        ValidationError,
    },
};

pub struct HeaderFullValidator {
    rules: ConsensusManager,
    difficulty_calculator: DifficultyCalculator,
    bypass_timestamp_count_verification: bool,
}

impl HeaderFullValidator {
    pub fn new(
        rules: ConsensusManager,
        difficulty_calculator: DifficultyCalculator,
        bypass_timestamp_count_verification: bool,
    ) -> Self {
        Self {
            rules,
            difficulty_calculator,
            bypass_timestamp_count_verification,
        }
    }
}

impl<B: BlockchainBackend> HeaderChainLinkedValidator<B> for HeaderFullValidator {
    fn validate(
        &self,
        db: &B,
        header: &BlockHeader,
        prev_header: &BlockHeader,
        prev_timestamps: &[EpochTime],
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);

        if !self.bypass_timestamp_count_verification {
            let expected_timestamp_count = cmp::min(constants.get_median_timestamp_count(), header.height as usize - 1);
            let timestamps: Vec<EpochTime> = prev_timestamps.iter().take(expected_timestamp_count).copied().collect();
            if timestamps.len() < expected_timestamp_count {
                return Err(ValidationError::NotEnoughTimestamps {
                    actual: timestamps.len() as usize,
                    expected: expected_timestamp_count,
                });
            }
        }

        if header.height != prev_header.height + 1 {
            let result = Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidHeight {
                    expected: prev_header.height + 1,
                    actual: header.height,
                },
            ));
            return result;
        }
        if header.prev_hash != prev_header.hash() {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidPreviousHash {
                    expected: prev_header.hash(),
                    actual: header.prev_hash,
                },
            ));
        }

        check_blockchain_version(constants, header.version)?;
        check_timestamp_ftl(header, &self.rules)?;
        check_header_timestamp_greater_than_median(header, prev_timestamps)?;

        check_not_bad_block(db, header.hash())?;
        check_pow_data(header, &self.rules, db)?;

        let achieved_target = self
            .difficulty_calculator
            .check_achieved_and_target_difficulty(db, header)?;

        Ok(achieved_target)
    }
}
