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

use std::collections::HashSet;

use log::*;
use tari_common_types::types::{Commitment, CommitmentFactory, FixedHash, PublicKey};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::PublicKey as PublicKeyTrait,
    tari_utilities::{epoch_time::EpochTime, hex::Hex},
};
use tari_script::TariScript;

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
        transaction_components::{KernelSum, TransactionError, TransactionInput, TransactionKernel, TransactionOutput},
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
    use PowAlgorithm::{Monero, Sha3};
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
            Some(block.header.prev_hash),
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

/// THis function checks the total burned sum in the header ensuring that every burned output is counted in the total
/// sum.
#[allow(clippy::mutable_key_type)]
pub fn check_total_burned(body: &AggregateBody) -> Result<(), ValidationError> {
    let mut burned_outputs = HashSet::new();
    for output in body.outputs() {
        if output.is_burned() {
            // we dont care about duplicate commitments are they should have already been checked
            burned_outputs.insert(output.commitment.clone());
        }
    }
    for kernel in body.kernels() {
        if kernel.is_burned() && !burned_outputs.remove(kernel.get_burn_commitment()?) {
            return Err(ValidationError::InvalidBurnError(
                "Burned kernel does not match burned output".to_string(),
            ));
        }
    }

    if !burned_outputs.is_empty() {
        return Err(ValidationError::InvalidBurnError(
            "Burned output has no matching burned kernel".to_string(),
        ));
    }
    Ok(())
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

    for input in body.inputs() {
        // If spending a unique_id, a new output must contain the unique id
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
                not_found_inputs.push(output_hash);
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
        "Validation failed due to input: {} which does not exist yet", input
    );
    Err(ValidationError::UnknownInput)
}

/// This function checks:
/// 1. that the output type is permitted
/// 2. the byte size of TariScript does not exceed the maximum
/// 3. that the outputs do not already exist in the UTxO set.
pub fn check_outputs<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    body: &AggregateBody,
) -> Result<(), ValidationError> {
    let max_script_size = constants.get_max_script_byte_size();
    for output in body.outputs() {
        check_permitted_output_types(constants, output)?;
        check_tari_script_byte_size(&output.script, max_script_size)?;
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

pub fn check_not_bad_block<B: BlockchainBackend>(db: &B, hash: FixedHash) -> Result<(), ValidationError> {
    if db.bad_block_exists(hash)? {
        return Err(ValidationError::BadBlockFound { hash: hash.to_hex() });
    }
    Ok(())
}

/// This checks to ensure that every kernel included in the block is a unique kernel in the block chain.
pub fn check_unique_kernels<B: BlockchainBackend>(db: &B, block_body: &AggregateBody) -> Result<(), ValidationError> {
    for kernel in block_body.kernels() {
        if let Some((db_kernel, header_hash)) = db.fetch_kernel_by_excess_sig(&kernel.excess_sig)? {
            let msg = format!(
                "Block contains kernel excess: {} which matches already existing excess signature in chain database \
                 block hash: {}. Existing kernel excess: {}, excess sig nonce: {}, excess signature: {}",
                kernel.excess.to_hex(),
                header_hash.to_hex(),
                db_kernel.excess.to_hex(),
                db_kernel.excess_sig.get_public_nonce().to_hex(),
                db_kernel.excess_sig.get_signature().to_hex(),
            );
            warn!(target: LOG_TARGET, "{}", msg);
            return Err(ValidationError::ConsensusError(msg));
        };
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
    height: u64,
    total_fees: MicroTari,
    coinbase_kernel: &TransactionKernel,
    coinbase_output: &TransactionOutput,
) -> Result<(), ValidationError> {
    let reward = rules.emission_schedule().block_reward(height) + total_fees;
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

pub fn check_coinbase_maturity(
    rules: &ConsensusManager,
    height: u64,
    coinbase_output: &TransactionOutput,
) -> Result<(), ValidationError> {
    let constants = rules.consensus_constants(height);
    if coinbase_output.features.maturity < height + constants.coinbase_lock_height() {
        warn!(
            target: LOG_TARGET,
            "Coinbase {} found with maturity set too low", coinbase_output
        );
        return Err(ValidationError::TransactionError(
            TransactionError::InvalidCoinbaseMaturity,
        ));
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
                if mature {
                    Ok(0)
                } else {
                    warn!(
                        target: LOG_TARGET,
                        "Input found that has not yet matured to spending height: {}", input
                    );
                    Err(TransactionError::InputMaturity)
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

pub fn check_blockchain_version(constants: &ConsensusConstants, version: u16) -> Result<(), ValidationError> {
    if constants.valid_blockchain_version_range().contains(&version) {
        Ok(())
    } else {
        Err(ValidationError::InvalidBlockchainVersion { version })
    }
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

pub fn validate_versions(
    body: &AggregateBody,
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    // validate input version
    for input in body.inputs() {
        validate_input_version(consensus_constants, input)?;
    }

    // validate output version and output features version
    for output in body.outputs() {
        validate_output_version(consensus_constants, output)?;
    }

    // validate kernel version
    for kernel in body.kernels() {
        validate_kernel_version(consensus_constants, kernel)?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use tari_test_utils::unpack_enum;

    use super::*;
    use crate::transactions::{
        test_helpers,
        test_helpers::TestParams,
        transaction_components::{OutputFeatures, TransactionInputVersion},
    };

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

        #[test]
        fn it_checks_the_kernel_timelock() {
            let mut kernel = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::empty());
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

        #[test]
        fn it_checks_the_input_maturity() {
            let input = TransactionInput::new_with_output_data(
                TransactionInputVersion::get_current_version(),
                OutputFeatures {
                    maturity: 5,
                    ..Default::default()
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                MicroTari::zero(),
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

    mod check_coinbase_maturity {

        use super::*;

        #[test]
        fn it_succeeds_for_valid_coinbase() {
            let test_params = TestParams::new();
            let rules = test_helpers::create_consensus_manager();
            let coinbase = test_helpers::create_unblinded_coinbase(&test_params, 1);
            let coinbase_output = coinbase.as_transaction_output(&CryptoFactories::default()).unwrap();
            check_coinbase_maturity(&rules, 1, &coinbase_output).unwrap();
        }

        #[test]
        fn it_returns_error_for_invalid_coinbase_maturity() {
            let test_params = TestParams::new();
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_unblinded_coinbase(&test_params, 1);
            coinbase.features.maturity = 0;
            let coinbase_output = coinbase.as_transaction_output(&CryptoFactories::default()).unwrap();
            let err = check_coinbase_maturity(&rules, 1, &coinbase_output).unwrap_err();
            unpack_enum!(ValidationError::TransactionError(err) = err);
            unpack_enum!(TransactionError::InvalidCoinbaseMaturity = err);
        }
    }

    mod check_coinbase_reward {

        use super::*;

        #[test]
        fn it_succeeds_for_valid_coinbase() {
            let test_params = TestParams::new();
            let rules = test_helpers::create_consensus_manager();
            let coinbase = test_helpers::create_unblinded_coinbase(&test_params, 1);
            let coinbase_output = coinbase.as_transaction_output(&CryptoFactories::default()).unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(&coinbase.spending_key);
            check_coinbase_reward(
                &CommitmentFactory::default(),
                &rules,
                1,
                0.into(),
                &coinbase_kernel,
                &coinbase_output,
            )
            .unwrap();
        }

        #[test]
        fn it_returns_error_for_invalid_coinbase_reward() {
            let test_params = TestParams::new();
            let rules = test_helpers::create_consensus_manager();
            let mut coinbase = test_helpers::create_unblinded_coinbase(&test_params, 1);
            coinbase.value = 123.into();
            let coinbase_output = coinbase.as_transaction_output(&CryptoFactories::default()).unwrap();
            let coinbase_kernel = test_helpers::create_coinbase_kernel(&coinbase.spending_key);
            let err = check_coinbase_reward(
                &CommitmentFactory::default(),
                &rules,
                1,
                0.into(),
                &coinbase_kernel,
                &coinbase_output,
            )
            .unwrap_err();
            unpack_enum!(ValidationError::TransactionError(err) = err);
            unpack_enum!(TransactionError::InvalidCoinbase = err);
        }
    }

    use crate::{covenants::Covenant, transactions::transaction_components::KernelFeatures};

    #[test]
    fn check_burned_succeeds_for_valid_outputs() {
        let mut kernel1 = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::create_burn());
        let mut kernel2 = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::create_burn());

        let (output1, _, _) = test_helpers::create_utxo(
            100.into(),
            &CryptoFactories::default(),
            &OutputFeatures::create_burn_output(),
            &TariScript::default(),
            &Covenant::default(),
            0.into(),
        );
        let (output2, _, _) = test_helpers::create_utxo(
            101.into(),
            &CryptoFactories::default(),
            &OutputFeatures::create_burn_output(),
            &TariScript::default(),
            &Covenant::default(),
            0.into(),
        );
        let (output3, _, _) = test_helpers::create_utxo(
            102.into(),
            &CryptoFactories::default(),
            &OutputFeatures::create_burn_output(),
            &TariScript::default(),
            &Covenant::default(),
            0.into(),
        );

        kernel1.burn_commitment = Some(output1.commitment.clone());
        kernel2.burn_commitment = Some(output2.commitment.clone());
        let kernel3 = kernel1.clone();

        let mut body = AggregateBody::new(Vec::new(), vec![output1.clone(), output2.clone()], vec![
            kernel1.clone(),
            kernel2.clone(),
        ]);
        assert!(check_total_burned(&body).is_ok());
        // lets add an extra kernel
        body.add_kernels(&mut vec![kernel3]);
        assert!(check_total_burned(&body).is_err());
        // lets add a kernel commitment mismatch
        body.add_outputs(&mut vec![output3.clone()]);
        assert!(check_total_burned(&body).is_err());
        // Lets try one with a commitment with no kernel
        let body2 = AggregateBody::new(Vec::new(), vec![output1, output2, output3], vec![kernel1, kernel2]);
        assert!(check_total_burned(&body2).is_err());
    }
}
