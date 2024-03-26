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
//
// Portions of this file were originally copyrighted (c) 2018 The Grin Developers, issued under the Apache License,
// Version 2.0, available at http://www.apache.org/licenses/LICENSE-2.0.

//! Encrypted data using the the extended-nonce variant XChaCha20-Poly1305 encryption with secure random nonce.

use std::mem::size_of;

use blake2::Blake2b;
use borsh::{BorshDeserialize, BorshSerialize};
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, Error, OsRng},
    KeyInit,
    Tag,
    XChaCha20Poly1305,
    XNonce,
};
use digest::{consts::U32, generic_array::GenericArray, FixedOutput};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, PrivateKey};
use tari_crypto::{hashing::DomainSeparatedHasher, keys::SecretKey};
use tari_hashing::TransactionSecureNonceKdfDomain;
use tari_utilities::{
    hex::{from_hex, to_hex, Hex, HexError},
    safe_array::SafeArray,
    ByteArray,
    ByteArrayError,
};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use super::EncryptedDataKey;
use crate::transactions::tari_amount::MicroMinotari;

// Useful size constants, each in bytes
const SIZE_NONCE: usize = size_of::<XNonce>();
const SIZE_VALUE: usize = size_of::<u64>();
const SIZE_MASK: usize = PrivateKey::KEY_LEN;
const SIZE_TAG: usize = size_of::<Tag>();
const SIZE_TOTAL: usize = SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_TAG;

// Number of hex characters of encrypted data to display on each side of ellipsis when truncating
const DISPLAY_CUTOFF: usize = 16;

#[derive(
    Debug, Copy, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize,
)]
pub struct EncryptedData {
    #[serde(with = "tari_utilities::serde::hex")]
    data: [u8; SIZE_TOTAL], // nonce, encrypted value, encrypted mask, tag
}

/// AEAD associated data
const ENCRYPTED_DATA_AAD: &[u8] = b"TARI_AAD_VALUE_AND_MASK_EXTEND_NONCE_VARIANT";

impl EncryptedData {
    /// Encrypt the value and mask (with fixed length) using XChaCha20-Poly1305 with a secure random nonce
    /// Notes: - This implementation does not require or assume any uniqueness for `encryption_key` or `commitment`
    ///        - With the use of a secure random nonce, there's no added security benefit in using the commitment in the
    ///          internal key derivation; but it binds the encrypted data to the commitment
    ///        - Consecutive calls to this function with the same inputs will produce different ciphertexts
    pub fn encrypt_data(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        value: MicroMinotari,
        mask: &PrivateKey,
    ) -> Result<EncryptedData, EncryptedDataError> {
        // Encode the value and mask
        let mut bytes = Zeroizing::new([0u8; SIZE_VALUE + SIZE_MASK]);
        bytes[..SIZE_VALUE].clone_from_slice(value.as_u64().to_le_bytes().as_ref());
        bytes[SIZE_VALUE..].clone_from_slice(mask.as_bytes());

        // Produce a secure random nonce
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Encrypt in place
        let tag = cipher.encrypt_in_place_detached(&nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice())?;

        // Put everything together: nonce, ciphertext, tag
        let mut data = [0u8; SIZE_TOTAL];
        data[..SIZE_NONCE].clone_from_slice(&nonce);
        data[SIZE_NONCE..SIZE_NONCE + SIZE_VALUE + SIZE_MASK].clone_from_slice(bytes.as_slice());
        data[SIZE_NONCE + SIZE_VALUE + SIZE_MASK..].clone_from_slice(&tag);

        Ok(Self { data })
    }

    /// Authenticate and decrypt the value and mask
    /// Note: This design (similar to other AEADs) is not key committing, thus the caller must not rely on successful
    ///       decryption to assert that the expected key was used
    pub fn decrypt_data(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        encrypted_data: &EncryptedData,
    ) -> Result<(MicroMinotari, PrivateKey), EncryptedDataError> {
        // Extract the nonce, ciphertext, and tag
        let nonce = XNonce::from_slice(&encrypted_data.as_bytes()[..SIZE_NONCE]);
        let mut bytes = Zeroizing::new([0u8; SIZE_VALUE + SIZE_MASK]);
        bytes.clone_from_slice(&encrypted_data.as_bytes()[SIZE_NONCE..SIZE_NONCE + SIZE_VALUE + SIZE_MASK]);
        let tag = Tag::from_slice(&encrypted_data.as_bytes()[SIZE_NONCE + SIZE_VALUE + SIZE_MASK..]);

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Decrypt in place
        cipher.decrypt_in_place_detached(nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice(), tag)?;

        // Decode the value and mask
        let mut value_bytes = [0u8; SIZE_VALUE];
        value_bytes.clone_from_slice(&bytes[0..SIZE_VALUE]);
        Ok((
            u64::from_le_bytes(value_bytes).into(),
            PrivateKey::from_canonical_bytes(&bytes[SIZE_VALUE..])?,
        ))
    }

    /// Parse encrypted data from a byte slice
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptedDataError> {
        if bytes.len() != SIZE_TOTAL {
            return Err(EncryptedDataError::IncorrectLength(format!(
                "Expected {} bytes, got {}",
                SIZE_TOTAL,
                bytes.len()
            )));
        }
        let mut data = [0u8; SIZE_TOTAL];
        data.copy_from_slice(bytes);
        Ok(Self { data })
    }

    /// Get a byte vector with the encrypted data contents
    pub fn to_byte_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }

    /// Get a byte array with the encrypted data contents
    pub fn to_bytes(&self) -> [u8; SIZE_TOTAL] {
        self.data
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
                encrypted_data_hex
            }
        }
    }
}

impl Hex for EncryptedData {
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        let v = from_hex(hex)?;
        Self::from_bytes(&v).map_err(|_| HexError::HexConversionError {})
    }

    fn to_hex(&self) -> String {
        to_hex(&self.to_byte_vec())
    }
}

impl Default for EncryptedData {
    fn default() -> Self {
        Self {
            data: [0u8; SIZE_TOTAL],
        }
    }
}
// EncryptedOpenings errors
#[derive(Debug, Error)]
pub enum EncryptedDataError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(Error),
    #[error("Conversion failed: {0}")]
    ByteArrayError(String),
    #[error("Incorrect length: {0}")]
    IncorrectLength(String),
}

impl From<ByteArrayError> for EncryptedDataError {
    fn from(e: ByteArrayError) -> Self {
        EncryptedDataError::ByteArrayError(e.to_string())
    }
}

// Chacha error is not StdError compatible
impl From<Error> for EncryptedDataError {
    fn from(err: Error) -> Self {
        Self::EncryptionFailed(err)
    }
}

// Generate a ChaCha20-Poly1305 key from a private key and commitment using Blake2b
fn kdf_aead(encryption_key: &PrivateKey, commitment: &Commitment) -> EncryptedDataKey {
    let mut aead_key = EncryptedDataKey::from(SafeArray::default());
    DomainSeparatedHasher::<Blake2b<U32>, TransactionSecureNonceKdfDomain>::new_with_label("encrypted_value_and_mask")
        .chain(encryption_key.as_bytes())
        .chain(commitment.as_bytes())
        .finalize_into(GenericArray::from_mut_slice(aead_key.reveal_mut()));

    aead_key
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common_types::types::{CommitmentFactory, PrivateKey};
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};

    use super::*;

    #[test]
    fn it_encrypts_and_decrypts_correctly() {
        for (value, mask) in [
            (0, PrivateKey::default()),
            (0, PrivateKey::random(&mut OsRng)),
            (123456, PrivateKey::default()),
            (654321, PrivateKey::random(&mut OsRng)),
            (u64::MAX, PrivateKey::random(&mut OsRng)),
        ] {
            let commitment = CommitmentFactory::default().commit(&mask, &PrivateKey::from(value));
            let encryption_key = PrivateKey::random(&mut OsRng);
            let amount = MicroMinotari::from(value);
            let encrypted_data = EncryptedData::encrypt_data(&encryption_key, &commitment, amount, &mask).unwrap();
            let (decrypted_value, decrypted_mask) =
                EncryptedData::decrypt_data(&encryption_key, &commitment, &encrypted_data).unwrap();
            assert_eq!(amount, decrypted_value);
            assert_eq!(mask, decrypted_mask);
            if let Ok((decrypted_value, decrypted_mask)) =
                EncryptedData::decrypt_data(&PrivateKey::random(&mut OsRng), &commitment, &encrypted_data)
            {
                assert_ne!(amount, decrypted_value);
                assert_ne!(mask, decrypted_mask);
            }
        }
    }

    #[test]
    fn it_converts_correctly() {
        for (value, mask) in [
            (0, PrivateKey::default()),
            (0, PrivateKey::random(&mut OsRng)),
            (123456, PrivateKey::default()),
            (654321, PrivateKey::random(&mut OsRng)),
            (u64::MAX, PrivateKey::random(&mut OsRng)),
        ] {
            let commitment = CommitmentFactory::default().commit(&mask, &PrivateKey::from(value));
            let encryption_key = PrivateKey::random(&mut OsRng);
            let amount = MicroMinotari::from(value);
            let encrypted_data = EncryptedData::encrypt_data(&encryption_key, &commitment, amount, &mask).unwrap();
            let bytes = encrypted_data.to_byte_vec();
            let encrypted_data_from_bytes = EncryptedData::from_bytes(&bytes).unwrap();
            assert_eq!(encrypted_data, encrypted_data_from_bytes);
        }
    }
}
