// Copyright 2019. The Tari Project
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

use crate::{
    blocks::block::KernelSum,
    tari_amount::*,
    transaction::*,
    types::{BlindingFactor, Commitment, CommitmentFactory, PrivateKey, RangeProofService, COMMITMENT_FACTORY},
};
use serde::{Deserialize, Serialize};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, ristretto::pedersen::PedersenCommitment};

/// The components of the block or transaction. The same struct can be used for either, since in Mimblewimble,
/// cut-through means that blocks and transactions have the same structure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AggregateBody {
    sorted: bool,
    /// List of inputs spent by the transaction.
    pub inputs: Vec<TransactionInput>,
    /// List of outputs the transaction produces.
    pub outputs: Vec<TransactionOutput>,
    /// Kernels contain the excesses and their signatures for transaction
    pub kernels: Vec<TransactionKernel>,
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

    /// Set the kernel of the aggregate body, replacing any previous kernels
    pub fn set_kernel(&mut self, kernel: TransactionKernel) {
        self.kernels = vec![kernel];
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
        for kernel in self.kernels.iter() {
            kernel.verify_signature()?;
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
    /// The reward is the amount of Tari rewarded for this block, this should be 0 for a transaction
    pub fn validate_internal_consistency(
        &self,
        offset: &BlindingFactor,
        reward: MicroTari,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError>
    {
        let total_offset = COMMITMENT_FACTORY.commit_value(&offset, reward.0);
        self.verify_kernel_signatures()?;
        self.validate_kernel_sum(total_offset, factory)?;
        self.validate_range_proofs(prover)
    }

    /// Calculate the sum of the inputs and outputs including fees
    fn sum_commitments(&self, fees: u64, factory: &CommitmentFactory) -> Commitment {
        let fee_commitment = factory.commit_value(&PrivateKey::default(), fees);
        let sum_inputs = &self.inputs.iter().map(|i| &i.commitment).sum::<Commitment>();
        let sum_outputs = &self.outputs.iter().map(|o| &o.commitment).sum::<Commitment>();
        &(sum_outputs - sum_inputs) + &fee_commitment
    }

    /// Calculate the sum of the kernels, taking into account the provided offset, and their constituent fees
    fn sum_kernels(&self, offset: PedersenCommitment) -> KernelSum {
        // Sum all kernel excesses and fees
        self.kernels.iter().fold(
            KernelSum {
                fees: MicroTari(0),
                sum: offset,
            },
            |acc, val| KernelSum {
                fees: &acc.fees + &val.fee,
                sum: &acc.sum + &val.excess,
            },
        )
    }

    /// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
    fn validate_kernel_sum(
        &self,
        offset: PedersenCommitment,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError>
    {
        let kernel_sum = self.sum_kernels(offset);
        let sum_io = self.sum_commitments(kernel_sum.fees.into(), factory);

        if kernel_sum.sum != sum_io {
            return Err(TransactionError::ValidationError(
                "Sum of inputs and outputs did not equal sum of kernels with fees".into(),
            ));
        }

        Ok(())
    }

    fn validate_range_proofs(&self, range_proof_service: &RangeProofService) -> Result<(), TransactionError> {
        for o in &self.outputs {
            if !o.verify_range_proof(&range_proof_service)? {
                return Err(TransactionError::ValidationError(
                    "Range proof could not be verified".into(),
                ));
            }
        }
        Ok(())
    }
}

/// This will strip away the offset of the transaction returning a pure aggregate body
impl From<Transaction> for AggregateBody {
    fn from(transaction: Transaction) -> Self {
        transaction.body
    }
}
