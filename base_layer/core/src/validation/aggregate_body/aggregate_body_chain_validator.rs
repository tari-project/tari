//  Copyright 2022, The Tari Project
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

use std::collections::HashSet;

use log::*;
use tari_common_types::epoch::VnEpoch;
use tari_utilities::hex::Hex;

use crate::{
    blocks::BlockHeader,
    chain_storage::BlockchainBackend,
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{
            OutputType,
            SideChainId,
            SpentOutput,
            TransactionError,
            TransactionInput,
            ValidatorNodeRegistration,
        },
    },
    validation::{
        helpers::{
            check_eviction_proof,
            check_input_is_utxo,
            check_not_duplicate_txo,
            check_tari_encrypted_data_byte_size,
            check_tari_script_byte_size,
            check_validator_node_exit,
            check_validator_node_registration,
        },
        ValidationError,
    },
};

pub const LOG_TARGET: &str = "c::val::aggregate_body_chain_linked_validator";

/// This validator assumes that the body was already validated for internal consistency and it will skip that step.
#[derive(Clone)]
pub struct AggregateBodyChainLinkedValidator {
    consensus_manager: ConsensusManager,
}

impl AggregateBodyChainLinkedValidator {
    pub fn new(consensus_manager: ConsensusManager) -> Self {
        Self { consensus_manager }
    }

    pub fn validate<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        header: &BlockHeader,
        db: &B,
    ) -> Result<AggregateBody, ValidationError> {
        let constants = self.consensus_manager.consensus_constants(header.height);

        self.validate_consensus(body, db)?;
        let body = self.validate_body(body, db, constants, header)?;

        Ok(body)
    }

    fn validate_consensus<B: BlockchainBackend>(&self, body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
        validate_excess_sig_not_in_db(body, db)?;
        Ok(())
    }

    fn validate_body<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        db: &B,
        constants: &ConsensusConstants,
        header: &BlockHeader,
    ) -> Result<AggregateBody, ValidationError> {
        // inputs may be "slim", only containing references to outputs
        // so we need to resolve those references, creating a new body in the process
        let inputs = validate_input_not_pruned(body, db)?;
        // UNCHECKED: sorting has been checked by the AggregateBodyInternalConsistencyValidator
        let body = AggregateBody::new_sorted_unchecked(inputs, body.outputs().to_vec(), body.kernels().to_vec());

        validate_input_maturity(&body, header.height)?;
        check_inputs_are_spendable(db, constants, header.height, &body)?;
        check_outputs(db, constants, &body, header.height)?;
        verify_no_duplicated_inputs_outputs(&body)?;
        check_total_burned(&body)?;
        verify_timelocks(&body, header.height)?;

        Ok(body)
    }
}

fn validate_input_not_pruned<B: BlockchainBackend>(
    body: &AggregateBody,
    db: &B,
) -> Result<Vec<TransactionInput>, ValidationError> {
    let mut inputs: Vec<TransactionInput> = body.inputs().clone();
    for input in &mut inputs {
        if input.is_compact() {
            let input_output_hash = input.output_hash();
            // TODO: we clone the block body 3 times in validation and the inputs 1 more time here. We also discard
            //      the hydrated block in all cases expect block sync. This is unnecessarily slow and wasteful.
            //      SIMPLE REFACTOR: populate/hydrate the block inputs (which is owned, no cloning necessary) before
            //      performing validation. If hydration fails with UnknownInput, the block is invalid.
            let output = match db.fetch_output(&input_output_hash) {
                Ok(val) => match val {
                    Some(output_mined_info) => output_mined_info.output,
                    None => {
                        // Input is found in this block
                        if let Some(found) = body.outputs().iter().find(|o| o.hash() == input_output_hash) {
                            found.clone()
                        } else {
                            debug!(
                                target: LOG_TARGET,
                                "Input not found in database or block, commitment: {}, hash: {}",
                                input.commitment()?.to_hex(), input_output_hash,
                            );
                            return Err(ValidationError::UnknownInput);
                        }
                    },
                },
                Err(e) => return Err(ValidationError::from(e)),
            };

            input.add_output_data(output);
        }
    }

    Ok(inputs)
}

fn validate_input_maturity(body: &AggregateBody, height: u64) -> Result<(), ValidationError> {
    for input in body.inputs() {
        if !input.is_mature_at(height)? {
            return Err(TransactionError::InputMaturity.into());
        }
    }

    Ok(())
}

fn validate_excess_sig_not_in_db<B: BlockchainBackend>(body: &AggregateBody, db: &B) -> Result<(), ValidationError> {
    for kernel in body.kernels() {
        if let Some((db_kernel, header_hash)) = db.fetch_kernel_by_excess_sig(&kernel.excess_sig)? {
            let msg = format!(
                "Aggregate body contains kernel excess: {} which matches already existing excess signature in chain \
                 database block hash: {}. Existing kernel excess: {}, excess sig nonce: {}, excess signature: {}",
                kernel.excess.to_hex(),
                header_hash.to_hex(),
                db_kernel.excess.to_hex(),
                db_kernel.excess_sig.get_compressed_public_nonce().to_hex(),
                db_kernel.excess_sig.get_signature().to_hex(),
            );
            return Err(ValidationError::DuplicateKernelError(msg));
        };
    }
    Ok(())
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spent
fn check_inputs_are_spendable<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    current_height: u64,
    body: &AggregateBody,
) -> Result<(), ValidationError> {
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
                let input_output_hash = input.output_hash();
                if output_hashes.iter().any(|val| val == &input_output_hash) {
                    continue;
                }
                debug!(
                    target: LOG_TARGET,
                    "Input not found in database, commitment: {}, hash: {}",
                    input.commitment()?.to_hex(), input_output_hash.to_hex()
                );
                not_found_inputs.push(input_output_hash);
            },
            Err(err) => {
                return Err(err);
            },
        }

        check_output_feature_rules_for_input(db, constants, current_height, input)?;
    }

    if !not_found_inputs.is_empty() {
        return Err(ValidationError::UnknownInputs(not_found_inputs));
    }

    Ok(())
}

/// This function checks:
/// 1. that the output type is permitted
/// 2. the byte size of TariScript does not exceed the maximum
/// 3. that the outputs do not already exist in the UTxO set.
pub fn check_outputs<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    body: &AggregateBody,
    height: u64,
) -> Result<(), ValidationError> {
    let max_script_size = constants.max_script_byte_size();
    let max_encrypted_data_size = constants.max_extra_encrypted_data_byte_size();
    for output in body.outputs() {
        let epoch = constants.block_height_to_epoch(height);
        check_tari_script_byte_size(&output.script, max_script_size)?;
        check_tari_encrypted_data_byte_size(&output.encrypted_data, max_encrypted_data_size)?;
        check_not_duplicate_txo(db, output)?;
        check_validator_node_registration(db, output, epoch)?;
        check_validator_node_exit(db, output, epoch)?;
        check_eviction_proof(db, output, constants)?;
    }
    Ok(())
}

/// This function checks the body contains no duplicated inputs or outputs.
fn verify_no_duplicated_inputs_outputs(body: &AggregateBody) -> Result<(), ValidationError> {
    if body.contains_duplicated_inputs() {
        warn!(
            target: LOG_TARGET,
            "AggregateBody validation failed due to double input"
        );
        return Err(ValidationError::UnsortedOrDuplicateInput);
    }
    if body.contains_duplicated_outputs() {
        warn!(
            target: LOG_TARGET,
            "AggregateBody validation failed due to double output"
        );
        return Err(ValidationError::UnsortedOrDuplicateOutput);
    }
    Ok(())
}

/// This function checks the total burned sum in the header ensuring that every burned output is counted in the total
/// sum.
#[allow(clippy::mutable_key_type)]
fn check_total_burned(body: &AggregateBody) -> Result<(), ValidationError> {
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

// This function checks that all the timelocks in the provided transaction pass. It checks kernel lock heights and
// input maturities
fn verify_timelocks(body: &AggregateBody, current_height: u64) -> Result<(), ValidationError> {
    if body.min_spendable_height()? > current_height.saturating_add(1) {
        warn!(
            target: LOG_TARGET,
            "AggregateBody has a min spend height higher than the current tip"
        );
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}

/// If applicable, check any spend rules for output features including sidechain features
fn check_output_feature_rules_for_input<B: BlockchainBackend>(
    db: &B,
    constants: &ConsensusConstants,
    current_height: u64,
    input: &TransactionInput,
) -> Result<(), ValidationError> {
    match &input.spent_output {
        SpentOutput::OutputHash(_) => unreachable!("check_output_feature_rules_for_input: SpentOutput not hydrated"),
        SpentOutput::OutputData { features, .. } => {
            match features.output_type {
                OutputType::Standard | OutputType::Coinbase => {
                    // no special spend rules
                },

                OutputType::ValidatorNodeRegistration => {
                    // Prevents validator node registration output from being spent if the validator is still active.
                    // Effectively locking the funds in the UTXO until the validator exits/is evicted.
                    let reg = features.validator_node_registration().ok_or_else(|| {
                        ValidationError::OutputTypeNotMatchSidechainData {
                            output_type: features.output_type,
                            details: "Expected OutputType::ValidatorNodeRegistration to have validator node \
                                      registration sidechain data"
                                .to_string(),
                        }
                    })?;
                    let epoch = constants.block_height_to_epoch(current_height);
                    check_validator_node_registration_spend(db, reg, features.sidechain_id(), epoch)?
                },
                OutputType::ValidatorNodeExit => {
                    // should we disallow this? Since this UTXO has been processed w.r.t the active validator set, there
                    // is no reason to keep it in the UTXO set
                },
                OutputType::Burn => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "Burn outputs cannot be spent".to_string(),
                    });
                },
                OutputType::CodeTemplateRegistration => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "CodeTemplateRegistration cannot be spent".to_string(),
                    });
                },
                OutputType::SidechainCheckpoint => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "SidechainCheckpoint cannot be spent".to_string(),
                    });
                },
                OutputType::SidechainProof => {
                    return Err(ValidationError::OutputSpendRuleDisallow {
                        output_type: features.output_type,
                        details: "SidechainProof cannot be spent".to_string(),
                    });
                },
            }
        },
    }
    Ok(())
}

fn check_validator_node_registration_spend<B: BlockchainBackend>(
    db: &B,
    reg: &ValidatorNodeRegistration,
    sidechain_id: Option<&SideChainId>,
    epoch: VnEpoch,
) -> Result<(), ValidationError> {
    if db.validator_node_is_active(sidechain_id.map(|id| id.public_key()), epoch, reg.public_key())? {
        return Err(ValidationError::OutputSpendRuleDisallow {
            output_type: OutputType::ValidatorNodeRegistration,
            details: format!(
                "Validator node registration {} is active and cannot be spent",
                reg.public_key()
            ),
        });
    }
    Ok(())
}
