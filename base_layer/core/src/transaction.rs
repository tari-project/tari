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
    blocks::aggregated_body::AggregateBody,
    tari_amount::MicroTari,
    types::{BlindingFactor, Commitment, CommitmentFactory, Signature},
};

use crate::{
    transaction_protocol::{build_challenge, TransactionMetadata},
    types::{HashDigest, RangeProof, RangeProofService},
};
use derive_error::Error;
use digest::Input;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    range_proof::{RangeProofError, RangeProofService as RangeProofServiceTrait},
};
use tari_utilities::{ByteArray, Hashable};

// These are set fairly arbitrarily at the moment. We'll need to do some modelling / testing to tune these values.
pub const MAX_TRANSACTION_INPUTS: usize = 500;
pub const MAX_TRANSACTION_OUTPUTS: usize = 100;
pub const MAX_TRANSACTION_RECIPIENTS: usize = 15;
pub const MINIMUM_TRANSACTION_FEE: MicroTari = MicroTari(100);

//--------------------------------------        Bit flag features   --------------------------------------------------//

bitflags! {
    /// Options for a kernel's structure or use.
    /// TODO:  expand to accommodate Tari DAN transaction types, such as namespace and validator node registrations
    #[derive(Deserialize, Serialize)]
    pub struct KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
    }
}

bitflags! {
    #[derive(Deserialize, Serialize)]
    pub struct OutputFeatures: u8 {
        /// Output is a coinbase output, must not be spent until maturity
        const COINBASE_OUTPUT = 0b0000_0001;
    }
}

//----------------------------------------     TransactionError   ----------------------------------------------------//

#[derive(Clone, Debug, PartialEq, Error)]
pub enum TransactionError {
    // Error validating the transaction
    #[error(msg_embedded, no_from, non_std)]
    ValidationError(String),
    // Signature could not be verified
    InvalidSignatureError,
    // Transaction kernel does not contain a signature
    NoSignatureError,
    // A range proof construction or verification has produced an error
    RangeProofError(RangeProofError),
}

//-----------------------------------------     UnblindedOutput   ----------------------------------------------------//

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
#[derive(Debug, Clone, Hash)]
pub struct UnblindedOutput {
    pub value: MicroTari,
    pub spending_key: BlindingFactor,
    pub features: OutputFeatures,
}

impl UnblindedOutput {
    /// Creates a new un-blinded output
    pub fn new(value: MicroTari, spending_key: BlindingFactor, features: Option<OutputFeatures>) -> UnblindedOutput {
        UnblindedOutput {
            value,
            spending_key,
            features: features.unwrap_or_else(OutputFeatures::empty),
        }
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub fn as_transaction_input(&self, factory: &CommitmentFactory, features: OutputFeatures) -> TransactionInput {
        let commitment = factory.commit(&self.spending_key, &self.value.into());
        TransactionInput { commitment, features }
    }

    pub fn as_transaction_output(
        &self,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
        features: OutputFeatures,
    ) -> Result<TransactionOutput, TransactionError>
    {
        let commitment = factory.commit(&self.spending_key, &self.value.into());
        let output = TransactionOutput {
            features,
            commitment,
            proof: RangeProof::from_bytes(&prover.construct_proof(&self.spending_key, self.value.into())?)
                .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?,
        };
        // A range proof can be constructed for an invalid value so we should confirm that the proof can be verified.
        if !output.verify_range_proof(&prover)? {
            return Err(TransactionError::ValidationError(
                "Range proof could not be verified".into(),
            ));
        }
        Ok(output)
    }
}

// These implementations are used for order these outputs for UTXO selection which will be done by comparing the values
impl Eq for UnblindedOutput {}

impl PartialEq for UnblindedOutput {
    fn eq(&self, other: &UnblindedOutput) -> bool {
        self.value == other.value
    }
}

impl PartialOrd<UnblindedOutput> for UnblindedOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl Ord for UnblindedOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

//----------------------------------------     TransactionInput   ----------------------------------------------------//

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Checks if the given un-blinded input instance corresponds to this blinded Transaction Input
    pub fn opened_by(&self, input: &UnblindedOutput, factory: &CommitmentFactory) -> bool {
        factory.open(&input.spending_key, &input.value.into(), &self.commitment)
    }
}

impl From<TransactionOutput> for TransactionInput {
    fn from(item: TransactionOutput) -> Self {
        TransactionInput {
            features: item.features,
            commitment: item.commitment,
        }
    }
}

/// Implement the canonical hashing function for TransactionInput for use in ordering
impl Hashable for TransactionInput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(vec![self.features.bits])
            .chain(self.commitment.as_bytes())
            .result()
            .to_vec()
    }
}

//----------------------------------------   TransactionOutput    ----------------------------------------------------//

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransactionOutput {
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commitment: Commitment,
    /// A proof that the commitment is in the right range
    pub proof: RangeProof,
}

/// An output for a transaction, includes a range proof
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
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Accessor method for the range proof contained in an output
    pub fn proof(&self) -> &RangeProof {
        &self.proof
    }

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<bool, TransactionError> {
        Ok(prover.verify(&self.proof.to_vec(), &self.commitment))
    }
}

/// Implement the canonical hashing function for TransactionOutput for use in ordering
impl Hashable for TransactionOutput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(vec![self.features.bits])
            .chain(self.commitment.as_bytes())
            .chain(self.proof.as_bytes())
            .result()
            .to_vec()
    }
}

impl Default for TransactionOutput {
    fn default() -> Self {
        TransactionOutput::new(
            OutputFeatures::empty(),
            CommitmentFactory::default().zero(),
            RangeProof::default(),
        )
    }
}

//----------------------------------------   Transaction Kernel   ----------------------------------------------------//

/// The transaction kernel tracks the excess for a given transaction. For an explanation of what the excess is, and
/// why it is necessary, refer to the
/// [Mimblewimble TLU post](https://tlu.tarilabs.com/protocols/mimblewimble-1/sources/PITCHME.link.html?highlight=mimblewimble#mimblewimble).
/// The kernel also tracks other transaction metadata, such as the lock height for the transaction (i.e. the earliest
/// this transaction can be mined) and the transaction fee, in cleartext.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransactionKernel {
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: MicroTari,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments. If the transaction
    /// is well formed, amounts components should sum to zero and the excess
    /// is hence a valid public key.
    pub excess: Commitment,
    /// The signature proving the excess is a valid public key, which signs
    /// the transaction fee.
    pub excess_sig: Signature,
}

/// A version of Transaction kernel with optional fields. This struct is only used in constructing transaction kernels
pub struct KernelBuilder {
    features: KernelFeatures,
    fee: MicroTari,
    lock_height: u64,
    excess: Option<Commitment>,
    excess_sig: Option<Signature>,
}

/// Implementation of the transaction kernel
impl KernelBuilder {
    /// Creates an empty transaction kernel
    pub fn new() -> KernelBuilder {
        KernelBuilder::default()
    }

    /// Build a transaction kernel with the provided features
    pub fn with_features(mut self, features: KernelFeatures) -> KernelBuilder {
        self.features = features;
        self
    }

    /// Build a transaction kernel with the provided fee
    pub fn with_fee(mut self, fee: MicroTari) -> KernelBuilder {
        self.fee = fee;
        self
    }

    /// Build a transaction kernel with the provided lock height
    pub fn with_lock_height(mut self, lock_height: u64) -> KernelBuilder {
        self.lock_height = lock_height;
        self
    }

    /// Add the excess (sum of public spend keys minus the offset)
    pub fn with_excess(mut self, excess: &Commitment) -> KernelBuilder {
        self.excess = Some(excess.clone());
        self
    }

    /// Add the excess signature
    pub fn with_signature(mut self, signature: &Signature) -> KernelBuilder {
        self.excess_sig = Some(signature.clone());
        self
    }

    pub fn build(self) -> Result<TransactionKernel, TransactionError> {
        if self.excess.is_none() || self.excess_sig.is_none() {
            return Err(TransactionError::NoSignatureError);
        }
        Ok(TransactionKernel {
            features: self.features,
            fee: self.fee,
            lock_height: self.lock_height,
            excess: self.excess.unwrap(),
            excess_sig: self.excess_sig.unwrap(),
        })
    }
}

impl Default for KernelBuilder {
    fn default() -> Self {
        KernelBuilder {
            features: KernelFeatures::empty(),
            fee: MicroTari::from(0),
            lock_height: 0,
            excess: None,
            excess_sig: None,
        }
    }
}

impl TransactionKernel {
    pub fn verify_signature(&self) -> Result<(), TransactionError> {
        let excess = self.excess.as_public_key();
        let r = self.excess_sig.get_public_nonce();
        let m = TransactionMetadata {
            lock_height: self.lock_height,
            fee: self.fee,
        };
        let c = build_challenge(r, &m);
        if self.excess_sig.verify_challenge(excess, &c) {
            Ok(())
        } else {
            Err(TransactionError::InvalidSignatureError)
        }
    }
}

impl Hashable for TransactionKernel {
    /// Produce a canonical hash for a transaction kernel. The hash is given by
    /// $$ H(feature_bits | fee | lock_height | P_excess | R_sum | s_sum)
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(&[self.features.bits])
            .chain(u64::from(self.fee).to_le_bytes())
            .chain(self.lock_height.to_le_bytes())
            .chain(self.excess.as_bytes())
            .chain(self.excess_sig.get_public_nonce().as_bytes())
            .chain(self.excess_sig.get_signature().as_bytes())
            .result()
            .to_vec()
    }
}

//----------------------------------------      Transaction       ----------------------------------------------------//

/// A transaction which consists of a kernel offset and an aggregate body made up of inputs, outputs and kernels.
/// This struct is used to describe single transactions only. The common part between transactions and Tari blocks is
/// accessible via the `body` field, but single transactions also need to carry the public offset around with them so
/// that these can be aggregated into block offsets.
#[derive(Clone, Debug)]
pub struct Transaction {
    /// This kernel offset will be accumulated when transactions are aggregated to prevent the "subset" problem where
    /// kernels can be linked to inputs and outputs by testing a series of subsets and see which produce valid
    /// transactions.
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

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    pub fn validate_internal_consistency(
        &mut self,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> Result<(), TransactionError>
    {
        self.body.validate_internal_consistency(&self.offset, prover, factory)
    }

    pub fn get_body(&self) -> &AggregateBody {
        &self.body
    }
}

//----------------------------------------  Transaction Builder   ----------------------------------------------------//
pub struct TransactionBuilder {
    body: AggregateBody,
    offset: Option<BlindingFactor>,
}

impl TransactionBuilder {
    /// Create an new empty TransactionBuilder
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the offset of an existing transaction
    pub fn add_offset(&mut self, offset: BlindingFactor) -> &mut Self {
        self.offset = Some(offset);
        self
    }

    /// Add an input to an existing transaction
    pub fn add_input(&mut self, input: TransactionInput) -> &mut Self {
        self.body.add_input(input);
        self
    }

    /// Add an output to an existing transaction
    pub fn add_output(&mut self, output: TransactionOutput) -> &mut Self {
        self.body.add_output(output);
        self
    }

    /// Moves a series of inputs to an existing transaction, leaving `inputs` empty
    pub fn add_inputs(&mut self, inputs: &mut Vec<TransactionInput>) -> &mut Self {
        self.body.add_inputs(inputs);
        self
    }

    /// Moves a series of outputs to an existing transaction, leaving `outputs` empty
    pub fn add_outputs(&mut self, outputs: &mut Vec<TransactionOutput>) -> &mut Self {
        self.body.add_outputs(outputs);
        self
    }

    /// Set the kernel of a transaction. Currently only one kernel is allowed per transaction
    pub fn with_kernel(&mut self, kernel: TransactionKernel) -> &mut Self {
        self.body.set_kernel(kernel);
        self
    }

    pub fn build(
        self,
        prover: &RangeProofService,
        factory: &CommitmentFactory,
    ) -> Result<Transaction, TransactionError>
    {
        if let Some(offset) = self.offset {
            let mut tx = Transaction::new(self.body.inputs, self.body.outputs, self.body.kernels, offset);
            tx.validate_internal_consistency(prover, factory)?;
            Ok(tx)
        } else {
            return Err(TransactionError::ValidationError(
                "Transaction validation failed".into(),
            ));
        }
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self {
            offset: None,
            body: AggregateBody::empty(),
        }
    }
}

//-----------------------------------------       Tests           ----------------------------------------------------//

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        transaction::OutputFeatures,
        types::{BlindingFactor, PrivateKey, RangeProof},
    };
    use rand;
    use tari_crypto::{
        keys::SecretKey as SecretKeyTrait,
        ristretto::{dalek_range_proof::DalekRangeProofService, pedersen::PedersenCommitmentFactory},
    };

    #[test]
    fn unblinded_input() {
        let mut rng = rand::OsRng::new().unwrap();
        let k = BlindingFactor::random(&mut rng);
        let factory = PedersenCommitmentFactory::default();
        let i = UnblindedOutput::new(10.into(), k, None);
        let input = i.as_transaction_input(&factory, OutputFeatures::empty());
        assert_eq!(input.features, OutputFeatures::empty());
        assert!(input.opened_by(&i, &factory));
    }

    #[test]
    fn range_proof_verification() {
        let mut rng = rand::OsRng::new().unwrap();
        let factory = PedersenCommitmentFactory::default();
        let prover = DalekRangeProofService::new(32, &factory).unwrap();
        // Directly test the tx_output verification
        let k1 = BlindingFactor::random(&mut rng);
        let k2 = BlindingFactor::random(&mut rng);

        // For testing the max range has been limited to 2^32 so this value is too large.
        let unblinded_output1 = UnblindedOutput::new((2u64.pow(32) - 1u64).into(), k1, None);
        let tx_output1 = unblinded_output1
            .as_transaction_output(&prover, &factory, OutputFeatures::empty())
            .unwrap();
        assert!(tx_output1.verify_range_proof(&prover).unwrap());

        let unblinded_output2 = UnblindedOutput::new((2u64.pow(32) + 1u64).into(), k2.clone(), None);
        let tx_output2 = unblinded_output2.as_transaction_output(&prover, &factory, OutputFeatures::empty());

        match tx_output2 {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionError::ValidationError("Range proof could not be verified".to_string())
            ),
        }
        let v = PrivateKey::from(2u64.pow(32) + 1);
        let c = factory.commit(&k2, &v);
        let proof = prover.construct_proof(&k2, 2u64.pow(32) + 1).unwrap();
        let tx_output3 = TransactionOutput::new(OutputFeatures::empty(), c, RangeProof::from_bytes(&proof).unwrap());
        assert_eq!(tx_output3.verify_range_proof(&prover).unwrap(), false);
    }
}
