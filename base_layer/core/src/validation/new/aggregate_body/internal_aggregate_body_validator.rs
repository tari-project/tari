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

use std::collections::HashSet;

use log::{trace, warn};
use tari_common_types::types::{BlindingFactor, Commitment, CommitmentFactory, PrivateKey, RangeProofService};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, ristretto::pedersen::PedersenCommitment};
use tari_utilities::hex::Hex;

use crate::{
    consensus::ConsensusConstants,
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

pub const LOG_TARGET: &str = "c::val::internal_consistency_aggregate_body_validator";

pub struct InternalConsistencyAggregateBodyValidator {
    consensus_constants: ConsensusConstants,
    factories: CryptoFactories,
}

impl InternalConsistencyAggregateBodyValidator {
    pub fn new(consensus_constants: ConsensusConstants) -> Self {
        Self {
            consensus_constants,
            factories: CryptoFactories::default(),
        }
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    /// The reward is the total amount of Tari rewarded for this block (block reward + total fees), this should be 0
    /// for a transaction
    #[allow(dead_code)]
    pub fn validate(
        &self,
        body: &AggregateBody,
        tx_offset: &BlindingFactor,
        total_reward: MicroTari,
    ) -> Result<(), ValidationError> {
        // TODO: include bypass_range_proof_verification parameter?

        validate_is_not_coinbase(body)?;
        validate_maximum_weight(body, &self.consensus_constants)?;
        validate_versions(body, &self.consensus_constants)?;
        validate_output_features(body, &self.consensus_constants)?;
        verify_kernel_signatures(body)?;
        validate_kernel_sum(body, &self.factories.commitment, total_reward, tx_offset)?;
        validate_range_proofs(body, &self.factories.range_proof)?;
        verify_metadata_signatures(body)?;
        verify_no_duplicated_inputs_outputs(body)?;
        check_total_burned(body)?;

        Ok(())
    }
}

fn validate_is_not_coinbase(body: &AggregateBody) -> Result<(), ValidationError> {
    if body.outputs().iter().any(|o| o.features.is_coinbase()) {
        return Err(ValidationError::ErroneousCoinbaseOutput);
    }

    Ok(())
}

fn validate_maximum_weight(
    body: &AggregateBody,
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    if body.calculate_weight(consensus_constants.transaction_weight()) >
        consensus_constants.get_max_block_weight_excluding_coinbase()
    {
        return Err(ValidationError::MaxTransactionWeightExceeded);
    }

    Ok(())
}

fn validate_output_features(
    body: &AggregateBody,
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    // We can call this function with a constant value, because we've just shown that this is NOT a coinbase, and
    // only coinbases may have the extra field set (the only field that the fn argument affects).
    body.check_output_features(1)?;

    for output in body.outputs() {
        check_permitted_output_types(output, consensus_constants)?;
        check_validator_node_registration_utxo(output, consensus_constants)?;
    }

    Ok(())
}

/// Verify the signatures in all kernels contained in this aggregate body. Clients must provide an offset that
/// will be added to the public key used in the signature verification.
fn verify_kernel_signatures(body: &AggregateBody) -> Result<(), TransactionError> {
    trace!(target: LOG_TARGET, "Checking kernel signatures",);
    for kernel in body.kernels() {
        kernel.verify_signature().map_err(|e| {
            warn!(target: LOG_TARGET, "Kernel ({}) signature failed {:?}.", kernel, e);
            e
        })?;
    }
    Ok(())
}

/// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
fn validate_kernel_sum(
    body: &AggregateBody,
    factory: &CommitmentFactory,
    total_reward: MicroTari,
    tx_offset: &BlindingFactor,
) -> Result<(), TransactionError> {
    let total_offset = factory.commit_value(tx_offset, total_reward.0);

    trace!(target: LOG_TARGET, "Checking kernel total");
    let KernelSum { sum: excess, fees } = sum_kernels(body, total_offset);
    let sum_io = sum_commitments(body)?;
    trace!(target: LOG_TARGET, "Total outputs - inputs:{}", sum_io.to_hex());
    let fees = factory.commit_value(&PrivateKey::default(), fees.into());
    trace!(
        target: LOG_TARGET,
        "Comparing sum.  excess:{} == sum {} + fees {}",
        excess.to_hex(),
        sum_io.to_hex(),
        fees.to_hex()
    );
    if excess != &sum_io + &fees {
        return Err(TransactionError::ValidationError(
            "Sum of inputs and outputs did not equal sum of kernels with fees".into(),
        ));
    }

    Ok(())
}

/// Calculate the sum of the kernels, taking into account the provided offset, and their constituent fees
fn sum_kernels(body: &AggregateBody, offset_with_fee: PedersenCommitment) -> KernelSum {
    // Sum all kernel excesses and fees
    body.kernels().iter().fold(
        KernelSum {
            fees: MicroTari(0),
            sum: offset_with_fee,
        },
        |acc, val| KernelSum {
            fees: acc.fees + val.fee,
            sum: &acc.sum + &val.excess,
        },
    )
}

/// Calculate the sum of the outputs - inputs
fn sum_commitments(body: &AggregateBody) -> Result<Commitment, TransactionError> {
    let sum_inputs = body
        .inputs()
        .iter()
        .map(|i| i.commitment())
        .collect::<Result<Vec<&Commitment>, _>>()?
        .into_iter()
        .sum::<Commitment>();
    let sum_outputs = body.outputs().iter().map(|o| &o.commitment).sum::<Commitment>();
    Ok(&sum_outputs - &sum_inputs)
}

fn validate_range_proofs(
    body: &AggregateBody,
    range_proof_service: &RangeProofService,
) -> Result<(), TransactionError> {
    trace!(target: LOG_TARGET, "Checking range proofs");
    let outputs = body.outputs().iter().collect::<Vec<_>>();
    batch_verify_range_proofs(range_proof_service, &outputs)?;
    Ok(())
}

fn verify_metadata_signatures(body: &AggregateBody) -> Result<(), TransactionError> {
    trace!(target: LOG_TARGET, "Checking sender signatures");
    for o in body.outputs() {
        o.verify_metadata_signature()?;
    }
    Ok(())
}

fn check_permitted_output_types(
    output: &TransactionOutput,
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    if !consensus_constants
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
    utxo: &TransactionOutput,
    consensus_constants: &ConsensusConstants,
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

/// This function checks the at the tx contains no duplicated inputs or outputs.
fn verify_no_duplicated_inputs_outputs(body: &AggregateBody) -> Result<(), ValidationError> {
    if body.contains_duplicated_inputs() {
        warn!(target: LOG_TARGET, "Transaction validation failed due to double input");
        return Err(ValidationError::UnsortedOrDuplicateInput);
    }
    if body.contains_duplicated_outputs() {
        warn!(target: LOG_TARGET, "Transaction validation failed due to double output");
        return Err(ValidationError::UnsortedOrDuplicateOutput);
    }
    Ok(())
}

/// THis function checks the total burned sum in the header ensuring that every burned output is counted in the
/// total sum.
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
