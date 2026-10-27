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

use std::{collections::HashSet, convert::TryInto};

use log::*;
use tari_common::configuration::Network;
use tari_common_types::types::{
    CommitmentFactory,
    CompressedCommitment,
    CompressedPublicKey,
    HashOutput,
    PrivateKey,
    RangeProofService,
    UncompressedCommitment,
    UncompressedPublicKey,
};
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_script::ScriptContext;
use tari_utilities::{ByteArray, hex::Hex};

use crate::{
    MicroMinotari,
    aggregated_body::AggregateBody,
    consensus::{ConsensusConstants, ConsensusManager},
    crypto_factories::CryptoFactories,
    transaction_components::{
        KernelSum,
        SideChainFeature,
        TransactionError,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
        covenants::{CovenantError, MAX_COVENANT_WORK_PER_BODY, OutputHashCache, WorkMeter, hopcroft_karp},
        transaction_output::batch_verify_range_proofs,
    },
    validation::{
        AggregatedBodyValidationError,
        helpers::{
            check_burn_claim_script,
            check_covenant_length,
            check_permitted_output_types,
            check_permitted_range_proof_types,
            check_range_proof_presence,
            check_sidechain_data_rules,
            check_tari_encrypted_data_byte_size,
            check_tari_script_byte_size,
            is_all_unique_and_sorted,
            validate_input_version,
            validate_kernel_version,
            validate_output_version,
        },
    },
};

pub const LOG_TARGET: &str = "c::val::aggregate_body_internal_consistency_validator";

#[derive(Clone)]
pub struct AggregateBodyInternalConsistencyValidator {
    bypass_range_proof_verification: bool,
    consensus_manager: ConsensusManager,
    factories: CryptoFactories,
}

impl AggregateBodyInternalConsistencyValidator {
    pub fn new(
        bypass_range_proof_verification: bool,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
    ) -> Self {
        Self {
            bypass_range_proof_verification,
            consensus_manager,
            factories,
        }
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    /// The reward is the total amount of MicroTari rewarded for this block (block reward + total fees), this should be
    /// 0 for a transaction
    pub fn validate(
        &self,
        body: &AggregateBody,
        tx_offset: &PrivateKey,
        script_offset: &PrivateKey,
        total_reward: Option<MicroMinotari>,
        prev_header: Option<HashOutput>,
        height: u64,
    ) -> Result<(), AggregatedBodyValidationError> {
        let total_reward = total_reward.unwrap_or(MicroMinotari::zero());

        let constants = self.consensus_manager.consensus_constants(height);

        // Structural checks first. Most checks below this point cost at least one signature
        // verification per kernel or output (covenant evaluation does no crypto and is bounded by
        // its own work limit), so the bound on how many of those there can be has to
        // be applied before any of it runs - otherwise a peer can spend a single oversized
        // `submit_transaction` to buy an unbounded amount of our CPU for a body we were always
        // going to reject. Neither of these does any crypto.
        check_weight(body, height, constants)?;
        check_sorting_and_duplicates(body)?;

        // old internal validator
        verify_kernel_signatures(body)?;

        validate_versions(body, constants)?;

        let network = self.consensus_manager.network().as_network();
        for output in body.outputs() {
            validate_individual_output(output, constants, network)?;
        }
        // Every spent output had its script size checked when it was created, so this only rejects inputs that can
        // never be valid. It runs before any input script is executed, to bound the cost of executing them.
        check_input_script_sizes(body.inputs(), constants.max_script_byte_size())?;

        // Check that inputs are allowed to be spent
        check_maturity(height, body.inputs())?;
        check_kernel_lock_height(height, body.kernels())?;
        // Covenants need no crypto and their evaluation cost is bounded, so they run before the kernel sum, range
        // proofs and scripts.
        validate_covenants(body, height)?;

        let total_offset =
            CompressedCommitment::from_commitment(self.factories.commitment.commit_value(tx_offset, total_reward.0));
        validate_kernel_sum(body, total_offset, &self.factories.commitment)?;

        if !self.bypass_range_proof_verification {
            validate_range_proofs(body, &self.factories.range_proof)?;
        }

        let script_offset_g = CompressedPublicKey::from_secret_key(script_offset);
        validate_script_and_script_offset(body, script_offset_g, &self.factories.commitment, prev_header, height)?;

        check_total_burned(body)?;

        Ok(())
    }
}

fn check_template_registration_utxo(sidechain_feature: &SideChainFeature) -> Result<(), AggregatedBodyValidationError> {
    if let Some(template_reg) = sidechain_feature.code_template_registration() {
        let message =
            template_reg.create_signature_message(template_reg.author_signature.get_compressed_public_nonce());
        let signature = template_reg
            .author_signature
            .to_schnorr_signature()
            .map_err(|_| AggregatedBodyValidationError::TemplateAuthorSignatureNotValid)?;
        let author_public_key = template_reg
            .author_public_key
            .to_public_key()
            .map_err(|_| AggregatedBodyValidationError::TemplateAuthorSignatureNotValid)?;

        if !signature.verify_raw_uniform(&author_public_key, &message) {
            return Err(AggregatedBodyValidationError::TemplateAuthorSignatureNotValid);
        }
    }
    Ok(())
}

/// Validates a single output in isolation. `network` is the network the output is being validated for; validator node
/// registration and exit signatures are bound to it.
pub fn validate_individual_output(
    output: &TransactionOutput,
    consensus_constants: &ConsensusConstants,
    network: Network,
) -> Result<(), AggregatedBodyValidationError> {
    check_permitted_output_types(consensus_constants, output)?;
    check_script_size(output, consensus_constants.max_script_byte_size())?;
    check_encrypted_data_byte_size(output, consensus_constants.max_extra_encrypted_data_byte_size())?;
    check_covenant_length(&output.covenant, consensus_constants.max_covenant_length())?;
    check_permitted_range_proof_types(consensus_constants, output)?;
    check_range_proof_presence(output)?;
    check_sidechain_data_rules(output)?;
    check_burn_claim_script(output)?;
    output.verify_metadata_signature()?;
    check_sidechain_features(consensus_constants, output, network)?;

    Ok(())
}

/// Verify the signatures in all kernels contained in this aggregate body. Clients must provide an offset that
/// will be added to the public key used in the signature verification.
fn verify_kernel_signatures(body: &AggregateBody) -> Result<(), AggregatedBodyValidationError> {
    trace!(target: LOG_TARGET, "Checking kernel signatures",);
    for kernel in body.kernels() {
        kernel.verify_signature().map_err(|e| {
            warn!(target: LOG_TARGET, "Kernel ({kernel}) signature failed {e:?}.");
            e
        })?;
    }
    Ok(())
}

/// Verify that the TariScript is not larger than the max size
fn check_script_size(output: &TransactionOutput, max_script_size: usize) -> Result<(), AggregatedBodyValidationError> {
    check_tari_script_byte_size(output.script(), max_script_size).map_err(|e| {
        warn!(
            target: LOG_TARGET,
            "output ({output}) script size exceeded max size {e:?}."
        );
        e
    })
}

/// Verify that no input's TariScript is larger than the max size
fn check_input_script_sizes(
    inputs: &[TransactionInput],
    max_script_size: usize,
) -> Result<(), AggregatedBodyValidationError> {
    for input in inputs {
        check_tari_script_byte_size(input.script()?, max_script_size).map_err(|e| {
            warn!(
                target: LOG_TARGET,
                "input ({input}) script size exceeded max size {e:?}."
            );
            e
        })?;
    }
    Ok(())
}

/// Verify that the TariScript is not larger than the max size
fn check_encrypted_data_byte_size(
    output: &TransactionOutput,
    max_encrypted_data_size: usize,
) -> Result<(), AggregatedBodyValidationError> {
    check_tari_encrypted_data_byte_size(output.encrypted_data(), max_encrypted_data_size).map_err(|e| {
        warn!(
            target: LOG_TARGET,
            "output ({output}) script size exceeded max size {e:?}."
        );
        e
    })
}

/// This function checks for duplicate inputs and outputs. There should be no duplicate inputs or outputs in a
/// aggregated body
fn check_sorting_and_duplicates(body: &AggregateBody) -> Result<(), AggregatedBodyValidationError> {
    if !is_all_unique_and_sorted(body.inputs()) {
        return Err(AggregatedBodyValidationError::UnsortedOrDuplicateInput);
    }

    if !is_all_unique_and_sorted(body.outputs()) {
        return Err(AggregatedBodyValidationError::UnsortedOrDuplicateOutput);
    }

    if !is_all_unique_and_sorted(body.kernels()) {
        return Err(AggregatedBodyValidationError::UnsortedOrDuplicateKernel);
    }

    // Kernels are ordered by signature only, so the sort check above does not catch a repeated excess.
    let mut excesses = HashSet::new();
    for kernel in body.kernels() {
        if !excesses.insert(kernel.excess.as_bytes()) {
            return Err(AggregatedBodyValidationError::DuplicateKernelExcess);
        }
    }

    Ok(())
}

/// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
///
/// The offset_and_reward commitment includes the offset & the total coinbase reward (block reward + fees for
/// block balances, or zero for transaction balances)
// Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn validate_kernel_sum(
    body: &AggregateBody,
    offset_and_reward: CompressedCommitment,
    factory: &CommitmentFactory,
) -> Result<(), AggregatedBodyValidationError> {
    trace!(target: LOG_TARGET, "Checking kernel total");
    let KernelSum { sum: excess, fees } = sum_kernels(body, offset_and_reward)?;
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
    if excess.to_commitment()? != &sum_io.to_commitment()? + &fees {
        return Err(AggregatedBodyValidationError::InvalidAccountingBalance);
    }

    Ok(())
}
/// Calculate the sum of the kernels, taking into account the provided offset, and their constituent fees
// Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn sum_kernels(
    body: &AggregateBody,
    offset_with_fee: CompressedCommitment,
) -> Result<KernelSum, AggregatedBodyValidationError> {
    // Sum all kernel excesses and fees
    let mut kernel_sum = KernelSum {
        fees: MicroMinotari(0),
        sum: offset_with_fee,
    };
    for kernel in body.kernels() {
        kernel_sum.fees = kernel_sum
            .fees
            .checked_add(kernel.fee)
            .ok_or(AggregatedBodyValidationError::InvalidAccountingBalance)?;
        kernel_sum.sum =
            CompressedCommitment::from_commitment(&kernel_sum.sum.to_commitment()? + &kernel.excess.to_commitment()?);
    }
    Ok(kernel_sum)
}

/// Calculate the sum of the outputs - inputs
// Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn sum_commitments(body: &AggregateBody) -> Result<CompressedCommitment, AggregatedBodyValidationError> {
    let mut sum_inputs = UncompressedCommitment::default();
    for inputs in body.inputs() {
        sum_inputs = &sum_inputs + &inputs.commitment()?.to_commitment()?;
    }
    let mut sum_outputs = UncompressedCommitment::default();
    for outputs in body.outputs() {
        sum_outputs = &sum_outputs + &outputs.commitment.to_commitment()?;
    }

    Ok(CompressedCommitment::from_commitment(&sum_outputs - &sum_inputs))
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

/// this will validate the script and script offset of the aggregate body.
// Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn validate_script_and_script_offset(
    body: &AggregateBody,
    script_offset: CompressedPublicKey,
    factory: &CommitmentFactory,
    prev_header: Option<HashOutput>,
    height: u64,
) -> Result<(), AggregatedBodyValidationError> {
    trace!(target: LOG_TARGET, "Checking script and script offset");
    // lets count up the input script public keys
    let mut input_keys = UncompressedPublicKey::default();
    let prev_hash: [u8; 32] = prev_header.unwrap_or_default().as_slice().try_into().unwrap_or([0; 32]);
    for input in body.inputs() {
        let height_dependent = input.script().is_ok_and(|s| s.is_context_sensitive());
        let context = ScriptContext::new(height, &prev_hash, input.commitment()?);
        let key = input
            .run_and_verify_script(factory, Some(context))
            .and_then(|key| Ok(key.to_public_key()?))
            .map_err(|error| AggregatedBodyValidationError::InputScript {
                error,
                height_dependent,
            })?;
        input_keys = input_keys + key;
    }

    // Now lets gather the output public keys and hashes.
    let mut output_keys = UncompressedPublicKey::default();
    for output in body.outputs() {
        // We should not count the coinbase tx here
        if !output.is_coinbase() {
            output_keys = output_keys + output.sender_offset_public_key.to_public_key()?;
        }
    }
    let lhs = input_keys - output_keys;
    if lhs != script_offset.to_public_key()? {
        return Err(AggregatedBodyValidationError::TransactionError(
            TransactionError::ScriptOffset,
        ));
    }
    Ok(())
}

fn validate_covenants(body: &AggregateBody, height: u64) -> Result<(), AggregatedBodyValidationError> {
    validate_covenants_with_limit(body, height, MAX_COVENANT_WORK_PER_BODY)
}

/// Evaluates the covenant of every input against the body's outputs, with at most `limit` work units in total. Each
/// output may satisfy the covenant of at most one input: the body is valid only if every input with a non-empty
/// covenant can be assigned a distinct output that its covenant admits.
pub(crate) fn validate_covenants_with_limit(
    body: &AggregateBody,
    height: u64,
    limit: u64,
) -> Result<(), AggregatedBodyValidationError> {
    let outputs = body.outputs();
    let mut meter = WorkMeter::new(limit);
    let hashes = OutputHashCache::new(outputs.len());
    let mut demands = Vec::new();
    for input in body.inputs() {
        let covenant = input.covenant()?;
        if covenant.is_empty() {
            continue;
        }
        match covenant.matching_outputs(height, input, outputs, &mut meter, Some(&hashes)) {
            Ok(matching) => demands.push(matching),
            Err(CovenantError::WorkLimitExceeded) => {
                return Err(AggregatedBodyValidationError::CovenantWorkLimitExceeded { limit });
            },
            Err(e) => return Err(e.into()),
        }
    }
    if demands.len() > outputs.len() {
        return Err(AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
            inputs: demands.len(),
            matched: outputs.len(),
        });
    }

    // Fast path: each input claims the lowest indexed free output it admits
    let mut claimed = vec![false; outputs.len()];
    let mut greedy_matched = true;
    for demand in &demands {
        let free = demand
            .iter()
            .map(|&index| index as usize)
            .find(|&index| claimed.get(index) == Some(&false));
        match free.and_then(|index| claimed.get_mut(index)) {
            Some(is_claimed) => *is_claimed = true,
            None => {
                greedy_matched = false;
                break;
            },
        }
    }
    if greedy_matched {
        return Ok(());
    }

    let matched = hopcroft_karp(&demands, outputs.len());
    if matched < demands.len() {
        return Err(AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
            inputs: demands.len(),
            matched,
        });
    }
    Ok(())
}

fn check_weight(
    body: &AggregateBody,
    height: u64,
    consensus_constants: &ConsensusConstants,
) -> Result<(), AggregatedBodyValidationError> {
    let block_weight = body
        .calculate_weight(consensus_constants.transaction_weight_params())
        .map_err(|e| {
            AggregatedBodyValidationError::SerializationError(format!("Unable to calculate body weight: {e}"))
        })?;
    let max_weight = consensus_constants.max_block_transaction_weight();
    if block_weight <= max_weight {
        trace!(
            target: LOG_TARGET,
            "Aggregated body at height #{} : {}; weight {} is valid.",
            height,
            body.to_counts_string(),
            block_weight,
        );

        Ok(())
    } else {
        Err(AggregatedBodyValidationError::BlockTooLarge {
            actual_weight: block_weight,
            max_weight,
        })
    }
}

/// Checks that all transactions (given by their kernels) are spendable at the given height
fn check_kernel_lock_height(height: u64, kernels: &[TransactionKernel]) -> Result<(), AggregatedBodyValidationError> {
    if kernels.iter().any(|k| k.lock_height > height) {
        return Err(AggregatedBodyValidationError::MaturityError);
    }
    Ok(())
}

/// Checks that all inputs have matured at the given height
fn check_maturity(height: u64, inputs: &[TransactionInput]) -> Result<(), TransactionError> {
    for input in inputs {
        if !input.is_mature_at(height)? {
            debug!(
                target: LOG_TARGET,
                "Input found that has not yet matured to spending height: {input}"
            );
            return Err(TransactionError::InputMaturity);
        }
    }
    Ok(())
}

/// This function checks the total burned sum in the header ensuring that every burned output is counted in the total
/// sum.
#[allow(clippy::mutable_key_type)]
fn check_total_burned(body: &AggregateBody) -> Result<(), AggregatedBodyValidationError> {
    let mut burned_outputs = HashSet::new();
    for output in body.outputs() {
        if output.is_burned() {
            // we dont care about duplicate commitments are they should have already been checked
            burned_outputs.insert(output.commitment.clone());
        }
    }
    for kernel in body.kernels() {
        if kernel.is_burned() && !burned_outputs.remove(kernel.get_burn_commitment()?) {
            return Err(AggregatedBodyValidationError::InvalidBurnError(
                "Burned kernel does not match burned output".to_string(),
            ));
        }
    }

    if !burned_outputs.is_empty() {
        return Err(AggregatedBodyValidationError::InvalidBurnError(
            "Burned output has no matching burned kernel".to_string(),
        ));
    }
    Ok(())
}

fn check_sidechain_features(
    constants: &ConsensusConstants,
    output: &TransactionOutput,
    network: Network,
) -> Result<(), AggregatedBodyValidationError> {
    let Some(sidechain_feature) = output.features.sidechain_feature.as_ref() else {
        return Ok(());
    };
    check_sidechain_id_proof_of_knowledge(sidechain_feature)?;
    check_validator_node_registration_utxo(constants, output, sidechain_feature, network)?;
    check_template_registration_utxo(sidechain_feature)?;
    check_validator_node_exit_utxo(sidechain_feature, network)?;

    Ok(())
}
fn check_sidechain_id_proof_of_knowledge(
    sidechain_feature: &SideChainFeature,
) -> Result<(), AggregatedBodyValidationError> {
    if !sidechain_feature.is_sidechain_id_valid() {
        return Err(AggregatedBodyValidationError::ValidatorNodeInvalidSidechainIdKnowledgeProof);
    }

    Ok(())
}

fn check_validator_node_registration_utxo(
    consensus_constants: &ConsensusConstants,
    utxo: &TransactionOutput,
    sidechain_feature: &SideChainFeature,
    network: Network,
) -> Result<(), AggregatedBodyValidationError> {
    let Some(reg) = sidechain_feature.validator_node_registration() else {
        return Ok(());
    };

    if utxo.minimum_value_promise < consensus_constants.validator_node_registration_min_deposit_amount() {
        return Err(
            AggregatedBodyValidationError::ValidatorNodeRegistrationMinDepositAmount {
                min: consensus_constants.validator_node_registration_min_deposit_amount(),
                actual: utxo.minimum_value_promise,
            },
        );
    }
    if utxo.features.maturity < consensus_constants.validator_node_registration_min_lock_height() {
        return Err(AggregatedBodyValidationError::ValidatorNodeRegistrationMinLockHeight {
            min: consensus_constants.validator_node_registration_min_lock_height(),
            actual: utxo.features.maturity,
        });
    }

    if !reg.is_valid_signature_for(
        network.as_byte(),
        sidechain_feature.sidechain_id.as_ref().map(|id| id.public_key()),
    ) {
        return Err(AggregatedBodyValidationError::InvalidValidatorNodeSignature);
    }

    Ok(())
}

fn check_validator_node_exit_utxo(
    sidechain_feature: &SideChainFeature,
    network: Network,
) -> Result<(), AggregatedBodyValidationError> {
    let Some(exit) = sidechain_feature.validator_node_exit() else {
        return Ok(());
    };

    if !exit.is_valid_signature_for(
        network.as_byte(),
        sidechain_feature.sidechain_id.as_ref().map(|id| id.public_key()),
    ) {
        return Err(AggregatedBodyValidationError::InvalidValidatorNodeSignature);
    }

    Ok(())
}

fn validate_versions(
    body: &AggregateBody,
    consensus_constants: &ConsensusConstants,
) -> Result<(), AggregatedBodyValidationError> {
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
    #![allow(clippy::indexing_slicing)]
    use std::iter;

    use rand::seq::SliceRandom;
    use tari_common::configuration::Network;
    use tari_common_types::types::RANGE_PROOF_AGGREGATION_FACTOR;
    use tari_crypto::keys::SecretKey;
    use tari_script::script;

    use super::*;
    use crate::{
        key_manager::KeyManager,
        test_helpers,
        transaction_components::{KernelFeatures, OutputFeatures, TransactionInputVersion, covenants::Covenant},
    };
    mod check_lock_height {
        use super::*;

        #[test]
        fn it_checks_the_kernel_timelock() {
            let mut kernel = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::empty());
            kernel.lock_height = 2;
            assert!(matches!(
                check_kernel_lock_height(1, &[kernel.clone()]),
                Err(AggregatedBodyValidationError::MaturityError)
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
                Default::default(),
                Default::default(),
                MicroMinotari::zero(),
            );

            assert!(matches!(
                check_maturity(1, std::slice::from_ref(&input)),
                Err(TransactionError::InputMaturity)
            ));

            assert!(matches!(
                check_maturity(4, std::slice::from_ref(&input)),
                Err(TransactionError::InputMaturity)
            ));

            check_maturity(5, std::slice::from_ref(&input)).unwrap();
            check_maturity(6, &[input]).unwrap();
        }
    }

    #[test]
    fn check_burned_succeeds_for_valid_outputs() {
        let mut kernel1 = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::create_burn());
        let mut kernel2 = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::create_burn());

        let key_manager = KeyManager::new_random().unwrap();
        let (output1, _, _) = test_helpers::create_utxo(
            100.into(),
            &key_manager,
            &OutputFeatures::create_burn_output(),
            &script!(Nop).unwrap(),
            &Covenant::default(),
            0.into(),
        );
        let (output2, _, _) = test_helpers::create_utxo(
            101.into(),
            &key_manager,
            &OutputFeatures::create_burn_output(),
            &script!(Nop).unwrap(),
            &Covenant::default(),
            0.into(),
        );
        let (output3, _, _) = test_helpers::create_utxo(
            102.into(),
            &key_manager,
            &OutputFeatures::create_burn_output(),
            &script!(Nop).unwrap(),
            &Covenant::default(),
            0.into(),
        );

        kernel1.burn_commitment = Some(output1.commitment.clone());
        kernel2.burn_commitment = Some(output2.commitment.clone());
        let kernel3 = kernel1.clone();

        let mut body = AggregateBody::new_unsorted(Vec::new(), vec![output1.clone(), output2.clone()], vec![
            kernel1.clone(),
            kernel2.clone(),
        ]);
        assert!(check_total_burned(&body).is_ok());
        // lets add an extra kernel
        body.add_kernels([kernel3]);
        assert!(check_total_burned(&body).is_err());
        // lets add a kernel commitment mismatch
        body.add_outputs(vec![output3.clone()]);
        assert!(check_total_burned(&body).is_err());
        // Lets try one with a commitment with no kernel
        let body2 = AggregateBody::new_unsorted(Vec::new(), vec![output1, output2, output3], vec![kernel1, kernel2]);
        assert!(check_total_burned(&body2).is_err());
    }

    /// `is_all_unique_and_sorted` is what makes an output commitment unique within a body: `Ord for
    /// TransactionOutput` compares nothing but the commitment, so two outputs sharing a commitment compare `Equal`
    /// and the check rejects them no matter what the rest of the output looks like. The same holds for inputs, which
    /// are ordered on the hash of the output they spend.
    mod duplicate_commitments {
        use tari_script::{ExecutionStack, StackItem};

        use super::*;
        use crate::transaction_components::SpentOutput;

        fn utxo(key_manager: &KeyManager) -> TransactionOutput {
            let (output, _, _) = test_helpers::create_utxo(
                100.into(),
                key_manager,
                &OutputFeatures::default(),
                &script!(Nop).unwrap(),
                &Covenant::default(),
                0.into(),
            );
            output
        }

        fn input_spending(output: &TransactionOutput, input_data: ExecutionStack) -> TransactionInput {
            TransactionInput::new_current_version(
                SpentOutput::create_from_output(output.clone()),
                input_data,
                Default::default(),
            )
        }

        #[test]
        fn it_accepts_distinct_output_commitments() {
            let key_manager = KeyManager::new_random().unwrap();
            let mut outputs = vec![utxo(&key_manager), utxo(&key_manager), utxo(&key_manager)];
            outputs.sort();
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), outputs, Vec::new());
            check_sorting_and_duplicates(&body).unwrap();
        }

        #[test]
        fn it_rejects_two_identical_outputs() {
            let key_manager = KeyManager::new_random().unwrap();
            let output = utxo(&key_manager);
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), vec![output.clone(), output], Vec::new());
            assert!(matches!(
                check_sorting_and_duplicates(&body),
                Err(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
            ));
        }

        #[test]
        fn it_rejects_two_outputs_sharing_a_commitment() {
            let key_manager = KeyManager::new_random().unwrap();
            let output = utxo(&key_manager);
            // Same commitment, different canonical hash. A duplicate check keyed on the output hash would wave this
            // pair through; only the commitment ordering catches it. The commitment is what the UTXO set is keyed on,
            // so letting both into a block would put two entries under one key.
            let mut twin = output.clone();
            twin.script = script!(Nop Nop).unwrap();
            assert_eq!(output.commitment, twin.commitment);
            assert_ne!(output.hash(), twin.hash());

            let body = AggregateBody::new_sorted_unchecked(Vec::new(), vec![output.clone(), twin.clone()], Vec::new());
            assert!(matches!(
                check_sorting_and_duplicates(&body),
                Err(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
            ));

            // The order the two are presented in must not matter.
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), vec![twin, output], Vec::new());
            assert!(matches!(
                check_sorting_and_duplicates(&body),
                Err(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
            ));
        }

        #[test]
        fn it_rejects_a_duplicate_commitment_hidden_among_distinct_outputs() {
            let key_manager = KeyManager::new_random().unwrap();
            let mut outputs = vec![utxo(&key_manager), utxo(&key_manager), utxo(&key_manager)];
            outputs.sort();
            // Re-insert the middle output next to itself so the body stays sorted; only the duplicate breaks it.
            outputs.insert(1, outputs[1].clone());
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), outputs, Vec::new());
            assert!(matches!(
                check_sorting_and_duplicates(&body),
                Err(AggregatedBodyValidationError::UnsortedOrDuplicateOutput)
            ));
        }

        #[test]
        fn it_rejects_two_inputs_spending_the_same_output() {
            let key_manager = KeyManager::new_random().unwrap();
            let output = utxo(&key_manager);
            let input = input_spending(&output, ExecutionStack::default());
            // A competing double spend differs only in its witness, which `canonical_hash` distinguishes but `Ord`
            // deliberately does not.
            let twin = input_spending(&output, ExecutionStack::new(vec![StackItem::Number(1)]));
            assert_eq!(input.output_hash(), twin.output_hash());
            assert_ne!(input.canonical_hash(), twin.canonical_hash());

            let body = AggregateBody::new_sorted_unchecked(vec![input, twin], Vec::new(), Vec::new());
            assert!(matches!(
                check_sorting_and_duplicates(&body),
                Err(AggregatedBodyValidationError::UnsortedOrDuplicateInput)
            ));
        }
    }

    mod covenants {
        use super::*;
        use crate::covenant;

        pub(super) fn input_with_covenant(covenant: Covenant) -> TransactionInput {
            TransactionInput::new_with_output_data(
                TransactionInputVersion::get_current_version(),
                OutputFeatures::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
                covenant,
                Default::default(),
                Default::default(),
                Default::default(),
                MicroMinotari::zero(),
            )
        }

        pub(super) fn outputs(n: usize, key_manager: &KeyManager) -> Vec<TransactionOutput> {
            let mut outputs = Vec::new();
            for _ in 0..n {
                let (output, _, _) = test_helpers::create_utxo(
                    100.into(),
                    key_manager,
                    &OutputFeatures::default(),
                    &script!(Nop).unwrap(),
                    &Covenant::default(),
                    0.into(),
                );
                outputs.push(output);
            }
            outputs
        }

        #[test]
        #[allow(clippy::arithmetic_side_effects)]
        fn covenant_evaluation_respects_work_limit() {
            let key_manager = KeyManager::new_random().unwrap();
            let n = 10;
            let mut outputs = outputs(n, &key_manager);
            outputs[3].features.maturity = 42;
            let covenant = covenant!(or(
                field_eq(@field::features_maturity, @uint(42),),
                field_eq(@field::features_maturity, @uint(7))
            ))
            .unwrap();
            let body = AggregateBody::new_sorted_unchecked(
                vec![input_with_covenant(covenant.clone())],
                outputs.clone(),
                Vec::new(),
            );

            // 7 tokens, the initial set of n, 3 filters, 2n for `or` and n for each `field_eq`
            let exact_cost = 7 + n as u64 + 3 + 2 * n as u64 + 2 * n as u64;
            validate_covenants_with_limit(&body, 0, exact_cost).unwrap();
            let err = validate_covenants_with_limit(&body, 0, exact_cost - 1).unwrap_err();
            assert!(
                matches!(err, AggregatedBodyValidationError::CovenantWorkLimitExceeded { limit } if limit == exact_cost - 1),
                "{err}"
            );

            // The public path uses MAX_COVENANT_WORK_PER_BODY
            const { assert!(MAX_COVENANT_WORK_PER_BODY == 250_000) };
            validate_covenants(&body, 0).unwrap();
            let inputs_over_limit = usize::try_from(MAX_COVENANT_WORK_PER_BODY / exact_cost + 1).unwrap();
            let body = AggregateBody::new_sorted_unchecked(
                vec![input_with_covenant(covenant); inputs_over_limit],
                outputs,
                Vec::new(),
            );
            let err = validate_covenants(&body, 0).unwrap_err();
            assert!(
                matches!(err, AggregatedBodyValidationError::CovenantWorkLimitExceeded { limit } if limit == MAX_COVENANT_WORK_PER_BODY),
                "{err}"
            );
        }

        fn admits_one(output: &TransactionOutput) -> TransactionInput {
            input_with_covenant(covenant!(output_hash_eq(@hash(output.hash()))).unwrap())
        }

        fn admits_either(a: &TransactionOutput, b: &TransactionOutput) -> TransactionInput {
            input_with_covenant(
                covenant!(or(output_hash_eq(@hash(a.hash()),), output_hash_eq(@hash(b.hash())))).unwrap(),
            )
        }

        fn check(
            inputs: Vec<TransactionInput>,
            outputs: Vec<TransactionOutput>,
        ) -> Result<(), AggregatedBodyValidationError> {
            validate_covenants(&AggregateBody::new_sorted_unchecked(inputs, outputs, Vec::new()), 0)
        }

        #[test]
        fn covenant_outputs_are_claimed_once() {
            let key_manager = KeyManager::new_random().unwrap();
            let outputs = outputs(2, &key_manager);
            let (o1, o2) = (outputs[0].clone(), outputs[1].clone());

            let err = check(vec![admits_one(&o1), admits_one(&o1)], vec![o1.clone()]).unwrap_err();
            assert!(
                matches!(err, AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
                    inputs: 2,
                    matched: 1
                }),
                "{err}"
            );
            let err = check(vec![admits_one(&o1), admits_one(&o1)], vec![o1.clone(), o2.clone()]).unwrap_err();
            assert!(
                matches!(err, AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
                    inputs: 2,
                    matched: 1
                }),
                "{err}"
            );

            check(vec![admits_either(&o1, &o2), admits_one(&o1)], vec![o1.clone(), o2]).unwrap();

            // Inputs with an empty covenant claim nothing
            let empty = || input_with_covenant(Covenant::default());
            check(vec![empty(), admits_one(&o1), empty(), empty()], vec![o1]).unwrap();
        }

        #[test]
        fn covenant_claiming_is_order_independent() {
            let key_manager = KeyManager::new_random().unwrap();
            let outputs = outputs(2, &key_manager);
            let (o1, o2) = (outputs[0].clone(), outputs[1].clone());
            let a = admits_either(&o1, &o2);
            let b = admits_one(&o2);
            let c = admits_one(&o1);

            for outputs in [vec![o1.clone(), o2.clone()], vec![o2.clone(), o1.clone()]] {
                for inputs in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
                    check(inputs, outputs.clone()).unwrap();
                }
            }
            // With o2 first, greedy gives o2 to `a` and leaves nothing for `b`; the matching still finds one
            let demands = vec![vec![0, 1], vec![0]];
            assert_eq!(hopcroft_karp(&demands, 2), 2);

            for outputs in [vec![o1.clone(), o2.clone()], vec![o2, o1]] {
                for inputs in [
                    vec![a.clone(), b.clone(), c.clone()],
                    vec![a.clone(), c.clone(), b.clone()],
                    vec![b.clone(), a.clone(), c.clone()],
                    vec![b.clone(), c.clone(), a.clone()],
                    vec![c.clone(), a.clone(), b.clone()],
                    vec![c.clone(), b.clone(), a.clone()],
                ] {
                    let err = check(inputs, outputs.clone()).unwrap_err();
                    assert!(
                        matches!(err, AggregatedBodyValidationError::CovenantOutputsClaimedTwice {
                            inputs: 3,
                            ..
                        }),
                        "{err}"
                    );
                }
            }
        }

        #[test]
        fn covenant_checks_run_before_range_proofs() {
            let key_manager = KeyManager::new_random().unwrap();
            let mut outputs = outputs(2, &key_manager);
            // A proof for a different commitment does not verify, but the metadata signature does not cover it
            outputs[0].proof = outputs[1].proof.clone();
            let output = outputs.remove(0);
            let input = input_with_covenant(covenant!(field_eq(@field::features_maturity, @uint(99))).unwrap());
            let body = AggregateBody::new_sorted_unchecked(vec![input], vec![output], Vec::new());
            let factories = CryptoFactories::new(RANGE_PROOF_AGGREGATION_FACTOR);
            assert!(validate_range_proofs(&body, &factories.range_proof).is_err());

            let err = AggregateBodyInternalConsistencyValidator::new(
                false,
                ConsensusManager::builder(Network::LocalNet).build(),
                factories,
            )
            .validate(&body, &Default::default(), &Default::default(), None, None, 0)
            .unwrap_err();
            assert!(
                matches!(
                    err,
                    AggregatedBodyValidationError::CovenantError(CovenantError::NoMatchingOutputs)
                ),
                "{err}"
            );
        }
    }

    mod transaction_ordering {
        use super::*;

        #[test]
        fn it_rejects_unordered_bodies() {
            let mut kernels =
                iter::repeat_with(|| test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::default()))
                    .take(10)
                    .collect::<Vec<_>>();

            // Sort the kernels, we'll check that the outputs fail the sorting check
            kernels.sort();

            let key_manager = KeyManager::new_random().unwrap();
            let mut outputs = Vec::new();
            for _ in 0..10 {
                let (o, _, _) = test_helpers::create_utxo(
                    100.into(),
                    &key_manager,
                    &OutputFeatures::create_burn_output(),
                    &script!(Nop).unwrap(),
                    &Covenant::default(),
                    0.into(),
                );
                outputs.push(o);
            }

            while is_all_unique_and_sorted(&outputs) {
                // Shuffle the outputs until they are not sorted
                outputs.shuffle(&mut rand::rng());
            }

            // Break the contract of new_unsorted_unchecked by calling it with unsorted outputs. The validator must not
            // rely on the sorted flag.
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), outputs, kernels);
            let err = AggregateBodyInternalConsistencyValidator::new(
                true,
                ConsensusManager::builder(Network::LocalNet).build(),
                CryptoFactories::new(RANGE_PROOF_AGGREGATION_FACTOR),
            )
            .validate(&body, &Default::default(), &Default::default(), None, None, u64::MAX)
            .unwrap_err();

            assert!(matches!(err, AggregatedBodyValidationError::UnsortedOrDuplicateOutput));
        }

        fn kernel_with_excess(k: &PrivateKey) -> TransactionKernel {
            let excess = CompressedCommitment::from_compressed_key(CompressedPublicKey::from_secret_key(k));
            let sig = test_helpers::create_signature(k.clone(), 0.into(), 0, KernelFeatures::empty());
            TransactionKernel::new_current_version(KernelFeatures::empty(), 0.into(), 0, excess, sig, None)
        }

        #[test]
        fn aggregate_body_rejects_duplicate_kernel_excess() {
            let validator = AggregateBodyInternalConsistencyValidator::new(
                true,
                ConsensusManager::builder(Network::LocalNet).build(),
                CryptoFactories::new(RANGE_PROOF_AGGREGATION_FACTOR),
            );

            let k = PrivateKey::random(&mut rand::rng());
            let mut kernels = vec![kernel_with_excess(&k), kernel_with_excess(&k)];
            kernels.sort();
            assert_ne!(kernels[0].excess_sig, kernels[1].excess_sig);
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), Vec::new(), kernels);
            let offset = PrivateKey::default() - &k - &k;
            let err = validator
                .validate(&body, &offset, &Default::default(), None, None, 0)
                .unwrap_err();
            assert!(matches!(err, AggregatedBodyValidationError::DuplicateKernelExcess));

            let k2 = PrivateKey::random(&mut rand::rng());
            let mut kernels = vec![kernel_with_excess(&k), kernel_with_excess(&k2)];
            kernels.sort();
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), Vec::new(), kernels);
            let offset = PrivateKey::default() - &k - &k2;
            validator
                .validate(&body, &offset, &Default::default(), None, None, 0)
                .unwrap();
        }
    }

    mod range_proof_presence {
        use tari_common_types::types::RangeProof;

        use super::*;
        use crate::transaction_components::RangeProofType;

        fn utxo(key_manager: &KeyManager, range_proof_type: RangeProofType) -> TransactionOutput {
            let value = MicroMinotari(100);
            let minimum_value_promise = if range_proof_type == RangeProofType::RevealedValue {
                value
            } else {
                MicroMinotari::zero()
            };
            let (output, _, _) = test_helpers::create_utxo(
                value,
                key_manager,
                &OutputFeatures {
                    range_proof_type,
                    ..Default::default()
                },
                &script!(Nop).unwrap(),
                &Covenant::default(),
                minimum_value_promise,
            );
            output
        }

        #[test]
        fn revealed_value_output_with_proof_is_rejected() {
            let key_manager = KeyManager::new_random().unwrap();
            let consensus_manager = test_helpers::create_consensus_manager();
            let constants = consensus_manager.consensus_constants(0);
            let factories = CryptoFactories::default();
            let valid = utxo(&key_manager, RangeProofType::RevealedValue);
            validate_individual_output(&valid, constants, Network::LocalNet).unwrap();

            for proof in [RangeProof::from(vec![1; 32]), RangeProof::default()] {
                let mut output = valid.clone();
                output.proof = Some(proof);
                let err = validate_individual_output(&output, constants, Network::LocalNet).unwrap_err();
                assert!(matches!(
                    err,
                    AggregatedBodyValidationError::RangeProofPresenceMismatch {
                        range_proof_type: RangeProofType::RevealedValue,
                        proof_present: true
                    }
                ));
                assert!(output.verify_range_proof(&factories.range_proof).is_err());
                assert!(batch_verify_range_proofs(&factories.range_proof, &[&output]).is_err());
                output.verify_metadata_signature().unwrap();
            }
        }

        #[test]
        fn bulletproof_output_without_proof_is_rejected() {
            let key_manager = KeyManager::new_random().unwrap();
            let consensus_manager = test_helpers::create_consensus_manager();
            let constants = consensus_manager.consensus_constants(0);
            let mut output = utxo(&key_manager, RangeProofType::BulletProofPlus);
            validate_individual_output(&output, constants, Network::LocalNet).unwrap();

            output.proof = None;
            let err = validate_individual_output(&output, constants, Network::LocalNet).unwrap_err();
            assert!(matches!(
                err,
                AggregatedBodyValidationError::RangeProofPresenceMismatch {
                    range_proof_type: RangeProofType::BulletProofPlus,
                    proof_present: false
                }
            ));
        }

        #[test]
        fn range_proof_presence_is_checked_when_range_proof_verification_is_bypassed() {
            let key_manager = KeyManager::new_random().unwrap();
            let mut output = utxo(&key_manager, RangeProofType::RevealedValue);
            output.proof = Some(RangeProof::from(vec![1; 32]));
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), vec![output], Vec::new());
            let err = AggregateBodyInternalConsistencyValidator::new(
                true,
                test_helpers::create_consensus_manager(),
                CryptoFactories::default(),
            )
            .validate(&body, &Default::default(), &Default::default(), None, None, 0)
            .unwrap_err();
            assert!(matches!(
                err,
                AggregatedBodyValidationError::RangeProofPresenceMismatch {
                    range_proof_type: RangeProofType::RevealedValue,
                    proof_present: true
                }
            ));
        }
    }

    mod input_script {
        use tari_script::{ExecutionStack, TariScript};

        use super::*;
        use crate::{
            test_helpers::{TestParams, UtxoTestParams, spend_utxos},
            transaction_components::WalletOutput,
            txn_schema,
        };

        const HEIGHT: u64 = 50;

        fn input(key_manager: &KeyManager, script: TariScript) -> TransactionInput {
            TestParams::new(key_manager)
                .create_input(
                    UtxoTestParams {
                        script,
                        ..Default::default()
                    },
                    key_manager,
                )
                .to_transaction_input(key_manager)
                .unwrap()
        }

        fn check(inputs: Vec<TransactionInput>, script_offset: CompressedPublicKey) -> AggregatedBodyValidationError {
            let body = AggregateBody::new_unsorted(inputs, Vec::new(), Vec::new());
            validate_script_and_script_offset(&body, script_offset, &CommitmentFactory::default(), None, HEIGHT)
                .unwrap_err()
        }

        fn ordinary(key_manager: &KeyManager) -> TransactionInput {
            input(key_manager, script!(Nop).unwrap())
        }

        fn height_reading(key_manager: &KeyManager) -> TransactionInput {
            input(key_manager, script!(CheckHeightVerify(1)).unwrap())
        }

        #[test]
        fn script_errors_are_input_script_failures() {
            let key_manager = KeyManager::new_random().unwrap();
            let err = check(vec![input(&key_manager, script!(Return).unwrap())], Default::default());
            assert!(matches!(err, AggregatedBodyValidationError::InputScript {
                error: TransactionError::ScriptError(_),
                height_dependent: false
            }));

            let not_yet = input(&key_manager, script!(CheckHeightVerify(HEIGHT + 1)).unwrap());
            let err = check(vec![not_yet], Default::default());
            assert!(matches!(err, AggregatedBodyValidationError::InputScript {
                error: TransactionError::ScriptError(_),
                height_dependent: true
            }));
        }

        #[test]
        fn only_the_failing_input_decides_the_flag() {
            let key_manager = KeyManager::new_random().unwrap();
            let failing = input(&key_manager, script!(Return).unwrap());
            let err = check(vec![height_reading(&key_manager), failing], Default::default());
            assert!(matches!(err, AggregatedBodyValidationError::InputScript {
                height_dependent: false,
                ..
            }));
        }

        #[test]
        fn script_signature_failures_are_input_script_failures() {
            let key_manager = KeyManager::new_random().unwrap();
            for (mut input, expected) in [(ordinary(&key_manager), false), (height_reading(&key_manager), true)] {
                input.script_signature = ordinary(&key_manager).script_signature;
                let err = check(vec![input], Default::default());
                assert!(
                    matches!(
                        err,
                        AggregatedBodyValidationError::InputScript {
                            error: TransactionError::InvalidSignatureError(_),
                            height_dependent
                        } if height_dependent == expected
                    ),
                    "{err:?}"
                );
            }
        }

        #[test]
        fn script_offset_mismatch_is_not_an_input_script_failure() {
            let key_manager = KeyManager::new_random().unwrap();
            for inputs in [vec![ordinary(&key_manager)], vec![
                ordinary(&key_manager),
                height_reading(&key_manager),
            ]] {
                let err = check(inputs, test_helpers::new_public_key());
                assert!(
                    matches!(
                        err,
                        AggregatedBodyValidationError::TransactionError(TransactionError::ScriptOffset)
                    ),
                    "{err:?}"
                );
            }
        }

        #[test]
        fn kernel_signature_failure_is_not_an_input_script_failure() {
            let key_manager = KeyManager::new_random().unwrap();
            let mut kernel = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::empty());
            kernel.fee = 1.into();
            let body = AggregateBody::new_unsorted(vec![height_reading(&key_manager)], Vec::new(), vec![kernel]);
            let err = verify_kernel_signatures(&body).unwrap_err();
            assert!(matches!(
                err,
                AggregatedBodyValidationError::TransactionError(TransactionError::InvalidSignatureError(_))
            ));
        }

        /// A script that returns a different key below height 100 is only valid at or above it
        #[test]
        fn height_branching_script_fails_only_below_its_height() {
            let key_manager = KeyManager::new_random().unwrap();
            let params = TestParams::new(&key_manager);
            let other = Box::new(test_helpers::new_public_key());
            let signed = Box::new(params.script_key_pk.clone());
            let script =
                script!(CheckHeight(100) LtZero IfThen PushPubKey(other) Else PushPubKey(signed) EndIf).unwrap();
            let utxo: WalletOutput = params.create_input(
                UtxoTestParams {
                    value: 100_000.into(),
                    script,
                    input_data: Some(ExecutionStack::default()),
                    ..Default::default()
                },
                &key_manager,
            );
            let (tx, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![50_000.into()]), &key_manager);

            let validator = AggregateBodyInternalConsistencyValidator::new(
                true,
                ConsensusManager::builder(Network::LocalNet).build(),
                CryptoFactories::default(),
            );
            let validate = |height| validator.validate(&tx.body, &tx.offset, &tx.script_offset, None, None, height);
            let err = validate(99).unwrap_err();
            assert!(
                matches!(err, AggregatedBodyValidationError::InputScript {
                    error: TransactionError::InvalidSignatureError(_),
                    height_dependent: true
                }),
                "{err:?}"
            );
            validate(100).unwrap();
        }
    }
}
