// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Address handling utilities for lightweight wallets
//!
//! This module provides functionality to load and parse Tari addresses
//! from various formats including base58, hex, and emoji.

use crate::errors::{LightweightWalletError, DataStructureError};
use crate::hex_utils::{HexEncodable};
use crate::data_structures::types::{CompressedPublicKey, };
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use crate::data_structures::PrivateKey;

// Tari address constants (ported from core)
pub const TARI_ADDRESS_INTERNAL_DUAL_SIZE: usize = 67;
pub const TARI_ADDRESS_INTERNAL_SINGLE_SIZE: usize = 35;
const INTERNAL_DUAL_BASE58_MIN_SIZE: usize = 89;
const INTERNAL_DUAL_BASE58_MAX_SIZE: usize = 443;
const INTERNAL_SINGLE_MIN_BASE58_SIZE: usize = 45;
const INTERNAL_SINGLE_MAX_BASE58_SIZE: usize = 48;
const MAX_ENCRYPTED_DATA_SIZE: usize = 256;

// Tari emoji set (ported from core)
pub const EMOJI: [char; 256] = [
    '🦀', '🦁', '🦂', '🦃', '🦄', '🦅', '🦆', '🦇', '🦈', '🦉', '🦊', '🦋', '🦌', '🦍', '🦎', '🦏',
    '🦐', '🦑', '🦒', '🦓', '🦔', '🦕', '🦖', '🦗', '🦘', '🦙', '🦚', '🦛', '🦜', '🦝', '🦞', '🦟',
    '🦠', '🦡', '🦢', '🦣', '🦤', '🦥', '🦦', '🦧', '🦨', '🦩', '🦪', '🦫', '🦬', '🦭', '🦮', '🦯',
    '🦰', '🦱', '🦲', '🦳', '🦴', '🦵', '🦶', '🦷', '🦸', '🦹', '🦺', '🦻', '🦼', '🦽', '🦾', '🦿',
    '🧀', '🧁', '🧂', '🧃', '🧄', '🧅', '🧆', '🧇', '🧈', '🧉', '🧊', '🧋', '🧌', '🧍', '🧎', '🧏',
    '🧐', '🧑', '🧒', '🧓', '🧔', '🧕', '🧖', '🧗', '🧘', '🧙', '🧚', '🧛', '🧜', '🧝', '🧞', '🧟',
    '🧠', '🧡', '🧢', '🧣', '🧤', '🧥', '🧦', '🧧', '🧨', '🧩', '🧪', '🧫', '🧬', '🧭', '🧮', '🧯',
    '🧰', '🧱', '🧲', '🧳', '🧴', '🧵', '🧶', '🧷', '🧸', '🧹', '🧺', '🧻', '🧼', '🧽', '🧾', '🧿',
    '🩀', '🩁', '🩂', '🩃', '🩄', '🩅', '🩆', '🩇', '🩈', '🩉', '🩊', '🩋', '🩌', '🩍', '🩎', '🩏',
    '🩐', '🩑', '🩒', '🩓', '🩔', '🩕', '🩖', '🩗', '🩘', '🩙', '🩚', '🩛', '🩜', '🩝', '🩞', '🩟',
    '🩠', '🩡', '🩢', '🩣', '🩤', '🩥', '🩦', '🩧', '🩨', '🩩', '🩪', '🩫', '🩬', '🩭', '🩮', '🩯',
    '🩰', '🩱', '🩲', '🩳', '🩴', '🩵', '🩶', '🩷', '🩸', '🩹', '🩺', '🩻', '🩼', '🩽', '🩾', '🩿',
    '🪀', '🪁', '🪂', '🪃', '🪄', '🪅', '🪆', '🪇', '🪈', '🪉', '🪊', '🪋', '🪌', '🪍', '🪎', '🪏',
    '🪐', '🪑', '🪒', '🪓', '🪔', '🪕', '🪖', '🪗', '🪘', '🪙', '🪚', '🪛', '🪜', '🪝', '🪞', '🪟',
    '🪠', '🪡', '🪢', '🪣', '🪤', '🪥', '🪦', '🪧', '🪨', '🪩', '🪪', '🪫', '🪬', '🪭', '🪮', '🪯',
    '🪰', '🪱', '🪲', '🪳', '🪴', '🪵', '🪶', '🪷', '🪸', '🪹', '🪺', '🪻', '🪼', '🪽', '🪾', '🪿',
];

lazy_static::lazy_static! {
    static ref REVERSE_EMOJI: HashMap<char, u8> = {
        let mut map = HashMap::new();
        for (i, &emoji) in EMOJI.iter().enumerate() {
            map.insert(emoji, i as u8);
        }
        map
    };
}

// Network enum (simplified from core)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Network {
    Mainnet = 0x00,
    Esmeralda = 0x01,
    Stagenet = 0x02,
    Localnet = 0x03,
}

impl Network {
    pub fn as_byte(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for Network {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Network::Mainnet),
            0x01 => Ok(Network::Esmeralda),
            0x02 => Ok(Network::Stagenet),
            0x03 => Ok(Network::Localnet),
            _ => Err(()),
        }
    }
}

// Tari address features (ported from core)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TariAddressFeatures(pub u8);

impl TariAddressFeatures {
    pub const PAYMENT_ID: u8 = 0b0000_0100;
    pub const INTERACTIVE: u8 = 0b0000_0010;
    pub const ONE_SIDED: u8 = 0b0000_0001;

    pub fn create_interactive_only() -> Self {
        TariAddressFeatures(Self::INTERACTIVE)
    }

    pub fn create_one_sided_only() -> Self {
        TariAddressFeatures(Self::ONE_SIDED)
    }

    pub fn create_interactive_and_one_sided() -> Self {
        TariAddressFeatures(Self::INTERACTIVE | Self::ONE_SIDED)
    }

    pub fn from_bits(bits: u8) -> Option<Self> {
        Some(TariAddressFeatures(bits))
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
}

impl Default for TariAddressFeatures {
    fn default() -> Self {
        Self::create_interactive_and_one_sided()
    }
}

// Checksum functions (ported from core)
fn compute_checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

fn validate_checksum(bytes: &[u8]) -> Result<(), ()> {
    if bytes.is_empty() {
        return Err(());
    }
    let expected_checksum = bytes[bytes.len() - 1];
    let computed_checksum = compute_checksum(&bytes[..bytes.len() - 1]);
    if expected_checksum == computed_checksum {
        Ok(())
    } else {
        Err(())
    }
}

// MaxSizeBytes wrapper (simplified from core)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaxSizeBytes<const MAX_SIZE: usize> {
    data: Vec<u8>,
}

impl<const MAX_SIZE: usize> MaxSizeBytes<MAX_SIZE> {
    pub fn from_bytes_truncate(bytes: &[u8]) -> Self {
        let mut data = bytes.to_vec();
        if data.len() > MAX_SIZE {
            data.truncate(MAX_SIZE);
        }
        Self { data }
    }

    pub fn empty() -> Self {
        Self { data: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl<const MAX_SIZE: usize> AsRef<[u8]> for MaxSizeBytes<MAX_SIZE> {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

// Dual address implementation (ported from core)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DualAddress {
    network: Network,
    features: TariAddressFeatures,
    public_view_key: CompressedPublicKey,
    public_spend_key: CompressedPublicKey,
    payment_id_user_data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
}

impl DualAddress {
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

    pub fn new_with_default_features(
        view_key: CompressedPublicKey,
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Self::new(view_key, spend_key, network, TariAddressFeatures::default(), None)
    }

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

    pub fn from_emoji_string(emoji: &str) -> Result<Self, LightweightWalletError> {
        let bytes = Self::emoji_to_bytes(emoji)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError> {
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

    pub fn to_emoji_string(&self) -> String {
        let bytes = self.to_vec();
        bytes.iter().map(|&b| EMOJI[b as usize]).collect::<String>()
    }

    pub fn to_base58(&self) -> String {
        let bytes = self.to_vec();
        let mut network = bs58::encode(&bytes[0..1]).into_string();
        let features = bs58::encode(&bytes[1..2]).into_string();
        let rest = bs58::encode(&bytes[2..]).into_string();
        network.push_str(&features);
        network.push_str(&rest);
        network
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_vec())
    }

    pub fn from_base58(base58_str: &str) -> Result<Self, LightweightWalletError> {
        if base58_str.len() < INTERNAL_DUAL_BASE58_MIN_SIZE || base58_str.len() > INTERNAL_DUAL_BASE58_MAX_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid base58 size".to_string()).into());
        }
        let (first, rest) = base58_str.split_at(2);
        let (network, features) = first.split_at(1);
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover network".to_string()))?;
        let mut features = bs58::decode(features)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover features".to_string()))?;
        let mut rest = bs58::decode(rest)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        result.append(&mut features);
        result.append(&mut rest);
        Self::from_bytes(&result)
    }

    pub fn from_hex(hex_str: &str) -> Result<Self, LightweightWalletError> {
        let bytes = hex::decode(hex_str)
            .map_err(|_| DataStructureError::InvalidAddress("Invalid hex".to_string()))?;
        Self::from_bytes(&bytes)
    }

    pub fn public_view_key(&self) -> &CompressedPublicKey {
        &self.public_view_key
    }

    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        &self.public_spend_key
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn features(&self) -> TariAddressFeatures {
        self.features
    }
}

// Single address implementation (ported from core)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SingleAddress {
    network: Network,
    features: TariAddressFeatures,
    public_spend_key: CompressedPublicKey,
}

impl SingleAddress {
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

    pub fn new_with_interactive_only(
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Self::new(spend_key, network, TariAddressFeatures::create_interactive_only())
    }

    pub fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, LightweightWalletError> {
        let length = emoji.chars().count();
        if length != TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
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

    pub fn from_emoji_string(emoji: &str) -> Result<Self, LightweightWalletError> {
        let bytes = Self::emoji_to_bytes(emoji)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError> {
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
        let mut spend_key_bytes = [0u8; 32];
        spend_key_bytes.copy_from_slice(&bytes[2..34]);
        let public_spend_key = CompressedPublicKey::new(spend_key_bytes);
        Ok(Self {
            network,
            features,
            public_spend_key,
        })
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf = [0u8; TARI_ADDRESS_INTERNAL_SINGLE_SIZE];
        buf[0] = self.network.as_byte();
        buf[1] = self.features.0;
        buf[2..34].copy_from_slice(&self.public_spend_key.as_bytes());
        let checksum = compute_checksum(&buf[0..34]);
        buf[34] = checksum;
        buf.to_vec()
    }

    pub fn to_emoji_string(&self) -> String {
        let bytes = self.to_vec();
        bytes.iter().map(|&b| EMOJI[b as usize]).collect::<String>()
    }

    pub fn to_base58(&self) -> String {
        let bytes = self.to_vec();
        let mut network = bs58::encode(&bytes[0..1]).into_string();
        let features = bs58::encode(&bytes[1..2]).into_string();
        let rest = bs58::encode(&bytes[2..]).into_string();
        network.push_str(&features);
        network.push_str(&rest);
        network
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_vec())
    }

    pub fn from_base58(base58_str: &str) -> Result<Self, LightweightWalletError> {
        if base58_str.len() < INTERNAL_SINGLE_MIN_BASE58_SIZE || base58_str.len() > INTERNAL_SINGLE_MAX_BASE58_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid base58 size".to_string()).into());
        }
        let (first, rest) = base58_str.split_at(2);
        let (network, features) = first.split_at(1);
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover network".to_string()))?;
        let mut features = bs58::decode(features)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover features".to_string()))?;
        let mut rest = bs58::decode(rest)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        result.append(&mut features);
        result.append(&mut rest);
        Self::from_bytes(&result)
    }

    pub fn from_hex(hex_str: &str) -> Result<Self, LightweightWalletError> {
        let bytes = hex::decode(hex_str)
            .map_err(|_| DataStructureError::InvalidAddress("Invalid hex".to_string()))?;
        Self::from_bytes(&bytes)
    }

    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        &self.public_spend_key
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn features(&self) -> TariAddressFeatures {
        self.features
    }
}

// Main TariAddress enum (ported from core)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TariAddress {
    Dual(DualAddress),
    Single(SingleAddress),
}

impl TariAddress {
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

    pub fn new_single_address_with_interactive_only(
        spend_key: CompressedPublicKey,
        network: Network,
    ) -> Result<Self, LightweightWalletError> {
        Ok(TariAddress::Single(SingleAddress::new_with_interactive_only(
            spend_key,
            network,
        )?))
    }

    pub fn from_emoji_string(emoji: &str) -> Result<Self, LightweightWalletError> {
        let length = emoji.chars().count();
        if length == TARI_ADDRESS_INTERNAL_SINGLE_SIZE {
            Ok(TariAddress::Single(SingleAddress::from_emoji_string(emoji)?))
        } else {
            Ok(TariAddress::Dual(DualAddress::from_emoji_string(emoji)?))
        }
    }

    pub fn from_base58(base58_str: &str) -> Result<Self, LightweightWalletError> {
        if base58_str.len() < INTERNAL_SINGLE_MIN_BASE58_SIZE {
            return Err(DataStructureError::InvalidAddress("Invalid base58 size".to_string()).into());
        }
        let (first, rest) = base58_str.split_at(2);
        let (network, features) = first.split_at(1);
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover network".to_string()))?;
        let mut features = bs58::decode(features)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover features".to_string()))?;
        let mut rest = bs58::decode(rest)
            .into_vec()
            .map_err(|_| DataStructureError::InvalidAddress("Cannot recover public key".to_string()))?;
        result.append(&mut features);
        result.append(&mut rest);
        Self::from_bytes(&result)
    }

    pub fn from_hex(hex_str: &str) -> Result<Self, LightweightWalletError> {
        let bytes = hex::decode(hex_str)
            .map_err(|_| DataStructureError::InvalidAddress("Invalid hex".to_string()))?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LightweightWalletError> {
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

    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            TariAddress::Dual(addr) => addr.to_vec(),
            TariAddress::Single(addr) => addr.to_vec(),
        }
    }

    pub fn to_emoji_string(&self) -> String {
        match self {
            TariAddress::Dual(addr) => addr.to_emoji_string(),
            TariAddress::Single(addr) => addr.to_emoji_string(),
        }
    }

    pub fn to_base58(&self) -> String {
        match self {
            TariAddress::Dual(addr) => addr.to_base58(),
            TariAddress::Single(addr) => addr.to_base58(),
        }
    }

    pub fn to_hex(&self) -> String {
        match self {
            TariAddress::Dual(addr) => addr.to_hex(),
            TariAddress::Single(addr) => addr.to_hex(),
        }
    }

    pub fn public_view_key(&self) -> Option<&CompressedPublicKey> {
        match self {
            TariAddress::Dual(addr) => Some(addr.public_view_key()),
            TariAddress::Single(_) => None,
        }
    }

    pub fn public_spend_key(&self) -> &CompressedPublicKey {
        match self {
            TariAddress::Dual(addr) => addr.public_spend_key(),
            TariAddress::Single(addr) => addr.public_spend_key(),
        }
    }

    pub fn network(&self) -> Network {
        match self {
            TariAddress::Dual(addr) => addr.network(),
            TariAddress::Single(addr) => addr.network(),
        }
    }

    pub fn features(&self) -> TariAddressFeatures {
        match self {
            TariAddress::Dual(addr) => addr.features(),
            TariAddress::Single(addr) => addr.features(),
        }
    }

    // Auto-detect and load address from string
    pub fn from_string(input: &str) -> Result<Self, LightweightWalletError> {
        // Try emoji first (check if all characters are valid emoji)
        if input.chars().all(|c| REVERSE_EMOJI.contains_key(&c)) {
            return Self::from_emoji_string(input);
        }
        // Try hex (check if it's valid hex and reasonable length)
        if input.chars().all(|c| c.is_ascii_hexdigit()) && input.len() % 2 == 0 {
            return Self::from_hex(input);
        }
        // Try base58 (check if it's valid base58 charset)
        if input.chars().all(|c| "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c)) {
            return Self::from_base58(input);
        }
        Err(DataStructureError::InvalidAddress("Unable to detect address format".to_string()).into())
    }
}

// Address format enum for compatibility
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressFormat {
    Emoji,
    Base58,
    Hex,
}

impl TariAddress {
    pub fn format(&self) -> AddressFormat {
        // This is a simplified format detection - in practice you'd need to know the original format
        AddressFormat::Emoji
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_dual_emoji_address() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let emoji_string = address.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE);

        let recovered = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(recovered.to_emoji_string(), emoji_string);
        assert_eq!(recovered.public_view_key(), Some(&view_key));
        assert_eq!(recovered.public_spend_key(), &spend_key);
    }

    #[test]
    fn test_valid_single_emoji_address() {
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_single_address_with_interactive_only(
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let emoji_string = address.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_SINGLE_SIZE);

        let recovered = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(recovered.to_emoji_string(), emoji_string);
        assert_eq!(recovered.public_view_key(), None);
        assert_eq!(recovered.public_spend_key(), &spend_key);
    }

    #[test]
    fn test_valid_dual_base58_address() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let base58_string = address.to_base58();
        assert!(base58_string.len() >= INTERNAL_DUAL_BASE58_MIN_SIZE);
        assert!(base58_string.len() <= INTERNAL_DUAL_BASE58_MAX_SIZE);

        let recovered = TariAddress::from_base58(&base58_string).unwrap();
        assert_eq!(recovered.to_base58(), base58_string);
        assert_eq!(recovered.public_view_key(), Some(&view_key));
        assert_eq!(recovered.public_spend_key(), &spend_key);
    }

    #[test]
    fn test_valid_single_base58_address() {
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_single_address_with_interactive_only(
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let base58_string = address.to_base58();
        assert!(base58_string.len() >= INTERNAL_SINGLE_MIN_BASE58_SIZE);
        assert!(base58_string.len() <= INTERNAL_SINGLE_MAX_BASE58_SIZE);

        let recovered = TariAddress::from_base58(&base58_string).unwrap();
        assert_eq!(recovered.to_base58(), base58_string);
        assert_eq!(recovered.public_view_key(), None);
        assert_eq!(recovered.public_spend_key(), &spend_key);
    }

    #[test]
    fn test_valid_dual_hex_address() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let hex_string = address.to_hex();
        assert_eq!(hex_string.len(), (TARI_ADDRESS_INTERNAL_DUAL_SIZE * 2));

        let recovered = TariAddress::from_hex(&hex_string).unwrap();
        assert_eq!(recovered.to_hex(), hex_string);
        assert_eq!(recovered.public_view_key(), Some(&view_key));
        assert_eq!(recovered.public_spend_key(), &spend_key);
    }

    #[test]
    fn test_valid_single_hex_address() {
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_single_address_with_interactive_only(
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let hex_string = address.to_hex();
        assert_eq!(hex_string.len(), (TARI_ADDRESS_INTERNAL_SINGLE_SIZE * 2));

        let recovered = TariAddress::from_hex(&hex_string).unwrap();
        assert_eq!(recovered.to_hex(), hex_string);
        assert_eq!(recovered.public_view_key(), None);
        assert_eq!(recovered.public_spend_key(), &spend_key);
    }

    #[test]
    fn test_auto_detect_emoji() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let emoji_string = address.to_emoji_string();
        let detected = TariAddress::from_string(&emoji_string).unwrap();
        assert_eq!(detected.to_emoji_string(), emoji_string);
    }

    #[test]
    fn test_auto_detect_hex() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let hex_string = address.to_hex();
        let detected = TariAddress::from_string(&hex_string).unwrap();
        assert_eq!(detected.to_hex(), hex_string);
    }

    #[test]
    fn test_auto_detect_base58() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        ).unwrap();

        let base58_string = address.to_base58();
        let detected = TariAddress::from_string(&base58_string).unwrap();
        assert_eq!(detected.to_base58(), base58_string);
    }

    #[test]
    fn test_invalid_emoji_length() {
        let result = TariAddress::from_emoji_string("🦀🦁🦂"); // Too short
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_emoji_character() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());

        let address = TariAddress::new_dual_address_with_default_features(
            view_key,
            spend_key,
            Network::Esmeralda,
        ).unwrap();

        let mut emoji_string = address.to_emoji_string();
        // Replace the first emoji character with an invalid character
        // Find the first emoji character boundary and replace it
        let first_char = emoji_string.chars().next().unwrap();
        let first_char_len = first_char.len_utf8();
        emoji_string.replace_range(0..first_char_len, "A"); // Replace first emoji with invalid character

        let result = TariAddress::from_emoji_string(&emoji_string);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_hex() {
        let result = TariAddress::from_hex("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_base58() {
        let result = TariAddress::from_base58("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_dual_address_with_payment_id() {
        let view_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let spend_key = CompressedPublicKey::from_private_key(&PrivateKey::random());
        let payment_id = vec![1, 2, 3, 4, 5];

        let address = TariAddress::new_dual_address(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::default(),
            Some(payment_id.clone()),
        ).unwrap();

        let emoji_string = address.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE + payment_id.len());

        let recovered = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(recovered.to_emoji_string(), emoji_string);
    }
} 