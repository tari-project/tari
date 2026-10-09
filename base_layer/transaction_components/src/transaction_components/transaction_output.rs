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
use serde::{Deserialize, Serialize};
use tari_common_types::types::{
    ComAndPubSignature,
    CommitmentFactory,
    CompressedCommitment,
    CompressedPublicKey,
    FixedHash,
    PrivateKey,
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
    MicroMinotari,
    consensus::DomainSeparatedConsensusHasher,
    helpers::borsh::SerializedSize,
    transaction_components,
    transaction_components::{
        EncryptedData,
        OutputFeatures,
        OutputType,
        RangeProofType,
        TransactionError,
        TransactionInput,
        WalletOutput,
        covenants::Covenant,
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
    pub commitment: CompressedCommitment,
    /// A proof that the commitment is in the right range
    pub proof: Option<RangeProof>,
    /// The script that will be executed when spending this output
    pub script: TariScript,
    /// Tari script offset pubkey, K_O
    pub sender_offset_public_key: CompressedPublicKey,
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
        commitment: CompressedCommitment,
        proof: Option<RangeProof>,
        script: TariScript,
        sender_offset_public_key: CompressedPublicKey,
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
        commitment: CompressedCommitment,
        proof: Option<RangeProof>,
        script: TariScript,
        sender_offset_public_key: CompressedPublicKey,
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
    pub fn commitment(&self) -> &CompressedCommitment {
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
                        &proof_hex[proof_hex.len().saturating_sub(16)..proof_hex.len()]
                    )
                } else {
                    "Some(".to_owned() + &proof_hex + ")"
                }
            }
        } else {
            format!("None({})", self.minimum_value_promise)
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

    // Verify that range proof is valid
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
                        commitment: self.commitment.to_commitment()?,
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
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn revealed_value_range_proof_check(&self) -> Result<(), RangeProofError> {
        if self.features.range_proof_type != RangeProofType::RevealedValue {
            return Err(RangeProofError::InvalidRangeProof {
                reason: format!(
                    "Commitment {} does not have a RevealedValue range proof",
                    self.commitment.to_hex()
                ),
            });
        }
        // NOTE: The metadata signature must also be verified elsewhere
        let e_bytes = self.get_metadata_signature_challenge();
        // Now we can perform the balance proof
        // `get_metadata_signature_challenge` returns exactly the 64 bytes wide reduction needs, so this cannot
        // currently fail. Map it rather than unwrap so that a future change to the challenge width surfaces as an
        // invalid range proof instead of taking the node down.
        let e = PrivateKey::from_uniform_bytes(&e_bytes).map_err(|e| RangeProofError::InvalidRangeProof {
            reason: format!(
                "Could not construct the metadata signature challenge scalar for commitment {}: {}",
                self.commitment.to_hex(),
                e
            ),
        })?;
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

    fn get_metadata_signature_challenge(&self) -> [u8; 64] {
        TransactionOutput::build_metadata_signature_challenge(
            self.version,
            &self.script,
            &self.features,
            &self.sender_offset_public_key,
            self.metadata_signature.ephemeral_commitment(),
            self.metadata_signature.ephemeral_pubkey(),
            &self.commitment,
            &self.covenant,
            &self.encrypted_data,
            self.minimum_value_promise,
        )
    }

    fn verify_metadata_signature_internal(&self) -> Result<[u8; 64], TransactionError> {
        let challenge = self.get_metadata_signature_challenge();

        if !self.metadata_signature.to_capk_signature()?.verify_challenge(
            &self.commitment.to_commitment()?,
            &self.sender_offset_public_key.to_public_key()?,
            &challenge,
            &CommitmentFactory::default(),
            &mut rand::rng(),
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

    /// Verify a recovered mask (blinding factor) for a proof against the commitment. Returns
    /// `Err(TransactionError::InvalidMask)` if the commitment does not open to the value under the mask.
    pub fn verify_mask(
        &self,
        prover: &RangeProofService,
        commitment_mask_key: &PrivateKey,
        value: u64,
    ) -> Result<(), TransactionError> {
        prover
            .verify_mask(&self.commitment.to_commitment()?, commitment_mask_key, value)
            .map_err(Into::into)
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

    pub fn is_burned_to_sidechain(&self) -> bool {
        self.is_burned() && self.features.sidechain_feature.is_some()
    }

    /// Convenience function that calculates the challenge for the metadata commitment signature
    pub fn build_metadata_signature_challenge(
        version: TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_commitment: &CompressedCommitment,
        ephemeral_pubkey: &CompressedPublicKey,
        commitment: &CompressedCommitment,
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
        version: TransactionOutputVersion,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_commitment: &CompressedCommitment,
        ephemeral_pubkey: &CompressedPublicKey,
        commitment: &CompressedCommitment,
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
            wallet_output.version(),
            wallet_output.script(),
            wallet_output.features(),
            wallet_output.covenant(),
            wallet_output.encrypted_data(),
            &wallet_output.minimum_value_promise(),
        )
    }

    /// Convenience function to create the entire metadata signature message for the challenge. This contains all data
    /// outside of the signing keys and nonces.
    pub fn metadata_signature_message_from_parts(
        version: TransactionOutputVersion,
        script: &TariScript,
        features: &OutputFeatures,
        covenant: &Covenant,
        encrypted_data: &EncryptedData,
        minimum_value_promise: &MicroMinotari,
    ) -> [u8; 32] {
        // NOTE: the "metadata_message" label (TransactionHashDomain) is shared by the "common" hash (version, features,
        // covenant, encrypted data, minimum value promise) and the outer hash (script || common), two different
        // preimage shapes separated only by length/layout today. Give each a distinct label at the next hard fork.
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message")
            .chain(&version)
            .chain(features)
            .chain(covenant)
            .chain(encrypted_data)
            .chain(minimum_value_promise);
        let common: [u8; 32] = match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize().into(),
        };

        let total = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message")
            .chain(&script)
            .chain(&common);

        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => total.finalize().into(),
        }
    }

    pub fn metadata_signature_message_common_from_parts(
        version: &TransactionOutputVersion,
        features: &OutputFeatures,
        covenant: &Covenant,
        encrypted_data: &EncryptedData,
        minimum_value_promise: &MicroMinotari,
    ) -> [u8; 32] {
        // NOTE: the "metadata_message" label (TransactionHashDomain) is shared by the "common" hash (version, features,
        // covenant, encrypted data, minimum value promise) and the outer hash (script || common), two different
        // preimage shapes separated only by length/layout today. Give each a distinct label at the next hard fork.
        let common = DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message")
            .chain(version)
            .chain(features)
            .chain(covenant)
            .chain(encrypted_data)
            .chain(minimum_value_promise);
        match version {
            TransactionOutputVersion::V0 | TransactionOutputVersion::V1 => common.finalize().into(),
        }
    }

    /// The preimage of [`Self::metadata_signature_message_common_from_parts`]: the borsh encodings of the five fields,
    /// back to back, exactly as that hasher writes them. A ledger device is sent this, rather than the hash, so that it
    /// can read the fields it is about to sign for and hash them itself; see
    /// `minotari_ledger_wallet_common::metadata_output`.
    pub fn metadata_signature_message_common_preimage(
        version: &TransactionOutputVersion,
        features: &OutputFeatures,
        covenant: &Covenant,
        encrypted_data: &EncryptedData,
        minimum_value_promise: &MicroMinotari,
    ) -> std::io::Result<Vec<u8>> {
        let mut preimage = Vec::new();
        BorshSerialize::serialize(version, &mut preimage)?;
        BorshSerialize::serialize(features, &mut preimage)?;
        BorshSerialize::serialize(covenant, &mut preimage)?;
        BorshSerialize::serialize(encrypted_data, &mut preimage)?;
        BorshSerialize::serialize(minimum_value_promise, &mut preimage)?;
        Ok(preimage)
    }

    pub fn metadata_signature_message_from_script_and_common(script: &TariScript, common: &[u8; 32]) -> [u8; 32] {
        // NOTE: the "metadata_message" label (TransactionHashDomain) is shared by the "common" hash (version, features,
        // covenant, encrypted data, minimum value promise) and the outer hash (script || common), two different
        // preimage shapes separated only by length/layout today. Give each a distinct label at the next hard fork.
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message")
            .chain(&script)
            .chain(common)
            .finalize()
            .into()
    }

    pub fn get_features_and_scripts_size(&self) -> std::io::Result<usize> {
        Ok(self
            .features
            .get_serialized_size()?
            .saturating_add(self.script.get_serialized_size()?)
            .saturating_add(self.covenant.get_serialized_size()?)
            .saturating_add(self.encrypted_data.get_payment_id_size()))
    }
}

impl Default for TransactionOutput {
    fn default() -> Self {
        TransactionOutput::new_current_version(
            OutputFeatures::default(),
            CompressedCommitment::from_commitment(CommitmentFactory::default().zero()),
            Some(RangeProof::default()),
            TariScript::default(),
            CompressedPublicKey::default(),
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
// /// Performs batched range proof verification for an arbitrary number of outputs
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
                    commitment: output
                        .commitment
                        .to_commitment()
                        .map_err(|_e| RangeProofError::InvalidRangeProof {
                            reason: "Invalid commitment".to_string(),
                        })?,
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
mod metadata_preimage_test {
    //! Parity between the host's consensus encoding of the metadata signature's common fields and what a ledger device
    //! reads and hashes from them: `minotari_ledger_wallet_common::metadata_output` walks the preimage with its own
    //! grammar, and the device hashes the raw preimage bytes. Either drifting from the consensus types makes every
    //! signature the device produces invalid, or worse, lets it read a field from a different offset than consensus
    //! does - so every feature variant a ledger wallet can meet is checked here against the real types.

    use blake2::Blake2b;
    use borsh::{BorshSerialize, io};
    use digest::consts::U32;
    use minotari_ledger_wallet_common::metadata_output::{
        MetadataPreimageError,
        SideChainKind,
        parse_metadata_preimage,
    };
    use tari_common_types::{
        epoch::VnEpoch,
        types::{CompressedPublicKey, CompressedSignature, FixedHash, PrivateKey},
    };
    use tari_crypto::keys::SecretKey;
    use tari_hashing::TransactionHashDomain;
    use tari_max_size::{MaxSizeBytes, MaxSizeString};
    use tari_utilities::ByteArray;

    use super::TransactionOutput;
    use crate::{
        MicroMinotari,
        consensus::DomainSeparatedConsensusHasher,
        transaction_components::{
            CodeTemplateRegistration,
            EncryptedData,
            OutputFeatures,
            OutputFeaturesVersion,
            OutputType,
            RangeProofType,
            SideChainFeature,
            SideChainFeatureData,
            TransactionOutputVersion,
            ValidatorNodeSignature,
            covenants::Covenant,
            encrypted_data::MAX_ENCRYPTED_DATA_SIZE,
            side_chain::{BuildInfo, ConfidentialOutputData, TemplateType},
        },
    };

    /// Raw bytes, written to a borsh writer with no length prefix - what the device's hasher does with the preimage.
    struct Raw<'a>(&'a [u8]);

    impl BorshSerialize for Raw<'_> {
        fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
            writer.write_all(self.0)
        }
    }

    fn key() -> CompressedPublicKey {
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng()))
    }

    fn signature() -> CompressedSignature {
        CompressedSignature::new(key(), PrivateKey::random(&mut rand::rng()))
    }

    fn encrypted_data(size: usize) -> EncryptedData {
        EncryptedData::from_bytes(&vec![0x5a; size]).unwrap()
    }

    fn vn_registration(sidechain_key: Option<&PrivateKey>) -> (OutputFeatures, CompressedPublicKey) {
        let vn_key = PrivateKey::random(&mut rand::rng());
        let claim = key();
        let signature = ValidatorNodeSignature::sign_for_registration(&vn_key, 0x26, None, &claim, VnEpoch(9));
        let vn_public_key = signature.public_key().clone();
        (
            OutputFeatures::for_validator_node_registration(signature, claim, sidechain_key, VnEpoch(9)),
            vn_public_key,
        )
    }

    fn code_template_registration() -> OutputFeatures {
        let registration = CodeTemplateRegistration {
            author_public_key: key(),
            author_signature: signature(),
            template_name: MaxSizeString::try_from("t".repeat(32)).unwrap(),
            template_version: 7,
            template_type: TemplateType::Wasm { abi_version: 3 },
            build_info: BuildInfo {
                repo_url: MaxSizeString::try_from("r".repeat(255)).unwrap(),
                commit_hash: MaxSizeBytes::try_from(vec![0xc0; 32]).unwrap(),
            },
            binary_sha: FixedHash::zero(),
            binary_url: MaxSizeString::try_from("b".repeat(255)).unwrap(),
        };
        let sidechain_id = Some(crate::transaction_components::SideChainId::new(key(), signature()));
        OutputFeatures::new_current_version(
            OutputType::CodeTemplateRegistration,
            0,
            Default::default(),
            Some(SideChainFeature {
                data: SideChainFeatureData::CodeTemplateRegistration(registration),
                sidechain_id,
            }),
            RangeProofType::BulletProofPlus,
        )
    }

    fn preimage(
        version: TransactionOutputVersion,
        features: &OutputFeatures,
        encrypted_data: &EncryptedData,
        minimum_value_promise: u64,
    ) -> Vec<u8> {
        TransactionOutput::metadata_signature_message_common_preimage(
            &version,
            features,
            &Covenant::default(),
            encrypted_data,
            &MicroMinotari(minimum_value_promise),
        )
        .unwrap()
    }

    /// The device's `common`: the raw preimage under the same domain separated hasher.
    fn device_common(preimage: &[u8]) -> [u8; 32] {
        DomainSeparatedConsensusHasher::<TransactionHashDomain, Blake2b<U32>>::new("metadata_message")
            .chain(&Raw(preimage))
            .finalize()
            .into()
    }

    /// Every feature shape a ledger wallet can meet, under both output versions, with the smallest and the largest
    /// encrypted data: the raw preimage hashes to exactly the consensus `common`, and the shared parser reads the
    /// fields the consensus types hold.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn the_device_reads_and_hashes_every_output_shape_as_consensus_does() {
        let (vn_registration_plain, vn_key_plain) = vn_registration(None);
        let (vn_registration_with_id, vn_key_with_id) = vn_registration(Some(&PrivateKey::random(&mut rand::rng())));
        let vn_exit_signature = ValidatorNodeSignature::new(key(), signature());
        let vn_exit_key = vn_exit_signature.public_key().clone();
        let vn_exit = OutputFeatures::for_validator_node_exit(vn_exit_signature, None, VnEpoch(3), VnEpoch(4));
        let confidential = OutputFeatures::new_current_version(
            OutputType::Standard,
            0,
            Default::default(),
            Some(SideChainFeature {
                data: SideChainFeatureData::ConfidentialOutput(ConfidentialOutputData {
                    claim_public_key: key(),
                }),
                sidechain_id: None,
            }),
            RangeProofType::BulletProofPlus,
        );
        let revealed = OutputFeatures {
            range_proof_type: RangeProofType::RevealedValue,
            ..Default::default()
        };
        let features_v1 = OutputFeatures {
            version: OutputFeaturesVersion::V1,
            ..Default::default()
        };
        let matured = OutputFeatures {
            maturity: u64::MAX,
            ..Default::default()
        };

        type Expected = (u8, u64, Option<SideChainKind>, Option<[u8; 32]>, bool);
        let cases: Vec<(&str, OutputFeatures, Expected)> = vec![
            ("default", OutputFeatures::default(), (0, 0, None, None, true)),
            ("maturity", matured, (0, u64::MAX, None, None, false)),
            ("revealed value", revealed, (0, 0, None, None, false)),
            ("features v1", features_v1, (0, 0, None, None, false)),
            (
                "vn registration",
                vn_registration_plain,
                (
                    3,
                    0,
                    Some(SideChainKind::ValidatorNodeRegistration),
                    Some(vn_key_plain.as_bytes().try_into().unwrap()),
                    false,
                ),
            ),
            (
                "vn registration with sidechain id",
                vn_registration_with_id,
                (
                    3,
                    0,
                    Some(SideChainKind::ValidatorNodeRegistration),
                    Some(vn_key_with_id.as_bytes().try_into().unwrap()),
                    false,
                ),
            ),
            (
                "vn exit",
                vn_exit,
                (
                    7,
                    0,
                    Some(SideChainKind::ValidatorNodeExit),
                    Some(vn_exit_key.as_bytes().try_into().unwrap()),
                    false,
                ),
            ),
            (
                "confidential output",
                confidential,
                (0, 0, Some(SideChainKind::ConfidentialOutput), None, false),
            ),
            (
                "code template registration, every string at its maximum",
                code_template_registration(),
                (4, 0, Some(SideChainKind::CodeTemplateRegistration), None, false),
            ),
        ];

        for version in [TransactionOutputVersion::V0, TransactionOutputVersion::V1] {
            for encrypted_data_size in [80, MAX_ENCRYPTED_DATA_SIZE] {
                let encrypted_data = encrypted_data(encrypted_data_size);
                for (name, features, (output_type, maturity, sidechain, vn_key, default)) in &cases {
                    for minimum_value_promise in [0, 1_000] {
                        let bytes = preimage(version, features, &encrypted_data, minimum_value_promise);
                        let consensus = TransactionOutput::metadata_signature_message_common_from_parts(
                            &version,
                            features,
                            &Covenant::default(),
                            &encrypted_data,
                            &MicroMinotari(minimum_value_promise),
                        );
                        assert_eq!(device_common(&bytes), consensus, "{name}: the device's common hash");

                        let parsed = parse_metadata_preimage(&bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
                        assert_eq!(parsed.output_version, version.as_u8(), "{name}");
                        assert_eq!(parsed.output_type, *output_type, "{name}");
                        assert_eq!(parsed.maturity, *maturity, "{name}");
                        assert_eq!(parsed.sidechain, *sidechain, "{name}");
                        assert_eq!(parsed.validator_node_public_key, *vn_key, "{name}");
                        assert_eq!(parsed.range_proof_type, features.range_proof_type.as_byte(), "{name}");
                        assert_eq!(parsed.has_default_features, *default, "{name}");
                        assert_eq!(parsed.minimum_value_promise, minimum_value_promise, "{name}");
                        assert_eq!(
                            parsed.may_skip_review(),
                            *default && minimum_value_promise == 0,
                            "{name}"
                        );
                    }
                }
            }
        }
    }

    /// A burn, a coinbase (with or without extra data) and a covenant are refused, read from the real encodings.
    #[test]
    fn the_device_refuses_a_burn_a_coinbase_and_a_covenant() {
        let encrypted_data = encrypted_data(80);
        let burn = OutputFeatures::create_burn_confidential_output(key(), Some(&PrivateKey::random(&mut rand::rng())));
        assert_eq!(
            parse_metadata_preimage(&preimage(TransactionOutputVersion::V0, &burn, &encrypted_data, 0)),
            Err(MetadataPreimageError::Burn)
        );
        let coinbase = OutputFeatures::create_coinbase(10, None, RangeProofType::BulletProofPlus);
        assert_eq!(
            parse_metadata_preimage(&preimage(TransactionOutputVersion::V0, &coinbase, &encrypted_data, 0)),
            Err(MetadataPreimageError::Coinbase)
        );
        let extra = OutputFeatures {
            coinbase_extra: vec![1, 2, 3].try_into().unwrap(),
            ..Default::default()
        };
        assert_eq!(
            parse_metadata_preimage(&preimage(TransactionOutputVersion::V0, &extra, &encrypted_data, 0)),
            Err(MetadataPreimageError::Coinbase)
        );

        let covenant = crate::covenant!(absolute_height(@uint(100))).unwrap();
        let bytes = TransactionOutput::metadata_signature_message_common_preimage(
            &TransactionOutputVersion::V0,
            &OutputFeatures::default(),
            &covenant,
            &encrypted_data,
            &MicroMinotari(0),
        )
        .unwrap();
        assert_eq!(
            parse_metadata_preimage(&bytes),
            Err(MetadataPreimageError::CovenantNotEmpty)
        );
    }
}

#[cfg(test)]
mod test {
    use super::{TransactionOutput, batch_verify_range_proofs};
    use crate::{
        MicroMinotari,
        crypto_factories::CryptoFactories,
        key_manager::{KeyManager, TransactionKeyManagerInterface},
        test_helpers::{TestParams, UtxoTestParams},
        transaction_components::{OutputFeatures, RangeProofType},
    };

    #[test]
    fn it_builds_correctly() {
        let factories = CryptoFactories::default();
        let key_manager = KeyManager::new_random().unwrap();
        let test_params = TestParams::new(&key_manager);

        let value = MicroMinotari(10);
        let minimum_value_promise = MicroMinotari(10);
        let tx_output = create_output(
            &test_params,
            value,
            minimum_value_promise,
            RangeProofType::BulletProofPlus,
            &key_manager,
        )
        .unwrap();

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_ok());
        assert!(tx_output.verify_metadata_signature().is_ok());
        let (_, recovered_value, _) = key_manager
            .try_output_key_recovery(
                tx_output.commitment(),
                tx_output.encrypted_data(),
                &tx_output.sender_offset_public_key,
            )
            .unwrap()
            .unwrap();
        assert_eq!(recovered_value, value);
    }

    #[test]
    fn it_does_not_verify_incorrect_minimum_value() {
        let factories = CryptoFactories::default();
        let key_manager = KeyManager::new_random().unwrap();
        let test_params = TestParams::new(&key_manager);

        let value = MicroMinotari(10);
        let minimum_value_promise = MicroMinotari(11);
        let tx_output = create_invalid_output(
            &test_params,
            value,
            minimum_value_promise,
            RangeProofType::BulletProofPlus,
            &key_manager,
        );

        assert!(tx_output.verify_range_proof(&factories.range_proof).is_err());
    }

    #[test]
    fn it_does_batch_verify_correct_minimum_values() {
        let factories = CryptoFactories::default();
        let key_manager = KeyManager::new_random().unwrap();
        let test_params = TestParams::new(&key_manager);

        let outputs = [
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(5),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(10),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .unwrap(),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[test]
    fn it_does_batch_verify_with_mixed_range_proof_types() {
        let key_manager = KeyManager::new_random().unwrap();
        let factories = CryptoFactories::default();
        let test_params = TestParams::new(&key_manager);

        let outputs = [
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(10),
                RangeProofType::RevealedValue,
                &key_manager,
            )
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .unwrap(),
            &create_output(
                &test_params,
                MicroMinotari(20),
                MicroMinotari(20),
                RangeProofType::RevealedValue,
                &key_manager,
            )
            .unwrap(),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs,).is_ok());
    }

    #[test]
    fn invalid_revealed_value_proofs_are_blocked() {
        let key_manager = KeyManager::new_random().unwrap();
        let test_params = TestParams::new(&key_manager);
        assert!(
            create_output(
                &test_params,
                MicroMinotari(20),
                MicroMinotari::zero(),
                RangeProofType::BulletProofPlus,
                &key_manager
            )
            .is_ok()
        );
        match create_output(
            &test_params,
            MicroMinotari(20),
            MicroMinotari::zero(),
            RangeProofType::RevealedValue,
            &key_manager,
        ) {
            Ok(_) => panic!("Should not have been able to create output"),
            Err(e) => assert_eq!(
                e,
                "A range proof construction or verification has produced an error: Invalid revealed value: Expected \
                 20 µT, received 0 µT"
            ),
        }
    }

    #[test]
    fn it_does_not_batch_verify_incorrect_minimum_values() {
        let factories = CryptoFactories::default();
        let key_manager = KeyManager::new_random().unwrap();
        let test_params = TestParams::new(&key_manager);

        let outputs = [
            &create_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(10),
                RangeProofType::BulletProofPlus,
                &key_manager,
            )
            .unwrap(),
            &create_invalid_output(
                &test_params,
                MicroMinotari(10),
                MicroMinotari(11),
                RangeProofType::BulletProofPlus,
                &key_manager,
            ),
        ];

        assert!(batch_verify_range_proofs(&factories.range_proof, &outputs).is_err());
    }

    fn create_output<KM: TransactionKeyManagerInterface>(
        test_params: &TestParams,
        value: MicroMinotari,
        minimum_value_promise: MicroMinotari,
        range_proof_type: RangeProofType,
        key_manager: &KM,
    ) -> Result<TransactionOutput, String> {
        let utxo = test_params.create_output(
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
        );
        utxo?.to_transaction_output().map_err(|e| e.to_string())
    }

    fn create_invalid_output<KM: TransactionKeyManagerInterface>(
        test_params: &TestParams,
        value: MicroMinotari,
        minimum_value_promise: MicroMinotari,
        range_proof_type: RangeProofType,
        key_manager: &KM,
    ) -> TransactionOutput {
        // we need first to create a valid minimum value, regardless of the minimum_value_promise
        // because this test function should allow creating an invalid proof for later testing
        let mut output =
            create_output(test_params, value, MicroMinotari::zero(), range_proof_type, key_manager).unwrap();

        // Now we can updated the minimum value, even to an invalid value
        output.minimum_value_promise = minimum_value_promise;

        output
    }
}
