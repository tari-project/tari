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
    fmt::{Display, Error, Formatter},
};

use borsh::{BorshDeserialize, BorshSerialize};
use log::*;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, PrivateKey};
use tari_crypto::commitment::HomomorphicCommitmentFactory;

use crate::transactions::{
    crypto_factories::CryptoFactories,
    tari_amount::MicroMinotari,
    transaction_components::{
        KernelFeatures,
        OutputType,
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
/// blocks consist of inputs, outputs and kernels, rather than transactions.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AggregateBody {
    /// This flag indicates if the inputs, outputs and kernels have been sorted internally, that is, the sort() method
    /// has been called. This may be false even if all components are sorted.
    #[borsh(skip)]
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

    /// Provide read-only access to the kernel list
    pub fn kernels(&self) -> &Vec<TransactionKernel> {
        &self.kernels
    }

    /// Add an input to the existing aggregate body
    pub fn add_input(&mut self, input: TransactionInput) {
        self.inputs.push(input);
        self.sorted = false;
    }

    /// Add a series of inputs to the existing aggregate body
    pub fn add_inputs<I: IntoIterator<Item = TransactionInput>>(&mut self, inputs: I) {
        self.inputs.extend(inputs);
        self.sorted = false;
    }

    /// Add an output to the existing aggregate body
    pub fn add_output(&mut self, output: TransactionOutput) {
        self.outputs.push(output);
        self.sorted = false;
    }

    /// Add a series of outputs to the existing aggregate body
    pub fn add_outputs<I: IntoIterator<Item = TransactionOutput>>(&mut self, outputs: I) {
        self.outputs.extend(outputs);
        self.sorted = false;
    }

    /// Add a kernel to the existing aggregate body
    pub fn add_kernel(&mut self, kernel: TransactionKernel) {
        self.kernels.push(kernel);
        self.sorted = false;
    }

    /// Add a series of kernels to the existing aggregate body
    pub fn add_kernels<I: IntoIterator<Item = TransactionKernel>>(&mut self, new_kernels: I) {
        self.kernels.extend(new_kernels);
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
        for kernel in &self.kernels {
            kernel.verify_signature().map_err(|e| {
                warn!(target: LOG_TARGET, "Kernel ({}) signature failed {:?}.", kernel, e);
                e
            })?;
        }
        Ok(())
    }

    pub fn get_total_fee(&self) -> Result<MicroMinotari, TransactionError> {
        let mut fee = MicroMinotari::from(0);
        for kernel in &self.kernels {
            fee = fee.checked_add(kernel.fee).ok_or(TransactionError::InvalidKernel(
                "Aggregated body has greater fee than u64::MAX".to_string(),
            ))?;
        }
        Ok(fee)
    }

    /// Run through the outputs of the block and check that
    /// 1. There is exactly ONE coinbase output
    /// 1. The coinbase output's maturity is correctly set
    /// 1. The reward amount is correct.
    pub fn check_coinbase_output(
        &self,
        reward: MicroMinotari,
        coinbase_min_maturity: u64,
        factories: &CryptoFactories,
        height: u64,
    ) -> Result<(), TransactionError> {
        let mut coinbase_utxo_sum = Commitment::default();
        let mut coinbase_kernel = None;
        let mut coinbase_counter = 0;
        for utxo in self.outputs() {
            if utxo.features.output_type == OutputType::Coinbase {
                coinbase_counter += 1;
                if utxo.features.maturity < (height + coinbase_min_maturity) {
                    warn!(target: LOG_TARGET, "Coinbase {} found with maturity set too low", utxo);
                    return Err(TransactionError::InvalidCoinbaseMaturity);
                }
                coinbase_utxo_sum = &coinbase_utxo_sum + &utxo.commitment;
            }
        }

        if coinbase_counter == 0 {
            return Err(TransactionError::NoCoinbase);
        }

        debug!(
            target: LOG_TARGET,
            "{} coinbases found in body.", coinbase_counter,
        );

        let mut coinbase_kernel_counter = 0; // there should be exactly 1 coinbase kernel as well
        for kernel in self.kernels() {
            if kernel.features.contains(KernelFeatures::COINBASE_KERNEL) {
                coinbase_kernel_counter += 1;
                coinbase_kernel = Some(kernel);
            }
        }
        if coinbase_kernel.is_none() || coinbase_kernel_counter != 1 {
            warn!(
                target: LOG_TARGET,
                "{} coinbase kernels found in body. Only a single coinbase kernel is permitted.", coinbase_counter,
            );
            return Err(TransactionError::MoreThanOneCoinbaseKernel);
        }

        let coinbase_kernel = coinbase_kernel.expect("coinbase_kernel: none checked");

        let rhs = &coinbase_kernel.excess + &factories.commitment.commit_value(&PrivateKey::default(), reward.0);
        if rhs != coinbase_utxo_sum {
            warn!(
                target: LOG_TARGET,
                "Coinbase amount validation failed"
            );
            return Err(TransactionError::InvalidCoinbase);
        }
        Ok(())
    }

    pub fn check_output_features(&self, max_coinbase_metadata_size: u32) -> Result<(), TransactionError> {
        for output in self.outputs() {
            if !output.is_coinbase() && !output.features.coinbase_extra.is_empty() {
                return Err(TransactionError::NonCoinbaseHasOutputFeaturesCoinbaseExtra);
            }

            if output.is_coinbase() && output.features.coinbase_extra.len() > max_coinbase_metadata_size as usize {
                return Err(TransactionError::InvalidOutputFeaturesCoinbaseExtraSize {
                    len: output.features.coinbase_extra.len(),
                    max: max_coinbase_metadata_size,
                });
            }
        }

        Ok(())
    }

    /// This function will check all UTXO to ensure that feature flags where followed
    pub fn check_utxo_rules(&self, height: u64) -> Result<(), TransactionError> {
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

    pub fn dissolve(self) -> (Vec<TransactionInput>, Vec<TransactionOutput>, Vec<TransactionKernel>) {
        (self.inputs, self.outputs, self.kernels)
    }

    /// Returns the weight in grams of a body
    pub fn calculate_weight(&self, transaction_weight: &TransactionWeight) -> Result<u64, TransactionError> {
        transaction_weight
            .calculate_body(self)
            .map_err(|e| TransactionError::SerializationError(e.to_string()))
    }

    pub fn sum_features_and_scripts_size(&self) -> std::io::Result<usize> {
        Ok(self
            .outputs
            .iter()
            .map(|o| o.get_features_and_scripts_size())
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum())
    }

    pub fn is_sorted(&self) -> bool {
        // a block containing only a single kernel, single output and single input is sorted by default
        self.sorted || (self.kernels.len() <= 1 && self.outputs.len() <= 1 && self.inputs.len() <= 1)
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

    /// Returns the maximum maturity of the input UTXOs.
    /// This function panics if any of the inputs are compact.
    pub fn max_input_maturity(&self) -> Result<u64, TransactionError> {
        self.inputs()
            .iter()
            .map(|i| i.features())
            .try_fold(0, |max_maturity, features| Ok(max(max_maturity, features?.maturity)))
    }

    pub fn max_kernel_timelock(&self) -> u64 {
        self.kernels()
            .iter()
            .fold(0, |max_timelock, kernel| max(max_timelock, kernel.lock_height))
    }

    /// Returns the height of the minimum height where the body is spendable. This is calculated from the
    /// kernel lock_heights and the maturity of the input UTXOs.
    pub fn min_spendable_height(&self) -> Result<u64, TransactionError> {
        Ok(max(self.max_kernel_timelock(), self.max_input_maturity()?))
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
        for input in &self.inputs {
            writeln!(fmt, "{}", input)?;
        }
        writeln!(fmt, "--- Outputs ({}) ---", self.outputs.len())?;
        for output in &self.outputs {
            writeln!(fmt, "{}", output)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::types::{ComAndPubSignature, Commitment, FixedHash, PublicKey, Signature};
    use tari_script::{ExecutionStack, TariScript};

    use super::*;
    use crate::{
        covenants::Covenant,
        transactions::transaction_components::{EncryptedData, OutputFeatures, TransactionInputVersion},
    };

    #[test]
    fn test_sorted() {
        let mut body = AggregateBody::empty();
        assert!(body.is_sorted());
        let kernel = TransactionKernel::new_current_version(
            KernelFeatures::default(),
            0.into(),
            0,
            Commitment::default(),
            Signature::default(),
            None,
        );
        let output = TransactionOutput::default();
        let input = TransactionInput::new_with_output_data(
            TransactionInputVersion::get_current_version(),
            OutputFeatures::default(),
            Commitment::default(),
            TariScript::default(),
            ExecutionStack::default(),
            ComAndPubSignature::default(),
            PublicKey::default(),
            Covenant::default(),
            EncryptedData::default(),
            ComAndPubSignature::default(),
            FixedHash::zero(),
            0.into(),
        );

        body.add_kernel(kernel.clone());
        assert!(body.is_sorted());
        assert!(!body.sorted);

        body.add_input(input.clone());
        assert!(body.is_sorted());
        assert!(!body.sorted);

        body.add_output(output.clone());
        assert!(body.is_sorted());
        assert!(!body.sorted);
        body.sort();
        assert!(body.sorted);

        let mut body2 = body.clone();
        body2.add_kernel(kernel);
        assert!(!body2.is_sorted());

        let mut body3 = body.clone();
        body3.add_input(input);
        assert!(!body3.is_sorted());

        body.add_output(output);
        assert!(!body.is_sorted())
    }
}
