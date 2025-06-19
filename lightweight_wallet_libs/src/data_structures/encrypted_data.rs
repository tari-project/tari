// Copyright 2022 The Tari Project
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

//! Encrypted data using the extended-nonce variant XChaCha20-Poly1305 encryption with secure random nonce.

use std::mem::size_of;

use blake2::{Blake2b, Digest};
use borsh::{BorshDeserialize, BorshSerialize};
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, OsRng},
    KeyInit,
    Tag,
    XChaCha20Poly1305,
    XNonce,
};
use digest::{consts::U32, generic_array::GenericArray};
use hex::ToHex;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    data_structures::{
        payment_id::PaymentId,
        types::{CompressedCommitment, EncryptedDataKey, MicroMinotari, PrivateKey, SafeArray},
    },
    errors::{LightweightWalletError, EncryptionError, DataStructureError},
    hex_utils::{HexEncodable, HexValidatable, HexError},
};

// Useful size constants, each in bytes
const SIZE_NONCE: usize = size_of::<XNonce>();
pub const SIZE_VALUE: usize = 8;
const SIZE_MASK: usize = 32;
const SIZE_TAG: usize = size_of::<Tag>();
pub const SIZE_U256: usize = size_of::<primitive_types::U256>();
pub const STATIC_ENCRYPTED_DATA_SIZE_TOTAL: usize = SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_TAG;
const MAX_ENCRYPTED_DATA_SIZE: usize = 256 + STATIC_ENCRYPTED_DATA_SIZE_TOTAL;

// Number of hex characters of encrypted data to display on each side of ellipsis when truncating
const DISPLAY_CUTOFF: usize = 16;

/// AEAD associated data
const ENCRYPTED_DATA_AAD: &[u8] = b"TARI_AAD_VALUE_AND_MASK_EXTEND_NONCE_VARIANT";

/// Encrypted data structure for storing encrypted value, mask, and payment ID
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize)]
pub struct EncryptedData {
    #[serde(with = "hex_serde")]
    data: Vec<u8>,
}

impl EncryptedData {
    /// Encrypt the value and mask (with fixed length) using XChaCha20-Poly1305 with a secure random nonce
    /// Notes: - This implementation does not require or assume any uniqueness for `encryption_key` or `commitment`
    ///        - With the use of a secure random nonce, there's no added security benefit in using the commitment in the
    ///          internal key derivation; but it binds the encrypted data to the commitment
    ///        - Consecutive calls to this function with the same inputs will produce different ciphertexts
    pub fn encrypt_data(
        encryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
        value: MicroMinotari,
        mask: &PrivateKey,
        payment_id: PaymentId,
    ) -> Result<EncryptedData, LightweightWalletError> {
        // Encode the value and mask
        let mut bytes = Zeroizing::new(vec![0; SIZE_VALUE + SIZE_MASK + payment_id.get_size()]);
        bytes[..SIZE_VALUE].clone_from_slice(value.as_u64().to_le_bytes().as_ref());
        bytes[SIZE_VALUE..SIZE_VALUE + SIZE_MASK].clone_from_slice(&mask.as_bytes());
        bytes[SIZE_VALUE + SIZE_MASK..].clone_from_slice(&payment_id.to_bytes());

        // Produce a secure random nonce
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Encrypt in place
        let tag = cipher.encrypt_in_place_detached(&nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice())
            .map_err(|e| EncryptionError::encryption_failed(&e.to_string()))?;

        // Put everything together: nonce, ciphertext, tag
        let mut data = vec![0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL + payment_id.get_size()];
        data[..SIZE_TAG].clone_from_slice(&tag);
        data[SIZE_TAG..SIZE_TAG + SIZE_NONCE].clone_from_slice(&nonce);
        data[SIZE_TAG + SIZE_NONCE..SIZE_TAG + SIZE_NONCE + SIZE_VALUE + SIZE_MASK + payment_id.get_size()]
            .clone_from_slice(bytes.as_slice());
        
        Ok(Self { data })
    }

    /// Authenticate and decrypt the value and mask
    /// Note: This design (similar to other AEADs) is not key committing, thus the caller must not rely on successful
    ///       decryption to assert that the expected key was used
    pub fn decrypt_data(
        encryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
    ) -> Result<(MicroMinotari, PrivateKey, PaymentId), LightweightWalletError> {
        // Extract the nonce, ciphertext, and tag
        let tag = Tag::from_slice(&encrypted_data.as_bytes()[..SIZE_TAG]);
        let nonce = XNonce::from_slice(&encrypted_data.as_bytes()[SIZE_TAG..SIZE_TAG + SIZE_NONCE]);
        let mut bytes = Zeroizing::new(vec![
            0;
            encrypted_data
                .data
                .len()
                .saturating_sub(SIZE_TAG)
                .saturating_sub(SIZE_NONCE)
        ]);
        bytes.clone_from_slice(&encrypted_data.as_bytes()[SIZE_TAG + SIZE_NONCE..]);

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Decrypt in place
        cipher.decrypt_in_place_detached(&nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice(), tag)
            .map_err(|e| EncryptionError::decryption_failed(&e.to_string()))?;

        // Decode the value and mask
        let mut value_bytes = [0u8; SIZE_VALUE];
        value_bytes.clone_from_slice(&bytes[0..SIZE_VALUE]);
        Ok((
            u64::from_le_bytes(value_bytes).into(),
            PrivateKey::from_canonical_bytes(&bytes[SIZE_VALUE..SIZE_VALUE + SIZE_MASK])
                .map_err(|e| DataStructureError::InvalidPrivateKey(e.to_string()))?,
            PaymentId::from_bytes(&bytes[SIZE_VALUE + SIZE_MASK..])
                .map_err(|e| DataStructureError::InvalidPaymentId(e.to_string()))?,
        ))
    }

    /// Parse encrypted data from a byte slice
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError> {
        if bytes.len() < STATIC_ENCRYPTED_DATA_SIZE_TOTAL {
            return Err(DataStructureError::data_too_small(
                STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
                bytes.len()
            ).into());
        }
        if bytes.len() > MAX_ENCRYPTED_DATA_SIZE {
            return Err(DataStructureError::data_too_large(
                MAX_ENCRYPTED_DATA_SIZE,
                bytes.len()
            ).into());
        }
        Ok(Self {
            data: bytes.to_vec(),
        })
    }

    /// Get a byte vector with the encrypted data contents
    pub fn to_byte_vec(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Get a byte slice with the encrypted data contents
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Accessor method for the encrypted data hex display
    pub fn hex_display(&self, full: bool) -> String {
        if full {
            self.to_hex()
        } else {
            let encrypted_data_hex = self.to_hex();
            if encrypted_data_hex.len() > 2 * DISPLAY_CUTOFF {
                format!(
                    "Some({}..{})",
                    &encrypted_data_hex[0..DISPLAY_CUTOFF],
                    &encrypted_data_hex[encrypted_data_hex.len() - DISPLAY_CUTOFF..encrypted_data_hex.len()]
                )
            } else {
                format!("Some({})", encrypted_data_hex)
            }
        }
    }

    /// Get the payment ID size from the encrypted data
    pub fn get_payment_id_size(&self) -> usize {
        self.data.len().saturating_sub(STATIC_ENCRYPTED_DATA_SIZE_TOTAL)
    }
}

impl Default for EncryptedData {
    fn default() -> Self {
        Self {
            data: vec![0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL],
        }
    }
}

/// Hex encoding/decoding implementation for EncryptedData
impl EncryptedData {
    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        self.data.encode_hex()
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() > MAX_ENCRYPTED_DATA_SIZE {
            return Err(HexError::InvalidLength {
                expected: MAX_ENCRYPTED_DATA_SIZE,
                actual: bytes.len(),
            });
        }
        Ok(Self { data: bytes })
    }
}

impl HexEncodable for EncryptedData {
    fn to_hex(&self) -> String {
        self.data.encode_hex()
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let bytes = hex::decode(hex).map_err(|e| HexError::InvalidHex(e.to_string()))?;
        if bytes.len() > MAX_ENCRYPTED_DATA_SIZE {
            return Err(HexError::InvalidLength {
                expected: MAX_ENCRYPTED_DATA_SIZE,
                actual: bytes.len(),
            });
        }
        Ok(Self { data: bytes })
    }
}

impl HexValidatable for EncryptedData {}

/// Key derivation function for AEAD
fn kdf_aead(encryption_key: &PrivateKey, commitment: &CompressedCommitment) -> EncryptedDataKey {
    let mut aead_key = EncryptedDataKey::from(SafeArray::default());
    
    // Use Blake2b for key derivation
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(b"TARI_ENCRYPTED_DATA_KDF");
    hasher.update(encryption_key.as_bytes());
    hasher.update(commitment.as_bytes());
    
    let result = hasher.finalize();
    aead_key.as_mut().copy_from_slice(result.as_slice());
    
    aead_key
}

/// Hex serialization/deserialization helper
mod hex_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(value);
        hex_string.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = String::deserialize(deserializer)?;
        hex::decode(&hex_string).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use primitive_types::U256;

    #[test]
    fn test_encrypt_decrypt_basic() {
        let encryption_key = PrivateKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        let value = MicroMinotari::new(1000000);
        let mask = PrivateKey::new([3u8; 32]);
        let payment_id = PaymentId::Empty;

        let encrypted = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id.clone(),
        ).unwrap();

        let (decrypted_value, decrypted_mask, decrypted_payment_id) = 
            EncryptedData::decrypt_data(&encryption_key, &commitment, &encrypted).unwrap();

        assert_eq!(decrypted_value, value);
        assert_eq!(decrypted_mask, mask);
        assert_eq!(decrypted_payment_id, payment_id);
    }

    #[test]
    fn test_encrypt_decrypt_with_payment_id() {
        let encryption_key = PrivateKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        let value = MicroMinotari::new(5000000);
        let mask = PrivateKey::new([3u8; 32]);
        let payment_id = PaymentId::U256 { value: U256::from(12345) };

        let encrypted = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id.clone(),
        ).unwrap();

        let (decrypted_value, decrypted_mask, decrypted_payment_id) = 
            EncryptedData::decrypt_data(&encryption_key, &commitment, &encrypted).unwrap();

        assert_eq!(decrypted_value, value);
        assert_eq!(decrypted_mask, mask);
        assert_eq!(decrypted_payment_id, payment_id);
    }

    #[test]
    fn test_hex_serialization() {
        let encryption_key = PrivateKey::new([1u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        let value = MicroMinotari::new(1000000);
        let mask = PrivateKey::new([3u8; 32]);
        let payment_id = PaymentId::Empty;

        let encrypted = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id,
        ).unwrap();

        let hex_string = encrypted.to_hex();
        let from_hex = EncryptedData::from_hex(&hex_string).unwrap();
        
        assert_eq!(encrypted, from_hex);
    }

    #[test]
    fn test_wrong_key_fails() {
        let encryption_key = PrivateKey::new([1u8; 32]);
        let wrong_key = PrivateKey::new([9u8; 32]);
        let commitment = CompressedCommitment::new([2u8; 33]);
        let value = MicroMinotari::new(1000000);
        let mask = PrivateKey::new([3u8; 32]);
        let payment_id = PaymentId::Empty;

        let encrypted = EncryptedData::encrypt_data(
            &encryption_key,
            &commitment,
            value,
            &mask,
            payment_id,
        ).unwrap();

        let result = EncryptedData::decrypt_data(&wrong_key, &commitment, &encrypted);
        assert!(result.is_err());
    }
} 