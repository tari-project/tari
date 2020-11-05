// Copyright 2020. The Tari Project
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
//

use crate::{
    crypto::{
        commitment::HomomorphicCommitmentFactory,
        script::{to_hash, DEFAULT_SCRIPT_HASH},
    },
    serialization::hash_serializer,
    tari_utilities::{hex::Hex, ByteArray, Hashable},
    transactions::{
        crypto::{
            keys::{PublicKey as PKTrait, SecretKey},
            range_proof::{RangeProofError, RangeProofService as RangeProofTrait},
        },
        tari_amount::MicroTari,
        transaction::TransactionError,
        types::{
            BlindingFactor,
            Commitment,
            CommitmentFactory,
            CryptoFactories,
            HashArray,
            HashDigest,
            HashOutput,
            PrivateKey,
            PublicKey,
            RangeProof,
            RangeProofService,
        },
    },
};
use digest::Input;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
};
use tari_crypto::script::TariScript;

//--------------------------------------        Output features   --------------------------------------------------//

/// Options for UTXOs
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct OutputFeatures {
    /// Flags are the feature flags that differentiate between outputs, eg Coinbase all of which has different rules
    pub flags: OutputFlags,
    /// the maturity of the specific UTXO. This is the min lock height at which an UTXO can be spent. Coinbase UTXO
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

//-----------------------------------------     OutputBuilder     ----------------------------------------------------//

/// A builder for helping construct [UnblindedOutput] instances. All the parameters are optional. If omitted, the
/// following defaults are used:
/// * value: 0
/// * key: A new random value
/// * features: Default features
/// * script: Null script
pub struct OutputBuilder {
    value: MicroTari,
    spending_key: Option<BlindingFactor>,
    features: Option<OutputFeatures>,
    script: Option<TariScript>,
}

impl OutputBuilder {
    /// Return a new OutputBuilder instance. The builder methods use fluent syntax, so you could build an output as
    /// follows:
    ///
    /// # Example
    ///
    /// ```edition2018
    ///  # use tari_core::transactions::OutputBuilder;
    ///  # use tari_core::transactions::types::CommitmentFactory;
    ///  let factory = CommitmentFactory::default();
    ///  let _output = OutputBuilder::new().with_value(100).build(&factory).unwrap();
    /// ```
    pub fn new() -> Self {
        OutputBuilder {
            value: 0.into(),
            spending_key: None,
            features: None,
            script: None,
        }
    }

    /// Set the value of the output. [value] is any type that can be converted into [MicroTari].
    pub fn with_value<T: Into<MicroTari>>(mut self, value: T) -> Self {
        self.value = value.into();
        self
    }

    /// Assign the spending key for the output.
    pub fn with_spending_key(mut self, key: PrivateKey) -> Self {
        self.spending_key = Some(key);
        self
    }

    /// Assign the featires for the output.
    pub fn with_features(mut self, features: OutputFeatures) -> Self {
        self.features = Some(features);
        self
    }

    /// Assign the redeem script for this output.
    pub fn with_script(mut self, script: TariScript) -> Self {
        self.script = Some(script);
        self
    }

    /// Build the UnblindedOutput instance
    pub fn build(self, factory: &CommitmentFactory) -> Result<UnblindedOutput, TransactionError> {
        let key = match self.spending_key {
            Some(k) => k,
            None => PrivateKey::random(&mut rand::rngs::OsRng),
        };
        let script = self.script.unwrap_or_default();
        UnblindedOutput::new(self.value, key, self.features, script, factory)
    }
}

//-----------------------------------------     UnblindedOutput   ----------------------------------------------------//

/// An unblinded output is one where the value and spending key (blinding factor) are known. This can be used to
/// build both inputs and outputs (every input comes from an output)
#[derive(Debug, Clone)]
pub struct UnblindedOutput {
    value: MicroTari,
    spending_key: BlindingFactor,
    features: OutputFeatures,
    script: TariScript,
    script_hash: HashArray,
    blinding_factor: BlindingFactor,
    commit_hash: PrivateKey,
    commitment: Commitment,
}

impl UnblindedOutput {
    /// Creates a new un-blinded output. This constructor is private. Use [OutputBuilder] to create
    pub fn new(
        value: MicroTari,
        spending_key: BlindingFactor,
        features: Option<OutputFeatures>,
        script: TariScript,
        factory: &CommitmentFactory,
    ) -> Result<UnblindedOutput, TransactionError>
    {
        let script_hash = script
            .as_hash::<HashDigest>()
            .map_err(TransactionError::InvalidScript)?;
        let base_commitment = factory.commit_value(&spending_key, value.into());
        let commit_hash = HashDigest::new()
            .chain(base_commitment.as_bytes())
            .chain(&script_hash)
            .result();
        let commit_hash = PrivateKey::from_bytes(commit_hash.as_slice())
            .expect("One should always be able to convert a slice to a private key");
        let blinding_factor = &spending_key + &commit_hash;
        let commitment = factory.commit_value(&blinding_factor, value.into());

        Ok(UnblindedOutput {
            value,
            spending_key,
            features: features.unwrap_or_default(),
            script,
            script_hash: to_hash(&script_hash),
            commit_hash,
            blinding_factor,
            commitment,
        })
    }

    /// Return the value represented by this output
    pub fn value(&self) -> MicroTari {
        self.value
    }

    /// Return a reference to the output features for this output instance
    pub fn features(&self) -> &OutputFeatures {
        &self.features
    }

    /// Commits an UnblindedOutput into a Transaction input
    pub fn as_transaction_input(&self) -> TransactionInput {
        let script_hash = self.script_hash().to_vec();
        let commitment = self.commitment().clone();
        TransactionInput {
            commitment,
            features: self.features().clone(),
            script_hash,
        }
    }

    pub fn as_transaction_output(&self, factories: &CryptoFactories) -> Result<TransactionOutput, TransactionError> {
        // Check that value is in range. Rust ensures that it is >= 0, so we must check the upper bound
        // This check is a bit weird, but it's saying that any bits to the left of the max range value must be zero
        // (i.e. the upper bound is respected); and we split it to avoid overflow errors

        if u64::from(self.value) >> (factories.range_proof.range() - 1) as u64 >> 1 != 0u64 {
            return Err(TransactionError::ValidationError(
                "Invalid transaction output: Value outside of range".into(),
            ));
        }
        let script_hash = self.script_hash().to_vec();
        let proof = self.generate_range_proof(factories)?;
        let commitment = self.commitment().clone();
        let output = TransactionOutput {
            features: self.features.clone(),
            commitment,
            proof,
            script_hash,
        };
        Ok(output)
    }

    /// Generate and validate a range proof for the output.
    pub fn generate_range_proof(&self, factories: &CryptoFactories) -> Result<RangeProof, TransactionError> {
        let proof = RangeProof::from_bytes(
            &factories
                .range_proof
                .construct_proof(self.blinding_factor(), self.value.into())?,
        )
        .map_err(|_| TransactionError::RangeProofError(RangeProofError::ProofConstructionError))?;
        Ok(proof)
    }

    /// Return the hash of the Tari script associated with this output.
    pub fn script_hash(&self) -> &[u8] {
        &self.script_hash
    }

    /// Calculate the base commitment for this output. The base commitment is defined as `k.G + v.H`
    pub fn base_commitment(&self, factory: &CommitmentFactory) -> Commitment {
        factory.commit_value(self.blinding_factor(), self.value().into())
    }

    /// Calculate the commitment hash -- the term(s) that are added to the spending_key to make up the output
    /// blinding factor.
    ///
    /// The function takes the base commitment as a parameter to avoid unnecessary recalculations of this term.
    ///
    /// # Returns
    /// The function returns the commitment hash
    pub fn commitment_hash(&self) -> &PrivateKey {
        &self.commit_hash
    }

    /// Return the effective blinding factor representing this output. In standard Mimblewimble, this is the
    /// spending_key. With Tari script, the blinding factor is `spending_key + H(C||si)`
    ///
    /// # Returns
    ///
    /// The adjusted blinding factor for this output
    pub fn blinding_factor(&self) -> &BlindingFactor {
        &self.blinding_factor
    }

    /// Return the spending key representing this output. In standard Mimblewimble, this is the same as the blinding
    /// factor.
    ///
    /// # Returns
    ///
    /// The spending key associated with this output
    pub fn spending_key(&self) -> &BlindingFactor {
        &self.spending_key
    }

    /// Calculate the public key from the spending key, i.e. k.G
    pub fn public_spending_key(&self) -> PublicKey {
        PublicKey::from_secret_key(&self.spending_key)
    }

    /// Calculate the public key associated with the blinding factor (k + H(C||s)).G
    pub fn public_blinding_factor(&self) -> PublicKey {
        PublicKey::from_secret_key(&self.blinding_factor)
    }

    /// Calculate the commitment associated with this output. For standard Mimblewimble, this would be equal to the
    /// base commitment. With Tari script it is `base_commitment + commitment_hash.G`
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }
}

// These implementations are used for order these outputs for UTXO selection which will be done by comparing the values
impl Eq for UnblindedOutput {}

impl PartialEq for UnblindedOutput {
    fn eq(&self, other: &UnblindedOutput) -> bool {
        self.commitment == other.commitment
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

//----------------------------------------   TransactionOutput    ----------------------------------------------------//

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransactionOutput {
    /// Options for an output's structure or use
    pub(crate) features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    commitment: Commitment,
    /// A proof the commitment is in the right range
    proof: RangeProof,
    /// The hash of the locking script on this UTXO.
    #[serde(with = "hash_serializer")]
    script_hash: HashOutput,
}

/// An output for a transaction, includes a range proof
impl TransactionOutput {
    /// Create new Transaction Output
    pub fn new(
        features: OutputFeatures,
        commitment: Commitment,
        proof: RangeProof,
        script_hash: &[u8],
    ) -> TransactionOutput
    {
        TransactionOutput {
            features,
            commitment,
            proof,
            script_hash: script_hash.to_vec(),
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

    /// Return the feature set for this output
    pub fn features(&self) -> &OutputFeatures {
        &self.features
    }

    /// Return the hash of the locking script on this output
    pub fn script_hash(&self) -> &[u8] {
        &self.script_hash
    }

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<bool, TransactionError> {
        Ok(prover.verify(&self.proof().to_vec(), &self.commitment))
    }

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output rangeproof
    pub fn is_equal_to(&self, output: &TransactionInput) -> bool {
        self.commitment == output.commitment &&
            self.features == output.features &&
            self.script_hash == output.script_hash
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
            // commitment has the script hash baked in
            .chain(self.commitment.as_bytes())
            .chain(&self.script_hash)
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
            &DEFAULT_SCRIPT_HASH,
        )
    }
}

impl Display for TransactionOutput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let proof = self.proof.to_hex();
        fmt.write_str(&format!(
            "{} [{:?}] Proof: {}..{}\n",
            self.commitment.to_hex(),
            self.features,
            proof[0..16].to_string(),
            proof[proof.len() - 16..proof.len()].to_string()
        ))
    }
}

//----------------------------------------     TransactionInput   ----------------------------------------------------//

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransactionInput {
    /// The features of the output being spent. We will check maturity for all outputs.
    features: OutputFeatures,
    /// The commitment referencing the output being spent.
    commitment: Commitment,
    /// The hash of the locking script on this input
    #[serde(with = "hash_serializer")]
    script_hash: HashOutput,
}

/// An input for a transaction that spends an existing output
impl TransactionInput {
    /// Create a new Transaction Input
    pub fn new(features: OutputFeatures, commitment: Commitment, script_hash: &[u8]) -> TransactionInput {
        TransactionInput {
            features,
            commitment,
            script_hash: script_hash.to_vec(),
        }
    }

    /// Accessor method for the commitment contained in an input
    pub fn features(&self) -> &OutputFeatures {
        &self.features
    }

    /// Accessor method for the commitment contained in an input
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Return the hash of the locking script on this input
    pub fn script_hash(&self) -> &[u8] {
        &self.script_hash
    }

    /// Checks if the given un-blinded input instance corresponds to this blinded Transaction Input
    pub fn opened_by(&self, input: &UnblindedOutput, factory: &CommitmentFactory) -> bool {
        factory.open_value(input.blinding_factor(), input.value().into(), &self.commitment)
    }

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output range proof
    pub fn is_equal_to(&self, output: &TransactionOutput) -> bool {
        self.commitment == output.commitment && self.features == output.features
    }
}

impl From<TransactionOutput> for TransactionInput {
    fn from(item: TransactionOutput) -> Self {
        TransactionInput {
            features: item.features,
            commitment: item.commitment,
            script_hash: item.script_hash,
        }
    }
}

/// Implement the canonical hashing function for TransactionInput for use in ordering
impl Hashable for TransactionInput {
    fn hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.features.to_bytes())
            .chain(self.commitment.as_bytes())
            .chain(&self.script_hash)
            .result()
            .to_vec()
    }
}

impl Display for TransactionInput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fmt.write_str(&format!("{} [{:?}]\n", self.commitment.to_hex(), self.features))
    }
}

//-----------------------------------------       Tests           ----------------------------------------------------//

#[cfg(test)]
mod test {
    use super::*;
    use crate::transactions::{
        types::{BlindingFactor, PrivateKey, RangeProof},
        OutputFeatures,
    };
    use rand::{self, rngs::OsRng};
    use tari_crypto::{keys::SecretKey as SecretKeyTrait, ristretto::pedersen::PedersenCommitmentFactory};

    #[test]
    fn unblinded_input() {
        let factory = PedersenCommitmentFactory::default();
        let i = OutputBuilder::new().with_value(10).build(&factory).unwrap();
        let input = i.as_transaction_input();
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
        let k2 = BlindingFactor::random(&mut OsRng);

        // For testing the max range has been limited to 2^32 so this value is too large.
        let unblinded_output1 = OutputBuilder::new()
            .with_value(2u64.pow(32) - 1u64)
            .build(&factories.commitment)
            .unwrap();
        let tx_output1 = unblinded_output1.as_transaction_output(&factories).unwrap();
        assert!(tx_output1.verify_range_proof(&factories.range_proof).unwrap());

        let unblinded_output2 = OutputBuilder::new()
            .with_value(2u64.pow(32) + 1u64)
            .with_spending_key(k2.clone())
            .build(&factories.commitment)
            .unwrap();
        let tx_output2 = unblinded_output2.as_transaction_output(&factories);

        match tx_output2 {
            Ok(_) => panic!("Range proof should have failed to verify"),
            Err(e) => assert_eq!(
                e,
                TransactionError::ValidationError("Invalid transaction output: Value outside of range".to_string())
            ),
        }
        let v = PrivateKey::from(2u64.pow(32) + 1);
        let c = factories.commitment.commit(&k2, &v);
        let proof = factories.range_proof.construct_proof(&k2, 2u64.pow(32) + 1).unwrap();
        let tx_output3 = TransactionOutput::new(
            OutputFeatures::default(),
            c,
            RangeProof::from_bytes(&proof).unwrap(),
            &DEFAULT_SCRIPT_HASH,
        );
        assert_eq!(tx_output3.verify_range_proof(&factories.range_proof).unwrap(), false);
    }
}
