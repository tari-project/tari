// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::cmp;

use tari_utilities::epoch_time::EpochTime;

use super::helpers::check_header_timestamp_greater_than_median;
use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    proof_of_work::AchievedTargetDifficulty,
    validation::{
        helpers::{check_blockchain_version, check_not_bad_block, check_pow_data, check_timestamp_ftl},
        ChainLinkedHeaderValidator,
        DifficultyCalculator,
        ValidationError,
    },
};

pub const LOG_TARGET: &str = "c::val::header_validators";

pub struct DefaultHeaderValidator {
    rules: ConsensusManager,
}

impl DefaultHeaderValidator {
    pub fn new(rules: ConsensusManager) -> Self {
        Self { rules }
    }
}

impl<TBackend: BlockchainBackend> ChainLinkedHeaderValidator<TBackend> for DefaultHeaderValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block timestamp within the Future Time Limit (FTL)?
    /// 1. Is the Proof of Work struct valid? Note it does not check the actual PoW here
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?

    fn validate(
        &self,
        backend: &TBackend,
        prev_timestamps: &[EpochTime],
        prev_header: &BlockHeader,
        header: &BlockHeader,
        difficulty_calculator: &DifficultyCalculator,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);

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

        check_not_bad_block(backend, header.hash())?;
        check_blockchain_version(constants, header.version)?;
        check_timestamp_ftl(header, &self.rules)?;

        let expected_timestamp_count = cmp::min(constants.get_median_timestamp_count(), header.height as usize - 1);
        let timestamps: Vec<EpochTime> = prev_timestamps.iter().take(expected_timestamp_count).copied().collect();
        if timestamps.len() < expected_timestamp_count {
            return Err(ValidationError::NotEnoughTimestamps {
                actual: timestamps.len() as usize,
                expected: expected_timestamp_count,
            });
        }
        check_header_timestamp_greater_than_median(header, &timestamps)?;

        check_pow_data(header, &self.rules, backend)?;
        let achieved_target = difficulty_calculator.check_achieved_and_target_difficulty(backend, header)?;

        Ok(achieved_target)
    }
}
