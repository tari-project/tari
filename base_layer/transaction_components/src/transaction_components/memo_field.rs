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
    io,
};

use borsh::{BorshDeserialize, BorshSerialize};
use integer_encoding::{VarIntReader, VarIntWriter};
use log::debug;
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use tari_common_types::{
    tari_address::{
        MAX_ENCRYPTED_DATA_SIZE,
        TARI_ADDRESS_INTERNAL_DUAL_SIZE,
        TARI_ADDRESS_INTERNAL_SINGLE_SIZE,
        TariAddress,
    },
    types::FixedHash,
};
use tari_utilities::hex::Hex;

use crate::{
    MicroMinotari,
    transaction_components::encrypted_data::{SIZE_U256, SIZE_VALUE},
};
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
pub struct MemoField {
    inner: InnerMemoField,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
enum InnerMemoField {
    // No payment ID.
    #[default]
    Empty,
    // A u256 number.
    U256(U256),
    // Open - the user optionally specifies 'payment_id' ('tx_type' is added by the system).
    Open {
        payment_id: Vec<u8>,
        tx_type: TxType,
    },
    // This payment ID is automatically generated by the system for output UTXOs. The optional user specified
    // `MemoField::Open` payment ID will be assigned to `tx_type` and `payment_id`; the system adds in the sender
    // address.
    AddressAndData {
        sender_address: TariAddress,
        sender_one_sided: bool,
        fee: MicroMinotari,
        tx_type: TxType,
        payment_id: Vec<u8>,
    },
    // This payment ID is automatically generated by the system for change outputs. The optional user specified
    // `MemoField::Open` payment ID will be assigned to `tx_type` and `payment_id`; the system adds in the other
    // data address.
    TransactionInfo {
        recipient_address: TariAddress,
        sender_one_sided: bool,
        amount: MicroMinotari,
        fee: MicroMinotari,
        tx_type: TxType,
        sent_output_hashes: Vec<FixedHash>,
        payment_id: Vec<u8>,
    },
    // This is a fallback if nothing else fits, so we want to preserve the raw bytes.
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

impl MemoField {
    const SIZE_META_DATA: usize = 5;
    const SIZE_VALUE_AND_META_DATA: usize = SIZE_VALUE + MemoField::SIZE_META_DATA;

    /// Calculates the actual size that would be used by an AddressAndData PaymentId
    /// This includes the recursive size of any PaymentIds contained within the address
    fn calculate_address_and_data_size(address: &TariAddress, payment_id_len: usize) -> usize {
        let base_size = address
            .get_size()
            .saturating_add(MemoField::SIZE_META_DATA)
            .saturating_add(payment_id_len)
            .saturating_add(3);
        std::cmp::max(base_size, PADDING_SIZE)
    }

    /// Calculates the actual size that would be used by a TransactionInfo PaymentId
    /// This includes the recursive size of any PaymentIds contained within the address
    fn calculate_transaction_info_size(
        address: &TariAddress,
        sent_output_hashes_len: usize,
        payment_id_len: usize,
    ) -> usize {
        let base_size = address
            .get_size()
            .saturating_add(MemoField::SIZE_VALUE_AND_META_DATA)
            .saturating_add(sent_output_hashes_len.saturating_mul(FixedHash::byte_size()))
            .saturating_add(payment_id_len)
            .saturating_add(4);
        std::cmp::max(base_size, PADDING_SIZE)
    }

    pub fn new_address_and_data(
        sender_address: TariAddress,
        fee: MicroMinotari,
        sender_one_sided: bool,
        tx_type: TxType,
        payment_id: Vec<u8>,
    ) -> Result<Self, String> {
        // Calculate the actual size this PaymentId would occupy (including any nested PaymentIds in the address)
        let total_size = Self::calculate_address_and_data_size(&sender_address, payment_id.len());

        if total_size > MAX_ENCRYPTED_DATA_SIZE {
            return Err(format!(
                "PaymentId exceeds {}-byte limit: {} bytes (address: {} bytes, payment_id: {} bytes, overhead: {} \
                 bytes)",
                MAX_ENCRYPTED_DATA_SIZE,
                total_size,
                sender_address.get_size(),
                payment_id.len(),
                total_size
                    .saturating_sub(sender_address.get_size())
                    .saturating_sub(payment_id.len())
            ));
        }

        let payment_id = InnerMemoField::AddressAndData {
            sender_address,
            fee,
            sender_one_sided,
            tx_type,
            payment_id,
        };

        Ok(MemoField { inner: payment_id })
    }

    pub fn new_transaction_info(
        recipient_address: TariAddress,
        amount: MicroMinotari,
        fee: MicroMinotari,
        sender_one_sided: bool,
        tx_type: TxType,
        sent_output_hashes: Vec<FixedHash>,
        payment_id: Vec<u8>,
    ) -> Result<Self, String> {
        // Calculate the actual size this PaymentId would occupy (including any nested PaymentIds in the address)
        let total_size =
            Self::calculate_transaction_info_size(&recipient_address, sent_output_hashes.len(), payment_id.len());

        if total_size > MAX_ENCRYPTED_DATA_SIZE {
            return Err(format!(
                "PaymentId exceeds {}-byte limit: {} bytes (address: {} bytes, hashes: {} bytes, payment_id: {} \
                 bytes, overhead: {} bytes)",
                MAX_ENCRYPTED_DATA_SIZE,
                total_size,
                recipient_address.get_size(),
                sent_output_hashes.len().saturating_mul(FixedHash::byte_size()),
                payment_id.len(),
                total_size
                    .saturating_sub(recipient_address.get_size())
                    .saturating_sub(sent_output_hashes.len().saturating_mul(FixedHash::byte_size()))
                    .saturating_sub(payment_id.len())
            ));
        }

        let payment_id = InnerMemoField::TransactionInfo {
            recipient_address,
            amount,
            fee,
            sender_one_sided,
            tx_type,
            sent_output_hashes,
            payment_id,
        };

        Ok(MemoField { inner: payment_id })
    }

    pub fn new_empty() -> Self {
        MemoField {
            inner: InnerMemoField::Empty,
        }
    }

    pub fn new_raw(data: Vec<u8>) -> Result<Self, String> {
        // Raw Memo: 1 byte for tag + data.len() bytes for data
        let total_size = data.len().saturating_add(1);

        if total_size > MAX_ENCRYPTED_DATA_SIZE {
            return Err(format!(
                "Memo exceeds {}-byte limit: {} bytes (data: {} bytes, tag: 1 byte)",
                MAX_ENCRYPTED_DATA_SIZE,
                total_size,
                data.len()
            ));
        }
        Ok(MemoField {
            inner: InnerMemoField::Raw(data),
        })
    }

    pub fn new_u256(value: U256) -> MemoField {
        // U256 Memo: 1 byte for tag + 32 bytes for U256 = 33 bytes total
        // This always fits within 256 bytes, no runtime validation needed
        MemoField {
            inner: InnerMemoField::U256(value),
        }
    }

    /// Helper function to create a validated `MemoField::Open` from user data and transaction type
    pub fn new_open(payment_id: Vec<u8>, tx_type: TxType) -> Result<Self, String> {
        // Open Memo: 1 byte for tag + payment_id.len() bytes + 1 byte for tx_type
        let total_size = payment_id.len().saturating_add(2);

        if total_size > MAX_ENCRYPTED_DATA_SIZE {
            return Err(format!(
                "Memo exceeds {}-byte limit: {} bytes (payment_id: {} bytes, tag: 1 byte, tx_type: 1 byte)",
                MAX_ENCRYPTED_DATA_SIZE,
                total_size,
                payment_id.len()
            ));
        }

        Ok(MemoField {
            inner: InnerMemoField::Open { payment_id, tx_type },
        })
    }

    /// Helper function to create a validated `MemoField::Open` from a string and transaction type
    pub fn new_open_from_string(s: &str, tx_type: TxType) -> Result<Self, String> {
        Self::new_open(s.as_bytes().to_vec(), tx_type)
    }

    fn to_tag(&self) -> Vec<u8> {
        match &self.inner {
            InnerMemoField::Empty => vec![],
            InnerMemoField::U256(_) => vec![PTag::U256 as u8],
            InnerMemoField::Open { .. } => vec![PTag::Open as u8],
            InnerMemoField::AddressAndData { .. } => vec![PTag::AddressAndData as u8],
            InnerMemoField::TransactionInfo { .. } => vec![PTag::TransactionInfo as u8],
            InnerMemoField::Raw(_) => vec![PTag::Raw as u8],
        }
    }

    pub fn get_size(&self) -> usize {
        match &self.inner {
            // Empty payment ID has no bytes
            InnerMemoField::Empty => 0,

            // U256 payment ID:
            // - 1 byte for the PTag (enum discriminator)
            // - SIZE_U256 bytes for the U256 value (32 bytes = size_of::<U256>())
            InnerMemoField::U256(_) => 1 + SIZE_U256,

            // Open payment ID:
            // - 1 byte for the PTag (enum discriminator)
            // - payment_id.len() bytes for the variable-length payment ID
            // - 1 byte for the TxType (transaction type as u8)
            InnerMemoField::Open { payment_id, .. } => payment_id.len().saturating_add(2),

            InnerMemoField::AddressAndData {
                sender_address,
                payment_id,
                ..
            } => {
                // AddressAndData payment ID:
                // - 1 byte for the PTag (enum discriminator)
                // - 1 byte for sender_one_sided boolean flag
                // - sender_address.get_size() bytes for the TariAddress (67 bytes for dual, 35 bytes for single)
                // - MemoField::SIZE_META_DATA bytes for metadata (5 bytes: 1 byte TxType + 4 bytes fee as u32)
                // - 1 byte for payment_id length
                // - payment_id.len() bytes for the variable-length payment ID
                let len = sender_address
                    .get_size()
                    .saturating_add(MemoField::SIZE_META_DATA)
                    .saturating_add(payment_id.len())
                    .saturating_add(3);
                // Ensure minimum size of PADDING_SIZE (130 bytes) for consistent serialization
                std::cmp::max(len, PADDING_SIZE)
            },

            InnerMemoField::TransactionInfo {
                recipient_address,
                payment_id,
                sent_output_hashes,
                ..
            } => {
                // TransactionInfo payment ID:
                // - 1 byte for the PTag (enum discriminator)
                // - 1 byte for sender_one_sided boolean flag
                // - recipient_address.get_size() bytes for the TariAddress (67 bytes for dual, 35 bytes for single)
                // - MemoField::SIZE_VALUE_AND_META_DATA bytes for value and metadata (13 bytes: 8 bytes amount + 5
                //   bytes metadata)
                // - 1 byte for sent_output_hashes length
                // - (sent_output_hashes.len() * FixedHash::byte_size()) bytes for output hashes (32 bytes per hash)
                // - 1 byte for payment_id length
                // - payment_id.len() bytes for the variable-length payment ID
                let len = recipient_address
                    .get_size()
                    .saturating_add(MemoField::SIZE_VALUE_AND_META_DATA)
                    .saturating_add(sent_output_hashes.len().saturating_mul(FixedHash::byte_size()))
                    .saturating_add(payment_id.len())
                    .saturating_add(4);
                // Ensure minimum size of PADDING_SIZE (130 bytes) for consistent serialization
                if len < PADDING_SIZE { PADDING_SIZE } else { len }
            },

            InnerMemoField::Raw(bytes) => {
                // Raw payment ID:
                // - 1 byte for the PTag (enum discriminator)
                // - bytes.len() bytes for the raw data
                bytes.len().saturating_add(1)
            },
        }
    }

    pub fn get_fee(&self) -> Option<MicroMinotari> {
        match &self.inner {
            InnerMemoField::AddressAndData { fee, .. } | InnerMemoField::TransactionInfo { fee, .. } => Some(*fee),
            _ => None,
        }
    }

    pub fn get_sent_hashes(&self) -> Option<Vec<FixedHash>> {
        match &self.inner {
            InnerMemoField::TransactionInfo { sent_output_hashes, .. } => Some(sent_output_hashes.clone()),
            _ => None,
        }
    }

    /// Helper function to set the 'amount' of a 'MemoField::TransactionInfo'
    pub fn transaction_info_set_amount(&mut self, amount: MicroMinotari) {
        if let InnerMemoField::TransactionInfo { amount: ref mut a, .. } = self.inner {
            *a = amount;
        }
    }

    /// Helper function to set the 'fee' of a 'MemoField::TransactionInfo' or 'MemoField::AddressAndData'
    pub fn set_fee(&mut self, amount: MicroMinotari) {
        match &mut self.inner {
            InnerMemoField::TransactionInfo { fee, .. } | InnerMemoField::AddressAndData { fee, .. } => {
                *fee = amount;
            },
            _ => {},
        }
    }

    pub fn get_type(&self) -> TxType {
        match &self.inner {
            InnerMemoField::Open { tx_type, .. } |
            InnerMemoField::AddressAndData { tx_type, .. } |
            InnerMemoField::TransactionInfo { tx_type, .. } => *tx_type,
            _ => TxType::default(),
        }
    }

    /// Helper function to set the 'recipient_address' of a 'MemoField::TransactionInfo'
    pub fn transaction_info_set_address(&mut self, address: TariAddress) -> Result<(), String> {
        if let InnerMemoField::TransactionInfo {
            ref mut recipient_address,
            ref sent_output_hashes,
            ref payment_id,
            ..
        } = self.inner
        {
            // Calculate the new size with the updated address
            let total_size =
                Self::calculate_transaction_info_size(&address, sent_output_hashes.len(), payment_id.len());

            if total_size > MAX_ENCRYPTED_DATA_SIZE {
                return Err(format!(
                    "Setting address would exceed {}-byte limit: {} bytes (new address: {} bytes, hashes: {} bytes, \
                     payment_id: {} bytes, overhead: {} bytes)",
                    MAX_ENCRYPTED_DATA_SIZE,
                    total_size,
                    address.get_size(),
                    sent_output_hashes.len().saturating_mul(FixedHash::byte_size()),
                    payment_id.len(),
                    total_size
                        .saturating_sub(address.get_size())
                        .saturating_sub(sent_output_hashes.len().saturating_mul(FixedHash::byte_size()))
                        .saturating_sub(payment_id.len())
                ));
            }

            *recipient_address = address;
            Ok(())
        } else {
            Err("Cannot set address on non-TransactionInfo memo field".to_string())
        }
    }

    pub fn transaction_info_set_sent_output_hashes(
        &mut self,
        sent_output_hashes: Vec<FixedHash>,
    ) -> Result<(), String> {
        if let InnerMemoField::TransactionInfo {
            ref recipient_address,
            ref payment_id,
            sent_output_hashes: ref mut hashes,
            ..
        } = self.inner
        {
            // Calculate the new size with the updated hashes
            let total_size =
                Self::calculate_transaction_info_size(recipient_address, sent_output_hashes.len(), payment_id.len());

            if total_size > MAX_ENCRYPTED_DATA_SIZE {
                return Err(format!(
                    "Setting sent output hashes would exceed {}-byte limit: {} bytes (address: {} bytes, new hashes: \
                     {} bytes, payment_id: {} bytes, overhead: {} bytes)",
                    MAX_ENCRYPTED_DATA_SIZE,
                    total_size,
                    recipient_address.get_size(),
                    sent_output_hashes.len().saturating_mul(FixedHash::byte_size()),
                    payment_id.len(),
                    total_size
                        .saturating_sub(recipient_address.get_size())
                        .saturating_sub(sent_output_hashes.len().saturating_mul(FixedHash::byte_size()))
                        .saturating_sub(payment_id.len())
                ));
            }

            *hashes = sent_output_hashes;
            Ok(())
        } else {
            Err("Cannot set sent output hashes on non-TransactionInfo memo field".to_string())
        }
    }

    pub fn update_fee(&mut self, new_fee: MicroMinotari) {
        match self.inner {
            InnerMemoField::TransactionInfo { fee: ref mut old, .. } |
            InnerMemoField::AddressAndData { fee: ref mut old, .. } => {
                *old = new_fee;
            },
            _ => {},
        }
    }

    /// Helper function to set the 'payment_id' of a 'MemoField::TransactionInfo'
    pub fn transaction_info_set_payment_id(&mut self, payment_id: Vec<u8>) -> Result<(), String> {
        if let InnerMemoField::TransactionInfo {
            ref recipient_address,
            ref sent_output_hashes,
            payment_id: ref mut current_payment_id,
            ..
        } = self.inner
        {
            // Calculate the new size with the updated payment_id
            let total_size =
                Self::calculate_transaction_info_size(recipient_address, sent_output_hashes.len(), payment_id.len());

            if total_size > MAX_ENCRYPTED_DATA_SIZE {
                return Err(format!(
                    "Setting payment ID would exceed {}-byte limit: {} bytes (address: {} bytes, hashes: {} bytes, \
                     new payment_id: {} bytes, overhead: {} bytes)",
                    MAX_ENCRYPTED_DATA_SIZE,
                    total_size,
                    recipient_address.get_size(),
                    sent_output_hashes.len().saturating_mul(FixedHash::byte_size()),
                    payment_id.len(),
                    total_size
                        .saturating_sub(recipient_address.get_size())
                        .saturating_sub(sent_output_hashes.len().saturating_mul(FixedHash::byte_size()))
                        .saturating_sub(payment_id.len())
                ));
            }

            *current_payment_id = payment_id;
            Ok(())
        } else {
            Err("Cannot set payment ID on non-TransactionInfo memo field".to_string())
        }
    }

    /// Helper function to convert a 'MemoField::Open' or 'MemoField::Empty' to a
    /// 'MemoField::AddressAndData', with the optional 'tx_type' only applicable to 'MemoField::Open',
    /// otherwise 'payment_id' is kept as is.
    pub fn add_sender_address(
        self,
        sender_address: TariAddress,
        sender_one_sided: bool,
        fee: MicroMinotari,
        tx_type: Option<TxType>,
    ) -> Result<MemoField, String> {
        match self.inner {
            InnerMemoField::Open { payment_id, tx_type } => {
                MemoField::new_address_and_data(sender_address, fee, sender_one_sided, tx_type, payment_id)
            },
            InnerMemoField::Empty => MemoField::new_address_and_data(
                sender_address,
                fee,
                sender_one_sided,
                tx_type.unwrap_or_default(),
                vec![],
            ),
            _ => Ok(self),
        }
    }

    // This method is infallible; any out-of-bound values will be zeroed.
    fn pack_meta_data(&self) -> Vec<u8> {
        match &self.inner {
            InnerMemoField::TransactionInfo {
                fee,
                sender_one_sided,
                tx_type,
                ..
            } |
            InnerMemoField::AddressAndData {
                fee,
                sender_one_sided,
                tx_type,
                ..
            } => {
                let mut bytes = Vec::with_capacity(5);
                // Zero out-of-bound values
                // - Use 4 bytes for 'fee', max value: 4,294,967,295
                let fee = if fee.as_u64() > u64::from(u32::MAX) {
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

    pub fn payment_id_as_bytes(&self) -> Vec<u8> {
        match &self.inner {
            InnerMemoField::Empty => vec![],
            InnerMemoField::U256(v) => {
                let bytes: &mut [u8] = &mut [0; SIZE_U256];
                v.to_little_endian(bytes);
                bytes.to_vec()
            },
            InnerMemoField::Open { payment_id, .. } => payment_id.clone(),
            InnerMemoField::AddressAndData { payment_id, .. } => payment_id.clone(),
            InnerMemoField::TransactionInfo { payment_id, .. } => payment_id.clone(),
            InnerMemoField::Raw(bytes) => bytes.clone(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match &self.inner {
            InnerMemoField::Empty => Vec::new(),
            InnerMemoField::U256(v) => {
                let mut bytes = self.to_tag();
                let mut value = vec![0; 32];
                v.to_little_endian(&mut value);
                bytes.extend_from_slice(&value);
                bytes
            },
            InnerMemoField::Open { payment_id, tx_type } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&tx_type.as_bytes());
                bytes.extend_from_slice(payment_id);
                bytes
            },
            InnerMemoField::AddressAndData {
                sender_address,
                payment_id,
                ..
            } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&self.pack_meta_data());
                let address_bytes = sender_address.to_vec();
                bytes.push(u8::try_from(address_bytes.len()).expect("User data length should fit in a u8"));
                bytes.extend_from_slice(&address_bytes);
                bytes.push(u8::try_from(payment_id.len()).expect("Payment ID length should fit in a u8"));
                bytes.extend_from_slice(payment_id);
                // Ensure we have enough padding to match the min size
                while bytes.len() < PADDING_SIZE {
                    bytes.push(0);
                }
                bytes
            },
            InnerMemoField::TransactionInfo {
                recipient_address,
                amount,
                payment_id,
                sent_output_hashes,
                ..
            } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&amount.as_u64().to_le_bytes());
                bytes.extend_from_slice(&self.pack_meta_data());
                let address_bytes = recipient_address.to_vec();
                bytes.push(u8::try_from(address_bytes.len()).expect("User data length should fit in a u8"));
                bytes.extend_from_slice(&address_bytes.to_vec());
                bytes.push(u8::try_from(payment_id.len()).expect("Payment ID length should fit in a u8"));
                bytes.extend_from_slice(payment_id);
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
            InnerMemoField::Raw(data) => {
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
                return MemoField {
                    inner: InnerMemoField::Open {
                        tx_type: TxType::PaymentToOther,
                        payment_id: bytes.to_vec(),
                    },
                };
            }
        }

        let p_tag = if bytes.is_empty() {
            PTag::Empty
        } else {
            PTag::from_u8(*bytes.first().expect("Already checked"))
        };
        let bytes = if bytes.len() > 1 {
            bytes.get(1..).expect("Already checked")
        } else {
            &[]
        };
        match p_tag {
            PTag::Empty => {
                return MemoField {
                    inner: InnerMemoField::Empty,
                };
            },
            PTag::U256 => {
                if bytes.len() != SIZE_U256 {
                    let inner_payment_id = InnerMemoField::Open {
                        tx_type: TxType::from_u8(*bytes.first().unwrap_or(&0)),
                        payment_id: bytes.get(1..).unwrap_or_default().to_vec(),
                    };
                    return MemoField {
                        inner: inner_payment_id,
                    };
                }
                let v = U256::from_little_endian(bytes);
                return MemoField {
                    inner: InnerMemoField::U256(v),
                };
            },
            PTag::Open => {
                let inner_payment_id = InnerMemoField::Open {
                    tx_type: TxType::from_u8(*bytes.first().unwrap_or(&0)),
                    payment_id: bytes.get(1..).unwrap_or_default().to_vec(),
                };
                return MemoField {
                    inner: inner_payment_id,
                };
            },
            PTag::Raw => {
                return MemoField {
                    inner: InnerMemoField::Raw(raw_bytes),
                };
            },
            _ => {},
        }

        match MemoField::try_deserialize_address_or_transaction_data(bytes, p_tag) {
            Ok(payment_id) => payment_id,
            Err(e) => {
                debug!("Failed to parse Memo from bytes: {e}, returning Raw");
                MemoField {
                    inner: InnerMemoField::Raw(raw_bytes),
                }
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn try_deserialize_address_or_transaction_data(bytes: &[u8], p_tag: PTag) -> Result<MemoField, String> {
        if bytes.len() < MemoField::SIZE_VALUE_AND_META_DATA {
            // if the bytes are too short, we cannot parse it as AddressAndData or TransactionInfo
            return Err("Not enough bytes to parse AddressAndData or TransactionInfo".to_string());
        }

        if p_tag == PTag::TransactionInfoV1 || p_tag == PTag::AddressAndDataV1 {
            let mut amount_bytes = [0u8; SIZE_VALUE];
            amount_bytes.copy_from_slice(bytes.get(0..SIZE_VALUE).expect("Already checked"));
            let amount = MicroMinotari::from(u64::from_le_bytes(amount_bytes));
            let mut meta_data_bytes = [0u8; MemoField::SIZE_META_DATA];
            meta_data_bytes.copy_from_slice(
                bytes
                    .get(SIZE_VALUE..MemoField::SIZE_VALUE_AND_META_DATA)
                    .expect("Already checked"),
            );
            let (fee, sender_one_sided, tx_meta_data) = MemoField::unpack_meta_data(meta_data_bytes);
            let (address, size) = if let Ok((address, size)) = Self::find_tari_address(
                bytes
                    .get(MemoField::SIZE_VALUE_AND_META_DATA..)
                    .expect("Already checked"),
            ) {
                (address, size)
            } else {
                // if we cannot find a valid TariAddress, we return the raw bytes
                return Err("No valid TariAddress found in bytes".to_string());
            };

            // legacy support for AddressAndDataV1
            if p_tag == PTag::AddressAndDataV1 {
                let payment_id = bytes
                    .get(MemoField::SIZE_VALUE_AND_META_DATA.saturating_add(size)..)
                    .expect("Already checked")
                    .to_vec();
                return Ok(MemoField {
                    inner: InnerMemoField::AddressAndData {
                        sender_address: address,
                        sender_one_sided,
                        fee,
                        tx_type: tx_meta_data,
                        payment_id,
                    },
                });
            }

            // legacy support for TransactionInfoV1
            if p_tag == PTag::TransactionInfoV1 {
                let payment_id = bytes
                    .get(MemoField::SIZE_VALUE_AND_META_DATA.saturating_add(size)..)
                    .expect("Already checked")
                    .to_vec();
                return Ok(MemoField {
                    inner: InnerMemoField::TransactionInfo {
                        recipient_address: address,
                        sender_one_sided,
                        amount,
                        fee,
                        tx_type: tx_meta_data,
                        payment_id,
                        sent_output_hashes: vec![],
                    },
                });
            }
        }
        // now we assume this has to be off type AddressAndData or TransactionInfo
        let data_start_index = if p_tag == PTag::AddressAndData { 0 } else { SIZE_VALUE };
        let metadata_end_index = if p_tag == PTag::AddressAndData {
            MemoField::SIZE_META_DATA
        } else {
            MemoField::SIZE_VALUE_AND_META_DATA
        };

        let mut meta_data_bytes = [0u8; MemoField::SIZE_META_DATA];
        meta_data_bytes.copy_from_slice(
            bytes
                .get(data_start_index..metadata_end_index)
                .ok_or("Not enough bytes for meta data")?,
        );
        let (fee, sender_one_sided, tx_meta_data) = MemoField::unpack_meta_data(meta_data_bytes);

        let address_size = *bytes
            .get(metadata_end_index)
            .ok_or("Address bytes does not have size encoded")? as usize;
        let address = TariAddress::from_bytes(
            bytes
                .get(
                    metadata_end_index.saturating_add(1)..
                        metadata_end_index.saturating_add(1).saturating_add(address_size),
                )
                .ok_or("Not enough bytes for TariAddress")?,
        )
        .map_err(|_| "Invalid TariAddress in bytes".to_string())?;
        let payment_id_length = *bytes
            .get(metadata_end_index.saturating_add(1).saturating_add(address_size))
            .ok_or("Payment ID bytes does not have length encoded")? as usize;
        let payment_id_start = metadata_end_index.saturating_add(2).saturating_add(address_size);
        let payment_id = bytes
            .get(payment_id_start..payment_id_start.saturating_add(payment_id_length))
            .ok_or("Not enough bytes for payment ID")?;

        if p_tag == PTag::AddressAndData {
            if !Self::check_padding(bytes, payment_id_start.saturating_add(payment_id_length)) {
                return Err("Invalid padding for AddressAndData".to_string());
            }
            return Ok(MemoField {
                inner: InnerMemoField::AddressAndData {
                    sender_address: address,
                    sender_one_sided,
                    fee,
                    tx_type: tx_meta_data,
                    payment_id: payment_id.to_vec(),
                },
            });
        }
        // so this must be a TransactionInfo
        let mut amount_bytes = [0u8; SIZE_VALUE];
        amount_bytes.copy_from_slice(bytes.get(0..SIZE_VALUE).ok_or("Not enough bytes for amount")?);
        let amount = MicroMinotari::from(u64::from_le_bytes(amount_bytes));
        let mut sent_output_hashes = Vec::new();
        let sent_output_hashes_length = *bytes
            .get(payment_id_start.saturating_add(payment_id_length))
            .ok_or("Sent output hashes bytes does not have length encoded")?
            as usize;
        let sent_output_hashes_start = payment_id_start.saturating_add(payment_id_length).saturating_add(1);
        for hash_num in 0..sent_output_hashes_length {
            let hash_start =
                sent_output_hashes_start.saturating_add(hash_num.saturating_mul(FixedHash::byte_size()));
            let hash_end = hash_start.saturating_add(FixedHash::byte_size());
            let hash = bytes
                .get(hash_start..hash_end)
                .ok_or("Not enough bytes for sent output hash")?;
            let sent_output_hash = FixedHash::try_from(hash).map_err(|_| "Invalid sent output hash".to_string())?;
            sent_output_hashes.push(sent_output_hash);
        }
        if !Self::check_padding(
            bytes,
            sent_output_hashes_start.saturating_add(sent_output_hashes_length.saturating_mul(FixedHash::byte_size())),
        ) {
            return Err("Invalid padding for TransactionInfo".to_string());
        }
        Ok(MemoField {
            inner: InnerMemoField::TransactionInfo {
                recipient_address: address,
                sender_one_sided,
                amount,
                fee,
                tx_type: tx_meta_data,
                payment_id: payment_id.to_vec(),
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
        for &byte in bytes.get(start_index..).expect("Already checked") {
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
        while TARI_ADDRESS_INTERNAL_DUAL_SIZE.saturating_add(offset) <= bytes.len() {
            let end = TARI_ADDRESS_INTERNAL_DUAL_SIZE.saturating_add(offset);
            if let Ok(address) = TariAddress::from_bytes(bytes.get(..end).expect("Already checked")) {
                return Ok((address, end));
            }
            offset = offset.saturating_add(1);
        }
        if let Ok(address) =
            TariAddress::from_bytes(bytes.get(..TARI_ADDRESS_INTERNAL_SINGLE_SIZE).expect("Already checked"))
        {
            return Ok((address, TARI_ADDRESS_INTERNAL_SINGLE_SIZE));
        }
        Err("No valid TariAddress found".to_string())
    }

    /// Helper function to convert a byte slice to a string for the open and data variants
    pub fn stringify_bytes(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).to_string()
    }

    /// Helper function to display the payment id's user data
    pub fn payment_id_as_string(&self) -> String {
        match &self.inner {
            InnerMemoField::Empty => self.to_string(),
            InnerMemoField::U256(v) => format!("{v}"),
            InnerMemoField::Open { payment_id, .. } => MemoField::stringify_bytes(payment_id),
            InnerMemoField::AddressAndData { payment_id, .. } => MemoField::stringify_bytes(payment_id),
            InnerMemoField::TransactionInfo { payment_id, .. } => MemoField::stringify_bytes(payment_id),
            InnerMemoField::Raw(bytes) => bytes.to_hex(),
        }
    }

    /// Convenience method for pattern matching - checks if this is an Empty payment ID
    pub fn is_empty(&self) -> bool {
        matches!(self.inner, InnerMemoField::Empty)
    }

    /// Convenience method for pattern matching - checks if this is a U256 payment ID
    pub fn is_u256(&self) -> bool {
        matches!(self.inner, InnerMemoField::U256(_))
    }

    /// Convenience method for pattern matching - checks if this is an Open payment ID
    pub fn is_open(&self) -> bool {
        matches!(self.inner, InnerMemoField::Open { .. })
    }

    /// Convenience method for pattern matching - checks if this is an AddressAndData payment ID
    pub fn is_address_and_data(&self) -> bool {
        matches!(self.inner, InnerMemoField::AddressAndData { .. })
    }

    /// Convenience method for pattern matching - checks if this is a TransactionInfo payment ID
    pub fn is_transaction_info(&self) -> bool {
        matches!(self.inner, InnerMemoField::TransactionInfo { .. })
    }

    /// Convenience method for pattern matching - checks if this is a Raw payment ID
    pub fn is_raw(&self) -> bool {
        matches!(self.inner, InnerMemoField::Raw(_))
    }

    /// Get user data from Open, AddressAndData, or TransactionInfo variants
    /// Returns empty Vec for other variants
    pub fn get_payment_id(&self) -> Vec<u8> {
        match &self.inner {
            InnerMemoField::Open { payment_id, .. } |
            InnerMemoField::AddressAndData { payment_id, .. } |
            InnerMemoField::TransactionInfo { payment_id, .. } => payment_id.clone(),
            _ => Vec::new(),
        }
    }

    /// Get transaction type from variants that have it
    /// Returns None for variants without tx_type
    pub fn get_tx_type(&self) -> Option<TxType> {
        match &self.inner {
            InnerMemoField::Open { tx_type, .. } |
            InnerMemoField::AddressAndData { tx_type, .. } |
            InnerMemoField::TransactionInfo { tx_type, .. } => Some(*tx_type),
            _ => None,
        }
    }

    /// Get the sender address from AddressAndData variant
    /// Returns None for other variants
    pub fn get_sender_address(&self) -> Option<TariAddress> {
        match &self.inner {
            InnerMemoField::AddressAndData { sender_address, .. } => Some(sender_address.to_owned()),
            _ => None,
        }
    }

    /// Get the recipient address from TransactionInfo variant
    /// Returns None for other variants
    pub fn get_recipient_address(&self) -> Option<TariAddress> {
        match &self.inner {
            InnerMemoField::TransactionInfo { recipient_address, .. } => Some(recipient_address.to_owned()),
            _ => None,
        }
    }

    /// Get the amount from TransactionInfo variant
    /// Returns None for other variants
    pub fn get_amount(&self) -> Option<MicroMinotari> {
        match &self.inner {
            InnerMemoField::TransactionInfo { amount, .. } => Some(*amount),
            _ => None,
        }
    }

    /// Get the sender_one_sided flag from AddressAndData or TransactionInfo variants
    /// Returns None for other variants
    pub fn get_sender_one_sided(&self) -> Option<bool> {
        match &self.inner {
            InnerMemoField::AddressAndData { sender_one_sided, .. } |
            InnerMemoField::TransactionInfo { sender_one_sided, .. } => Some(*sender_one_sided),
            _ => None,
        }
    }

    /// Get the U256 value from U256 variant
    /// Returns None for other variants
    pub fn get_u256(&self) -> Option<U256> {
        match &self.inner {
            InnerMemoField::U256(value) => Some(*value),
            _ => None,
        }
    }

    /// Get raw bytes from Raw variant
    /// Returns None for other variants
    pub fn get_raw_bytes(&self) -> Option<&[u8]> {
        match &self.inner {
            InnerMemoField::Raw(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Get open payment ID user data
    /// Returns None for other variants
    pub fn get_open_payment_id(&self) -> Option<&[u8]> {
        match &self.inner {
            InnerMemoField::Open { payment_id, .. } => Some(payment_id),
            _ => None,
        }
    }

    /// Get u64 data from U256 or Open variants
    /// Returns Err for other variants
    pub fn get_u64_data(&self) -> Result<u64, String> {
        match &self.inner {
            InnerMemoField::U256(index) => {
                u64::try_from(*index).map_err(|_| "U256 value exceeds u64 range".to_string())
            },
            InnerMemoField::Open { payment_id, .. } => {
                if payment_id.len() != std::mem::size_of::<u64>() {
                    return Err(format!(
                        "Invalid payment id: expected {} bytes, got {}",
                        std::mem::size_of::<u64>(),
                        payment_id.len()
                    ));
                }
                let bytes: [u8; std::mem::size_of::<u64>()] = payment_id
                    .clone()
                    .try_into()
                    .map_err(|_| "Invalid payment id: expected u64 bytes".to_string())?;
                Ok(u64::from_le_bytes(bytes))
            },
            _ => Err(format!(
                "Invalid memo: expected 8 bytes in 'MemoField::U256' or 'MemoField::Open' , received {:?}",
                self.inner
            )),
        }
    }

    /// Unchecked constructor for Open payment ID
    /// Use this for migration from old direct enum construction
    /// WARNING: This bypasses validation - use new_open() for new code
    pub fn open_unchecked(payment_id: Vec<u8>, tx_type: TxType) -> Self {
        MemoField {
            inner: InnerMemoField::Open { payment_id, tx_type },
        }
    }

    /// Unchecked constructor for Raw payment ID
    /// Use this for migration from old direct enum construction
    /// WARNING: This bypasses validation - use new_raw() for new code
    pub fn raw_unchecked(data: Vec<u8>) -> Self {
        MemoField {
            inner: InnerMemoField::Raw(data),
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
        payment_id: Vec<u8>,
    ) -> Self {
        MemoField {
            inner: InnerMemoField::AddressAndData {
                sender_address,
                sender_one_sided,
                fee,
                tx_type,
                payment_id,
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
        payment_id: Vec<u8>,
    ) -> Self {
        MemoField {
            inner: InnerMemoField::TransactionInfo {
                recipient_address,
                sender_one_sided,
                amount,
                fee,
                tx_type,
                sent_output_hashes,
                payment_id,
            },
        }
    }

    /// Extract transaction information for wallet transaction processing
    /// Returns (sender_address, recipient_address, amount, tx_type, sender_one_sided) if available
    pub fn get_transaction_info(&self) -> Option<(TariAddress, TariAddress, MicroMinotari, TxType, bool)> {
        match &self.inner {
            InnerMemoField::AddressAndData {
                sender_address,
                tx_type,
                ..
            } => Some((
                sender_address.clone(),
                TariAddress::default(), // Will be set by caller based on context
                MicroMinotari::zero(),  // Amount not available in AddressAndData
                *tx_type,
                false, // Default for AddressAndData
            )),
            InnerMemoField::TransactionInfo {
                recipient_address,
                amount,
                tx_type,
                sender_one_sided,
                ..
            } => Some((
                TariAddress::default(), // Will be set by caller based on context
                recipient_address.clone(),
                *amount,
                *tx_type,
                *sender_one_sided,
            )),
            _ => None,
        }
    }

    /// Get transaction info details from TransactionInfo payment ID
    pub fn get_transaction_info_details(&self) -> Option<(TariAddress, MicroMinotari, TxType, bool)> {
        match &self.inner {
            InnerMemoField::TransactionInfo {
                recipient_address,
                amount,
                tx_type,
                sender_one_sided,
                ..
            } => Some((recipient_address.clone(), *amount, *tx_type, *sender_one_sided)),
            _ => None,
        }
    }
}

impl Display for MemoField {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.inner {
            InnerMemoField::Empty => write!(f, "None"),
            InnerMemoField::U256(v) => write!(f, "u256({v})"),
            InnerMemoField::Open { payment_id, tx_type } => {
                write!(f, "type({}), data({})", tx_type, MemoField::stringify_bytes(payment_id))
            },
            InnerMemoField::AddressAndData {
                sender_address,
                sender_one_sided,
                fee,
                tx_type,
                payment_id,
            } => write!(
                f,
                "sender_address({}), sender_one_sided({}), fee({}), type({}), data({})",
                sender_address.to_base58(),
                sender_one_sided,
                fee,
                tx_type,
                MemoField::stringify_bytes(payment_id)
            ),
            InnerMemoField::TransactionInfo {
                recipient_address,
                sender_one_sided,
                amount,
                fee,
                payment_id,
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
                MemoField::stringify_bytes(payment_id),
            ),
            InnerMemoField::Raw(bytes) => write!(f, "Raw({})", bytes.to_hex()),
        }
    }
}

impl BorshSerialize for MemoField {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        let bytes = self.to_bytes();
        writer.write_varint(bytes.len())?;
        for b in &bytes {
            BorshSerialize::serialize(&b, writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for MemoField {
    fn deserialize_reader<R>(reader: &mut R) -> Result<Self, io::Error>
    where R: io::Read {
        let len = reader.read_varint()?;
        if len > MAX_ENCRYPTED_DATA_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Larger than bytes".to_string(),
            ));
        }
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push(u8::deserialize_reader(reader)?);
        }
        let memo = MemoField::from_bytes(data.as_slice());
        Ok(memo)
    }
}

#[cfg(test)]
mod test {
    use tari_common_types::{
        tari_address::TariAddress,
        types::{CommitmentFactory, CompressedCommitment, FixedHash, PrivateKey},
    };
    use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};

    use super::*;
    use crate::{
        MicroMinotari,
        transaction_components::{
            EncryptedData,
            memo_field::{MemoField, TxType},
        },
    };

    fn create_random_fixed_hash() -> FixedHash {
        use rand::Rng;
        let mut bytes = [0u8; FixedHash::byte_size()];
        rand::rng().fill_bytes(&mut bytes);
        FixedHash::from(bytes)
    }

    #[allow(clippy::too_many_lines)]
    fn create_test_data_array() -> Vec<MemoField> {
        let mut pay_id_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        pay_id_address = pay_id_address
            .with_memo_field_payment_id(vec![0, 1, 2, 3, 4, 5])
            .unwrap();
        let sent_output_hashes = vec![create_random_fixed_hash()];
        vec![
            MemoField::new_empty(),
            MemoField::new_u256(1.into()),
            MemoField::new_u256(156486946518564u64.into()),
            MemoField::new_u256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            // Open - no data
            MemoField::new_open(vec![], TxType::PaymentToOther).unwrap(),
            // Open - some data
            MemoField::new_open(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], TxType::default()).unwrap(),
            // Open - max data
            MemoField::new_open(vec![1; 254], TxType::default()).unwrap(),
            // AddressAndData - dual, no data
            MemoField::new_address_and_data(
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
            MemoField::new_address_and_data(
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
            MemoField::new_address_and_data(
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
            MemoField::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![],
            )
            .unwrap(),
            // AddressAndData - single, some data
            MemoField::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            // AddressAndData - single, max data
            MemoField::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![1; 40],
            )
            .unwrap(),
            MemoField::new_address_and_data(
                pay_id_address.clone(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![1; 30],
            )
            .unwrap(),
            // TransactionInfo - single + amount, no data
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
        // We need to create a InnerMemoField::Open that, when serialized, will produce bytes that
        // will be parsed as InnerMemoField::TransactionInfo.
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
        // Craft payment id, when preceded by tx_type, will match the TransactionInfo structure
        let mut payment_id = Vec::new();
        // The first byte will be tx_type (0x03)
        // Next 7 bytes plus tx_type will form the amount (8 bytes total)
        let amount_value = 1000u64;
        let amount_bytes = amount_value.to_le_bytes();
        // Skip first byte since tx_type will take that place
        payment_id.extend_from_slice(&amount_bytes[1..]);
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
        payment_id.extend_from_slice(&meta_data);
        // Lastly, add the TariAddress
        payment_id.extend_from_slice(&fake_recipient_bytes);
        // Create our original InnerMemoField::Open
        let original_payment_id = MemoField::new_open(payment_id, tx_type).unwrap();
        // Serialize to bytes
        let bytes = original_payment_id.to_bytes();

        // Crucial insight: The key to preventing TariAddress parsing is to ensure
        // the first byte of our payload doesn't match the expected format for a TariAddress.
        // CoinSplit (0x03) should be different enough from a valid TariAddress start byte.
        // Parse back from bytes
        let parsed_payment_id = MemoField::from_bytes(&bytes);

        // If this assert passes, the attack failed
        assert_eq!(parsed_payment_id, original_payment_id);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn it_encrypts_and_decrypts_correctly() {
        for payment_id in create_test_data_array() {
            for (value, mask) in [
                (0, PrivateKey::default()),
                (0, PrivateKey::random(&mut rand::rng())),
                (123456, PrivateKey::default()),
                (654321, PrivateKey::random(&mut rand::rng())),
                (u64::MAX, PrivateKey::random(&mut rand::rng())),
            ] {
                let commitment = CompressedCommitment::from_commitment(
                    CommitmentFactory::default().commit(&mask, &PrivateKey::from(value)),
                );
                let encryption_key = PrivateKey::random(&mut rand::rng());
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
                (0, PrivateKey::random(&mut rand::rng())),
                (123456, PrivateKey::default()),
                (654321, PrivateKey::random(&mut rand::rng())),
                (u64::MAX, PrivateKey::random(&mut rand::rng())),
            ] {
                let commitment = CompressedCommitment::from_commitment(
                    CommitmentFactory::default().commit(&mask, &PrivateKey::from(value)),
                );
                let encryption_key = PrivateKey::random(&mut rand::rng());
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
            let payment_id = MemoField::new_open(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], tx_type).unwrap();
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = MemoField::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = MemoField::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                tx_type,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap();
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = MemoField::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = MemoField::new_transaction_info(
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
            let payment_id_from_bytes = MemoField::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);
        }
    }

    #[test]
    fn payment_id_display() {
        assert_eq!(MemoField::new_empty().to_string(), "None");
        assert_eq!(MemoField::new_u256(1235678.into()).to_string(), "u256(1235678)");
        assert_eq!(
            MemoField::new_u256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail")
            )
            .to_string(),
            "u256(465465489789785458694894263185648978947864164681631)"
        );
        assert_eq!(
            MemoField::new_open(
                vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64],
                TxType::CoinSplit
            )
            .unwrap()
            .to_string(),
            "type(CoinSplit), data(Hello World)"
        );
        assert_eq!(
            MemoField::new_address_and_data(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
        let payment_id_1 = MemoField::new_transaction_info(
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
        let payment_id_2 = MemoField::new_transaction_info(
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

        assert_eq!(payment_id_1, MemoField::from_bytes(&payment_id_1_bytes));
        assert_eq!(payment_id_2, MemoField::from_bytes(&payment_id_2_bytes));

        // Increase metadata fields to test 'to_bytes' overflow
        let payment_id_3 = MemoField::new_transaction_info(
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
        let payment_id_3_from_bytes = MemoField::from_bytes(&payment_id_3_bytes);
        assert_eq!(
            payment_id_3_from_bytes.to_string(),
            "recipient_address(f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb), \
            sender_one_sided(true), amount(18446744073709.551615 T), fee(0 µT), type(Coinbase), data(Hello World!!! 11-22-33)"
        );
    }

    #[test]
    fn it_gets_useable_payment_id_data() {
        let payment_id = MemoField::new_empty();
        assert_eq!("", MemoField::stringify_bytes(&payment_id.payment_id_as_bytes()));

        let payment_id = MemoField::new_u256(U256::from_dec_str("123456789").unwrap());
        assert_eq!(
            "123456789",
            U256::from_little_endian(&payment_id.payment_id_as_bytes()).to_string()
        );

        let payment_id = MemoField::new_address_and_data(
            TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            MicroMinotari::from(123),
            false,
            TxType::CoinSplit,
            "Hello World!!!".as_bytes().to_vec(),
        )
        .unwrap();
        assert_eq!(
            "Hello World!!!",
            MemoField::stringify_bytes(&payment_id.payment_id_as_bytes())
        );

        let payment_id = MemoField::new_transaction_info(
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
            MemoField::stringify_bytes(&payment_id.payment_id_as_bytes())
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
            .with_memo_field_payment_id(vec![0, 1, 2, 3, 4, 5])
            .unwrap();
        let payment_ids = vec![
            // AddressAndData - dual, no data
            MemoField::new_address_and_data(
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
            MemoField::new_address_and_data(
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
            MemoField::new_address_and_data(
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
            MemoField::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::CoinSplit,
                vec![],
            )
            .unwrap(),
            // AddressAndData - single, some data
            MemoField::new_address_and_data(
                TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                MicroMinotari::from(123),
                false,
                TxType::Burn,
                vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            )
            .unwrap(),
            MemoField::new_address_and_data(pay_id_address, MicroMinotari::from(123), false, TxType::Burn, vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            ])
            .unwrap(),
        ];
        fn old_to_bytes(payment_id: &MemoField) -> Vec<u8> {
            fn pack_meta_data(fee: MicroMinotari, tx_type: TxType, sender_one_side: bool) -> Vec<u8> {
                let mut bytes = Vec::with_capacity(5);
                // Zero out-of-bound values
                // - Use 4 bytes for 'fee', max value: 4,294,967,295
                let fee = if fee.as_u64() > u64::from(u32::MAX) {
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
            if let InnerMemoField::AddressAndData {
                sender_address,
                tx_type,
                sender_one_sided,
                fee,
                payment_id,
            } = &payment_id.inner
            {
                bytes.push(PTag::AddressAndDataV1 as u8);
                bytes.extend_from_slice(&0u64.to_le_bytes());
                bytes.extend_from_slice(&pack_meta_data(*fee, *tx_type, *sender_one_sided));
                bytes.extend_from_slice(&sender_address.to_vec());
                bytes.extend_from_slice(payment_id);
            };
            bytes
        }

        for payment_id in payment_ids {
            let bytes = old_to_bytes(&payment_id);
            let decoded = MemoField::from_bytes(&bytes);
            assert_eq!(decoded, payment_id);
        }
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn test_legacy_transaction_info() {
        let mut pay_id_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        pay_id_address = pay_id_address
            .with_memo_field_payment_id(vec![0, 1, 2, 3, 4, 5])
            .unwrap();
        let payment_ids = vec![
            // TransactionInfo - single + amount, no data
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
            MemoField::new_transaction_info(
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
        fn old_to_bytes(payment_id: &MemoField) -> Vec<u8> {
            let mut bytes = Vec::new();
            if let InnerMemoField::TransactionInfo {
                recipient_address,
                tx_type,
                sender_one_sided,
                amount,
                fee,
                payment_id,
                sent_output_hashes: _,
            } = &payment_id.inner
            {
                fn pack_meta_data(fee: MicroMinotari, tx_type: TxType, sender_one_side: bool) -> Vec<u8> {
                    let mut bytes = Vec::with_capacity(5);
                    // Zero out-of-bound values
                    // - Use 4 bytes for 'fee', max value: 4,294,967,295
                    let fee = if fee.as_u64() > u64::from(u32::MAX) {
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
                bytes.extend_from_slice(payment_id);
            };
            bytes
        }

        for payment_id in payment_ids {
            let bytes = old_to_bytes(&payment_id);
            let decoded = MemoField::from_bytes(&bytes);
            assert_eq!(decoded, payment_id);
        }
    }

    // This is a rare edge case where the first byte of the spend key, matches the correct checksum for a single
    // address.
    #[test]
    fn test_edge_case_with_tari_address() {
        let hex = "03404e9c30000000000000000a8000016c1b073261df680b5a95dbc8c559ed1eec8d31f66c90e9e2843d3376cb6142511299678d6494bd091405cd78b1b9cb8d1602b7d075f72dbf54fde4b89fbbe016ab34f142623015444b06f34f3f4f860c94";
        let bytes = hex::decode(hex).expect("Failed to decode hex");
        let payment_id = MemoField::from_bytes(&bytes);
        let address = match &payment_id.inner {
            InnerMemoField::AddressAndData { sender_address, .. } => sender_address.clone(),
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
        // Test U256 Memo validation
        let u256_value = U256::from(12345u64);
        let u256_payment_id = MemoField::new_u256(u256_value);
        assert_eq!(u256_payment_id.get_size(), 1 + SIZE_U256); // 1 + 32 = 33 bytes

        // Test Open Memo validation - valid case
        let small_payment_id = vec![1, 2, 3, 4, 5];
        let open_payment_id = MemoField::new_open(small_payment_id.clone(), TxType::PaymentToOther)
            .expect("Small Open Memo should be valid");
        assert_eq!(open_payment_id.get_size(), 1 + small_payment_id.len() + 1); // tag + data + tx_type

        // Test Open Memo validation - too large
        let large_payment_id = vec![0u8; MAX_ENCRYPTED_DATA_SIZE]; // 256 bytes
        let result = MemoField::new_open(large_payment_id, TxType::PaymentToOther);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Test Open Memo validation - maximum valid size
        let max_valid_open_data = vec![0u8; MAX_ENCRYPTED_DATA_SIZE - 2]; // 254 bytes (256 - 1 tag - 1 tx_type)
        let max_open_payment_id = MemoField::new_open(max_valid_open_data.clone(), TxType::PaymentToOther)
            .expect("Maximum valid Open Memo should be valid");
        assert_eq!(max_open_payment_id.get_size(), MAX_ENCRYPTED_DATA_SIZE);

        // Test Raw Memo validation - valid case
        let raw_data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let raw_payment_id = MemoField::new_raw(raw_data.clone()).expect("Small Raw Memo should be valid");
        assert_eq!(raw_payment_id.get_size(), 1 + raw_data.len()); // tag + data

        // Test Raw Memo validation - too large
        let large_raw_data = vec![0u8; MAX_ENCRYPTED_DATA_SIZE]; // 256 bytes
        let result = MemoField::new_raw(large_raw_data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Test Raw Memo validation - maximum valid size
        let max_valid_raw_data = vec![0u8; MAX_ENCRYPTED_DATA_SIZE - 1]; // 255 bytes (256 - 1 tag)
        let max_raw_payment_id =
            MemoField::new_raw(max_valid_raw_data.clone()).expect("Maximum valid Raw Memo should be valid");
        assert_eq!(max_raw_payment_id.get_size(), MAX_ENCRYPTED_DATA_SIZE);
    }

    #[test]
    fn test_address_and_data_validation() {
        // Create a test single address (smaller)
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Test AddressAndData with valid size
        let small_payment_id = vec![1, 2, 3, 4, 5];
        let fee = MicroMinotari::from(100u64);
        let address_and_data = MemoField::new_address_and_data(
            single_address.clone(),
            fee,
            false,
            TxType::PaymentToOther,
            small_payment_id.clone(),
        )
        .expect("Valid AddressAndData should be created");

        // Verify the size calculation
        let expected_size = MemoField::calculate_address_and_data_size(&single_address, small_payment_id.len());
        assert_eq!(address_and_data.get_size(), expected_size);
        assert!(address_and_data.get_size() <= MAX_ENCRYPTED_DATA_SIZE);

        // Test AddressAndData with user data that would exceed limit
        let large_payment_id = vec![0u8; MAX_ENCRYPTED_DATA_SIZE];
        let result = MemoField::new_address_and_data(
            single_address.clone(),
            fee,
            false,
            TxType::PaymentToOther,
            large_payment_id,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));
    }

    #[test]
    fn test_transaction_info_validation() {
        // Create a test single address
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Test TransactionInfo with valid size
        let small_payment_id = vec![1, 2, 3, 4, 5];
        let amount = MicroMinotari::from(1000u64);
        let fee = MicroMinotari::from(100u64);
        let sent_hashes = vec![create_random_fixed_hash(), create_random_fixed_hash()];

        let transaction_info = MemoField::new_transaction_info(
            single_address.clone(),
            amount,
            fee,
            false,
            TxType::PaymentToOther,
            sent_hashes.clone(),
            small_payment_id.clone(),
        )
        .expect("Valid TransactionInfo should be created");

        // Verify the size calculation
        let expected_size =
            MemoField::calculate_transaction_info_size(&single_address, sent_hashes.len(), small_payment_id.len());
        assert_eq!(transaction_info.get_size(), expected_size);
        assert!(transaction_info.get_size() <= MAX_ENCRYPTED_DATA_SIZE);

        // Test TransactionInfo with too many hashes
        let many_hashes = vec![create_random_fixed_hash(); 10]; // 10 * 32 = 320 bytes just for hashes
        let result = MemoField::new_transaction_info(
            single_address.clone(),
            amount,
            fee,
            false,
            TxType::PaymentToOther,
            many_hashes,
            small_payment_id,
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
            .with_memo_field_payment_id(vec![1, 2, 3, 4, 5])
            .unwrap();
        let address_payment_id = dual_address_with_payment_id.get_memo_field_payment_id_bytes();
        assert_eq!(address_payment_id, vec![1, 2, 3, 4, 5], "Payment ID data should match");

        // Test that we CAN create AddressAndData with an address that contains payment_id (recursion allowed)
        let result = MemoField::new_address_and_data(
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
        let memo = result.unwrap();
        let payment_id_data = memo.payment_id_as_bytes();
        assert_eq!(payment_id_data, vec![1, 2, 3], "Payment ID data should match");
        let sender_address = memo.get_sender_address().unwrap();
        assert_eq!(
            sender_address, dual_address_with_payment_id,
            "Recipient address should match"
        );

        // Test that we CAN create TransactionInfo with an address that contains payment_id (recursion allowed)
        let result = MemoField::new_transaction_info(
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
        let memo = result.unwrap();
        let payment_id_data = memo.payment_id_as_bytes();
        assert_eq!(payment_id_data, vec![1, 2, 3], "Payment ID data should match");
        let recipient_address = memo.get_recipient_address().unwrap();
        assert_eq!(
            recipient_address, dual_address_with_payment_id,
            "Recipient address should match"
        );

        // Test that validation still fails if the total size would exceed 256 bytes
        // Create an address with large payment ID data
        let mut large_dual_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        large_dual_address = large_dual_address
            .with_memo_field_payment_id(vec![0u8; 200]) // Large payload
            .unwrap();

        let result = MemoField::new_address_and_data(
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
        // Test that deeply nested Memos are correctly size-validated
        // Create a base dual address with small nested Memo
        let mut nested_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();

        // Add a small Memo to the address (level 1 nesting)
        nested_address = nested_address.with_memo_field_payment_id(vec![1, 2, 3, 4, 5]).unwrap();

        // Verify the nested address size includes the Memo data
        let nested_address_size = nested_address.get_size();
        assert!(
            nested_address_size > TARI_ADDRESS_INTERNAL_DUAL_SIZE,
            "Address with Memo should be larger than base dual address"
        );

        // Test creating AddressAndData with the nested address (level 2 nesting)
        let result = MemoField::new_address_and_data(
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
            total_size <= MAX_ENCRYPTED_DATA_SIZE,
            "Total nested Memo size should not exceed 256 bytes"
        );

        // Test creating TransactionInfo with nested address and verify size
        let result = MemoField::new_transaction_info(
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
            total_size <= MAX_ENCRYPTED_DATA_SIZE,
            "Total nested TransactionInfo size should not exceed 256 bytes"
        );

        // Test that validation fails when nested structure becomes too large
        // Create an address with larger Memo data
        let mut large_nested_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();

        // Add a large Memo that will make the total structure exceed 256 bytes
        large_nested_address = large_nested_address
            .with_memo_field_payment_id(vec![0u8; 180]) // Large nested data
            .unwrap();

        // This should fail because the total size exceeds 256 bytes
        let result = MemoField::new_address_and_data(
            large_nested_address.clone(),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![1, 2, 3, 4, 5], // Even small user data should fail
        );
        assert!(result.is_err(), "Should fail when total nested size exceeds 256 bytes");
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Verify the error shows the actual calculated size
        let calculated_size = MemoField::calculate_address_and_data_size(&large_nested_address, 5);
        assert!(
            calculated_size > MAX_ENCRYPTED_DATA_SIZE,
            "Calculated size should exceed the limit"
        );
    }

    #[test]
    fn test_open_from_string_validation() {
        // Test valid string
        let valid_string = "Hello World!";
        let open_payment_id = MemoField::new_open_from_string(valid_string, TxType::PaymentToOther)
            .expect("Valid string should create Open Memo");

        match &open_payment_id.inner {
            InnerMemoField::Open { payment_id, tx_type } => {
                assert_eq!(payment_id, valid_string.as_bytes());
                assert_eq!(*tx_type, TxType::PaymentToOther);
            },
            _ => panic!("Expected Open Memo"),
        }

        // Test string that would exceed size limit
        let large_string = "x".repeat(MAX_ENCRYPTED_DATA_SIZE); // 256 chars
        let result = MemoField::new_open_from_string(&large_string, TxType::PaymentToOther);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exceeds 256-byte limit"));

        // Test maximum valid string size
        let max_valid_string = "x".repeat(MAX_ENCRYPTED_DATA_SIZE - 2); // 254 chars (256 - 1 tag - 1 tx_type)
        let max_open_payment_id = MemoField::new_open_from_string(&max_valid_string, TxType::PaymentToOther)
            .expect("Maximum valid string should create Open Memo");
        assert_eq!(max_open_payment_id.get_size(), MAX_ENCRYPTED_DATA_SIZE);
    }

    #[test]
    fn test_padding_behavior() {
        // Create a test single address
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Test that small AddressAndData gets padded to PADDING_SIZE
        let small_payment_id = vec![1u8; 5];
        let address_and_data = MemoField::new_address_and_data(
            single_address.clone(),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            small_payment_id.clone(),
        )
        .expect("Valid AddressAndData should be created");

        let calculated_base_size =
            1 + 1 + single_address.get_size() + MemoField::SIZE_META_DATA + 1 + small_payment_id.len();
        assert!(calculated_base_size < PADDING_SIZE);
        assert_eq!(address_and_data.get_size(), PADDING_SIZE);

        // Test that small TransactionInfo gets padded to PADDING_SIZE
        let transaction_info = MemoField::new_transaction_info(
            single_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            small_payment_id.clone(),
        )
        .expect("Valid TransactionInfo should be created");

        let calculated_base_size =
            1 + 1 + single_address.get_size() + MemoField::SIZE_VALUE_AND_META_DATA + 1 + 1 + small_payment_id.len();
        assert!(calculated_base_size < PADDING_SIZE);
        assert_eq!(transaction_info.get_size(), PADDING_SIZE);
    }

    #[test]
    fn test_transaction_info_set_address_validation() {
        // Create a small dual address and transaction info
        let small_dual_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();
        let large_dual_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();

        let mut transaction_info = MemoField::new_transaction_info(
            small_dual_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            vec![1u8; 50], // Small payment ID
        )
        .expect("Should create transaction info");

        // Test setting valid address
        let result = transaction_info.transaction_info_set_address(small_dual_address.clone());
        assert!(result.is_ok(), "Setting valid address should succeed");

        // Test setting address that would exceed size limit
        // Create a large payment ID to get close to the limit
        let mut large_transaction_info = MemoField::new_transaction_info(
            small_dual_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            vec![1u8; 180], // Large payment ID to get close to limit
        )
        .expect("Should create transaction info");

        // Setting a larger address should fail
        let result = large_transaction_info.transaction_info_set_address(large_dual_address);
        assert!(result.is_err(), "Setting address that would exceed limit should fail");
        assert!(
            result.unwrap_err().contains("exceed"),
            "Error should mention exceeding limit"
        );

        // Test setting address on non-TransactionInfo memo field
        let mut empty_memo = MemoField::new_empty();
        let result = empty_memo.transaction_info_set_address(small_dual_address);
        assert!(result.is_err(), "Setting address on empty memo should fail");
        assert!(
            result.unwrap_err().contains("non-TransactionInfo"),
            "Error should mention wrong type"
        );
    }

    #[test]
    fn test_transaction_info_set_sent_output_hashes_validation() {
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        let mut transaction_info = MemoField::new_transaction_info(
            single_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            vec![1u8; 50], // Small payment ID
        )
        .expect("Should create transaction info");

        // Test setting valid number of hashes
        let few_hashes = vec![FixedHash::zero(); 2];
        let result = transaction_info.transaction_info_set_sent_output_hashes(few_hashes);
        assert!(result.is_ok(), "Setting valid hashes should succeed");

        // Test setting too many hashes that would exceed size limit
        // Each FixedHash is 32 bytes, so we need enough to exceed the limit
        let many_hashes = vec![FixedHash::zero(); 7]; // 7 * 32 = 224 bytes of hashes
        let result = transaction_info.transaction_info_set_sent_output_hashes(many_hashes);
        assert!(result.is_err(), "Setting too many hashes should fail");
        assert!(
            result.unwrap_err().contains("exceed"),
            "Error should mention exceeding limit"
        );

        // Test setting hashes on non-TransactionInfo memo field
        let mut empty_memo = MemoField::new_empty();
        let result = empty_memo.transaction_info_set_sent_output_hashes(vec![FixedHash::zero()]);
        assert!(result.is_err(), "Setting hashes on empty memo should fail");
        assert!(
            result.unwrap_err().contains("non-TransactionInfo"),
            "Error should mention wrong type"
        );
    }

    #[test]
    fn test_transaction_info_edge_cases_256_byte_limit() {
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        // Create transaction info that's just under the limit
        let mut transaction_info = MemoField::new_transaction_info(
            single_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![FixedHash::zero(); 4], // 4 * 32 = 128 bytes
            vec![1u8; 50],              // 50 bytes
        )
        .expect("Should create transaction info");

        // Test adding one more hash - should fail
        let mut current_hashes = transaction_info.get_sent_hashes().unwrap().clone();
        current_hashes.push(FixedHash::zero());
        let result = transaction_info.transaction_info_set_sent_output_hashes(current_hashes);
        assert!(result.is_err(), "Adding one more hash should exceed limit");

        // Test replacing with a larger address - should fail
        let dual_address = TariAddress::from_base58(
            "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
        )
        .unwrap();
        let result = transaction_info.transaction_info_set_address(dual_address);
        assert!(result.is_err(), "Setting larger address should exceed limit");
    }

    #[test]
    fn test_transaction_info_set_payment_id_validation() {
        let single_address = TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap();

        let mut transaction_info = MemoField::new_transaction_info(
            single_address.clone(),
            MicroMinotari::from(1000u64),
            MicroMinotari::from(100u64),
            false,
            TxType::PaymentToOther,
            vec![],
            vec![1u8; 50], // Small payment ID
        )
        .expect("Should create transaction info");

        // Test setting valid payment ID
        let small_payment_id = vec![1u8; 30];
        let result = transaction_info.transaction_info_set_payment_id(small_payment_id);
        assert!(result.is_ok(), "Setting valid payment ID should succeed");

        // Test setting payment ID that would exceed size limit
        // Need to account for: 1 tag + 1 sender_one_sided + 35 address + 13 metadata + 1 hash_count + 1 payment_id_len
        // = 52 bytes overhead So 256 - 52 = 204 bytes max for payment_id
        let large_payment_id = vec![1u8; 210]; // Large payment ID that will exceed limit
        let result = transaction_info.transaction_info_set_payment_id(large_payment_id);
        assert!(result.is_err(), "Setting large payment ID should fail");
        assert!(
            result.unwrap_err().contains("exceed"),
            "Error should mention exceeding limit"
        );

        // Test setting payment ID on non-TransactionInfo memo field
        let mut empty_memo = MemoField::new_empty();
        let result = empty_memo.transaction_info_set_payment_id(vec![1u8; 10]);
        assert!(result.is_err(), "Setting payment ID on empty memo should fail");
        assert!(
            result.unwrap_err().contains("non-TransactionInfo"),
            "Error should mention wrong type"
        );
    }
}
