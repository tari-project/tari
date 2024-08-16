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

use std::{
    convert::{TryFrom, TryInto},
    fmt,
    fmt::{Display, Formatter},
    mem::size_of,
};

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
use tari_common_types::{
    tari_address::{TariAddress, TARI_ADDRESS_INTERNAL_DUAL_SIZE, TARI_ADDRESS_INTERNAL_SINGLE_SIZE},
    types::{Commitment, PrivateKey},
    MaxSizeBytes,
};
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
pub const STATIC_ENCRYPTED_DATA_SIZE_TOTAL: usize = SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_TAG;
const MAX_ENCRYPTED_DATA_SIZE: usize = 256 + STATIC_ENCRYPTED_DATA_SIZE_TOTAL;

// Number of hex characters of encrypted data to display on each side of ellipsis when truncating
const DISPLAY_CUTOFF: usize = 16;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize)]
pub struct EncryptedData {
    #[serde(with = "tari_utilities::serde::hex")]
    data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum PaymentId {
    Empty,
    U64(u64),
    U256(U256),
    Address(TariAddress),
    Open(Vec<u8>),
    AddressAndData(TariAddress, Vec<u8>),
}

impl PaymentId {
    pub fn get_size(&self) -> usize {
        match self {
            PaymentId::Empty => 0,
            PaymentId::U64(_) => size_of::<u64>(),
            PaymentId::U256(_) => size_of::<U256>(),
            PaymentId::Address(a) => a.get_size(),
            PaymentId::Open(v) => v.len(),
            PaymentId::AddressAndData(a, v) => a.get_size() + v.len(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            PaymentId::Empty => Vec::new(),
            PaymentId::U64(v) => (*v).to_le_bytes().to_vec(),
            PaymentId::U256(v) => {
                let mut bytes = vec![0; 32];
                v.to_little_endian(&mut bytes);
                bytes
            },
            PaymentId::Address(v) => v.to_vec(),
            PaymentId::Open(v) => v.clone(),
            PaymentId::AddressAndData(v, d) => {
                let mut bytes = v.to_vec();
                bytes.extend_from_slice(d);
                bytes
            },
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EncryptedDataError> {
        match bytes.len() {
            0 => Ok(PaymentId::Empty),
            8 => {
                let bytes: [u8; 8] = bytes.try_into().expect("Cannot fail, as we already test the length");
                let v = u64::from_le_bytes(bytes);
                Ok(PaymentId::U64(v))
            },
            32 => {
                let v = U256::from_little_endian(bytes);
                Ok(PaymentId::U256(v))
            },
            TARI_ADDRESS_INTERNAL_DUAL_SIZE => {
                let v =
                    TariAddress::from_bytes(bytes).map_err(|e| EncryptedDataError::ByteArrayError(e.to_string()))?;
                Ok(PaymentId::Address(v))
            },
            TARI_ADDRESS_INTERNAL_SINGLE_SIZE => {
                let v =
                    TariAddress::from_bytes(bytes).map_err(|e| EncryptedDataError::ByteArrayError(e.to_string()))?;
                Ok(PaymentId::Address(v))
            },
            len if len < TARI_ADDRESS_INTERNAL_SINGLE_SIZE => Ok(PaymentId::Open(bytes.to_vec())),
            len if len < TARI_ADDRESS_INTERNAL_DUAL_SIZE => {
                if let Ok(address) = TariAddress::from_bytes(&bytes[0..TARI_ADDRESS_INTERNAL_SINGLE_SIZE]) {
                    Ok(PaymentId::AddressAndData(
                        address,
                        bytes[TARI_ADDRESS_INTERNAL_SINGLE_SIZE..].to_vec(),
                    ))
                } else {
                    Ok(PaymentId::Open(bytes.to_vec()))
                }
            },
            _ => {
                if let Ok(address) = TariAddress::from_bytes(&bytes[0..TARI_ADDRESS_INTERNAL_SINGLE_SIZE]) {
                    Ok(PaymentId::AddressAndData(
                        address,
                        bytes[TARI_ADDRESS_INTERNAL_SINGLE_SIZE..].to_vec(),
                    ))
                } else if let Ok(address) = TariAddress::from_bytes(&bytes[0..TARI_ADDRESS_INTERNAL_DUAL_SIZE]) {
                    Ok(PaymentId::AddressAndData(
                        address,
                        bytes[TARI_ADDRESS_INTERNAL_DUAL_SIZE..].to_vec(),
                    ))
                } else {
                    Ok(PaymentId::Open(bytes.to_vec()))
                }
            },
        }
    }
}

impl Display for PaymentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PaymentId::Empty => write!(f, "N/A"),
            PaymentId::U64(v) => write!(f, "{}", v),
            PaymentId::U256(v) => write!(f, "{}", v),
            PaymentId::Address(v) => write!(f, "{}", v.to_emoji_string()),
            PaymentId::Open(v) => write!(f, "byte vector of len: {}", v.len()),
            PaymentId::AddressAndData(v, d) => write!(f, "From {} with data: {:?}", v.to_emoji_string(), d),
        }
    }
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
        commitment: &Commitment,
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
            PaymentId::from_bytes(&bytes[SIZE_VALUE + SIZE_MASK..])?,
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
    use tari_common_types::types::CommitmentFactory;
    use tari_crypto::commitment::HomomorphicCommitmentFactory;

    use super::*;

    #[test]
    fn it_encrypts_and_decrypts_correctly() {
        for payment_id in [
            PaymentId::Empty,
            PaymentId::U64(1),
            PaymentId::U64(156486946518564),
            PaymentId::U256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            PaymentId::Address(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
            ),
            PaymentId::Address(TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap()),
            PaymentId::Open(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            PaymentId::Open(vec![1; 256]),
            PaymentId::AddressAndData(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            ),
            PaymentId::AddressAndData(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                vec![1; 189],
            ),
            PaymentId::AddressAndData(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            ),
            PaymentId::AddressAndData(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                vec![1; 189],
            ),
        ] {
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
                let encrypted_data =
                    EncryptedData::encrypt_data(&encryption_key, &commitment, amount, &mask, payment_id.clone())
                        .unwrap();
                let (decrypted_value, decrypted_mask, decrypted_payment_id) =
                    EncryptedData::decrypt_data(&encryption_key, &commitment, &encrypted_data).unwrap();
                assert_eq!(amount, decrypted_value);
                assert_eq!(mask, decrypted_mask);
                assert_eq!(payment_id, decrypted_payment_id);
                if let Ok((decrypted_value, decrypted_mask, decrypted_payment_id)) =
                    EncryptedData::decrypt_data(&PrivateKey::random(&mut OsRng), &commitment, &encrypted_data)
                {
                    assert_ne!(amount, decrypted_value);
                    assert_ne!(mask, decrypted_mask);
                    assert_ne!(payment_id, decrypted_payment_id);
                }
            }
        }
    }

    #[test]
    fn it_converts_correctly() {
        for payment_id in [
            PaymentId::Empty,
            PaymentId::U64(1),
            PaymentId::U64(156486946518564),
            PaymentId::U256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            PaymentId::Address(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
            ),
            PaymentId::Address(TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap()),
            PaymentId::Open(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            PaymentId::Open(vec![1; 256]),
            PaymentId::AddressAndData(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            ),
            PaymentId::AddressAndData(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                vec![1; 189],
            ),
            PaymentId::AddressAndData(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            ),
            PaymentId::AddressAndData(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                vec![1; 189],
            ),
        ] {
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
                let encrypted_data =
                    EncryptedData::encrypt_data(&encryption_key, &commitment, amount, &mask, payment_id.clone())
                        .unwrap();
                let bytes = encrypted_data.to_byte_vec();
                let encrypted_data_from_bytes = EncryptedData::from_bytes(&bytes).unwrap();
                assert_eq!(encrypted_data, encrypted_data_from_bytes);
            }
        }
    }
}
