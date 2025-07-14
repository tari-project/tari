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

//! Encrypted data using the extended-nonce variant XChaCha20-Poly1305 encryption with secure random nonce.

use std::{convert::TryFrom, mem::size_of};

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
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use tari_common_types::types::{CompressedCommitment, PrivateKey};
use tari_crypto::{hashing::DomainSeparatedHasher, keys::SecretKey};
use tari_hashing::TransactionSecureNonceKdfDomain;
use tari_max_size::MaxSizeBytes;
use tari_utilities::{
    hex::{from_hex, to_hex, Hex, HexError},
    safe_array::SafeArray,
    ByteArray,
    ByteArrayError,
};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use super::EncryptedDataKey;
use crate::transactions::{tari_amount::MicroMinotari, transaction_components::payment_id::PaymentId};

// Useful size constants, each in bytes
const SIZE_NONCE: usize = size_of::<XNonce>();
pub const SIZE_VALUE: usize = size_of::<u64>();
const SIZE_MASK: usize = PrivateKey::KEY_LEN;
const SIZE_TAG: usize = size_of::<Tag>();
pub const SIZE_U256: usize = size_of::<U256>();
pub const STATIC_ENCRYPTED_DATA_SIZE_TOTAL: usize = SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_TAG;
const MAX_ENCRYPTED_DATA_SIZE: usize = 256 + STATIC_ENCRYPTED_DATA_SIZE_TOTAL;

// Number of hex characters of encrypted data to display on each side of ellipsis when truncating
const DISPLAY_CUTOFF: usize = 16;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize)]
pub struct EncryptedData {
    #[serde(with = "tari_utilities::serde::hex")]
    data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
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
        commitment: &CompressedCommitment,
        value: MicroMinotari,
        mask: &PrivateKey,
        payment_id: PaymentId,
    ) -> Result<EncryptedData, EncryptedDataError> {
        // Encode the value and mask
        let mut bytes = Zeroizing::new(vec![0; SIZE_VALUE + SIZE_MASK + payment_id.get_size()]);
        bytes[..SIZE_VALUE].clone_from_slice(value.as_u64().to_le_bytes().as_ref());
        bytes[SIZE_VALUE..SIZE_VALUE + SIZE_MASK].clone_from_slice(mask.as_bytes());
        bytes[SIZE_VALUE + SIZE_MASK..].clone_from_slice(&payment_id.to_bytes());

        // Produce a secure random nonce
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Encrypt in place
        let tag = cipher.encrypt_in_place_detached(&nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice())?;

        // Put everything together: nonce, ciphertext, tag
        let mut data = vec![0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL + payment_id.get_size()];
        data[..SIZE_TAG].clone_from_slice(&tag);
        data[SIZE_TAG..SIZE_TAG + SIZE_NONCE].clone_from_slice(&nonce);
        data[SIZE_TAG + SIZE_NONCE..SIZE_TAG + SIZE_NONCE + SIZE_VALUE + SIZE_MASK + payment_id.get_size()]
            .clone_from_slice(bytes.as_slice());
        Ok(Self {
            data: MaxSizeBytes::try_from(data)
                .map_err(|_| EncryptedDataError::IncorrectLength("Data too long".to_string()))?,
        })
    }

    /// Authenticate and decrypt the value and mask
    /// Note: This design (similar to other AEADs) is not key committing, thus the caller must not rely on successful
    ///       decryption to assert that the expected key was used
    pub fn decrypt_data(
        encryption_key: &PrivateKey,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
    ) -> Result<(MicroMinotari, PrivateKey, PaymentId), EncryptedDataError> {
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
        cipher.decrypt_in_place_detached(nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice(), tag)?;

        // Decode the value and mask
        let mut value_bytes = [0u8; SIZE_VALUE];
        value_bytes.clone_from_slice(&bytes[0..SIZE_VALUE]);
        Ok((
            u64::from_le_bytes(value_bytes).into(),
            PrivateKey::from_canonical_bytes(&bytes[SIZE_VALUE..SIZE_VALUE + SIZE_MASK])?,
            PaymentId::from_bytes(&bytes[SIZE_VALUE + SIZE_MASK..]),
        ))
    }

    /// Parse encrypted data from a byte slice
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptedDataError> {
        if bytes.len() < STATIC_ENCRYPTED_DATA_SIZE_TOTAL {
            return Err(EncryptedDataError::IncorrectLength(format!(
                "Expected bytes to be at least {}, got {}",
                STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
                bytes.len()
            )));
        }
        Ok(Self {
            data: MaxSizeBytes::from_bytes_checked(bytes)
                .ok_or(EncryptedDataError::IncorrectLength("Data too long".to_string()))?,
        })
    }

    #[cfg(test)]
    pub fn from_vec_unsafe(data: Vec<u8>) -> Self {
        Self {
            data: MaxSizeBytes::from_bytes_checked(data).unwrap(),
        }
    }

    /// Get a byte vector with the encrypted data contents
    pub fn to_byte_vec(&self) -> Vec<u8> {
        self.data.clone().into()
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

    /// Returns the size of the payment id
    pub fn get_payment_id_size(&self) -> usize {
        // the length should always at least be the static total size, the extra len is the payment id
        self.data.len().saturating_sub(STATIC_ENCRYPTED_DATA_SIZE_TOTAL)
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
            data: MaxSizeBytes::try_from(vec![0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL])
                .expect("This will always be less then the max length"),
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
fn kdf_aead(encryption_key: &PrivateKey, commitment: &CompressedCommitment) -> EncryptedDataKey {
    let mut aead_key = EncryptedDataKey::from(SafeArray::default());
    DomainSeparatedHasher::<Blake2b<U32>, TransactionSecureNonceKdfDomain>::new_with_label("encrypted_value_and_mask")
        .chain(encryption_key.as_bytes())
        .chain(commitment.as_bytes())
        .finalize_into(GenericArray::from_mut_slice(aead_key.reveal_mut()));

    aead_key
}

#[cfg(test)]
mod test {
    use static_assertions::const_assert;
    use tari_common_types::{
        tari_address::{TARI_ADDRESS_INTERNAL_DUAL_SIZE, TARI_ADDRESS_INTERNAL_SINGLE_SIZE},
        types::CommitmentFactory,
    };
    use tari_crypto::commitment::HomomorphicCommitmentFactory;

    use super::*;
    use crate::transactions::transaction_components::payment_id::PaymentId;

    #[test]
    fn test_premine() {
        let id = 999u64;
        let value = 123456;
        let mask = PrivateKey::default();
        let commitment =
            CompressedCommitment::from_commitment(CommitmentFactory::default().commit(&mask, &PrivateKey::from(value)));
        let encryption_key = PrivateKey::random(&mut OsRng);
        let amount = MicroMinotari::from(value);
        let encrypted_data = {
            let mut bytes = Zeroizing::new(vec![0; SIZE_VALUE + SIZE_MASK + SIZE_VALUE]);
            bytes[..SIZE_VALUE].clone_from_slice(value.to_le_bytes().as_ref());
            bytes[SIZE_VALUE..SIZE_VALUE + SIZE_MASK].clone_from_slice(mask.as_bytes());
            bytes[SIZE_VALUE + SIZE_MASK..].clone_from_slice(&id.to_le_bytes().to_vec());

            // Produce a secure random nonce
            let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

            // Set up the AEAD
            let aead_key = kdf_aead(&encryption_key, &commitment);
            let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

            // Encrypt in place
            let tag = cipher
                .encrypt_in_place_detached(&nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice())
                .unwrap();

            // Put everything together: nonce, ciphertext, tag
            let mut data = vec![0; STATIC_ENCRYPTED_DATA_SIZE_TOTAL + SIZE_VALUE];
            data[..SIZE_TAG].clone_from_slice(&tag);
            data[SIZE_TAG..SIZE_TAG + SIZE_NONCE].clone_from_slice(&nonce);
            data[SIZE_TAG + SIZE_NONCE..SIZE_TAG + SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_VALUE]
                .clone_from_slice(bytes.as_slice());
            EncryptedData {
                data: MaxSizeBytes::try_from(data)
                    .map_err(|_| EncryptedDataError::IncorrectLength("Data too long".to_string()))
                    .unwrap(),
            }
        };
        let (decrypted_value, decrypted_mask, decrypted_payment_id) =
            EncryptedData::decrypt_data(&encryption_key, &commitment, &encrypted_data).unwrap();
        assert_eq!(amount, decrypted_value);
        assert_eq!(mask, decrypted_mask);
        match decrypted_payment_id {
            PaymentId::Open { user_data: data, .. } => {
                let bytes: [u8; SIZE_VALUE] = data.try_into().unwrap();
                let v = u64::from_le_bytes(bytes);
                assert_eq!(v, id);
            },
            _ => panic!("Expected PaymentId::Open"),
        }
    }

    #[test]
    fn address_sizes_increase_as_expected() {
        const_assert!(SIZE_VALUE < SIZE_U256);
        const_assert!(SIZE_U256 < TARI_ADDRESS_INTERNAL_SINGLE_SIZE);
        const_assert!(TARI_ADDRESS_INTERNAL_SINGLE_SIZE < TARI_ADDRESS_INTERNAL_DUAL_SIZE);
    }
}
