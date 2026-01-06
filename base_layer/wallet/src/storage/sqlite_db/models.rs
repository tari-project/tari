// Copyright 2025 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use chacha20poly1305::XChaCha20Poly1305;
use tari_common_types::{
    burn_proof::{BurnClaimProof, EncodedMerkleProof},
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce, Encryptable},
    types::FixedHash,
};
use tari_transaction_components::{
    transaction_components::{EncryptedData, TransactionKernel},
    MicroMinotari,
};
use tari_utilities::Hidden;

use crate::{error::WalletStorageError, schema, storage::serializers};

pub struct DbBurnProof {
    pub id: i32,
    pub output_hash: FixedHash,
    pub burn_proof: BurnClaimProof,
    pub kernel: TransactionKernel,
    pub kernel_merkle_proof: Option<EncodedMerkleProof>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub encrypted_data: Option<EncryptedData>,
    pub value: Option<MicroMinotari>,
}

impl TryFrom<BurntProofSql> for DbBurnProof {
    type Error = WalletStorageError;

    fn try_from(value: BurntProofSql) -> Result<Self, Self::Error> {
        let burn_proof = serializers::bincode_decode(&value.burn_proof)?;
        let kernel = serializers::bincode_decode(&value.kernel)?;
        let kernel_merkle_proof = value
            .kernel_merkle_proof
            .as_ref()
            .map(|kp| serializers::bincode_decode(kp))
            .transpose()?;
        let output_hash = FixedHash::try_from(value.output_hash.as_slice())
            .map_err(|e| WalletStorageError::ConversionError(format!("Invalid output hash length in DB: {}", e)))?;
        let encrypted_data = match value.encrypted_data {
            Some(data) => Some(EncryptedData::from_bytes(data.as_slice()).map_err(|e| {
                WalletStorageError::ConversionError(format!("Invalid encrypted data length in DB: {}", e))
            })?),
            None => None,
        };
        let v = match value.value {
            Some(v) => Some(MicroMinotari(u64::try_from(v).map_err(|e| {
                WalletStorageError::ConversionError(format!("Invalid value in DB: {}", e))
            })?)),
            None => None,
        };

        Ok(Self {
            id: value.id,
            output_hash,
            burn_proof,
            kernel,
            kernel_merkle_proof,
            created_at: value.created_at,
            updated_at: value.updated_at,
            encrypted_data,
            value: v,
        })
    }
}

#[derive(Debug, Queryable)]
#[diesel(table_name = schema::burn_proofs)]
pub(crate) struct BurntProofSql {
    pub id: i32,
    pub output_hash: Vec<u8>,
    pub commitment: Vec<u8>,
    pub burn_proof: Vec<u8>,
    pub kernel: Vec<u8>,
    pub kernel_merkle_proof: Option<Vec<u8>>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
    pub encrypted_data: Option<Vec<u8>>,
    pub value: Option<i64>,
    pub kernel_excess: Option<Vec<u8>>,
    pub kernel_excess_sig: Option<Vec<u8>>,
}

fn get_encryption_domain(commitment: &[u8], field_name: &'static str) -> Vec<u8> {
    [BurntProofSql::BURNT_PROOF, commitment, field_name.as_bytes()].concat()
}

impl Encryptable<XChaCha20Poly1305> for BurntProofSql {
    fn domain(&self, field_name: &'static str) -> Vec<u8> {
        get_encryption_domain(&self.commitment, field_name)
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
    pub burn_proof: Vec<u8>,
    pub kernel: Vec<u8>,
    pub kernel_merkle_proof: Option<&'a [u8]>,
    pub encrypted_data: Option<&'a [u8]>,
    pub value: Option<i64>,
    pub kernel_excess: Option<&'a [u8]>,
    pub kernel_excess_sig: Option<&'a [u8]>,
}

impl<'a> NewBurntProofSql<'a> {
    pub fn new_encrypted(
        output_hash: &'a [u8],
        commitment: &'a [u8],
        burn_proof: Vec<u8>,
        kernel: Vec<u8>,
        kernel_merkle_proof: Option<&'a [u8]>,
        cipher: &XChaCha20Poly1305,
        encrypted_data: Option<&'a [u8]>,
        value: Option<i64>,
        kernel_excess: Option<&'a [u8]>,
        kernel_excess_sig: Option<&'a [u8]>,
    ) -> Result<Self, WalletStorageError> {
        let burn_proof = encrypt_bytes_integral_nonce(
            cipher,
            get_encryption_domain(commitment, "encoded_burn_proof"),
            Hidden::hide(burn_proof),
        )
        .map_err(WalletStorageError::AeadError)?;
        let entry = Self {
            output_hash,
            commitment,
            burn_proof,
            kernel,
            kernel_merkle_proof,
            encrypted_data,
            value,
            kernel_excess,
            kernel_excess_sig,
        };
        Ok(entry)
    }
}
