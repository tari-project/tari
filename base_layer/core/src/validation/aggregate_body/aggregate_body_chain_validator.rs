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
use tari_script::TariScript;
use tari_utilities::hex::Hex;

use crate::{
    borsh::SerializedSize,
    chain_storage::{BlockchainBackend, MmrTree},
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{
        aggregated_body::AggregateBody,
        transaction_components::{TransactionInput, TransactionKernel, TransactionOutput},
    },
    validation::ValidationError,
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
    ) -> Result<(), ValidationError> {
        let constants = self.consensus_manager.consensus_constants(height);

        self.validate_consensus(body, db, constants)?;
        self.validate_input_and_maturity(body, db, constants, height)?;

        Ok(())
    }

    fn validate_consensus<B: BlockchainBackend>(
        &self,
        body: &AggregateBody,
        db: &B,
        constants: &ConsensusConstants,
    ) -> Result<(), ValidationError> {
        validate_excess_sig_not_in_db(body, db)?;

        validate_versions(body, constants)?;
        for output in body.outputs() {
            check_permitted_output_types(constants, output)?;
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
    ) -> Result<(), ValidationError> {
        check_inputs_are_utxos(db, body)?;
        check_outputs(db, constants, body)?;
        verify_no_duplicated_inputs_outputs(body)?;
        check_total_burned(body)?;
        verify_timelocks(body, height)?;

        Ok(())
    }
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

fn validate_versions(body: &AggregateBody, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
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
        check_validator_node_registration_utxo(constants, output)?;
    }
    Ok(())
}

/// This function checks the at the body contains no duplicated inputs or outputs.
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

/// THis function checks the total burned sum in the header ensuring that every burned output is counted in the total
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

/// Checks the byte size of TariScript is less than or equal to the given size, otherwise returns an error.
fn check_tari_script_byte_size(script: &TariScript, max_script_size: usize) -> Result<(), ValidationError> {
    let script_size = script.get_serialized_size();
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

// This function checks that all the timelocks in the provided transaction pass. It checks kernel lock heights and
// input maturities
fn verify_timelocks(body: &AggregateBody, current_height: u64) -> Result<(), ValidationError> {
    if body.min_spendable_height() > current_height + 1 {
        warn!(
            target: LOG_TARGET,
            "AggregateBody has a min spend height higher than the current tip"
        );
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}
