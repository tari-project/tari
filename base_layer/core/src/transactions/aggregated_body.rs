// Copyright 2019, The Tari Project
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
use std::{
    cmp::max,
    convert::TryInto,
    fmt::{Display, Error, Formatter},
};

use log::*;
use serde::{Deserialize, Serialize};
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
    script::ScriptContext,
    tari_utilities::hex::Hex,
};

use crate::transactions::{
    crypto_factories::CryptoFactories,
    tari_amount::MicroTari,
    transaction_components::{
        KernelFeatures,
        KernelSum,
        OutputFlags,
        Transaction,
        TransactionError,
        TransactionInput,
        TransactionKernel,
        TransactionOutput,
    },
    weight::TransactionWeight,
};

pub const LOG_TARGET: &str = "c::tx::aggregated_body";

/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregateBody {
    sorted: bool,
    /// List of inputs spent by the transaction.
    inputs: Vec<TransactionInput>,
    /// List of outputs the transaction produces.
    outputs: Vec<TransactionOutput>,
    /// Kernels contain the excesses and their signatures for transaction
    kernels: Vec<TransactionKernel>,
}

impl AggregateBody {
    /// Create an empty aggregate body
    pub fn empty() -> AggregateBody {
        // UNCHECKED: empty vecs are sorted
        AggregateBody::new_sorted_unchecked(vec![], vec![], vec![])
    }

    /// Create a new aggregate body from provided inputs, outputs and kernels
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
    ) -> AggregateBody {
        AggregateBody {
            sorted: false,
            inputs,
            outputs,
            kernels,
        }
    }

    /// Create a new aggregate body from provided inputs, outputs and kernels.
    /// It is up to the caller to ensure that the inputs, outputs and kernels are sorted
    pub(crate) fn new_sorted_unchecked(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
    ) -> AggregateBody {
        AggregateBody {
            sorted: true,
            inputs,
            outputs,
            kernels,
        }
    }

    /// Provide read-only access to the input list
    pub fn inputs(&self) -> &Vec<TransactionInput> {
        &self.inputs
    }

    /// Provide read-only access to the output list
    pub fn outputs(&self) -> &Vec<TransactionOutput> {
        &self.outputs
    }

    /// Should be used for tests only. Get a mutable reference to the outputs
    pub fn outputs_mut(&mut self) -> &mut Vec<TransactionOutput> {
        &mut self.outputs
    }

    /// Provide read-only access to the kernel list
    pub fn kernels(&self) -> &Vec<TransactionKernel> {
        &self.kernels
    }

    /// Should be used for tests only. Get a mutable reference to the inputs
    pub fn inputs_mut(&mut self) -> &mut Vec<TransactionInput> {
        &mut self.inputs
    }

    /// Add an input to the existing aggregate body
    pub fn add_input(&mut self, input: TransactionInput) {
        self.inputs.push(input);
        self.sorted = false;
    }

    /// Add a series of inputs to the existing aggregate body
    pub fn add_inputs(&mut self, inputs: &mut Vec<TransactionInput>) {
        self.inputs.append(inputs);
        self.sorted = false;
    }

    /// Add an output to the existing aggregate body
    pub fn add_output(&mut self, output: TransactionOutput) {
        self.outputs.push(output);
        self.sorted = false;
    }

    /// Add an output to the existing aggregate body
    pub fn add_outputs(&mut self, outputs: &mut Vec<TransactionOutput>) {
        self.outputs.append(outputs);
        self.sorted = false;
    }

    /// Add a kernel to the existing aggregate body
    pub fn add_kernel(&mut self, kernel: TransactionKernel) {
        self.kernels.push(kernel);
    }

    /// Add a kernels to the existing aggregate body
    pub fn add_kernels(&mut self, new_kernels: &mut Vec<TransactionKernel>) {
        self.kernels.append(new_kernels);
        self.sorted = false;
    }

    /// Set the kernel of the aggregate body, replacing any previous kernels
    pub fn set_kernel(&mut self, kernel: TransactionKernel) {
        self.kernels = vec![kernel];
    }

    pub fn contains_duplicated_inputs(&self) -> bool {
        // If the body is sorted, can do a linear check instead of n^2
        if self.sorted {
            for i in 1..self.inputs().len() {
                if self.inputs()[i] == self.inputs()[i - 1] {
                    return true;
                }
            }
            return false;
        }
        for i in 0..self.inputs().len() {
            for j in (i + 1)..self.inputs().len() {
                if self.inputs()[i] == self.inputs()[j] {
                    return true;
                }
            }
        }
        false
    }

    pub fn contains_duplicated_outputs(&self) -> bool {
        // If the body is sorted, can do a linear check instead of n^2
        if self.sorted {
            for i in 1..self.outputs().len() {
                if self.outputs()[i] == self.outputs()[i - 1] {
                    return true;
                }
            }
            return false;
        }
        for i in 0..self.outputs().len() {
            for j in (i + 1)..self.outputs().len() {
                if self.outputs()[i] == self.outputs()[j] {
                    return true;
                }
            }
        }
        false
    }

    /// Sort the component lists of the aggregate body
    pub fn sort(&mut self) {
        if self.sorted {
            return;
        }
        self.inputs.sort();
        self.outputs.sort();
        self.kernels.sort();
        self.sorted = true;
    }

    /// Verify the signatures in all kernels contained in this aggregate body. Clients must provide an offset that
    /// will be added to the public key used in the signature verification.
    pub fn verify_kernel_signatures(&self) -> Result<(), TransactionError> {
        trace!(target: LOG_TARGET, "Checking kernel signatures",);
        for kernel in self.kernels.iter() {
            kernel.verify_signature().map_err(|e| {
                warn!(target: LOG_TARGET, "Kernel ({}) signature failed {:?}.", kernel, e);
                e
            })?;
        }
        Ok(())
    }

    pub fn get_total_fee(&self) -> MicroTari {
        let mut fee = MicroTari::from(0);
        for kernel in &self.kernels {
            fee += kernel.fee;
        }
        fee
    }

    /// This function will check spent kernel rules like tx lock height etc
    pub fn check_kernel_rules(&self, height: u64) -> Result<(), TransactionError> {
        for kernel in self.kernels() {
            if kernel.lock_height > height {
                warn!(target: LOG_TARGET, "Kernel lock height was not reached: {}", kernel);
                return Err(TransactionError::InvalidKernel);
            }
        }
        Ok(())
    }

    /// Run through the outputs of the block and check that
    /// 1. There is exactly ONE coinbase output
    /// 1. The output's maturity is correctly set
    /// 1. The amount is correct.
    pub fn check_coinbase_output(
        &self,
        reward: MicroTari,
        coinbase_lock_height: u64,
        factories: &CryptoFactories,
        height: u64,
    ) -> Result<(), TransactionError> {
        let mut coinbase_utxo = None;
        let mut coinbase_kernel = None;
        let mut coinbase_counter = 0; // there should be exactly 1 coinbase
        for utxo in self.outputs() {
            if utxo.features.flags.contains(OutputFlags::COINBASE_OUTPUT) {
                coinbase_counter += 1;
                if utxo.features.maturity < (height + coinbase_lock_height) {
                    warn!(target: LOG_TARGET, "Coinbase {} found with maturity set too low", utxo);
                    return Err(TransactionError::InvalidCoinbaseMaturity);
                }
                coinbase_utxo = Some(utxo.clone());
            }
        }
        if coinbase_counter != 1 {
            warn!(
                target: LOG_TARGET,
                "{} coinbases found in body. Only a single coinbase is permitted.", coinbase_counter,
            );
            return Err(TransactionError::MoreThanOneCoinbase);
        }

        let mut coinbase_counter = 0; // there should be exactly 1 coinbase kernel as well
        for kernel in self.kernels() {
            if kernel.features.contains(KernelFeatures::COINBASE_KERNEL) {
                coinbase_counter += 1;
                coinbase_kernel = Some(kernel.clone());
            }
        }
        if coinbase_counter != 1 {
            warn!(
                target: LOG_TARGET,
                "{} coinbase kernels found in body. Only a single coinbase kernel is permitted.", coinbase_counter,
            );
            return Err(TransactionError::MoreThanOneCoinbase);
        }
        // Unwrap used here are fine as they should have an amount in them by here. If the coinbase's are missing the
        // counters should be 0 and the fn should have returned an error by now.
        let utxo = coinbase_utxo.unwrap();
        let rhs =
            &coinbase_kernel.unwrap().excess + &factories.commitment.commit_value(&BlindingFactor::default(), reward.0);
        if rhs != utxo.commitment {
            warn!(target: LOG_TARGET, "Coinbase {} amount validation failed", utxo);
            return Err(TransactionError::InvalidCoinbase);
        }
        Ok(())
    }

    /// This function will check all stxo to ensure that feature flags where followed
    pub fn check_stxo_rules(&self, height: u64) -> Result<(), TransactionError> {
        for input in self.inputs() {
            if input.features()?.maturity > height {
                warn!(
                    target: LOG_TARGET,
                    "Input found that has not yet matured to spending height: {}", input
                );
                return Err(TransactionError::InputMaturity);
            }
        }
        Ok(())
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    /// The reward is the total amount of Tari rewarded for this block (block reward + total fees), this should be 0
    /// for a transaction

    pub fn validate_internal_consistency(
        &self,
        tx_offset: &BlindingFactor,
        script_offset: &BlindingFactor,
        bypass_range_proof_verification: bool,
        total_reward: MicroTari,
        factories: &CryptoFactories,
        prev_header: Option<HashOutput>,
        height: u64,
    ) -> Result<(), TransactionError> {
        self.verify_kernel_signatures()?;

        let total_offset = factories.commitment.commit_value(tx_offset, total_reward.0);
        self.validate_kernel_sum(total_offset, &factories.commitment)?;

        if !bypass_range_proof_verification {
            self.validate_range_proofs(&factories.range_proof)?;
        }
        self.verify_metadata_signatures()?;

        let script_offset_g = PublicKey::from_secret_key(script_offset);
        self.validate_script_offset(script_offset_g, &factories.commitment, prev_header, height)?;
        self.validate_covenants(height)?;
        Ok(())
    }

    pub fn dissolve(self) -> (Vec<TransactionInput>, Vec<TransactionOutput>, Vec<TransactionKernel>) {
        (self.inputs, self.outputs, self.kernels)
    }

    /// Calculate the sum of the outputs - inputs
    fn sum_commitments(&self) -> Result<Commitment, TransactionError> {
        let sum_inputs = &self
            .inputs
            .iter()
            .map(|i| i.commitment())
            .collect::<Result<Vec<&Commitment>, _>>()?
            .into_iter()
            .sum::<Commitment>();
        let sum_outputs = &self.outputs.iter().map(|o| &o.commitment).sum::<Commitment>();
        Ok(sum_outputs - sum_inputs)
    }

    /// Calculate the sum of the kernels, taking into account the provided offset, and their constituent fees
    fn sum_kernels(&self, offset_with_fee: PedersenCommitment) -> KernelSum {
        // Sum all kernel excesses and fees
        self.kernels.iter().fold(
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

    /// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
    ///
    /// The offset_and_reward commitment includes the offset & the total coinbase reward (block reward + fees for
    /// block balances, or zero for transaction balances)
    fn validate_kernel_sum(
        &self,
        offset_and_reward: Commitment,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError> {
        trace!(target: LOG_TARGET, "Checking kernel total");
        let KernelSum { sum: excess, fees } = self.sum_kernels(offset_and_reward);
        let sum_io = self.sum_commitments()?;
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

    /// this will validate the script offset of the aggregate body.
    fn validate_script_offset(
        &self,
        script_offset: PublicKey,
        factory: &CommitmentFactory,
        prev_header: Option<HashOutput>,
        height: u64,
    ) -> Result<(), TransactionError> {
        trace!(target: LOG_TARGET, "Checking script offset");
        // lets count up the input script public keys
        let mut input_keys = PublicKey::default();
        let prev_hash: [u8; 32] = prev_header.unwrap_or_default().as_slice().try_into().unwrap_or([0; 32]);
        for input in &self.inputs {
            let context = ScriptContext::new(height, &prev_hash, input.commitment()?);
            input_keys = input_keys + input.run_and_verify_script(factory, Some(context))?;
        }

        // Now lets gather the output public keys and hashes.
        let mut output_keys = PublicKey::default();
        for output in &self.outputs {
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

    fn validate_covenants(&self, height: u64) -> Result<(), TransactionError> {
        for input in self.inputs.iter() {
            input.covenant()?.execute(height, input, &self.outputs)?;
        }
        Ok(())
    }

    fn validate_range_proofs(&self, range_proof_service: &RangeProofService) -> Result<(), TransactionError> {
        trace!(target: LOG_TARGET, "Checking range proofs");
        for o in &self.outputs {
            o.verify_range_proof(range_proof_service)?;
        }
        Ok(())
    }

    fn verify_metadata_signatures(&self) -> Result<(), TransactionError> {
        trace!(target: LOG_TARGET, "Checking sender signatures");
        for o in &self.outputs {
            o.verify_metadata_signature()?;
        }
        Ok(())
    }

    /// Returns the weight in grams of a body
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> u64 {
        transaction_weight.calculate_body(self)
    }

    pub fn sum_metadata_size(&self) -> usize {
        self.outputs.iter().map(|o| o.get_metadata_size()).sum()
    }

    pub fn is_sorted(&self) -> bool {
        self.sorted
    }

    /// Lists the number of inputs, outputs, and kernels in the block
    pub fn to_counts_string(&self) -> String {
        format!(
            "{} input(s), {} output(s), {} kernel(s)",
            self.inputs.len(),
            self.outputs.len(),
            self.kernels.len()
        )
    }

    pub fn max_kernel_timelock(&self) -> u64 {
        self.kernels()
            .iter()
            .fold(0, |max_timelock, kernel| max(max_timelock, kernel.lock_height))
    }

    /// Return a cloned version of self with TransactionInputs in their compact form
    pub fn to_compact(&self) -> Self {
        Self {
            sorted: self.sorted,
            inputs: self.inputs.iter().map(|i| i.to_compact()).collect(),
            outputs: self.outputs.clone(),
            kernels: self.kernels.clone(),
        }
    }
}

impl PartialEq for AggregateBody {
    fn eq(&self, other: &Self) -> bool {
        self.kernels == other.kernels && self.inputs == other.inputs && self.outputs == other.outputs
    }
}

impl Eq for AggregateBody {}

/// This will strip away the offset of the transaction returning a pure aggregate body
impl From<Transaction> for AggregateBody {
    fn from(transaction: Transaction) -> Self {
        transaction.body
    }
}

impl Display for AggregateBody {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        if !self.is_sorted() {
            writeln!(fmt, "WARNING: Block body is not sorted.")?;
        }
        writeln!(fmt, "--- Transaction Kernels ---")?;
        for (i, kernel) in self.kernels.iter().enumerate() {
            writeln!(fmt, "Kernel {}:", i)?;
            writeln!(fmt, "{}", kernel)?;
        }
        writeln!(fmt, "--- Inputs ({}) ---", self.inputs.len())?;
        for input in self.inputs.iter() {
            writeln!(fmt, "{}", input)?;
        }
        writeln!(fmt, "--- Outputs ({}) ---", self.outputs.len())?;
        for output in self.outputs.iter() {
            writeln!(fmt, "{}", output)?;
        }
        Ok(())
    }
}
