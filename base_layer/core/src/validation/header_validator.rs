use crate::{
    blocks::BlockHeader,
    chain_storage::{
        fetch_headers,
        fetch_target_difficulty,
        BlockHeaderAccumulatedData,
        BlockHeaderAccumulatedDataBuilder,
        BlockchainBackend,
    },
    consensus::ConsensusManager,
    proof_of_work::{randomx_factory::RandomXFactory, Difficulty},
    validation::{
        helpers::{
            check_header_timestamp_greater_than_median,
            check_pow_data,
            check_target_difficulty,
            check_timestamp_ftl,
        },
        HeaderValidation,
        ValidationError,
    },
};
use log::*;
use tari_crypto::tari_utilities::{hash::Hashable, hex::Hex};

pub const LOG_TARGET: &str = "c::val::block_validators";

pub struct HeaderValidator {
    rules: ConsensusManager,
    randomx_factory: RandomXFactory,
}

impl HeaderValidator {
    pub fn new(rules: ConsensusManager, randomx_factory: RandomXFactory) -> Self {
        Self { rules, randomx_factory }
    }

    /// Calculates the achieved and target difficulties at the specified height and compares them.
    pub fn check_achieved_and_target_difficulty<B: BlockchainBackend>(
        &self,
        db: &B,
        block_header: &BlockHeader,
    ) -> Result<(Difficulty, Difficulty), ValidationError>
    {
        let difficulty_window = fetch_target_difficulty(db, &self.rules, block_header.pow_algo(), block_header.height)?;

        let target = difficulty_window.calculate();
        Ok((
            check_target_difficulty(block_header, target, &self.randomx_factory)?,
            target,
        ))
    }

    /// This function tests that the block timestamp is greater than the median timestamp at the specified height.
    fn check_median_timestamp<B: BlockchainBackend>(
        &self,
        db: &B,
        block_header: &BlockHeader,
    ) -> Result<(), ValidationError>
    {
        if block_header.height == 0 {
            return Ok(()); // Its the genesis block, so we dont have to check median
        }

        let height = block_header.height - 1;
        let min_height = height.saturating_sub(
            self.rules
                .consensus_constants(block_header.height)
                .get_median_timestamp_count() as u64,
        );
        let timestamps = fetch_headers(db, min_height, height)?
            .iter()
            .map(|h| h.timestamp)
            .collect::<Vec<_>>();

        check_header_timestamp_greater_than_median(block_header, &timestamps)?;

        Ok(())
    }
}

impl<B: BlockchainBackend> HeaderValidation<B> for HeaderValidator {
    /// The consensus checks that are done (in order of cheapest to verify to most expensive):
    /// 1. Is the block timestamp within the Future Time Limit (FTL)?
    /// 1. Is the Proof of Work valid?
    /// 1. Is the achieved difficulty of this block >= the target difficulty for this block?

    fn validate(
        &self,
        backend: &B,
        header: &BlockHeader,
        previous_data: &BlockHeaderAccumulatedData,
    ) -> Result<BlockHeaderAccumulatedDataBuilder, ValidationError>
    {
        check_timestamp_ftl(&header, &self.rules)?;
        let hash = header.hash();
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: FTL timestamp is ok for {} ",
            header_id
        );
        self.check_median_timestamp(backend, header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Median timestamp is ok for {} ",
            header_id
        );
        check_pow_data(header, &self.rules, backend)?;
        let (achieved, target) = self.check_achieved_and_target_difficulty(backend, header)?;
        let accum_data = BlockHeaderAccumulatedDataBuilder::default()
            .hash(hash)
            .target_difficulty(target)
            .achieved_difficulty(previous_data, header.pow_algo(), achieved);
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", header_id
        );
        Ok(accum_data)
    }
}
