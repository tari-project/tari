// Copyright 2025 The Tari Project
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

use std::{
    fmt::{self, Display, Formatter},
    ops::Deref,
};

use log::debug;
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    tari_address::{TariAddress, TARI_ADDRESS_INTERNAL_DUAL_SIZE, TARI_ADDRESS_INTERNAL_SINGLE_SIZE},
    types::FixedHash,
};
use tari_utilities::hex::Hex;

use crate::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::encrypted_data::{SIZE_U256, SIZE_VALUE},
};

// Maximum size for a PaymentID in bytes (256 bytes)
const MAX_PAYMENT_ID_SIZE: usize = 256;

// We pad the bytes to min this size, so that we can use the same size for AddressAndData and TransactionInfo
const PADDING_SIZE: usize = 130;
const PADDING_SIZE_NO_TAG: usize = 129;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
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
    Coinbase = 0b1011,
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
            0b1011 => TxType::Coinbase,
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
            TxType::Coinbase => 0b1011,
        }
    }

    fn as_bytes(self) -> Vec<u8> {
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
            TxType::Coinbase => write!(f, "Coinbase"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct PaymentId {
    inner: InnerPaymentId,
}

impl Deref for PaymentId {
    type Target = InnerPaymentId;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub enum InnerPaymentId {
    /// No payment ID.
    #[default]
    Empty,
    /// A u256 number.
    U256(U256),
    /// Open - the user optionally specifies 'user_data' ('tx_type' is added by the system).
    Open { user_data: Vec<u8>, tx_type: TxType },
    /// This payment ID is automatically generated by the system for output UTXOs. The optional user specified
    /// `PaymentId::Open` payment ID will be assigned to `tx_type` and `user_data`; the system adds in the sender
    /// address.
    AddressAndData {
        sender_address: TariAddress,
        sender_one_sided: bool,
        fee: MicroMinotari,
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
        tx_type: TxType,
        sent_output_hashes: Vec<FixedHash>,
        user_data: Vec<u8>,
    },
    /// This is a fallback if nothing else fits, so we want to preserve the raw bytes.
    Raw(Vec<u8>),
}
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PTag {
    Empty = 0,
    U256 = 1,
    Open = 2,
    AddressAndDataV1 = 3,
    TransactionInfoV1 = 4,
    AddressAndData = 5,
    TransactionInfo = 6,
    Raw = 7,
}

impl PTag {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => PTag::Empty,
            1 => PTag::U256,
            2 => PTag::Open,
            3 => PTag::AddressAndDataV1,
            4 => PTag::TransactionInfoV1,
            5 => PTag::AddressAndData,
            6 => PTag::TransactionInfo,
            _ => PTag::Raw,
        }
    }
}

impl PaymentId {
    const SIZE_META_DATA: usize = 5;
    const SIZE_VALUE_AND_META_DATA: usize = SIZE_VALUE + PaymentId::SIZE_META_DATA;

    /// Calculates the actual size that would be used by an AddressAndData PaymentId
    /// This includes the recursive size of any PaymentIds contained within the address
    fn calculate_address_and_data_size(address: &TariAddress, user_data_len: usize) -> usize {
        let base_size = 1 + 1 + address.get_size() + PaymentId::SIZE_META_DATA + 1 + user_data_len;
        std::cmp::max(base_size, PADDING_SIZE)
    }

    /// Calculates the actual size that would be used by a TransactionInfo PaymentId
    /// This includes the recursive size of any PaymentIds contained within the address
    fn calculate_transaction_info_size(
        address: &TariAddress,
        sent_output_hashes_len: usize,
        user_data_len: usize,
    ) -> usize {
        let base_size = 1 +
            1 +
            address.get_size() +
            PaymentId::SIZE_VALUE_AND_META_DATA +
            1 +
            (sent_output_hashes_len * FixedHash::byte_size()) +
            1 +
            user_data_len;
        std::cmp::max(base_size, PADDING_SIZE)
    }

    pub fn new_address_and_data(
        sender_address: TariAddress,
        fee: MicroMinotari,
        sender_one_sided: bool,
        tx_type: TxType,
        user_data: Vec<u8>,
    ) -> Result<Self, String> {
        // Calculate the actual size this PaymentId would occupy (including any nested PaymentIds in the address)
        let total_size = Self::calculate_address_and_data_size(&sender_address, user_data.len());

        if total_size > MAX_PAYMENT_ID_SIZE {
            return Err(format!(
                "PaymentId exceeds {}-byte limit: {} bytes (address: {} bytes, user_data: {} bytes, overhead: {} \
                 bytes)",
                MAX_PAYMENT_ID_SIZE,
                total_size,
                sender_address.get_size(),
                user_data.len(),
                total_size - sender_address.get_size() - user_data.len()
            ));
        }

        let payment_id = InnerPaymentId::AddressAndData {
            sender_address,
            fee,
            sender_one_sided,
            tx_type,
            user_data,
        };

        Ok(PaymentId { inner: payment_id })
    }

    pub fn new_transaction_info(
        recipient_address: TariAddress,
        amount: MicroMinotari,
        fee: MicroMinotari,
        sender_one_sided: bool,
        tx_type: TxType,
        sent_output_hashes: Vec<FixedHash>,
        user_data: Vec<u8>,
    ) -> Result<Self, String> {
        // Calculate the actual size this PaymentId would occupy (including any nested PaymentIds in the address)
        let total_size =
            Self::calculate_transaction_info_size(&recipient_address, sent_output_hashes.len(), user_data.len());

        if total_size > MAX_PAYMENT_ID_SIZE {
            return Err(format!(
                "PaymentId exceeds {}-byte limit: {} bytes (address: {} bytes, hashes: {} bytes, user_data: {} bytes, \
                 overhead: {} bytes)",
                MAX_PAYMENT_ID_SIZE,
                total_size,
                recipient_address.get_size(),
                sent_output_hashes.len() * FixedHash::byte_size(),
                user_data.len(),
                total_size -
                    recipient_address.get_size() -
                    (sent_output_hashes.len() * FixedHash::byte_size()) -
                    user_data.len()
            ));
        }

        let payment_id = InnerPaymentId::TransactionInfo {
            recipient_address,
            amount,
            fee,
            sender_one_sided,
            tx_type,
            sent_output_hashes,
            user_data,
        };

        Ok(PaymentId { inner: payment_id })
    }

    pub fn new_empty() -> Self {
        PaymentId {
            inner: InnerPaymentId::Empty,
        }
    }

    pub fn new_raw(data: Vec<u8>) -> Result<Self, String> {
        // Raw PaymentId: 1 byte for tag + data.len() bytes for data
        let total_size = 1 + data.len();

        if total_size > MAX_PAYMENT_ID_SIZE {
            return Err(format!(
                "PaymentId exceeds {}-byte limit: {} bytes (data: {} bytes, tag: 1 byte)",
                MAX_PAYMENT_ID_SIZE,
                total_size,
                data.len()
            ));
        }
        Ok(PaymentId {
            inner: InnerPaymentId::Raw(data),
        })
    }

    pub fn new_u256(value: U256) -> PaymentId {
        // U256 PaymentId: 1 byte for tag + 32 bytes for U256 = 33 bytes total
        // This always fits within 256 bytes, no runtime validation needed
        PaymentId {
            inner: InnerPaymentId::U256(value),
        }
    }

    /// Helper function to create a validated `PaymentId::Open` from user data and transaction type
    pub fn new_open(user_data: Vec<u8>, tx_type: TxType) -> Result<Self, String> {
        // Open PaymentId: 1 byte for tag + user_data.len() bytes + 1 byte for tx_type
        let total_size = 1 + user_data.len() + 1;

        if total_size > MAX_PAYMENT_ID_SIZE {
            return Err(format!(
                "PaymentId exceeds {}-byte limit: {} bytes (user_data: {} bytes, tag: 1 byte, tx_type: 1 byte)",
                MAX_PAYMENT_ID_SIZE,
                total_size,
                user_data.len()
            ));
        }

        Ok(PaymentId {
            inner: InnerPaymentId::Open { user_data, tx_type },
        })
    }

    /// Helper function to create a validated `PaymentId::Open` from a string and transaction type
    pub fn new_open_from_string(s: &str, tx_type: TxType) -> Result<Self, String> {
        Self::new_open(s.as_bytes().to_vec(), tx_type)
    }

    fn to_tag(&self) -> Vec<u8> {
        match &self.inner {
            InnerPaymentId::Empty => vec![],
            InnerPaymentId::U256(_) => vec![PTag::U256 as u8],
            InnerPaymentId::Open { .. } => vec![PTag::Open as u8],
            InnerPaymentId::AddressAndData { .. } => vec![PTag::AddressAndData as u8],
            InnerPaymentId::TransactionInfo { .. } => vec![PTag::TransactionInfo as u8],
            InnerPaymentId::Raw(_) => vec![PTag::Raw as u8],
        }
    }

    pub fn get_size(&self) -> usize {
        match &self.inner {
            // Empty payment ID has no bytes
            InnerPaymentId::Empty => 0,

            // U256 payment ID:
            // - 1 byte for the PTag (enum discriminator)
            // - SIZE_U256 bytes for the U256 value (32 bytes = size_of::<U256>())
            InnerPaymentId::U256(_) => 1 + SIZE_U256,

            // Open payment ID:
            // - 1 byte for the PTag (enum discriminator)
            // - user_data.len() bytes for the variable-length user data
            // - 1 byte for the TxType (transaction type as u8)
            InnerPaymentId::Open { user_data, .. } => 1 + user_data.len() + 1,

            InnerPaymentId::AddressAndData {
                sender_address,
                user_data,
                ..
            } => {
                // AddressAndData payment ID:
                // - 1 byte for the PTag (enum discriminator)
                // - 1 byte for sender_one_sided boolean flag
                // - sender_address.get_size() bytes for the TariAddress (67 bytes for dual, 35 bytes for single)
                // - PaymentId::SIZE_META_DATA bytes for metadata (5 bytes: 1 byte TxType + 4 bytes fee as u32)
                // - 1 byte for user_data length
                // - user_data.len() bytes for the variable-length user data
                let len = 1 + 1 + sender_address.get_size() + PaymentId::SIZE_META_DATA + 1 + user_data.len();
                // Ensure minimum size of PADDING_SIZE (130 bytes) for consistent serialization
                std::cmp::max(len, PADDING_SIZE)
            },

            InnerPaymentId::TransactionInfo {
                recipient_address,
                user_data,
                sent_output_hashes,
                ..
            } => {
                // TransactionInfo payment ID:
                // - 1 byte for the PTag (enum discriminator)
                // - 1 byte for sender_one_sided boolean flag
                // - recipient_address.get_size() bytes for the TariAddress (67 bytes for dual, 35 bytes for single)
                // - PaymentId::SIZE_VALUE_AND_META_DATA bytes for value and metadata (13 bytes: 8 bytes amount + 5
                //   bytes metadata)
                // - 1 byte for sent_output_hashes length
                // - (sent_output_hashes.len() * FixedHash::byte_size()) bytes for output hashes (32 bytes per hash)
                // - 1 byte for user_data length
                // - user_data.len() bytes for the variable-length user data
                let len = 1 +
                    1 +
                    recipient_address.get_size() +
                    PaymentId::SIZE_VALUE_AND_META_DATA +
                    1 +
                    (sent_output_hashes.len() * FixedHash::byte_size()) +
                    1 +
                    user_data.len();
                // Ensure minimum size of PADDING_SIZE (130 bytes) for consistent serialization
                if len < PADDING_SIZE {
                    PADDING_SIZE
                } else {
                    len
                }
            },

            InnerPaymentId::Raw(bytes) => {
                // Raw payment ID:
                // - 1 byte for the PTag (enum discriminator)
                // - bytes.len() bytes for the raw data
                1 + bytes.len()
            },
        }
    }

    pub fn get_fee(&self) -> Option<MicroMinotari> {
        match &self.inner {
            InnerPaymentId::AddressAndData { fee, .. } | InnerPaymentId::TransactionInfo { fee, .. } => Some(*fee),
            _ => None,
        }
    }

    pub fn get_sent_hashes(&self) -> Option<Vec<FixedHash>> {
        match &self.inner {
            InnerPaymentId::TransactionInfo { sent_output_hashes, .. } => Some(sent_output_hashes.clone()),
            _ => None,
        }
    }

    /// Helper function to set the 'amount' of a 'PaymentId::TransactionInfo'
    pub fn transaction_info_set_amount(&mut self, amount: MicroMinotari) {
        if let InnerPaymentId::TransactionInfo { amount: ref mut a, .. } = self.inner {
            *a = amount;
        }
    }

    pub fn get_type(&self) -> TxType {
        match &self.inner {
            InnerPaymentId::Open { tx_type, .. } |
            InnerPaymentId::AddressAndData { tx_type, .. } |
            InnerPaymentId::TransactionInfo { tx_type, .. } => *tx_type,
            _ => TxType::default(),
        }
    }

    /// Helper function to set the 'recipient_address' of a 'PaymentId::TransactionInfo'
    pub fn transaction_info_set_address(&mut self, address: TariAddress) {
        if let InnerPaymentId::TransactionInfo {
            ref mut recipient_address,
            ..
        } = self.inner
        {
            *recipient_address = address
        }
    }

    pub fn transaction_info_set_sent_output_hashes(&mut self, sent_output_hashes: Vec<FixedHash>) {
        if let InnerPaymentId::TransactionInfo {
            sent_output_hashes: ref mut hashes,
            ..
        } = self.inner
        {
            *hashes = sent_output_hashes;
        }
    }

    /// Helper function to convert a 'PaymentId::Open' or 'PaymentId::Empty' to a 'PaymentId::AddressAndData', with the
    /// optional 'tx_type' only applicable to 'PaymentId::Open', otherwise 'payment_id' is kept as is.
    pub fn add_sender_address(
        self,
        sender_address: TariAddress,
        sender_one_sided: bool,
        fee: MicroMinotari,
        tx_type: Option<TxType>,
    ) -> PaymentId {
        match self.inner {
            InnerPaymentId::Open { user_data, tx_type } => {
                match PaymentId::new_address_and_data(sender_address, fee, sender_one_sided, tx_type, user_data) {
                    Ok(payment_id) => payment_id,
                    Err(e) => panic!("Cannot create AddressAndData PaymentId: {}", e),
                }
            },
            InnerPaymentId::Empty => {
                match PaymentId::new_address_and_data(
                    sender_address,
                    fee,
                    sender_one_sided,
                    tx_type.unwrap_or_default(),
                    vec![],
                ) {
                    Ok(payment_id) => payment_id,
                    Err(e) => panic!("Cannot create AddressAndData PaymentId: {}", e),
                }
            },
            _ => self,
        }
    }

    // This method is infallible; any out-of-bound values will be zeroed.
    fn pack_meta_data(&self) -> Vec<u8> {
        match &self.inner {
            InnerPaymentId::TransactionInfo {
                fee,
                sender_one_sided,
                tx_type,
                ..
            } |
            InnerPaymentId::AddressAndData {
                fee,
                sender_one_sided,
                tx_type,
                ..
            } => {
                let mut bytes = Vec::with_capacity(5);
                // Zero out-of-bound values
                // - Use 4 bytes for 'fee', max value: 4,294,967,295
                let fee = if fee.as_u64() > 2u64.pow(32) - 1 {
                    0
                } else {
                    fee.as_u64()
                };
                // Pack
                bytes.extend_from_slice(&fee.to_be_bytes()[4..]);
                let tx_type = tx_type.as_u8() & 0b00001111 | (u8::from(*sender_one_sided) << 7);

                bytes.push(tx_type);
                bytes
            },
            _ => vec![],
        }
    }

    fn unpack_meta_data(bytes: [u8; 5]) -> (MicroMinotari, bool, TxType) {
        // Extract fee from the first 4 bytes
        let fee = u64::from(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        let tx_type_packed = bytes[4];
        let tx_type = TxType::from_u8(tx_type_packed & 0b00001111);
        let sender_one_sided = (tx_type_packed & 0b10000000) != 0;
        (MicroMinotari::from(fee), sender_one_sided, tx_type)
    }

    pub fn user_data_as_bytes(&self) -> Vec<u8> {
        match &self.inner {
            InnerPaymentId::Empty => vec![],
            InnerPaymentId::U256(v) => {
                let bytes: &mut [u8] = &mut [0; SIZE_U256];
                v.to_little_endian(bytes);
                bytes.to_vec()
            },
            InnerPaymentId::Open { user_data, .. } => user_data.clone(),
            InnerPaymentId::AddressAndData { user_data, .. } => user_data.clone(),
            InnerPaymentId::TransactionInfo { user_data, .. } => user_data.clone(),
            InnerPaymentId::Raw(bytes) => bytes.clone(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match &self.inner {
            InnerPaymentId::Empty => Vec::new(),
            InnerPaymentId::U256(v) => {
                let mut bytes = self.to_tag();
                let mut value = vec![0; 32];
                v.to_little_endian(&mut value);
                bytes.extend_from_slice(&value);
                bytes
            },
            InnerPaymentId::Open { user_data, tx_type } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&tx_type.as_bytes());
                bytes.extend_from_slice(user_data);
                bytes
            },
            InnerPaymentId::AddressAndData {
                sender_address,
                user_data,
                ..
            } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&self.pack_meta_data());
                let address_bytes = sender_address.to_vec();
                bytes.push(u8::try_from(address_bytes.len()).expect("User data length should fit in a u8"));
                bytes.extend_from_slice(&address_bytes);
                bytes.push(u8::try_from(user_data.len()).expect("User data length should fit in a u8"));
                bytes.extend_from_slice(user_data);
                // Ensure we have enough padding to match the min size
                while bytes.len() < PADDING_SIZE {
                    bytes.push(0);
                }
                bytes
            },
            InnerPaymentId::TransactionInfo {
                recipient_address,
                amount,
                user_data,
                sent_output_hashes,
                ..
            } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&amount.as_u64().to_le_bytes());
                bytes.extend_from_slice(&self.pack_meta_data());
                let address_bytes = recipient_address.to_vec();
                bytes.push(u8::try_from(address_bytes.len()).expect("User data length should fit in a u8"));
                bytes.extend_from_slice(&address_bytes.to_vec());
                bytes.push(u8::try_from(user_data.len()).expect("User data length should fit in a u8"));
                bytes.extend_from_slice(user_data);
                bytes.push(
                    u8::try_from(sent_output_hashes.len()).expect("Sent output hashes length should fit in a u8"),
                );
                for hash in sent_output_hashes {
                    bytes.extend_from_slice(hash.as_slice());
                }
                // Ensure we have enough padding to match the min size
                while bytes.len() < PADDING_SIZE {
                    bytes.push(0);
                }
                bytes
            },
            InnerPaymentId::Raw(data) => {
                let mut result = self.to_tag();
                result.extend_from_slice(data);
                result
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let raw_bytes = bytes.to_vec();
        // edge case for premine:
        if bytes.len() == SIZE_VALUE {
            let bytes_array: [u8; SIZE_VALUE] = bytes.try_into().expect("We already test the length");
            let v = u64::from_le_bytes(bytes_array);
            if v < 1000 {
                return PaymentId {
                    inner: InnerPaymentId::Open {
                        tx_type: TxType::PaymentToOther,
                        user_data: bytes.to_vec(),
                    },
                };
            }
        }

        let p_tag = if bytes.is_empty() {
            PTag::Empty
        } else {
            PTag::from_u8(bytes[0])
        };
        let bytes = if bytes.len() > 1 { &bytes[1..] } else { &[] };
        match p_tag {
            PTag::Empty => {
                return PaymentId {
                    inner: InnerPaymentId::Empty,
                }
            },
            PTag::U256 => {
                if bytes.len() != SIZE_U256 {
                    let inner_payment_id = InnerPaymentId::Open {
                        tx_type: TxType::from_u8(*bytes.first().unwrap_or(&0)),
                        user_data: bytes.get(1..).unwrap_or_default().to_vec(),
                    };
                    return PaymentId {
                        inner: inner_payment_id,
                    };
                }
                let v = U256::from_little_endian(bytes);
                return PaymentId {
                    inner: InnerPaymentId::U256(v),
                };
            },
            PTag::Open => {
                let inner_payment_id = InnerPaymentId::Open {
                    tx_type: TxType::from_u8(*bytes.first().unwrap_or(&0)),
                    user_data: bytes.get(1..).unwrap_or_default().to_vec(),
                };
                return PaymentId {
                    inner: inner_payment_id,
                };
            },
            PTag::Raw => {
                return PaymentId {
                    inner: InnerPaymentId::Raw(raw_bytes),
                }
            },
            _ => {},
        }

        match PaymentId::try_deserialize_address_or_transaction_data(bytes, p_tag) {
            Ok(payment_id) => payment_id,
            Err(e) => {
                debug!("Failed to parse PaymentId from bytes: {}, returning Raw", e);
                PaymentId {
                    inner: InnerPaymentId::Raw(raw_bytes),
                }
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn try_deserialize_address_or_transaction_data(bytes: &[u8], p_tag: PTag) -> Result<PaymentId, String> {
        if bytes.len() < PaymentId::SIZE_VALUE_AND_META_DATA {
            // if the bytes are too short, we cannot parse it as AddressAndData or TransactionInfo
            return Err("Not enough bytes to parse AddressAndData or TransactionInfo".to_string());
        }

        if p_tag == PTag::TransactionInfoV1 || p_tag == PTag::AddressAndDataV1 {
            let mut amount_bytes = [0u8; SIZE_VALUE];
            amount_bytes.copy_from_slice(&bytes[0..SIZE_VALUE]);
            let amount = MicroMinotari::from(u64::from_le_bytes(amount_bytes));
            let mut meta_data_bytes = [0u8; PaymentId::SIZE_META_DATA];
            meta_data_bytes.copy_from_slice(&bytes[SIZE_VALUE..PaymentId::SIZE_VALUE_AND_META_DATA]);
            let (fee, sender_one_sided, tx_meta_data) = PaymentId::unpack_meta_data(meta_data_bytes);
            let (address, size) =
                if let Ok((address, size)) = Self::find_tari_address(&bytes[PaymentId::SIZE_VALUE_AND_META_DATA..]) {
                    (address, size)
                } else {
                    // if we cannot find a valid TariAddress, we return the raw bytes
                    return Err("No valid TariAddress found in bytes".to_string());
                };

            // legacy support for AddressAndDataV1
            if p_tag == PTag::AddressAndDataV1 {
                let user_data = bytes[PaymentId::SIZE_VALUE_AND_META_DATA + size..].to_vec();
                return Ok(PaymentId {
                    inner: InnerPaymentId::AddressAndData {
                        sender_address: address,
                        sender_one_sided,
                        fee,
                        tx_type: tx_meta_data,
                        user_data,
                    },
                });
            }

            // legacy support for TransactionInfoV1
            if p_tag == PTag::TransactionInfoV1 {
                let user_data = bytes[PaymentId::SIZE_VALUE_AND_META_DATA + size..].to_vec();
                return Ok(PaymentId {
                    inner: InnerPaymentId::TransactionInfo {
                        recipient_address: address,
                        sender_one_sided,
                        amount,
                        fee,
                        tx_type: tx_meta_data,
                        user_data,
                        sent_output_hashes: vec![],
                    },
                });
            }
        }
        // now we assume this has to be off type AddressAndData or TransactionInfo
        let data_start_index = if p_tag == PTag::AddressAndData { 0 } else { SIZE_VALUE };
        let metadata_end_index = if p_tag == PTag::AddressAndData {
            PaymentId::SIZE_META_DATA
        } else {
            PaymentId::SIZE_VALUE_AND_META_DATA
        };

        let mut meta_data_bytes = [0u8; PaymentId::SIZE_META_DATA];
        meta_data_bytes.copy_from_slice(
            bytes
                .get(data_start_index..metadata_end_index)
                .ok_or("Not enough bytes for meta data")?,
        );
        let (fee, sender_one_sided, tx_meta_data) = PaymentId::unpack_meta_data(meta_data_bytes);

        let address_size = *bytes
            .get(metadata_end_index)
            .ok_or("Address bytes does not have size encoded")? as usize;
        let address = TariAddress::from_bytes(
            bytes
                .get(metadata_end_index + 1..metadata_end_index + 1 + address_size)
                .ok_or("Not enough bytes for TariAddress")?,
        )
        .map_err(|_| "Invalid TariAddress in bytes".to_string())?;
        let user_data_length = *bytes
            .get(metadata_end_index + 1 + address_size)
            .ok_or("User data bytes does not have length encoded")? as usize;
        let user_data_start = metadata_end_index + 1 + address_size + 1;
        let user_data = bytes
            .get(user_data_start..user_data_start + user_data_length)
            .ok_or("Not enough bytes for user data")?;

        if p_tag == PTag::AddressAndData {
            if !Self::check_padding(bytes, user_data_start + user_data_length) {
                return Err("Invalid padding for AddressAndData".to_string());
            }
            return Ok(PaymentId {
                inner: InnerPaymentId::AddressAndData {
                    sender_address: address,
                    sender_one_sided,
                    fee,
                    tx_type: tx_meta_data,
                    user_data: user_data.to_vec(),
                },
            });
        }
        // so this must be a TransactionInfo
        let mut amount_bytes = [0u8; SIZE_VALUE];
        amount_bytes.copy_from_slice(bytes.get(0..SIZE_VALUE).ok_or("Not enough bytes for amount")?);
        let amount = MicroMinotari::from(u64::from_le_bytes(amount_bytes));
        let mut sent_output_hashes = Vec::new();
        let sent_output_hashes_length = *bytes
            .get(user_data_start + user_data_length)
            .ok_or("Sent output hashes bytes does not have length encoded")?
            as usize;
        let sent_output_hashes_start = user_data_start + user_data_length + 1;
        for hash_num in 0..sent_output_hashes_length {
            let hash_start = sent_output_hashes_start + (hash_num * FixedHash::byte_size());
            let hash_end = hash_start + FixedHash::byte_size();
            let hash = bytes
                .get(hash_start..hash_end)
                .ok_or("Not enough bytes for sent output hash")?;
            let sent_output_hash = FixedHash::try_from(hash).map_err(|_| "Invalid sent output hash".to_string())?;
            sent_output_hashes.push(sent_output_hash);
        }
        if !Self::check_padding(
            bytes,
            sent_output_hashes_start + (sent_output_hashes_length * FixedHash::byte_size()),
        ) {
            return Err("Invalid padding for TransactionInfo".to_string());
        }
        Ok(PaymentId {
            inner: InnerPaymentId::TransactionInfo {
                recipient_address: address,
                sender_one_sided,
                amount,
                fee,
                tx_type: tx_meta_data,
                user_data: user_data.to_vec(),
                sent_output_hashes,
            },
        })
    }

    /// helper function to check padding
    fn check_padding(bytes: &[u8], start_index: usize) -> bool {
        if bytes.len() > PADDING_SIZE_NO_TAG {
            // larger than the minimum size, so no padding here
            return true;
        }

        // Check if the last bytes are zeroed out
        for &byte in &bytes[start_index..] {
            if byte != 0 {
                return false;
            }
        }
        true
    }

    // we dont know where the tari address ends and the user data starts, so we need to find it using the checksum
    fn find_tari_address(bytes: &[u8]) -> Result<(TariAddress, usize), String> {
        if bytes.len() < TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
            return Err("Not enough bytes for single TariAddress".to_string());
        }
        // Now we have to try and brute force a match here
        let mut offset = 0;
        while (TARI_ADDRESS_INTERNAL_DUAL_SIZE + offset) <= bytes.len() {
            if let Ok(address) = TariAddress::from_bytes(&bytes[..(TARI_ADDRESS_INTERNAL_DUAL_SIZE + offset)]) {
                return Ok((address, TARI_ADDRESS_INTERNAL_DUAL_SIZE + offset));
            }
            offset += 1;
        }
        if let Ok(address) = TariAddress::from_bytes(&bytes[..TARI_ADDRESS_INTERNAL_SINGLE_SIZE]) {
            return Ok((address, TARI_ADDRESS_INTERNAL_SINGLE_SIZE));
        }
        Err("No valid TariAddress found".to_string())
    }

    /// Helper function to convert a byte slice to a string for the open and data variants
    pub fn stringify_bytes(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).to_string()
    }

    /// Helper function to display the payment id's user data
    pub fn user_data_as_string(&self) -> String {
        match &self.inner {
            InnerPaymentId::Empty => self.to_string(),
            InnerPaymentId::U256(v) => format!("{}", v),
            InnerPaymentId::Open { user_data, .. } => PaymentId::stringify_bytes(user_data),
            InnerPaymentId::AddressAndData { user_data, .. } => PaymentId::stringify_bytes(user_data),
            InnerPaymentId::TransactionInfo { user_data, .. } => PaymentId::stringify_bytes(user_data),
            InnerPaymentId::Raw(bytes) => bytes.to_hex(),
        }
    }

    /// Helper function to create a `PaymentId::Open` from a string and the transaction type
    ///
    /// # Deprecated
    /// Use `new_open_from_string` instead for proper validation
    pub fn open_from_string(s: &str, tx_type: TxType) -> Self {
        PaymentId {
            inner: InnerPaymentId::Open {
                user_data: s.as_bytes().to_vec(),
                tx_type,
            },
        }
    }

    /// Helper function to create a `PaymentId::Open` from a bytes and the transaction type
    ///
    /// # Deprecated
    /// Use `new_open` instead for proper validation
    pub fn open(bytes: Vec<u8>, tx_type: TxType) -> Self {
        PaymentId {
            inner: InnerPaymentId::Open {
                user_data: bytes,
                tx_type,
            },
        }
    }

    /// Convenience method for pattern matching - checks if this is an Empty payment ID
    pub fn is_empty(&self) -> bool {
        matches!(self.inner, InnerPaymentId::Empty)
    }

    /// Convenience method for pattern matching - checks if this is a U256 payment ID
    pub fn is_u256(&self) -> bool {
        matches!(self.inner, InnerPaymentId::U256(_))
    }

    /// Convenience method for pattern matching - checks if this is an Open payment ID
    pub fn is_open(&self) -> bool {
        matches!(self.inner, InnerPaymentId::Open { .. })
    }

    /// Convenience method for pattern matching - checks if this is an AddressAndData payment ID
    pub fn is_address_and_data(&self) -> bool {
        matches!(self.inner, InnerPaymentId::AddressAndData { .. })
    }

    /// Convenience method for pattern matching - checks if this is a TransactionInfo payment ID
    pub fn is_transaction_info(&self) -> bool {
        matches!(self.inner, InnerPaymentId::TransactionInfo { .. })
    }

    /// Convenience method for pattern matching - checks if this is a Raw payment ID
    pub fn is_raw(&self) -> bool {
        matches!(self.inner, InnerPaymentId::Raw(_))
    }

    /// Get user data from Open, AddressAndData, or TransactionInfo variants
    /// Returns empty Vec for other variants
    pub fn get_user_data(&self) -> Vec<u8> {
        match &self.inner {
            InnerPaymentId::Open { user_data, .. } |
            InnerPaymentId::AddressAndData { user_data, .. } |
            InnerPaymentId::TransactionInfo { user_data, .. } => user_data.clone(),
            _ => Vec::new(),
        }
    }

    /// Get transaction type from variants that have it
    /// Returns None for variants without tx_type
    pub fn get_tx_type(&self) -> Option<TxType> {
        match &self.inner {
            InnerPaymentId::Open { tx_type, .. } |
            InnerPaymentId::AddressAndData { tx_type, .. } |
            InnerPaymentId::TransactionInfo { tx_type, .. } => Some(*tx_type),
            _ => None,
        }
    }

    /// Get the sender address from AddressAndData variant
    /// Returns None for other variants
    pub fn get_sender_address(&self) -> Option<&TariAddress> {
        match &self.inner {
            InnerPaymentId::AddressAndData { sender_address, .. } => Some(sender_address),
            _ => None,
        }
    }

    /// Get the recipient address from TransactionInfo variant
    /// Returns None for other variants
    pub fn get_recipient_address(&self) -> Option<&TariAddress> {
        match &self.inner {
            InnerPaymentId::TransactionInfo { recipient_address, .. } => Some(recipient_address),
            _ => None,
        }
    }

    /// Get the amount from TransactionInfo variant
    /// Returns None for other variants
    pub fn get_amount(&self) -> Option<MicroMinotari> {
        match &self.inner {
            InnerPaymentId::TransactionInfo { amount, .. } => Some(*amount),
            _ => None,
        }
    }

    /// Get the sender_one_sided flag from AddressAndData or TransactionInfo variants
    /// Returns None for other variants
    pub fn get_sender_one_sided(&self) -> Option<bool> {
        match &self.inner {
            InnerPaymentId::AddressAndData { sender_one_sided, .. } |
            InnerPaymentId::TransactionInfo { sender_one_sided, .. } => Some(*sender_one_sided),
            _ => None,
        }
    }

    /// Get the U256 value from U256 variant
    /// Returns None for other variants
    pub fn get_u256(&self) -> Option<U256> {
        match &self.inner {
            InnerPaymentId::U256(value) => Some(*value),
            _ => None,
        }
    }

    /// Get raw bytes from Raw variant
    /// Returns None for other variants
    pub fn get_raw_bytes(&self) -> Option<&[u8]> {
        match &self.inner {
            InnerPaymentId::Raw(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Unchecked constructor for Empty payment ID
    /// Use this for migration from old direct enum construction
    pub fn empty() -> Self {
        PaymentId {
            inner: InnerPaymentId::Empty,
        }
    }

    /// Unchecked constructor for Open payment ID
    /// Use this for migration from old direct enum construction
    /// WARNING: This bypasses validation - use new_open() for new code
    pub fn open_unchecked(user_data: Vec<u8>, tx_type: TxType) -> Self {
        PaymentId {
            inner: InnerPaymentId::Open { user_data, tx_type },
        }
    }

    /// Unchecked constructor for Raw payment ID
    /// Use this for migration from old direct enum construction
    /// WARNING: This bypasses validation - use new_raw() for new code
    pub fn raw_unchecked(data: Vec<u8>) -> Self {
        PaymentId {
            inner: InnerPaymentId::Raw(data),
        }
    }

    /// Unchecked constructor for AddressAndData payment ID
    /// Use this for migration from old direct enum construction
    /// WARNING: This bypasses validation - use new_address_and_data() for new code
    pub fn address_and_data_unchecked(
        sender_address: TariAddress,
        sender_one_sided: bool,
        fee: MicroMinotari,
        tx_type: TxType,
        user_data: Vec<u8>,
    ) -> Self {
        PaymentId {
            inner: InnerPaymentId::AddressAndData {
                sender_address,
                sender_one_sided,
                fee,
                tx_type,
                user_data,
            },
        }
    }

    /// Unchecked constructor for TransactionInfo payment ID
    /// Use this for migration from old direct enum construction
    /// WARNING: This bypasses validation - use new_transaction_info() for new code
    pub fn transaction_info_unchecked(
        recipient_address: TariAddress,
        sender_one_sided: bool,
        amount: MicroMinotari,
        fee: MicroMinotari,
        tx_type: TxType,
        sent_output_hashes: Vec<FixedHash>,
        user_data: Vec<u8>,
    ) -> Self {
        PaymentId {
            inner: InnerPaymentId::TransactionInfo {
                recipient_address,
                sender_one_sided,
                amount,
                fee,
                tx_type,
                sent_output_hashes,
                user_data,
            },
        }
    }
}

impl Display for PaymentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.inner {
            InnerPaymentId::Empty => write!(f, "None"),
            InnerPaymentId::U256(v) => write!(f, "u256({v})"),
            InnerPaymentId::Open { user_data, tx_type } => {
                write!(f, "type({}), data({})", tx_type, PaymentId::stringify_bytes(user_data))
            },
            InnerPaymentId::AddressAndData {
                sender_address,
                sender_one_sided,
                fee,
                tx_type,
                user_data,
            } => write!(
                f,
                "sender_address({}), sender_one_sided({}), fee({}), type({}), data({})",
                sender_address.to_base58(),
                sender_one_sided,
                fee,
                tx_type,
                PaymentId::stringify_bytes(user_data)
            ),
            InnerPaymentId::TransactionInfo {
                recipient_address,
                sender_one_sided,
                amount,
                fee,
                user_data,
                tx_type: tx_meta_data,
                sent_output_hashes: _,
            } => write!(
                f,
                "recipient_address({}), sender_one_sided({}), amount({}), fee({}), type({}), data({})",
                recipient_address.to_base58(),
                sender_one_sided,
                amount,
                fee,
                tx_meta_data,
                PaymentId::stringify_bytes(user_data),
            ),
            InnerPaymentId::Raw(bytes) => write!(f, "Raw({})", bytes.to_hex()),
        }
    }
}

#[cfg(test)]
mod test {
    use chacha20poly1305::aead::OsRng;
    use tari_common_types::{
        tari_address::TariAddress,
        types::{CommitmentFactory, CompressedCommitment, FixedHash, PrivateKey},
    };
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};

    use super::*;
    use crate::transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
            payment_id::{PaymentId, TxType},
            EncryptedData,
        },
    };

    fn create_random_fixed_hash() -> FixedHash {
        use rand::RngCore;
        let mut bytes = [0u8; FixedHash::byte_size()];
        rand::thread_rng().fill_bytes(&mut bytes);
        FixedHash::from(bytes)
    }

    #[allow(clippy::too_many_lines)]
    fn create_test_data_array() -> Vec<PaymentId> {
        let mut pay_id_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        pay_id_address = pay_id_address.with_payment_id_user_data(vec![11u8; 50]).unwrap();
        // pay_id_address = pay_id_address
        //     .with_payment_id_user_data(vec![0, 1, 2, 3, 4, 5])
        //     .unwrap();
        let sent_output_hashes = vec![create_random_fixed_hash()];
        vec![
            PaymentId::new_empty(),
            PaymentId::new_u256(1.into()),
            PaymentId::new_u256(156486946518564u64.into()),
            PaymentId::new_u256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            // Open - no data
            PaymentId::new_open(vec![], TxType::PaymentToOther).unwrap(),
            // Open - some data
            PaymentId::new_open(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], TxType::default()).unwrap(),
            // Open - max data
            PaymentId::new_open(vec![1; 254], TxType::default()).unwrap(),
            // AddressAndData - dual, no data
            PaymentId::new_address_and_data(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::PaymentToSelf,
                vec![],
            )
            .unwrap(),
            // // AddressAndData - dual, some data
            PaymentId::new_address_and_data(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::PaymentToOther,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            // AddressAndData - dual,
            PaymentId::new_address_and_data(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::PaymentToSelf,
                vec![1; 80],
            )
            .unwrap(),
            // AddressAndData - single, no data
            PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![],
            )
            .unwrap(),
            // AddressAndData - single, some data
            PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            // AddressAndData - single, max data
            PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![1; 40],
            )
            .unwrap(),
            PaymentId::new_address_and_data(
                pay_id_address.clone(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![1; 30],
            )
            .unwrap(),
            // TransactionInfo - single + amount, no data
            PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::CoinJoin,
                vec![],
                vec![],
            )
            .unwrap(),
            // TransactionInfo - single + amount + some data
            PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::ValidatorNodeRegistration,
                sent_output_hashes.clone(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            // TransactionInfo - dual + amount, no data
            PaymentId::new_transaction_info(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                true,
                TxType::CoinSplit,
                sent_output_hashes.clone(),
                vec![],
            )
            .unwrap(),
            // TransactionInfo - dual + amount + some data
            PaymentId::new_transaction_info(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                sent_output_hashes.clone(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            PaymentId::new_transaction_info(
                pay_id_address,
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                sent_output_hashes.clone(),
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
        ]
    }

    #[test]
    fn test_payment_id_parsing_confusion() {
        // We need to create a PaymentId::Open that, when serialized, will produce bytes that
        // will be parsed as PaymentId::TransactionInfo.
        // Create a valid TariAddress to use for our test
        let fake_recipient = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();
        let fake_recipient_bytes = fake_recipient.to_vec();
        // We'll construct our payload in a way that:
        // 1. It's too large to be parsed as a simple type
        // 2. It will fail TariAddress parsing from the start (for AddressAndData)
        // 3. It has the correct structure for TransactionInfo
        // Use CoinSplit as our tx_type (0x03)
        // This should help prevent the bytes from being parsed as a valid TariAddress
        let tx_type = TxType::CoinSplit;
        // Craft user_data that, when preceded by tx_type, will match the TransactionInfo structure
        let mut user_data = Vec::new();
        // The first byte will be tx_type (0x03)
        // Next 7 bytes plus tx_type will form the amount (8 bytes total)
        let amount_value = 1000u64;
        let amount_bytes = amount_value.to_le_bytes();
        // Skip first byte since tx_type will take that place
        user_data.extend_from_slice(&amount_bytes[1..]);
        // Next 10 bytes for metadata
        let fee = 100u32;
        let weight = 1000u16;
        let inputs_count = 2u16;
        let sender_one_sided = false;
        let outputs_count = 3u16;
        let tx_meta_type = TxType::PaymentToOther;
        // Create metadata bytes
        let mut meta_data = Vec::with_capacity(10);
        meta_data.extend_from_slice(&fee.to_be_bytes());
        meta_data.extend_from_slice(&weight.to_be_bytes());
        let inputs_count_packed = (inputs_count & 0b0111111111111111) | (u16::from(sender_one_sided) << 15);
        meta_data.extend_from_slice(&inputs_count_packed.to_be_bytes());
        let outputs_count_packed = (outputs_count & 0b0000111111111111) | (u16::from(tx_meta_type.as_u8()) << 12);
        meta_data.extend_from_slice(&outputs_count_packed.to_be_bytes());
        user_data.extend_from_slice(&meta_data);
        // Lastly, add the TariAddress
        user_data.extend_from_slice(&fake_recipient_bytes);
        // Create our original PaymentId::Open
        let original_payment_id = PaymentId::new_open(user_data, tx_type).unwrap();
        // Serialize to bytes
        let bytes = original_payment_id.to_bytes();

        // Crucial insight: The key to preventing TariAddress parsing is to ensure
        // the first byte of our payload doesn't match the expected format for a TariAddress.
        // CoinSplit (0x03) should be different enough from a valid TariAddress start byte.
        // Parse back from bytes
        let parsed_payment_id = PaymentId::from_bytes(&bytes);

        // If this assert passes, the attack failed
        assert_eq!(parsed_payment_id, original_payment_id);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn it_encrypts_and_decrypts_correctly() {
        for payment_id in create_test_data_array() {
            for (value, mask) in [
                (0, PrivateKey::default()),
                (0, PrivateKey::random(&mut OsRng)),
                (123456, PrivateKey::default()),
                (654321, PrivateKey::random(&mut OsRng)),
                (u64::MAX, PrivateKey::random(&mut OsRng)),
            ] {
                let commitment = CompressedCommitment::from_commitment(
                    CommitmentFactory::default().commit(&mask, &PrivateKey::from(value)),
                );
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
            }
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn it_converts_correctly() {
        for payment_id in create_test_data_array() {
            for (value, mask) in [
                (0, PrivateKey::default()),
                (0, PrivateKey::random(&mut OsRng)),
                (123456, PrivateKey::default()),
                (654321, PrivateKey::random(&mut OsRng)),
                (u64::MAX, PrivateKey::random(&mut OsRng)),
            ] {
                let commitment = CompressedCommitment::from_commitment(
                    CommitmentFactory::default().commit(&mask, &PrivateKey::from(value)),
                );
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
            TxType::Coinbase,
        ] {
            let payment_id = PaymentId::new_open(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], tx_type).unwrap();
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                tx_type,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap();
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                tx_type,
                vec![create_random_fixed_hash()],
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap();
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);
        }
    }

    #[test]
    fn payment_id_display() {
        assert_eq!(PaymentId::new_empty().to_string(), "None");
        assert_eq!(PaymentId::new_u256(1235678.into()).to_string(), "u256(1235678)");
        assert_eq!(
            PaymentId::new_u256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail")
            )
            .to_string(),
            "u256(465465489789785458694894263185648978947864164681631)"
        );
        assert_eq!(
            PaymentId::new_open(
                vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64],
                TxType::CoinSplit
            )
            .unwrap()
            .to_string(),
            "type(CoinSplit), data(Hello World)"
        );
        assert_eq!(
            PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::HtlcAtomicSwapRefund,
                vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64]
            )
            .unwrap()
            .to_string(),
            "sender_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), fee(123 µT), \
             type(HtlcAtomicSwapRefund), data(Hello World)"
        );
        assert_eq!(
            PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![],
                vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64]
            )
            .unwrap()
            .to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), \
             amount(123456 µT), fee(123 µT), type(Burn), data(Hello World)"
        );
        assert_eq!(
            PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(1234),
                MicroMinotari::from(123),
                true,
                TxType::ValidatorNodeRegistration,
                vec![],
                "Hello World!!! 11-22-33".as_bytes().to_vec()
            )
            .unwrap()
            .to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(true), amount(1234 \
             µT), fee(123 µT), type(ValidatorNodeRegistration), data(Hello World!!! 11-22-33)"
        );
    }

    #[test]
    fn test_payment_id_max_meta_data_values() {
        // Maximum values for the metadata fields
        let payment_id_1 = PaymentId::new_transaction_info(
            TariAddress::from_base58(
                "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
            )
            .unwrap(),
            MicroMinotari::from(u64::MAX),
            MicroMinotari::from(4_294_967_295),
            true,
            TxType::PaymentToOther,
            vec![create_random_fixed_hash()],
            "Hello World!!! 11-22-33".as_bytes().to_vec(),
        )
        .unwrap();
        let payment_id_2 = PaymentId::new_transaction_info(
            TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            MicroMinotari::from(u64::MAX),
            MicroMinotari::from(4_294_967_295),
            false,
            TxType::PaymentToSelf,
            vec![create_random_fixed_hash()],
            "Hello World!!! 11-22-33".as_bytes().to_vec(),
        )
        .unwrap();

        assert_eq!(
            payment_id_1.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(4294.967295 T), type(PaymentToOther), data(Hello World!!! 11-22-33)"
        );
        assert_eq!(
            payment_id_2.to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), \
             amount(18446744073709.551615 T), fee(4294.967295 T), type(PaymentToSelf), data(Hello World!!! 11-22-33)"
        );

        let payment_id_1_bytes = payment_id_1.to_bytes();
        let payment_id_2_bytes = payment_id_2.to_bytes();

        assert_eq!(payment_id_1, PaymentId::from_bytes(&payment_id_1_bytes));
        assert_eq!(payment_id_2, PaymentId::from_bytes(&payment_id_2_bytes));

        // Increase metadata fields to test 'to_bytes' overflow
        let payment_id_3 = PaymentId::new_transaction_info(
            TariAddress::from_base58(
                "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
            )
            .unwrap(),
            MicroMinotari::from(u64::MAX),
            MicroMinotari::from(4_294_967_295 + 100), // 4294.967395 T
            true,
            TxType::Coinbase,
            vec![create_random_fixed_hash()],
            "Hello World!!! 11-22-33".as_bytes().to_vec(),
        )
        .unwrap();
        // - It can be displayed as is ...
        assert_eq!(
            payment_id_3.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(4294.967395 T), type(Coinbase), data(Hello World!!! 11-22-33)"
        );
        // ... but it cannot be serialized and deserialized as is - overflowed metadata will be zeroed.
        let payment_id_3_bytes = payment_id_3.to_bytes();
        let payment_id_3_from_bytes = PaymentId::from_bytes(&payment_id_3_bytes);
        assert_eq!(
            payment_id_3_from_bytes.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(0 µT), type(Coinbase), data(Hello World!!! 11-22-33)"
        );
    }

    #[test]
    fn it_gets_useable_payment_id_data() {
        let payment_id = PaymentId::new_empty();
        assert_eq!("", PaymentId::stringify_bytes(&payment_id.user_data_as_bytes()));

        let payment_id = PaymentId::new_u256(U256::from_dec_str("123456789").unwrap());
        assert_eq!(
            "123456789",
            U256::from_little_endian(&payment_id.user_data_as_bytes()).to_string()
        );

        let payment_id = PaymentId::new_address_and_data(
            TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            MicroMinotari::from(123),
            false,
            TxType::CoinSplit,
            "Hello World!!!".as_bytes().to_vec(),
        )
        .unwrap();
        assert_eq!(
            "Hello World!!!",
            PaymentId::stringify_bytes(&payment_id.user_data_as_bytes())
        );

        let payment_id = PaymentId::new_transaction_info(
            TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            MicroMinotari::from(1234),
            MicroMinotari::from(123),
            true,
            TxType::PaymentToOther,
            vec![create_random_fixed_hash()],
            "Hello World!!! 11-22-33".as_bytes().to_vec(),
        )
        .unwrap();
        assert_eq!(
            "Hello World!!! 11-22-33",
            PaymentId::stringify_bytes(&payment_id.user_data_as_bytes())
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_legacy_data_address_and_data() {
        let mut pay_id_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        pay_id_address = pay_id_address
            .with_payment_id_user_data(vec![0, 1, 2, 3, 4, 5])
            .unwrap();
        let payment_ids = vec![
            // AddressAndData - dual, no data
            PaymentId::new_address_and_data(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::PaymentToSelf,
                vec![],
            )
            .unwrap(),
            // // AddressAndData - dual, some data
            PaymentId::new_address_and_data(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::PaymentToOther,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            // AddressAndData - dual,
            PaymentId::new_address_and_data(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::PaymentToSelf,
                vec![1; 80],
            )
            .unwrap(),
            // AddressAndData - single, no data
            PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![],
            )
            .unwrap(),
            // AddressAndData - single, some data
            PaymentId::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            PaymentId::new_address_and_data(pay_id_address, MicroMinotari::from(123), false, TxType::Burn, vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            ])
            .unwrap(),
        ];
        fn old_to_bytes(payment_id: &PaymentId) -> Vec<u8> {
            fn pack_meta_data(fee: MicroMinotari, tx_type: TxType, sender_one_side: bool) -> Vec<u8> {
                let mut bytes = Vec::with_capacity(5);
                // Zero out-of-bound values
                // - Use 4 bytes for 'fee', max value: 4,294,967,295
                let fee = if fee.as_u64() > 2u64.pow(32) - 1 {
                    0
                } else {
                    fee.as_u64()
                };
                // Pack
                bytes.extend_from_slice(&fee.to_be_bytes()[4..]);
                let tx_type = tx_type.as_u8() & 0b00001111 | (u8::from(sender_one_side) << 7);

                bytes.push(tx_type);
                bytes
            }
            let mut bytes = Vec::new();
            if let InnerPaymentId::AddressAndData {
                sender_address,
                tx_type,
                sender_one_sided,
                fee,
                user_data,
            } = &payment_id.inner
            {
                bytes.push(PTag::AddressAndDataV1 as u8);
                bytes.extend_from_slice(&0u64.to_le_bytes());
                bytes.extend_from_slice(&pack_meta_data(*fee, *tx_type, *sender_one_sided));
                bytes.extend_from_slice(&sender_address.to_vec());
                bytes.extend_from_slice(user_data);
            };
            bytes
        }

        for payment_id in payment_ids {
            let bytes = old_to_bytes(&payment_id);
            let decoded = PaymentId::from_bytes(&bytes);
            assert_eq!(decoded, payment_id);
        }
    }

    #[test]
    fn test_legacy_transaction_info() {
        let mut pay_id_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        pay_id_address = pay_id_address
            .with_payment_id_user_data(vec![0, 1, 2, 3, 4, 5])
            .unwrap();
        let payment_ids = vec![
            // TransactionInfo - single + amount, no data
            PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::CoinJoin,
                vec![],
                vec![],
            )
            .unwrap(),
            // TransactionInfo - single + amount + some data
            PaymentId::new_transaction_info(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::ValidatorNodeRegistration,
                vec![],
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            // TransactionInfo - dual + amount, no dta
            PaymentId::new_transaction_info(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                true,
                TxType::CoinSplit,
                vec![],
                vec![],
            )
            .unwrap(),
            // TransactionInfo - dual + amount + some data
            PaymentId::new_transaction_info(
                TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![],
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            PaymentId::new_transaction_info(
                pay_id_address,
                MicroMinotari::from(123456),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![],
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
        ];
        fn old_to_bytes(payment_id: &PaymentId) -> Vec<u8> {
            let mut bytes = Vec::new();
            if let InnerPaymentId::TransactionInfo {
                recipient_address,
                tx_type,
                sender_one_sided,
                amount,
                fee,
                user_data,
                sent_output_hashes: _,
            } = &payment_id.inner
            {
                fn pack_meta_data(fee: MicroMinotari, tx_type: TxType, sender_one_side: bool) -> Vec<u8> {
                    let mut bytes = Vec::with_capacity(5);
                    // Zero out-of-bound values
                    // - Use 4 bytes for 'fee', max value: 4,294,967,295
                    let fee = if fee.as_u64() > 2u64.pow(32) - 1 {
                        0
                    } else {
                        fee.as_u64()
                    };
                    // Pack
                    bytes.extend_from_slice(&fee.to_be_bytes()[4..]);
                    let tx_type = tx_type.as_u8() & 0b00001111 | (u8::from(sender_one_side) << 7);

                    bytes.push(tx_type);
                    bytes
                }
                bytes.push(PTag::TransactionInfoV1 as u8);
                bytes.extend_from_slice(&amount.as_u64().to_le_bytes());
                bytes.extend_from_slice(&pack_meta_data(*fee, *tx_type, *sender_one_sided));
                bytes.extend_from_slice(&recipient_address.to_vec());
                bytes.extend_from_slice(user_data);
            };
            bytes
        }

        for payment_id in payment_ids {
            let bytes = old_to_bytes(&payment_id);
            let decoded = PaymentId::from_bytes(&bytes);
            assert_eq!(decoded, payment_id);
        }
    }

    // This is a rare edge case where the first byte of the spend key, matches the correct checksum for a single
    // address.
    #[test]
    fn test_edge_case_with_tari_address() {
        let hex = "03404e9c30000000000000000a8000016c1b073261df680b5a95dbc8c559ed1eec8d31f66c90e9e2843d3376cb6142511299678d6494bd091405cd78b1b9cb8d1602b7d075f72dbf54fde4b89fbbe016ab34f142623015444b06f34f3f4f860c94";
        let bytes = hex::decode(hex).expect("Failed to decode hex");
        let payment_id = PaymentId::from_bytes(&bytes);
        let address = match &payment_id.inner {
            InnerPaymentId::AddressAndData { sender_address, .. } => sender_address.clone(),
            _ => panic!("Expected AddressAndData variant"),
        };
        match address {
            TariAddress::Dual(address) => {
                assert_eq!(
                    address.public_spend_key().to_hex(),
                    "1299678d6494bd091405cd78b1b9cb8d1602b7d075f72dbf54fde4b89fbbe016"
                );
                assert_eq!(
                    address.public_view_key().to_hex(),
                    "6c1b073261df680b5a95dbc8c559ed1eec8d31f66c90e9e2843d3376cb614251"
                );
            },
            _ => panic!("Dual variant was expected"),
        }
    }

    #[test]
    fn test_payment_id_size_validation() {
        // Test U256 PaymentId validation
        let u256_value = U256::from(12345u64);
        let u256_payment_id = PaymentId::new_u256(u256_value);
        assert_eq!(u256_payment_id.get_size(), 1 + SIZE_U256); // 1 + 32 = 33 bytes

        // Test Open PaymentId validation - valid case
        let small_user_data = vec![1, 2, 3, 4, 5];
        let open_payment_id = PaymentId::new_open(small_user_data.clone(), TxType::PaymentToOther)
            .expect("Small Open PaymentId should be valid");
        assert_eq!(open_payment_id.get_size(), 1 + small_user_data.len() + 1); // tag + data + tx_type

        // Test Open PaymentId validation - too large
        let large_user_data = vec![0u8; MAX_PAYMENT_ID_SIZE]; // 256 bytes
        let result = PaymentId::new_open(large_user_data, TxType::PaymentToOther);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Test Open PaymentId validation - maximum valid size
        let max_valid_open_data = vec![0u8; MAX_PAYMENT_ID_SIZE - 2]; // 254 bytes (256 - 1 tag - 1 tx_type)
        let max_open_payment_id = PaymentId::new_open(max_valid_open_data.clone(), TxType::PaymentToOther)
            .expect("Maximum valid Open PaymentId should be valid");
        assert_eq!(max_open_payment_id.get_size(), MAX_PAYMENT_ID_SIZE);

        // Test Raw PaymentId validation - valid case
        let raw_data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let raw_payment_id = PaymentId::new_raw(raw_data.clone()).expect("Small Raw PaymentId should be valid");
        assert_eq!(raw_payment_id.get_size(), 1 + raw_data.len()); // tag + data

        // Test Raw PaymentId validation - too large
        let large_raw_data = vec![0u8; MAX_PAYMENT_ID_SIZE]; // 256 bytes
        let result = PaymentId::new_raw(large_raw_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Test Raw PaymentId validation - maximum valid size
        let max_valid_raw_data = vec![0u8; MAX_PAYMENT_ID_SIZE - 1]; // 255 bytes (256 - 1 tag)
        let max_raw_payment_id =
            PaymentId::new_raw(max_valid_raw_data.clone()).expect("Maximum valid Raw PaymentId should be valid");
        assert_eq!(max_raw_payment_id.get_size(), MAX_PAYMENT_ID_SIZE);
    }

    #[test]
    fn test_address_and_data_validation() {
        // Create a test single address (smaller)
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Test AddressAndData with valid size
        let small_user_data = vec![1, 2, 3, 4, 5];
        let fee = MicroMinotari::from(100u64);
        let address_and_data = PaymentId::new_address_and_data(
            single_address.clone(),
            fee,
            false,
            TxType::PaymentToOther,
            small_user_data.clone(),
        )
        .expect("Valid AddressAndData should be created");

        // Verify the size calculation
        let expected_size = PaymentId::calculate_address_and_data_size(&single_address, small_user_data.len());
        assert_eq!(address_and_data.get_size(), expected_size);
        assert!(address_and_data.get_size() <= MAX_PAYMENT_ID_SIZE);

        // Test AddressAndData with user data that would exceed limit
        let large_user_data = vec![0u8; MAX_PAYMENT_ID_SIZE];
        let result = PaymentId::new_address_and_data(
            single_address.clone(),
            fee,
            false,
            TxType::PaymentToOther,
            large_user_data,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));
    }

    #[test]
    fn test_transaction_info_validation() {
        // Create a test single address
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Test TransactionInfo with valid size
        let small_user_data = vec![1, 2, 3, 4, 5];
        let amount = MicroMinotari::from(1000u64);
        let fee = MicroMinotari::from(100u64);
        let sent_hashes = vec![create_random_fixed_hash(), create_random_fixed_hash()];

        let transaction_info = PaymentId::new_transaction_info(
            single_address.clone(),
            amount,
            fee,
            false,
            TxType::PaymentToOther,
            sent_hashes.clone(),
            small_user_data.clone(),
        )
        .expect("Valid TransactionInfo should be created");

        // Verify the size calculation
        let expected_size =
            PaymentId::calculate_transaction_info_size(&single_address, sent_hashes.len(), small_user_data.len());
        assert_eq!(transaction_info.get_size(), expected_size);
        assert!(transaction_info.get_size() <= MAX_PAYMENT_ID_SIZE);

        // Test TransactionInfo with too many hashes
        let many_hashes = vec![create_random_fixed_hash(); 10]; // 10 * 32 = 320 bytes just for hashes
        let result = PaymentId::new_transaction_info(
            single_address.clone(),
            amount,
            fee,
            false,
            TxType::PaymentToOther,
            many_hashes,
            small_user_data,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));
    }

    #[test]
    fn test_recursive_payment_id_validation() {
        // Create dual address with payment ID feature (recursion allowed but total size limited)
        let mut dual_address_with_payment_id = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();

        // Add small payment ID data to the address
        dual_address_with_payment_id = dual_address_with_payment_id
            .with_payment_id_user_data(vec![1, 2, 3, 4, 5])
            .unwrap();

        // Test that we CAN create AddressAndData with an address that contains payment_id (recursion allowed)
        let result = PaymentId::new_address_and_data(
            dual_address_with_payment_id.clone(),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![1, 2, 3],
        );
        assert!(
            result.is_ok(),
            "Recursion should be allowed as long as total size is under 256 bytes"
        );

        // Test that we CAN create TransactionInfo with an address that contains payment_id (recursion allowed)
        let result = PaymentId::new_transaction_info(
            dual_address_with_payment_id.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            vec![1, 2, 3],
        );
        assert!(
            result.is_ok(),
            "Recursion should be allowed as long as total size is under 256 bytes"
        );

        // Test that validation still fails if the total size would exceed 256 bytes
        // Create an address with large payment ID data
        let mut large_dual_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        large_dual_address = large_dual_address
            .with_payment_id_user_data(vec![0u8; 200]) // Large payload
            .unwrap();

        let result = PaymentId::new_address_and_data(
            large_dual_address,
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![1, 2, 3],
        );
        assert!(result.is_err(), "Should fail if total recursive size exceeds 256 bytes");
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));
    }

    #[test]
    fn test_deeply_nested_payment_id_size_validation() {
        // Test that deeply nested PaymentIds are correctly size-validated
        // Create a base dual address with small nested PaymentId
        let mut nested_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();

        // Add a small PaymentId to the address (level 1 nesting)
        nested_address = nested_address.with_payment_id_user_data(vec![1, 2, 3, 4, 5]).unwrap();

        // Verify the nested address size includes the PaymentId data
        let nested_address_size = nested_address.get_size();
        assert!(
            nested_address_size > TARI_ADDRESS_INTERNAL_DUAL_SIZE,
            "Address with PaymentId should be larger than base dual address"
        );

        // Test creating AddressAndData with the nested address (level 2 nesting)
        let result = PaymentId::new_address_and_data(
            nested_address.clone(),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![1, 2, 3, 4, 5], // Small user data
        );
        assert!(result.is_ok(), "Should succeed with reasonable nested size");

        let nested_payment_id = result.unwrap();
        let total_size = nested_payment_id.get_size();
        assert!(
            total_size <= MAX_PAYMENT_ID_SIZE,
            "Total nested PaymentId size should not exceed 256 bytes"
        );

        // Test creating TransactionInfo with nested address and verify size
        let result = PaymentId::new_transaction_info(
            nested_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![create_random_fixed_hash()], // One hash
            vec![1, 2, 3],                    // Small user data
        );
        assert!(result.is_ok(), "Should succeed with reasonable nested size");

        let nested_transaction_info = result.unwrap();
        let total_size = nested_transaction_info.get_size();
        assert!(
            total_size <= MAX_PAYMENT_ID_SIZE,
            "Total nested TransactionInfo size should not exceed 256 bytes"
        );

        // Test that validation fails when nested structure becomes too large
        // Create an address with larger PaymentId data
        let mut large_nested_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();

        // Add a large PaymentId that will make the total structure exceed 256 bytes
        large_nested_address = large_nested_address
            .with_payment_id_user_data(vec![0u8; 180]) // Large nested data
            .unwrap();

        // This should fail because the total size exceeds 256 bytes
        let result = PaymentId::new_address_and_data(
            large_nested_address.clone(),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![1, 2, 3, 4, 5], // Even small user data should fail
        );
        assert!(result.is_err(), "Should fail when total nested size exceeds 256 bytes");
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Verify the error shows the actual calculated size
        let calculated_size = PaymentId::calculate_address_and_data_size(&large_nested_address, 5);
        assert!(
            calculated_size > MAX_PAYMENT_ID_SIZE,
            "Calculated size should exceed the limit"
        );
    }

    #[test]
    fn test_open_from_string_validation() {
        // Test valid string
        let valid_string = "Hello World!";
        let open_payment_id = PaymentId::new_open_from_string(valid_string, TxType::PaymentToOther)
            .expect("Valid string should create Open PaymentId");

        match &open_payment_id.inner {
            InnerPaymentId::Open { user_data, tx_type } => {
                assert_eq!(user_data, valid_string.as_bytes());
                assert_eq!(*tx_type, TxType::PaymentToOther);
            },
            _ => panic!("Expected Open PaymentId"),
        }

        // Test string that would exceed size limit
        let large_string = "x".repeat(MAX_PAYMENT_ID_SIZE); // 256 chars
        let result = PaymentId::new_open_from_string(&large_string, TxType::PaymentToOther);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Test maximum valid string size
        let max_valid_string = "x".repeat(MAX_PAYMENT_ID_SIZE - 2); // 254 chars (256 - 1 tag - 1 tx_type)
        let max_open_payment_id = PaymentId::new_open_from_string(&max_valid_string, TxType::PaymentToOther)
            .expect("Maximum valid string should create Open PaymentId");
        assert_eq!(max_open_payment_id.get_size(), MAX_PAYMENT_ID_SIZE);
    }

    #[test]
    fn test_padding_behavior() {
        // Create a test single address
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Test that small AddressAndData gets padded to PADDING_SIZE
        let small_user_data = vec![1u8; 5];
        let address_and_data = PaymentId::new_address_and_data(
            single_address.clone(),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            small_user_data.clone(),
        )
        .expect("Valid AddressAndData should be created");

        let calculated_base_size =
            1 + 1 + single_address.get_size() + PaymentId::SIZE_META_DATA + 1 + small_user_data.len();
        assert!(calculated_base_size < PADDING_SIZE);
        assert_eq!(address_and_data.get_size(), PADDING_SIZE);

        // Test that small TransactionInfo gets padded to PADDING_SIZE
        let transaction_info = PaymentId::new_transaction_info(
            single_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            small_user_data.clone(),
        )
        .expect("Valid TransactionInfo should be created");

        let calculated_base_size =
            1 + 1 + single_address.get_size() + PaymentId::SIZE_VALUE_AND_META_DATA + 1 + 1 + small_user_data.len();
        assert!(calculated_base_size < PADDING_SIZE);
        assert_eq!(transaction_info.get_size(), PADDING_SIZE);
    }
}
