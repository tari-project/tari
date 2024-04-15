// Copyright 2019. The Tari Project
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

use std::convert::TryFrom;

use log::*;
use tari_common_types::types::FixedHash;
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex};
use tari_script::TariScript;

use crate::{
    blocks::{BlockHeader, BlockHeaderValidationError, BlockValidationError},
    borsh::SerializedSize,
    chain_storage::{BlockchainBackend, MmrRoots, MmrTree},
    consensus::{ConsensusConstants, ConsensusManager},
    covenants::Covenant,
    proof_of_work::{
        randomx_difficulty,
        randomx_factory::RandomXFactory,
        sha3x_difficulty,
        AchievedTargetDifficulty,
        Difficulty,
        PowAlgorithm,
        PowError,
    },
    transactions::transaction_components::{TransactionInput, TransactionKernel, TransactionOutput},
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::helpers";

/// Returns the median timestamp for the provided timestamps.
///
/// ## Panics
/// When an empty slice is given as this is undefined for median average.
/// https://math.stackexchange.com/a/3451015
pub fn calc_median_timestamp(timestamps: &[EpochTime]) -> Result<EpochTime, ValidationError> {
    trace!(
        target: LOG_TARGET,
        "Calculate the median timestamp from {} timestamps",
        timestamps.len()
    );
    if timestamps.is_empty() {
        return Err(ValidationError::IncorrectNumberOfTimestampsProvided { expected: 1, actual: 0 });
    }

    let mid_index = timestamps.len() / 2;
    let median_timestamp = if timestamps.len() % 2 == 0 {
        trace!(
            target: LOG_TARGET,
            "No median timestamp available, estimating median as avg of [{}] and [{}]",
            timestamps[mid_index - 1],
            timestamps[mid_index],
        );
        // To compute this mean, we use `u128` to avoid overflow with the internal `u64` typing
        // Note that the final cast back to `u64` will never truncate since each summand is bounded by `u64`
        // To make the linter happy, we use `u64::MAX` in the impossible case that the cast fails
        EpochTime::from(
            u64::try_from(
                (u128::from(timestamps[mid_index - 1].as_u64()) + u128::from(timestamps[mid_index].as_u64())) / 2,
            )
            .unwrap_or(u64::MAX),
        )
    } else {
        timestamps[mid_index]
    };
    trace!(target: LOG_TARGET, "Median timestamp:{}", median_timestamp);
    Ok(median_timestamp)
}
pub fn check_header_timestamp_greater_than_median(
    block_header: &BlockHeader,
    timestamps: &[EpochTime],
) -> Result<(), ValidationError> {
    if timestamps.is_empty() {
        // unreachable due to sanity_check_timestamp_count
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestamp("The timestamp is empty".to_string()),
        ));
    }

    let median_timestamp = calc_median_timestamp(timestamps)?;
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
pub fn check_target_difficulty(
    block_header: &BlockHeader,
    target: Difficulty,
    randomx_factory: &RandomXFactory,
    gen_hash: &FixedHash,
    consensus: &ConsensusManager,
) -> Result<AchievedTargetDifficulty, ValidationError> {
    let achieved = match block_header.pow_algo() {
        PowAlgorithm::RandomX => randomx_difficulty(block_header, randomx_factory, gen_hash, consensus)?,
        PowAlgorithm::Sha3x => sha3x_difficulty(block_header)?,
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
                BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow { achieved, target }),
            ))
        },
    }
}

pub fn is_all_unique_and_sorted<'a, I: IntoIterator<Item = &'a T>, T: PartialOrd + 'a>(items: I) -> bool {
    let mut items = items.into_iter();
    let prev_item = items.next();
    if prev_item.is_none() {
        return true;
    }
    let mut prev_item = prev_item.unwrap();
    for item in items {
        if item <= prev_item {
            return false;
        }
        prev_item = item;
    }

    true
}

/// This function checks that an input is a valid spendable UTXO in the database. It cannot confirm
/// zero confermation transactions.
pub fn check_input_is_utxo<B: BlockchainBackend>(db: &B, input: &TransactionInput) -> Result<(), ValidationError> {
    let output_hash = input.output_hash();
    if let Some(utxo_hash) = db.fetch_unspent_output_hash_by_commitment(input.commitment()?)? {
        // We know that the commitment exists in the UTXO set. Check that the output hash matches (i.e. all fields
        // like output features match)
        if utxo_hash == output_hash {
            // Because the retrieved hash matches the new input.output_hash() we know all the fields match and are all
            // still the same
            return Ok(());
        }

        let output = db.fetch_output(&utxo_hash)?;
        warn!(
            target: LOG_TARGET,
            "Input spends a UTXO but does not produce the same hash as the output it spends: Expected hash: {}, \
             provided hash:{}
            input: {:?}. output in db: {:?}",
            utxo_hash.to_hex(),
            output_hash.to_hex(),
            input,
            output
        );

        return Err(ValidationError::UnknownInput);
    }

    // Wallet needs to know if a transaction has already been mined and uses this error variant to do so.
    if db.fetch_output(&output_hash)?.is_some() {
        warn!(
            target: LOG_TARGET,
            "Validation failed due to already spent input: {}", input
        );
        // We know that the output here must be spent because `fetch_unspent_output_hash_by_commitment` would have
        // been Some
        return Err(ValidationError::ContainsSTxO);
    }

    warn!(
        target: LOG_TARGET,
        "Input ({}, {}) does not exist in the database yet", input.commitment()?.to_hex(), output_hash.to_hex()
    );
    Err(ValidationError::UnknownInput)
}

/// Checks the byte size of TariScript is less than or equal to the given size, otherwise returns an error.
pub fn check_tari_script_byte_size(script: &TariScript, max_script_size: usize) -> Result<(), ValidationError> {
    let script_size = script
        .get_serialized_size()
        .map_err(|e| ValidationError::SerializationError(format!("Failed to get serialized script size: {}", e)))?;
    if script_size > max_script_size {
        return Err(ValidationError::TariScriptExceedsMaxSize {
            max_script_size,
            actual_script_size: script_size,
        });
    }
    Ok(())
}

/// This function checks that the outputs do not already exist in the TxO set.
pub fn check_not_duplicate_txo<B: BlockchainBackend>(
    db: &B,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    if db
        .fetch_unspent_output_hash_by_commitment(&output.commitment)?
        .is_some()
    {
        warn!(
            target: LOG_TARGET,
            "Duplicate UTXO set commitment found for output: {}", output
        );
        return Err(ValidationError::ContainsDuplicateUtxoCommitment);
    }

    Ok(())
}

pub fn check_mmr_roots(header: &BlockHeader, mmr_roots: &MmrRoots) -> Result<(), ValidationError> {
    if header.kernel_mr != mmr_roots.kernel_mr {
        warn!(
            target: LOG_TARGET,
            "Block header kernel MMR roots in #{} {} do not match calculated roots. Expected: {}, Actual:{}",
            header.height,
            header.hash().to_hex(),
            header.kernel_mr.to_hex(),
            mmr_roots.kernel_mr.to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots {
            kind: "Kernel",
        }));
    };
    if header.kernel_mmr_size != mmr_roots.kernel_mmr_size {
        warn!(
            target: LOG_TARGET,
            "Block header kernel MMR size in #{} {} does not match. Expected: {}, Actual:{}",
            header.height,
            header.hash().to_hex(),
            header.kernel_mmr_size,
            mmr_roots.kernel_mmr_size
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
            mmr_tree: MmrTree::Kernel.to_string(),
            expected: mmr_roots.kernel_mmr_size,
            actual: header.kernel_mmr_size,
        }));
    }
    if header.output_mr != mmr_roots.output_mr {
        warn!(
            target: LOG_TARGET,
            "Block header output MMR roots in #{} {} do not match calculated roots. Expected: {}, Actual:{}",
            header.height,
            header.hash().to_hex(),
            header.output_mr.to_hex(),
            mmr_roots.output_mr.to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots {
            kind: "Utxo",
        }));
    };
    if header.output_smt_size != mmr_roots.output_smt_size {
        warn!(
            target: LOG_TARGET,
            "Block header output MMR size in {} does not match. Expected: {}, Actual: {}",
            header.hash().to_hex(),
            header.output_smt_size,
            mmr_roots.output_smt_size
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
            mmr_tree: "UTXO".to_string(),
            expected: mmr_roots.output_smt_size,
            actual: header.output_smt_size,
        }));
    }
    if header.input_mr != mmr_roots.input_mr {
        warn!(
            target: LOG_TARGET,
            "Block header input merkle root in {} do not match calculated root. Header.input_mr: {}, Calculated: {}",
            header.hash().to_hex(),
            header.input_mr.to_hex(),
            mmr_roots.input_mr.to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots {
            kind: "Input",
        }));
    }
    if header.validator_node_mr != mmr_roots.validator_node_mr {
        warn!(
            target: LOG_TARGET,
            "Block header validator node merkle root in {} do not match calculated root. Header.validator_node_mr: \
             {}, Calculated: {}",
            header.hash().to_hex(),
            header.validator_node_mr.to_hex(),
            mmr_roots.validator_node_mr.to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots {
            kind: "Validator Node",
        }));
    }

    if header.validator_node_size != mmr_roots.validator_node_size {
        warn!(
            target: LOG_TARGET,
            "Block header validator size in #{} {} does not match. Expected: {}, Actual:{}",
            header.height,
            header.hash().to_hex(),
            header.validator_node_size,
            mmr_roots.validator_node_size
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
            mmr_tree: "Validator_node".to_string(),
            expected: mmr_roots.validator_node_size,
            actual: header.validator_node_size,
        }));
    }
    Ok(())
}

pub fn check_permitted_output_types(
    constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    if !constants
        .permitted_output_types()
        .contains(&output.features.output_type)
    {
        return Err(ValidationError::OutputTypeNotPermitted {
            output_type: output.features.output_type,
        });
    }

    Ok(())
}

pub fn check_covenant_length(covenant: &Covenant, max_token_len: u32) -> Result<(), ValidationError> {
    if covenant.num_tokens() > max_token_len as usize {
        return Err(ValidationError::CovenantTooLarge {
            max_size: max_token_len as usize,
            actual_size: covenant.num_tokens(),
        });
    }

    Ok(())
}

pub fn check_permitted_range_proof_types(
    constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    let binding = constants.permitted_range_proof_types();
    let permitted_range_proof_types = binding.iter().find(|&&t| t.0 == output.features.output_type).ok_or(
        ValidationError::OutputTypeNotMatchedToRangeProofType {
            output_type: output.features.output_type,
        },
    )?;

    if !permitted_range_proof_types
        .1
        .contains(&output.features.range_proof_type)
    {
        return Err(ValidationError::RangeProofTypeNotPermitted {
            range_proof_type: output.features.range_proof_type,
        });
    }

    Ok(())
}

pub fn validate_input_version(
    consensus_constants: &ConsensusConstants,
    input: &TransactionInput,
) -> Result<(), ValidationError> {
    if !consensus_constants.input_version_range().contains(&input.version) {
        let msg = format!(
            "Transaction input contains a version not allowed by consensus ({:?})",
            input.version
        );
        return Err(ValidationError::ConsensusError(msg));
    }

    Ok(())
}

pub fn validate_output_version(
    consensus_constants: &ConsensusConstants,
    output: &TransactionOutput,
) -> Result<(), ValidationError> {
    let valid_output_version = consensus_constants
        .output_version_range()
        .outputs
        .contains(&output.version);

    if !valid_output_version {
        let msg = format!(
            "Transaction output version is not allowed by consensus ({:?})",
            output.version
        );
        return Err(ValidationError::ConsensusError(msg));
    }

    let valid_features_version = consensus_constants
        .output_version_range()
        .features
        .contains(&output.features.version);

    if !valid_features_version {
        let msg = format!(
            "Transaction output features version is not allowed by consensus ({:?})",
            output.features.version
        );
        return Err(ValidationError::ConsensusError(msg));
    }

    for opcode in output.script.as_slice() {
        if !consensus_constants
            .output_version_range()
            .opcode
            .contains(&opcode.get_version())
        {
            let msg = format!(
                "Transaction output script opcode is not allowed by consensus ({})",
                opcode
            );
            return Err(ValidationError::ConsensusError(msg));
        }
    }

    Ok(())
}

pub fn validate_kernel_version(
    consensus_constants: &ConsensusConstants,
    kernel: &TransactionKernel,
) -> Result<(), ValidationError> {
    if !consensus_constants.kernel_version_range().contains(&kernel.version) {
        let msg = format!(
            "Transaction kernel version is not allowed by consensus ({:?})",
            kernel.version
        );
        return Err(ValidationError::ConsensusError(msg));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use tari_test_utils::unpack_enum;

    use super::*;
    use crate::transactions::{test_helpers, test_helpers::TestParams, CryptoFactories};

    mod is_all_unique_and_sorted {
        use super::*;

        #[test]
        fn it_returns_true_when_nothing_to_compare() {
            assert!(is_all_unique_and_sorted::<_, usize>(&[]));
            assert!(is_all_unique_and_sorted(&[1]));
        }

        #[test]
        fn it_returns_true_when_unique_and_sorted() {
            let v = [1, 2, 3, 4, 5];
            assert!(is_all_unique_and_sorted(&v));
        }

        #[test]
        fn it_returns_false_when_unsorted() {
            let v = [2, 1, 3, 4, 5];
            assert!(!is_all_unique_and_sorted(&v));
        }

        #[test]
        fn it_returns_false_when_duplicate() {
            let v = [1, 2, 3, 4, 4];
            assert!(!is_all_unique_and_sorted(&v));
        }

        #[test]
        fn it_returns_false_when_duplicate_and_unsorted() {
            let v = [4, 2, 3, 0, 4];
            assert!(!is_all_unique_and_sorted(&v));
        }
    }

    mod calc_median_timestamp {
        use super::*;

        #[test]
        fn it_errors_on_empty() {
            assert!(calc_median_timestamp(&[]).is_err());
        }

        #[test]
        fn it_calculates_the_correct_median_timestamp() {
            let median_timestamp = calc_median_timestamp(&[0.into()]).unwrap();
            assert_eq!(median_timestamp, 0.into());

            let median_timestamp = calc_median_timestamp(&[123.into()]).unwrap();
            assert_eq!(median_timestamp, 123.into());

            let median_timestamp = calc_median_timestamp(&[2.into(), 4.into()]).unwrap();
            assert_eq!(median_timestamp, 3.into());

            let median_timestamp = calc_median_timestamp(&[0.into(), 100.into(), 0.into()]).unwrap();
            assert_eq!(median_timestamp, 100.into());

            let median_timestamp = calc_median_timestamp(&[1.into(), 2.into(), 3.into(), 4.into()]).unwrap();
            assert_eq!(median_timestamp, 2.into());

            let median_timestamp = calc_median_timestamp(&[1.into(), 2.into(), 3.into(), 4.into(), 5.into()]).unwrap();
            assert_eq!(median_timestamp, 3.into());
        }
    }

    mod check_coinbase_maturity {
        use futures::executor::block_on;

        use super::*;
        use crate::transactions::{
            aggregated_body::AggregateBody,
            key_manager::create_memory_db_key_manager,
            transaction_components::{RangeProofType, TransactionError},
        };

        #[tokio::test]
        async fn it_succeeds_for_valid_coinbase() {
            let height = 1;
            let key_manager = create_memory_db_key_manager();
            let test_params = TestParams::new(&key_manager).await;
            let rules = test_helpers::create_consensus_manager();
            let key_manager = create_memory_db_key_manager();
            let coinbase = block_on(test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::RevealedValue,
            ));
            let coinbase_output = coinbase.to_transaction_output(&key_manager).await.unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(&coinbase.spending_key_id, &key_manager).await;

            let body = AggregateBody::new(vec![], vec![coinbase_output], vec![coinbase_kernel]);

            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();
            body.check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height)
                .unwrap();
        }

        #[tokio::test]
        async fn it_returns_error_for_invalid_coinbase_maturity() {
            let height = 1;
            let key_manager = create_memory_db_key_manager();
            let test_params = TestParams::new(&key_manager).await;
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase =
                test_helpers::create_coinbase_wallet_output(&test_params, height, None, RangeProofType::RevealedValue)
                    .await;
            coinbase.features.maturity = 0;
            let coinbase_output = coinbase.to_transaction_output(&key_manager).await.unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(&coinbase.spending_key_id, &key_manager).await;

            let body = AggregateBody::new(vec![], vec![coinbase_output], vec![coinbase_kernel]);

            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();

            let err = body
                .check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height)
                .unwrap_err();
            unpack_enum!(TransactionError::InvalidCoinbaseMaturity = err);
        }

        #[tokio::test]
        async fn it_returns_error_for_invalid_coinbase_reward() {
            let height = 1;
            let key_manager = create_memory_db_key_manager();
            let test_params = TestParams::new(&key_manager).await;
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::BulletProofPlus,
            )
            .await;
            coinbase.value = 123.into();
            let coinbase_output = coinbase.to_transaction_output(&key_manager).await.unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(&coinbase.spending_key_id, &key_manager).await;

            let body = AggregateBody::new(vec![], vec![coinbase_output], vec![coinbase_kernel]);
            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();

            let err = body
                .check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height)
                .unwrap_err();
            unpack_enum!(TransactionError::InvalidCoinbase = err);
        }
    }
}
