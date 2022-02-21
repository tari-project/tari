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

use log::*;
use tari_common_types::types::{Commitment, CommitmentFactory, PublicKey};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::PublicKey as PublicKeyTrait,
    script::TariScript,
    tari_utilities::{
        epoch_time::EpochTime,
        hash::Hashable,
        hex::{to_hex, Hex},
    },
};

use crate::{
    blocks::{Block, BlockHeader, BlockHeaderValidationError, BlockValidationError},
    chain_storage::{BlockchainBackend, MmrRoots, MmrTree},
    consensus::{emission::Emission, ConsensusConstants, ConsensusEncodingSized, ConsensusManager},
    proof_of_work::{
        monero_difficulty,
        monero_rx::MoneroPowData,
        randomx_factory::RandomXFactory,
        sha3_difficulty,
        AchievedTargetDifficulty,
        Difficulty,
        PowAlgorithm,
        PowError,
    },
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroTari,
        transaction_components::{
            KernelSum,
            OutputFlags,
            TransactionError,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
        },
        CryptoFactories,
    },
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::helpers";

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

/// Returns the median timestamp for the provided timestamps.
///
/// ## Panics
/// When an empty slice is given as this is undefined for median average.
/// https://math.stackexchange.com/a/3451015
pub fn calc_median_timestamp(timestamps: &[EpochTime]) -> EpochTime {
    assert!(
        !timestamps.is_empty(),
        "calc_median_timestamp: timestamps cannot be empty"
    );
    trace!(
        target: LOG_TARGET,
        "Calculate the median timestamp from {} timestamps",
        timestamps.len()
    );

    let mid_index = timestamps.len() / 2;
    let median_timestamp = if timestamps.len() % 2 == 0 {
        trace!(
            target: LOG_TARGET,
            "No median timestamp available, estimating median as avg of [{}] and [{}]",
            timestamps[mid_index - 1],
            timestamps[mid_index],
        );
        (timestamps[mid_index - 1] + timestamps[mid_index]) / 2
    } else {
        timestamps[mid_index]
    };
    trace!(target: LOG_TARGET, "Median timestamp:{}", median_timestamp);
    median_timestamp
}

pub fn check_header_timestamp_greater_than_median(
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

/// Check the PoW data in the BlockHeader. This currently only applies to blocks merged mined with Monero.
pub fn check_pow_data<B: BlockchainBackend>(
    block_header: &BlockHeader,
    rules: &ConsensusManager,
    db: &B,
) -> Result<(), ValidationError> {
    use PowAlgorithm::*;
    match block_header.pow.pow_algo {
        Monero => {
            let monero_data =
                MoneroPowData::from_header(block_header).map_err(|e| ValidationError::CustomError(e.to_string()))?;
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
        Sha3 => {
            if !block_header.pow.pow_data.is_empty() {
                return Err(ValidationError::CustomError(
                    "Proof of work data must be empty for Sha3 blocks".to_string(),
                ));
            }
            Ok(())
        },
    }
}

pub fn check_target_difficulty(
    block_header: &BlockHeader,
    target: Difficulty,
    randomx_factory: &RandomXFactory,
) -> Result<AchievedTargetDifficulty, ValidationError> {
    let achieved = match block_header.pow_algo() {
        PowAlgorithm::Monero => monero_difficulty(block_header, randomx_factory)?,
        PowAlgorithm::Sha3 => sha3_difficulty(block_header),
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

pub fn check_block_weight(block: &Block, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
    // The genesis block has a larger weight than other blocks may have so we have to exclude it here
    let block_weight = block.body.calculate_weight(consensus_constants.transaction_weight());
    let max_weight = consensus_constants.get_max_block_transaction_weight();
    if block_weight <= max_weight || block.header.height == 0 {
        trace!(
            target: LOG_TARGET,
            "SV - Block contents for block #{} : {}; weight {}.",
            block.header.height,
            block.body.to_counts_string(),
            block_weight,
        );

        Ok(())
    } else {
        Err(BlockValidationError::BlockTooLarge {
            actual_weight: block_weight,
            max_weight,
        }
        .into())
    }
}

pub fn check_accounting_balance(
    block: &Block,
    rules: &ConsensusManager,
    bypass_range_proof_verification: bool,
    factories: &CryptoFactories,
) -> Result<(), ValidationError> {
    if block.header.height == 0 {
        // Gen block does not need to be checked for this.
        return Ok(());
    }
    let offset = &block.header.total_kernel_offset;
    let script_offset = &block.header.total_script_offset;
    let total_coinbase = rules.calculate_coinbase_and_fees(block.header.height, block.body.kernels());
    block
        .body
        .validate_internal_consistency(
            offset,
            script_offset,
            bypass_range_proof_verification,
            total_coinbase,
            factories,
            Some(block.header.prev_hash.clone()),
            block.header.height,
        )
        .map_err(|err| {
            warn!(
                target: LOG_TARGET,
                "Validation failed on block:{}:{:?}",
                block.hash().to_hex(),
                err
            );
            ValidationError::TransactionError(err)
        })
}

pub fn check_coinbase_output(
    block: &Block,
    rules: &ConsensusManager,
    factories: &CryptoFactories,
) -> Result<(), ValidationError> {
    let total_coinbase = rules.calculate_coinbase_and_fees(block.header.height, block.body.kernels());
    block
        .check_coinbase_output(
            total_coinbase,
            rules.consensus_constants(block.header.height),
            factories,
        )
        .map_err(ValidationError::from)
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

// This function checks for duplicate inputs and outputs. There should be no duplicate inputs or outputs in a block
pub fn check_sorting_and_duplicates(body: &AggregateBody) -> Result<(), ValidationError> {
    if !is_all_unique_and_sorted(body.inputs()) {
        return Err(ValidationError::UnsortedOrDuplicateInput);
    }

    if !is_all_unique_and_sorted(body.outputs()) {
        return Err(ValidationError::UnsortedOrDuplicateOutput);
    }

    if !is_all_unique_and_sorted(body.kernels()) {
        return Err(ValidationError::UnsortedOrDuplicateKernel);
    }

    Ok(())
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spent
pub fn check_inputs_are_utxos<B: BlockchainBackend>(db: &B, body: &AggregateBody) -> Result<(), ValidationError> {
    let mut not_found_inputs = Vec::new();
    let mut output_hashes = None;
    let output_unique_ids = body
        .outputs()
        .iter()
        .filter_map(|output| {
            output
                .features
                .unique_id
                .as_ref()
                .map(|ui| (output.features.parent_public_key.as_ref(), ui))
        })
        .collect::<Vec<_>>();
    for input in body.inputs() {
        // If spending a unique_id, a new output must contain the unique id
        if let Some(ref unique_id) = input.features()?.unique_id {
            let exactly_one = output_unique_ids
                .iter()
                .filter_map(|(parent_public_key, output_unique_id)| match input.features() {
                    Ok(features) => {
                        if features.parent_public_key.as_ref() == *parent_public_key && unique_id == *output_unique_id {
                            Some(Ok((parent_public_key, output_unique_id)))
                        } else {
                            None
                        }
                    },
                    Err(e) => Some(Err(e)),
                })
                .take(2)
                .collect::<Result<Vec<_>, TransactionError>>()?;
            // Unless a burn flag is present
            if input.features()?.flags.contains(OutputFlags::BURN_NON_FUNGIBLE) {
                if !exactly_one.is_empty() {
                    return Err(ValidationError::UniqueIdBurnedButPresentInOutputs);
                }
            } else {
                if exactly_one.is_empty() {
                    return Err(ValidationError::UniqueIdInInputNotPresentInOutputs);
                }
                if exactly_one.len() > 1 {
                    return Err(ValidationError::DuplicateUniqueIdInOutputs);
                }
            }
        }
        match check_input_is_utxo(db, input) {
            Ok(_) => continue,
            Err(ValidationError::UnknownInput) => {
                // Lazily allocate and hash outputs as needed
                if output_hashes.is_none() {
                    output_hashes = Some(body.outputs().iter().map(|output| output.hash()).collect::<Vec<_>>());
                }

                let output_hashes = output_hashes.as_ref().unwrap();
                let output_hash = input.output_hash();
                if output_hashes.iter().any(|output| output == &output_hash) {
                    continue;
                }

                warn!(
                    target: LOG_TARGET,
                    "Validation failed due to input: {} which does not exist yet", input
                );
                not_found_inputs.push(output_hash.clone());
            },
            Err(err) => {
                return Err(err);
            },
        }
    }

    if !not_found_inputs.is_empty() {
        return Err(ValidationError::UnknownInputs(not_found_inputs));
    }

    Ok(())
}

/// This function checks that an input is a valid spendable UTXO
pub fn check_input_is_utxo<B: BlockchainBackend>(db: &B, input: &TransactionInput) -> Result<(), ValidationError> {
    let output_hash = input.output_hash();
    if let Some(utxo_hash) = db.fetch_unspent_output_hash_by_commitment(input.commitment()?)? {
        // We know that the commitment exists in the UTXO set. Check that the output hash matches (i.e. all fields
        // like output features match)
        if utxo_hash == output_hash {
            // Check that the input found by commitment, matches the input given here
            match db
                .fetch_output(&utxo_hash)?
                .and_then(|output| output.output.into_unpruned_output())
            {
                Some(output) => {
                    let mut compact = input.to_compact();
                    compact.add_output_data(
                        output.version,
                        output.features,
                        output.commitment,
                        output.script,
                        output.sender_offset_public_key,
                        output.covenant,
                    );
                    let input_hash = input.canonical_hash()?;
                    if compact.canonical_hash()? != input_hash {
                        warn!(
                            target: LOG_TARGET,
                            "Input '{}' spends commitment '{}' found in the UTXO set but does not contain the \
                             matching metadata fields.",
                            input_hash.to_hex(),
                            input.commitment()?.to_hex(),
                        );
                        return Err(ValidationError::UnknownInput);
                    }
                },
                None => {
                    error!(
                        target: LOG_TARGET,
                        "🚨 Output '{}' was in unspent but was pruned - this indicates a blockchain database \
                         inconsistency!",
                        output_hash.to_hex()
                    );
                    return Err(ValidationError::UnknownInput);
                },
            }

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
        return Err(ValidationError::BlockError(BlockValidationError::InvalidInput));
    }

    if let Some(unique_id) = &input.features()?.unique_id {
        if let Some(utxo_hash) =
            db.fetch_utxo_by_unique_id(input.features()?.parent_public_key.as_ref(), unique_id, None)?
        {
            // Check that it is the same utxo in which the unique_id was created
            if utxo_hash.output.hash() == output_hash {
                return Ok(());
            }

            warn!(
                target: LOG_TARGET,
                "Input spends a UTXO but has a duplicate unique_id:
            {}",
                input
            );
            return Err(ValidationError::BlockError(BlockValidationError::InvalidInput));
        }
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
        "Validation failed due to input: {} which does not exist yet", input
    );
    Err(ValidationError::UnknownInput)
}

/// This function checks:
/// 1. the byte size of TariScript does not exceed the maximum
/// 2. that the outputs do not already exist in the UTxO set.
pub fn check_outputs<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    body: &AggregateBody,
) -> Result<(), ValidationError> {
    let mut unique_ids = Vec::new();
    let max_script_size = constants.get_max_script_byte_size();
    for output in body.outputs() {
        check_tari_script_byte_size(&output.script, max_script_size)?;
        // Check outputs for duplicate asset ids
        if output.features.is_non_fungible_mint() || output.features.is_non_fungible_burn() {
            if let Some(unique_id) = output.features.unique_asset_id() {
                let parent_pk = output.features.parent_public_key.as_ref();

                let asset_tuple = (parent_pk, unique_id);
                if unique_ids.contains(&asset_tuple) {
                    return Err(ValidationError::ContainsDuplicateUtxoUniqueID);
                }
                unique_ids.push(asset_tuple)
            }
        }
        check_not_duplicate_txo(db, output)?;
    }
    Ok(())
}

/// Checks the byte size of TariScript is less than or equal to the given size, otherwise returns an error.
pub fn check_tari_script_byte_size(script: &TariScript, max_script_size: usize) -> Result<(), ValidationError> {
    let script_size = script.consensus_encode_exact_size();
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
    if let Some(index) = db.fetch_mmr_leaf_index(MmrTree::Utxo, &output.hash())? {
        warn!(
            target: LOG_TARGET,
            "Validation failed due to previously spent output: {} (MMR index = {})", output, index
        );
        return Err(ValidationError::ContainsTxO);
    }
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

    if let Some(unique_id) = &output.features.unique_id {
        // Needs to have a mint flag
        if output.features.is_non_fungible_mint() &&
            db.fetch_utxo_by_unique_id(output.features.parent_public_key.as_ref(), unique_id, None)?
                .is_some()
        {
            warn!(
                target: LOG_TARGET,
                "A UTXO with unique_id {} and parent public key {} already exists for output: {}",
                unique_id.to_hex(),
                output
                    .features
                    .parent_public_key
                    .as_ref()
                    .map(|pk| pk.to_hex())
                    .unwrap_or_else(|| "<None>".to_string()),
                output
            );
            return Err(ValidationError::ContainsDuplicateUtxoUniqueID);
        }
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
    if header.witness_mr != mmr_roots.witness_mr {
        warn!(
            target: LOG_TARGET,
            "Block header witness MMR roots in {} do not match calculated roots",
            header.hash().to_hex()
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrRoots {
            kind: "Witness",
        }));
    };
    if header.output_mmr_size != mmr_roots.output_mmr_size {
        warn!(
            target: LOG_TARGET,
            "Block header output MMR size in {} does not match. Expected: {}, Actual: {}",
            header.hash().to_hex(),
            header.output_mmr_size,
            mmr_roots.output_mmr_size
        );
        return Err(ValidationError::BlockError(BlockValidationError::MismatchedMmrSize {
            mmr_tree: MmrTree::Utxo.to_string(),
            expected: mmr_roots.output_mmr_size,
            actual: header.output_mmr_size,
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
    Ok(())
}

pub fn check_not_bad_block<B: BlockchainBackend>(db: &B, hash: &[u8]) -> Result<(), ValidationError> {
    if db.bad_block_exists(hash.to_vec())? {
        return Err(ValidationError::BadBlockFound { hash: to_hex(hash) });
    }
    Ok(())
}

pub fn validate_covenants(block: &Block) -> Result<(), ValidationError> {
    for input in block.body.inputs() {
        let output_set_size = input
            .covenant()?
            .execute(block.header.height, input, block.body.outputs())?;
        trace!(target: LOG_TARGET, "{} output(s) passed covenant", output_set_size);
    }
    Ok(())
}

pub fn check_coinbase_reward(
    factory: &CommitmentFactory,
    rules: &ConsensusManager,
    header: &BlockHeader,
    total_fees: MicroTari,
    coinbase_kernel: &TransactionKernel,
    coinbase_output: &TransactionOutput,
) -> Result<(), ValidationError> {
    let reward = rules.emission_schedule().block_reward(header.height) + total_fees;
    let rhs = &coinbase_kernel.excess + &factory.commit_value(&Default::default(), reward.into());
    if rhs != coinbase_output.commitment {
        warn!(
            target: LOG_TARGET,
            "Coinbase {} amount validation failed", coinbase_output
        );
        return Err(ValidationError::TransactionError(TransactionError::InvalidCoinbase));
    }
    Ok(())
}

pub fn check_kernel_sum(
    factory: &CommitmentFactory,
    kernel_sum: &KernelSum,
    output_commitment_sum: &Commitment,
    input_commitment_sum: &Commitment,
) -> Result<(), ValidationError> {
    let KernelSum { sum: excess, fees } = kernel_sum;
    let sum_io = output_commitment_sum - input_commitment_sum;
    let fees = factory.commit_value(&Default::default(), fees.as_u64());
    if *excess != &sum_io + &fees {
        return Err(TransactionError::ValidationError(
            "Sum of inputs and outputs did not equal sum of kernels with fees".into(),
        )
        .into());
    }
    Ok(())
}

pub fn check_script_offset(
    header: &BlockHeader,
    aggregate_offset_pubkey: &PublicKey,
    aggregate_input_key: &PublicKey,
) -> Result<(), ValidationError> {
    let script_offset = PublicKey::from_secret_key(&header.total_script_offset);
    let lhs = aggregate_input_key - aggregate_offset_pubkey;
    if lhs != script_offset {
        return Err(TransactionError::ScriptOffset.into());
    }
    Ok(())
}

/// Checks that all transactions (given by their kernels) are spendable at the given height
pub fn check_kernel_lock_height(height: u64, kernels: &[TransactionKernel]) -> Result<(), BlockValidationError> {
    if kernels.iter().any(|k| k.lock_height > height) {
        return Err(BlockValidationError::MaturityError);
    }
    Ok(())
}

/// Checks that all inputs have matured at the given height
pub fn check_maturity(height: u64, inputs: &[TransactionInput]) -> Result<(), TransactionError> {
    if let Err(e) = inputs
        .iter()
        .map(|input| match input.is_mature_at(height) {
            Ok(mature) => {
                if !mature {
                    warn!(
                        target: LOG_TARGET,
                        "Input found that has not yet matured to spending height: {}", input
                    );
                    Err(TransactionError::InputMaturity)
                } else {
                    Ok(0)
                }
            },
            Err(e) => Err(e),
        })
        .sum::<Result<usize, TransactionError>>()
    {
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

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
        #[should_panic]
        fn it_panics_if_empty() {
            calc_median_timestamp(&[]);
        }

        #[test]
        fn it_calculates_the_correct_median_timestamp() {
            let median_timestamp = calc_median_timestamp(&[0.into()]);
            assert_eq!(median_timestamp, 0.into());

            let median_timestamp = calc_median_timestamp(&[123.into()]);
            assert_eq!(median_timestamp, 123.into());

            let median_timestamp = calc_median_timestamp(&[2.into(), 4.into()]);
            assert_eq!(median_timestamp, 3.into());

            let median_timestamp = calc_median_timestamp(&[0.into(), 100.into(), 0.into()]);
            assert_eq!(median_timestamp, 100.into());

            let median_timestamp = calc_median_timestamp(&[1.into(), 2.into(), 3.into(), 4.into()]);
            assert_eq!(median_timestamp, 2.into());

            let median_timestamp = calc_median_timestamp(&[1.into(), 2.into(), 3.into(), 4.into(), 5.into()]);
            assert_eq!(median_timestamp, 3.into());
        }
    }

    mod check_lock_height {
        use super::*;
        use crate::transactions::test_helpers;

        #[test]
        fn it_checks_the_kernel_timelock() {
            let mut kernel = test_helpers::create_test_kernel(0.into(), 0);
            kernel.lock_height = 2;
            assert!(matches!(
                check_kernel_lock_height(1, &[kernel.clone()]),
                Err(BlockValidationError::MaturityError)
            ));

            check_kernel_lock_height(2, &[kernel.clone()]).unwrap();
            check_kernel_lock_height(3, &[kernel]).unwrap();
        }
    }

    mod check_maturity {
        use super::*;
        use crate::transactions::transaction_components::{OutputFeatures, TransactionInputVersion};

        #[test]
        fn it_checks_the_input_maturity() {
            let input = TransactionInput::new_with_output_data(
                TransactionInputVersion::get_current_version(),
                OutputFeatures::with_maturity(5),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            );

            assert!(matches!(
                check_maturity(1, &[input.clone()]),
                Err(TransactionError::InputMaturity)
            ));

            assert!(matches!(
                check_maturity(4, &[input.clone()]),
                Err(TransactionError::InputMaturity)
            ));

            check_maturity(5, &[input.clone()]).unwrap();
            check_maturity(6, &[input]).unwrap();
        }
    }
}
