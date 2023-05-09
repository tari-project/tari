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

//! Encrypted openings using the the standard variant ChaCha20-Poly1305 encryption with a fixed zero nonce to save
//! space - this design assumes a unique `encryption_key` every time and therefore uses a fixed nonce.

use std::mem::size_of;

use borsh::{BorshDeserialize, BorshSerialize};
use chacha20poly1305::{
    aead::{Aead, Error, Payload},
    ChaCha20Poly1305,
    KeyInit,
    Nonce,
    Tag,
};
use digest::{generic_array::GenericArray, FixedOutput};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, PrivateKey};
use tari_crypto::{hash::blake2::Blake256, hashing::DomainSeparatedHasher};
use tari_utilities::{safe_array::SafeArray, ByteArray, ByteArrayError};
use thiserror::Error;
use zeroize::Zeroize;

use super::EncryptedOpeningsKey;
use crate::transactions::{tari_amount::MicroTari, TransactionFixedNonceKdfDomain};

const VALUE_SIZE: usize = size_of::<u64>();
const KEY_SIZE: usize = size_of::<PrivateKey>();
const TAG_SIZE: usize = size_of::<Tag>();
const SIZE: usize = VALUE_SIZE + KEY_SIZE + TAG_SIZE;
const BORSH_32: usize = 32;
const BORSH_X: usize = SIZE - BORSH_32;

/// Encrypted openings for the standard variant ChaCha20-Poly1305 encryption
/// Borsh schema only accept array sizes 0 - 32, 64, 65, 128, 256, 512, 1024 and 2048
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize)]
pub struct EncryptedOpeningsS {
    #[serde(with = "tari_utilities::serde::hex")]
    data_1: [u8; BORSH_32],
    #[serde(with = "tari_utilities::serde::hex")]
    data_2: [u8; BORSH_X],
}

impl EncryptedOpeningsS {
    /// Custom convert `EncryptedOpenings` to bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptedOpeningsError> {
        if bytes.len() != SIZE {
            return Err(EncryptedOpeningsError::IncorrectLength(format!(
                "Expected {} bytes, got {}",
                SIZE,
                bytes.len()
            )));
        }
        let mut data_1: [u8; BORSH_32] = [0u8; BORSH_32];
        data_1.copy_from_slice(bytes.get(..BORSH_32).ok_or_else(|| EncryptedOpeningsError::IncorrectLength(
            "Out of bounds 'data_1'".to_string(),
        ))?);
        let mut data_2: [u8; BORSH_X] = [0u8; BORSH_X];
        data_2.copy_from_slice(
            bytes
                .get(BORSH_32..SIZE)
                .ok_or_else(|| EncryptedOpeningsError::IncorrectLength(
                    "Out of bounds 'data_2'".to_string(),
                ))?,
        );
        Ok(Self { data_1, data_2 })
    }

    /// Custom convert `EncryptedOpenings` to byte vector
    pub fn as_byte_vector(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SIZE);
        bytes.extend_from_slice(&self.data_1);
        bytes.extend_from_slice(&self.data_2);
        bytes
    }
}

impl Default for EncryptedOpeningsS {
    fn default() -> Self {
        Self {
            data_1: [0u8; BORSH_32],
            data_2: [0u8; BORSH_X],
        }
    }
}

/// EncryptedOpenings errors
#[derive(Debug, Error)]
pub enum EncryptedOpeningsError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(Error),
    #[error("Conversion failed: {0}")]
    ByteArrayError(#[from] ByteArrayError),
    #[error("Incorrect length: {0}")]
    IncorrectLength(String),
}

// Chacha error is not StdError compatible
impl From<Error> for EncryptedOpeningsError {
    fn from(err: Error) -> Self {
        Self::EncryptionFailed(err)
    }
}

impl EncryptedOpeningsS {
    const TAG: &'static [u8] = b"TARI_AAD_VALUE_AND_MASK_STANDARD_VARIANT";

    /// Encrypt the value and mask (with fixed length) using ChaCha20-Poly1305 with a fixed zero nonce to save space -
    /// this design assumes a unique `encryption_key` every time and therefore uses a fixed nonce
    /// Notes: - `encryption_key`-`commitment` input pairs must be unique, or the value and mask will leak
    ///        - `commitment` is used used here to ensure uniqueness of the key as well as to bind the encrypted value
    ///           and mask to the commitment
    pub fn encrypt_openings(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        value: MicroTari,
        mask: &PrivateKey,
    ) -> Result<EncryptedOpeningsS, EncryptedOpeningsError> {
        let mut openings = value.as_u64().to_le_bytes().to_vec();
        openings.append(&mut mask.to_vec());
        let aead_payload = Payload {
            msg: openings.as_slice(),
            aad: Self::TAG,
        };
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));
        let ciphertext = cipher.encrypt(&Nonce::default(), aead_payload)?;

        EncryptedOpeningsX::from_bytes(ciphertext.as_slice())
    }

    /// Authenticate and decrypt the value and mask
    /// Note: This design (similar to other AEADs) is not key committing, thus the caller must not rely on successful
    ///       decryption to assert that the expected key was used
    pub fn decrypt_openings(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        encrypted_openings: &EncryptedOpeningsS,
    ) -> Result<(MicroTari, PrivateKey), EncryptedOpeningsError> {
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = ChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));
        let aead_payload = Payload {
            msg: &encrypted_openings.as_byte_vector(),
            aad: Self::TAG,
        };
        let decrypted_bytes = cipher.decrypt(&Nonce::default(), aead_payload)?;
        let mut value_bytes = [0u8; VALUE_SIZE];
        value_bytes.clone_from_slice(&decrypted_bytes[0..VALUE_SIZE]);
        let mut mask_bytes = [0u8; KEY_SIZE];
        mask_bytes.clone_from_slice(&decrypted_bytes[VALUE_SIZE..VALUE_SIZE + KEY_SIZE]);
        Ok((
            u64::from_le_bytes(value_bytes).into(),
            PrivateKey::from_bytes(&mask_bytes)?,
        ))
    }
}

// Generate a ChaCha20-Poly1305 key from a private key and commitment using Blake2b
fn kdf_aead(encryption_key: &PrivateKey, commitment: &Commitment) -> EncryptedOpeningsKey {
    let mut aead_key = EncryptedOpeningsKey::from(SafeArray::default());
    DomainSeparatedHasher::<Blake256, TransactionFixedNonceKdfDomain>::new_with_label("encrypted_value_and_mask")
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
        const BORSH_64: usize = 64;
        const BORSH_32: usize = 32;
        if SIZE >= BORSH_64 {
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
            let encrypted_openings =
                EncryptedOpeningsX::encrypt_openings(&encryption_key, &commitment, amount, &mask).unwrap();
            let (decrypted_value, decrypted_mask) =
                EncryptedOpeningsX::decrypt_openings(&encryption_key, &commitment, &encrypted_openings).unwrap();
            assert_eq!(amount, decrypted_value);
            assert_eq!(mask, decrypted_mask);
        }
    }
}
