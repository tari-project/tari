// Copyright 2018 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

use crate::{
    block::AggregateBody,
    range_proof::RangeProof,
    types::{Base, BlindingFactor, Commitment, Signature},
};

use crypto::{
    commitment::HomomorphicCommitment,
    common::{Blake256, ByteArray},
};
use curve25519_dalek::scalar::Scalar;
use derive_error::Error;
use digest::Digest;
use std::cmp::Ordering;

bitflags! {
    /// Options for a kernel's structure or use.
    /// TODO:  expand to accommodate Tari DAN transaction types, such as namespace and validator node registrations
    pub struct KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
    }
}

bitflags! {
    pub struct OutputFeatures: u8 {
        /// Output is a coinbase output, must not be spent until maturity
        const COINBASE_OUTPUT = 0b00000001;
    }
}

#[derive(Debug, PartialEq, Error)]
pub enum TransactionError {
    // Error validating the transaction
    ValidationError,
}

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone)]
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for coinbase output.
    pub features: OutputFeatures,
    /// The commitment referencing the output being spent.
    pub commitment: Commitment,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    /// Create a new Transaction Input
    pub fn new(features: OutputFeatures, commitment: Commitment) -> TransactionInput {
        TransactionInput { features, commitment }
    }

    /// Accessor method for the commitment contained in an input
    pub fn commitment(&self) -> Commitment {
        self.commitment
    }
}

/// Implement the canonical hashing function for TransactionInput for use in ordering
impl Hashable for TransactionInput {
    type Hasher = Blake256;

    fn hash(&self) -> Vec<u8> {
        let mut hasher = Self::Hasher::new();
        hasher.input(vec![self.features.bits]);
        hasher.input(self.commitment.to_bytes());
        hasher.result().to_vec()
    }
}

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Copy, Clone)]
pub struct TransactionOutput {
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commitment: Commitment,
    /// A proof that the commitment is in the right range
    pub proof: RangeProof,
}

/// An output for a transaction, includes a rangeproof
impl TransactionOutput {
    /// Create new Transaction Output
    pub fn new(features: OutputFeatures, commitment: Commitment, proof: RangeProof) -> TransactionOutput {
        TransactionOutput { features, commitment, proof }
    }

    /// Accessor method for the commitment contained in an output
    pub fn commitment(&self) -> Commitment {
        self.commitment
    }

    /// Accessor method for the range proof contained in an output
    pub fn proof(&self) -> RangeProof {
        self.proof
    }
}

/// Implement the canonical hashing function for TransactionOutput for use in ordering
impl Hashable for TransactionOutput {
    type Hasher = Blake256;

    fn hash(&self) -> Vec<u8> {
        let mut hasher = Self::Hasher::new();
        hasher.input(vec![self.features.bits]);
        hasher.input(self.commitment.to_bytes());
        hasher.input(self.proof.0);
        hasher.result().to_vec()
    }
}

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone)]
pub struct TransactionKernel {
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: u64,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments. If the transaction
    /// is well formed, amounts components should sum to zero and the excess
    /// is hence a valid public key.
    pub excess: Option<Commitment>,
    /// The signature proving the excess is a valid public key, which signs
    /// the transaction fee.
    pub excess_sig: Option<Signature>,
}

/// Implementation of the transaction kernel
impl TransactionKernel {
    /// Creates an empty transaction kernel
    pub fn empty() -> TransactionKernel {
        TransactionKernel { features: KernelFeatures::empty(), fee: 0, lock_height: 0, excess: None, excess_sig: None }
    }

    /// Build a transaction kernel with the provided fee
    pub fn with_fee(mut self, fee: u64) -> TransactionKernel {
        self.fee = fee;
        self
    }

    /// Build a transaction kernel with the provided lock height
    pub fn with_lock_height(mut self, lock_height: u64) -> TransactionKernel {
        self.lock_height = lock_height;
        self
    }
}

/// Implement the canonical hashing function for TransactionKernel for use in ordering
impl Hashable for TransactionKernel {
    type Hasher = Blake256;

    fn hash(&self) -> Vec<u8> {
        let mut hasher = Self::Hasher::new();
        hasher.input(vec![self.features.bits]);
        hasher.input(self.fee.to_le_bytes());
        hasher.input(self.lock_height.to_le_bytes());
        if self.excess.is_some() {
            hasher.input(self.excess.unwrap().to_bytes());
        }
        if self.excess_sig.is_some() {
            hasher.input(self.excess_sig.unwrap().get_signature().to_bytes());
        }
        hasher.result().to_vec()
    }
}

/// A transaction which consists of a kernel offset and an aggregate body made up of inputs, outputs and kernels.
pub struct Transaction {
    /// This kernel offset will be accumulated when transactions are aggregated to prevent the "subset" problem where
    /// kernels can be linked to inputs and outputs by testing a series of subsets and see which produce valid
    /// transactions
    pub offset: BlindingFactor,
    /// The constituents of a transaction which has the same structure as the body of a block.
    pub body: AggregateBody,
    /// reference to the Base point used in the commitments in this transaction
    pub base: &'static Base,
}

impl Transaction {
    /// Create a new transaction from the provided inputs, outputs, kernels and offset
    pub fn new(
        base: &'static Base,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
        offset: BlindingFactor,
    ) -> Transaction
    {
        Transaction { base, offset, body: AggregateBody::new(inputs, outputs, kernels) }
    }

    /// Calculate the sum of the inputs and outputs including the fees
    pub fn sum_commitments(&self, fees: u64) -> Commitment {
        let fee_commitment = Commitment::new(&Scalar::zero(), &Scalar::from(fees), self.base);

        let outputs_minus_inputs =
            &self.body.outputs.iter().fold(Commitment::zero(self.base), |acc, val| &acc + &val.commitment) -
                &self.body.inputs.iter().fold(Commitment::zero(self.base), |acc, val| &acc + &val.commitment);

        &outputs_minus_inputs + &fee_commitment
    }

    /// Calculate the sum of the kernels, taking into account the offset if it exists, and their constituent fees
    pub fn sum_kernels(&self) -> KernelSum {
        // Sum all kernel excesses and fees
        let mut kernel_sum =
            self.body.kernels.iter().fold(KernelSum { fees: 0u64, sum: Commitment::zero(self.base) }, |acc, val| {
                KernelSum {
                    fees: &acc.fees + val.fee,
                    sum: &acc.sum + &val.excess.unwrap_or(Commitment::zero(self.base)),
                }
            });

        // Add the offset commitment
        kernel_sum.sum = kernel_sum.sum + Commitment::new(&self.offset.into(), &Scalar::zero(), self.base);

        kernel_sum
    }

    /// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
    pub fn validate_kernel_sum(&self) -> Result<(), TransactionError> {
        let kernel_sum = self.sum_kernels();
        let sum_io = self.sum_commitments(kernel_sum.fees);

        if kernel_sum.sum != sum_io {
            return Err(TransactionError::ValidationError);
        }

        Ok(())
    }
}

/// This struct holds the result of calculating the sum of the kernels in a Transaction
/// and returns the summed commitments and the total fees
pub struct KernelSum {
    pub sum: Commitment,
    pub fees: u64,
}
