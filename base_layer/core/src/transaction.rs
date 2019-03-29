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
    types::{BlindingFactor, Commitment, CommitmentFactory, Signature},
};

use crate::types::SignatureHash;
use derive::HashableOrdering;
use derive_error::Error;
use digest::Digest;
use std::cmp::Ordering;
use tari_crypto::{
    challenge::Challenge,
    commitment::{HomomorphicCommitment, HomomorphicCommitmentFactory},
    common::Blake256,
    ristretto::RistrettoSecretKey,
};
use tari_utilities::{ByteArray, Hashable};

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

type Hasher = Blake256;

#[derive(Debug, PartialEq, Error)]
pub enum TransactionError {
    // Error validating the transaction
    ValidationError,
    // Signature could not be verified
    InvalidSignatureError,
    // Transaction kernel does not contain a signature
    NoSignatureError,
}

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, HashableOrdering)]
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
    fn hash(&self) -> Vec<u8> {
        let mut hasher = Hasher::new();
        hasher.input(vec![self.features.bits]);
        hasher.input(self.commitment.as_bytes());
        hasher.result().to_vec()
    }
}

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Copy, Clone, HashableOrdering)]
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
        TransactionOutput {
            features,
            commitment,
            proof,
        }
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
    fn hash(&self) -> Vec<u8> {
        let mut hasher = Hasher::new();
        hasher.input(vec![self.features.bits]);
        hasher.input(self.commitment.as_bytes());
        hasher.input(self.proof.0);
        hasher.result().to_vec()
    }
}

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, HashableOrdering)]
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
        TransactionKernel {
            features: KernelFeatures::empty(),
            fee: 0,
            lock_height: 0,
            excess: None,
            excess_sig: None,
        }
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

    pub fn verify_signature(&self) -> Result<(), TransactionError> {
        if self.excess.is_none() || self.excess_sig.is_none() {
            return Err(TransactionError::NoSignatureError);
        }

        let signature = self.excess_sig.unwrap();
        let excess = self.excess.unwrap();
        let excess = excess.as_public_key();
        let r = signature.get_public_nonce();
        let c = Challenge::<SignatureHash>::new()
            .concat(r.as_bytes())
            .concat(excess.clone().as_bytes())
            .concat(&self.fee.to_le_bytes())
            .concat(&self.lock_height.to_le_bytes());

        if signature.verify_challenge(excess, c) {
            return Ok(());
        } else {
            return Err(TransactionError::InvalidSignatureError);
        }
    }
}

/// Implement the canonical hashing function for TransactionKernel for use in ordering
impl Hashable for TransactionKernel {
    fn hash(&self) -> Vec<u8> {
        let mut hasher = Hasher::new();
        hasher.input(vec![self.features.bits]);
        hasher.input(self.fee.to_le_bytes());
        hasher.input(self.lock_height.to_le_bytes());
        if self.excess.is_some() {
            hasher.input(self.excess.unwrap().as_bytes());
        }
        if self.excess_sig.is_some() {
            hasher.input(self.excess_sig.unwrap().get_signature().as_bytes());
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
}

impl Transaction {
    /// Create a new transaction from the provided inputs, outputs, kernels and offset
    pub fn new(
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        kernels: Vec<TransactionKernel>,
        offset: BlindingFactor,
    ) -> Transaction
    {
        Transaction {
            offset,
            body: AggregateBody::new(inputs, outputs, kernels),
        }
    }

    /// Calculate the sum of the inputs and outputs including the fees
    fn sum_commitments(&self, fees: u64) -> Commitment {
        let fee_commitment = CommitmentFactory::create(&RistrettoSecretKey::default(), &RistrettoSecretKey::from(fees));

        let outputs_minus_inputs = &self
            .body
            .outputs
            .iter()
            .fold(CommitmentFactory::zero(), |acc, val| &acc + &val.commitment) -
            &self
                .body
                .inputs
                .iter()
                .fold(CommitmentFactory::zero(), |acc, val| &acc + &val.commitment);

        &outputs_minus_inputs + &fee_commitment
    }

    /// Calculate the sum of the kernels, taking into account the offset if it exists, and their constituent fees
    fn sum_kernels(&self) -> KernelSum {
        // Sum all kernel excesses and fees
        let mut kernel_sum = self.body.kernels.iter().fold(
            KernelSum {
                fees: 0u64,
                sum: CommitmentFactory::zero(),
            },
            |acc, val| KernelSum {
                fees: &acc.fees + val.fee,
                sum: &acc.sum + &val.excess.unwrap_or(CommitmentFactory::zero()),
            },
        );

        // Add the offset commitment
        kernel_sum.sum =
            kernel_sum.sum + CommitmentFactory::create(&self.offset.into(), &RistrettoSecretKey::default());

        kernel_sum
    }

    /// Confirm that the (sum of the outputs) - (sum of inputs) = Kernel excess
    fn validate_kernel_sum(&self) -> Result<(), TransactionError> {
        let kernel_sum = self.sum_kernels();
        let sum_io = self.sum_commitments(kernel_sum.fees);

        if kernel_sum.sum != sum_io {
            return Err(TransactionError::ValidationError);
        }

        Ok(())
    }

    /// Validate this transaction
    pub fn validate(&self) -> Result<(), TransactionError> {
        self.body.verify_kernel_signatures()?;
        self.validate_kernel_sum()?;
        Ok(())
    }
}

/// This struct holds the result of calculating the sum of the kernels in a Transaction
/// and returns the summed commitments and the total fees
pub struct KernelSum {
    pub sum: Commitment,
    pub fees: u64,
}

pub struct TransactionBuilder {
    body: AggregateBody,
    offset: Option<BlindingFactor>,
}

impl TransactionBuilder {
    /// Create an new empty TransactionBuilder
    pub fn new() -> Self {
        Self {
            offset: None,
            body: AggregateBody::empty(),
        }
    }

    /// Update the offset of an existing transaction
    pub fn add_offset(mut self, offset: BlindingFactor) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Add an input to an existing transaction
    pub fn add_input(mut self, input: TransactionInput) -> Self {
        self.body = self.body.add_input(input);
        self
    }

    /// Add an output to an existing transaction
    pub fn add_output(mut self, output: TransactionOutput) -> Self {
        self.body = self.body.add_output(output);
        self
    }

    /// Add a series of inputs to an existing transaction
    pub fn add_inputs(mut self, inputs: Vec<TransactionInput>) -> Self {
        self.body = self.body.add_inputs(inputs);
        self
    }

    /// Add a series of outputs to an existing transaction
    pub fn add_outputs(mut self, outputs: Vec<TransactionOutput>) -> Self {
        self.body = self.body.add_outputs(outputs);
        self
    }

    /// Set the kernel of a transaction. Currently only one kernel is allowed per transaction
    pub fn with_kernel(mut self, kernel: TransactionKernel) -> Self {
        self.body = self.body.set_kernel(kernel);
        self
    }

    pub fn build(&self) -> Result<Transaction, TransactionError> {
        if let Some(offset) = self.offset {
            let tx = Transaction::new(
                self.body.inputs.clone(),
                self.body.outputs.clone(),
                self.body.kernels.clone(),
                offset,
            );
            tx.validate()?;
            Ok(tx)
        } else {
            return Err(TransactionError::ValidationError);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        range_proof::RangeProof,
        transaction::{KernelFeatures, OutputFeatures, TransactionInput, TransactionKernel, TransactionOutput},
        types::{BlindingFactor, PublicKey},
    };
    use rand;
    use tari_crypto::{
        challenge::Challenge,
        common::Blake256,
        keys::{PublicKey as PublicKeyTrait, SecretKey},
    };
    use tari_utilities::ByteArray;

    #[test]
    fn build_transaction_test_and_validation() {
        let mut rng = rand::OsRng::new().unwrap();

        let input_secret_key = BlindingFactor::random(&mut rng);
        let input_secret_key2 = BlindingFactor::random(&mut rng);
        let change_secret_key = BlindingFactor::random(&mut rng);
        let receiver_secret_key = BlindingFactor::random(&mut rng);
        let receiver_secret_key2 = BlindingFactor::random(&mut rng);
        let receiver_full_secret_key = &receiver_secret_key + &receiver_secret_key2;

        let input = TransactionInput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&input_secret_key, &RistrettoSecretKey::from(12u64)),
        );

        let change_output = TransactionOutput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&change_secret_key, &RistrettoSecretKey::from(4u64)),
            RangeProof([0; 1]),
        );

        let output = TransactionOutput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&receiver_secret_key, &RistrettoSecretKey::from(7u64)),
            RangeProof([0; 1]),
        );

        let offset: BlindingFactor = BlindingFactor::random(&mut rng).into();
        let sender_private_nonce = BlindingFactor::random(&mut rng);
        let sender_public_nonce = PublicKey::from_secret_key(&sender_private_nonce);
        let fee = 1u64;
        let lock_height = 0u64;

        // Create a transaction
        let tx_builder = TransactionBuilder::new()
            .add_input(input.clone())
            .add_output(output)
            .add_output(change_output)
            .add_offset(offset.clone());

        // Test adding inputs and outputs in vector form
        let input2 = TransactionInput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&input_secret_key2, &RistrettoSecretKey::from(2u64)),
        );
        let output2 = TransactionOutput::new(
            OutputFeatures::empty(),
            CommitmentFactory::create(&receiver_secret_key2, &RistrettoSecretKey::from(2u64)),
            RangeProof([0; 1]),
        );

        let tx_builder = tx_builder
            .add_inputs(vec![input2.clone()])
            .add_outputs(vec![output2.clone()]);

        // Should fail the validation because there is no kernel yet.
        let tx = tx_builder.build();
        assert!(tx.is_err());

        // Calculate Excess
        let mut sender_excess_key = &change_secret_key - &input_secret_key;
        sender_excess_key = &sender_excess_key - &input_secret_key2;
        sender_excess_key = &sender_excess_key - &offset;

        let sender_public_excess = PublicKey::from_secret_key(&sender_excess_key);
        // Receiver generate partial signatures

        let mut final_excess = &output.commitment + &change_output.commitment;
        let zero = RistrettoSecretKey::default();
        final_excess = &final_excess + &output2.commitment;
        final_excess = &final_excess - &input.commitment;
        final_excess = &final_excess - &input2.commitment;
        final_excess = &final_excess + &CommitmentFactory::create(&zero, &RistrettoSecretKey::from(fee)); // add fee
        final_excess = &final_excess - &CommitmentFactory::create(&offset, &zero); // subtract Offset

        let receiver_private_nonce = BlindingFactor::random(&mut rng);
        let receiver_public_nonce = PublicKey::from_secret_key(&receiver_private_nonce);
        let receiver_public_key = PublicKey::from_secret_key(&receiver_full_secret_key);

        let challenge = Challenge::<Blake256>::new()
            .concat((&sender_public_nonce + &receiver_public_nonce).as_bytes())
            .concat((&sender_public_excess + &receiver_public_key).as_bytes())
            .concat(&fee.to_le_bytes())
            .concat(&lock_height.to_le_bytes());

        let receiver_partial_sig =
            Signature::sign(receiver_full_secret_key, receiver_private_nonce, challenge.clone()).unwrap();
        let sender_partial_sig = Signature::sign(sender_excess_key, sender_private_nonce, challenge.clone()).unwrap();

        let s_agg = &sender_partial_sig + &receiver_partial_sig;

        // Create a kernel with a fee (taken into account in the creation of the inputs and outputs
        let kernel = TransactionKernel {
            features: KernelFeatures::empty(),
            fee,
            lock_height,
            excess: Some(final_excess),
            excess_sig: Some(s_agg),
        };

        let tx = tx_builder.with_kernel(kernel).build().unwrap();
        tx.validate().unwrap();
    }
}
