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

use crate::transactions::{
    fee::Fee,
    tari_amount::*,
    transaction::*,
    types::{BlindingFactor, Commitment, CommitmentFactory, CryptoFactories, PrivateKey, RangeProofService},
};
use log::*;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Error, Formatter};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, ristretto::pedersen::PedersenCommitment};
pub const LOG_TARGET: &str = "c::tx::aggregated_body";

/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
        AggregateBody {
            sorted: false,
            inputs: vec![],
            outputs: vec![],
            kernels: vec![],
        }
    }

    /// Create a new aggregate body from provided inputs, outputs and kernels
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
    ) -> AggregateBody
    {
        AggregateBody {
            sorted: false,
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

    /// This will perform cut-through on the aggregate body. It will remove all outputs (and inputs) that are being
    /// spent as inputs.
    pub fn do_cut_through(&mut self) {
        let double_inputs: Vec<TransactionInput> = self
            .inputs
            .iter()
            .filter(|input| self.outputs.iter().any(|o| o.is_equal_to(input)))
            .cloned()
            .collect();

        for input in double_inputs {
            trace!(
                target: LOG_TARGET,
                "removing following utxo for cut-through: {:?}",
                input
            );
            self.outputs.retain(|x| !input.is_equal_to(x));
            self.inputs.retain(|x| *x != input);
        }
    }

    /// This will perform a check that cut-through was performed on the aggregate body. It will return true if there are
    /// no outputs that are being spent as inputs.
    pub fn cut_through_check(&self) -> bool {
        !self
            .inputs
            .iter()
            .any(|input| self.outputs.iter().any(|o| o.is_equal_to(input)))
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
            kernel.verify_signature().or_else(|e| {
                warn!(target: LOG_TARGET, "Kernel ({}) signature failed {:?}.", kernel, e);
                Err(e)
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
        offset: &BlindingFactor,
        total_reward: MicroTari,
        factories: &CryptoFactories,
    ) -> Result<(), TransactionError>
    {
        let total_offset = factories.commitment.commit_value(&offset, total_reward.0);

        self.verify_kernel_signatures()?;
        self.validate_kernel_sum(total_offset, &factories.commitment)?;
        self.validate_range_proofs(&factories.range_proof)
    }

    pub fn dissolve(self) -> (Vec<TransactionInput>, Vec<TransactionOutput>, Vec<TransactionKernel>) {
        (self.inputs, self.outputs, self.kernels)
    }

    /// Calculate the sum of the outputs - inputs
    fn sum_commitments(&self) -> Commitment {
        let sum_inputs = &self.inputs.iter().map(|i| &i.commitment).sum::<Commitment>();
        let sum_outputs = &self.outputs.iter().map(|o| &o.commitment).sum::<Commitment>();
        sum_outputs - sum_inputs
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
    ) -> Result<(), TransactionError>
    {
        trace!(target: LOG_TARGET, "Checking kernel total");
        let KernelSum { sum: excess, fees } = self.sum_kernels(offset_and_reward);
        let sum_io = self.sum_commitments();
        let fees = factory.commit_value(&PrivateKey::default(), fees.into());
        if excess != &sum_io + &fees {
            return Err(TransactionError::ValidationError(
                "Sum of inputs and outputs did not equal sum of kernels with fees".into(),
            ));
        }

        Ok(())
    }

    fn validate_range_proofs(&self, range_proof_service: &RangeProofService) -> Result<(), TransactionError> {
        trace!(target: LOG_TARGET, "Checking range proofs");
        for o in &self.outputs {
            if !o.verify_range_proof(&range_proof_service)? {
                return Err(TransactionError::ValidationError(
                    "Range proof could not be verified".into(),
                ));
            }
        }
        Ok(())
    }

    /// Returns the byte size or weight of a body
    pub fn calculate_weight(&self) -> u64 {
        Fee::calculate_weight(self.kernels().len(), self.inputs().len(), self.outputs().len())
    }
}

/// This will strip away the offset of the transaction returning a pure aggregate body
impl From<Transaction> for AggregateBody {
    fn from(transaction: Transaction) -> Self {
        transaction.body
    }
}

impl Display for AggregateBody {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        if !self.sorted {
            fmt.write_str("WARNING: Block body is not sorted.\n")?;
        }
        fmt.write_str("--- Transaction Kernels ---\n")?;
        for (i, kernel) in self.kernels.iter().enumerate() {
            fmt.write_str(&format!("Kernel {}:\n", i))?;
            fmt.write_str(&format!("{}\n", kernel))?;
        }
        fmt.write_str(&format!("--- Inputs ({}) ---\n", self.inputs.len()))?;
        for input in self.inputs.iter() {
            fmt.write_str(&format!("{}", input))?;
        }
        fmt.write_str(&format!("--- Outputs ({}) ---\n", self.outputs.len()))?;
        for output in self.outputs.iter() {
            fmt.write_str(&format!("{}", output))?;
        }
        Ok(())
    }
}
