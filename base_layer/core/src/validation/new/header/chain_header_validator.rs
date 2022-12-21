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

use log::{debug, warn};
use tari_utilities::{epoch_time::EpochTime, hex::Hex};

use super::valid_header::InternallyValidHeader;
use crate::{
    blocks::{BlockHeader, BlockHeaderAccumulatedData, BlockHeaderValidationError, ChainHeader},
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, ChainStorageError, TargetDifficulties},
    common::rolling_vec::RollingVec,
    consensus::ConsensusManager,
    proof_of_work::{
        monero_difficulty,
        randomx_factory::RandomXFactory,
        sha3x_difficulty,
        AchievedTargetDifficulty,
        Difficulty,
        PowAlgorithm,
        PowError,
    },
    validation::{helpers::calc_median_timestamp, ValidationError},
};

pub const LOG_TARGET: &str = "c::val::chain_header_validator";

pub struct ChainLinkedHeaderValidator<B> {
    db: AsyncBlockchainDb<B>,
    consensus_rules: ConsensusManager,
    randomx_factory: RandomXFactory,
}

impl<B: BlockchainBackend + 'static> ChainLinkedHeaderValidator<B> {
    pub fn new(db: AsyncBlockchainDb<B>, consensus_rules: ConsensusManager, randomx_factory: RandomXFactory) -> Self {
        Self {
            db,
            consensus_rules,
            randomx_factory,
        }
    }

    /// Takes an (internally) valid header and validates it in context of previous headers in the chain
    pub async fn validate(&self, headers: Vec<InternallyValidHeader>) -> Result<ChainHeader, ValidationError> {
        let start_header = match headers.first() {
            Some(header) => header,
            // TODO: create a custom variant
            None => return Err(ValidationError::CustomError("Empty headers".to_string())),
        };
        let mut state = self.initialize_state(start_header).await?;

        for header in headers {
            self.validate_header_with_state(&header.0, &mut state).await?;
        }

        Ok(state.valid_headers.last().unwrap().clone())
    }

    async fn initialize_state(&self, start_header: &InternallyValidHeader) -> Result<State, ValidationError> {
        let start_hash = start_header.0.hash();
        let start_header = self
            .db
            .fetch_header_by_block_hash(start_hash)
            .await?
            // TODO: create a validation error variant
            .ok_or_else(|| ValidationError::CustomError("StartHashNotFound".to_string()))?;
        let timestamps = self.db.fetch_block_timestamps(start_hash).await?;
        let target_difficulties = self.db.fetch_target_difficulties_for_next_block(start_hash).await?;
        let previous_accum = self
            .db
            .fetch_header_accumulated_data(start_hash)
            .await?
            .ok_or_else(|| ChainStorageError::ValueNotFound {
                entity: "BlockHeaderAccumulatedData",
                field: "hash",
                value: start_hash.to_hex(),
            })?;
        debug!(
            target: LOG_TARGET,
            "Setting header validator state ({} timestamp(s), target difficulties: {} SHA3, {} Monero)",
            timestamps.len(),
            target_difficulties.get(PowAlgorithm::Sha3).len(),
            target_difficulties.get(PowAlgorithm::Monero).len(),
        );
        Ok(State {
            current_height: start_header.height,
            timestamps,
            target_difficulties,
            previous_accum,
            // One large allocation is usually better even if it is not always used.
            valid_headers: Vec::with_capacity(1000),
        })
    }

    async fn validate_header_with_state(&self, header: &BlockHeader, state: &mut State) -> Result<(), ValidationError> {
        let constants = self.consensus_rules.consensus_constants(header.height);

        let expected_height = state.current_height + 1;
        if header.height != expected_height {
            return Err(ValidationError::InvalidMinedHeight);
        }
        if header.prev_hash != state.previous_accum.hash {
            // TODO: include ChainLinkBroken error variant
            return Err(ValidationError::CustomError("ChainLinkBroken".to_string()));
        }

        Self::check_header_timestamp_greater_than_median(header, &state.timestamps)?;

        let target_difficulty = state.target_difficulties.get(header.pow_algo()).calculate(
            constants.min_pow_difficulty(header.pow_algo()),
            constants.max_pow_difficulty(header.pow_algo()),
        );
        let achieved_target = Self::check_target_difficulty(header, target_difficulty, &self.randomx_factory)?;

        // Ensure that timestamps are inserted in sorted order
        let maybe_index = state.timestamps.iter().position(|ts| ts >= &header.timestamp());
        match maybe_index {
            Some(pos) => {
                state.timestamps.insert(pos, header.timestamp());
            },
            None => state.timestamps.push(header.timestamp()),
        }

        state.current_height = header.height;
        // Add a "more recent" datapoint onto the target difficulty
        state.target_difficulties.add_back(header, target_difficulty);

        let block_hash = header.hash();
        let accumulated_data = BlockHeaderAccumulatedData::builder(&state.previous_accum)
            .with_hash(block_hash)
            .with_achieved_target_difficulty(achieved_target)
            .with_total_kernel_offset(header.total_kernel_offset.clone())
            .build()
            .map_err(|_| ValidationError::CustomError("BlockError".to_string()))?;

        // NOTE: accumulated_data constructed from header so they are guaranteed to correspond
        let chain_header = ChainHeader::try_construct(header.clone(), accumulated_data).unwrap();

        state.previous_accum = chain_header.accumulated_data().clone();
        state.valid_headers.push(chain_header);

        Ok(())
    }

    fn check_header_timestamp_greater_than_median(
        block_header: &BlockHeader,
        timestamps: &[EpochTime],
    ) -> Result<(), ValidationError> {
        if timestamps.is_empty() {
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidTimestamp("The timestamp is empty".to_string()),
            ));
        }

        let median_timestamp = calc_median_timestamp(timestamps);
        if block_header.timestamp < median_timestamp {
            warn!(
                target: LOG_TARGET,
                "Block header timestamp {} is less than median timestamp: {} for block:{}",
                block_header.timestamp,
                median_timestamp,
                block_header.hash().to_hex()
            );
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::InvalidTimestamp(format!(
                    "The timestamp `{}` was less than the median timestamp `{}`",
                    block_header.timestamp, median_timestamp
                )),
            ));
        }

        Ok(())
    }

    fn check_target_difficulty(
        block_header: &BlockHeader,
        target: Difficulty,
        randomx_factory: &RandomXFactory,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let achieved = match block_header.pow_algo() {
            PowAlgorithm::Monero => monero_difficulty(block_header, randomx_factory)?,
            PowAlgorithm::Sha3 => sha3x_difficulty(block_header),
        };

        match AchievedTargetDifficulty::try_construct(block_header.pow_algo(), target, achieved) {
            Some(achieved_target) => Ok(achieved_target),
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Proof of work for {} at height {} was below the target difficulty. Achieved: {}, Target: {}",
                    block_header.hash().to_hex(),
                    block_header.height,
                    achieved,
                    target
                );
                Err(ValidationError::BlockHeaderError(
                    BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow {
                        achieved,
                        target,
                    }),
                ))
            },
        }
    }
}

#[derive(Debug, Clone)]
struct State {
    current_height: u64,
    timestamps: RollingVec<EpochTime>,
    target_difficulties: TargetDifficulties,
    previous_accum: BlockHeaderAccumulatedData,
    valid_headers: Vec<ChainHeader>,
}
