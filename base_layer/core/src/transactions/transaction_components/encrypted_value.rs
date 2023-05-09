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

use super::EncryptedValueKey;
use crate::transactions::{tari_amount::MicroTari, TransactionFixedNonceKdfDomain};

const SIZE: usize = size_of::<u64>() + size_of::<Tag>();

/// value: u64 + tag: [u8; 16]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize)]
pub struct EncryptedValue(#[serde(with = "tari_utilities::serde::hex")] pub [u8; SIZE]);

impl Default for EncryptedValue {
    fn default() -> Self {
        Self([0; SIZE])
    }
}

impl ByteArray for EncryptedValue {
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        ByteArray::from_bytes(bytes).map(Self)
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(Error),
}

// chacha error is not StdError compatible
impl From<Error> for EncryptionError {
    fn from(err: Error) -> Self {
        Self::EncryptionFailed(err)
    }
}

impl EncryptedValue {
    const TAG: &'static [u8] = b"TARI_AAD_VALUE";

    /// Encrypt the value (with fixed length) using ChaCha20-Poly1305 with a fixed zero nonce to save space -
    /// this design assumes a unique `encryption_key` every time and therefore uses a fixed nonce
    /// Notes: - `encryption_key`-`commitment` input pairs must be unique, or the value will leak
    ///        - `commitment` is used used here to ensure uniqueness of the key as well as to bind the encrypted value
    ///          to the commitment
    pub fn encrypt_value(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        value: MicroTari,
    ) -> Result<EncryptedValue, EncryptionError> {
        let aead_key = kdf_aead(encryption_key, commitment);
        // Encrypt the value (with fixed length) using ChaCha20-Poly1305 with a fixed zero nonce
        let aead_payload = Payload {
            msg: &value.as_u64().to_le_bytes(),
            aad: Self::TAG,
        };
        // Included in the public transaction
        let buffer = ChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()))
            .encrypt(&Nonce::default(), aead_payload)?;
        let mut data: [u8; SIZE] = [0; SIZE];
        data.copy_from_slice(&buffer);
        Ok(EncryptedValue(data))
    }

    /// Authenticate and decrypt the value
    /// Note: This design (similar to other AEADs) is not key committing, thus the caller must not rely on successful
    ///       decryption to assert that the expected key was used
    pub fn decrypt_value(
        encryption_key: &PrivateKey,
        commitment: &Commitment,
        value: &EncryptedValue,
    ) -> Result<MicroTari, EncryptionError> {
        let aead_key = kdf_aead(encryption_key, commitment);
        let aead_payload = Payload {
            msg: value.as_bytes(),
            aad: Self::TAG,
        };
        let mut value_bytes = [0u8; 8];
        let decrypted_bytes = ChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()))
            .decrypt(&Nonce::default(), aead_payload)?;
        value_bytes.clone_from_slice(&decrypted_bytes[..8]);
        Ok(u64::from_le_bytes(value_bytes).into())
    }
}

// Generate a ChaCha20-Poly1305 key from a private key and commitment using Blake2b
fn kdf_aead(private_key: &PrivateKey, commitment: &Commitment) -> EncryptedValueKey {
    let mut aead_key = EncryptedValueKey::from(SafeArray::default());
    DomainSeparatedHasher::<Blake256, TransactionFixedNonceKdfDomain>::new_with_label("encrypted_value")
        .chain(private_key.as_bytes())
        .chain(commitment.as_bytes())
        .finalize_into(GenericArray::from_mut_slice(aead_key.reveal_mut()));

    aead_key
}

#[cfg(test)]
mod test {
    use rand::rngs::OsRng;
    use tari_common_types::types::PrivateKey;
    use tari_crypto::keys::{PublicKey, SecretKey};

    use super::*;

    #[test]
    fn const_sizes_for_serialization_is_optimized() {
        const BORSH_32: usize = 32;
        if SIZE > BORSH_32 {
            panic!("SIZE is not optimized for serialization");
        }
    }

    #[test]
    fn it_encrypts_and_decrypts_correctly() {
        for value in [0, 123456, 654321, u64::MAX] {
            let commitment = Commitment::from_public_key(&PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)));
            let encryption_key = PrivateKey::random(&mut OsRng);
            let amount = MicroTari::from(value);
            let encrypted_value = EncryptedValue::encrypt_value(&encryption_key, &commitment, amount).unwrap();
            let decrypted_value =
                EncryptedValue::decrypt_value(&encryption_key, &commitment, &encrypted_value).unwrap();
            assert_eq!(amount, decrypted_value);
        }
    }
}
