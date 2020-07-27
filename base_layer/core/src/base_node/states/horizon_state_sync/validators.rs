//  Copyright 2020, The Tari Project
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

use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    chain_storage::{BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    proof_of_work::{get_median_timestamp, get_target_difficulty, Difficulty, PowError},
    validation::{StatelessValidation, StatelessValidator, ValidationError},
};
use log::*;
use std::{fmt, sync::Arc};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex, Hashable};

const LOG_TARGET: &str = "c::bn::states::horizon_state_sync::validators";

#[derive(Clone)]
pub struct HorizonSyncValidators {
    pub header: Arc<StatelessValidator<BlockHeader>>,
}

impl HorizonSyncValidators {
    pub fn new<S: StatelessValidation<BlockHeader> + 'static>(header: S) -> Self {
        Self {
            header: Arc::new(Box::new(header)),
        }
    }
}

impl fmt::Debug for HorizonSyncValidators {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HorizonHeaderValidators")
            .field("header", &"...")
            .finish()
    }
}

pub struct HorizonHeadersValidator<B> {
    rules: ConsensusManager,
    db: BlockchainDatabase<B>,
}

impl<B: BlockchainBackend> HorizonHeadersValidator<B> {
    pub fn new(db: BlockchainDatabase<B>, rules: ConsensusManager) -> Self {
        Self { db, rules }
    }
}

impl<B: BlockchainBackend> StatelessValidation<BlockHeader> for HorizonHeadersValidator<B> {
    fn validate(&self, header: &BlockHeader) -> Result<(), ValidationError> {
        let header_id = format!("header #{} ({})", header.height, header.hash().to_hex());
        let tip_header = self
            .db
            .fetch_tip_header()
            .map_err(|e| ValidationError::CustomError(e.to_string()))?;
        self.check_median_timestamp(header, &tip_header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Median timestamp is ok for {} ",
            &header_id
        );
        self.check_achieved_and_target_difficulty(header, &tip_header)?;
        trace!(
            target: LOG_TARGET,
            "BlockHeader validation: Achieved difficulty is ok for {} ",
            &header_id
        );
        debug!(
            target: LOG_TARGET,
            "Block header validation: BlockHeader is VALID for {}", &header_id
        );

        Ok(())
    }
}

impl<B: BlockchainBackend> HorizonHeadersValidator<B> {
    pub fn is_genesis(&self, block_header: &BlockHeader) -> bool {
        block_header.height == 0 && self.rules.get_genesis_block_hash() == block_header.hash()
    }

    /// Calculates the achieved and target difficulties at the specified height and compares them.
    pub fn check_achieved_and_target_difficulty(
        &self,
        block_header: &BlockHeader,
        tip_header: &BlockHeader,
    ) -> Result<(), ValidationError>
    {
        let pow_algo = block_header.pow.pow_algo;
        let target = if self.is_genesis(block_header) {
            Difficulty::from(1)
        } else {
            let target_difficulties = self.fetch_target_difficulties(block_header, tip_header)?;

            let constants = self.rules.consensus_constants();
            get_target_difficulty(
                target_difficulties,
                constants.get_difficulty_block_window() as usize,
                constants.get_diff_target_block_interval(),
                constants.min_pow_difficulty(pow_algo),
                constants.get_difficulty_max_block_interval(),
            )
            .or_else(|e| {
                error!(target: LOG_TARGET, "Validation could not get target difficulty");
                Err(e)
            })
            .map_err(|_| {
                ValidationError::BlockHeaderError(BlockHeaderValidationError::ProofOfWorkError(
                    PowError::InvalidProofOfWork,
                ))
            })?
        };

        if block_header.pow.target_difficulty != target {
            warn!(
                target: LOG_TARGET,
                "Recorded header target difficulty was incorrect: (got = {}, expected = {})",
                block_header.pow.target_difficulty,
                target
            );
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::ProofOfWorkError(PowError::InvalidTargetDifficulty),
            ));
        }

        let achieved = block_header.achieved_difficulty();
        if achieved < target {
            warn!(
                target: LOG_TARGET,
                "Proof of work for {} was below the target difficulty. Achieved: {}, Target:{}",
                block_header.hash().to_hex(),
                achieved,
                target
            );
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow),
            ));
        }

        Ok(())
    }

    /// This function tests that the block timestamp is greater than the median timestamp at the specified height.
    pub fn check_median_timestamp(
        &self,
        block_header: &BlockHeader,
        tip_header: &BlockHeader,
    ) -> Result<(), ValidationError>
    {
        if self.is_genesis(block_header) {
            // Median timestamps check not required for the genesis block header
            return Ok(());
        }

        let start_height = block_header
            .height
            .saturating_sub(self.rules.consensus_constants().get_median_timestamp_count() as u64);

        if start_height == tip_header.height {
            return Ok(());
        }

        let block_nums = (start_height..tip_header.height).collect();
        let mut timestamps = self
            .db
            .fetch_headers(block_nums)
            .map_err(|e| ValidationError::CustomError(e.to_string()))?
            .iter()
            .map(|h| h.timestamp)
            .collect::<Vec<_>>();
        timestamps.push(tip_header.timestamp);

        // TODO: get_median_timestamp incorrectly returns an Option
        let median_timestamp =
            get_median_timestamp(timestamps).expect("get_median_timestamp only returns None if `timestamps` is empty");

        if block_header.timestamp < median_timestamp {
            warn!(
                target: LOG_TARGET,
                "Block header timestamp {} is less than median timestamp: {} for block:{}",
                block_header.timestamp,
                median_timestamp,
                block_header.hash().to_hex()
            );
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidTimestamp,
            ));
        }
        Ok(())
    }

    /// Returns the set of target difficulties for the specified proof of work algorithm.
    fn fetch_target_difficulties(
        &self,
        block_header: &BlockHeader,
        tip_header: &BlockHeader,
    ) -> Result<Vec<(EpochTime, Difficulty)>, ValidationError>
    {
        let block_window = self.rules.consensus_constants().get_difficulty_block_window();
        let start_height = tip_header.height.saturating_sub(block_window);
        if start_height == tip_header.height {
            return Ok(vec![]);
        }

        trace!(
            target: LOG_TARGET,
            "fetch_target_difficulties: tip height = {}, new header height = {}, block window = {}",
            tip_header.height,
            block_header.height,
            block_window
        );

        let block_window = block_window as usize;
        // TODO: create custom iterator for chunks that does not require a large number of u64s to exist in memory
        let heights = (0..=tip_header.height).rev().collect::<Vec<_>>();
        let mut target_difficulties = Vec::with_capacity(block_window);
        for block_nums in heights.chunks(block_window) {
            let headers = self
                .db
                .fetch_headers(block_nums.to_vec())
                .map_err(|err| ValidationError::CustomError(err.to_string()))?;

            let max_remaining = block_window.saturating_sub(target_difficulties.len());
            trace!(
                target: LOG_TARGET,
                "fetch_target_difficulties: max_remaining = {}",
                max_remaining
            );
            target_difficulties.extend(
                headers
                    .into_iter()
                    .filter(|h| h.pow.pow_algo == block_header.pow.pow_algo)
                    .take(max_remaining)
                    .map(|h| (h.timestamp, h.pow.target_difficulty)),
            );

            assert!(
                target_difficulties.len() <= block_window,
                "target_difficulties can never contain more elements than the block window"
            );
            if target_difficulties.len() == block_window {
                break;
            }
        }

        trace!(
            target: LOG_TARGET,
            "fetch_target_difficulties: #returned = {}",
            target_difficulties.len()
        );
        Ok(target_difficulties.into_iter().rev().collect())
    }
}
