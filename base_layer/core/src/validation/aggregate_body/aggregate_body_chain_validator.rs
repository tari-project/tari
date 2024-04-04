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

use log::warn;
use tari_common_types::types::FixedHash;
use tari_utilities::hex::Hex;

use crate::{
    chain_storage::BlockchainBackend,
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{TransactionError, TransactionInput, TransactionOutput},
    },
    validation::{
        helpers::{check_input_is_utxo, check_not_duplicate_txo, check_tari_script_byte_size},
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
        height: u64,
        db: &B,
    ) -> Result<AggregateBody, ValidationError> {
        let constants = self.consensus_manager.consensus_constants(height);

        self.validate_consensus(body, db, constants)?;
        let body = self.validate_input_and_maturity(body, db, constants, height)?;

        Ok(body)
    }

    fn validate_consensus<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        db: &B,
        constants: &ConsensusConstants,
    ) -> Result<(), ValidationError> {
        validate_excess_sig_not_in_db(body, db)?;

        for output in body.outputs() {
            check_validator_node_registration_utxo(constants, output)?;
        }

        Ok(())
    }

    fn validate_input_and_maturity<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        db: &B,
        constants: &ConsensusConstants,
        height: u64,
    ) -> Result<AggregateBody, ValidationError> {
        // inputs may be "slim", only containing references to outputs
        // so we need to resolve those references, creating a new body in the process
        let inputs = validate_input_not_pruned(body, db)?;
        // UNCHECKED: sorting has been checked by the AggregateBodyInternalConsistencyValidator
        let body = AggregateBody::new_sorted_unchecked(inputs, body.outputs().to_vec(), body.kernels().to_vec());

        validate_input_maturity(&body, height)?;
        check_inputs_are_utxos(db, &body)?;
        check_outputs(db, constants, &body)?;
        verify_no_duplicated_inputs_outputs(&body)?;
        check_total_burned(&body)?;
        verify_timelocks(&body, height)?;

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
            let output = match db.fetch_output(&input.output_hash()) {
                Ok(val) => match val {
                    Some(output_mined_info) => output_mined_info.output,
                    None => {
                        let input_output_hash = input.output_hash();
                        if let Some(found) = body.outputs().iter().find(|o| o.hash() == input_output_hash) {
                            found.clone()
                        } else {
                            warn!(
                                target: LOG_TARGET,
                                "Input not found in database or block, commitment: {}, hash: {}",
                                input.commitment()?.to_hex(), input_output_hash.to_hex()
                            );
                            return Err(ValidationError::UnknownInput);
                        }
                    },
                },
                Err(e) => return Err(ValidationError::from(e)),
            };

            let rp_hash = match output.proof {
                Some(proof) => proof.hash(),
                None => FixedHash::zero(),
            };
            input.add_output_data(
                output.version,
                output.features,
                output.commitment,
                output.script,
                output.sender_offset_public_key,
                output.covenant,
                output.encrypted_data,
                output.metadata_signature,
                rp_hash,
                output.minimum_value_promise,
            );
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
                db_kernel.excess_sig.get_public_nonce().to_hex(),
                db_kernel.excess_sig.get_signature().to_hex(),
            );
            return Err(ValidationError::DuplicateKernelError(msg));
        };
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

        if !reg.is_valid_signature_for(&[]) {
            return Err(ValidationError::InvalidValidatorNodeSignature);
        }
    }
    Ok(())
}

/// This function checks that all inputs in the blocks are valid UTXO's to be spent
fn check_inputs_are_utxos<B: BlockchainBackend>(db: &B, body: &AggregateBody) -> Result<(), ValidationError> {
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
                warn!(
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
) -> Result<(), ValidationError> {
    let max_script_size = constants.max_script_byte_size();
    for output in body.outputs() {
        check_tari_script_byte_size(&output.script, max_script_size)?;
        check_not_duplicate_txo(db, output)?;
        check_validator_node_registration_utxo(constants, output)?;
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
