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
use tari_node_components::blocks::{BlockHeader, BlockHeaderValidationError};
use tari_transaction_components::{
    consensus::ConsensusConstants,
    tari_proof_of_work::{PowAlgorithm, PowError, ProofOfWork},
};
use tari_utilities::{epoch_time::EpochTime, hex::Hex};

use crate::{
    chain_storage::BlockchainBackend,
    consensus::BaseNodeConsensusManager,
    proof_of_work::{AchievedTargetDifficulty, AdjustedTarget},
    validation::{
        DifficultyCalculator,
        HeaderChainLinkedValidator,
        ValidationError,
        helpers::{check_header_timestamp_greater_than_median, check_target_difficulty},
    },
};
pub const LOG_TARGET: &str = "c::val::header_full_validator";

#[derive(Clone)]
pub struct HeaderFullValidator {
    rules: BaseNodeConsensusManager,
    difficulty_calculator: DifficultyCalculator,
    gen_hash: FixedHash,
}

impl HeaderFullValidator {
    pub fn new(rules: BaseNodeConsensusManager, difficulty_calculator: DifficultyCalculator) -> Self {
        let gen_hash = *rules.get_genesis_block().hash();
        Self {
            rules,
            difficulty_calculator,
            gen_hash,
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
        target_difficulty: Option<AdjustedTarget>,
        vm_key: FixedHash,
    ) -> Result<AchievedTargetDifficulty, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);

        check_not_bad_block(db, header.hash())?;
        check_blockchain_version(constants, header.version)?;
        check_height(header, prev_header)?;
        check_prev_hash(header, prev_header)?;

        sanity_check_timestamp_count(header, prev_timestamps, constants)?;
        check_header_timestamp_greater_than_median(header, prev_timestamps)?;

        check_timestamp_ftl(header, &self.rules)?;
        check_pow_data(header, constants)?;
        let achieved_target = if let Some(target) = target_difficulty {
            check_target_difficulty(
                header,
                target,
                &self.difficulty_calculator.randomx_factory,
                &self.gen_hash,
                &self.rules,
                vm_key,
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
    if header.height != prev_header.height.saturating_add(1) {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidHeight {
                expected: prev_header.height.saturating_add(1),
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
    consensus_manager: &BaseNodeConsensusManager,
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
    let (is_bad_block, reason) = db.bad_block_exists(hash)?;
    if is_bad_block {
        return Err(ValidationError::BadBlockFound {
            hash: hash.to_hex(),
            reason,
        });
    }
    Ok(())
}

fn check_allowed_algos(pow: &ProofOfWork, allowed_algos: &[PowAlgorithm]) -> Result<(), ValidationError> {
    if !allowed_algos.contains(&pow.pow_algo) {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidPowAlgorithm(pow.pow_algo.to_string()),
        ));
    }
    Ok(())
}

/// Check the PoW data in the BlockHeader. This currently only applies to blocks merged mined with Monero.
fn check_pow_data(block_header: &BlockHeader, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
    let allowed_algos = consensus_constants.current_permitted_pow_algos();
    check_allowed_algos(&block_header.pow, &allowed_algos)?;
    check_pow_data_inner(
        &block_header.pow,
        block_header.nonce,
        consensus_constants.cuckaroo_cycle_length(),
        consensus_constants.cuckaroo_edge_bits(),
    )
}

fn check_pow_data_inner(
    pow: &ProofOfWork,
    nonce: u64,
    cuckaroo_cycle_length: u8,
    cuckaroo_edge_bits: u8,
) -> Result<(), ValidationError> {
    match pow.pow_algo {
        PowAlgorithm::RandomXM => {
            if nonce != 0 {
                return Err(ValidationError::BlockHeaderError(
                    BlockHeaderValidationError::InvalidNonce,
                ));
            }
            Ok(())
        },
        PowAlgorithm::RandomXT => {
            if pow.pow_data.len() > 32 {
                return Err(PowError::RandomxTPowDataTooLong.into());
            }
            Ok(())
        },
        PowAlgorithm::Sha3x => {
            if !pow.pow_data.is_empty() {
                return Err(PowError::Sha3HeaderNonEmptyPowBytes.into());
            }
            Ok(())
        },
        PowAlgorithm::Cuckaroo => {
            let cycle_length = cuckaroo_cycle_length as usize;
            let edge_bits = cuckaroo_edge_bits as usize;
            let total_packed_size = cycle_length.saturating_mul(edge_bits);
            let remainder = total_packed_size % 8;
            let total_bytes = if remainder != 0 {
                (total_packed_size / 8).saturating_add(1)
            } else {
                total_packed_size / 8
            };

            if pow.pow_data.len() != total_bytes {
                return Err(PowError::CuckarooPowDataSizeMismatch {
                    expected: total_bytes,
                    actual: pow.pow_data.len(),
                }
                .into());
            }

            if remainder != 0 {
                // Ensure that the last byte is not padded with zeros
                let last_byte = *pow
                    .pow_data
                    .get(total_bytes.saturating_sub(1))
                    .expect("Already checked");

                let padding_mask = (1u8 << remainder).saturating_sub(1);
                let mask = 0xff ^ padding_mask; // Mask to check if the last byte is padded

                if last_byte & mask != 0 {
                    return Err(PowError::CuckarooPowDataNonZeroPadding {
                        padding: last_byte & mask,
                    }
                    .into());
                }
            }
            Ok(())
        },
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_check_pow_data_allowed_algos() {
        let allowed_algos = vec![PowAlgorithm::RandomXM];

        let pow = ProofOfWork::new(PowAlgorithm::RandomXM);
        let res = check_allowed_algos(&pow, &allowed_algos);
        assert!(res.is_ok());
        let pow = ProofOfWork::new(PowAlgorithm::RandomXT);

        let res = check_allowed_algos(&pow, &allowed_algos);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_randomxm() {
        let pow = ProofOfWork::new(PowAlgorithm::RandomXM);
        let res = check_pow_data_inner(&pow, 0, 0, 0);
        assert!(res.is_ok());

        let pow = ProofOfWork::new(PowAlgorithm::RandomXM);
        let res = check_pow_data_inner(&pow, 1, 0, 0);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_randomxt() {
        let pow = ProofOfWork::new(PowAlgorithm::RandomXT);
        let res = check_pow_data_inner(&pow, 0, 0, 0);
        assert!(res.is_ok());

        let pow = ProofOfWork::new(PowAlgorithm::RandomXT);
        let res = check_pow_data_inner(&pow, 0, 0, 0);
        assert!(res.is_ok());

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXT,
            pow_data: vec![0; 33].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 0, 0);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_sha3x() {
        let pow = ProofOfWork::new(PowAlgorithm::Sha3x);
        let res = check_pow_data_inner(&pow, 0, 0, 0);
        assert!(res.is_ok());

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Sha3x,
            pow_data: vec![1].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 0, 0);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_cuckaroo_data_size_mismatch() {
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 10].try_into().unwrap(),
        };
        // Check with multiple of 8
        let res = check_pow_data_inner(&pow, 0, 10, 8);
        assert!(res.is_ok());

        // Check with non-multiple of 8
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 11].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 9, 9);
        assert!(res.is_ok());

        // Check now with invalid data.

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 11].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 10, 8);
        assert!(res.is_err());

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 12].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 9, 9);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_cuckaroo_non_zero_padding() {
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![128u8; 11].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 9, 9);
        assert!(res.is_err());
    }
}
