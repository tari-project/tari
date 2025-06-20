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

use primitive_types::U256;
use serde::{Deserialize, Serialize};
use crate::data_structures::types::MicroMinotari;
use crate::errors::{LightweightWalletError, DataStructureError};
use crate::hex_utils::{HexEncodable, HexValidatable, HexError};
use borsh::{BorshSerialize, BorshDeserialize};
use hex::ToHex;

/// Transaction type enumeration
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum TxType {
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
        match value {
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
            _ => TxType::PaymentToOther,
        }
    }

    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

impl Default for TxType {
    fn default() -> Self {
        TxType::PaymentToOther
    }
}

/// Payment ID for identifying payments in encrypted data
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentId {
    /// Empty payment ID
    Empty,
    /// U256 payment ID
    U256 { value: U256 },
    /// Open payment ID (public)
    Open { data: Vec<u8> },
    /// Address and data payment ID
    AddressAndData {
        /// Address bytes
        address: Vec<u8>,
        /// Data bytes
        data: Vec<u8>,
    },
    /// Transaction info payment ID
    TransactionInfo {
        /// Transaction ID
        tx_id: Vec<u8>,
        /// Output index
        output_index: u64,
    },
    /// Raw payment ID
    Raw { data: Vec<u8> },
}

// Helper module for U256 serialization
mod u256_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(value: &U256, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = [0u8; 32];
        value.to_big_endian(&mut bytes);
        serde::Serialize::serialize(&hex::encode(&bytes), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<U256, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = <String as serde::Deserialize>::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Expected 32 bytes for U256"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(U256::from_big_endian(&arr))
    }
}

impl<'de> Deserialize<'de> for PaymentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum PaymentIdHelper {
            Empty,
            U256 { value: U256 },
            Open { data: Vec<u8> },
            AddressAndData { address: Vec<u8>, data: Vec<u8> },
            TransactionInfo { tx_id: Vec<u8>, output_index: u64 },
            Raw { data: Vec<u8> },
        }

        let helper = PaymentIdHelper::deserialize(deserializer)?;
        match helper {
            PaymentIdHelper::Empty => Ok(PaymentId::Empty),
            PaymentIdHelper::U256 { value } => Ok(PaymentId::U256 { value }),
            PaymentIdHelper::Open { data } => Ok(PaymentId::Open { data }),
            PaymentIdHelper::AddressAndData { address, data } => {
                Ok(PaymentId::AddressAndData { address, data })
            }
            PaymentIdHelper::TransactionInfo { tx_id, output_index } => {
                Ok(PaymentId::TransactionInfo { tx_id, output_index })
            }
            PaymentIdHelper::Raw { data } => Ok(PaymentId::Raw { data }),
        }
    }
}

// Helper module for hex serialization of [u8; 32]
mod hex_serde_array_32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_string = hex::encode(value);
        serializer.serialize_str(&hex_string)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_string = String::deserialize(deserializer)?;
        let bytes = hex::decode(&hex_string).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Expected 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

impl BorshSerialize for PaymentId {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            PaymentId::Empty => {
                borsh::BorshSerialize::serialize(&0u8, writer)?;
            }
            PaymentId::U256 { value } => {
                borsh::BorshSerialize::serialize(&1u8, writer)?;
                let mut bytes = [0u8; 32];
                value.to_big_endian(&mut bytes);
                borsh::BorshSerialize::serialize(&bytes, writer)?;
            }
            PaymentId::Open { data } => {
                borsh::BorshSerialize::serialize(&2u8, writer)?;
                borsh::BorshSerialize::serialize(data, writer)?;
            }
            PaymentId::AddressAndData { address, data } => {
                borsh::BorshSerialize::serialize(&3u8, writer)?;
                borsh::BorshSerialize::serialize(address, writer)?;
                borsh::BorshSerialize::serialize(data, writer)?;
            }
            PaymentId::TransactionInfo { tx_id, output_index } => {
                borsh::BorshSerialize::serialize(&4u8, writer)?;
                borsh::BorshSerialize::serialize(tx_id, writer)?;
                borsh::BorshSerialize::serialize(output_index, writer)?;
            }
            PaymentId::Raw { data } => {
                borsh::BorshSerialize::serialize(&5u8, writer)?;
                borsh::BorshSerialize::serialize(data, writer)?;
            }
        }
        Ok(())
    }
}

impl BorshDeserialize for PaymentId {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let tag: u8 = borsh::BorshDeserialize::deserialize_reader(reader)?;
        match tag {
            0 => Ok(PaymentId::Empty),
            1 => {
                let bytes: [u8; 32] = borsh::BorshDeserialize::deserialize_reader(reader)?;
                let value = U256::from_big_endian(&bytes);
                Ok(PaymentId::U256 { value })
            }
            2 => {
                let data: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
                Ok(PaymentId::Open { data })
            }
            3 => {
                let address: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
                let data: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
                Ok(PaymentId::AddressAndData { address, data })
            }
            4 => {
                let tx_id: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
                let output_index: u64 = borsh::BorshDeserialize::deserialize_reader(reader)?;
                Ok(PaymentId::TransactionInfo { tx_id, output_index })
            }
            5 => {
                let data: Vec<u8> = borsh::BorshDeserialize::deserialize_reader(reader)?;
                Ok(PaymentId::Raw { data })
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid PaymentId tag",
            )),
        }
    }
}

impl PaymentId {
    const SIZE_META_DATA: usize = 5;
    const SIZE_VALUE: usize = crate::data_structures::encrypted_data::SIZE_VALUE;
    const SIZE_VALUE_AND_META_DATA: usize = Self::SIZE_VALUE + Self::SIZE_META_DATA;
    const PADDING_SIZE: usize = 32;

    /// Get the size of this payment ID in bytes
    pub fn get_size(&self) -> usize {
        match self {
            PaymentId::Empty => 0,
            PaymentId::U256 { .. } => 1 + 32, // 1 byte tag + 32 bytes for U256
            PaymentId::Open { data } => 1 + data.len() + 1,
            PaymentId::AddressAndData {
                address,
                data,
                ..
            } => {
                1 + 1 + address.len() + 1 + data.len()
            },
            PaymentId::TransactionInfo {
                tx_id: _,
                output_index: _,
            } => {
                1 + 32 + 8 // tag + 32-byte tx_id + 8-byte output_index
            },
            PaymentId::Raw { data } => {
                // We add 1 for the tag byte
                1 + data.len()
            },
        }
    }

    /// Get the fee from this payment ID if available
    pub fn get_fee(&self) -> Option<MicroMinotari> {
        match self {
            PaymentId::AddressAndData { .. } | PaymentId::TransactionInfo { .. } => None,
            _ => None,
        }
    }

    /// Get the transaction type from this payment ID
    pub fn get_type(&self) -> TxType {
        match self {
            PaymentId::Open { .. } |
            PaymentId::AddressAndData { .. } |
            PaymentId::TransactionInfo { .. } => TxType::default(),
            _ => TxType::default(),
        }
    }

    /// Convert payment ID to bytes for serialization
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        match self {
            PaymentId::Empty => {
                // No tag needed for empty
            },
            PaymentId::U256 { value } => {
                bytes.push(1); // Tag for U256
                let mut bytes_array = [0u8; 32];
                value.to_big_endian(&mut bytes_array);
                bytes.extend_from_slice(&bytes_array);
            },
            PaymentId::Open { data } => {
                bytes.push(2); // Tag for Open
                bytes.push(data.len() as u8);
                bytes.extend_from_slice(data);
            },
            PaymentId::AddressAndData {
                address,
                data,
            } => {
                bytes.push(5); // Tag for AddressAndData
                bytes.push(address.len() as u8);
                bytes.extend_from_slice(address);
                bytes.push(data.len() as u8); // Add data length
                bytes.extend_from_slice(data);
            },
            PaymentId::TransactionInfo {
                tx_id,
                output_index,
            } => {
                bytes.push(6); // Tag for TransactionInfo
                bytes.extend_from_slice(tx_id);
                bytes.extend_from_slice(&output_index.to_le_bytes());
            },
            PaymentId::Raw { data } => {
                bytes.push(7); // Tag for Raw
                bytes.extend_from_slice(data);
            },
        }
        
        bytes
    }

    /// Create payment ID from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError> {
        if bytes.is_empty() {
            return Ok(PaymentId::Empty);
        }

        let tag = bytes[0];
        let data = &bytes[1..];

        match tag {
            0 => Ok(PaymentId::Empty),
            1 => {
                if data.len() != 32 {
                    return Err(DataStructureError::InvalidPaymentId("U256 payment ID must be 32 bytes".to_string()).into());
                }
                let mut value_bytes = [0u8; 32];
                value_bytes.copy_from_slice(data);
                Ok(PaymentId::U256 { value: U256::from_big_endian(&value_bytes) })
            },
            2 => {
                if data.len() < 2 {
                    return Err(DataStructureError::InvalidPaymentId("Open payment ID data too short".to_string()).into());
                }
                let user_data_len = data[0] as usize;
                if data.len() < 1 + user_data_len {
                    return Err(DataStructureError::InvalidPaymentId("Open payment ID data too short".to_string()).into());
                }
                let user_data = data[1..1 + user_data_len].to_vec();
                Ok(PaymentId::Open { data: user_data })
            },
            5 => {
                if data.len() < 2 {
                    return Err(DataStructureError::InvalidPaymentId("AddressAndData payment ID data too short".to_string()).into());
                }
                let addr_len = data[0] as usize;
                if data.len() < 1 + addr_len + 1 {
                    return Err(DataStructureError::InvalidPaymentId("AddressAndData payment ID data too short".to_string()).into());
                }
                let address = data[1..1 + addr_len].to_vec();
                let data_len = data[1 + addr_len] as usize;
                if data.len() < 1 + addr_len + 1 + data_len {
                    return Err(DataStructureError::InvalidPaymentId("AddressAndData payment ID data too short".to_string()).into());
                }
                let data = data[1 + addr_len + 1..1 + addr_len + 1 + data_len].to_vec();
                Ok(PaymentId::AddressAndData {
                    address,
                    data,
                })
            },
            6 => {
                if data.len() < 40 {
                    return Err(DataStructureError::InvalidPaymentId("TransactionInfo payment ID data too short".to_string()).into());
                }
                let tx_id = data[0..32].to_vec();
                let output_index = u64::from_le_bytes(data[32..40].try_into().unwrap());
                Ok(PaymentId::TransactionInfo {
                    tx_id,
                    output_index,
                })
            },
            7 => Ok(PaymentId::Raw { data: data.to_vec() }),
            _ => Ok(PaymentId::Raw { data: bytes.to_vec() }),
        }
    }
}

impl HexEncodable for PaymentId {
    fn to_hex(&self) -> String {
        match self {
            PaymentId::Empty => String::new(),
            PaymentId::U256 { value } => {
                let mut bytes = [0u8; 32];
                value.to_big_endian(&mut bytes);
                format!("{:064x}", U256::from_big_endian(&bytes))
            },
            PaymentId::Open { data } => data.encode_hex::<String>(),
            PaymentId::AddressAndData { address, data } => {
                format!("{}{}", hex::encode(address), data.encode_hex::<String>())
            }
            PaymentId::TransactionInfo { tx_id, output_index } => {
                format!("{}{:08x}", hex::encode(tx_id), output_index)
            }
            PaymentId::Raw { data } => data.encode_hex(),
        }
    }
    
    fn from_hex(hex: &str) -> Result<Self, HexError> {
        if hex.is_empty() {
            return Ok(PaymentId::Empty);
        }
        
        // Try to parse as U256 first (64 hex chars)
        if hex.len() == 64 {
            if let Ok(value) = U256::from_str_radix(hex, 16) {
                return Ok(PaymentId::U256 { value });
            }
        }
        
        // Try to parse as raw data
        if let Ok(data) = hex::decode(hex) {
            return Ok(PaymentId::Raw { data });
        }
        
        Err(HexError::InvalidHex("Could not parse payment ID from hex".to_string()))
    }
}

impl HexValidatable for PaymentId {} 