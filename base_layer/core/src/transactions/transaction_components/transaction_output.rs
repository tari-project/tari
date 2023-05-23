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

use borsh::{BorshDeserialize, BorshSerialize};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    BlindingFactor,
    ComAndPubSignature,
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
    borsh::SerializedSize,
    consensus::DomainSeparatedConsensusHasher,
    covenants::Covenant,
    transactions::{
        tari_amount::MicroTari,
        transaction_components,
        transaction_components::{
            EncryptedOpenings,
            OutputFeatures,
            OutputType,
            RangeProofType,
            TransactionError,
            TransactionInput,
        },
        TransactionHashDomain,
    },
};

pub const LOG_TARGET: &str = "c::transactions::transaction_output";

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded value for the output while the range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
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
    pub metadata_signature: ComAndPubSignature,
    /// The covenant that will be executed when spending this output
    #[serde(default)]
    pub covenant: Covenant,
    /// Encrypted value.
    pub encrypted_openings: EncryptedOpenings,
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
        metadata_signature: ComAndPubSignature,
        covenant: Covenant,
        encrypted_openings: EncryptedOpenings,
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
            encrypted_openings,
            minimum_value_promise,
        }
    }

    pub fn new_current_version(
        features: OutputFeatures,
        commitment: Commitment,
        proof: RangeProof,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        covenant: Covenant,
        encrypted_openings: EncryptedOpenings,
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
            encrypted_openings,
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

    /// Accessor method for the TariScript contained in an output
    pub fn script(&self) -> &TariScript {
        &self.script
    }

    pub fn hash(&self) -> FixedHash {
        transaction_components::hash_output(
            self.version,
            &self.features,
            &self.commitment,
            &self.script,
            &self.covenant,
            &self.encrypted_openings,
            &self.sender_offset_public_key,
            self.minimum_value_promise,
        )
    }

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<(), TransactionError> {
        match self.features.range_proof_type {
            RangeProofType::RevealedValue => match self.revealed_value_range_proof_check() {
                Ok(_) => Ok(()),
                Err(e) => Err(TransactionError::ValidationError(format!(
                    "Recipient output RevealedValue range proof for commitment {} failed to verify ({})",
                    self.commitment.to_hex(),
                    e
                ))),
            },
            RangeProofType::BulletProofPlus => {
                let statement = RistrettoAggregatedPublicStatement {
                    statements: vec![Statement {
                        commitment: self.commitment.clone(),
                        minimum_value_promise: self.minimum_value_promise.as_u64(),
                    }],
                };
                match prover.verify_batch(vec![&self.proof.0], vec![&statement]) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(TransactionError::ValidationError(format!(
                        "Recipient output BulletProofPlus range proof for commitment {} failed to verify ({})",
                        self.commitment.to_hex(),
                        e
                    ))),
                }
            },
        }
    }

    // As an alternate range proof check, the value of the commitment with a deterministic ephemeral_commitment nonce
    // `r_a` of zero can optionally be bound into the metadata signature. This is a much faster check than the full
    // range proof verification.
    fn revealed_value_range_proof_check(&self) -> Result<(), RangeProofError> {
        if self.features.range_proof_type != RangeProofType::RevealedValue {
            return Err(RangeProofError::InvalidRangeProof(format!(
                "Commitment {} does not have a RevealedValue range proof",
                self.commitment.to_hex()
            )));
        }
        let e_bytes = TransactionOutput::build_metadata_signature_challenge(
            self.version,
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.ephemeral_commitment(),
            self.metadata_signature.ephemeral_pubkey(),
            &self.commitment,
            &self.covenant,
            &self.encrypted_openings,
            self.minimum_value_promise,
        );
        let e = PrivateKey::from_bytes(&e_bytes).unwrap();
        let value_as_private_key = PrivateKey::from(self.minimum_value_promise.as_u64());
        let commit_nonce_a = PrivateKey::default(); // This is the deterministic nonce `r_a` of zero
        if self.metadata_signature.u_a().to_hex() == (commit_nonce_a + e * value_as_private_key).to_hex() {
            Ok(())
        } else {
            Err(RangeProofError::InvalidRangeProof(format!(
                "RevealedValue range proof check for commitment {} failed",
                self.commitment.to_hex()
            )))
        }
    }

    /// Verify that the metadata signature is valid
    pub fn verify_metadata_signature(&self) -> Result<(), TransactionError> {
        let challenge = TransactionOutput::build_metadata_signature_challenge(
            self.version,
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.ephemeral_commitment(),
            self.metadata_signature.ephemeral_pubkey(),
            &self.commitment,
            &self.covenant,
            &self.encrypted_openings,
            self.minimum_value_promise,
        );
        if !self.metadata_signature.verify_challenge(
            &self.commitment,
            &self.sender_offset_public_key,
            &challenge,
            &CommitmentFactory::default(),
            &mut OsRng,
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
            // TODO(SECURITY): Signing this with a blank msg allows the signature to be replayed. Using the commitment
            //                 is ideal as uniqueness is enforced. However, because the VN and wallet have different
            //                 keys this becomes difficult. Fix this once we have decided on a solution.
            if !validator_node_reg.is_valid_signature_for(&[]) {
                return Err(TransactionError::InvalidSignatureError(
                    "Validator node signature is not valid!".to_string(),
                ));
            }
        }
        Ok(())
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

    /// Convenience function that calculates the challenge for the metadata commitment signature
    pub fn build_metadata_signature_challenge(
        version: TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        ephemeral_commitment: &Commitment,
        ephemeral_pubkey: &PublicKey,
        commitment: &Commitment,
        covenant: &Covenant,
        encrypted_openings: &EncryptedOpenings,
        minimum_value_promise: MicroTari,
    ) -> [u8; 32] {
        // We build the message separately to help with hardware wallet support. This reduces the amount of data that
        // needs to be transferred in order to sign the signature.
        let message = TransactionOutput::build_metadata_signature_message(
            version,
            script,
            features,
            covenant,
            encrypted_openings,
            minimum_value_promise,
        );
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("metadata_signature")
            .chain(ephemeral_pubkey)
            .chain(ephemeral_commitment)
            .chain(sender_offset_public_key)
            .chain(commitment)
            .chain(&message);
        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize(),
        }
    }

    /// Convenience function to create the entire metadata signature message for the challenge. This contains all data
    /// outside of the signing keys and nonces.
    pub fn build_metadata_signature_message(
        version: TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        covenant: &Covenant,
        encrypted_openings: &EncryptedOpenings,
        minimum_value_promise: MicroTari,
    ) -> [u8; 32] {
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("metadata_message")
            .chain(&version)
            .chain(script)
            .chain(features)
            .chain(covenant)
            .chain(encrypted_openings)
            .chain(&minimum_value_promise);
        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize(),
        }
    }

    // Create partial commitment signature for the metadata for the receiver
    pub fn create_receiver_partial_metadata_signature(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        covenant: &Covenant,
        encrypted_openings: &EncryptedOpenings,
        minimum_value_promise: MicroTari,
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let nonce_a = TransactionOutput::nonce_a(range_proof_type, value, minimum_value_promise)?;
        let nonce_b = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = CommitmentFactory::default().commit(&nonce_b, &nonce_a);
        let pk_value = PrivateKey::from(value.as_u64());
        let commitment = CommitmentFactory::default().commit(spending_key, &pk_value);
        let e = TransactionOutput::build_metadata_signature_challenge(
            version,
            script,
            output_features,
            sender_offset_public_key,
            &ephemeral_commitment,
            ephemeral_pubkey,
            &commitment,
            covenant,
            encrypted_openings,
            minimum_value_promise,
        );
        Ok(ComAndPubSignature::sign(
            &pk_value,
            spending_key,
            &PrivateKey::default(),
            &nonce_a,
            &nonce_b,
            &PrivateKey::default(),
            &e,
            &CommitmentFactory::default(),
        )?)
    }

    // Create partial commitment signature for the metadata for the sender
    pub fn create_sender_partial_metadata_signature(
        version: TransactionOutputVersion,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_private_key: &PrivateKey,
        ephemeral_private_key: Option<&PrivateKey>,
        covenant: &Covenant,
        encrypted_openings: &EncryptedOpenings,
        minimum_value_promise: MicroTari,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let sender_offset_public_key = PublicKey::from_secret_key(sender_offset_private_key);
        let random_key = PrivateKey::random(&mut OsRng);
        let nonce = match ephemeral_private_key {
            Some(v) => v,
            None => &random_key,
        };
        let ephemeral_pubkey = PublicKey::from_secret_key(nonce);
        let e = TransactionOutput::build_metadata_signature_challenge(
            version,
            script,
            output_features,
            &sender_offset_public_key,
            ephemeral_commitment,
            &ephemeral_pubkey,
            commitment,
            covenant,
            encrypted_openings,
            minimum_value_promise,
        );
        Ok(ComAndPubSignature::sign(
            &PrivateKey::default(),
            &PrivateKey::default(),
            sender_offset_private_key,
            &PrivateKey::default(),
            &PrivateKey::default(),
            nonce,
            &e,
            &CommitmentFactory::default(),
        )?)
    }

    // With BulletProofPlus type range proofs, the nonce is a secure random value
    // With RevealedValue type range proofs, the nonce is always 0 and the minimum value promise equal to the value
    fn nonce_a(
        range_proof_type: RangeProofType,
        value: MicroTari,
        minimum_value_promise: MicroTari,
    ) -> Result<PrivateKey, TransactionError> {
        match range_proof_type {
            RangeProofType::BulletProofPlus => Ok(PrivateKey::random(&mut OsRng)),
            RangeProofType::RevealedValue => {
                if minimum_value_promise != value {
                    return Err(TransactionError::InvalidRevealedValue(format!(
                        "Expected {}, received {}",
                        value, minimum_value_promise
                    )));
                }
                Ok(PrivateKey::default())
            },
        }
    }

    // Create complete commitment signature if you are both the sender and receiver
    pub fn create_metadata_signature(
        version: TransactionOutputVersion,
        value: MicroTari,
        spending_key: &BlindingFactor,
        script: &TariScript,
        output_features: &OutputFeatures,
        sender_offset_private_key: &PrivateKey,
        covenant: &Covenant,
        encrypted_openings: &EncryptedOpenings,
        minimum_value_promise: MicroTari,
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let nonce_a = TransactionOutput::nonce_a(range_proof_type, value, minimum_value_promise)?;
        let nonce_b = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = CommitmentFactory::default().commit(&nonce_b, &nonce_a);
        let nonce_x = PrivateKey::random(&mut OsRng);
        let ephemeral_pubkey = PublicKey::from_secret_key(&nonce_x);
        let pk_value = PrivateKey::from(value.as_u64());
        let commitment = CommitmentFactory::default().commit(spending_key, &pk_value);
        let sender_offset_public_key = PublicKey::from_secret_key(sender_offset_private_key);
        let e = TransactionOutput::build_metadata_signature_challenge(
            version,
            script,
            output_features,
            &sender_offset_public_key,
            &ephemeral_commitment,
            &ephemeral_pubkey,
            &commitment,
            covenant,
            encrypted_openings,
            minimum_value_promise,
        );
        Ok(ComAndPubSignature::sign(
            &pk_value,
            spending_key,
            sender_offset_private_key,
            &nonce_a,
            &nonce_b,
            &nonce_x,
            &e,
            &CommitmentFactory::default(),
        )?)
    }

    pub fn witness_hash(&self) -> FixedHash {
        DomainSeparatedConsensusHasher::<TransactionHashDomain>::new("transaction_output_witness")
            .chain(&self.proof)
            .chain(&self.metadata_signature)
            .finalize()
            .into()
    }

    pub fn get_features_and_scripts_size(&self) -> usize {
        self.features.get_serialized_size() + self.script.get_serialized_size() + self.covenant.get_serialized_size()
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
            ComAndPubSignature::default(),
            Covenant::default(),
            EncryptedOpenings::default(),
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
            "{} [{:?}], Script: ({}), Offset Pubkey: ({}), Metadata Signature: ({}, {}, {}, {}, {}), Proof: {}",
            self.commitment.to_hex(),
            self.features,
            self.script,
            self.sender_offset_public_key.to_hex(),
            self.metadata_signature.u_a().to_hex(),
            self.metadata_signature.u_x().to_hex(),
            self.metadata_signature.u_y().to_hex(),
            self.metadata_signature.ephemeral_commitment().to_hex(),
            self.metadata_signature.ephemeral_pubkey().to_hex(),
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

/// Performs batched range proof verification for an arbitrary number of outputs
pub fn batch_verify_range_proofs(
    prover: &RangeProofService,
    outputs: &[&TransactionOutput],
) -> Result<(), RangeProofError> {
    let bulletproof_plus_proofs = outputs
        .iter()
        .filter(|o| o.features.range_proof_type == RangeProofType::BulletProofPlus)
        .copied()
        .collect::<Vec<&TransactionOutput>>();
    if !bulletproof_plus_proofs.is_empty() {
        let mut statements = Vec::with_capacity(bulletproof_plus_proofs.len());
        let mut proofs = Vec::with_capacity(bulletproof_plus_proofs.len());
        for output in &bulletproof_plus_proofs {
            statements.push(RistrettoAggregatedPublicStatement {
                statements: vec![Statement {
                    commitment: output.commitment.clone(),
                    minimum_value_promise: output.minimum_value_promise.into(),
                }],
            });
            proofs.push(output.proof.to_vec().clone());
        }
        if let Err(err_1) = prover.verify_batch(proofs.iter().collect(), statements.iter().collect()) {
            for output in &bulletproof_plus_proofs {
                if let Err(err_2) = output.verify_range_proof(prover) {
                    let proof = output.proof.to_hex();
                    let proof = if proof.len() > 32 {
                        format!("{}..{}", &proof[0..16], &proof[proof.len() - 16..proof.len()])
                    } else {
                        proof
                    };
                    return Err(RangeProofError::InvalidRangeProof(format!(
                        "commitment {}, minimum_value_promise {}, proof {} ({:?})",
                        output.commitment.to_hex(),
                        output.minimum_value_promise,
                        proof,
                        err_2,
                    )));
                }
            }
            Err(RangeProofError::InvalidRangeProof(format!(
                "Batch verification failed, but individual verification passed - {:?}",
                err_1
            )))?
        }
    }

    let revealed_value_proofs = outputs
        .iter()
        .filter(|o| o.features.range_proof_type == RangeProofType::RevealedValue)
        .copied()
        .collect::<Vec<&TransactionOutput>>();
    for output in revealed_value_proofs {
        output.revealed_value_range_proof_check()?;
    }

    // An empty batch is valid
    Ok(())
}

#[cfg(test)]
mod test {
    use super::{batch_verify_range_proofs, TransactionOutput};
    use crate::transactions::{
        tari_amount::MicroTari,
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::RangeProofType,
        CryptoFactories,
    };

    #[test]
    fn it_builds_correctly_from_unblinded_output() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let value = MicroTari(10);
        let minimum_value_promise = MicroTari(10);
        let tx_output = create_output(
            &test_params,
            &factories,
            value,
            minimum_value_promise,
            RangeProofType::BulletProofPlus,
        )
        .unwrap();

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
        let tx_output = create_invalid_output(
            &test_params,
            &factories,
            value,
            minimum_value_promise,
            RangeProofType::BulletProofPlus,
        );

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_err());
    }

    #[test]
    fn it_does_batch_verify_correct_minimum_values() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let outputs = [
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari::zero(),
                RangeProofType::BulletProofPlus,
            )
            .unwrap(),
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari(5),
                RangeProofType::BulletProofPlus,
            )
            .unwrap(),
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari(10),
                RangeProofType::BulletProofPlus,
            )
            .unwrap(),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[test]
    fn it_does_batch_verify_with_mixed_range_proof_types() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let outputs = [
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari::zero(),
                RangeProofType::BulletProofPlus,
            )
            .unwrap(),
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari(10),
                RangeProofType::RevealedValue,
            )
            .unwrap(),
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari::zero(),
                RangeProofType::BulletProofPlus,
            )
            .unwrap(),
            &create_output(
                &test_params,
                &factories,
                MicroTari(20),
                MicroTari(20),
                RangeProofType::RevealedValue,
            )
            .unwrap(),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[test]
    fn invalid_revealed_value_proofs_are_blocked() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();
        assert!(create_output(
            &test_params,
            &factories,
            MicroTari(20),
            MicroTari::zero(),
            RangeProofType::BulletProofPlus,
        )
        .is_ok());
        match create_output(
            &test_params,
            &factories,
            MicroTari(20),
            MicroTari::zero(),
            RangeProofType::RevealedValue,
        ) {
            Ok(_) => panic!("Should not have been able to create output"),
            Err(e) => assert_eq!(e, "InvalidRevealedValue(\"Expected 20 µT, received 0 µT\")"),
        }
    }

    #[test]
    fn it_does_not_batch_verify_incorrect_minimum_values() {
        let factories = CryptoFactories::default();
        let test_params = TestParams::new();

        let outputs = [
            &create_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari(10),
                RangeProofType::BulletProofPlus,
            )
            .unwrap(),
            &create_invalid_output(
                &test_params,
                &factories,
                MicroTari(10),
                MicroTari(11),
                RangeProofType::BulletProofPlus,
            ),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs).is_err());
    }

    fn create_output(
        test_params: &TestParams,
        factories: &CryptoFactories,
        value: MicroTari,
        minimum_value_promise: MicroTari,
        range_proof_type: RangeProofType,
    ) -> Result<TransactionOutput, String> {
        let utxo = test_params.create_unblinded_output_with_recovery_data(
            UtxoTestParams {
                value,
                minimum_value_promise,
                ..Default::default()
            },
            range_proof_type,
        );
        utxo?.as_transaction_output(factories, None).map_err(|e| e.to_string())
    }

    fn create_invalid_output(
        test_params: &TestParams,
        factories: &CryptoFactories,
        value: MicroTari,
        minimum_value_promise: MicroTari,
        range_proof_type: RangeProofType,
    ) -> TransactionOutput {
        // we need first to create a valid minimum value, regardless of the minimum_value_promise
        // because this test function should allow creating an invalid proof for later testing
        let mut output = create_output(test_params, factories, value, MicroTari::zero(), range_proof_type).unwrap();

        // Now we can updated the minimum value, even to an invalid value
        output.minimum_value_promise = minimum_value_promise;

        output
    }
}
