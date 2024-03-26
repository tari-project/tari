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

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use digest::consts::{U32, U64};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
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
    keys::SecretKey,
    ristretto::bulletproofs_plus::RistrettoAggregatedPublicStatement,
    tari_utilities::hex::Hex,
};
use tari_hashing::TransactionHashDomain;
use tari_script::TariScript;

use super::TransactionOutputVersion;
use crate::{
    borsh::SerializedSize,
    consensus::DomainSeparatedConsensusHasher,
    covenants::Covenant,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components,
        transaction_components::{
            EncryptedData,
            OutputFeatures,
            OutputType,
            RangeProofType,
            TransactionError,
            TransactionInput,
            WalletOutput,
        },
    },
};

/// Output for a transaction, defining the new ownership of coins that are being transferred. The commitment is a
/// blinded/masked value for the output while the range proof guarantees the commitment includes a positive value
/// without overflow and the ownership of the private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TransactionOutput {
    pub version: TransactionOutputVersion,
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commitment: Commitment,
    /// A proof that the commitment is in the right range
    pub proof: Option<RangeProof>,
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
    pub encrypted_data: EncryptedData,
    /// The minimum value of the commitment that is proven by the range proof
    #[serde(default)]
    pub minimum_value_promise: MicroMinotari,
}

/// An output for a transaction, includes a range proof and Tari script metadata
impl TransactionOutput {
    /// Create new Transaction Output

    pub fn new(
        version: TransactionOutputVersion,
        features: OutputFeatures,
        commitment: Commitment,
        proof: Option<RangeProof>,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
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
            encrypted_data,
            minimum_value_promise,
        }
    }

    pub fn new_current_version(
        features: OutputFeatures,
        commitment: Commitment,
        proof: Option<RangeProof>,
        script: TariScript,
        sender_offset_public_key: PublicKey,
        metadata_signature: ComAndPubSignature,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
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
            encrypted_data,
            minimum_value_promise,
        )
    }

    /// Accessor method for the commitment contained in an output
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    /// Accessor method for the encrypted_data contained in an output
    pub fn encrypted_data(&self) -> &EncryptedData {
        &self.encrypted_data
    }

    /// Accessor method for the range proof contained in an output
    pub fn proof_result(&self) -> Result<&RangeProof, RangeProofError> {
        if let Some(proof) = self.proof.as_ref() {
            Ok(proof)
        } else {
            Err(RangeProofError::InvalidRangeProof {
                reason: "Range proof not found".to_string(),
            })
        }
    }

    /// Accessor method for the range proof hex option display
    pub fn proof_hex_display(&self, full: bool) -> String {
        if let Some(proof) = self.proof.as_ref() {
            if full {
                "Some(".to_owned() + &proof.to_hex() + ")"
            } else {
                let proof_hex = proof.to_hex();
                if proof_hex.len() > 32 {
                    format!(
                        "Some({}..{})",
                        &proof_hex[0..16],
                        &proof_hex[proof_hex.len() - 16..proof_hex.len()]
                    )
                } else {
                    "Some(".to_owned() + &proof_hex + ")"
                }
            }
        } else {
            "None".to_string()
        }
    }

    /// Accessor method for the TariScript contained in an output
    pub fn script(&self) -> &TariScript {
        &self.script
    }

    pub fn hash(&self) -> FixedHash {
        let rp_hash = match &self.proof {
            Some(rp) => rp.hash(),
            None => FixedHash::zero(),
        };
        transaction_components::hash_output(
            self.version,
            &self.features,
            &self.commitment,
            &rp_hash,
            &self.script,
            &self.sender_offset_public_key,
            &self.metadata_signature,
            &self.covenant,
            &self.encrypted_data,
            self.minimum_value_promise,
        )
    }

    pub fn smt_hash(&self, mined_height: u64) -> FixedHash {
        let utxo_hash = self.hash();
        let smt_hash = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("smt_hash")
            .chain(&utxo_hash)
            .chain(&mined_height);

        match self.version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => smt_hash.finalize().into(),
        }
    }

    /// Verify that range proof is valid
    pub fn verify_range_proof(&self, prover: &RangeProofService) -> Result<(), TransactionError> {
        match self.features.range_proof_type {
            RangeProofType::RevealedValue => match self.revealed_value_range_proof_check() {
                Ok(_) => Ok(()),
                Err(e) => Err(TransactionError::RangeProofError(format!(
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
                match prover.verify_batch(vec![&self.proof_result()?.0], vec![&statement]) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(TransactionError::RangeProofError(format!(
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
            return Err(RangeProofError::InvalidRangeProof {
                reason: format!(
                    "Commitment {} does not have a RevealedValue range proof",
                    self.commitment.to_hex()
                ),
            });
        }
        // Let's first verify that the metadata signature is valid.
        // Note: If normal code paths are followed, this is checked elsewhere already, but it is theoretically possible
        //       to meddle with the metadata signature after it has been verified and before it is used here, so we
        //       check it again. It is also a very cheap test in comparison to a range proof verification
        let e_bytes = match self.verify_metadata_signature_internal() {
            Ok(val) => val,
            Err(e) => {
                return Err(RangeProofError::InvalidRangeProof {
                    reason: format!("{}", e),
                });
            },
        };
        // Now we can perform the balance proof
        let e = PrivateKey::from_uniform_bytes(&e_bytes).unwrap();
        let value_as_private_key = PrivateKey::from(self.minimum_value_promise.as_u64());
        let commit_nonce_a = PrivateKey::default(); // This is the deterministic nonce `r_a` of zero
        if self.metadata_signature.u_a() == &(commit_nonce_a + e * value_as_private_key) {
            Ok(())
        } else {
            Err(RangeProofError::InvalidRangeProof {
                reason: format!(
                    "RevealedValue range proof check for commitment {} failed",
                    self.commitment.to_hex()
                ),
            })
        }
    }

    fn verify_metadata_signature_internal(&self) -> Result<[u8; 64], TransactionError> {
        let challenge = TransactionOutput::build_metadata_signature_challenge(
            &self.version,
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.ephemeral_commitment(),
            self.metadata_signature.ephemeral_pubkey(),
            &self.commitment,
            &self.covenant,
            &self.encrypted_data,
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
        Ok(challenge)
    }

    /// Verify that the metadata signature is valid
    pub fn verify_metadata_signature(&self) -> Result<(), TransactionError> {
        let _challenge = self.verify_metadata_signature_internal()?;
        Ok(())
    }

    pub fn verify_validator_node_signature(&self) -> Result<(), TransactionError> {
        if let Some(validator_node_reg) = self
            .features
            .sidechain_feature
            .as_ref()
            .and_then(|f| f.validator_node_registration())
        {
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
        spending_key: &PrivateKey,
        value: u64,
    ) -> Result<bool, TransactionError> {
        Ok(prover.verify_mask(&self.commitment, spending_key, value)?)
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
        version: &TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &PublicKey,
        ephemeral_commitment: &Commitment,
        ephemeral_pubkey: &PublicKey,
        commitment: &Commitment,
        covenant: &Covenant,
        encrypted_data: &EncryptedData,
        minimum_value_promise: MicroMinotari,
    ) -> [u8; 64] {
        // We build the message separately to help with hardware wallet support. This reduces the amount of data that
        // needs to be transferred in order to sign the signature.
        let message = TransactionOutput::metadata_signature_message_from_parts(
            version,
            script,
            features,
            covenant,
            encrypted_data,
            &minimum_value_promise,
        );
        TransactionOutput::finalize_metadata_signature_challenge(
            version,
            sender_offset_public_key,
            ephemeral_commitment,
            ephemeral_pubkey,
            commitment,
            &message,
        )
    }

    pub fn finalize_metadata_signature_challenge(
        version: &TransactionOutputVersion,
        sender_offset_public_key: &PublicKey,
        ephemeral_commitment: &Commitment,
        ephemeral_pubkey: &PublicKey,
        commitment: &Commitment,
        message: &[u8; 32],
    ) -> [u8; 64] {
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U64>>::new("metadata_signature")
            .chain(ephemeral_pubkey)
            .chain(ephemeral_commitment)
            .chain(sender_offset_public_key)
            .chain(commitment)
            .chain(&message);
        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize().into(),
        }
    }

    /// Convenience function to get the entire metadata signature message for the challenge. This contains all data
    /// outside of the signing keys and nonces.
    pub fn metadata_signature_message(wallet_output: &WalletOutput) -> [u8; 32] {
        TransactionOutput::metadata_signature_message_from_parts(
            &wallet_output.version,
            &wallet_output.script,
            &wallet_output.features,
            &wallet_output.covenant,
            &wallet_output.encrypted_data,
            &wallet_output.minimum_value_promise,
        )
    }

    /// Convenience function to create the entire metadata signature message for the challenge. This contains all data
    /// outside of the signing keys and nonces.
    pub fn metadata_signature_message_from_parts(
        version: &TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        covenant: &Covenant,
        encrypted_data: &EncryptedData,
        minimum_value_promise: &MicroMinotari,
    ) -> [u8; 32] {
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message")
            .chain(version)
            .chain(script)
            .chain(features)
            .chain(covenant)
            .chain(encrypted_data)
            .chain(minimum_value_promise);
        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize().into(),
        }
    }

    pub fn get_features_and_scripts_size(&self) -> std::io::Result<usize> {
        Ok(self.features.get_serialized_size()? +
            self.script.get_serialized_size()? +
            self.covenant.get_serialized_size()?)
    }
}

impl Default for TransactionOutput {
    fn default() -> Self {
        TransactionOutput::new_current_version(
            OutputFeatures::default(),
            CommitmentFactory::default().zero(),
            Some(RangeProof::default()),
            TariScript::default(),
            PublicKey::default(),
            ComAndPubSignature::default(),
            Covenant::default(),
            EncryptedData::default(),
            MicroMinotari::zero(),
        )
    }
}

impl Display for TransactionOutput {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "({}, {}) [{:?}], Script: ({}), Offset Pubkey: ({}), Metadata Signature: ({}, {}, {}, {}, {}), Encrypted \
             data ({}), Proof: {}",
            self.commitment.to_hex(),
            self.hash(),
            self.features,
            self.script,
            self.sender_offset_public_key.to_hex(),
            self.metadata_signature.u_a().to_hex(),
            self.metadata_signature.u_x().to_hex(),
            self.metadata_signature.u_y().to_hex(),
            self.metadata_signature.ephemeral_commitment().to_hex(),
            self.metadata_signature.ephemeral_pubkey().to_hex(),
            self.encrypted_data.hex_display(false),
            self.proof_hex_display(false),
        )
    }
}

impl PartialOrd for TransactionOutput {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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
            proofs.push(output.proof_result()?.as_vec());
        }

        // Attempt to verify the range proofs in a batch
        prover.verify_batch(proofs, statements.iter().collect())?;
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
    use tari_crypto::errors::RangeProofError;

    use super::{batch_verify_range_proofs, TransactionOutput};
    use crate::transactions::{
        key_manager::{create_memory_db_key_manager, MemoryDbKeyManager, TransactionKeyManagerInterface},
        tari_amount::MicroMinotari,
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::{OutputFeatures, RangeProofType},
        CryptoFactories,
    };

    #[tokio::test]
    async fn it_builds_correctly() {
        let factories = CryptoFactories::default();
        let key_manager = create_memory_db_key_manager();
        let test_params = TestParams::new(&key_manager).await;

        let value = MicroMinotari(10);
        let minimum_value_promise = MicroMinotari(10);
        let tx_output = create_output(
            &test_params,
            value,
            minimum_value_promise,
            RangeProofType::BulletProofPlus,
            &key_manager,
        )
        .await
        .unwrap();

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_ok());
        assert!(tx_output.verify_metadata_signature().is_ok());
        let (_, recovered_value) = key_manager.try_output_key_recovery(&tx_output, None).await.unwrap();
        assert_eq!(recovered_value, value);
    }

    #[tokio::test]
    async fn it_does_not_verify_incorrect_minimum_value() {
        let factories = CryptoFactories::default();
        let key_manager = create_memory_db_key_manager();
        let test_params = TestParams::new(&key_manager).await;

        let value = MicroMinotari(10);
        let minimum_value_promise = MicroMinotari(11);
        let tx_output = create_invalid_output(
            &test_params,
            value,
            minimum_value_promise,
            RangeProofType::BulletProofPlus,
            &key_manager,
        )
        .await;

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_err());
    }

    #[tokio::test]
    async fn it_does_batch_verify_correct_minimum_values() {
        let factories = CryptoFactories::default();
        let key_manager = create_memory_db_key_manager();
        let test_params = TestParams::new(&key_manager).await;

        let outputs = [
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(5),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(10),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await
            .unwrap(),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[tokio::test]
    async fn it_does_batch_verify_with_mixed_range_proof_types() {
        let key_manager = create_memory_db_key_manager();
        let factories = CryptoFactories::default();
        let test_params = TestParams::new(&key_manager).await;

        let outputs = [
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(10),
                RangeProofType::RevealedValue,
                &key_manager,
            )
            .await
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(20),
                MicroMinotari(20),
                RangeProofType::RevealedValue,
                &key_manager,
            )
            .await
            .unwrap(),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[tokio::test]
    async fn invalid_revealed_value_proofs_are_blocked() {
        let key_manager = create_memory_db_key_manager();
        let test_params = TestParams::new(&key_manager).await;
        assert!(create_output(
            &test_params,
            MicroMinotari(20),
            MicroMinotari::zero(),
            RangeProofType::BulletProofPlus,
            &key_manager
        )
        .await
        .is_ok());
        match create_output(
            &test_params,
            MicroMinotari(20),
            MicroMinotari::zero(),
            RangeProofType::RevealedValue,
            &key_manager,
        )
        .await
        {
            Ok(_) => panic!("Should not have been able to create output"),
            Err(e) => assert_eq!(
                e,
                "A range proof construction or verification has produced an error: Invalid revealed value: Expected \
                 20 µT, received 0 µT"
            ),
        }
    }

    #[tokio::test]
    async fn revealed_value_proofs_only_succeed_with_valid_metadata_signatures() {
        let key_manager = create_memory_db_key_manager();
        let test_params = TestParams::new(&key_manager).await;
        let mut output = create_output(
            &test_params,
            MicroMinotari(20),
            MicroMinotari(20),
            RangeProofType::RevealedValue,
            &key_manager,
        )
        .await
        .unwrap();
        assert!(output.verify_metadata_signature().is_ok());
        assert!(output.revealed_value_range_proof_check().is_ok());

        output.features.maturity += 1;
        assert!(output.verify_metadata_signature().is_err());
        match output.revealed_value_range_proof_check() {
            Ok(_) => panic!("Should not have passed check"),
            Err(e) => assert_eq!(e, RangeProofError::InvalidRangeProof {
                reason: "Signature is invalid: Metadata signature not valid!".to_string()
            }),
        }
    }

    #[tokio::test]
    async fn it_does_not_batch_verify_incorrect_minimum_values() {
        let factories = CryptoFactories::default();
        let key_manager = create_memory_db_key_manager();
        let test_params = TestParams::new(&key_manager).await;

        let outputs = [
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(10),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await
            .unwrap(),
            &create_invalid_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(11),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .await,
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs).is_err());
    }

    async fn create_output(
        test_params: &TestParams,
        value: MicroMinotari,
        minimum_value_promise: MicroMinotari,
        range_proof_type: RangeProofType,
        key_manager: &MemoryDbKeyManager,
    ) -> Result<TransactionOutput, String> {
        let utxo = test_params
            .create_output(
                UtxoTestParams {
                    value,
                    minimum_value_promise,
                    features: OutputFeatures {
                        range_proof_type,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                key_manager,
            )
            .await;
        utxo?
            .to_transaction_output(key_manager)
            .await
            .map_err(|e| e.to_string())
    }

    async fn create_invalid_output(
        test_params: &TestParams,
        value: MicroMinotari,
        minimum_value_promise: MicroMinotari,
        range_proof_type: RangeProofType,
        key_manager: &MemoryDbKeyManager,
    ) -> TransactionOutput {
        // we need first to create a valid minimum value, regardless of the minimum_value_promise
        // because this test function should allow creating an invalid proof for later testing
        let mut output = create_output(test_params, value, MicroMinotari::zero(), range_proof_type, key_manager)
            .await
            .unwrap();

        // Now we can updated the minimum value, even to an invalid value
        output.minimum_value_promise = minimum_value_promise;

        output
    }
}
