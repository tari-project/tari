// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Address handling utilities for lightweight wallets
//!
//! This module provides functionality to load and parse Tari addresses
//! from various formats including base58, hex, and emoji.
//! 
//! This implementation follows the exact specification from the core Tari implementation
//! in base_layer/common_types/src/tari_address/

use crate::errors::{LightweightWalletError, DataStructureError};
use crate::hex_utils::{HexEncodable};
use crate::data_structures::types::{CompressedPublicKey, };
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;

// Address size constants (from source of truth)
const TARI_ADDRESS_INTERNAL_DUAL_SIZE: usize = 67;
const TARI_ADDRESS_INTERNAL_SINGLE_SIZE: usize = 35;
const INTERNAL_DUAL_BASE58_MIN_SIZE: usize = 89;
const INTERNAL_DUAL_BASE58_MAX_SIZE: usize = 443;
const INTERNAL_SINGLE_MIN_BASE58_SIZE: usize = 45;
const INTERNAL_SINGLE_MAX_BASE58_SIZE: usize = 48;
const MAX_ENCRYPTED_DATA_SIZE: usize = 256;

/// The correct Tari emoji set (exactly as in source of truth)
pub const EMOJI: [char; 256] = [
    '🐢', '📟', '🌈', '🌊', '🎯', '🐋', '🌙', '🤔', '🌕', '⭐', '🎋', '🌰', '🌴', '🌵', '🌲', '🌸', '🌹', '🌻', '🌽',
    '🍀', '🍁', '🍄', '🥑', '🍆', '🍇', '🍈', '🍉', '🍊', '🍋', '🍌', '🍍', '🍎', '🍐', '🍑', '🍒', '🍓', '🍔', '🍕',
    '🍗', '🍚', '🍞', '🍟', '🥝', '🍣', '🍦', '🍩', '🍪', '🍫', '🍬', '🍭', '🍯', '🥐', '🍳', '🥄', '🍵', '🍶', '🍷',
    '🍸', '🍾', '🍺', '🍼', '🎀', '🎁', '🎂', '🎃', '🤖', '🎈', '🎉', '🎒', '🎓', '🎠', '🎡', '🎢', '🎣', '🎤', '🎥',
    '🎧', '🎨', '🎩', '🎪', '🎬', '🎭', '🎮', '🎰', '🎱', '🎲', '🎳', '🎵', '🎷', '🎸', '🎹', '🎺', '🎻', '🎼', '🎽',
    '🎾', '🎿', '🏀', '🏁', '🏆', '🏈', '⚽', '🏠', '🏥', '🏦', '🏭', '🏰', '🐀', '🐉', '🐊', '🐌', '🐍', '🦁', '🐐',
    '🐑', '🐔', '🙈', '🐗', '🐘', '🐙', '🐚', '🐛', '🐜', '🐝', '🐞', '🦋', '🐣', '🐨', '🦀', '🐪', '🐬', '🐭', '🐮',
    '🐯', '🐰', '🦆', '🦂', '🐴', '🐵', '🐶', '🐷', '🐸', '🐺', '🐻', '🐼', '🐽', '🐾', '👀', '👅', '👑', '👒', '🧢',
    '💅', '👕', '👖', '👗', '👘', '👙', '💃', '👛', '👞', '👟', '👠', '🥊', '👢', '👣', '🤡', '👻', '👽', '👾', '🤠',
    '👃', '💄', '💈', '💉', '💊', '💋', '👂', '💍', '💎', '💐', '💔', '🔒', '🧩', '💡', '💣', '💤', '💦', '💨', '💩',
    '➕', '💯', '💰', '💳', '💵', '💺', '💻', '💼', '📈', '📜', '📌', '📎', '📖', '📿', '📡', '⏰', '📱', '📷', '🔋',
    '🔌', '🚰', '🔑', '🔔', '🔥', '🔦', '🔧', '🔨', '🔩', '🔪', '🔫', '🔬', '🔭', '🔮', '🔱', '🗽', '😂', '😇', '😈',
    '🤑', '😍', '😎', '😱', '😷', '🤢', '👍', '👶', '🚀', '🚁', '🚂', '🚚', '🚑', '🚒', '🚓', '🛵', '🚗', '🚜', '🚢',
    '🚦', '🚧', '🚨', '🚪', '🚫', '🚲', '🚽', '🚿', '🧲',
];

// Create reverse emoji mapping for parsing emoji addresses
lazy_static::lazy_static! {
    static ref REVERSE_EMOJI: HashMap<char, u8> = {
        let mut m = HashMap::with_capacity(256);
        EMOJI.iter().enumerate().for_each(|(i, c)| {
            m.insert(*c, i as u8);
        });
        m
    };
}

// DammSum checksum functions (ported from core exactly)
const COEFFICIENTS: [u8; 3] = [4, 3, 1];

fn compute_mask() -> u8 {
    let mut mask = 1u8;
    for bit in COEFFICIENTS {
        let shift = 1u8.checked_shl(u32::from(bit)).unwrap();
        mask = mask.checked_add(shift).unwrap();
    }
    mask
}

/// Compute the DammSum checksum for a byte slice
pub fn compute_checksum(data: &[u8]) -> u8 {
    let mask = compute_mask();
    let mut result = 0u8;

    for digit in data {
        result ^= *digit; // add
        let overflow = (result & (1 << 7)) != 0;
        result <<= 1; // double
        if overflow {
            // reduce
            result ^= mask;
        }
    }

    result
}

/// Validate that a byte slice has a valid DammSum checksum
pub fn validate_checksum(data: &[u8]) -> Result<&[u8], LightweightWalletError> {
    if data.is_empty() {
        return Err(DataStructureError::InvalidChecksum("Empty data".to_string()).into());
    }
    
    let (data_part, checksum) = data.split_at(data.len() - 1);
    let expected_checksum = compute_checksum(data_part);
    
    if checksum[0] == expected_checksum {
        Ok(data_part)
    } else {
        Err(DataStructureError::InvalidChecksum(format!(
            "Expected checksum {}, got {}", expected_checksum, checksum[0]
        )).into())
    }
}

/// Tari Network types (exact values from source of truth)
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Eq, Copy, Serialize, Deserialize)]
pub enum Network {
    MainNet = 0x00,
    StageNet = 0x01, 
    NextNet = 0x02,
    LocalNet = 0x10,
    Igor = 0x24,
    Esmeralda = 0x26,
}

impl Network {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn as_key_str(self) -> &'static str {
        match self {
            Network::MainNet => "mainnet",
            Network::StageNet => "stagenet", 
            Network::NextNet => "nextnet",
            Network::LocalNet => "localnet",
            Network::Igor => "igor",
            Network::Esmeralda => "esmeralda",
        }
    }
}

impl TryFrom<u8> for Network {
    type Error = LightweightWalletError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Network::MainNet),
            0x01 => Ok(Network::StageNet),
            0x02 => Ok(Network::NextNet), 
            0x10 => Ok(Network::LocalNet),
            0x24 => Ok(Network::Igor),
            0x26 => Ok(Network::Esmeralda),
            _ => Err(DataStructureError::InvalidNetwork(format!("Unknown network byte: 0x{:02x}", value)).into()),
        }
    }
}

impl std::str::FromStr for Network {
    type Err = LightweightWalletError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mainnet" => Ok(Network::MainNet),
            "stagenet" => Ok(Network::StageNet),
            "nextnet" => Ok(Network::NextNet),
            "localnet" => Ok(Network::LocalNet),
            "igor" => Ok(Network::Igor),
            "esmeralda" | "esme" => Ok(Network::Esmeralda),
            _ => Err(DataStructureError::InvalidNetwork(format!("Unknown network: {}", s)).into()),
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Network::Esmeralda
    }
}

/// Tari address features (exact implementation from source of truth)
#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
pub struct TariAddressFeatures(pub u8);

impl TariAddressFeatures {
    pub const INTERACTIVE_ONLY: u8 = 0b00000001;
    pub const ONE_SIDED_ONLY: u8 = 0b00000010;
    pub const PAYMENT_ID: u8 = 0b00000100;

    pub fn create_interactive_only() -> Self {
        Self(Self::INTERACTIVE_ONLY)
    }

    pub fn create_one_sided_only() -> Self {
        Self(Self::ONE_SIDED_ONLY)
    }

    pub fn create_interactive_and_one_sided() -> Self {
        Self(Self::INTERACTIVE_ONLY | Self::ONE_SIDED_ONLY)
    }

    pub fn from_bits(bits: u8) -> Option<Self> {
        // Validate that only known feature flags are set
        const VALID_FLAGS: u8 = TariAddressFeatures::PAYMENT_ID | TariAddressFeatures::INTERACTIVE_ONLY | TariAddressFeatures::ONE_SIDED_ONLY;
        if (bits & !VALID_FLAGS) == 0 {
            Some(TariAddressFeatures(bits))
        } else {
            None
        }
    }

    pub fn set(&mut self, flag: u8, value: bool) {
        if value {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }

    pub fn contains(&self, flag: u8) -> bool {
        (self.0 & flag) != 0
    }

    pub fn is_interactive(&self) -> bool {
        self.0 & Self::INTERACTIVE_ONLY != 0
    }

    pub fn is_one_sided(&self) -> bool {
        self.0 & Self::ONE_SIDED_ONLY != 0
    }
}

impl Default for TariAddressFeatures {
    fn default() -> Self {
        Self::create_interactive_and_one_sided()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaxSizeBytes {
    bytes: Vec<u8>,
}

impl MaxSizeBytes {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn from_bytes_truncate(bytes: &[u8]) -> Self {
        Self { bytes: bytes.to_vec() }
    }

    pub fn empty() -> Self {
        Self::new()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl Default for MaxSizeBytes {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<[u8]> for MaxSizeBytes {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

// Dual address implementation (ported from core exactly)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DualAddress {
    network: Network,
    features: TariAddressFeatures,
    public_view_key: CompressedPublicKey,
    public_spend_key: CompressedPublicKey,
    payment_id_user_data: MaxSizeBytes,
}

impl Default for DualAddress {
    fn default() -> Self {
        Self {
            network: Network::Esmeralda,
            features: TariAddressFeatures::default(),
            public_view_key: CompressedPublicKey::new([0u8; 32]),
            public_spend_key: CompressedPublicKey::new([0u8; 32]),
            payment_id_user_data: MaxSizeBytes::empty(),
        }
    }
}

impl DualAddress {
    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new(
        view_key: CompressedPublicKey,
        spend_key: CompressedPublicKey,
        network: Network,
        features: TariAddressFeatures,
        payment_id_user_data: Option<Vec<u8>>,
    ) -> Result<Self, LightweightWalletError> {
        let mut features = features;
        let payment_id_user_data = match payment_id_user_data {
            Some(data) => {
                if data.len() > MAX_ENCRYPTED_DATA_SIZE {
                    return Err(DataStructureError::InvalidAddress("Payment ID too large".to_string()).into());
                }
                features.set(TariAddressFeatures::PAYMENT_ID, true);
                MaxSizeBytes::from_bytes_truncate(&data)
            },
            None => MaxSizeBytes::empty(),
        };
        Ok(Self {
            network,
            features,
            public_view_key: view_key,
            public_spend_key: spend_key,
            payment_id_user_data,
        })
    }

    /// Creates a new Tari Address from the provided public keys and network while using the default features
    pub fn new_with_default_features(
        view_key: CompressedPublicKey,
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Self::new(view_key, spend_key, network, TariAddressFeatures::default(), None)
    }

    pub fn add_payment_id_user_data(&mut self, data: Vec<u8>) -> Result<(), LightweightWalletError> {
        if data.len() > MAX_ENCRYPTED_DATA_SIZE {
            return Err(DataStructureError::InvalidAddress("Payment ID too large".to_string()).into());
        }
        self.features.set(TariAddressFeatures::PAYMENT_ID, true);
        self.payment_id_user_data = MaxSizeBytes::from_bytes_truncate(&data);
        Ok(())
    }

    /// helper function to convert emojis to u8
    pub fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, LightweightWalletError> {
        let length = emoji.chars().count();
        if !(TARI_ADDRESS_INTERNAL_DUAL_SIZE..=TARI_ADDRESS_INTERNAL_DUAL_SIZE + MAX_ENCRYPTED_DATA_SIZE)
            .contains(&length)
        {
            return Err(DataStructureError::InvalidAddress("Invalid emoji length".to_string()).into());
        }
        let mut bytes = Vec::with_capacity(length);
        for c in emoji.chars() {
            if let Some(&i) = REVERSE_EMOJI.get(&c) {
                bytes.push(i);
            } else {
                return Err(DataStructureError::InvalidAddress("Invalid emoji character".to_string()).into());
            }
        }
        Ok(bytes)
    }

    /// Construct an TariAddress from an emoji string
    pub fn from_emoji_string(emoji: &str) -> Result<Self, LightweightWalletError> {
        let bytes = Self::emoji_to_bytes(emoji)?;
        Self::from_bytes(&bytes)
    }

    pub fn get_payment_id_user_data_bytes(&self) -> Vec<u8> {
        self.payment_id_user_data.as_ref().to_vec()
    }

    /// Gets the network from the Tari Address
    pub fn network(&self) -> Network {
        self.network
    }

    /// Gets the features from the Tari Address
    pub fn features(&self) -> TariAddressFeatures {
        self.features
    }

    /// Convert Tari Address to an emoji string
    pub fn to_emoji_string(&self) -> String {
        // Convert the public key to bytes and compute the checksum
        let bytes = self.to_vec();
        bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>()
    }

    /// Return the public view key of a Tari Address
    pub fn public_view_key(&self) -> &CompressedPublicKey {
        &self.public_view_key
    }

    /// Return the public spend key of a Tari Address
    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        &self.public_spend_key
    }

    /// Construct Tari Address from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError>
    where Self: Sized {
        let length = bytes.len();
        if !(TARI_ADDRESS_INTERNAL_DUAL_SIZE..=TARI_ADDRESS_INTERNAL_DUAL_SIZE + MAX_ENCRYPTED_DATA_SIZE)
            .contains(&length)
        {
            return Err(DataStructureError::InvalidAddress("Invalid size".to_string()).into());
        }
        if validate_checksum(bytes).is_err() {
            return Err(DataStructureError::InvalidAddress("Invalid checksum".to_string()).into());
        }
        let network = Network::try_from(bytes[0])
            .map_err(|_| DataStructureError::InvalidAddress("Invalid network".to_string()))?;
        let features = TariAddressFeatures::from_bits(bytes[1])
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid features".to_string()))?;
        
        // Use from_canonical_bytes equivalent for CompressedPublicKey
        let mut view_key_bytes = [0u8; 32];
        view_key_bytes.copy_from_slice(&bytes[2..34]);
        let public_view_key = CompressedPublicKey::new(view_key_bytes);
        
        let mut spend_key_bytes = [0u8; 32];
        spend_key_bytes.copy_from_slice(&bytes[34..66]);
        let public_spend_key = CompressedPublicKey::new(spend_key_bytes);
        
        let payment_id_user_data = MaxSizeBytes::from_bytes_truncate(&bytes[66..length - 1]);
        Ok(Self {
            network,
            features,
            public_view_key,
            public_spend_key,
            payment_id_user_data,
        })
    }

    /// Convert Tari Address to bytes
    pub fn to_vec(&self) -> Vec<u8> {
        let length = TARI_ADDRESS_INTERNAL_DUAL_SIZE + self.payment_id_user_data.len();
        let mut buf = vec![0; length];
        buf[0] = self.network.as_byte();
        buf[1] = self.features.0;
        buf[2..34].copy_from_slice(&self.public_view_key.as_bytes());
        buf[34..66].copy_from_slice(&self.public_spend_key.as_bytes());
        buf[66..(length - 1)].copy_from_slice(self.payment_id_user_data.as_bytes());
        let checksum = compute_checksum(&buf[0..(length - 1)]);
        buf[length - 1] = checksum;
        buf
    }

    /// Convert Tari Address to Base58 string (exact format from source of truth)
    pub fn to_base58(&self) -> String {
        let bytes = self.to_vec();
        let mut base58 = "".to_string();
        base58.push_str(&bs58::encode(&bytes[0..1]).into_string());
        base58.push_str(&bs58::encode(&bytes[1..2]).into_string());
        base58.push_str(&bs58::encode(&bytes[2..]).into_string());
        base58
    }

    /// Construct Tari Address from Base58 (exact format from source of truth)
    pub fn from_base58(base58_str: &str) -> Result<Self, LightweightWalletError> {
        if base58_str.len() < INTERNAL_DUAL_BASE58_MIN_SIZE || base58_str.len() > INTERNAL_DUAL_BASE58_MAX_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid base58 size".to_string()).into());
        }

        // Split the base58 string into three parts as per source of truth:
        // first 2 characters: network (1 char) + features (1 char)
        // remaining: public keys + payment_id + checksum
        let (first, rest) = base58_str
            .split_at_checked(2)
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid character".to_string()))?;
        let (network, features) = first
            .split_at_checked(1)
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid character".to_string()))?;
        
        // Decode each part separately
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover network".to_string()))?;
        let mut features_bytes = bs58::decode(features)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover features".to_string()))?;
        let mut rest_bytes = bs58::decode(rest)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public keys".to_string()))?;
        
        // Reconstruct the full byte array
        result.append(&mut features_bytes);
        result.append(&mut rest_bytes);
        
        Self::from_bytes(&result)
    }

    /// Convert Tari Address to emoji format (using correct EMOJI array)
    pub fn to_emoji(&self) -> String {
        let bytes = self.to_vec();
        bytes.iter().map(|&b| EMOJI[b as usize]).collect()
    }

    /// Construct Tari Address from emoji format (using correct EMOJI array)
    pub fn from_emoji(emoji_str: &str) -> Result<Self, LightweightWalletError> {
        let mut bytes = Vec::new();
        
        for emoji_char in emoji_str.chars() {
            if let Some(&byte_val) = REVERSE_EMOJI.get(&emoji_char) {
                bytes.push(byte_val);
            } else {
                return Err(DataStructureError::InvalidAddress(format!("Invalid emoji character: {}", emoji_char)).into());
            }
        }

        Self::from_bytes(&bytes)
    }

    /// Convert Tari dual Address to hex
    pub fn to_hex(&self) -> String {
        let buf = self.to_vec();
        hex::encode(buf)
    }

    /// Creates Tari dual Address from hex
    pub fn from_hex(hex_str: &str) -> Result<Self, LightweightWalletError> {
        let buf = hex::decode(hex_str)
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        Self::from_bytes(buf.as_slice())
    }
}

// Single address implementation (ported from core exactly)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SingleAddress {
    network: Network,
    features: TariAddressFeatures,
    public_spend_key: CompressedPublicKey,
}

impl Default for SingleAddress {
    fn default() -> Self {
        Self {
            network: Network::Esmeralda,
            features: TariAddressFeatures::default(),
            public_spend_key: CompressedPublicKey::new([0u8; 32]),
        }
    }
}

impl SingleAddress {
    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new(
        spend_key: CompressedPublicKey,
        network: Network,
        features: TariAddressFeatures,
    ) -> Result<Self, LightweightWalletError> {
        Ok(Self {
            network,
            features,
            public_spend_key: spend_key,
        })
    }

    /// Creates a new Tari Address from the provided public keys and network while using the default features
    pub fn new_with_interactive_only(
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Self::new(spend_key, network, TariAddressFeatures::create_interactive_only())
    }

    /// helper function to convert emojis to u8
    pub fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, LightweightWalletError> {
        // The string must be the correct size, including the checksum
        let length = emoji.chars().count();
        if length != TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid emoji length".to_string()).into());
        }

        // Convert the emoji string to a byte array
        let mut bytes = Vec::with_capacity(TARI_ADDRESS_INTERNAL_SINGLE_SIZE);
        for c in emoji.chars() {
            if let Some(&i) = REVERSE_EMOJI.get(&c) {
                bytes.push(i);
            } else {
                return Err(DataStructureError::InvalidAddress("Invalid emoji character".to_string()).into());
            }
        }
        Ok(bytes)
    }

    /// Construct an TariAddress from an emoji string
    pub fn from_emoji_string(emoji: &str) -> Result<Self, LightweightWalletError> {
        let bytes = Self::emoji_to_bytes(emoji)?;
        Self::from_bytes(&bytes)
    }

    /// Gets the network from the Tari Address
    pub fn network(&self) -> Network {
        self.network
    }

    /// Gets the features from the Tari Address
    pub fn features(&self) -> TariAddressFeatures {
        self.features
    }

    /// Convert Tari Address to an emoji string
    pub fn to_emoji_string(&self) -> String {
        // Convert the public key to bytes and compute the checksum
        let bytes = self.to_vec();
        bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>()
    }

    /// Return the public spend key of a Tari Address
    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        &self.public_spend_key
    }

    /// Construct Tari Address from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError>
    where Self: Sized {
        let length = bytes.len();
        if length != TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid size".to_string()).into());
        }
        if validate_checksum(bytes).is_err() {
            return Err(DataStructureError::InvalidAddress("Invalid checksum".to_string()).into());
        }
        let network = Network::try_from(bytes[0])
            .map_err(|_| DataStructureError::InvalidAddress("Invalid network".to_string()))?;
        let features = TariAddressFeatures::from_bits(bytes[1])
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid features".to_string()))?;
        
        // Use from_canonical_bytes equivalent for CompressedPublicKey
        let mut spend_key_bytes = [0u8; 32];
        spend_key_bytes.copy_from_slice(&bytes[2..34]);
        let public_spend_key = CompressedPublicKey::new(spend_key_bytes);
        Ok(Self {
            network,
            features,
            public_spend_key,
        })
    }

    /// Convert Tari Address to bytes
    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf = [0u8; TARI_ADDRESS_INTERNAL_SINGLE_SIZE];
        buf[0] = self.network.as_byte();
        buf[1] = self.features.0;
        buf[2..34].copy_from_slice(&self.public_spend_key.as_bytes());
        let checksum = compute_checksum(&buf[0..34]);
        buf[34] = checksum;
        buf.to_vec()
    }

    /// Convert Tari Address to Base58 string (exact format from source of truth)
    pub fn to_base58(&self) -> String {
        let bytes = self.to_vec();
        let mut base58 = "".to_string();
        base58.push_str(&bs58::encode(&bytes[0..1]).into_string());
        base58.push_str(&bs58::encode(&bytes[1..2]).into_string());
        base58.push_str(&bs58::encode(&bytes[2..]).into_string());
        base58
    }

    /// Construct Tari Address from Base58 (exact format from source of truth)  
    pub fn from_base58(base58_str: &str) -> Result<Self, LightweightWalletError> {
        if base58_str.len() < INTERNAL_SINGLE_MIN_BASE58_SIZE || base58_str.len() > INTERNAL_SINGLE_MAX_BASE58_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid base58 size".to_string()).into());
        }

        // Split the base58 string into three parts as per source of truth:
        // first 2 characters: network (1 char) + features (1 char)
        // remaining: public key + checksum
        let (first, rest) = base58_str
            .split_at_checked(2)
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid character".to_string()))?;
        let (network, features) = first
            .split_at_checked(1)
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid character".to_string()))?;
        
        // Decode each part separately
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover network".to_string()))?;
        let mut features_bytes = bs58::decode(features)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover features".to_string()))?;
        let mut rest_bytes = bs58::decode(rest)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        
        // Reconstruct the full byte array
        result.append(&mut features_bytes);
        result.append(&mut rest_bytes);
        
        Self::from_bytes(&result)
    }

    /// Convert Tari Address to emoji format (using correct EMOJI array)
    pub fn to_emoji(&self) -> String {
        let bytes = self.to_vec();
        bytes.iter().map(|&b| EMOJI[b as usize]).collect()
    }

    /// Construct Tari Address from emoji format (using correct EMOJI array)
    pub fn from_emoji(emoji_str: &str) -> Result<Self, LightweightWalletError> {
        let mut bytes = Vec::new();
        
        for emoji_char in emoji_str.chars() {
            if let Some(&byte_val) = REVERSE_EMOJI.get(&emoji_char) {
                bytes.push(byte_val);
            } else {
                return Err(DataStructureError::InvalidAddress(format!("Invalid emoji character: {}", emoji_char)).into());
            }
        }

        Self::from_bytes(&bytes)
    }

    /// Convert Tari single Address to hex
    pub fn to_hex(&self) -> String {
        let buf = self.to_vec();
        hex::encode(buf)
    }

    /// Creates Tari single Address from hex
    pub fn from_hex(hex_str: &str) -> Result<Self, LightweightWalletError> {
        let buf = hex::decode(hex_str)
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        Self::from_bytes(buf.as_slice())
    }
}

// Main TariAddress enum (ported from core exactly)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TariAddress {
    Dual(DualAddress),
    Single(SingleAddress),
}

impl TariAddress {
    /// Creates a new dual Tari Address from the provided public keys, network and features
    pub fn new_dual_address(
        view_key: CompressedPublicKey,
        spend_key: CompressedPublicKey,
        network: Network,
        features: TariAddressFeatures,
        payment_id_user_data: Option<Vec<u8>>,
    ) -> Result<Self, LightweightWalletError> {
        Ok(TariAddress::Dual(DualAddress::new(
            view_key,
            spend_key,
            network,
            features,
            payment_id_user_data,
        )?))
    }

    /// Creates a new single Tari Address from the provided public key, network and features
    pub fn new_single_address(
        spend_key: CompressedPublicKey,
        network: Network,
        features: TariAddressFeatures,
    ) -> Result<Self, LightweightWalletError> {
        Ok(TariAddress::Single(SingleAddress::new(
            spend_key,
            network,
            features,
        )?))
    }

    /// Creates a new dual Tari Address from the provided public keys and network while using the default features
    pub fn new_dual_address_with_default_features(
        view_key: CompressedPublicKey,
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Ok(TariAddress::Dual(DualAddress::new_with_default_features(
            view_key,
            spend_key,
            network,
        )?))
    }

    /// Creates a new single Tari Address from the provided public key and network while using interactive_only features
    pub fn new_single_address_with_interactive_only(
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Ok(TariAddress::Single(SingleAddress::new_with_interactive_only(
            spend_key,
            network,
        )?))
    }

    /// Construct Tari Address from an emoji string
    pub fn from_emoji_string(emoji: &str) -> Result<Self, LightweightWalletError> {
        let length = emoji.chars().count();
        if length == TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
            Ok(TariAddress::Single(SingleAddress::from_emoji_string(emoji)?))
        } else if (TARI_ADDRESS_INTERNAL_DUAL_SIZE..=TARI_ADDRESS_INTERNAL_DUAL_SIZE + MAX_ENCRYPTED_DATA_SIZE)
            .contains(&length)
        {
            Ok(TariAddress::Dual(DualAddress::from_emoji_string(emoji)?))
        } else {
            Err(DataStructureError::InvalidAddress("Invalid emoji length".to_string()).into())
        }
    }

    /// Construct Tari Address from Base58
    pub fn from_base58(base58_str: &str) -> Result<Self, LightweightWalletError> {
        if base58_str.len() < INTERNAL_SINGLE_MIN_BASE58_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid base58 size".to_string()).into());
        }

        let (first, rest) = base58_str
            .split_at_checked(2)
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid character".to_string()))?;
        let (network, features) = first
            .split_at_checked(1)
            .ok_or_else(|| DataStructureError::InvalidAddress("Invalid character".to_string()))?;
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover network".to_string()))?;
        let mut features = bs58::decode(features)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover feature".to_string()))?;
        let mut rest = bs58::decode(rest)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        result.append(&mut features);
        result.append(&mut rest);

        Self::from_bytes(result.as_slice())
    }

    /// Construct Tari Address from hex
    pub fn from_hex(hex_str: &str) -> Result<Self, LightweightWalletError> {
        let bytes = hex::decode(hex_str)
            .map_err(|_| DataStructureError::InvalidAddress("Invalid hex".to_string()))?;
        Self::from_bytes(&bytes)
    }

    /// Construct Tari Address from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError>
    where Self: Sized {
        if !(bytes.len() == TARI_ADDRESS_INTERNAL_SINGLE_SIZE ||
            (bytes.len() >= TARI_ADDRESS_INTERNAL_DUAL_SIZE &&
                bytes.len() <= (TARI_ADDRESS_INTERNAL_DUAL_SIZE + MAX_ENCRYPTED_DATA_SIZE)))
        {
            return Err(DataStructureError::InvalidAddress("Invalid size".to_string()).into());
        }
        if bytes.len() == TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
            Ok(TariAddress::Single(SingleAddress::from_bytes(bytes)?))
        } else {
            Ok(TariAddress::Dual(DualAddress::from_bytes(bytes)?))
        }
    }

    /// Convert Tari Address to bytes
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            TariAddress::Dual(v) => v.to_vec(),
            TariAddress::Single(v) => v.to_vec(),
        }
    }

    /// Convert Tari Address to an emoji string
    pub fn to_emoji_string(&self) -> String {
        match self {
            TariAddress::Dual(v) => v.to_emoji_string(),
            TariAddress::Single(v) => v.to_emoji_string(),
        }
    }

    /// Convert Tari Address to Base58
    pub fn to_base58(&self) -> String {
        match self {
            TariAddress::Dual(v) => v.to_base58(),
            TariAddress::Single(v) => v.to_base58(),
        }
    }

    /// Convert Tari Address to hex
    pub fn to_hex(&self) -> String {
        match self {
            TariAddress::Dual(v) => v.to_hex(),
            TariAddress::Single(v) => v.to_hex(),
        }
    }

    /// Return the public view key of a Tari Address (only for dual addresses)
    pub fn public_view_key(&self) -> Option<&CompressedPublicKey> {
        match self {
            TariAddress::Dual(v) => Some(v.public_view_key()),
            TariAddress::Single(_) => None,
        }
    }

    /// Return the public spend key of a Tari Address
    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        match self {
            TariAddress::Dual(v) => v.public_spend_key(),
            TariAddress::Single(v) => v.public_spend_key(),
        }
    }

    /// Gets the network from the Tari Address
    pub fn network(&self) -> Network {
        match self {
            TariAddress::Dual(v) => v.network(),
            TariAddress::Single(v) => v.network(),
        }
    }

    /// Gets the features from the Tari Address
    pub fn features(&self) -> TariAddressFeatures {
        match self {
            TariAddress::Dual(v) => v.features(),
            TariAddress::Single(v) => v.features(),
        }
    }

    /// Try to parse a string as a Tari address (auto-detects format)
    pub fn from_string(input: &str) -> Result<Self, LightweightWalletError> {
        // Try emoji first (most common)
        if let Ok(address) = Self::from_emoji_string(input) {
            return Ok(address);
        }
        
        // Try hex
        if let Ok(address) = Self::from_hex(input) {
            return Ok(address);
        }
        
        // Try base58
        if let Ok(address) = Self::from_base58(input) {
            return Ok(address);
        }
        
        Err(DataStructureError::InvalidAddress("Cannot parse address in any known format".to_string()).into())
    }
}

// Address format detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressFormat {
    Emoji,
    Base58,
    Hex,
}

impl TariAddress {
    pub fn format(&self) -> AddressFormat {
        // This is a placeholder - in practice, you'd detect based on the string format
        AddressFormat::Emoji
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_structures::types::PrivateKey;

    #[test]
    fn test_valid_dual_emoji_address() {
        // Generate random public keys
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Generate a dual address from the public keys and ensure we recover it
        let emoji_id_from_public_key =
            DualAddress::new_with_default_features(view_key.clone(), spend_key.clone(), Network::Esmeralda).unwrap();
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_and_one_sided());
        
        // Generate a dual address from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = DualAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_emoji_string.public_view_key(), &view_key);
    }

    #[test]
    fn test_valid_single_emoji_address() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Generate a single address from the public key and ensure we recover it
        let emoji_id_from_public_key =
            SingleAddress::new_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &public_key);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_only());

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_SINGLE_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = SingleAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);
    }

    #[test]
    fn test_valid_dual_base58_address() {
        // Generate random public keys
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Generate a dual address from the public keys
        let address = DualAddress::new_with_default_features(view_key.clone(), spend_key.clone(), Network::Esmeralda).unwrap();

        let buff = address.to_vec();
        let base58 = address.to_base58();
        let hex = address.to_hex();
        let emoji = address.to_emoji_string();

        let address_buff = DualAddress::from_bytes(&buff).unwrap();
        assert_eq!(address_buff.public_spend_key(), address.public_spend_key());
        assert_eq!(address_buff.public_view_key(), address.public_view_key());
        assert_eq!(address_buff.network(), address.network());
        assert_eq!(address_buff.features(), address.features());

        let address_base58 = DualAddress::from_base58(&base58).unwrap();
        assert_eq!(address_base58.public_spend_key(), address.public_spend_key());
        assert_eq!(address_base58.public_view_key(), address.public_view_key());
        assert_eq!(address_base58.network(), address.network());
        assert_eq!(address_base58.features(), address.features());

        let address_hex = DualAddress::from_hex(&hex).unwrap();
        assert_eq!(address_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_hex.public_view_key(), address.public_view_key());
        assert_eq!(address_hex.network(), address.network());
        assert_eq!(address_hex.features(), address.features());

        let address_emoji = DualAddress::from_emoji_string(&emoji).unwrap();
        assert_eq!(address_emoji.public_spend_key(), address.public_spend_key());
        assert_eq!(address_emoji.public_view_key(), address.public_view_key());
        assert_eq!(address_emoji.network(), address.network());
        assert_eq!(address_emoji.features(), address.features());
    }

    #[test]
    fn test_valid_single_base58_address() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Generate a single address from the public key
        let address = SingleAddress::new_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();

        let buff = address.to_vec();
        let base58 = address.to_base58();
        let hex = address.to_hex();
        let emoji = address.to_emoji_string();

        let address_buff = SingleAddress::from_bytes(&buff).unwrap();
        assert_eq!(address_buff.public_spend_key(), address.public_spend_key());
        assert_eq!(address_buff.network(), address.network());
        assert_eq!(address_buff.features(), address.features());

        let address_base58 = SingleAddress::from_base58(&base58).unwrap();
        assert_eq!(address_base58.public_spend_key(), address.public_spend_key());
        assert_eq!(address_base58.network(), address.network());
        assert_eq!(address_base58.features(), address.features());

        let address_hex = SingleAddress::from_hex(&hex).unwrap();
        assert_eq!(address_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_hex.network(), address.network());
        assert_eq!(address_hex.features(), address.features());

        let address_emoji = SingleAddress::from_emoji_string(&emoji).unwrap();
        assert_eq!(address_emoji.public_spend_key(), address.public_spend_key());
        assert_eq!(address_emoji.network(), address.network());
        assert_eq!(address_emoji.features(), address.features());
    }

    #[test]
    fn test_valid_dual_hex_address() {
        // Generate random public keys
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Generate a dual address from the public keys
        let address = DualAddress::new_with_default_features(view_key.clone(), spend_key.clone(), Network::Esmeralda).unwrap();
        let hex_string = address.to_hex();
        
        // Parse it back and verify
        let address_from_hex = DualAddress::from_hex(&hex_string).unwrap();
        assert_eq!(address_from_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_from_hex.public_view_key(), address.public_view_key());
        assert_eq!(address_from_hex.network(), address.network());
        assert_eq!(address_from_hex.features(), address.features());
    }

    #[test]
    fn test_valid_single_hex_address() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Generate a single address from the public key
        let address = SingleAddress::new_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();
        let hex_string = address.to_hex();
        
        // Parse it back and verify
        let address_from_hex = SingleAddress::from_hex(&hex_string).unwrap();
        assert_eq!(address_from_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_from_hex.network(), address.network());
        assert_eq!(address_from_hex.features(), address.features());
    }

    #[test]
    fn test_auto_detect_emoji() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Create single address
        let address = TariAddress::new_single_address_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();
        let emoji_string = address.to_emoji_string();
        
        // Auto-detect format
        let parsed_address = TariAddress::from_string(&emoji_string).unwrap();
        assert_eq!(parsed_address.public_spend_key(), &public_key);
        assert_eq!(parsed_address.network(), Network::Esmeralda);
    }

    #[test]
    fn test_auto_detect_hex() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Create single address
        let address = TariAddress::new_single_address_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();
        let hex_string = address.to_hex();
        
        // Auto-detect format
        let parsed_address = TariAddress::from_string(&hex_string).unwrap();
        assert_eq!(parsed_address.public_spend_key(), &public_key);
        assert_eq!(parsed_address.network(), Network::Esmeralda);
    }

    #[test]
    fn test_auto_detect_base58() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Create single address
        let address = TariAddress::new_single_address_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();
        let base58_string = address.to_base58();
        
        // Auto-detect format
        let parsed_address = TariAddress::from_string(&base58_string).unwrap();
        assert_eq!(parsed_address.public_spend_key(), &public_key);
        assert_eq!(parsed_address.network(), Network::Esmeralda);
    }

    #[test]
    fn test_invalid_emoji_length() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🦀🦁🦂🦃🦄";
        assert!(SingleAddress::from_emoji_string(emoji_string).is_err());
    }

    #[test]
    fn test_invalid_emoji_character() {
        // Create a valid length string but with invalid emoji
        let mut emoji_string = "🦀".repeat(TARI_ADDRESS_INTERNAL_SINGLE_SIZE - 1);
        emoji_string.push('🎅'); // This emoji is not in our EMOJI array
        assert!(SingleAddress::from_emoji_string(&emoji_string).is_err());
    }

    #[test]
    fn test_invalid_hex() {
        // Invalid hex string
        let hex_string = "xyz123";
        assert!(TariAddress::from_hex(hex_string).is_err());
    }

    #[test]
    fn test_invalid_base58() {
        // Invalid base58 string (contains 0 and O which are not valid base58 characters)
        let base58_string = "0O123456";
        assert!(TariAddress::from_base58(base58_string).is_err());
    }

    #[test]
    fn test_dual_address_with_payment_id() {
        // Generate random public keys
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Create a dual address with payment ID
        let payment_id = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let address = DualAddress::new(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::default(),
            Some(payment_id.clone()),
        ).unwrap();

        assert_eq!(address.public_spend_key(), &spend_key);
        assert_eq!(address.public_view_key(), &view_key);
        assert_eq!(address.get_payment_id_user_data_bytes(), payment_id);

        // Check the size of the corresponding emoji string
        let emoji_string = address.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE + 8);

        let features = address.features();
        assert!(features.contains(TariAddressFeatures::PAYMENT_ID));

        // Verify round-trip
        let address_from_emoji = DualAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(address_from_emoji.to_emoji_string(), emoji_string);
        assert_eq!(address_from_emoji.public_spend_key(), &spend_key);
        assert_eq!(address_from_emoji.public_view_key(), &view_key);
        assert_eq!(address_from_emoji.get_payment_id_user_data_bytes(), payment_id);
    }

    #[test]
    fn test_checksum_validation() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Create single address
        let address = SingleAddress::new_with_interactive_only(public_key.clone(), Network::Esmeralda).unwrap();
        let mut bytes = address.to_vec();
        
        // Verify valid checksum passes
        assert!(validate_checksum(&bytes).is_ok());
        
        // Corrupt the checksum and verify it fails
        bytes[TARI_ADDRESS_INTERNAL_SINGLE_SIZE - 1] = !bytes[TARI_ADDRESS_INTERNAL_SINGLE_SIZE - 1];
        assert!(validate_checksum(&bytes).is_err());
        assert!(SingleAddress::from_bytes(&bytes).is_err());
    }

    #[test]
    fn test_network_validation() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Create single address for each network
        for network in [Network::MainNet, Network::StageNet, Network::NextNet, Network::LocalNet, Network::Igor] {
            let address = SingleAddress::new_with_interactive_only(public_key.clone(), network).unwrap();
            assert_eq!(address.network(), network);
            
            // Verify round-trip
            let bytes = address.to_vec();
            let parsed_address = SingleAddress::from_bytes(&bytes).unwrap();
            assert_eq!(parsed_address.network(), network);
        }
    }

    #[test]
    fn test_features_validation() {
        // Generate random public key
        let public_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        // Test different feature combinations
        let features_list = [
            TariAddressFeatures::create_interactive_only(),
            TariAddressFeatures::create_one_sided_only(),
            TariAddressFeatures::create_interactive_and_one_sided(),
        ];

        for features in features_list {
            let address = SingleAddress::new(public_key.clone(), Network::Esmeralda, features).unwrap();
            assert_eq!(address.features(), features);
            
            // Verify round-trip
            let bytes = address.to_vec();
            let parsed_address = SingleAddress::from_bytes(&bytes).unwrap();
            assert_eq!(parsed_address.features(), features);
        }
    }
} 