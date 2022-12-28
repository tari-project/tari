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

use std::{cmp, convert::TryInto, thread, time::Instant};

use futures::{stream::FuturesUnordered, StreamExt};
use log::{debug, trace, warn};
use tari_common_types::types::{Commitment, CommitmentFactory, HashOutput, PublicKey};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::PublicKey as PublicKeyTrait};
use tari_script::{ScriptContext, TariScript};
use tari_utilities::hex::Hex;
use tokio::task;

use super::abort_on_drop::AbortOnDropJoinHandle;
use crate::{
    blocks::{Block, BlockHeader},
    borsh::SerializedSize,
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, MmrTree, PrunedOutput},
    consensus::{emission::Emission, ConsensusConstants, ConsensusManager},
    iterators::NonOverlappingIntegerPairIter,
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroTari,
        transaction_components::{
            transaction_output::batch_verify_range_proofs,
            KernelSum,
            TransactionError,
            TransactionInput,
            TransactionKernel,
            TransactionOutput,
        },
        CryptoFactories,
    },
    validation::ValidationError,
};

pub const LOG_TARGET: &str = "c::val::block_validator";

pub struct BlockValidator<B> {
    db: AsyncBlockchainDb<B>,
    consensus_rules: ConsensusManager,
    factories: CryptoFactories,
    concurrency: usize,
    bypass_range_proof_verification: bool,
}

impl<B: BlockchainBackend + 'static> BlockValidator<B> {
    pub fn new(
        db: AsyncBlockchainDb<B>,
        consensus_rules: ConsensusManager,
        bypass_range_proof_verification: bool,
        concurrency: usize,
    ) -> Self {
        Self {
            db,
            consensus_rules,
            factories: CryptoFactories::default(),
            concurrency,
            bypass_range_proof_verification,
        }
    }

    pub async fn validate(&self, block: Block) -> Result<(), ValidationError> {
        let (valid_header, inputs, outputs, kernels) = block.dissolve();

        // Start all validation tasks concurrently
        let kernels_task = self.start_kernel_validation(&valid_header, kernels);

        let inputs_task =
            self.start_input_validation(&valid_header, outputs.iter().map(|o| o.hash()).collect(), inputs);

        // Output order cannot be checked concurrently so it is checked here first
        if !is_all_unique_and_sorted(&outputs) {
            return Err(ValidationError::UnsortedOrDuplicateOutput);
        }
        let outputs_task = self.start_output_validation(&valid_header, outputs);

        // Wait for them to complete
        let outputs_result = outputs_task.await??;
        let inputs_result = inputs_task.await??;
        let kernels_result = kernels_task.await??;

        // Perform final checks using validation outputs
        check_coinbase_maturity(&self.consensus_rules, valid_header.height, outputs_result.coinbase())?;
        check_coinbase_reward(
            &self.factories.commitment,
            &self.consensus_rules,
            valid_header.height,
            kernels_result.kernel_sum.fees,
            kernels_result.coinbase(),
            outputs_result.coinbase(),
        )?;

        check_script_offset(
            &valid_header,
            &outputs_result.aggregate_offset_pubkey,
            &inputs_result.aggregate_input_key,
        )?;

        check_kernel_sum(
            &self.factories.commitment,
            &kernels_result.kernel_sum,
            &outputs_result.commitment_sum,
            &inputs_result.commitment_sum,
        )?;

        let block = Block::new(
            valid_header,
            // UNCHECKED: the validator has checked all inputs/outputs are sorted and preserves order in it's output
            AggregateBody::new_sorted_unchecked(inputs_result.inputs, outputs_result.outputs, kernels_result.kernels),
        );

        validate_covenants(&block)?;

        Ok(())
    }

    fn start_kernel_validation(
        &self,
        header: &BlockHeader,
        kernels: Vec<TransactionKernel>,
    ) -> AbortOnDropJoinHandle<Result<KernelValidationData, ValidationError>> {
        let height = header.height;

        let total_kernel_offset = header.total_kernel_offset.clone();
        let total_reward = self.consensus_rules.calculate_coinbase_and_fees(height, &kernels);
        let total_offset = self
            .factories
            .commitment
            .commit_value(&total_kernel_offset, total_reward.as_u64());
        let db = self.db.inner().clone();
        let constants = self.consensus_rules.consensus_constants(height).clone();
        task::spawn_blocking(move || {
            let db = db.db_read_access()?;
            let timer = Instant::now();
            let mut kernel_sum = KernelSum {
                sum: total_offset,
                ..Default::default()
            };

            let mut coinbase_index = None;
            let mut max_kernel_timelock = 0;
            for (i, kernel) in kernels.iter().enumerate() {
                if i > 0 && kernel <= &kernels[i - 1] {
                    return Err(ValidationError::UnsortedOrDuplicateKernel);
                }

                validate_kernel_version(&constants, kernel)?;
                kernel.verify_signature()?;

                if kernel.is_coinbase() {
                    if coinbase_index.is_some() {
                        warn!(
                            target: LOG_TARGET,
                            "Block #{} failed to validate: more than one kernel coinbase", height
                        );
                        return Err(ValidationError::TransactionError(TransactionError::MoreThanOneCoinbase));
                    }
                    coinbase_index = Some(i);
                }

                if let Some((db_kernel, header_hash)) = db.fetch_kernel_by_excess_sig(&kernel.excess_sig)? {
                    let msg = format!(
                        "Block contains kernel excess: {} which matches already existing excess signature in chain \
                         database block hash: {}. Existing kernel excess: {}, excess sig nonce: {}, excess signature: \
                         {}",
                        kernel.excess.to_hex(),
                        header_hash.to_hex(),
                        db_kernel.excess.to_hex(),
                        db_kernel.excess_sig.get_public_nonce().to_hex(),
                        db_kernel.excess_sig.get_signature().to_hex(),
                    );
                    warn!(target: LOG_TARGET, "{}", msg);
                    return Err(ValidationError::ConsensusError(msg));
                };

                max_kernel_timelock = cmp::max(max_kernel_timelock, kernel.lock_height);
                kernel_sum.fees += kernel.fee;
                kernel_sum.sum = &kernel_sum.sum + &kernel.excess;
            }

            if max_kernel_timelock > height {
                return Err(ValidationError::MaturityError);
            }

            if coinbase_index.is_none() {
                warn!(
                    target: LOG_TARGET,
                    "Block #{} failed to validate: no coinbase kernel", height
                );
                return Err(ValidationError::TransactionError(TransactionError::NoCoinbase));
            }

            let coinbase_index = coinbase_index.unwrap();

            debug!(
                target: LOG_TARGET,
                "Validated {} kernel(s) in {:.2?}",
                kernels.len(),
                timer.elapsed()
            );
            Ok(KernelValidationData {
                kernels,
                kernel_sum,
                coinbase_index,
            })
        })
        .into()
    }

    fn start_input_validation(
        &self,
        header: &BlockHeader,
        output_hashes: Vec<HashOutput>,
        mut inputs: Vec<TransactionInput>,
    ) -> AbortOnDropJoinHandle<Result<InputValidationData, ValidationError>> {
        let block_height = header.height;
        let commitment_factory = self.factories.commitment.clone();
        let db = self.db.inner().clone();
        let prev_hash: [u8; 32] = header.prev_hash.as_slice().try_into().unwrap_or([0; 32]);
        let height = header.height;
        let constants = self.consensus_rules.consensus_constants(height).clone();
        task::spawn_blocking(move || {
            let timer = Instant::now();
            let mut aggregate_input_key = PublicKey::default();
            let mut commitment_sum = Commitment::default();
            let mut not_found_inputs = Vec::new();
            let db = db.db_read_access()?;

            // Check for duplicates and/or incorrect sorting
            for (i, input) in inputs.iter().enumerate() {
                if i > 0 && input <= &inputs[i - 1] {
                    return Err(ValidationError::UnsortedOrDuplicateInput);
                }
            }

            for input in &mut inputs {
                // Read the spent_output for this compact input
                if input.is_compact() {
                    let output_mined_info = db
                        .fetch_output(&input.output_hash())?
                        .ok_or(ValidationError::TransactionInputSpentOutputMissing)?;

                    match output_mined_info.output {
                        PrunedOutput::Pruned { .. } => {
                            return Err(ValidationError::TransactionInputSpendsPrunedOutput);
                        },
                        PrunedOutput::NotPruned { output } => {
                            input.add_output_data(
                                output.version,
                                output.features,
                                output.commitment,
                                output.script,
                                output.sender_offset_public_key,
                                output.covenant,
                                output.encrypted_value,
                                output.minimum_value_promise,
                            );
                        },
                    }
                }

                if !input.is_mature_at(block_height)? {
                    warn!(
                        target: LOG_TARGET,
                        "Input found that has not yet matured to spending height: {}", block_height
                    );
                    return Err(TransactionError::InputMaturity.into());
                }

                validate_input_version(&constants, input)?;

                match check_input_is_utxo(&*db, input) {
                    Err(ValidationError::UnknownInput) => {
                        // Check if the input spends from the current block
                        let output_hash = input.output_hash();
                        if output_hashes.iter().all(|hash| hash != &output_hash) {
                            warn!(
                                target: LOG_TARGET,
                                "Validation failed due to input: {} which does not exist yet", input
                            );
                            not_found_inputs.push(output_hash);
                        }
                    },
                    Err(err) => return Err(err),
                    _ => {},
                }

                // Once we've found unknown inputs, the aggregate data will be discarded and there is no reason to run
                // the tari script
                let commitment = match input.commitment() {
                    Ok(c) => c,
                    Err(e) => return Err(ValidationError::from(e)),
                };
                if not_found_inputs.is_empty() {
                    let context = ScriptContext::new(height, &prev_hash, commitment);
                    // lets count up the input script public keys
                    aggregate_input_key =
                        aggregate_input_key + input.run_and_verify_script(&commitment_factory, Some(context))?;
                    commitment_sum = &commitment_sum + input.commitment()?;
                }
            }

            if !not_found_inputs.is_empty() {
                return Err(ValidationError::UnknownInputs(not_found_inputs));
            }

            debug!(
                target: LOG_TARGET,
                "Validated {} inputs(s) in {:.2?}",
                inputs.len(),
                timer.elapsed()
            );
            Ok(InputValidationData {
                inputs,
                aggregate_input_key,
                commitment_sum,
            })
        })
        .into()
    }

    #[allow(clippy::too_many_lines)]
    fn start_output_validation(
        &self,
        header: &BlockHeader,
        outputs: Vec<TransactionOutput>,
    ) -> AbortOnDropJoinHandle<Result<OutputValidationData, ValidationError>> {
        let height = header.height;
        let num_outputs = outputs.len();
        let concurrency = cmp::min(self.concurrency, num_outputs);
        let output_chunks = into_enumerated_batches(outputs, concurrency);
        let bypass_range_proof_verification = self.bypass_range_proof_verification;
        if bypass_range_proof_verification {
            warn!(target: LOG_TARGET, "Range proof verification will be bypassed!")
        }

        debug!(
            target: LOG_TARGET,
            "Using {} worker(s) to validate #{} ({} output(s))",
            output_chunks.len(),
            height,
            num_outputs
        );
        let mut output_tasks = output_chunks
            .into_iter()
            .map(|outputs| {
                let range_proof_prover = self.factories.range_proof.clone();
                let db = self.db.inner().clone();
                let constants = self.consensus_rules.consensus_constants(height).clone();
                task::spawn_blocking(move || {
                    let db = db.db_read_access()?;
                    let mut aggregate_sender_offset = PublicKey::default();
                    let mut commitment_sum = Commitment::default();
                    let max_script_size = constants.get_max_script_byte_size();
                    let mut coinbase_index = None;
                    debug!(
                        target: LOG_TARGET,
                        "{} output(s) queued for validation in {:?}",
                        outputs.len(),
                        thread::current().id()
                    );
                    for (orig_idx, output) in &outputs {
                        if output.is_coinbase() {
                            if coinbase_index.is_some() {
                                warn!(
                                    target: LOG_TARGET,
                                    "Block #{} failed to validate: more than one coinbase output", height
                                );
                                return Err(ValidationError::TransactionError(TransactionError::MoreThanOneCoinbase));
                            }
                            coinbase_index = Some(*orig_idx);
                        } else {
                            // Lets gather the output public keys and hashes.
                            // We should not count the coinbase tx here
                            aggregate_sender_offset = aggregate_sender_offset + &output.sender_offset_public_key;
                        }

                        validate_output_version(&constants, output)?;
                        check_permitted_output_types(&constants, output)?;
                        check_tari_script_byte_size(&output.script, max_script_size)?;
                        check_output_feature(output, constants.coinbase_output_features_extra_max_length())?;
                        output.verify_metadata_signature()?;
                        output.verify_validator_node_signature()?;
                        check_not_duplicate_txo(&*db, output)?;
                        check_validator_node_registration_utxo(&constants, output)?;
                        commitment_sum = &commitment_sum + &output.commitment;
                    }
                    if !bypass_range_proof_verification {
                        let this_outputs = outputs.iter().map(|o| &o.1).collect::<Vec<_>>();
                        batch_verify_range_proofs(&range_proof_prover, &this_outputs)?;
                    }

                    Ok((outputs, aggregate_sender_offset, commitment_sum, coinbase_index))
                })
            })
            .collect::<FuturesUnordered<_>>();

        task::spawn(async move {
            let mut valid_outputs = Vec::with_capacity(num_outputs);
            let mut aggregate_offset_pubkey = PublicKey::default();
            let mut output_commitment_sum = Commitment::default();
            let mut coinbase_index = None;
            let timer = Instant::now();
            while let Some(output_validation_result) = output_tasks.next().await {
                let (outputs, agg_sender_offset, commitment_sum, cb_index) = output_validation_result??;
                aggregate_offset_pubkey = aggregate_offset_pubkey + agg_sender_offset;
                output_commitment_sum = &output_commitment_sum + &commitment_sum;
                if cb_index.is_some() {
                    if coinbase_index.is_some() {
                        return Err(ValidationError::TransactionError(TransactionError::MoreThanOneCoinbase));
                    }
                    coinbase_index = cb_index;
                }
                valid_outputs.extend(outputs);
            }
            debug!(
                target: LOG_TARGET,
                "Validated {} outputs(s) in {:.2?}",
                valid_outputs.len(),
                timer.elapsed()
            );

            if coinbase_index.is_none() {
                warn!(
                    target: LOG_TARGET,
                    "Block #{} failed to validate: no coinbase UTXO", height
                );
                return Err(ValidationError::TransactionError(TransactionError::NoCoinbase));
            }
            let coinbase_index = coinbase_index.unwrap();

            // Return result in original order
            valid_outputs.sort_by(|(a, _), (b, _)| a.cmp(b));
            let outputs = valid_outputs.into_iter().map(|(_, output)| output).collect();

            Ok(OutputValidationData {
                outputs,
                commitment_sum: output_commitment_sum,
                aggregate_offset_pubkey,
                coinbase_index,
            })
        })
        .into()
    }
}

struct KernelValidationData {
    pub kernels: Vec<TransactionKernel>,
    pub kernel_sum: KernelSum,
    pub coinbase_index: usize,
}

impl KernelValidationData {
    pub fn coinbase(&self) -> &TransactionKernel {
        &self.kernels[self.coinbase_index]
    }
}

struct OutputValidationData {
    pub outputs: Vec<TransactionOutput>,
    pub commitment_sum: Commitment,
    pub aggregate_offset_pubkey: PublicKey,
    pub coinbase_index: usize,
}

impl OutputValidationData {
    pub fn coinbase(&self) -> &TransactionOutput {
        &self.outputs[self.coinbase_index]
    }
}

struct InputValidationData {
    pub inputs: Vec<TransactionInput>,
    pub aggregate_input_key: PublicKey,
    pub commitment_sum: Commitment,
}

fn into_enumerated_batches<T>(mut items: Vec<T>, num_batches: usize) -> Vec<Vec<(usize, T)>> {
    if num_batches <= 1 {
        return vec![items.into_iter().enumerate().collect()];
    }

    let num_items = items.len();
    let mut batch_size = num_items / num_batches;
    if num_items % batch_size != 0 {
        batch_size += 1;
    }
    let mut idx = 0;
    NonOverlappingIntegerPairIter::new(0, num_items, batch_size)
        .map(|(start, end)| {
            let chunk_size = end - start;
            items
                .drain(..=chunk_size)
                .map(|output| {
                    let v = (idx, output);
                    idx += 1;
                    v
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn is_all_unique_and_sorted<'a, I: IntoIterator<Item = &'a T>, T: PartialOrd + 'a>(items: I) -> bool {
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

fn check_coinbase_maturity(
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

fn check_coinbase_reward(
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

fn check_script_offset(
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

fn check_kernel_sum(
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

fn validate_covenants(block: &Block) -> Result<(), ValidationError> {
    for input in block.body.inputs() {
        let output_set_size = input
            .covenant()?
            .execute(block.header.height, input, block.body.outputs())?;
        trace!(target: LOG_TARGET, "{} output(s) passed covenant", output_set_size);
    }
    Ok(())
}

fn validate_input_version(
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

fn validate_kernel_version(
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

fn validate_output_version(
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

fn check_permitted_output_types(
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

/// Checks the byte size of TariScript is less than or equal to the given size, otherwise returns an error.
pub fn check_tari_script_byte_size(script: &TariScript, max_script_size: usize) -> Result<(), ValidationError> {
    let script_size = script.get_serialized_size();
    if script_size > max_script_size {
        return Err(ValidationError::TariScriptExceedsMaxSize {
            max_script_size,
            actual_script_size: script_size,
        });
    }
    Ok(())
}

fn check_output_feature(output: &TransactionOutput, max_coinbase_extra_size: u32) -> Result<(), TransactionError> {
    // This field is optional for coinbases (mining pools and
    // other merge mined coins can use it), but must be empty for non-coinbases
    if !output.is_coinbase() && !output.features.coinbase_extra.is_empty() {
        return Err(TransactionError::NonCoinbaseHasOutputFeaturesCoinbaseExtra);
    }

    // For coinbases, the maximum length should be 64 bytes (2x hashes),
    // so that arbitrary data cannot be included
    if output.is_coinbase() && output.features.coinbase_extra.len() > max_coinbase_extra_size as usize {
        return Err(TransactionError::InvalidOutputFeaturesCoinbaseExtraSize {
            len: output.features.coinbase_extra.len(),
            max: max_coinbase_extra_size,
        });
    }
    Ok(())
}

/// This function checks that the outputs do not already exist in the TxO set.
fn check_not_duplicate_txo<B: BlockchainBackend>(db: &B, output: &TransactionOutput) -> Result<(), ValidationError> {
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

fn check_validator_node_registration_utxo(
    consensus_constants: &ConsensusConstants,
    utxo: &TransactionOutput,
) -> Result<(), ValidationError> {
    if let Some(reg) = utxo.features.validator_node_registration() {
        if utxo.minimum_value_promise < consensus_constants.validator_node_registration_min_deposit_amount() {
            return Err(ValidationError::ValidatorNodeRegistrationMinDepositAmount {
                min: consensus_constants.validator_node_registration_min_deposit_amount(),
                actual: utxo.minimum_value_promise,
            });
        }
        if utxo.features.maturity < consensus_constants.validator_node_registration_min_lock_height() {
            return Err(ValidationError::ValidatorNodeRegistrationMinLockHeight {
                min: consensus_constants.validator_node_registration_min_lock_height(),
                actual: utxo.features.maturity,
            });
        }

        // TODO(SECURITY): Signing this with a blank msg allows the signature to be replayed. Using the commitment
        //                 is ideal as uniqueness is enforced. However, because the VN and wallet have different
        //                 keys this becomes difficult. Fix this once we have decided on a solution.
        if !reg.is_valid_signature_for(&[]) {
            return Err(ValidationError::InvalidValidatorNodeSignature);
        }
    }
    Ok(())
}
