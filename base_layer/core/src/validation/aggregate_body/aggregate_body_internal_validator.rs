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

use log::{trace, warn};
use tari_common_types::types::{Commitment, CommitmentFactory, HashOutput, PrivateKey, PublicKey, RangeProofService};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::PublicKey as PublicKeyTrait,
    ristretto::pedersen::PedersenCommitment,
};
use tari_script::ScriptContext;
use tari_utilities::hex::Hex;

use crate::{
    consensus::{ConsensusConstants, ConsensusManager},
    transactions::{
        aggregated_body::AggregateBody,
        tari_amount::MicroMinotari,
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
    validation::{
        helpers::{
            check_covenant_length,
            check_permitted_output_types,
            check_permitted_range_proof_types,
            check_tari_script_byte_size,
            is_all_unique_and_sorted,
            validate_input_version,
            validate_kernel_version,
            validate_output_version,
        },
        ValidationError,
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
    ) -> Result<(), ValidationError> {
        let total_reward = total_reward.unwrap_or(MicroMinotari::zero());

        // old internal validator
        verify_kernel_signatures(body)?;

        let constants = self.consensus_manager.consensus_constants(height);

        validate_versions(body, constants)?;

        for output in body.outputs() {
            check_permitted_output_types(constants, output)?;
            check_script_size(output, constants.max_script_byte_size())?;
            check_covenant_length(&output.covenant, constants.max_covenant_length())?;
            check_permitted_range_proof_types(constants, output)?;
            check_validator_node_registration_utxo(constants, output)?;
        }

        check_weight(body, height, constants)?;
        check_sorting_and_duplicates(body)?;

        // Check that the inputs are are allowed to be spent
        check_maturity(height, body.inputs())?;
        check_kernel_lock_height(height, body.kernels())?;

        let total_offset = self.factories.commitment.commit_value(tx_offset, total_reward.0);
        validate_kernel_sum(body, total_offset, &self.factories.commitment)?;

        if !self.bypass_range_proof_verification {
            validate_range_proofs(body, &self.factories.range_proof)?;
        }
        verify_metadata_signatures(body)?;

        let script_offset_g = PublicKey::from_secret_key(script_offset);
        validate_script_and_script_offset(body, script_offset_g, &self.factories.commitment, prev_header, height)?;
        validate_covenants(body, height)?;

        check_total_burned(body)?;

        Ok(())
    }
}

/// Verify the signatures in all kernels contained in this aggregate body. Clients must provide an offset that
/// will be added to the public key used in the signature verification.
fn verify_kernel_signatures(body: &AggregateBody) -> Result<(), ValidationError> {
    trace!(target: LOG_TARGET, "Checking kernel signatures",);
    for kernel in body.kernels() {
        kernel.verify_signature().map_err(|e| {
            warn!(target: LOG_TARGET, "Kernel ({}) signature failed {:?}.", kernel, e);
            e
        })?;
    }
    Ok(())
}

/// Verify that the TariScript is not larger than the max size
fn check_script_size(output: &TransactionOutput, max_script_size: usize) -> Result<(), ValidationError> {
    check_tari_script_byte_size(output.script(), max_script_size).map_err(|e| {
        warn!(
            target: LOG_TARGET,
            "output ({}) script size exceeded max size {:?}.", output, e
        );
        e
    })
}

/// This function checks for duplicate inputs and outputs. There should be no duplicate inputs or outputs in a
/// aggregated body
fn check_sorting_and_duplicates(body: &AggregateBody) -> Result<(), ValidationError> {
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

/// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
///
/// The offset_and_reward commitment includes the offset & the total coinbase reward (block reward + fees for
/// block balances, or zero for transaction balances)
fn validate_kernel_sum(
    body: &AggregateBody,
    offset_and_reward: Commitment,
    factory: &CommitmentFactory,
) -> Result<(), ValidationError> {
    trace!(target: LOG_TARGET, "Checking kernel total");
    let KernelSum { sum: excess, fees } = sum_kernels(body, offset_and_reward);
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
        return Err(ValidationError::CustomError(
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
            fees: MicroMinotari(0),
            sum: offset_with_fee,
        },
        |acc, val| KernelSum {
            fees: acc.fees + val.fee,
            sum: &acc.sum + &val.excess,
        },
    )
}

/// Calculate the sum of the outputs - inputs
fn sum_commitments(body: &AggregateBody) -> Result<Commitment, ValidationError> {
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

fn verify_metadata_signatures(body: &AggregateBody) -> Result<(), ValidationError> {
    trace!(target: LOG_TARGET, "Checking sender signatures");
    for o in body.outputs() {
        o.verify_metadata_signature()?;
    }
    Ok(())
}

/// this will validate the script and script offset of the aggregate body.
fn validate_script_and_script_offset(
    body: &AggregateBody,
    script_offset: PublicKey,
    factory: &CommitmentFactory,
    prev_header: Option<HashOutput>,
    height: u64,
) -> Result<(), ValidationError> {
    trace!(target: LOG_TARGET, "Checking script and script offset");
    // lets count up the input script public keys
    let mut input_keys = PublicKey::default();
    let prev_hash: [u8; 32] = prev_header.unwrap_or_default().as_slice().try_into().unwrap_or([0; 32]);
    for input in body.inputs() {
        let context = ScriptContext::new(height, &prev_hash, input.commitment()?);
        input_keys = input_keys + input.run_and_verify_script(factory, Some(context))?;
    }

    // Now lets gather the output public keys and hashes.
    let mut output_keys = PublicKey::default();
    for output in body.outputs() {
        // We should not count the coinbase tx here
        if !output.is_coinbase() {
            output_keys = output_keys + output.sender_offset_public_key.clone();
        }
    }
    let lhs = input_keys - output_keys;
    if lhs != script_offset {
        return Err(ValidationError::TransactionError(TransactionError::ScriptOffset));
    }
    Ok(())
}

fn validate_covenants(body: &AggregateBody, height: u64) -> Result<(), ValidationError> {
    for input in body.inputs() {
        input.covenant()?.execute(height, input, body.outputs())?;
    }
    Ok(())
}

fn check_weight(
    body: &AggregateBody,
    height: u64,
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    let block_weight = body
        .calculate_weight(consensus_constants.transaction_weight_params())
        .map_err(|e| ValidationError::CustomError(e.to_string()))?;
    let max_weight = consensus_constants.max_block_transaction_weight();
    if block_weight <= max_weight {
        trace!(
            target: LOG_TARGET,
            "SV - Block contents for block #{} : {}; weight {}.",
            height,
            body.to_counts_string(),
            block_weight,
        );

        Ok(())
    } else {
        Err(ValidationError::BlockTooLarge {
            actual_weight: block_weight,
            max_weight,
        })
    }
}

/// Checks that all transactions (given by their kernels) are spendable at the given height
fn check_kernel_lock_height(height: u64, kernels: &[TransactionKernel]) -> Result<(), ValidationError> {
    if kernels.iter().any(|k| k.lock_height > height) {
        return Err(ValidationError::MaturityError);
    }
    Ok(())
}

/// Checks that all inputs have matured at the given height
fn check_maturity(height: u64, inputs: &[TransactionInput]) -> Result<(), TransactionError> {
    for input in inputs {
        if !input.is_mature_at(height)? {
            warn!(
                target: LOG_TARGET,
                "Input found that has not yet matured to spending height: {}", input
            );
            return Err(TransactionError::InputMaturity);
        }
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

#[cfg(test)]
mod test {
    use std::iter;

    use futures::StreamExt;
    use rand::seq::SliceRandom;
    use tari_common::configuration::Network;
    use tari_common_types::types::RANGE_PROOF_AGGREGATION_FACTOR;
    use tari_script::script;

    use super::*;
    use crate::{
        covenants::Covenant,
        transactions::{
            test_helpers,
            test_helpers::create_test_core_key_manager_with_memory_db,
            transaction_components::{KernelFeatures, OutputFeatures, TransactionInputVersion},
        },
    };

    mod check_lock_height {
        use super::*;

        #[test]
        fn it_checks_the_kernel_timelock() {
            let mut kernel = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::empty());
            kernel.lock_height = 2;
            assert!(matches!(
                check_kernel_lock_height(1, &[kernel.clone()]),
                Err(ValidationError::MaturityError)
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

    #[tokio::test]
    async fn check_burned_succeeds_for_valid_outputs() {
        let mut kernel1 = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::create_burn());
        let mut kernel2 = test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::create_burn());

        let key_manager = create_test_core_key_manager_with_memory_db();
        let (output1, _, _) = test_helpers::create_utxo(
            100.into(),
            &key_manager,
            &OutputFeatures::create_burn_output(),
            &script!(Nop),
            &Covenant::default(),
            0.into(),
        )
        .await;
        let (output2, _, _) = test_helpers::create_utxo(
            101.into(),
            &key_manager,
            &OutputFeatures::create_burn_output(),
            &script!(Nop),
            &Covenant::default(),
            0.into(),
        )
        .await;
        let (output3, _, _) = test_helpers::create_utxo(
            102.into(),
            &key_manager,
            &OutputFeatures::create_burn_output(),
            &script!(Nop),
            &Covenant::default(),
            0.into(),
        )
        .await;

        kernel1.burn_commitment = Some(output1.commitment.clone());
        kernel2.burn_commitment = Some(output2.commitment.clone());
        let kernel3 = kernel1.clone();

        let mut body = AggregateBody::new(Vec::new(), vec![output1.clone(), output2.clone()], vec![
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
        let body2 = AggregateBody::new(Vec::new(), vec![output1, output2, output3], vec![kernel1, kernel2]);
        assert!(check_total_burned(&body2).is_err());
    }

    mod transaction_ordering {
        use super::*;

        #[tokio::test]
        async fn it_rejects_unordered_bodies() {
            let mut kernels =
                iter::repeat_with(|| test_helpers::create_test_kernel(0.into(), 0, KernelFeatures::default()))
                    .take(10)
                    .collect::<Vec<_>>();

            // Sort the kernels, we'll check that the outputs fail the sorting check
            kernels.sort();

            let key_manager = create_test_core_key_manager_with_memory_db();
            let mut outputs = futures::stream::unfold((), |_| async {
                let (o, _, _) = test_helpers::create_utxo(
                    100.into(),
                    &key_manager,
                    &OutputFeatures::create_burn_output(),
                    &script!(Nop),
                    &Covenant::default(),
                    0.into(),
                )
                .await;
                Some((o, ()))
            })
            .take(10)
            .collect::<Vec<_>>()
            .await;

            while is_all_unique_and_sorted(&outputs) {
                // Shuffle the outputs until they are not sorted
                outputs.shuffle(&mut rand::thread_rng());
            }

            // Break the contract of new_unsorted_unchecked by calling it with unsorted outputs. The validator must not
            // rely on the sorted flag.
            let body = AggregateBody::new_sorted_unchecked(Vec::new(), outputs, kernels);
            let err = AggregateBodyInternalConsistencyValidator::new(
                true,
                ConsensusManager::builder(Network::LocalNet).build().unwrap(),
                CryptoFactories::new(RANGE_PROOF_AGGREGATION_FACTOR),
            )
            .validate(&body, &Default::default(), &Default::default(), None, None, u64::MAX)
            .unwrap_err();

            assert!(matches!(err, ValidationError::UnsortedOrDuplicateOutput));
        }
    }
}
