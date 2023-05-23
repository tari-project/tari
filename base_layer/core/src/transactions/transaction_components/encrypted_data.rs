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

use borsh::{BorshDeserialize, BorshSerialize};
use chacha20poly1305::{
    aead::{Aead, Error, Payload},
    KeyInit,
    Tag,
    XChaCha20Poly1305,
    XNonce,
};
use digest::{generic_array::GenericArray, FixedOutput};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, PrivateKey};
use tari_crypto::{hash::blake2::Blake256, hashing::DomainSeparatedHasher};
use tari_utilities::{
    hex::{from_hex, to_hex, Hex, HexError},
    safe_array::SafeArray,
    ByteArray,
    ByteArrayError,
};
use thiserror::Error;
use zeroize::Zeroize;

use super::EncryptedDataKey;
use crate::transactions::{tari_amount::MicroTari, TransactionSecureNonceKdfDomain};

const VALUE_SIZE: usize = size_of::<u64>(); // 8 bytes
const KEY_SIZE: usize = size_of::<PrivateKey>(); // 32 bytes
const SIZE: usize = VALUE_SIZE + KEY_SIZE + size_of::<Tag>() + size_of::<XNonce>(); // 80 bytes
const BORSH_64: usize = 64;
const BORSH_X: usize = SIZE - BORSH_64; // 16 bytes

/// Encrypted data for the extended-nonce variant XChaCha20-Poly1305 encryption
/// Borsh schema only accept array sizes 0 - 32, 64, 65, 128, 256, 512, 1024 and 2048
#[derive(
    Debug, Copy, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize,
)]
pub struct EncryptedData {
    #[serde(with = "tari_utilities::serde::hex")]
    data_1: [u8; BORSH_64],
    #[serde(with = "tari_utilities::serde::hex")]
    data_2: [u8; BORSH_X],
}

impl EncryptedData {
    const TAG: &'static [u8] = b"TARI_AAD_VALUE_AND_MASK_EXTEND_NONCE_VARIANT";

    /// Encrypt the value and mask (with fixed length) using XChaCha20-Poly1305 with a secure random nonce
    /// Notes: - This implementation does not require or assume any uniqueness for `encryption_key` or `commitment`
    ///        - With the use of a secure random nonce, there's no added security benefit in using the commitment in the
    ///          internal key derivation; but it binds the encrypted data to the commitment
    ///        - Consecutive calls to this function with the same inputs will produce different ciphertexts
    pub fn encrypt_data(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        value: MicroTari,
        mask: &PrivateKey,
    ) -> Result<EncryptedData, EncryptedDataError> {
        let mut openings = value.as_u64().to_le_bytes().to_vec();
        openings.append(&mut mask.to_vec());
        let aead_payload = Payload {
            msg: openings.as_slice(),
            aad: Self::TAG,
        };

        // Produce a secure random nonce
        let mut nonce = [0u8; size_of::<XNonce>()];
        OsRng.fill_bytes(&mut nonce);
        let nonce_ga = XNonce::from_slice(&nonce);

        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));
        let mut ciphertext = cipher.encrypt(nonce_ga, aead_payload)?;
        let mut ciphertext_integral_nonce = nonce.to_vec();
        ciphertext_integral_nonce.append(&mut ciphertext);

        EncryptedData::from_bytes(ciphertext_integral_nonce.as_slice())
    }

    /// Authenticate and decrypt the value and mask
    /// Note: This design (similar to other AEADs) is not key committing, thus the caller must not rely on successful
    ///       decryption to assert that the expected key was used
    pub fn decrypt_data(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        encrypted_data: &EncryptedData,
    ) -> Result<(MicroTari, PrivateKey), EncryptedDataError> {
        // Extract the nonce and ciphertext
        let binding = encrypted_data.to_byte_vec();
        let (nonce, ciphertext) = binding.split_at(size_of::<XNonce>());
        let nonce_ga = XNonce::from_slice(nonce);

        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));
        let aead_payload = Payload {
            msg: ciphertext,
            aad: Self::TAG,
        };
        let decrypted_bytes = cipher.decrypt(nonce_ga, aead_payload)?;
        let mut value_bytes = [0u8; VALUE_SIZE];
        value_bytes.clone_from_slice(&decrypted_bytes[0..VALUE_SIZE]);
        let mut mask_bytes = [0u8; KEY_SIZE];
        mask_bytes.clone_from_slice(&decrypted_bytes[VALUE_SIZE..VALUE_SIZE + KEY_SIZE]);
        Ok((
            u64::from_le_bytes(value_bytes).into(),
            PrivateKey::from_bytes(&mask_bytes)?,
        ))
    }

    /// Custom convert `EncryptedOpenings` to bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptedDataError> {
        if bytes.len() != SIZE {
            return Err(EncryptedDataError::IncorrectLength(format!(
                "Expected {} bytes, got {}",
                SIZE,
                bytes.len()
            )));
        }
        let mut data_1: [u8; BORSH_64] = [0u8; BORSH_64];
        data_1.copy_from_slice(
            bytes
                .get(..BORSH_64)
                .ok_or_else(|| EncryptedDataError::IncorrectLength("Out of bounds 'data_1'".to_string()))?,
        );
        let mut data_2: [u8; BORSH_X] = [0u8; BORSH_X];
        data_2.copy_from_slice(
            bytes
                .get(BORSH_64..SIZE)
                .ok_or_else(|| EncryptedDataError::IncorrectLength("Out of bounds 'data_2'".to_string()))?,
        );
        Ok(Self { data_1, data_2 })
    }

    /// Custom convert `EncryptedOpenings` to byte vector
    pub fn to_byte_vec(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SIZE);
        bytes.extend_from_slice(&self.data_1);
        bytes.extend_from_slice(&self.data_2);
        bytes
    }

    /// Accessor method for the encrypted data hex display
    pub fn hex_display(&self, full: bool) -> String {
        if full {
            self.to_hex()
        } else {
            let encrypted_data_hex = self.to_hex();
            if encrypted_data_hex.len() > 32 {
                format!(
                    "Some({}..{})",
                    &encrypted_data_hex[0..16],
                    &encrypted_data_hex[encrypted_data_hex.len() - 16..encrypted_data_hex.len()]
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
        Self::from_bytes(&v).map_err(|_| HexError::HexConversionError)
    }

    fn to_hex(&self) -> String {
        to_hex(&self.to_byte_vec())
    }
}

impl Default for EncryptedData {
    fn default() -> Self {
        Self {
            data_1: [0u8; BORSH_64],
            data_2: [0u8; BORSH_X],
        }
    }
}
// EncryptedOpenings errors
#[derive(Debug, Error)]
pub enum EncryptedDataError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(Error),
    #[error("Conversion failed: {0}")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Incorrect length: {0}")]
    IncorrectLength(String),
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
    DomainSeparatedHasher::<Blake256, TransactionSecureNonceKdfDomain>::new_with_label("encrypted_value_and_mask")
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
    fn const_sizes_for_serialization_is_optimized() {
        const BORSH_128: usize = 128;
        const BORSH_64: usize = 64;
        const BORSH_32: usize = 32;
        if SIZE >= BORSH_128 {
            panic!("SIZE is not optimized for serialization");
        }
        if SIZE <= BORSH_64 {
            panic!("SIZE is not optimized for serialization");
        }
        if BORSH_X >= BORSH_32 {
            panic!("BORSH_X is not optimized for serialization");
        }
    }

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
            let amount = MicroTari::from(value);
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
            let amount = MicroTari::from(value);
            let encrypted_data = EncryptedData::encrypt_data(&encryption_key, &commitment, amount, &mask).unwrap();
            let bytes = encrypted_data.to_byte_vec();
            let encrypted_data_from_bytes = EncryptedData::from_bytes(&bytes).unwrap();
            assert_eq!(encrypted_data, encrypted_data_from_bytes);
        }
    }
}
