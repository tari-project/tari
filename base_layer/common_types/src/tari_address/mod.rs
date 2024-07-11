// Copyright 2020. The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

pub mod dual_address;
mod single_address;

use std::{
    fmt,
    fmt::{Display, Error, Formatter},
    str::FromStr,
};

use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_crypto::tari_utilities::ByteArray;
use tari_utilities::hex::{from_hex, Hex};
use thiserror::Error;

use crate::{
    emoji::EMOJI,
    tari_address::{dual_address::DualAddress, single_address::SingleAddress},
    types::PublicKey,
};

const INTERNAL_DUAL_SIZE: usize = 67; // number of bytes used for the internal representation
const INTERNAL_SINGLE_SIZE: usize = 35; // number of bytes used for the internal representation

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TariAddressFeatures(u8);

bitflags! {
    impl TariAddressFeatures: u8 {
        const INTERACTIVE = 2u8;
        ///one sided payment
        const ONE_SIDED = 1u8;
    }
}

impl TariAddressFeatures {
    pub fn create_interactive_only() -> TariAddressFeatures {
        TariAddressFeatures::INTERACTIVE
    }

    pub fn create_one_sided_only() -> TariAddressFeatures {
        TariAddressFeatures::ONE_SIDED
    }

    pub fn create_interactive_and_one_sided() -> TariAddressFeatures {
        TariAddressFeatures::INTERACTIVE | TariAddressFeatures::ONE_SIDED
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

impl Default for TariAddressFeatures {
    fn default() -> TariAddressFeatures {
        TariAddressFeatures::INTERACTIVE | TariAddressFeatures::ONE_SIDED
    }
}

impl fmt::Display for TariAddressFeatures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.contains(TariAddressFeatures::INTERACTIVE) {
            write!(f, "Interactive,")?;
        }
        if self.contains(TariAddressFeatures::ONE_SIDED) {
            write!(f, "One-sided,")?;
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum TariAddressError {
    #[error("Invalid size")]
    InvalidSize,
    #[error("Invalid network")]
    InvalidNetwork,
    #[error("Invalid features")]
    InvalidFeatures,
    #[error("Invalid checksum")]
    InvalidChecksum,
    #[error("Invalid emoji character")]
    InvalidEmoji,
    #[error("Cannot recover public key")]
    CannotRecoverPublicKey,
    #[error("Cannot recover network")]
    CannotRecoverNetwork,
    #[error("Cannot recover feature")]
    CannotRecoverFeature,
    #[error("Could not recover TariAddress from string")]
    InvalidAddressString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TariAddress {
    Dual(DualAddress),
    Single(SingleAddress),
}

impl TariAddress {
    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new_dual_address(
        view_key: PublicKey,
        spend_key: PublicKey,
        network: Network,
        features: TariAddressFeatures,
    ) -> Self {
        TariAddress::Dual(DualAddress::new(view_key, spend_key, network, features))
    }

    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new_single_address(spend_key: PublicKey, network: Network, features: TariAddressFeatures) -> Self {
        TariAddress::Single(SingleAddress::new(spend_key, network, features))
    }

    /// Creates a new Tari Address from the provided public keys and network while using the default features
    pub fn new_dual_address_with_default_features(view_key: PublicKey, spend_key: PublicKey, network: Network) -> Self {
        TariAddress::Dual(DualAddress::new_with_default_features(view_key, spend_key, network))
    }

    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new_single_address_with_interactive_only(spend_key: PublicKey, network: Network) -> Self {
        TariAddress::Single(SingleAddress::new_with_interactive_only(spend_key, network))
    }

    /// helper function to convert emojis to u8
    fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, TariAddressError> {
        // The string must be the correct size, including the checksum
        if !(emoji.chars().count() == INTERNAL_SINGLE_SIZE || emoji.chars().count() == INTERNAL_DUAL_SIZE) {
            return Err(TariAddressError::InvalidSize);
        }
        if emoji.chars().count() == INTERNAL_SINGLE_SIZE {
            SingleAddress::emoji_to_bytes(emoji)
        } else {
            DualAddress::emoji_to_bytes(emoji)
        }
    }

    /// Construct an TariAddress from an emoji string
    pub fn from_emoji_string(emoji: &str) -> Result<Self, TariAddressError> {
        let bytes = TariAddress::emoji_to_bytes(emoji)?;

        TariAddress::from_bytes(&bytes)
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

    /// Gets the checksum from the Tari Address
    pub fn calculate_checksum(&self) -> u8 {
        let bytes = self.to_vec();
        // -1 is safe as this the len will always be greater than 0
        bytes[bytes.len() - 1]
    }

    /// Convert Tari Address to an emoji string
    pub fn to_emoji_string(&self) -> String {
        // Convert the public key to bytes and compute the checksum
        let bytes = self.to_vec();
        bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>()
    }

    /// Return the public view key of an Tari Address
    pub fn public_view_key(&self) -> Option<&PublicKey> {
        match self {
            TariAddress::Dual(v) => Some(v.public_view_key()),
            TariAddress::Single(_) => None,
        }
    }

    /// Return the public spend key of an Tari Address
    pub fn public_spend_key(&self) -> &PublicKey {
        match self {
            TariAddress::Dual(v) => v.public_spend_key(),
            TariAddress::Single(v) => v.public_spend_key(),
        }
    }

    /// Return the public comms key of an Tari Address, which is the public spend key
    pub fn comms_public_key(&self) -> &PublicKey {
        match self {
            TariAddress::Dual(v) => v.public_spend_key(),
            TariAddress::Single(v) => v.public_spend_key(),
        }
    }

    /// Construct Tari Address from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<TariAddress, TariAddressError>
    where Self: Sized {
        if !(bytes.len() == INTERNAL_SINGLE_SIZE || bytes.len() == INTERNAL_DUAL_SIZE) {
            return Err(TariAddressError::InvalidSize);
        }
        if bytes.len() == INTERNAL_SINGLE_SIZE {
            Ok(TariAddress::Single(SingleAddress::from_bytes(bytes)?))
        } else {
            Ok(TariAddress::Dual(DualAddress::from_bytes(bytes)?))
        }
    }

    /// Convert Tari Address to bytes
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            TariAddress::Dual(v) => v.to_bytes().to_vec(),
            TariAddress::Single(v) => v.to_bytes().to_vec(),
        }
    }

    /// Construct Tari Address from hex
    pub fn from_base58(hex_str: &str) -> Result<TariAddress, TariAddressError> {
        if hex_str.len() < 47 {
            return Err(TariAddressError::InvalidSize);
        }
        let (first, rest) = hex_str.split_at(2);
        let (network, features) = first.split_at(1);
        let mut result = bs58::decode(network)
            .into_vec()
            .map_err(|_| TariAddressError::CannotRecoverNetwork)?;
        let mut features = bs58::decode(features)
            .into_vec()
            .map_err(|_| TariAddressError::CannotRecoverFeature)?;
        let mut rest = bs58::decode(rest)
            .into_vec()
            .map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        result.append(&mut features);
        result.append(&mut rest);
        Self::from_bytes(result.as_slice())
    }

    /// Convert Tari Address to bytes
    pub fn to_base58(&self) -> String {
        let bytes = self.to_vec();
        let mut network = bs58::encode(&bytes[0..1]).into_string();
        let features = bs58::encode(&bytes[1..2].to_vec()).into_string();
        let rest = bs58::encode(&bytes[2..]).into_string();
        network.push_str(&features);
        network.push_str(&rest);
        network
    }

    /// Convert Tari Address to hex
    pub fn to_hex(&self) -> String {
        let buf = self.to_vec();
        buf.to_hex()
    }

    /// Creates Tari Address from hex
    pub fn from_hex(hex_str: &str) -> Result<TariAddress, TariAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        TariAddress::from_bytes(buf.as_slice())
    }
}

impl FromStr for TariAddress {
    type Err = TariAddressError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(address) = TariAddress::from_emoji_string(&key.trim().replace('|', "")) {
            Ok(address)
        } else if let Ok(address) = TariAddress::from_base58(key) {
            Ok(address)
        } else if let Ok(address) = TariAddress::from_hex(key) {
            Ok(address)
        } else {
            Err(TariAddressError::InvalidAddressString)
        }
    }
}

impl Display for TariAddress {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        fmt.write_str(&self.to_emoji_string())
    }
}

impl Default for TariAddress {
    fn default() -> Self {
        Self::Dual(DualAddress::default())
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::keys::{PublicKey as pk, SecretKey};

    use super::*;
    use crate::{dammsum::compute_checksum, types::PrivateKey};

    #[test]
    /// Test valid single tari address
    fn valid_emoji_id_single() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key =
            TariAddress::new_single_address_with_interactive_only(public_key.clone(), Network::Esmeralda);
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &public_key);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_only());

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_SINGLE_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TariAddress::new_single_address(
            public_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
        );
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &public_key);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_only());

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_SINGLE_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TariAddress::new_single_address(
            public_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
        );
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &public_key);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_one_sided_only());

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_SINGLE_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);
    }

    #[test]
    /// Test valid dual tari address
    fn valid_emoji_id_dual() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        );
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), Some(&view_key));

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_and_one_sided());

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), Some(&view_key));

        // Generate random public key
        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TariAddress::new_dual_address(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
        );
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), Some(&view_key));

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_only());

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), Some(&view_key));

        // Generate random public key
        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TariAddress::new_dual_address(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
        );
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), Some(&view_key));

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_one_sided_only());

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), Some(&view_key));
    }

    #[test]
    /// Test encoding for single tari address
    fn encoding_single() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::new_single_address_with_interactive_only(public_key.clone(), Network::Esmeralda);

        let buff = address.to_vec();
        let base58 = address.to_base58();
        let hex = address.to_hex();
        let emoji = address.to_emoji_string();

        let address_buff = TariAddress::from_bytes(&buff);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_buff = TariAddress::from_bytes(&buff).unwrap();
        assert_eq!(address_buff.public_spend_key(), address.public_spend_key());
        assert_eq!(address_buff.network(), address.network());
        assert_eq!(address_buff.features(), address.features());

        let address_base58 = TariAddress::from_base58(&base58).unwrap();
        assert_eq!(address_base58.public_spend_key(), address.public_spend_key());
        assert_eq!(address_base58.network(), address.network());
        assert_eq!(address_base58.features(), address.features());

        let address_hex = TariAddress::from_hex(&hex).unwrap();
        assert_eq!(address_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_hex.network(), address.network());
        assert_eq!(address_hex.features(), address.features());

        let address_emoji = TariAddress::from_emoji_string(&emoji).unwrap();
        assert_eq!(address_emoji.public_spend_key(), address.public_spend_key());
        assert_eq!(address_emoji.network(), address.network());
        assert_eq!(address_emoji.features(), address.features());

        let address_base58_string = TariAddress::from_str(&base58).unwrap();
        assert_eq!(address_base58_string, address_base58);
        let address_hex_string = TariAddress::from_str(&hex).unwrap();
        assert_eq!(address_hex_string, address_hex);
        let address_emoji_string = TariAddress::from_str(&emoji).unwrap();
        assert_eq!(address_emoji_string, address_emoji);

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::new_single_address(
            public_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
        );

        let buff = address.to_vec();
        let base58 = address.to_base58();
        let hex = address.to_hex();
        let emoji = address.to_emoji_string();

        let address_buff = TariAddress::from_bytes(&buff);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_buff = TariAddress::from_bytes(&buff).unwrap();
        assert_eq!(address_buff.public_spend_key(), address.public_spend_key());
        assert_eq!(address_buff.network(), address.network());
        assert_eq!(address_buff.features(), address.features());

        let address_base58 = TariAddress::from_base58(&base58).unwrap();
        assert_eq!(address_base58.public_spend_key(), address.public_spend_key());
        assert_eq!(address_base58.network(), address.network());
        assert_eq!(address_base58.features(), address.features());

        let address_hex = TariAddress::from_hex(&hex).unwrap();
        assert_eq!(address_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_hex.network(), address.network());
        assert_eq!(address_hex.features(), address.features());

        let address_emoji = TariAddress::from_emoji_string(&emoji).unwrap();
        assert_eq!(address_emoji.public_spend_key(), address.public_spend_key());
        assert_eq!(address_emoji.network(), address.network());
        assert_eq!(address_emoji.features(), address.features());

        let address_base58_string = TariAddress::from_str(&base58).unwrap();
        assert_eq!(address_base58_string, address_base58);
        let address_hex_string = TariAddress::from_str(&hex).unwrap();
        assert_eq!(address_hex_string, address_hex);
        let address_emoji_string = TariAddress::from_str(&emoji).unwrap();
        assert_eq!(address_emoji_string, address_emoji);
        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::new_single_address(
            public_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
        );

        let buff = address.to_vec();
        let base58 = address.to_base58();
        let hex = address.to_hex();
        let emoji = address.to_emoji_string();

        let address_buff = TariAddress::from_bytes(&buff);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_buff = TariAddress::from_bytes(&buff).unwrap();
        assert_eq!(address_buff.public_spend_key(), address.public_spend_key());
        assert_eq!(address_buff.network(), address.network());
        assert_eq!(address_buff.features(), address.features());

        let address_base58 = TariAddress::from_base58(&base58).unwrap();
        assert_eq!(address_base58.public_spend_key(), address.public_spend_key());
        assert_eq!(address_base58.network(), address.network());
        assert_eq!(address_base58.features(), address.features());

        let address_hex = TariAddress::from_hex(&hex).unwrap();
        assert_eq!(address_hex.public_spend_key(), address.public_spend_key());
        assert_eq!(address_hex.network(), address.network());
        assert_eq!(address_hex.features(), address.features());

        let address_emoji = TariAddress::from_emoji_string(&emoji).unwrap();
        assert_eq!(address_emoji.public_spend_key(), address.public_spend_key());
        assert_eq!(address_emoji.network(), address.network());
        assert_eq!(address_emoji.features(), address.features());

        let address_base58_string = TariAddress::from_str(&base58).unwrap();
        assert_eq!(address_base58_string, address_base58);
        let address_hex_string = TariAddress::from_str(&hex).unwrap();
        assert_eq!(address_hex_string, address_hex);
        let address_emoji_string = TariAddress::from_str(&emoji).unwrap();
        assert_eq!(address_emoji_string, address_emoji);
    }

    #[test]
    /// Test encoding for dual tari address
    fn encoding_dual() {
        fn test_addres(address: TariAddress) {
            let buff = address.to_vec();
            let base58 = address.to_base58();
            let hex = address.to_hex();
            let emoji = address.to_emoji_string();

            let address_buff = TariAddress::from_bytes(&buff).unwrap();
            assert_eq!(address_buff.public_spend_key(), address.public_spend_key());
            assert_eq!(address_buff.public_view_key(), address.public_view_key());
            assert_eq!(address_buff.network(), address.network());
            assert_eq!(address_buff.features(), address.features());

            let address_base58 = TariAddress::from_base58(&base58).unwrap();
            assert_eq!(address_base58.public_spend_key(), address.public_spend_key());
            assert_eq!(address_base58.public_view_key(), address.public_view_key());
            assert_eq!(address_base58.network(), address.network());
            assert_eq!(address_base58.features(), address.features());

            let address_hex = TariAddress::from_hex(&hex).unwrap();
            assert_eq!(address_hex.public_spend_key(), address.public_spend_key());
            assert_eq!(address_hex.public_view_key(), address.public_view_key());
            assert_eq!(address_hex.network(), address.network());
            assert_eq!(address_hex.features(), address.features());

            let address_emoji = TariAddress::from_emoji_string(&emoji).unwrap();
            assert_eq!(address_emoji.public_spend_key(), address.public_spend_key());
            assert_eq!(address_emoji.public_view_key(), address.public_view_key());
            assert_eq!(address_emoji.network(), address.network());
            assert_eq!(address_emoji.features(), address.features());

            let address_base58_string = TariAddress::from_str(&base58).unwrap();
            assert_eq!(address_base58_string, address_base58);
            let address_hex_string = TariAddress::from_str(&hex).unwrap();
            assert_eq!(address_hex_string, address_hex);
            let address_emoji_string = TariAddress::from_str(&emoji).unwrap();
            assert_eq!(address_emoji_string, address_emoji);
        }
        // Generate random public key
        let mut rng = rand::thread_rng();
        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::new_dual_address_with_default_features(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
        );
        test_addres(address);

        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::new_dual_address(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
        );
        test_addres(address);

        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::new_dual_address(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
        );
        test_addres(address);
    }

    #[test]
    /// Test invalid size
    fn invalid_size() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🦋🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🎒🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
        // This emoji string is too long to be a valid emoji ID
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🦋🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🎒🎒🎒🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );

        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👃🐛🎼🛵🔮💋👙💦🍷👠🦀🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
        // This emoji string is too long to be a valid emoji ID
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👃🐛🎼🛵🔮💋👙💦🍷👠🦀🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁👂🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
    }

    #[test]
    /// Test invalid emoji
    fn invalid_emoji() {
        // This emoji string contains an invalid emoji character
        let emoji_string = "🍗🌊🐉🦋🎪👛🌲🐭🦂🔨💺🎺🌕💦🚨🎼🍪⏰🍬🍚🎱💳🔱🐵🛵💡📱🌻📎🎻🐌😎👙🎹🎅";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidEmoji)
        );
    }

    #[test]
    /// Test invalid checksum
    fn invalid_checksum() {
        // This emoji string contains an invalid checksum
        let emoji_string = "🍗🌈🚓🧲📌🐺🐣🙈💰🍇🎓👂📈⚽🚧🚧🚢🍫💋👽🌈🎪🚽🍪🎳💼🙈🎪😎🏠🎳👍📷🎲🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidChecksum)
        );

        // This emoji string contains an invalid checksum
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👃🐛🎼🛵🔮💋👙💦🍷👠🦀🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidChecksum)
        );
    }

    #[test]
    /// Test invalid network
    fn invalid_network() {
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an address using a valid network and ensure it's not valid on another network
        let address = SingleAddress::new_with_interactive_only(public_key, Network::Esmeralda);
        let mut bytes = address.to_bytes();
        // this is an invalid network
        bytes[0] = 123;
        let checksum = compute_checksum(&bytes[0..34]);
        bytes[34] = checksum;
        assert_eq!(TariAddress::from_bytes(&bytes), Err(TariAddressError::InvalidNetwork));

        let view_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an address using a valid network and ensure it's not valid on another network
        let address = TariAddress::new_dual_address_with_default_features(view_key, spend_key, Network::Esmeralda);
        let mut bytes = address.to_vec();
        // this is an invalid network
        bytes[0] = 123;
        let checksum = compute_checksum(&bytes[0..66]);
        bytes[66] = checksum;
        assert_eq!(TariAddress::from_bytes(&bytes), Err(TariAddressError::InvalidNetwork));
    }

    #[test]
    /// Test invalid public key
    fn invalid_public_key() {
        let mut bytes = [0; 35].to_vec();
        bytes[0] = Network::Esmeralda.as_byte();
        bytes[1] = TariAddressFeatures::create_interactive_and_one_sided().0;
        bytes[2] = 1;
        let checksum = compute_checksum(&bytes[0..34]);
        bytes[34] = checksum;
        let emoji_string = bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>();

        // This emoji string contains an invalid checksum
        assert_eq!(
            SingleAddress::from_emoji_string(&emoji_string),
            Err(TariAddressError::CannotRecoverPublicKey)
        );

        let mut bytes = [0; 67].to_vec();
        bytes[0] = Network::Esmeralda.as_byte();
        bytes[1] = TariAddressFeatures::create_interactive_and_one_sided().0;
        bytes[2] = 1;
        let checksum = compute_checksum(&bytes[0..66]);
        bytes[66] = checksum;
        let emoji_string = bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>();

        // This emoji string contains an invalid checksum
        assert_eq!(
            DualAddress::from_emoji_string(&emoji_string),
            Err(TariAddressError::CannotRecoverPublicKey)
        );
    }
}
