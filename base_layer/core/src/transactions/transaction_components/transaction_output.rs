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
    cmp::Ordering,
    fmt::{Display, Formatter},
};

use digest::{Digest, FixedOutput};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    BlindingFactor,
    Challenge,
    ComSignature,
    Commitment,
    CommitmentFactory,
    HashDigest,
    PrivateKey,
    PublicKey,
    RangeProof,
    RangeProofService,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService as RangeProofServiceTrait,
    ristretto::pedersen::PedersenCommitmentFactory,
    script::TariScript,
    tari_utilities::{hex::Hex, ByteArray, Hashable},
};

use super::TransactionOutputVersion;
use crate::{
    common::hash_writer::HashWriter,
    consensus::{ConsensusEncoding, ConsensusEncodingSized, ToConsensusBytes},
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components,
        transaction_components::{
            full_rewind_result::FullRewindResult,
            rewind_result::RewindResult,
            OutputFeatures,
            OutputFlags,
            TransactionError,
            TransactionInput,
        },
    },
};

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
    /// The script that will be executed when spending this output
    #[serde(default)]
    pub covenant: Covenant,
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

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<(), TransactionError> {
        if prover.verify(&self.proof.0, &self.commitment) {
            Ok(())
        } else {
            Err(TransactionError::ValidationError(
                "Recipient output range proof failed to verify".to_string(),
            ))
        }
    }

    /// Verify that the metadata signature is valid
    pub fn verify_metadata_signature(&self) -> Result<(), TransactionError> {
        let challenge = TransactionOutput::build_metadata_signature_challenge(
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.public_nonce(),
            &self.commitment,
            &self.covenant,
        );
        if !self.metadata_signature.verify_challenge(
            &(&self.commitment + &self.sender_offset_public_key),
            &challenge.finalize_fixed(),
            &PedersenCommitmentFactory::default(),
        ) {
            return Err(TransactionError::InvalidSignatureError(
                "Metadata signature not valid!".to_string(),
            ));
        }
        Ok(())
    }

    /// Attempt to rewind the range proof to reveal the proof message and committed value
    pub fn rewind_range_proof_value_only(
        &self,
        prover: &RangeProofService,
        rewind_public_key: &PublicKey,
        rewind_blinding_public_key: &PublicKey,
    ) -> Result<RewindResult, TransactionError> {
        Ok(prover
            .rewind_proof_value_only(
                &self.proof.0,
                &self.commitment,
                rewind_public_key,
                rewind_blinding_public_key,
            )?
            .into())
    }

    /// Attempt to fully rewind the range proof to reveal the proof message, committed value and blinding factor
    pub fn full_rewind_range_proof(
        &self,
        prover: &RangeProofService,
        rewind_key: &PrivateKey,
        rewind_blinding_key: &PrivateKey,
    ) -> Result<FullRewindResult, TransactionError> {
        Ok(prover
            .rewind_proof_commitment_data(&self.proof.0, &self.commitment, rewind_key, rewind_blinding_key)?
            .into())
    }

    /// This will check if the input and the output is the same commitment by looking at the commitment and features.
    /// This will ignore the output range proof
    #[inline]
    pub fn is_equal_to(&self, output: &TransactionInput) -> bool {
        self.hash() == output.output_hash()
    }

    /// Returns true if the output is a coinbase, otherwise false
    pub fn is_coinbase(&self) -> bool {
        self.features.flags.contains(OutputFlags::COINBASE_OUTPUT)
    }

    /// Convenience function that returns the challenge for the metadata commitment signature
    pub fn get_metadata_signature_challenge(&self, partial_commitment_nonce: Option<&PublicKey>) -> Challenge {
        let nonce_commitment = match partial_commitment_nonce {
            None => self.metadata_signature.public_nonce().clone(),
            Some(partial_nonce) => self.metadata_signature.public_nonce() + partial_nonce,
        };
        TransactionOutput::build_metadata_signature_challenge(
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            &nonce_commitment,
            &self.commitment,
            &self.covenant,
        )
    }

    /// Convenience function that calculates the challenge for the metadata commitment signature
    pub fn build_metadata_signature_challenge(
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        public_commitment_nonce: &Commitment,
        commitment: &Commitment,
        covenant: &Covenant,
    ) -> Challenge {
        Challenge::new()
            .chain(public_commitment_nonce.to_consensus_bytes())
            .chain(script.to_consensus_bytes())
            .chain(features.to_consensus_bytes())
            .chain(sender_offset_public_key.to_consensus_bytes())
            .chain(commitment.to_consensus_bytes())
            .chain(covenant.to_consensus_bytes())
    }

    // Create commitment signature for the metadata

    fn create_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        partial_commitment_nonce: Option<&PublicKey>,
        sender_offset_private_key: Option<&PrivateKey>,
        covenant: &Covenant,
    ) -> Result<ComSignature, TransactionError> {
        let nonce_a = PrivateKey::random(&mut OsRng);
        let nonce_b = PrivateKey::random(&mut OsRng);
        let nonce_commitment = PedersenCommitmentFactory::default().commit(&nonce_b, &nonce_a);
        let nonce_commitment = match partial_commitment_nonce {
            None => nonce_commitment,
            Some(partial_nonce) => &nonce_commitment + partial_nonce,
        };
        let value = PrivateKey::from(value.as_u64());
        let commitment = PedersenCommitmentFactory::default().commit(spending_key, &value);
        let e = TransactionOutput::build_metadata_signature_challenge(
            script,
            output_features,
            sender_offset_public_key,
            &nonce_commitment,
            &commitment,
            covenant,
        );
        let secret_x = match sender_offset_private_key {
            None => spending_key.clone(),
            Some(key) => spending_key + key,
        };
        Ok(ComSignature::sign(
            value,
            secret_x,
            nonce_a,
            nonce_b,
            &e.finalize_fixed(),
            &PedersenCommitmentFactory::default(),
        )?)
    }

    /// Create partial commitment signature for the metadata, usually done by the receiver
    pub fn create_partial_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        partial_commitment_nonce: &PublicKey,
        covenant: &Covenant,
    ) -> Result<ComSignature, TransactionError> {
        TransactionOutput::create_metadata_signature(
            value,
            spending_key,
            script,
            output_features,
            sender_offset_public_key,
            Some(partial_commitment_nonce),
            None,
            covenant,
        )
    }

    /// Create final commitment signature for the metadata, signing with both keys
    pub fn create_final_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_private_key: &PrivateKey,
        covenant: &Covenant,
    ) -> Result<ComSignature, TransactionError> {
        let sender_offset_public_key = PublicKey::from_secret_key(sender_offset_private_key);
        TransactionOutput::create_metadata_signature(
            value,
            spending_key,
            script,
            output_features,
            &sender_offset_public_key,
            None,
            Some(sender_offset_private_key),
            covenant,
        )
    }

    pub fn witness_hash(&self) -> Vec<u8> {
        let mut hasher = HashWriter::new(HashDigest::new());
        // unwrap: HashWriter is infallible
        self.proof.consensus_encode(&mut hasher).unwrap();
        self.metadata_signature.consensus_encode(&mut hasher).unwrap();

        hasher.finalize().to_vec()
    }

    pub fn get_metadata_size(&self) -> usize {
        self.features.consensus_encode_exact_size() +
            self.script.consensus_encode_exact_size() +
            self.covenant.consensus_encode_exact_size()
    }
}

/// Implement the canonical hashing function for TransactionOutput for use in ordering.
impl Hashable for TransactionOutput {
    fn hash(&self) -> Vec<u8> {
        transaction_components::hash_output(
            self.version,
            &self.features,
            &self.commitment,
            &self.script,
            &self.covenant,
        )
        .to_vec()
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
