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

use crate::transactions::{
    aggregated_body::AggregateBody,
    tari_amount::{uT, MicroTari},
    transaction_protocol::{build_challenge, TransactionMetadata},
    types::{
        BlindingFactor,
        Commitment,
        CommitmentFactory,
        CryptoFactories,
        HashDigest,
        RangeProof,
        RangeProofService,
        Signature,
    },
};
use digest::Input;
use serde::{Deserialize, Serialize};
use std::{
    cmp::{max, min, Ordering},
    fmt,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    ops::Add,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    range_proof::{RangeProofError, RangeProofService as RangeProofServiceTrait},
    tari_utilities::{hex::Hex, message_format::MessageFormat, ByteArray, Hashable},
};
use thiserror::Error;

// Tx_weight(inputs(12,500), outputs(500), kernels(1)) = 19,003, still well enough below block weight of 19,500
pub const MAX_TRANSACTION_INPUTS: usize = 12_500;
pub const MAX_TRANSACTION_OUTPUTS: usize = 500;
pub const MAX_TRANSACTION_RECIPIENTS: usize = 15;
pub const MINIMUM_TRANSACTION_FEE: MicroTari = MicroTari(100);

//--------------------------------------        Output features   --------------------------------------------------//

bitflags! {
    /// Options for a kernel's structure or use.
    /// TODO:  expand to accommodate Tari DAN transaction types, such as namespace and validator node registrations
    #[derive(Deserialize, Serialize)]
    pub struct KernelFeatures: u8 {
        /// Coinbase transaction
        const COINBASE_KERNEL = 1u8;
    }
}

impl KernelFeatures {
    pub fn create_coinbase() -> KernelFeatures {
        KernelFeatures::COINBASE_KERNEL
    }
}

/// Options for UTXO's
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct OutputFeatures {
    /// Flags are the feature flags that differentiate between outputs, eg Coinbase all of which has different rules
    pub flags: OutputFlags,
    /// the maturity of the specific UTXO. This is the min lock height at which an UTXO can be spend. Coinbase UTXO
    /// require a min maturity of the Coinbase_lock_height, this should be checked on receiving new blocks.
    pub maturity: u64,
}

impl OutputFeatures {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        bincode::serialize_into(&mut buf, self).unwrap(); // this should not fail
        buf
    }

    pub fn create_coinbase(maturity_height: u64) -> OutputFeatures {
        OutputFeatures {
            flags: OutputFlags::COINBASE_OUTPUT,
            maturity: maturity_height,
        }
    }

    /// Create an `OutputFeatures` with the given maturity and all other values at their default setting
    pub fn with_maturity(maturity: u64) -> OutputFeatures {
        OutputFeatures {
            maturity,
            ..OutputFeatures::default()
        }
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures {
            flags: OutputFlags::empty(),
            maturity: 0,
        }
    }
}

impl PartialOrd for OutputFeatures {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OutputFeatures {
    fn cmp(&self, other: &Self) -> Ordering {
        self.maturity.cmp(&other.maturity)
    }
}

impl Display for OutputFeatures {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OutputFeatures: Flags = {:?}, Maturity = {}",
            self.flags, self.maturity
        )
    }
}

bitflags! {
    #[derive(Deserialize, Serialize)]
    pub struct OutputFlags: u8 {
        /// Output is a coinbase output, must not be spent until maturity
        const COINBASE_OUTPUT = 0b0000_0001;
    }
}

//----------------------------------------     TransactionError   ----------------------------------------------------//

#[derive(Clone, Debug, PartialEq, Error, Deserialize, Serialize)]
pub enum TransactionError {
    #[error("Error validating the transaction: {0}")]
    ValidationError(String),
    #[error("Signature is invalid")]
    InvalidSignatureError,
    #[error("Transaction kernel does not contain a signature")]
    NoSignatureError,
    #[error("A range proof construction or verification has produced an error: {0}")]
    RangeProofError(#[from] RangeProofError),
    #[error("Invalid kernel in body")]
    InvalidKernel,
    #[error("Invalid coinbase in body")]
    InvalidCoinbase,
    #[error("Invalid coinbase maturity in body")]
    InvalidCoinbaseMaturity,
    #[error("More than one coinbase in body")]
    MoreThanOneCoinbase,
    #[error("No coinbase in body")]
    NoCoinbase,
    #[error("Input maturity not reached")]
    InputMaturity,
}

//-----------------------------------------     UnblindedOutput   ----------------------------------------------------//

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
#[derive(Debug, Clone)]
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
            features: features.unwrap_or_default(),
        }
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub fn as_transaction_input(&self, factory: &CommitmentFactory, features: OutputFeatures) -> TransactionInput {
        let commitment = factory.commit(&self.spending_key, &self.value.into());
        TransactionInput { commitment, features }
    }

    pub fn as_transaction_output(&self, factories: &CryptoFactories) -> Result<TransactionOutput, TransactionError> {
        let commitment = factories.commitment.commit(&self.spending_key, &self.value.into());
        let output = TransactionOutput {
            features: self.features.clone(),
            commitment,
            proof: RangeProof::from_bytes(
                &factories
                    .range_proof
                    .construct_proof(&self.spending_key, self.value.into())?,
            )
            .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?,
        };
        // A range proof can be constructed for an invalid value so we should confirm that the proof can be verified.
        if !output.verify_range_proof(&factories.range_proof)? {
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

impl Hash for UnblindedOutput {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
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
    /// The features of the output being spent. We will check maturity for all outputs.
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

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output rangeproof
    pub fn is_equal_to(&self, output: &TransactionOutput) -> bool {
        self.commitment == output.commitment && self.features == output.features
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
            .chain(self.features.to_bytes())
            .chain(self.commitment.as_bytes())
            .result()
            .to_vec()
    }
}

impl Display for TransactionInput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str(&format!("{} [{:?}]\n", self.commitment.to_hex(), self.features))
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

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output rangeproof
    #[inline]
    pub fn is_equal_to(&self, output: &TransactionInput) -> bool {
        self.commitment == output.commitment && self.features == output.features
    }

    /// Returns true if the output is a coinbase, otherwise false
    pub fn is_coinbase(&self) -> bool {
        self.features.flags.contains(OutputFlags::COINBASE_OUTPUT)
    }
}

/// Implement the canonical hashing function for TransactionOutput for use in ordering.
///
/// We can exclude the range proof from this hash. The rationale for this is:
/// a) It is a significant performance boost, since the RP is the biggest part of an output
/// b) Range proofs are committed to elsewhere and so we'd be hashing them twice (and as mentioned, this is slow)
/// c) TransactionInputs will now have the same hash as UTXOs, which makes locating STXOs easier when doing reorgs
impl Hashable for TransactionOutput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.features.to_bytes())
            .chain(self.commitment.as_bytes())
            // .chain(range proof) // See docs as to why we exclude this
            .result()
            .to_vec()
    }
}

impl Default for TransactionOutput {
    fn default() -> Self {
        TransactionOutput::new(
            OutputFeatures::default(),
            CommitmentFactory::default().zero(),
            RangeProof::default(),
        )
    }
}

impl Display for TransactionOutput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let proof = self.proof.to_hex();
        fmt.write_str(&format!(
            "({} [{:?}] Proof: {}..{})",
            self.commitment.to_hex(),
            self.features,
            proof[0..16].to_string(),
            proof[proof.len() - 16..proof.len()].to_string()
        ))
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
    /// Remainder of the sum of all transaction commitments (minus an offset). If the transaction is well-formed,
    /// amounts plus fee will sum to zero, and the excess is hence a valid public key.
    pub excess: Commitment,
    /// An aggregated signature of the metadata in this kernel, signed by the individual excess values and the offset
    /// excess of the sender.
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

impl Display for TransactionKernel {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let msg = format!(
            "Fee: {}\nLock height: {}\nFeatures: {:?}\nExcess: {}\nExcess signature: {}\n",
            self.fee,
            self.lock_height,
            self.features,
            self.excess.to_hex(),
            self.excess_sig
                .to_json()
                .unwrap_or_else(|_| "Failed to serialize signature".into()),
        );
        fmt.write_str(&msg)
    }
}

/// This struct holds the result of calculating the sum of the kernels in a Transaction
/// and returns the summed commitments and the total fees
pub struct KernelSum {
    pub sum: Commitment,
    pub fees: MicroTari,
}

//----------------------------------------      Transaction       ----------------------------------------------------//

/// A transaction which consists of a kernel offset and an aggregate body made up of inputs, outputs and kernels.
/// This struct is used to describe single transactions only. The common part between transactions and Tari blocks is
/// accessible via the `body` field, but single transactions also need to carry the public offset around with them so
/// that these can be aggregated into block offsets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        let mut body = AggregateBody::new(inputs, outputs, kernels);
        body.sort();
        Transaction { offset, body }
    }

    /// Validate this transaction by checking the following:
    /// 1. The sum of inputs, outputs and fees equal the (public excess value + offset)
    /// 1. The signature signs the canonical message with the private excess
    /// 1. Range proofs of the outputs are valid
    ///
    /// This function does NOT check that inputs come from the UTXO set
    #[allow(clippy::erasing_op)] // This is for 0 * uT
    pub fn validate_internal_consistency(
        &self,
        factories: &CryptoFactories,
        reward: Option<MicroTari>,
    ) -> Result<(), TransactionError>
    {
        let reward = reward.unwrap_or_else(|| 0 * uT);
        self.body.validate_internal_consistency(&self.offset, reward, factories)
    }

    pub fn get_body(&self) -> &AggregateBody {
        &self.body
    }

    /// Returns the byte size or weight of a transaction
    pub fn calculate_weight(&self) -> u64 {
        self.body.calculate_weight()
    }

    /// Returns the total fee allocated to each byte of the transaction
    pub fn calculate_ave_fee_per_gram(&self) -> f64 {
        (self.body.get_total_fee().0 as f64) / self.calculate_weight() as f64
    }

    /// Returns the minimum maturity of the input UTXOs
    pub fn min_input_maturity(&self) -> u64 {
        self.body.inputs().iter().fold(std::u64::MAX, |min_maturity, input| {
            min(min_maturity, input.features.maturity)
        })
    }

    /// Returns the maximum maturity of the input UTXOs
    pub fn max_input_maturity(&self) -> u64 {
        self.body
            .inputs()
            .iter()
            .fold(0, |max_maturity, input| max(max_maturity, input.features.maturity))
    }

    /// Returns the maximum timelock of the kernels inside of the transaction
    pub fn max_kernel_timelock(&self) -> u64 {
        self.body
            .kernels()
            .iter()
            .fold(0, |max_timelock, kernel| max(max_timelock, kernel.lock_height))
    }

    /// Returns the height of the minimum height where the transaction is spendable. This is calculated from the
    /// transaction kernel lock_heights and the maturity of the input UTXOs.
    pub fn min_spendable_height(&self) -> u64 {
        max(self.max_kernel_timelock(), self.max_input_maturity())
    }

    /// This function adds two transactions together. It does not do cut-through. Calling Tx1 + Tx2 will result in
    /// vut-through being applied.
    pub fn add_no_cut_through(mut self, other: Self) -> Self {
        self.offset = self.offset + other.offset;
        let (mut inputs, mut outputs, mut kernels) = other.body.dissolve();
        self.body.add_inputs(&mut inputs);
        self.body.add_outputs(&mut outputs);
        self.body.add_kernels(&mut kernels);
        self
    }

    pub fn first_kernel_excess_sig(&self) -> Option<&Signature> {
        Some(&self.body.kernels().first()?.excess_sig)
    }
}

impl Add for Transaction {
    type Output = Self;

    // Note this will also do cut-through
    fn add(mut self, other: Self) -> Self {
        self = self.add_no_cut_through(other);
        self.body.do_cut_through();
        self
    }
}

impl Display for Transaction {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str("-------------- Transaction --------------\n")?;
        fmt.write_str("--- Offset ---\n")?;
        fmt.write_str(&format!("{}\n", self.offset.to_hex()))?;
        fmt.write_str("---  Body  ---\n")?;
        fmt.write_str(&format!("{}\n", self.body))
    }
}

//----------------------------------------  Transaction Builder   ----------------------------------------------------//
pub struct TransactionBuilder {
    body: AggregateBody,
    offset: Option<BlindingFactor>,
    reward: Option<MicroTari>,
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

    pub fn with_reward(&mut self, reward: MicroTari) -> &mut Self {
        self.reward = Some(reward);
        self
    }

    /// Build the transaction.
    pub fn build(self, factories: &CryptoFactories) -> Result<Transaction, TransactionError> {
        if let Some(offset) = self.offset {
            let (i, o, k) = self.body.dissolve();
            let tx = Transaction::new(i, o, k, offset);
            tx.validate_internal_consistency(factories, self.reward)?;
            Ok(tx)
        } else {
            Err(TransactionError::ValidationError(
                "Transaction validation failed".into(),
            ))
        }
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self {
            offset: None,
            body: AggregateBody::empty(),
            reward: None,
        }
    }
}

//-----------------------------------------       Tests           ----------------------------------------------------//

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        transactions::{
            helpers::{create_test_kernel, create_tx, spend_utxos},
            tari_amount::T,
            transaction::OutputFeatures,
            types::{BlindingFactor, PrivateKey, PublicKey, RangeProof},
        },
        txn_schema,
    };
    use rand::{self, rngs::OsRng};
    use tari_crypto::{keys::SecretKey as SecretKeyTrait, ristretto::pedersen::PedersenCommitmentFactory};

    #[test]
    fn unblinded_input() {
        let k = BlindingFactor::random(&mut OsRng);
        let factory = PedersenCommitmentFactory::default();
        let i = UnblindedOutput::new(10.into(), k, None);
        let input = i.as_transaction_input(&factory, OutputFeatures::default());
        assert_eq!(input.features, OutputFeatures::default());
        assert!(input.opened_by(&i, &factory));
    }

    #[test]
    fn with_maturity() {
        let features = OutputFeatures::with_maturity(42);
        assert_eq!(features.maturity, 42);
        assert_eq!(features.flags, OutputFlags::empty());
    }

    #[test]
    fn range_proof_verification() {
        let factories = CryptoFactories::new(32);
        // Directly test the tx_output verification
        let k1 = BlindingFactor::random(&mut OsRng);
        let k2 = BlindingFactor::random(&mut OsRng);

        // For testing the max range has been limited to 2^32 so this value is too large.
        let unblinded_output1 = UnblindedOutput::new((2u64.pow(32) - 1u64).into(), k1, None);
        let tx_output1 = unblinded_output1.as_transaction_output(&factories).unwrap();
        assert!(tx_output1.verify_range_proof(&factories.range_proof).unwrap());

        let unblinded_output2 = UnblindedOutput::new((2u64.pow(32) + 1u64).into(), k2.clone(), None);
        let tx_output2 = unblinded_output2.as_transaction_output(&factories);

        match tx_output2 {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionError::ValidationError("Range proof could not be verified".to_string())
            ),
        }
        let v = PrivateKey::from(2u64.pow(32) + 1);
        let c = factories.commitment.commit(&k2, &v);
        let proof = factories.range_proof.construct_proof(&k2, 2u64.pow(32) + 1).unwrap();
        let tx_output3 = TransactionOutput::new(OutputFeatures::default(), c, RangeProof::from_bytes(&proof).unwrap());
        assert_eq!(tx_output3.verify_range_proof(&factories.range_proof).unwrap(), false);
    }

    #[test]
    fn kernel_hash() {
        let s = PrivateKey::from_hex("6c6eebc5a9c02e1f3c16a69ba4331f9f63d0718401dea10adc4f9d3b879a2c09").unwrap();
        let r = PublicKey::from_hex("28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d43b").unwrap();
        let sig = Signature::new(r, s);
        let excess = Commitment::from_hex("9017be5092b85856ce71061cadeb20c2d1fabdf664c4b3f082bf44cf5065e650").unwrap();
        let k = KernelBuilder::new()
            .with_signature(&sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();
        assert_eq!(
            &k.hash().to_hex(),
            "fe25e4e961d5efec889c489d43e40a1334bf9b4408be4c2e8035a523f231a732"
        );
    }

    #[test]
    fn kernel_metadata() {
        let s = PrivateKey::from_hex("df9a004360b1cf6488d8ff7fb625bc5877f4b013f9b2b20d84932172e605b207").unwrap();
        let r = PublicKey::from_hex("5c6bfaceaa1c83fa4482a816b5f82ca3975cb9b61b6e8be4ee8f01c5f1bee561").unwrap();
        let sig = Signature::new(r, s);
        let excess = Commitment::from_hex("e0bd3f743b566272277c357075b0584fc840d79efac49e9b3b6dbaa8a351bc0c").unwrap();
        let k = KernelBuilder::new()
            .with_signature(&sig)
            .with_fee(100.into())
            .with_excess(&excess)
            .with_lock_height(500)
            .build()
            .unwrap();
        assert_eq!(
            &k.hash().to_hex(),
            "f1e7348b0952d8afbec6bfaa07a1cbc9c45e51e022242d3faeb0f190e2a9dd07"
        )
    }

    #[test]
    fn check_timelocks() {
        let factories = CryptoFactories::new(32);
        let k = BlindingFactor::random(&mut OsRng);
        let v = PrivateKey::from(2u64.pow(32) + 1);
        let c = factories.commitment.commit(&k, &v);

        let mut input = TransactionInput::new(OutputFeatures::default(), c);
        let mut kernel = create_test_kernel(0.into(), 0);
        let mut tx = Transaction::new(Vec::new(), Vec::new(), Vec::new(), 0.into());

        // lets add timelocks
        input.features.maturity = 5;
        kernel.lock_height = 2;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 2);
        assert_eq!(tx.min_spendable_height(), 5);

        input.features.maturity = 4;
        kernel.lock_height = 3;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 3);
        assert_eq!(tx.min_spendable_height(), 5);

        input.features.maturity = 2;
        kernel.lock_height = 10;
        tx.body.add_input(input.clone());
        tx.body.add_kernel(kernel.clone());

        assert_eq!(tx.max_input_maturity(), 5);
        assert_eq!(tx.max_kernel_timelock(), 10);
        assert_eq!(tx.min_spendable_height(), 10);
    }

    #[test]
    fn test_validate_internal_consistency() {
        let (tx, _, _) = create_tx(5000.into(), 15.into(), 1, 2, 1, 4);

        let factories = CryptoFactories::default();
        assert!(tx.validate_internal_consistency(&factories, None).is_ok());
    }

    #[test]
    fn check_cut_through_() {
        let (tx, _, outputs) = create_tx(50000000.into(), 15.into(), 1, 2, 1, 2);

        assert_eq!(tx.body.inputs().len(), 2);
        assert_eq!(tx.body.outputs().len(), 2);
        assert_eq!(tx.body.kernels().len(), 1);

        let factories = CryptoFactories::default();
        assert!(tx.validate_internal_consistency(&factories, None).is_ok());

        let schema = txn_schema!(from: vec![outputs[1].clone()], to: vec![1 * T, 2 * T]);
        let (tx2, _outputs, _) = spend_utxos(schema);

        assert_eq!(tx2.body.inputs().len(), 1);
        assert_eq!(tx2.body.outputs().len(), 3);
        assert_eq!(tx2.body.kernels().len(), 1);

        let mut tx3 = tx.clone().add_no_cut_through(tx2.clone());
        let tx = tx + tx2;
        // check that all inputs are as we expect them to be
        assert_eq!(tx3.body.inputs().len(), 3);
        assert_eq!(tx3.body.outputs().len(), 5);
        assert_eq!(tx3.body.kernels().len(), 2);
        // check that cut-though has not been applied
        assert!(!tx3.body.check_cut_through());

        // apply cut-through
        tx3.body.do_cut_through();

        // check that cut-through has been applied.
        assert!(tx.body.check_cut_through());
        assert!(tx.validate_internal_consistency(&factories, None).is_ok());
        assert_eq!(tx.body.inputs().len(), 2);
        assert_eq!(tx.body.outputs().len(), 4);
        assert_eq!(tx.body.kernels().len(), 2);

        assert!(tx3.body.check_cut_through());
        assert!(tx3.validate_internal_consistency(&factories, None).is_ok());
        assert_eq!(tx3.body.inputs().len(), 2);
        assert_eq!(tx3.body.outputs().len(), 4);
        assert_eq!(tx3.body.kernels().len(), 2);
    }
}
