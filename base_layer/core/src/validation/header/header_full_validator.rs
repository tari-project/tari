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

use log::warn;
use tari_common_types::types::FixedHash;
use tari_utilities::{epoch_time::EpochTime, hex::Hex};

use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError},
    chain_storage::BlockchainBackend,
    consensus::{ConsensusConstants, ConsensusManager},
    proof_of_work::{monero_rx::MoneroPowData, AchievedTargetDifficulty, Difficulty, PowAlgorithm, PowError},
    validation::{
        helpers::{check_header_timestamp_greater_than_median, check_target_difficulty},
        DifficultyCalculator,
        HeaderChainLinkedValidator,
        ValidationError,
    },
};

pub const LOG_TARGET: &str = "c::val::header_full_validator";

#[derive(Clone)]
pub struct HeaderFullValidator {
    rules: ConsensusManager,
    difficulty_calculator: DifficultyCalculator,
}

impl HeaderFullValidator {
    pub fn new(rules: ConsensusManager, difficulty_calculator: DifficultyCalculator) -> Self {
        Self {
            rules,
            difficulty_calculator,
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
        target_difficulty: Option<Difficulty>,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);

        check_not_bad_block(db, header.hash())?;
        check_blockchain_version(constants, header.version)?;
        check_height(header, prev_header)?;
        check_prev_hash(header, prev_header)?;

        sanity_check_timestamp_count(header, prev_timestamps, constants)?;
        check_header_timestamp_greater_than_median(header, prev_timestamps)?;

        check_timestamp_ftl(header, &self.rules)?;
        check_pow_data(header, &self.rules, db)?;
        let gen_hash = *self.rules.get_genesis_block().hash();

        let achieved_target = if let Some(target) = target_difficulty {
            check_target_difficulty(
                header,
                target,
                &self.difficulty_calculator.randomx_factory,
                &gen_hash,
                &self.rules,
            )?
        } else {
            self.difficulty_calculator
                .check_achieved_and_target_difficulty(db, header)?
        };

        Ok(achieved_target)
    }
}

/// This is a sanity check for the information provided by the caller, rather than a validation for the header itself.
fn sanity_check_timestamp_count(
    header: &BlockHeader,
    timestamps: &[EpochTime],
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    let expected_timestamp_count = cmp::min(consensus_constants.median_timestamp_count() as u64, header.height);
    // Empty `timestamps` is never valid
    if timestamps.is_empty() {
        return Err(ValidationError::IncorrectNumberOfTimestampsProvided {
            expected: expected_timestamp_count,
            actual: 0,
        });
    }

    if timestamps.len() as u64 != expected_timestamp_count {
        return Err(ValidationError::IncorrectNumberOfTimestampsProvided {
            actual: timestamps.len() as u64,
            expected: expected_timestamp_count,
        });
    }

    Ok(())
}

fn check_height(header: &BlockHeader, prev_header: &BlockHeader) -> Result<(), ValidationError> {
    if header.height != prev_header.height + 1 {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidHeight {
                expected: prev_header.height + 1,
                actual: header.height,
            },
        ));
    }

    Ok(())
}

fn check_prev_hash(header: &BlockHeader, prev_header: &BlockHeader) -> Result<(), ValidationError> {
    if header.prev_hash != prev_header.hash() {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidPreviousHash {
                expected: prev_header.hash(),
                actual: header.prev_hash,
            },
        ));
    }

    Ok(())
}

fn check_blockchain_version(constants: &ConsensusConstants, version: u16) -> Result<(), ValidationError> {
    if constants.valid_blockchain_version_range().contains(&version) {
        Ok(())
    } else {
        Err(ValidationError::InvalidBlockchainVersion { version })
    }
}

/// This function tests that the block timestamp is less than the FTL
pub fn check_timestamp_ftl(
    block_header: &BlockHeader,
    consensus_manager: &ConsensusManager,
) -> Result<(), ValidationError> {
    if block_header.timestamp > consensus_manager.consensus_constants(block_header.height).ftl() {
        warn!(
            target: LOG_TARGET,
            "Invalid Future Time Limit on block:{}",
            block_header.hash().to_hex()
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestampFutureTimeLimit,
        ));
    }
    Ok(())
}

fn check_not_bad_block<B: BlockchainBackend>(db: &B, hash: FixedHash) -> Result<(), ValidationError> {
    let block_exist = db.bad_block_exists(hash)?;
    if block_exist.0 {
        return Err(ValidationError::BadBlockFound {
            hash: hash.to_hex(),
            reason: block_exist.1,
        });
    }
    Ok(())
}

/// Check the PoW data in the BlockHeader. This currently only applies to blocks merged mined with Monero.
fn check_pow_data<B: BlockchainBackend>(
    block_header: &BlockHeader,
    rules: &ConsensusManager,
    db: &B,
) -> Result<(), ValidationError> {
    use PowAlgorithm::{RandomX, Sha3x};
    match block_header.pow.pow_algo {
        RandomX => {
            if block_header.nonce != 0 {
                return Err(ValidationError::BlockHeaderError(
                    BlockHeaderValidationError::InvalidNonce,
                ));
            }
            let monero_data = MoneroPowData::from_header(block_header, rules)?;
            let seed_height = db.fetch_monero_seed_first_seen_height(&monero_data.randomx_key)?;
            if seed_height != 0 {
                // Saturating sub: subtraction can underflow in reorgs / rewind-blockchain command
                let seed_used_height = block_header.height.saturating_sub(seed_height);
                if seed_used_height > rules.consensus_constants(block_header.height).max_randomx_seed_height() {
                    return Err(ValidationError::BlockHeaderError(
                        BlockHeaderValidationError::OldSeedHash,
                    ));
                }
            }

            Ok(())
        },
        Sha3x => {
            if !block_header.pow.pow_data.is_empty() {
                return Err(PowError::Sha3HeaderNonEmptyPowBytes.into());
            }
            Ok(())
        },
    }
}
