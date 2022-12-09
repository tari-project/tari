// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::cmp;

use log::*;
use tari_utilities::hex::Hex;

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
        HeaderValidator,
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

impl<TBackend: BlockchainBackend> HeaderValidator<TBackend> for DefaultHeaderValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block timestamp within the Future Time Limit (FTL)?
    /// 1. Is the Proof of Work struct valid? Note it does not check the actual PoW here
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?

    fn validate(
        &self,
        backend: &TBackend,
        last_x_headers: &[&BlockHeader],
        header: &BlockHeader,
        difficulty_calculator: &DifficultyCalculator,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);
        if header.height != last_x_headers[0].height + 1 {
            let result = Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidHeight {
                    expected: last_x_headers[0].height + 1,
                    actual: header.height,
                },
            ));
            return result;
        }
        if header.prev_hash != last_x_headers[0].hash() {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidPreviousHash {
                    expected: last_x_headers[0].hash(),
                    actual: header.prev_hash,
                },
            ));
        }
        check_not_bad_block(backend, header.hash())?;
        check_blockchain_version(constants, header.version)?;
        check_timestamp_ftl(header, &self.rules)?;
        let timestamps = last_x_headers
            .iter()
            .map(|h| h.timestamp)
            .take(constants.get_median_timestamp_count() as usize)
            .collect::<Vec<_>>();
        if timestamps.len() <
            cmp::min(
                constants.get_median_timestamp_count() as usize,
                header.height as usize - 1,
            )
        {
            return Err(ValidationError::NotEnoughTimestamps {
                actual: timestamps.len() as usize,
                expected: constants.get_median_timestamp_count() as usize,
            });
        }
        check_header_timestamp_greater_than_median(header, &timestamps)?;
        check_pow_data(header, &self.rules, backend)?;
        let achieved_target = difficulty_calculator.check_achieved_and_target_difficulty(backend, header)?;
        Ok(achieved_target)
    }
}
