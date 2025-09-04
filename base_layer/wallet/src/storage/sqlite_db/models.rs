// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use chacha20poly1305::XChaCha20Poly1305;
use tari_common_types::{
    burn_proof::{BurnClaimProof, EncodedMerkleProof},
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    types::CompressedPublicKey,
};
use tari_utilities::{ByteArray, Hidden};

use crate::{error::WalletStorageError, schema, storage::serializers};

pub struct DbBurnProof {
    pub id: i32,
    pub reciprocal_claim_public_key: CompressedPublicKey,
    pub burn_proof: BurnClaimProof,
    pub kernel_merkle_proof: Option<EncodedMerkleProof>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

impl TryFrom<BurntProofSql> for DbBurnProof {
    type Error = WalletStorageError;

    fn try_from(value: BurntProofSql) -> Result<Self, Self::Error> {
        let reciprocal_claim_public_key = CompressedPublicKey::from_canonical_bytes(&value.reciprocal_claim_public_key)
            .map_err(|e| WalletStorageError::ConversionError(format!("Invalid public key: {}", e)))?;
        let burn_proof: BurnClaimProof = serializers::bincode_decode(&value.burn_proof)?;
        let kernel_merkle_proof = value
            .kernel_merkle_proof
            .as_ref()
            .map(|kp| serializers::bincode_decode(kp))
            .transpose()?;
        Ok(Self {
            id: value.id,
            reciprocal_claim_public_key,
            burn_proof,
            kernel_merkle_proof,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Debug, Queryable)]
#[diesel(table_name = schema::burn_proofs)]
pub(crate) struct BurntProofSql {
    pub id: i32,
    pub _output_hash: Vec<u8>,
    pub commitment: Vec<u8>,
    pub reciprocal_claim_public_key: Vec<u8>,
    pub burn_proof: Vec<u8>,
    pub kernel_merkle_proof: Option<Vec<u8>>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

fn encrypt_domain(commitment: &[u8], field_name: &'static str) -> Vec<u8> {
    [BurntProofSql::BURNT_PROOF, commitment, field_name.as_bytes()].concat()
}

impl Encryptable<XChaCha20Poly1305> for BurntProofSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        encrypt_domain(&self.commitment, field_name)
    }

    fn encrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.burn_proof =
            encrypt_bytes_integral_nonce(cipher, self.domain("encoded_burn_proof"), Hidden::hide(self.burn_proof))?;

        Ok(self)
    }

    fn decrypt(mut self, cipher: &XChaCha20Poly1305) -> Result<Self, String> {
        self.burn_proof = decrypt_bytes_integral_nonce(cipher, self.domain("encoded_burn_proof"), &self.burn_proof)?;
        Ok(self)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = schema::burn_proofs)]
pub(crate) struct NewBurntProofSql<'a> {
    pub output_hash: &'a [u8],
    pub commitment: &'a [u8],
    pub reciprocal_claim_public_key: &'a [u8],
    pub burn_proof: Vec<u8>,
    pub kernel_merkle_proof: Option<&'a [u8]>,
}

impl<'a> NewBurntProofSql<'a> {
    pub fn new_encrypted(
        output_hash: &'a [u8],
        commitment: &'a [u8],
        reciprocal_claim_public_key: &'a [u8],
        burn_proof: Vec<u8>,
        kernel_merkle_proof: Option<&'a [u8]>,
        cipher: &XChaCha20Poly1305,
    ) -> Result<Self, WalletStorageError> {
        let burn_proof = encrypt_bytes_integral_nonce(
            cipher,
            encrypt_domain(commitment, "encoded_burn_proof"),
            Hidden::hide(burn_proof),
        )
        .map_err(WalletStorageError::AeadError)?;
        let entry = Self {
            output_hash,
            commitment,
            reciprocal_claim_public_key,
            burn_proof,
            kernel_merkle_proof,
        };
        Ok(entry)
    }
}
