// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use log::*;
use tari_utilities::hex::Hex;

use crate::{
    blocks::BlockHeader,
    chain_storage::BlockchainBackend,
    consensus::ConsensusManager,
    proof_of_work::AchievedTargetDifficulty,
    validation::{
        helpers::{check_blockchain_version, check_not_bad_block, check_pow_data, check_timestamp_ftl},
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
        header: &BlockHeader,
        difficulty_calculator: &DifficultyCalculator,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);
        check_blockchain_version(constants, header.version)?;

        check_timestamp_ftl(header, &self.rules)?;
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        check_not_bad_block(backend, header.hash())?;
        check_pow_data(header, &self.rules, backend)?;
        let achieved_target = difficulty_calculator.check_achieved_and_target_difficulty(backend, header)?;

        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", header_id
        );
        Ok(achieved_target)
    }
}
