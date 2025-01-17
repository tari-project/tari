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
use num_traits::{FromPrimitive, ToBytes};
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    tari_address::{TariAddress, TARI_ADDRESS_INTERNAL_DUAL_SIZE, TARI_ADDRESS_INTERNAL_SINGLE_SIZE},
    types::{Commitment, PrivateKey},
};
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
use crate::transactions::tari_amount::MicroMinotari;
// Useful size constants, each in bytes
const SIZE_NONCE: usize = size_of::<XNonce>();
const SIZE_VALUE: usize = size_of::<u64>();
const SIZE_MASK: usize = PrivateKey::KEY_LEN;
const SIZE_TAG: usize = size_of::<Tag>();
const SIZE_U256: usize = size_of::<U256>();
pub const STATIC_ENCRYPTED_DATA_SIZE_TOTAL: usize = SIZE_NONCE + SIZE_VALUE + SIZE_MASK + SIZE_TAG;
const MAX_ENCRYPTED_DATA_SIZE: usize = 256 + STATIC_ENCRYPTED_DATA_SIZE_TOTAL;

// Number of hex characters of encrypted data to display on each side of ellipsis when truncating
const DISPLAY_CUTOFF: usize = 16;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Zeroize)]
pub struct EncryptedData {
    #[serde(with = "tari_utilities::serde::hex")]
    data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub enum TxType {
    #[default]
    PaymentToOther = 0b0000,
    PaymentToSelf = 0b0001,
    Burn = 0b0010,
    CoinSplit = 0b0011,
    CoinJoin = 0b0100,
    ValidatorNodeRegistration = 0b0101,
    ClaimAtomicSwap = 0b0110,
    HtlcAtomicSwapRefund = 0b0111,
    CodeTemplateRegistration = 0b1000,
    ImportedUtxoNoneRewindable = 0b1001,
}

impl TxType {
    fn from_u8(value: u8) -> Self {
        TxType::from_u16(u16::from(value))
    }

    fn from_u16(value: u16) -> Self {
        match value & 0b1111 {
            0b0000 => TxType::PaymentToOther,
            0b0001 => TxType::PaymentToSelf,
            0b0010 => TxType::Burn,
            0b0011 => TxType::CoinSplit,
            0b0100 => TxType::CoinJoin,
            0b0101 => TxType::ValidatorNodeRegistration,
            0b0110 => TxType::ClaimAtomicSwap,
            0b0111 => TxType::HtlcAtomicSwapRefund,
            0b1000 => TxType::CodeTemplateRegistration,
            0b1001 => TxType::ImportedUtxoNoneRewindable,
            _ => TxType::default(),
        }
    }

    pub fn as_u8(&self) -> u8 {
        match self {
            TxType::PaymentToOther => 0b0000,
            TxType::PaymentToSelf => 0b0001,
            TxType::Burn => 0b0010,
            TxType::CoinSplit => 0b0011,
            TxType::CoinJoin => 0b0100,
            TxType::ValidatorNodeRegistration => 0b0101,
            TxType::ClaimAtomicSwap => 0b0110,
            TxType::HtlcAtomicSwapRefund => 0b0111,
            TxType::CodeTemplateRegistration => 0b1000,
            TxType::ImportedUtxoNoneRewindable => 0b1001,
        }
    }

    fn as_bytes(&self) -> Vec<u8> {
        vec![self.as_u8()]
    }
}

impl Display for TxType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TxType::PaymentToOther => write!(f, "PaymentToOther"),
            TxType::PaymentToSelf => write!(f, "PaymentToSelf"),
            TxType::Burn => write!(f, "Burn"),
            TxType::CoinSplit => write!(f, "CoinSplit"),
            TxType::CoinJoin => write!(f, "CoinJoin"),
            TxType::ValidatorNodeRegistration => write!(f, "ValidatorNodeRegistration"),
            TxType::ClaimAtomicSwap => write!(f, "ClaimAtomicSwap"),
            TxType::HtlcAtomicSwapRefund => write!(f, "HtlcAtomicSwapRefund"),
            TxType::CodeTemplateRegistration => write!(f, "CodeTemplateRegistration"),
            TxType::ImportedUtxoNoneRewindable => write!(f, "ImportedUtxoNoneRewindable"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub enum PaymentId {
    /// No payment ID.
    #[default]
    Empty,
    /// A u64 number.
    U64(u64),
    /// A u256 number.
    U256(U256),
    /// Open - the user optionally specifies 'user_data' ('tx_type' is added by the system).
    Open { user_data: Vec<u8>, tx_type: TxType },
    /// This payment ID is automatically generated by the system for output UTXOs. The optional user specified
    /// `PaymentId::Open` payment ID will be assigned to `tx_type` and `user_data`; the system adds in the sender
    /// address.
    AddressAndData {
        sender_address: TariAddress,
        tx_type: TxType,
        user_data: Vec<u8>,
    },
    /// This payment ID is automatically generated by the system for change outputs. The optional user specified
    /// `PaymentId::Open` payment ID will be assigned to `tx_type` and `user_data`; the system adds in the other data
    /// address.
    TransactionInfo {
        recipient_address: TariAddress,
        sender_one_sided: bool,
        amount: MicroMinotari,
        fee: MicroMinotari,
        weight: u64,
        inputs_count: usize,
        outputs_count: usize,
        tx_type: TxType,
        user_data: Vec<u8>,
    },
}

impl PaymentId {
    const SIZE_META_DATA: usize = 10;
    const SIZE_VALUE_AND_META_DATA: usize = SIZE_VALUE + PaymentId::SIZE_META_DATA;

    pub fn get_size(&self) -> usize {
        match self {
            PaymentId::Empty => 0,
            PaymentId::U64(_) => SIZE_VALUE,
            PaymentId::U256(_) => SIZE_U256,
            PaymentId::Open { user_data, .. } => user_data.len() + 1,
            PaymentId::AddressAndData {
                sender_address,
                user_data,
                ..
            } => sender_address.get_size() + user_data.len() + 1,
            PaymentId::TransactionInfo {
                recipient_address,
                user_data,
                ..
            } => recipient_address.get_size() + PaymentId::SIZE_VALUE_AND_META_DATA + user_data.len(),
        }
    }

    /// Helper function to set the 'amount' of a 'PaymentId::TransactionInfo'
    pub fn transaction_info_set_amount(&mut self, amount: MicroMinotari) {
        if let PaymentId::TransactionInfo { amount: a, .. } = self {
            *a = amount;
        }
    }

    pub fn get_type(&self) -> TxType {
        match self {
            PaymentId::Open { tx_type, .. } |
            PaymentId::AddressAndData { tx_type, .. } |
            PaymentId::TransactionInfo { tx_type, .. } => tx_type.clone(),
            _ => TxType::default(),
        }
    }

    /// Helper function to set the 'recipient_address' of a 'PaymentId::TransactionInfo'
    pub fn transaction_info_set_address(&mut self, address: TariAddress) {
        if let PaymentId::TransactionInfo { recipient_address, .. } = self {
            *recipient_address = address
        }
    }

    /// Helper function to convert a 'PaymentId::Open' or 'PaymentId::Empty' to a 'PaymentId::AddressAndData', with the
    /// optional 'tx_type' only applicable to 'PaymentId::Open', otherwise 'payment_id' is kept as is.
    pub fn add_sender_address(
        payment_id: PaymentId,
        sender_address: TariAddress,
        tx_type: Option<TxType>,
    ) -> PaymentId {
        match payment_id {
            PaymentId::Open { user_data, tx_type } => PaymentId::AddressAndData {
                sender_address,
                tx_type,
                user_data,
            },
            PaymentId::Empty => PaymentId::AddressAndData {
                sender_address,
                tx_type: tx_type.unwrap_or_default(),
                user_data: vec![],
            },
            _ => payment_id,
        }
    }

    // This method is infallible; any out-of-bound values will be zeroed.
    fn pack_meta_data(&self) -> Vec<u8> {
        if let PaymentId::TransactionInfo {
            fee,
            weight,
            inputs_count,
            outputs_count,
            sender_one_sided,
            tx_type,
            ..
        } = self
        {
            let mut bytes = Vec::with_capacity(10);
            // Zero out-of-bound values
            // - Use 4 bytes for 'fee', max value: 4,294,967,295
            let fee = if fee.as_u64() > 2u64.pow(32) - 1 {
                0
            } else {
                fee.as_u64()
            };
            // - Use 2 bytes for 'weight', max value: 65,535
            let weight = if *weight > 2u64.pow(16) - 1 { 0 } else { *weight };
            // - Use 2 bytes less 1 bit for 'inputs_count', max value: 32,767, and 1 bit for 'sender_one_sided'
            let inputs_count = if *inputs_count > 2usize.pow(15) - 1 {
                0
            } else {
                *inputs_count
            };
            // - Use 2 bytes less 4 bits for 'outputs_count', max value: 4,095, and 3 bits for 'tx_meta_data'
            let outputs_count = if *outputs_count > 2usize.pow(12) - 1 {
                0
            } else {
                *outputs_count
            };
            // Pack
            bytes.extend_from_slice(&fee.to_be_bytes()[4..]);
            bytes.extend_from_slice(&weight.to_be_bytes()[6..]);
            let inputs_count_packed = (u16::from_usize(inputs_count).unwrap_or_default() & 0b0111111111111111) |
                (u16::from(*sender_one_sided) << 15);
            bytes.extend_from_slice(&inputs_count_packed.to_be_bytes());
            let outputs_count_packed = (u16::from_usize(outputs_count).unwrap_or_default() & 0b0000111111111111) |
                (u16::from(tx_type.as_u8()) << 12);
            bytes.extend_from_slice(&outputs_count_packed.to_be_bytes());

            bytes
        } else {
            vec![]
        }
    }

    fn unpack_meta_data(bytes: &[u8; 10]) -> (MicroMinotari, u64, usize, usize, bool, TxType) {
        // Extract fee from the first 4 bytes
        let fee = u64::from(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        // Extract weight from the next 2 bytes
        let weight = u64::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        // Extract inputs_count and sender_one_sided from the next 2 bytes
        let inputs_count_packed = u16::from_be_bytes([bytes[6], bytes[7]]);
        let inputs_count = (inputs_count_packed & 0b0111111111111111) as usize;
        let sender_one_sided = (inputs_count_packed & 0b1000000000000000) != 0;
        // Extract outputs_count and tx_type from the next 2 bytes
        let outputs_count_packed = u16::from_be_bytes([bytes[8], bytes[9]]);
        let outputs_count = (outputs_count_packed & 0b0000111111111111) as usize;
        let tx_type = TxType::from_u16((outputs_count_packed & 0b1111000000000000) >> 12);

        (
            MicroMinotari::from(fee),
            weight,
            inputs_count,
            outputs_count,
            sender_one_sided,
            tx_type,
        )
    }

    pub fn user_data_as_bytes(&self) -> Vec<u8> {
        match &self {
            PaymentId::Empty => vec![],
            PaymentId::U64(v) => v.to_le_bytes().to_vec(),
            PaymentId::U256(v) => {
                let bytes: &mut [u8] = &mut [0; SIZE_U256];
                v.to_little_endian(bytes);
                bytes.to_vec()
            },
            PaymentId::Open { user_data, .. } => user_data.clone(),
            PaymentId::AddressAndData { user_data, .. } => user_data.clone(),
            PaymentId::TransactionInfo { user_data, .. } => user_data.clone(),
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
            PaymentId::Open { user_data, tx_type } => {
                let mut bytes = tx_type.as_bytes();
                bytes.extend_from_slice(user_data);
                bytes
            },
            PaymentId::AddressAndData {
                sender_address,
                user_data,
                tx_type,
            } => {
                let mut bytes = sender_address.to_vec();
                bytes.extend_from_slice(&tx_type.as_bytes());
                bytes.extend_from_slice(user_data);
                bytes
            },
            PaymentId::TransactionInfo {
                recipient_address,
                amount,
                user_data,
                ..
            } => {
                let mut bytes = amount.as_u64().to_le_bytes().to_vec();
                bytes.extend_from_slice(&self.pack_meta_data());
                bytes.extend_from_slice(&recipient_address.to_vec());
                bytes.extend_from_slice(user_data);
                bytes
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match bytes.len() {
            0 => PaymentId::Empty,
            SIZE_VALUE => {
                let bytes: [u8; SIZE_VALUE] = bytes.try_into().expect("Cannot fail, as we already test the length");
                let v = u64::from_le_bytes(bytes);
                PaymentId::U64(v)
            },
            SIZE_U256 => {
                let v = U256::from_little_endian(bytes);
                PaymentId::U256(v)
            },
            len if len <= TARI_ADDRESS_INTERNAL_SINGLE_SIZE => {
                // data
                PaymentId::Open {
                    user_data: if bytes.len() > 1 {
                        bytes[1..].to_vec()
                    } else {
                        Vec::new()
                    },
                    tx_type: TxType::from_u8(bytes[0]),
                }
            },
            _ => {
                // PaymentId::AddressAndData
                if bytes.len() > TARI_ADDRESS_INTERNAL_DUAL_SIZE {
                    // Dual + data
                    if let Ok(sender_address) = TariAddress::from_bytes(&bytes[0..TARI_ADDRESS_INTERNAL_DUAL_SIZE]) {
                        return PaymentId::AddressAndData {
                            sender_address,
                            tx_type: TxType::from_u8(bytes[TARI_ADDRESS_INTERNAL_DUAL_SIZE]),
                            user_data: bytes[TARI_ADDRESS_INTERNAL_DUAL_SIZE + 1..].to_vec(),
                        };
                    }
                }
                if bytes.len() > TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
                    // Single + data
                    if let Ok(sender_address) = TariAddress::from_bytes(&bytes[0..TARI_ADDRESS_INTERNAL_SINGLE_SIZE]) {
                        return PaymentId::AddressAndData {
                            sender_address,
                            tx_type: TxType::from_u8(bytes[TARI_ADDRESS_INTERNAL_SINGLE_SIZE]),
                            user_data: bytes[TARI_ADDRESS_INTERNAL_SINGLE_SIZE + 1..].to_vec(),
                        };
                    }
                }
                // PaymentId::TransactionInfo
                let mut amount_bytes = [0u8; SIZE_VALUE];
                amount_bytes.copy_from_slice(&bytes[0..SIZE_VALUE]);
                let amount = MicroMinotari::from(u64::from_le_bytes(amount_bytes));
                let mut meta_data_bytes = [0u8; PaymentId::SIZE_META_DATA];
                meta_data_bytes.copy_from_slice(&bytes[SIZE_VALUE..PaymentId::SIZE_VALUE_AND_META_DATA]);
                let (fee, weight, inputs_count, outputs_count, sender_one_sided, tx_meta_data) =
                    PaymentId::unpack_meta_data(&meta_data_bytes);
                // Amount + fee + Single/Dual
                if let Ok(recipient_address) = TariAddress::from_bytes(&bytes[PaymentId::SIZE_VALUE_AND_META_DATA..]) {
                    return PaymentId::TransactionInfo {
                        recipient_address,
                        sender_one_sided,
                        amount,
                        fee,
                        weight,
                        inputs_count,
                        outputs_count,
                        tx_type: tx_meta_data,
                        user_data: Vec::new(),
                    };
                }
                if bytes.len() > PaymentId::SIZE_VALUE_AND_META_DATA + TARI_ADDRESS_INTERNAL_DUAL_SIZE {
                    if let Ok(recipient_address) = TariAddress::from_bytes(
                        &bytes[PaymentId::SIZE_VALUE_AND_META_DATA..
                            PaymentId::SIZE_VALUE_AND_META_DATA + TARI_ADDRESS_INTERNAL_DUAL_SIZE],
                    ) {
                        // Amount + Dual + data
                        return PaymentId::TransactionInfo {
                            recipient_address,
                            sender_one_sided,
                            amount,
                            fee,
                            weight,
                            inputs_count,
                            outputs_count,
                            tx_type: tx_meta_data,
                            user_data: bytes[PaymentId::SIZE_VALUE_AND_META_DATA + TARI_ADDRESS_INTERNAL_DUAL_SIZE..]
                                .to_vec(),
                        };
                    }
                }
                if bytes.len() > PaymentId::SIZE_VALUE_AND_META_DATA + TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
                    if let Ok(recipient_address) = TariAddress::from_bytes(
                        &bytes[PaymentId::SIZE_VALUE_AND_META_DATA..
                            PaymentId::SIZE_VALUE_AND_META_DATA + TARI_ADDRESS_INTERNAL_SINGLE_SIZE],
                    ) {
                        // Amount + Single + data
                        return PaymentId::TransactionInfo {
                            recipient_address,
                            sender_one_sided,
                            amount,
                            fee,
                            weight,
                            inputs_count,
                            outputs_count,
                            tx_type: tx_meta_data,
                            user_data: bytes[PaymentId::SIZE_VALUE_AND_META_DATA + TARI_ADDRESS_INTERNAL_SINGLE_SIZE..]
                                .to_vec(),
                        };
                    }
                }
                // Single
                PaymentId::Open {
                    user_data: if bytes.len() > 1 {
                        bytes[1..].to_vec()
                    } else {
                        Vec::new()
                    },
                    tx_type: TxType::from_u8(bytes[0]),
                }
            },
        }
    }

    /// Helper function to convert a byte slice to a string for the open and data variants
    pub fn stringify_bytes(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).to_string()
    }

    /// Helper function to display the payment id's user data
    pub fn user_data_as_string(&self) -> String {
        match self {
            PaymentId::Empty => self.to_string(),
            PaymentId::U64(v) => format!("{}", v),
            PaymentId::U256(v) => format!("{}", v),
            PaymentId::Open { user_data, .. } => PaymentId::stringify_bytes(user_data),
            PaymentId::AddressAndData { user_data, .. } => PaymentId::stringify_bytes(user_data),
            PaymentId::TransactionInfo { user_data, .. } => PaymentId::stringify_bytes(user_data),
        }
    }

    /// Helper function to create a `PaymentId::Open` from a string and the transaction type
    pub fn open(s: &str, tx_type: TxType) -> Self {
        PaymentId::Open {
            user_data: s.as_bytes().to_vec(),
            tx_type,
        }
    }
}

impl Display for PaymentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PaymentId::Empty => write!(f, "None"),
            PaymentId::U64(v) => write!(f, "u64({v})"),
            PaymentId::U256(v) => write!(f, "u256({v})"),
            PaymentId::Open { user_data, tx_type } => {
                write!(f, "type({}), data({})", tx_type, PaymentId::stringify_bytes(user_data))
            },
            PaymentId::AddressAndData {
                sender_address,
                tx_type,
                user_data,
            } => write!(
                f,
                "sender_address({}), type({}), data({})",
                sender_address.to_base58(),
                tx_type,
                PaymentId::stringify_bytes(user_data)
            ),
            PaymentId::TransactionInfo {
                recipient_address,
                sender_one_sided,
                amount,
                fee,
                weight,
                inputs_count,
                outputs_count,
                user_data,
                tx_type: tx_meta_data,
            } => write!(
                f,
                "recipient_address({}), sender_one_sided({}), amount({}), fee({}), weight({}), inputs_count({}), \
                 outputs_count({}), type({}), data({})",
                recipient_address.to_base58(),
                sender_one_sided,
                amount,
                fee,
                weight,
                inputs_count,
                outputs_count,
                tx_meta_data,
                PaymentId::stringify_bytes(user_data),
            ),
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
    use static_assertions::const_assert;
    use tari_common_types::types::CommitmentFactory;
    use tari_crypto::commitment::HomomorphicCommitmentFactory;

    use super::*;

    #[test]
    fn address_sizes_increase_as_expected() {
        const_assert!(SIZE_VALUE < SIZE_U256);
        const_assert!(SIZE_U256 < TARI_ADDRESS_INTERNAL_SINGLE_SIZE);
        const_assert!(TARI_ADDRESS_INTERNAL_SINGLE_SIZE < TARI_ADDRESS_INTERNAL_DUAL_SIZE);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn it_encrypts_and_decrypts_correctly() {
        for payment_id in [
            PaymentId::Empty,
            PaymentId::U64(1),
            PaymentId::U64(156486946518564),
            PaymentId::U256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            PaymentId::Open {
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                tx_type: TxType::default(),
            },
            PaymentId::Open {
                user_data: vec![1; 255],
                tx_type: TxType::default(),
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                tx_type: TxType::PaymentToOther,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                tx_type: TxType::PaymentToSelf,
                user_data: vec![1; 188],
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type: TxType::CoinSplit,
                user_data: vec![1; 188],
            },
            // Single + amount
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::CoinJoin,
                user_data: vec![],
            },
            // Single + amount + data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::ValidatorNodeRegistration,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            // Dual + amount
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: true,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::CoinSplit,
                user_data: vec![],
            },
            // Dual + amount + data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
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
    #[allow(clippy::too_many_lines)]
    fn it_converts_correctly() {
        for payment_id in [
            PaymentId::Empty,
            PaymentId::U64(1),
            PaymentId::U64(156486946518564),
            PaymentId::U256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            PaymentId::Open {
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                tx_type: TxType::default(),
            },
            PaymentId::Open {
                user_data: vec![1; 255],
                tx_type: TxType::default(),
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                tx_type: TxType::PaymentToOther,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                tx_type: TxType::PaymentToSelf,
                user_data: vec![1; 188],
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type: TxType::CoinJoin,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type: TxType::ValidatorNodeRegistration,
                user_data: vec![1; 188],
            },
            // Single + amount
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::ClaimAtomicSwap,
                user_data: vec![],
            },
            // Single + amount + data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: true,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::PaymentToOther,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            // Dual + amount
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::PaymentToSelf,
                user_data: vec![],
            },
            // Dual + amount + data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::CoinSplit,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
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

    #[test]
    fn tx_type_serialize_deserialize_correctly() {
        for tx_type in [
            TxType::PaymentToOther,
            TxType::PaymentToSelf,
            TxType::Burn,
            TxType::CoinSplit,
            TxType::CoinJoin,
            TxType::ValidatorNodeRegistration,
            TxType::ClaimAtomicSwap,
            TxType::HtlcAtomicSwapRefund,
            TxType::CodeTemplateRegistration,
            TxType::ImportedUtxoNoneRewindable,
        ] {
            let payment_id = PaymentId::Open {
                tx_type: tx_type.clone(),
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            };
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type: tx_type.clone(),
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            };
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 19000,
                inputs_count: 712,
                outputs_count: 3,
                tx_type,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            };
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);
        }
    }

    #[test]
    fn payment_id_display() {
        assert_eq!(PaymentId::Empty.to_string(), "None");
        assert_eq!(PaymentId::U64(1235678).to_string(), "u64(1235678)");
        assert_eq!(
            PaymentId::U256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail")
            )
            .to_string(),
            "u256(465465489789785458694894263185648978947864164681631)"
        );
        assert_eq!(
            PaymentId::Open {
                user_data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64],
                tx_type: TxType::CoinSplit
            }
            .to_string(),
            "type(CoinSplit), data(Hello World)"
        );
        assert_eq!(
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type: TxType::HtlcAtomicSwapRefund,
                user_data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64]
            }
            .to_string(),
            "sender_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), type(HtlcAtomicSwapRefund), data(Hello \
             World)"
        );
        assert_eq!(
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                weight: 5127,
                inputs_count: 712,
                outputs_count: 3,
                tx_type: TxType::Burn,
                user_data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64]
            }
            .to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), \
             amount(123456 µT), fee(123 µT), weight(5127), inputs_count(712), outputs_count(3), type(Burn), \
             data(Hello World)"
        );
        assert_eq!(
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: true,
                amount: MicroMinotari::from(1234),
                fee: MicroMinotari::from(123),
                weight: 19227,
                inputs_count: 3124,
                outputs_count: 2533,
                tx_type: TxType::ValidatorNodeRegistration,
                user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
            }
            .to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(true), amount(1234 \
             µT), fee(123 µT), weight(19227), inputs_count(3124), outputs_count(2533), \
             type(ValidatorNodeRegistration), data(Hello World!!! 11-22-33)"
        );
    }

    #[test]
    fn test_payment_id_max_meta_data_values() {
        // Maximum values for the metadata fields
        let payment_id_1 = PaymentId::TransactionInfo {
            recipient_address: TariAddress::from_base58(
                "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
            )
            .unwrap(),
            sender_one_sided: true,
            amount: MicroMinotari::from(u64::MAX),
            fee: MicroMinotari::from(4_294_967_295),
            weight: 65_535,
            inputs_count: 32_767,
            outputs_count: 4_095,
            tx_type: TxType::PaymentToOther,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
        };
        let payment_id_2 = PaymentId::TransactionInfo {
            recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            sender_one_sided: false,
            amount: MicroMinotari::from(u64::MAX),
            fee: MicroMinotari::from(4_294_967_295),
            weight: 65_535,
            inputs_count: 32_767,
            outputs_count: 4_095,
            tx_type: TxType::PaymentToSelf,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
        };

        assert_eq!(
            payment_id_1.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(4294.967295 T), weight(65535), inputs_count(32767), \
            outputs_count(4095), type(PaymentToOther), data(Hello World!!! 11-22-33)"
        );
        assert_eq!(
            payment_id_2.to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), \
             amount(18446744073709.551615 T), fee(4294.967295 T), weight(65535), inputs_count(32767), \
             outputs_count(4095), type(PaymentToSelf), data(Hello World!!! 11-22-33)"
        );

        let payment_id_1_bytes = payment_id_1.to_bytes();
        let payment_id_2_bytes = payment_id_2.to_bytes();

        assert_eq!(payment_id_1, PaymentId::from_bytes(&payment_id_1_bytes));
        assert_eq!(payment_id_2, PaymentId::from_bytes(&payment_id_2_bytes));

        // Increase metadata fields to test 'to_bytes' overflow
        let payment_id_3 = PaymentId::TransactionInfo {
            recipient_address: TariAddress::from_base58(
                "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
            )
            .unwrap(),
            sender_one_sided: true,
            amount: MicroMinotari::from(u64::MAX),
            fee: MicroMinotari::from(4_294_967_295 + 100), // 4294.967395 T
            weight: 65_535 + 100,                          // = 65635
            inputs_count: 32_767 + 100,                    // = 32768
            outputs_count: 4_095 + 100,                    // = 4195
            tx_type: TxType::Burn,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
        };
        // - It can be displayed as is ...
        assert_eq!(
            payment_id_3.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(4294.967395 T), weight(65635), inputs_count(32867), \
            outputs_count(4195), type(Burn), data(Hello World!!! 11-22-33)"
        );
        // ... but it cannot be serialized and deserialized as is - overflowed metadata will be zeroed.
        let payment_id_3_bytes = payment_id_3.to_bytes();
        let payment_id_3_from_bytes = PaymentId::from_bytes(&payment_id_3_bytes);
        assert_eq!(
            payment_id_3_from_bytes.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(0 µT), weight(0), inputs_count(0), \
            outputs_count(0), type(Burn), data(Hello World!!! 11-22-33)"
        );
    }

    #[test]
    fn it_gets_useable_payment_id_data() {
        let payment_id = PaymentId::Empty;
        assert_eq!("", PaymentId::stringify_bytes(&payment_id.user_data_as_bytes()));

        let payment_id = PaymentId::U64(12345);
        assert_eq!(
            "12345",
            u64::from_le_bytes(payment_id.user_data_as_bytes().try_into().unwrap()).to_string()
        );

        let payment_id = PaymentId::U256(U256::from_dec_str("123456789").unwrap());
        assert_eq!(
            "123456789",
            U256::from_little_endian(&payment_id.user_data_as_bytes()).to_string()
        );

        let payment_id = PaymentId::AddressAndData {
            sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            tx_type: TxType::CoinSplit,
            user_data: "Hello World!!!".as_bytes().to_vec(),
        };
        assert_eq!(
            "Hello World!!!",
            PaymentId::stringify_bytes(&payment_id.user_data_as_bytes())
        );

        let payment_id = PaymentId::TransactionInfo {
            recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            sender_one_sided: true,
            amount: MicroMinotari::from(1234),
            fee: MicroMinotari::from(123),
            weight: 19227,
            inputs_count: 3124,
            outputs_count: 2533,
            tx_type: TxType::PaymentToOther,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
        };
        assert_eq!(
            "Hello World!!! 11-22-33",
            PaymentId::stringify_bytes(&payment_id.user_data_as_bytes())
        );
    }
}
