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
    fmt,
    fmt::{Display, Formatter},
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
pub enum PaymentId {
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

    fn to_tag(&self) -> Vec<u8> {
        match self {
            PaymentId::Empty => vec![],
            PaymentId::U256(_) => vec![PTag::U256 as u8],
            PaymentId::Open { .. } => vec![PTag::Open as u8],
            PaymentId::AddressAndData { .. } => vec![PTag::AddressAndData as u8],
            PaymentId::TransactionInfo { .. } => vec![PTag::TransactionInfo as u8],
            PaymentId::Raw(_) => vec![PTag::Raw as u8],
        }
    }

    pub fn get_size(&self) -> usize {
        match self {
            PaymentId::Empty => 0,
            PaymentId::U256(_) => 1 + SIZE_U256,
            PaymentId::Open { user_data, .. } => 1 + user_data.len() + 1,
            PaymentId::AddressAndData {
                sender_address,
                user_data,
                ..
            } => {
                let len = 1 + 1 + sender_address.get_size() + PaymentId::SIZE_META_DATA + 1 + user_data.len();
                if len < PADDING_SIZE {
                    PADDING_SIZE
                } else {
                    len
                }
            },
            PaymentId::TransactionInfo {
                recipient_address,
                user_data,
                sent_output_hashes,
                ..
            } => {
                let len = 1 +
                    1 +
                    recipient_address.get_size() +
                    PaymentId::SIZE_VALUE_AND_META_DATA +
                    1 +
                    (sent_output_hashes.len() * FixedHash::byte_size()) +
                    1 +
                    user_data.len();
                if len < PADDING_SIZE {
                    PADDING_SIZE
                } else {
                    len
                }
            },
            PaymentId::Raw(bytes) => {
                // We add 1 for the tag byte
                1 + bytes.len()
            },
        }
    }

    pub fn get_fee(&self) -> Option<MicroMinotari> {
        match self {
            PaymentId::AddressAndData { fee, .. } | PaymentId::TransactionInfo { fee, .. } => Some(*fee),
            _ => None,
        }
    }

    pub fn get_sent_hashes(&self) -> Option<Vec<FixedHash>> {
        match self {
            PaymentId::TransactionInfo { sent_output_hashes, .. } => Some(sent_output_hashes.clone()),
            _ => None,
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
            PaymentId::TransactionInfo { tx_type, .. } => *tx_type,
            _ => TxType::default(),
        }
    }

    /// Helper function to set the 'recipient_address' of a 'PaymentId::TransactionInfo'
    pub fn transaction_info_set_address(&mut self, address: TariAddress) {
        if let PaymentId::TransactionInfo { recipient_address, .. } = self {
            *recipient_address = address
        }
    }

    pub fn transaction_info_set_sent_output_hashes(&mut self, sent_output_hashes: Vec<FixedHash>) {
        if let PaymentId::TransactionInfo {
            sent_output_hashes: hashes,
            ..
        } = self
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
        match self {
            PaymentId::Open { user_data, tx_type } => PaymentId::AddressAndData {
                sender_address,
                sender_one_sided,
                fee,
                tx_type,
                user_data,
            },
            PaymentId::Empty => PaymentId::AddressAndData {
                sender_address,
                sender_one_sided,
                fee,
                tx_type: tx_type.unwrap_or_default(),
                user_data: vec![],
            },
            _ => self,
        }
    }

    // This method is infallible; any out-of-bound values will be zeroed.
    fn pack_meta_data(&self) -> Vec<u8> {
        match self {
            PaymentId::TransactionInfo {
                fee,
                sender_one_sided,
                tx_type,
                ..
            } |
            PaymentId::AddressAndData {
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
        match &self {
            PaymentId::Empty => vec![],
            PaymentId::U256(v) => {
                let bytes: &mut [u8] = &mut [0; SIZE_U256];
                v.to_little_endian(bytes);
                bytes.to_vec()
            },
            PaymentId::Open { user_data, .. } => user_data.clone(),
            PaymentId::AddressAndData { user_data, .. } => user_data.clone(),
            PaymentId::TransactionInfo { user_data, .. } => user_data.clone(),
            PaymentId::Raw(bytes) => bytes.clone(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            PaymentId::Empty => Vec::new(),
            PaymentId::U256(v) => {
                let mut bytes = self.to_tag();
                let mut value = vec![0; 32];
                v.to_little_endian(&mut value);
                bytes.extend_from_slice(&value);
                bytes
            },
            PaymentId::Open { user_data, tx_type } => {
                let mut bytes = self.to_tag();
                bytes.extend_from_slice(&tx_type.as_bytes());
                bytes.extend_from_slice(user_data);
                bytes
            },
            PaymentId::AddressAndData {
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
            PaymentId::TransactionInfo {
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
            PaymentId::Raw(bytes) => {
                let mut result = self.to_tag();
                result.extend_from_slice(bytes);
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
                return PaymentId::Open {
                    tx_type: TxType::PaymentToOther,
                    user_data: bytes.to_vec(),
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
            PTag::Empty => return PaymentId::Empty,
            PTag::U256 => {
                if bytes.len() != SIZE_U256 {
                    return PaymentId::Open {
                        tx_type: TxType::from_u8(*bytes.first().unwrap_or(&0)),
                        user_data: bytes.get(1..).unwrap_or_default().to_vec(),
                    };
                }
                let v = U256::from_little_endian(bytes);
                return PaymentId::U256(v);
            },
            PTag::Open => {
                return PaymentId::Open {
                    tx_type: TxType::from_u8(*bytes.first().unwrap_or(&0)),
                    user_data: bytes.get(1..).unwrap_or_default().to_vec(),
                }
            },
            PTag::Raw => return PaymentId::Raw(raw_bytes),
            _ => {},
        }

        match PaymentId::try_deserialize_address_or_transaction_data(bytes, p_tag) {
            Ok(payment_id) => payment_id,
            Err(e) => {
                debug!("Failed to parse PaymentId from bytes: {}, returning Raw", e);
                PaymentId::Raw(raw_bytes)
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
                return Ok(PaymentId::AddressAndData {
                    sender_address: address,
                    sender_one_sided,
                    fee,
                    tx_type: tx_meta_data,
                    user_data,
                });
            }

            // legacy support for TransactionInfoV1
            if p_tag == PTag::TransactionInfoV1 {
                let user_data = bytes[PaymentId::SIZE_VALUE_AND_META_DATA + size..].to_vec();
                return Ok(PaymentId::TransactionInfo {
                    recipient_address: address,
                    sender_one_sided,
                    amount,
                    fee,
                    tx_type: tx_meta_data,
                    user_data,
                    sent_output_hashes: vec![],
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
            return Ok(PaymentId::AddressAndData {
                sender_address: address,
                sender_one_sided,
                fee,
                tx_type: tx_meta_data,
                user_data: user_data.to_vec(),
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
        Ok(PaymentId::TransactionInfo {
            recipient_address: address,
            sender_one_sided,
            amount,
            fee,
            tx_type: tx_meta_data,
            user_data: user_data.to_vec(),
            sent_output_hashes,
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
        match self {
            PaymentId::Empty => self.to_string(),
            PaymentId::U256(v) => format!("{}", v),
            PaymentId::Open { user_data, .. } => PaymentId::stringify_bytes(user_data),
            PaymentId::AddressAndData { user_data, .. } => PaymentId::stringify_bytes(user_data),
            PaymentId::TransactionInfo { user_data, .. } => PaymentId::stringify_bytes(user_data),
            PaymentId::Raw(bytes) => bytes.to_hex(),
        }
    }

    /// Helper function to create a `PaymentId::Open` from a string and the transaction type
    pub fn open_from_string(s: &str, tx_type: TxType) -> Self {
        PaymentId::Open {
            user_data: s.as_bytes().to_vec(),
            tx_type,
        }
    }

    /// Helper function to create a `PaymentId::Open` from a bytes and the transaction type
    pub fn open(bytes: Vec<u8>, tx_type: TxType) -> Self {
        PaymentId::Open {
            user_data: bytes,
            tx_type,
        }
    }
}

impl Display for PaymentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PaymentId::Empty => write!(f, "None"),
            PaymentId::U256(v) => write!(f, "u256({v})"),
            PaymentId::Open { user_data, tx_type } => {
                write!(f, "type({}), data({})", tx_type, PaymentId::stringify_bytes(user_data))
            },
            PaymentId::AddressAndData {
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
            PaymentId::TransactionInfo {
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
            PaymentId::Raw(bytes) => write!(f, "Raw({})", bytes.to_hex()),
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
        pay_id_address = pay_id_address
            .with_payment_id_user_data(vec![0, 1, 2, 3, 4, 5])
            .unwrap();
        let sent_output_hashes = vec![create_random_fixed_hash()];
        vec![
            PaymentId::Empty,
            PaymentId::U256(1.into()),
            PaymentId::U256(156486946518564u64.into()),
            PaymentId::U256(
                U256::from_dec_str("465465489789785458694894263185648978947864164681631").expect("Should not fail"),
            ),
            // Open - no data
            PaymentId::Open {
                user_data: vec![],
                tx_type: TxType::default(),
            },
            // Open - some data
            PaymentId::Open {
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                tx_type: TxType::default(),
            },
            // Open - max data
            PaymentId::Open {
                user_data: vec![1; 254],
                tx_type: TxType::default(),
            },
            // AddressAndData - dual, no data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::PaymentToSelf,
                user_data: vec![],
            },
            // // AddressAndData - dual, some data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::PaymentToOther,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            // AddressAndData - dual,
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                fee: MicroMinotari::from(123),
                sender_one_sided: false,
                tx_type: TxType::PaymentToSelf,
                user_data: vec![1; 80],
            },
            // AddressAndData - single, no data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinSplit,
                user_data: vec![],
            },
            // AddressAndData - single, some data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            // AddressAndData - single, max data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinSplit,
                user_data: vec![1; 100],
            },
            PaymentId::AddressAndData {
                sender_address: pay_id_address.clone(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinSplit,
                user_data: vec![1; 80],
            },
            // TransactionInfo - single + amount, no data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinJoin,
                user_data: vec![],
                sent_output_hashes: vec![],
            },
            // TransactionInfo - single + amount + some data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::ValidatorNodeRegistration,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: sent_output_hashes.clone(),
            },
            // TransactionInfo - dual + amount, no dta
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: true,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinSplit,
                user_data: vec![],
                sent_output_hashes: sent_output_hashes.clone(),
            },
            // TransactionInfo - dual + amount + some data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: sent_output_hashes.clone(),
            },
            PaymentId::TransactionInfo {
                recipient_address: pay_id_address,
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: sent_output_hashes.clone(),
            },
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
        let original_payment_id = PaymentId::Open {
            tx_type, // This will be the first byte (0x03 for CoinSplit)
            user_data,
        };
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
            let payment_id = PaymentId::Open {
                tx_type,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            };
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);

            let payment_id = PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                tx_type,
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
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
                tx_type,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: vec![create_random_fixed_hash()],
            };
            let payment_id_bytes = payment_id.to_bytes();
            let payment_id_from_bytes = PaymentId::from_bytes(&payment_id_bytes);
            assert_eq!(payment_id, payment_id_from_bytes);
        }
    }

    #[test]
    fn payment_id_display() {
        assert_eq!(PaymentId::Empty.to_string(), "None");
        assert_eq!(PaymentId::U256(1235678.into()).to_string(), "u256(1235678)");
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
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                user_data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64]
            }
            .to_string(),
            "sender_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), fee(123 µT), \
             type(HtlcAtomicSwapRefund), data(Hello World)"
        );
        assert_eq!(
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x57, 0x6f, 0x72, 0x6c, 0x64],
                sent_output_hashes: vec![],
            }
            .to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(false), \
             amount(123456 µT), fee(123 µT), type(Burn), data(Hello World)"
        );
        assert_eq!(
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: true,
                amount: MicroMinotari::from(1234),
                fee: MicroMinotari::from(123),
                tx_type: TxType::ValidatorNodeRegistration,
                user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
                sent_output_hashes: vec![],
            }
            .to_string(),
            "recipient_address(f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk), sender_one_sided(true), amount(1234 \
             µT), fee(123 µT), type(ValidatorNodeRegistration), data(Hello World!!! 11-22-33)"
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
            tx_type: TxType::PaymentToOther,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
            sent_output_hashes: vec![create_random_fixed_hash()],
        };
        let payment_id_2 = PaymentId::TransactionInfo {
            recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            sender_one_sided: false,
            amount: MicroMinotari::from(u64::MAX),
            fee: MicroMinotari::from(4_294_967_295),
            tx_type: TxType::PaymentToSelf,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
            sent_output_hashes: vec![create_random_fixed_hash()],
        };

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
        let payment_id_3 = PaymentId::TransactionInfo {
            recipient_address: TariAddress::from_base58(
                "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
            )
            .unwrap(),
            sender_one_sided: true,
            amount: MicroMinotari::from(u64::MAX),
            fee: MicroMinotari::from(4_294_967_295 + 100), // 4294.967395 T
            tx_type: TxType::Coinbase,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
            sent_output_hashes: vec![create_random_fixed_hash()],
        };
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
        let payment_id = PaymentId::Empty;
        assert_eq!("", PaymentId::stringify_bytes(&payment_id.user_data_as_bytes()));

        let payment_id = PaymentId::U256(U256::from_dec_str("123456789").unwrap());
        assert_eq!(
            "123456789",
            U256::from_little_endian(&payment_id.user_data_as_bytes()).to_string()
        );

        let payment_id = PaymentId::AddressAndData {
            sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
            tx_type: TxType::CoinSplit,
            sender_one_sided: false,
            fee: MicroMinotari::from(123),
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
            tx_type: TxType::PaymentToOther,
            user_data: "Hello World!!! 11-22-33".as_bytes().to_vec(),
            sent_output_hashes: vec![create_random_fixed_hash()],
        };
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
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::PaymentToSelf,
                user_data: vec![],
            },
            // // AddressAndData - dual, some data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::PaymentToOther,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            // AddressAndData - dual,
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                fee: MicroMinotari::from(123),
                sender_one_sided: false,
                tx_type: TxType::PaymentToSelf,
                user_data: vec![1; 80],
            },
            // AddressAndData - single, no data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinSplit,
                user_data: vec![],
            },
            // AddressAndData - single, some data
            PaymentId::AddressAndData {
                sender_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            PaymentId::AddressAndData {
                sender_address: pay_id_address,
                sender_one_sided: false,
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
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
            if let PaymentId::AddressAndData {
                sender_address,
                tx_type,
                sender_one_sided,
                fee,
                user_data,
            } = payment_id
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
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinJoin,
                user_data: vec![],
                sent_output_hashes: vec![],
            },
            // TransactionInfo - single + amount + some data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58("f3S7XTiyKQauZpDUjdR8NbcQ33MYJigiWiS44ccZCxwAAjk").unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::ValidatorNodeRegistration,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: vec![],
            },
            // TransactionInfo - dual + amount, no dta
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: true,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::CoinSplit,
                user_data: vec![],
                sent_output_hashes: vec![],
            },
            // TransactionInfo - dual + amount + some data
            PaymentId::TransactionInfo {
                recipient_address: TariAddress::from_base58(
                    "f425UWsDp714RiN53c1G6ek57rfFnotB5NCMyrn4iDgbR8i2sXVHa4xSsedd66o9KmkRgErQnyDdCaAdNLzcKrj7eUb",
                )
                .unwrap(),
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: vec![],
            },
            PaymentId::TransactionInfo {
                recipient_address: pay_id_address,
                sender_one_sided: false,
                amount: MicroMinotari::from(123456),
                fee: MicroMinotari::from(123),
                tx_type: TxType::Burn,
                user_data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
                sent_output_hashes: vec![],
            },
        ];
        fn old_to_bytes(payment_id: &PaymentId) -> Vec<u8> {
            let mut bytes = Vec::new();
            if let PaymentId::TransactionInfo {
                recipient_address,
                tx_type,
                sender_one_sided,
                amount,
                fee,
                user_data,
                sent_output_hashes: _,
            } = payment_id
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
        let address = match payment_id {
            PaymentId::AddressAndData { sender_address, .. } => sender_address,
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
}
