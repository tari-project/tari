//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    cmp::Ordering,
    fmt::{Display, Formatter},
};

use blake2::Digest;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    BlindingFactor,
    Challenge,
    ComSignature,
    Commitment,
    HashDigest,
    MessageHash,
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

use crate::transactions::{
    tari_amount::MicroTari,
    transaction::{
        full_rewind_result::FullRewindResult,
        rewind_result::RewindResult,
        OutputFeatures,
        OutputFlags,
        TransactionError,
        TransactionInput,
    },
};

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionOutput {
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
}

/// An output for a transaction, includes a range proof and Tari script metadata
impl TransactionOutput {
    /// Create new Transaction Output
    pub fn new(
        features: OutputFeatures,
        commitment: Commitment,
        proof: RangeProof,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComSignature,
    ) -> TransactionOutput {
        TransactionOutput {
            features,
            commitment,
            proof,
            script,
            sender_offset_public_key,
            metadata_signature,
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
        Ok(prover.verify(&self.proof.0, &self.commitment))
    }

    /// Verify that the metadata signature is valid
    pub fn verify_metadata_signature(&self) -> Result<(), TransactionError> {
        let challenge = TransactionOutput::build_metadata_signature_challenge(
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.public_nonce(),
            &self.commitment,
        );
        if !self.metadata_signature.verify_challenge(
            &(&self.commitment + &self.sender_offset_public_key),
            &challenge,
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
        self.commitment == output.commitment && self.features == output.features
    }

    /// Returns true if the output is a coinbase, otherwise false
    pub fn is_coinbase(&self) -> bool {
        self.features.flags.contains(OutputFlags::COINBASE_OUTPUT)
    }

    /// Convenience function that returns the challenge for the metadata commitment signature
    pub fn get_metadata_signature_challenge(&self, partial_commitment_nonce: Option<&PublicKey>) -> MessageHash {
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
        )
    }

    /// Convenience function that calculates the challenge for the metadata commitment signature
    pub fn build_metadata_signature_challenge(
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        public_commitment_nonce: &Commitment,
        commitment: &Commitment,
    ) -> MessageHash {
        Challenge::new()
            .chain(public_commitment_nonce.as_bytes())
            .chain(script.as_bytes())
            // TODO: Use consensus encoded bytes #testnet reset
            .chain(features.to_v1_bytes())
            .chain(sender_offset_public_key.as_bytes())
            .chain(commitment.as_bytes())
            .finalize()
            .to_vec()
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
        );
        let secret_x = match sender_offset_private_key {
            None => spending_key.clone(),
            Some(key) => &spending_key.clone() + key,
        };
        Ok(ComSignature::sign(
            value,
            secret_x,
            nonce_a,
            nonce_b,
            &e,
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
    ) -> Result<ComSignature, TransactionError> {
        TransactionOutput::create_metadata_signature(
            value,
            spending_key,
            script,
            output_features,
            sender_offset_public_key,
            Some(partial_commitment_nonce),
            None,
        )
    }

    /// Create final commitment signature for the metadata, signing with both keys
    pub fn create_final_metadata_signature(
        value: &MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_private_key: &PrivateKey,
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
        )
    }

    pub fn witness_hash(&self) -> Vec<u8> {
        HashDigest::new()
            .chain(self.proof.as_bytes())
            .chain(self.metadata_signature.u().as_bytes())
            .chain(self.metadata_signature.v().as_bytes())
            .chain(self.metadata_signature.public_nonce().as_bytes())
            .finalize()
            .to_vec()
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
            // TODO: use consensus encoding #testnetreset
            .chain(self.features.to_v1_bytes())
            .chain(self.commitment.as_bytes())
            // .chain(range proof) // See docs as to why we exclude this
            .chain(self.script.as_bytes())
            .finalize()
            .to_vec()
    }
}

// impl Default for TransactionOutput {
//     fn default() -> Self {
//         TransactionOutput::new(
//             OutputFeatures::default(),
//             CommitmentFactory::default().zero(),
//             RangeProof::default(),
//             TariScript::default(),
//             PublicKey::default(),
//         )
//     }
// }

impl Display for TransactionOutput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let proof = self.proof.to_hex();
        let proof = if proof.len() > 32 {
            format!(
                "{}..{}",
                proof[0..16].to_string(),
                proof[proof.len() - 16..proof.len()].to_string()
            )
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
