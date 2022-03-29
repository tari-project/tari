use log::*;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};

use crate::{
    blocks::BlockHeader,
    chain_storage::{fetch_headers, BlockchainBackend},
    consensus::{ConsensusConstants, ConsensusManager},
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
        HeaderValidation,
        ValidationError,
    },
};

pub const LOG_TARGET: &str = "c::val::header_validators";

pub struct HeaderValidator {
    rules: ConsensusManager,
}

impl HeaderValidator {
    pub fn new(rules: ConsensusManager) -> Self {
        Self { rules }
    }

    /// This function tests that the block timestamp is greater than the median timestamp at the specified height.
    fn check_median_timestamp<B: BlockchainBackend>(
        &self,
        db: &B,
        constants: &ConsensusConstants,
        block_header: &BlockHeader,
    ) -> Result<(), ValidationError> {
        if block_header.height == 0 {
            return Ok(()); // Its the genesis block, so we dont have to check median
        }

        let height = block_header.height - 1;
        let min_height = block_header
            .height
            .saturating_sub(constants.get_median_timestamp_count() as u64);
        let timestamps = fetch_headers(db, min_height, height)?
            .iter()
            .map(|h| h.timestamp)
            .collect::<Vec<_>>();

        check_header_timestamp_greater_than_median(block_header, &timestamps)?;

        Ok(())
    }
}

impl<TBackend: BlockchainBackend> HeaderValidation<TBackend> for HeaderValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block timestamp within the Future Time Limit (FTL)?
    /// 1. Is the Proof of Work valid?
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
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: FTL timestamp is ok for {} ",
            header_id
        );
        self.check_median_timestamp(backend, constants, header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Median timestamp is ok for {} ",
            header_id
        );
        check_pow_data(header, &self.rules, backend)?;
        let achieved_target = difficulty_calculator.check_achieved_and_target_difficulty(backend, header)?;
        check_not_bad_block(backend, &header.hash())?;

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
