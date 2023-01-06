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

use std::convert::TryInto;

use log::{trace, warn};
use tari_common_types::types::{
    BlindingFactor,
    Commitment,
    CommitmentFactory,
    HashOutput,
    PrivateKey,
    PublicKey,
    RangeProofService,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::PublicKey as PublicKeyTrait,
    ristretto::pedersen::PedersenCommitment,
};
use tari_script::ScriptContext;
use tari_utilities::hex::Hex;

use crate::transactions::{
    aggregated_body::AggregateBody,
    tari_amount::MicroTari,
    transaction_components::{transaction_output::batch_verify_range_proofs, KernelSum, TransactionError},
    CryptoFactories,
};

pub const LOG_TARGET: &str = "c::val::aggregate_body_internal_consistency_validator";

pub struct AggregateBodyInternalConsistencyValidator {
    bypass_range_proof_verification: bool,
    factories: CryptoFactories,
}

impl AggregateBodyInternalConsistencyValidator {
    pub fn new(bypass_range_proof_verification: bool, factories: CryptoFactories) -> Self {
        Self {
            bypass_range_proof_verification,
            factories,
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
    pub fn validate(
        &self,
        body: &AggregateBody,
        tx_offset: &BlindingFactor,
        script_offset: &BlindingFactor,
        total_reward: Option<MicroTari>,
        prev_header: Option<HashOutput>,
        height: u64,
    ) -> Result<(), TransactionError> {
        let total_reward = total_reward.unwrap_or(MicroTari::zero());

        verify_kernel_signatures(body)?;

        let total_offset = self.factories.commitment.commit_value(tx_offset, total_reward.0);
        validate_kernel_sum(body, total_offset, &self.factories.commitment)?;

        if !self.bypass_range_proof_verification {
            validate_range_proofs(body, &self.factories.range_proof)?;
        }
        verify_metadata_signatures(body)?;

        let script_offset_g = PublicKey::from_secret_key(script_offset);
        validate_script_offset(body, script_offset_g, &self.factories.commitment, prev_header, height)?;
        validate_covenants(body, height)?;
        Ok(())
    }
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
///
/// The offset_and_reward commitment includes the offset & the total coinbase reward (block reward + fees for
/// block balances, or zero for transaction balances)
fn validate_kernel_sum(
    body: &AggregateBody,
    offset_and_reward: Commitment,
    factory: &CommitmentFactory,
) -> Result<(), TransactionError> {
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

/// this will validate the script offset of the aggregate body.
fn validate_script_offset(
    body: &AggregateBody,
    script_offset: PublicKey,
    factory: &CommitmentFactory,
    prev_header: Option<HashOutput>,
    height: u64,
) -> Result<(), TransactionError> {
    trace!(target: LOG_TARGET, "Checking script offset");
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
        return Err(TransactionError::ScriptOffset);
    }
    Ok(())
}

fn validate_covenants(body: &AggregateBody, height: u64) -> Result<(), TransactionError> {
    for input in body.inputs() {
        input.covenant()?.execute(height, input, body.outputs())?;
    }
    Ok(())
}
