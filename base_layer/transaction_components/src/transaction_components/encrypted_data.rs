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
    KeyInit,
    Tag,
    XChaCha20Poly1305,
    XNonce,
    aead::{AeadCore, AeadInPlace, Error, OsRng},
};
use digest::{FixedOutput, consts::U32, generic_array::GenericArray};
use primitive_types::U256;
use serde::{Deserialize, Deserializer, Serialize};
use tari_common_types::types::{CompressedCommitment, PrivateKey};
use tari_crypto::{hashing::DomainSeparatedHasher, keys::SecretKey};
use tari_hashing::TransactionSecureNonceKdfDomain;
use tari_max_size::MaxSizeBytes;
use tari_utilities::{
    ByteArray,
    ByteArrayError,
    hex::{Hex, HexError, from_hex, to_hex},
    safe_array::SafeArray,
};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use super::EncryptedDataKey;
use crate::{MicroMinotari, transaction_components::MemoField};

// Useful size constants, each in bytes
const SIZE_NONCE: usize = size_of::<XNonce>();
pub const SIZE_VALUE: usize = size_of::<u64>();
const SIZE_MASK: usize = PrivateKey::KEY_LEN;
const SIZE_TAG: usize = size_of::<Tag>();
const SIZE_TAG_AND_NONCE: usize = SIZE_TAG + SIZE_NONCE;
pub const SIZE_U256: usize = size_of::<U256>();
/// The size of the fixed part of [`EncryptedData`] (nonce, value, mask and tag), which is also its minimum size.
///
/// The bound is consensus critical: it is applied at decode time, so changing it changes which blocks and transactions
/// a node can decode at all and is a flag-day (hard) fork.
pub const STATIC_ENCRYPTED_DATA_SIZE_TOTAL: usize = SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_TAG;
/// The maximum size of [`EncryptedData`]: the fixed part plus a payment id of at most 256 bytes.
///
/// The bound is consensus critical: it is applied at decode time, so changing it changes which blocks and transactions
/// a node can decode at all and is a flag-day (hard) fork.
/// It must stay `>=` every network's `max_extra_encrypted_data_byte_size + STATIC_ENCRYPTED_DATA_SIZE_TOTAL`, which
/// is the (smaller) validation rule.
pub const MAX_ENCRYPTED_DATA_SIZE: usize = 256 + STATIC_ENCRYPTED_DATA_SIZE_TOTAL;

// Number of hex characters of encrypted data to display on each side of ellipsis when truncating
const DISPLAY_CUTOFF: usize = 16;

/// Encrypted value, mask and payment id of a transaction output.
///
/// `STATIC_ENCRYPTED_DATA_SIZE_TOTAL <= len() <= MAX_ENCRYPTED_DATA_SIZE` is an invariant of this type: every
/// constructor, and every decoder (serde and borsh, see the hand written `Deserialize` and `BorshDeserialize`
/// implementations below), routes through [`EncryptedData::from_bytes`]. Decoders must not be derived, as a derived
/// decoder would only enforce the upper bound of the inner `MaxSizeBytes` and accept values that are too short.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash, BorshSerialize, Zeroize)]
pub struct EncryptedData {
    #[serde(with = "tari_utilities::serde::hex")]
    data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
}

/// Mirror of the serde shape of [`EncryptedData`] (a struct named `EncryptedData` with a single hex/bytes field
/// `data`), used only to decode the wire format before the length invariants are checked.
#[derive(Deserialize)]
#[serde(rename = "EncryptedData")]
struct EncryptedDataSerde {
    #[serde(with = "tari_utilities::serde::hex")]
    data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
}

impl<'de> Deserialize<'de> for EncryptedData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let decoded = EncryptedDataSerde::deserialize(deserializer)?;
        EncryptedData::from_bytes(decoded.data.as_bytes()).map_err(serde::de::Error::custom)
    }
}

impl BorshDeserialize for EncryptedData {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let data = MaxSizeBytes::<MAX_ENCRYPTED_DATA_SIZE>::deserialize_reader(reader)?;
        EncryptedData::from_bytes(data.as_bytes())
            .map_err(|e| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, e.to_string()))
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
        commitment: &CompressedCommitment,
        value: MicroMinotari,
        mask: &PrivateKey,
        memo: MemoField,
    ) -> Result<EncryptedData, EncryptedDataError> {
        // The memo size drives the buffer sizes below. It is checked before encoding, as an oversized memo is the
        // only one that could fail to encode, and then it must match the encoded memo.
        let data_size = STATIC_ENCRYPTED_DATA_SIZE_TOTAL.saturating_add(memo.get_size());
        if data_size > MAX_ENCRYPTED_DATA_SIZE {
            return Err(EncryptedDataError::InvalidMemoSize(format!(
                "Encrypted data would be {data_size} bytes, the maximum is {MAX_ENCRYPTED_DATA_SIZE}"
            )));
        }
        let memo_bytes = memo.to_bytes();
        if memo_bytes.len() != memo.get_size() {
            return Err(EncryptedDataError::InvalidMemoSize(format!(
                "Encoded memo is {} bytes, expected {}",
                memo_bytes.len(),
                memo.get_size()
            )));
        }

        // Encode the value and mask
        let plaintext_size = SIZE_VALUE.saturating_add(SIZE_MASK).saturating_add(memo_bytes.len());
        let mut bytes = Zeroizing::new(vec![0; plaintext_size]);
        bytes
            .get_mut(..SIZE_VALUE)
            .expect("Already checked")
            .copy_from_slice(value.as_u64().to_le_bytes().as_ref());
        bytes
            .get_mut(SIZE_VALUE..SIZE_VALUE + SIZE_MASK)
            .expect("Already checked")
            .copy_from_slice(mask.as_bytes());
        bytes
            .get_mut(SIZE_VALUE + SIZE_MASK..)
            .expect("Already checked")
            .copy_from_slice(&memo_bytes);

        // Produce a secure random nonce
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Encrypt in place
        let tag = cipher.encrypt_in_place_detached(&nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice())?;

        // Put everything together: nonce, ciphertext, tag
        let mut data = vec![0; data_size];
        data.get_mut(..SIZE_TAG).expect("Already checked").copy_from_slice(&tag);
        data.get_mut(SIZE_TAG..SIZE_TAG + SIZE_NONCE)
            .expect("Already checked")
            .copy_from_slice(&nonce);
        data.get_mut(SIZE_TAG_AND_NONCE..SIZE_TAG_AND_NONCE.saturating_add(plaintext_size))
            .expect("Already checked")
            .copy_from_slice(bytes.as_slice());
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
    ) -> Result<(MicroMinotari, PrivateKey, MemoField), EncryptedDataError> {
        // Extract the nonce, ciphertext, and tag
        let tag = Tag::from_slice(
            encrypted_data
                .as_bytes()
                .get(..SIZE_TAG)
                .ok_or(EncryptedDataError::IncorrectLength("Tag too short".to_string()))?,
        );
        let nonce = XNonce::from_slice(
            encrypted_data
                .as_bytes()
                .get(SIZE_TAG..SIZE_TAG + SIZE_NONCE)
                .ok_or(EncryptedDataError::IncorrectLength("Data too short".to_string()))?,
        );
        let mut bytes = Zeroizing::new(vec![
            0;
            encrypted_data
                .data
                .len()
                .saturating_sub(SIZE_TAG)
                .saturating_sub(SIZE_NONCE)
        ]);
        bytes.copy_from_slice(
            encrypted_data
                .as_bytes()
                .get(SIZE_TAG + SIZE_NONCE..)
                .ok_or(EncryptedDataError::IncorrectLength("Data too short".to_string()))?,
        );

        // Set up the AEAD
        let aead_key = kdf_aead(encryption_key, commitment);
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(aead_key.reveal()));

        // Decrypt in place
        cipher.decrypt_in_place_detached(nonce, ENCRYPTED_DATA_AAD, bytes.as_mut_slice(), tag)?;

        // Decode the value and mask
        let mut value_bytes = [0u8; SIZE_VALUE];
        value_bytes.copy_from_slice(
            bytes
                .get(0..SIZE_VALUE)
                .ok_or(EncryptedDataError::IncorrectLength("Value too short".to_string()))?,
        );
        Ok((
            u64::from_le_bytes(value_bytes).into(),
            PrivateKey::from_canonical_bytes(
                bytes
                    .get(SIZE_VALUE..SIZE_VALUE + SIZE_MASK)
                    .ok_or(EncryptedDataError::IncorrectLength("Data too short".to_string()))?,
            )?,
            MemoField::from_bytes(
                bytes
                    .get(SIZE_VALUE + SIZE_MASK..)
                    .ok_or(EncryptedDataError::IncorrectLength("Data too long".to_string()))?,
            ),
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

    /// Get a byte vector with the encrypted data contents
    pub fn to_byte_vec(&self) -> Vec<u8> {
        self.data.clone().into()
    }

    /// Get a byte slice with the encrypted data contents
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Consumes self and returns the encrypted data as a byte vector
    pub fn into_vec(self) -> Vec<u8> {
        self.data.into_vec()
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
                    &encrypted_data_hex
                        [encrypted_data_hex.len().saturating_sub(DISPLAY_CUTOFF)..encrypted_data_hex.len()]
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
#[derive(Debug, Error, PartialEq, Clone)]
pub enum EncryptedDataError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(Error),
    #[error("Conversion failed: {0}")]
    ByteArrayError(String),
    #[error("Incorrect length: {0}")]
    IncorrectLength(String),
    #[error("Invalid memo size: {0}")]
    InvalidMemoSize(String),
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
    #![allow(clippy::indexing_slicing)]
    use static_assertions::const_assert;
    use tari_common_types::{
        tari_address::{TARI_ADDRESS_INTERNAL_DUAL_SIZE, TARI_ADDRESS_INTERNAL_SINGLE_SIZE},
        types::CommitmentFactory,
    };
    use tari_crypto::commitment::HomomorphicCommitmentFactory;

    use super::*;

    #[test]
    fn test_premine() {
        let id = 999u64;
        let value = 123456;
        let mask = PrivateKey::default();
        let commitment =
            CompressedCommitment::from_commitment(CommitmentFactory::default().commit(&mask, &PrivateKey::from(value)));
        let encryption_key = PrivateKey::random(&mut rand::rng());
        let amount = MicroMinotari::from(value);
        let encrypted_data = {
            let mut bytes = Zeroizing::new(vec![0; SIZE_VALUE + SIZE_MASK + SIZE_VALUE]);
            bytes[..SIZE_VALUE].copy_from_slice(value.to_le_bytes().as_ref());
            bytes[SIZE_VALUE..SIZE_VALUE + SIZE_MASK].copy_from_slice(mask.as_bytes());
            bytes[SIZE_VALUE + SIZE_MASK..].copy_from_slice(&id.to_le_bytes().to_vec());

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
            data[..SIZE_TAG].copy_from_slice(&tag);
            data[SIZE_TAG..SIZE_TAG + SIZE_NONCE].copy_from_slice(&nonce);
            data[SIZE_TAG + SIZE_NONCE..SIZE_TAG + SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_VALUE]
                .copy_from_slice(bytes.as_slice());
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
        if decrypted_payment_id.is_open() {
            let data = decrypted_payment_id.get_payment_id();
            let bytes: [u8; SIZE_VALUE] = data.try_into().unwrap();
            let v = u64::from_le_bytes(bytes);
            assert_eq!(v, id);
        } else {
            panic!("Expected PaymentId::Open");
        }
    }

    fn value_of_len(len: usize) -> EncryptedData {
        let bytes = (0..len).map(|i| u8::try_from(i % 256).unwrap()).collect::<Vec<_>>();
        EncryptedData::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn serde_json_rejects_short_values_and_round_trips_valid_ones_unchanged() {
        let short = format!(
            r#"{{"data":"{}"}}"#,
            to_hex(&[7u8; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1])
        );
        let err = serde_json::from_str::<EncryptedData>(&short).unwrap_err();
        assert!(err.to_string().contains("at least"), "{}", err);
        let empty = r#"{"data":""}"#;
        assert!(serde_json::from_str::<EncryptedData>(empty).is_err());

        for len in [STATIC_ENCRYPTED_DATA_SIZE_TOTAL, MAX_ENCRYPTED_DATA_SIZE] {
            let value = value_of_len(len);
            let json = serde_json::to_string(&value).unwrap();
            // The representation is unchanged: a struct with a single hex string field `data`
            assert_eq!(json, format!(r#"{{"data":"{}"}}"#, to_hex(value.as_bytes())));
            assert_eq!(serde_json::from_str::<EncryptedData>(&json).unwrap(), value);
        }

        let too_long = format!(r#"{{"data":"{}"}}"#, to_hex(&[7u8; MAX_ENCRYPTED_DATA_SIZE + 1]));
        assert!(serde_json::from_str::<EncryptedData>(&too_long).is_err());
    }

    #[test]
    fn bincode_rejects_short_values_and_round_trips_valid_ones_unchanged() {
        // The pre-change encoding (via `tari_utilities::serde::hex`) is `serialize_bytes` for binary formats
        #[derive(Serialize)]
        struct Legacy<'a> {
            #[serde(with = "serde_bytes_shim")]
            data: &'a [u8],
        }
        mod serde_bytes_shim {
            pub fn serialize<S: serde::Serializer>(data: &&[u8], s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(data)
            }
        }

        let short = bincode::serialize(&Legacy {
            data: &[7u8; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1],
        })
        .unwrap();
        let err = bincode::deserialize::<EncryptedData>(&short).unwrap_err();
        assert!(err.to_string().contains("at least"), "{}", err);

        for len in [STATIC_ENCRYPTED_DATA_SIZE_TOTAL, MAX_ENCRYPTED_DATA_SIZE] {
            let value = value_of_len(len);
            let encoded = bincode::serialize(&value).unwrap();
            assert_eq!(encoded, bincode::serialize(&Legacy { data: value.as_bytes() }).unwrap());
            assert_eq!(bincode::deserialize::<EncryptedData>(&encoded).unwrap(), value);
        }
    }

    #[test]
    fn borsh_rejects_short_values_and_round_trips_valid_ones_unchanged() {
        let short = borsh::to_vec(&vec![7u8; STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1]).unwrap();
        let err = EncryptedData::try_from_slice(&short).unwrap_err();
        assert!(err.to_string().contains("at least"), "{}", err);
        assert!(EncryptedData::try_from_slice(&borsh::to_vec(&Vec::<u8>::new()).unwrap()).is_err());

        for len in [STATIC_ENCRYPTED_DATA_SIZE_TOTAL, MAX_ENCRYPTED_DATA_SIZE] {
            let value = value_of_len(len);
            let encoded = borsh::to_vec(&value).unwrap();
            // The encoding is unchanged: the plain borsh encoding of a `Vec<u8>`
            assert_eq!(encoded, borsh::to_vec(&value.to_byte_vec()).unwrap());
            assert_eq!(EncryptedData::try_from_slice(&encoded).unwrap(), value);
        }

        let too_long = borsh::to_vec(&vec![7u8; MAX_ENCRYPTED_DATA_SIZE + 1]).unwrap();
        assert!(EncryptedData::try_from_slice(&too_long).is_err());
    }

    #[test]
    fn encrypt_data_rejects_oversized_memos() {
        let mask = PrivateKey::random(&mut rand::rng());
        let commitment =
            CompressedCommitment::from_commitment(CommitmentFactory::default().commit(&mask, &PrivateKey::from(1)));
        // One byte over the limit, and far over it
        for len in [
            MAX_ENCRYPTED_DATA_SIZE - STATIC_ENCRYPTED_DATA_SIZE_TOTAL,
            MAX_ENCRYPTED_DATA_SIZE,
        ] {
            let memo = MemoField::raw_unchecked(vec![1u8; len]);
            let result = EncryptedData::encrypt_data(&PrivateKey::default(), &commitment, 1.into(), &mask, memo);
            assert!(
                matches!(result, Err(EncryptedDataError::InvalidMemoSize(_))),
                "{result:?}"
            );
        }
        // The largest memo that fits
        let memo = MemoField::raw_unchecked(vec![
            1u8;
            MAX_ENCRYPTED_DATA_SIZE - STATIC_ENCRYPTED_DATA_SIZE_TOTAL - 1
        ]);
        assert!(EncryptedData::encrypt_data(&PrivateKey::default(), &commitment, 1.into(), &mask, memo).is_ok());
    }

    #[test]
    fn address_sizes_increase_as_expected() {
        const_assert!(SIZE_VALUE < SIZE_U256);
        const_assert!(SIZE_U256 < TARI_ADDRESS_INTERNAL_SINGLE_SIZE);
        const_assert!(TARI_ADDRESS_INTERNAL_SINGLE_SIZE < TARI_ADDRESS_INTERNAL_DUAL_SIZE);
    }
}
