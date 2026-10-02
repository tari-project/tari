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
use tari_common_types::{epoch::VnEpoch, types::FixedHash};
use tari_crypto::tari_utilities::{epoch_time::EpochTime, hex::Hex};
use tari_node_components::blocks::{BlockHeader, BlockHeaderValidationError, BlockValidationError};
use tari_transaction_components::{
    consensus::ConsensusConstants,
    tari_proof_of_work::{PowAlgorithm, PowError},
    transaction_components::{TransactionInput, TransactionOutput},
};

use crate::{
    chain_storage::{BlockchainBackend, MmrRoots, MmrTree},
    consensus::BaseNodeConsensusManager,
    proof_of_work::{
        AchievedTargetDifficulty,
        AdjustedTarget,
        cuckaroo_pow::cuckaroo_difficulty,
        monero_randomx_difficulty,
        randomx_factory::RandomXFactory,
        sha3x_difficulty,
        tari_randomx_difficulty,
    },
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::helpers";

/// Returns the median timestamp for the provided timestamps.
///
/// ## Panics
/// When an empty slice is given as this is undefined for median average.
/// https://math.stackexchange.com/a/3451015
pub fn calc_median_timestamp(timestamps: &[EpochTime]) -> Result<EpochTime, ValidationError> {
    let mut timestamps: Vec<EpochTime> = timestamps.to_vec();
    timestamps.sort();
    trace!(
        target: LOG_TARGET,
        "Calculate the median timestamp from {} timestamps",
        timestamps.len()
    );
    if timestamps.is_empty() {
        return Err(ValidationError::IncorrectNumberOfTimestampsProvided { expected: 1, actual: 0 });
    }

    let mid_index = timestamps.len() / 2;
    let median_timestamp = if timestamps.len().is_multiple_of(2) {
        trace!(
            target: LOG_TARGET,
            "No median timestamp available, estimating median as avg of [{}] and [{}]",
            timestamps.get(mid_index.saturating_sub(1)).expect("Already checked"),
            timestamps.get(mid_index).expect("Already checked"),
        );
        // To compute this mean, we use `u128` to avoid overflow with the internal `u64` typing
        // Note that the final cast back to `u64` will never truncate since each summand is bounded by `u64`
        // To make the linter happy, we use `u64::MAX` in the impossible case that the cast fails
        EpochTime::from(
            u64::try_from(
                u128::from(
                    timestamps
                        .get(mid_index.saturating_sub(1))
                        .expect("Already checked")
                        .as_u64(),
                )
                .saturating_add(u128::from(timestamps.get(mid_index).expect("Already checked").as_u64())) /
                    2,
            )
            .unwrap_or(u64::MAX),
        )
    } else {
        *timestamps.get(mid_index).expect("Already checked")
    };
    trace!(target: LOG_TARGET, "Median timestamp:{median_timestamp}");
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
    if block_header.timestamp <= median_timestamp {
        warn!(
            target: LOG_TARGET,
            "Block header timestamp {} is less or equal than median timestamp: {} for block:{}",
            block_header.timestamp,
            median_timestamp,
            block_header.hash().to_hex()
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestamp(format!(
                "The timestamp `{}` was less or equal than the median timestamp `{}`",
                block_header.timestamp, median_timestamp
            )),
        ));
    }

    Ok(())
}
pub fn check_target_difficulty(
    block_header: &BlockHeader,
    target: AdjustedTarget,
    randomx_factory: &RandomXFactory,
    gen_hash: &FixedHash,
    consensus: &BaseNodeConsensusManager,
    tari_vm_key: FixedHash,
) -> Result<AchievedTargetDifficulty, ValidationError> {
    let achieved = match block_header.pow_algo() {
        PowAlgorithm::RandomXM => monero_randomx_difficulty(block_header, randomx_factory, gen_hash, consensus)?,
        PowAlgorithm::RandomXT => tari_randomx_difficulty(block_header, randomx_factory, &tari_vm_key)?,
        PowAlgorithm::Sha3x => sha3x_difficulty(block_header)?,
        PowAlgorithm::Cuckaroo => {
            let constants = consensus.consensus_constants(block_header.height);
            cuckaroo_difficulty(
                block_header,
                constants.cuckaroo_cycle_length(),
                constants.cuckaroo_edge_bits(),
                constants.bipartite_cuckaroo_verification(),
            )?
        },
    };
    match AchievedTargetDifficulty::try_construct(block_header.pow_algo(), target.base, target.adjusted, achieved) {
        Some(achieved_target) => Ok(achieved_target),
        None => {
            warn!(
                target: LOG_TARGET,
                "Proof of work for {} at height {} was below the target difficulty. Achieved: {}, Target: {} (adjusted: {})",
                block_header.hash().to_hex(),
                block_header.height,
                achieved,
                target.base,
                target.adjusted
            );
            Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::ProofOfWorkError(PowError::AchievedDifficultyTooLow {
                    achieved,
                    target: target.adjusted,
                }),
            ))
        },
    }
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

        // Diagnostic logging only; any of the (content-identical) entries for this hash is fine.
        let output = db.fetch_outputs(&utxo_hash)?.into_iter().next();
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
    if !db.fetch_outputs(&output_hash)?.is_empty() {
        warn!(
            target: LOG_TARGET,
            "Validation failed due to already spent input: {input}"
        );
        // The commitment is not in the unspent set (checked above) but the output exists in the index,
        // so it must already be spent.
        return Err(ValidationError::ContainsSTxO);
    }

    debug!(
        target: LOG_TARGET,
        "Input ({}, {}) does not exist in the database yet", input.commitment()?.to_hex(), output_hash.to_hex()
    );
    Err(ValidationError::UnknownInput)
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
            "Duplicate UTXO set commitment found for output: {output}"
        );
        return Err(ValidationError::ContainsDuplicateUtxoCommitment);
    }

    Ok(())
}
/// This function checks the validity of the validator node registration if applicable
pub fn check_validator_node_registration<B: BlockchainBackend>(
    db: &B,
    output: &TransactionOutput,
    current_epoch: VnEpoch,
) -> Result<(), ValidationError> {
    let Some(sidechain_features) = output.features.sidechain_feature.as_ref() else {
        return Ok(());
    };
    let Some(vn_reg) = sidechain_features.validator_node_registration() else {
        return Ok(());
    };

    if vn_reg.max_epoch() < current_epoch {
        return Err(ValidationError::ValidatorNodeRegistrationMaxEpoch {
            public_key: vn_reg.public_key().to_string(),
            max_epoch: vn_reg.max_epoch(),
            current_epoch,
        });
    }

    if db.validator_node_exists(
        sidechain_features.sidechain_public_key(),
        current_epoch,
        vn_reg.public_key(),
    )? {
        return Err(ValidationError::ValidatorNodeAlreadyRegistered {
            public_key: vn_reg.public_key().to_string(),
        });
    }

    // A validator node that has exited but whose exit epoch has not been reached yet is no longer in the registered
    // set (so `validator_node_exists` is false), but still sits in the exit queue and still counts as active. A
    // re-registration in that window would let a second exit be queued while the first entry still exists, which can
    // collide in the exit queue at commit time (and make `undo_exit` restore the wrong instance on a reorg). It must
    // wait until the pending exit has taken effect.
    if db.validator_node_is_active(
        sidechain_features.sidechain_public_key(),
        current_epoch,
        vn_reg.public_key(),
    )? {
        return Err(ValidationError::ValidatorNodeAlreadyRegistered {
            public_key: vn_reg.public_key().to_string(),
        });
    }

    Ok(())
}

/// Checks the validity of the validator node exit if applicable.
///
/// Everything the commit-time exit (`ValidatorNodeStore::exit` via `get_next_exit_epoch`) can fail on must be rejected
/// here instead. A failure at commit time is not a validation failure: it surfaces as `AddBlockErrored`, so a locally
/// built block template carrying the exit would keep failing without the offending transaction being evicted.
///
/// The exit is accepted only if:
/// - exits are permitted at all (`vn_registration_max_exits_per_epoch > 0`; with zero, `get_next_exit_epoch` errors);
/// - the validator node is in the registered validator node set (NOT already queued in the exit queue - `exit()` reads
///   only the registered set, so a second exit for a queued validator fails at commit time). Note that
///   `validator_node_is_active` cannot be used for this: it deliberately still reports a queued validator as active
///   until its exit epoch so that the registration UTXO stays locked;
/// - the validator node has activated by `current_epoch`;
/// - the exit's `activation_epoch` (which is part of the signed message) matches the registered activation epoch,
///   binding the exit to this registration instance.
///
/// Duplicate exits within a single body are rejected by `verify_no_duplicate_validator_node_registrations`.
pub fn check_validator_node_exit<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    output: &TransactionOutput,
    current_epoch: VnEpoch,
) -> Result<(), ValidationError> {
    let Some(sidechain_features) = output.features.sidechain_feature.as_ref() else {
        return Ok(());
    };
    let Some(exit) = sidechain_features.validator_node_exit() else {
        return Ok(());
    };

    if constants.vn_registration_max_exits_per_epoch() == 0 {
        return Err(ValidationError::ValidatorNodeExitNotPermitted);
    }

    if exit.max_epoch() < current_epoch {
        return Err(ValidationError::ValidatorNodeRegistrationMaxEpoch {
            public_key: exit.public_key().to_string(),
            max_epoch: exit.max_epoch(),
            current_epoch,
        });
    }

    let Some(entry) = db.fetch_validator_node_entry(sidechain_features.sidechain_public_key(), exit.public_key())?
    else {
        return Err(ValidationError::ValidatorNodeNotRegistered {
            public_key: exit.public_key().to_string(),
            details: format!(
                "exit invalid for validator node that is not registered or has already exited in {current_epoch}"
            ),
        });
    };

    if entry.activation_epoch > current_epoch {
        return Err(ValidationError::ValidatorNodeNotRegistered {
            public_key: exit.public_key().to_string(),
            details: format!(
                "exit invalid for validator node that only activates in {} (current epoch {current_epoch})",
                entry.activation_epoch
            ),
        });
    }

    if entry.activation_epoch != exit.activation_epoch() {
        return Err(ValidationError::ValidatorNodeExitActivationEpochMismatch {
            public_key: exit.public_key().to_string(),
            exit_activation_epoch: exit.activation_epoch(),
            registered_activation_epoch: entry.activation_epoch,
        });
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
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
            kind: "Utxos",
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
    };
    if header.block_output_mr != mmr_roots.block_output_mr {
        warn!(
            target: LOG_TARGET,
            "Block header block output MMR roots in #{} {} do not match calculated roots. Expected: {}, Actual:{}",
            header.height,
            header.hash().to_hex(),
            header.block_output_mr,
            mmr_roots.block_output_mr,
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots {
            kind: "block outputs",
        }));
    };
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

#[cfg(test)]
mod test {
    use tari_test_utils::unpack_enum;
    use tari_transaction_components::{crypto_factories::CryptoFactories, test_helpers, test_helpers::TestParams};

    use super::*;

    mod is_all_unique_and_sorted {
        use tari_transaction_components::validation::helpers::is_all_unique_and_sorted;

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
            assert_eq!(median_timestamp, 0.into());

            let median_timestamp = calc_median_timestamp(&[1.into(), 2.into(), 3.into(), 4.into()]).unwrap();
            assert_eq!(median_timestamp, 2.into());

            let median_timestamp = calc_median_timestamp(&[1.into(), 2.into(), 3.into(), 4.into(), 5.into()]).unwrap();
            assert_eq!(median_timestamp, 3.into());
        }
    }

    mod check_coinbase_maturity {
        use tari_transaction_components::{
            aggregated_body::AggregateBody,
            key_manager::KeyManager,
            transaction_components::{RangeProofType, TransactionError},
        };

        use super::*;
        #[tokio::test]
        async fn it_succeeds_for_valid_coinbase() {
            let height = 1;
            let key_manager = KeyManager::new_random().unwrap();
            let test_params = TestParams::new(&key_manager);
            let rules = test_helpers::create_consensus_manager();
            let coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::RevealedValue,
                &key_manager,
            );
            let coinbase_output = coinbase.to_transaction_output().unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager);

            let body = AggregateBody::new_unsorted(vec![], vec![coinbase_output], vec![coinbase_kernel]);

            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();
            body.check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height, 1)
                .unwrap();
        }

        #[tokio::test]
        async fn it_returns_error_for_invalid_coinbase_maturity() {
            let height = 1;
            let key_manager = KeyManager::new_random().unwrap();
            let test_params = TestParams::new(&key_manager);
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::RevealedValue,
                &key_manager,
            );
            let mut features = coinbase.features().clone();
            features.maturity = 0;
            coinbase.set_features(features);
            let coinbase_output = coinbase.to_transaction_output().unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager);

            let body = AggregateBody::new_unsorted(vec![], vec![coinbase_output], vec![coinbase_kernel]);

            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();

            let err = body
                .check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height, 1)
                .unwrap_err();
            unpack_enum!(TransactionError::InvalidCoinbaseMaturity = err);
        }

        #[tokio::test]
        async fn it_returns_error_for_invalid_coinbase_reward() {
            let height = 1;
            let key_manager = KeyManager::new_random().unwrap();
            let test_params = TestParams::new(&key_manager);
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_coinbase_wallet_output(
                &test_params,
                height,
                None,
                RangeProofType::BulletProofPlus,
                &key_manager,
            );
            coinbase.set_value(123.into(), &key_manager).unwrap();
            let coinbase_output = coinbase.to_transaction_output().unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(coinbase.commitment_mask_key_id(), &key_manager);

            let body = AggregateBody::new_unsorted(vec![], vec![coinbase_output], vec![coinbase_kernel]);
            let reward = rules.calculate_coinbase_and_fees(height, body.kernels()).unwrap();
            let coinbase_lock_height = rules.consensus_constants(height).coinbase_min_maturity();

            let err = body
                .check_coinbase_output(reward, coinbase_lock_height, &CryptoFactories::default(), height, 1)
                .unwrap_err();
            unpack_enum!(TransactionError::InvalidCoinbase = err);
        }
    }
}
