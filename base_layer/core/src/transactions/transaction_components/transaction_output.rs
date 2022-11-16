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

use std::{
    cmp::{min, Ordering},
    fmt::{Display, Formatter},
    io,
    io::{Read, Write},
};

use log::*;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    BlindingFactor,
    ComSignature,
    Commitment,
    CommitmentFactory,
    FixedHash,
    PrivateKey,
    PublicKey,
    RangeProof,
    RangeProofService,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    errors::RangeProofError,
    extended_range_proof::{ExtendedRangeProofService, Statement},
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    ristretto::bulletproofs_plus::RistrettoAggregatedPublicStatement,
    tari_utilities::{hex::Hex, ByteArray},
};
use tari_script::TariScript;

use super::TransactionOutputVersion;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, DomainSeparatedConsensusHasher},
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components,
        transaction_components::{EncryptedValue, OutputFeatures, OutputType, TransactionError, TransactionInput},
        TransactionHashDomain,
    },
};

pub const LOG_TARGET: &str = "c::transactions::transaction_output";

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionOutput {
    pub version: TransactionOutputVersion,
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commitment: Commitment,
    /// A proof that the commitment is in the right range
    pub proof: RangeProof,
    /// The script that will be executed when spending this output
    pub script: TariScript,
    /// Tari script offset pubkey, K_O
    pub sender_offset_public_key: PublicKey,
    /// UTXO signature with the script offset private key, k_O
    pub metadata_signature: ComSignature,
    /// The covenant that will be executed when spending this output
    #[serde(default)]
    pub covenant: Covenant,
    /// Encrypted value.
    pub encrypted_value: EncryptedValue,
    /// The minimum value of the commitment that is proven by the range proof
    #[serde(default)]
    pub minimum_value_promise: MicroTari,
}

/// An output for a transaction, includes a range proof and Tari script metadata
impl TransactionOutput {
    /// Create new Transaction Output

    pub fn new(
        version: TransactionOutputVersion,
        features: OutputFeatures,
        commitment: Commitment,
        proof: RangeProof,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
        covenant: Covenant,
        encrypted_value: EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> TransactionOutput {
        TransactionOutput {
            version,
            features,
            commitment,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_value,
            minimum_value_promise,
        }
    }

    pub fn new_current_version(
        features: OutputFeatures,
        commitment: Commitment,
        proof: RangeProof,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
        covenant: Covenant,
        encrypted_value: EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> TransactionOutput {
        TransactionOutput::new(
            TransactionOutputVersion::get_current_version(),
            features,
            commitment,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_value,
            minimum_value_promise,
        )
    }

    /// Accessor method for the commitment contained in an output
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Accessor method for the range proof contained in an output
    pub fn proof(&self) -> &RangeProof {
        &self.proof
    }

    pub fn hash(&self) -> FixedHash {
        transaction_components::hash_output(
            self.version,
            &self.features,
            &self.commitment,
            &self.script,
            &self.covenant,
            &self.encrypted_value,
            &self.sender_offset_public_key,
            self.minimum_value_promise,
        )
    }

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<(), TransactionError> {
        let statement = RistrettoAggregatedPublicStatement {
            statements: vec![Statement {
                commitment: self.commitment.clone(),
                minimum_value_promise: self.minimum_value_promise.as_u64(),
            }],
        };
        match prover.verify_batch(vec![&self.proof.0], vec![&statement]) {
            Ok(_) => Ok(()),
            Err(e) => Err(TransactionError::ValidationError(format!(
                "Recipient output range proof failed to verify ({})",
                e
            ))),
        }
    }

    /// Verify that the metadata signature is valid
    pub fn verify_metadata_signature(&self) -> Result<(), TransactionError> {
        let challenge = TransactionOutput::build_metadata_signature_challenge(
            self.version,
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.public_nonce(),
            &self.commitment,
            &self.covenant,
            &self.encrypted_value,
            self.minimum_value_promise,
        );
        if !self.metadata_signature.verify_challenge(
            &(&self.commitment + &self.sender_offset_public_key),
            &challenge,
            &CommitmentFactory::default(),
        ) {
            return Err(TransactionError::InvalidSignatureError(
                "Metadata signature not valid!".to_string(),
            ));
        }
        Ok(())
    }

    pub fn verify_validator_node_signature(&self) -> Result<(), TransactionError> {
        if let Some(validator_node_reg) = self
            .features
            .sidechain_feature
            .as_ref()
            .and_then(|f| f.validator_node_registration())
        {
            if !validator_node_reg.is_valid_signature_for(self.commitment.as_bytes()) {
                return Err(TransactionError::InvalidSignatureError(
                    "Validator node signature is not valid!".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Attempt to rewind the range proof to reveal the mask (blinding factor)
    pub fn recover_mask(
        &self,
        prover: &RangeProofService,
        rewind_blinding_key: &PrivateKey,
    ) -> Result<BlindingFactor, TransactionError> {
        Ok(prover.recover_mask(&self.proof.0, &self.commitment, rewind_blinding_key)?)
    }

    /// Attempt to verify a recovered mask (blinding factor) for a proof against the commitment.
    pub fn verify_mask(
        &self,
        prover: &RangeProofService,
        blinding_factor: &PrivateKey,
        value: u64,
    ) -> Result<bool, TransactionError> {
        Ok(prover.verify_mask(&self.commitment, blinding_factor, value)?)
    }

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output range proof
    #[inline]
    pub fn is_equal_to(&self, output: &TransactionInput) -> bool {
        self.hash() == output.output_hash()
    }

    /// Returns true if the output is a coinbase, otherwise false
    pub fn is_coinbase(&self) -> bool {
        matches!(self.features.output_type, OutputType::Coinbase)
    }

    /// Returns true if the output is burned, otherwise false
    pub fn is_burned(&self) -> bool {
        matches!(self.features.output_type, OutputType::Burn)
    }

    /// Convenience function that returns the challenge for the metadata commitment signature
    pub fn get_metadata_signature_challenge(&self, partial_commitment_nonce: Option<&PublicKey>) -> [u8; 32] {
        let nonce_commitment = match partial_commitment_nonce {
            None => self.metadata_signature.public_nonce().clone(),
            Some(partial_nonce) => self.metadata_signature.public_nonce() + partial_nonce,
        };
        TransactionOutput::build_metadata_signature_challenge(
            self.version,
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            &nonce_commitment,
            &self.commitment,
            &self.covenant,
            &self.encrypted_value,
            self.minimum_value_promise,
        )
    }

    /// Convenience function that calculates the challenge for the metadata commitment signature
    pub fn build_metadata_signature_challenge(
        version: TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        public_commitment_nonce: &Commitment,
        commitment: &Commitment,
        covenant: &Covenant,
        encrypted_value: &EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> [u8; 32] {
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("metadata_signature")
            .chain(public_commitment_nonce)
            .chain(script)
            .chain(features)
            .chain(sender_offset_public_key)
            .chain(commitment)
            .chain(covenant)
            .chain(encrypted_value)
            .chain(&minimum_value_promise);
        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize(),
        }
    }

    // Create commitment signature for the metadata

    fn create_metadata_signature(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        partial_commitment_nonce: Option<&PublicKey>,
        sender_offset_private_key: Option<&PrivateKey>,
        covenant: &Covenant,
        encrypted_value: &EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Result<ComSignature, TransactionError> {
        let nonce_a = PrivateKey::random(&mut OsRng);
        let nonce_b = PrivateKey::random(&mut OsRng);
        let nonce_commitment = CommitmentFactory::default().commit(&nonce_b, &nonce_a);
        let nonce_commitment = match partial_commitment_nonce {
            None => nonce_commitment,
            Some(partial_nonce) => &nonce_commitment + partial_nonce,
        };
        let pk_value = PrivateKey::from(value.as_u64());
        let commitment = CommitmentFactory::default().commit(spending_key, &pk_value);
        let e = TransactionOutput::build_metadata_signature_challenge(
            version,
            script,
            output_features,
            sender_offset_public_key,
            &nonce_commitment,
            &commitment,
            covenant,
            encrypted_value,
            minimum_value_promise,
        );
        let secret_x = match sender_offset_private_key {
            None => spending_key.clone(),
            Some(key) => spending_key + key,
        };
        Ok(ComSignature::sign(
            &pk_value,
            &secret_x,
            &nonce_a,
            &nonce_b,
            &e,
            &CommitmentFactory::default(),
        )?)
    }

    /// Create partial commitment signature for the metadata, usually done by the receiver
    pub fn create_partial_metadata_signature(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        partial_commitment_nonce: &PublicKey,
        covenant: &Covenant,
        encrypted_value: &EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Result<ComSignature, TransactionError> {
        TransactionOutput::create_metadata_signature(
            version,
            value,
            spending_key,
            script,
            output_features,
            sender_offset_public_key,
            Some(partial_commitment_nonce),
            None,
            covenant,
            encrypted_value,
            minimum_value_promise,
        )
    }

    /// Create final commitment signature for the metadata, signing with both keys
    pub fn create_final_metadata_signature(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_private_key: &PrivateKey,
        covenant: &Covenant,
        encrypted_value: &EncryptedValue,
        minimum_value_promise: MicroTari,
    ) -> Result<ComSignature, TransactionError> {
        let sender_offset_public_key = PublicKey::from_secret_key(sender_offset_private_key);
        TransactionOutput::create_metadata_signature(
            version,
            value,
            spending_key,
            script,
            output_features,
            &sender_offset_public_key,
            None,
            Some(sender_offset_private_key),
            covenant,
            encrypted_value,
            minimum_value_promise,
        )
    }

    pub fn witness_hash(&self) -> FixedHash {
        DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("transaction_output_witness")
            .chain(&self.proof)
            .chain(&self.metadata_signature)
            .finalize()
            .into()
    }

    pub fn get_metadata_size(&self) -> usize {
        self.features.consensus_encode_exact_size() +
            self.script.consensus_encode_exact_size() +
            self.covenant.consensus_encode_exact_size()
    }
}

impl Default for TransactionOutput {
    fn default() -> Self {
        TransactionOutput::new_current_version(
            OutputFeatures::default(),
            CommitmentFactory::default().zero(),
            RangeProof::default(),
            TariScript::default(),
            PublicKey::default(),
            ComSignature::default(),
            Covenant::default(),
            EncryptedValue::default(),
            MicroTari::zero(),
        )
    }
}

impl Display for TransactionOutput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let proof = self.proof.to_hex();
        let proof = if proof.len() > 32 {
            format!("{}..{}", &proof[0..16], &proof[proof.len() - 16..proof.len()])
        } else {
            proof
        };
        write!(
            fmt,
            "{} [{:?}], Script: ({}), Offset Pubkey: ({}), Metadata Signature: ({}, {}, {}), Proof: {}",
            self.commitment.to_hex(),
            self.features,
            self.script,
            self.sender_offset_public_key.to_hex(),
            self.metadata_signature.u().to_hex(),
            self.metadata_signature.v().to_hex(),
            self.metadata_signature.public_nonce().to_hex(),
            proof
        )
    }
}

impl PartialOrd for TransactionOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.commitment.partial_cmp(&other.commitment)
    }
}

impl Ord for TransactionOutput {
    fn cmp(&self, other: &Self) -> Ordering {
        self.commitment.cmp(&other.commitment)
    }
}

impl ConsensusEncoding for TransactionOutput {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.version.consensus_encode(writer)?;
        self.features.consensus_encode(writer)?;
        self.commitment.consensus_encode(writer)?;
        self.proof.consensus_encode(writer)?;
        self.script.consensus_encode(writer)?;
        self.sender_offset_public_key.consensus_encode(writer)?;
        self.metadata_signature.consensus_encode(writer)?;
        self.covenant.consensus_encode(writer)?;
        self.encrypted_value.consensus_encode(writer)?;
        self.minimum_value_promise.consensus_encode(writer)?;
        Ok(())
    }
}
impl ConsensusEncodingSized for TransactionOutput {}

impl ConsensusDecoding for TransactionOutput {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let version = TransactionOutputVersion::consensus_decode(reader)?;
        let features = OutputFeatures::consensus_decode(reader)?;
        let commitment = Commitment::consensus_decode(reader)?;
        let proof = RangeProof::consensus_decode(reader)?;
        let script = TariScript::consensus_decode(reader)?;
        let sender_offset_public_key = PublicKey::consensus_decode(reader)?;
        let metadata_signature = ComSignature::consensus_decode(reader)?;
        let covenant = Covenant::consensus_decode(reader)?;
        let encrypted_value = EncryptedValue::consensus_decode(reader)?;
        let minimum_value_promise = MicroTari::consensus_decode(reader)?;
        let output = TransactionOutput::new(
            version,
            features,
            commitment,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
            covenant,
            encrypted_value,
            minimum_value_promise,
        );
        Ok(output)
    }
}

/// Performs batched range proof verification for an arbitrary number of outputs. Batched range proof verification gains
/// above batch sizes of 2^8 = 256 gives diminishing returns, see <https://github.com/tari-project/bulletproofs-plus>,
/// so the batch sizes are limited to 2^8.
pub fn batch_verify_range_proofs(
    prover: &RangeProofService,
    outputs: &[&TransactionOutput],
) -> Result<(), RangeProofError> {
    // We need optimized power of two chunks, for example if we have 15 outputs, then we need chunks of 8, 4, 2, 1.
    let power_of_two_vec = power_of_two_chunk_sizes(outputs.len(), 8);
    debug!(
        target: LOG_TARGET,
        "Queueing range proof batch verify output(s): {:?}", &power_of_two_vec
    );
    let mut index = 0;
    for power_of_two in power_of_two_vec {
        let mut statements = Vec::with_capacity(power_of_two);
        let mut proofs = Vec::with_capacity(power_of_two);
        for output in outputs.iter().skip(index).take(power_of_two) {
            statements.push(RistrettoAggregatedPublicStatement {
                statements: vec![Statement {
                    commitment: output.commitment.clone(),
                    minimum_value_promise: output.minimum_value_promise.into(),
                }],
            });
            proofs.push(output.proof.to_vec().clone());
        }
        index += power_of_two;
        prover.verify_batch(proofs.iter().collect(), statements.iter().collect())?;
    }
    Ok(())
}

// This function will create a vector of integers whose contents will all be powers of two; the entries will sum to the
// given length and each entry will be limited to the maximum power of two provided.
// Examples: A length of 15 without restrictions will produce chunks of [8, 4, 2, 1]; a length of 32 limited to 2^3 will
// produce chunks of [8, 8, 8, 8].
fn power_of_two_chunk_sizes(len: usize, max_power: u8) -> Vec<usize> {
    // This function will search for the highest power of two contained within an integer number
    fn highest_power_of_two(n: usize) -> usize {
        let mut res = 0;
        for i in (1..=n).rev() {
            if i.is_power_of_two() {
                res = i;
                break;
            }
        }
        res
    }

    if len == 0 {
        Vec::new()
    } else {
        let mut res_vec = Vec::new();
        let mut n = len;
        loop {
            let chunk = min(2usize.pow(u32::from(max_power)), highest_power_of_two(n));
            res_vec.push(chunk);
            n = n.saturating_sub(chunk);
            if n == 0 {
                break;
            }
        }
        res_vec
    }
}

#[cfg(test)]
mod test {
    use super::{batch_verify_range_proofs, TransactionOutput};
    use crate::{
        consensus::check_consensus_encoding_correctness,
        transactions::{
            tari_amount::MicroTari,
            test_helpers::{TestParams, UtxoTestParams},
            transaction_components::transaction_output::power_of_two_chunk_sizes,
            CryptoFactories,
        },
    };

    #[test]
    fn it_creates_power_of_two_chunks() {
        let p2vec = power_of_two_chunk_sizes(0, 7);
        assert!(p2vec.is_empty());
        let p2vec = power_of_two_chunk_sizes(1, 7);
        assert_eq!(p2vec, vec![1]);
        let p2vec = power_of_two_chunk_sizes(2, 7);
        assert_eq!(p2vec, vec![2]);
        let p2vec = power_of_two_chunk_sizes(3, 0);
        assert_eq!(p2vec, vec![1, 1, 1]);
        let p2vec = power_of_two_chunk_sizes(4, 2);
        assert_eq!(p2vec, vec![4]);
        let p2vec = power_of_two_chunk_sizes(15, 7);
        assert_eq!(p2vec, vec![8, 4, 2, 1]);
        let p2vec = power_of_two_chunk_sizes(32, 3);
        assert_eq!(p2vec, vec![8, 8, 8, 8]);
        let p2vec = power_of_two_chunk_sizes(1007, 8);
        assert_eq!(p2vec, vec![256, 256, 256, 128, 64, 32, 8, 4, 2, 1]);
        let p2vec = power_of_two_chunk_sizes(10307, 10);
        assert_eq!(p2vec, vec![
            1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 1024, 64, 2, 1
        ]);
    }

    #[test]
    fn it_builds_correctly_from_unblinded_output() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let value = MicroTari(10);
        let minimum_value_promise = MicroTari(10);
        let tx_output = create_valid_output(&test_params, &factories, value, minimum_value_promise);

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_ok());
        assert!(tx_output.verify_metadata_signature().is_ok());
        assert!(tx_output
            .verify_mask(&factories.range_proof, &test_params.spend_key, value.into())
            .is_ok());
    }

    #[test]
    fn it_does_not_verify_incorrect_minimum_value() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let value = MicroTari(10);
        let minimum_value_promise = MicroTari(11);
        let tx_output = create_invalid_output(&test_params, &factories, value, minimum_value_promise);

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_err());
    }

    #[test]
    fn it_does_batch_verify_correct_minimum_values() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let outputs = [
            &create_valid_output(&test_params, &factories, MicroTari(10), MicroTari::zero()),
            &create_valid_output(&test_params, &factories, MicroTari(10), MicroTari(5)),
            &create_valid_output(&test_params, &factories, MicroTari(10), MicroTari(10)),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[test]
    fn it_does_not_batch_verify_incorrect_minimum_values() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let outputs = [
            &create_valid_output(&test_params, &factories, MicroTari(10), MicroTari(10)),
            &create_invalid_output(&test_params, &factories, MicroTari(10), MicroTari(11)),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_err());
    }

    fn create_valid_output(
        test_params: &TestParams,
        factories: &CryptoFactories,
        value: MicroTari,
        minimum_value_promise: MicroTari,
    ) -> TransactionOutput {
        let utxo = test_params.create_unblinded_output(UtxoTestParams {
            value,
            minimum_value_promise,
            ..Default::default()
        });
        utxo.as_transaction_output(factories).unwrap()
    }

    fn create_invalid_output(
        test_params: &TestParams,
        factories: &CryptoFactories,
        value: MicroTari,
        minimum_value_promise: MicroTari,
    ) -> TransactionOutput {
        // we need first to create a valid minimum value, regardless of the minimum_value_promise
        // because this test function shoud allow creating an invalid proof for later testing
        let mut output = create_valid_output(test_params, factories, value, MicroTari::zero());

        // Now we can updated the minimum value, even to an invalid value
        output.minimum_value_promise = minimum_value_promise;

        output
    }

    #[test]
    fn consensus_encoding() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let output = create_valid_output(&test_params, &factories, 123.into(), MicroTari::zero());
        check_consensus_encoding_correctness(output).unwrap();
    }
}
