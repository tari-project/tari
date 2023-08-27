// Copyright 2020. The Taiji Project
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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use taiji_common::configuration::Network;
use tari_crypto::tari_utilities::ByteArray;
use tari_utilities::hex::{from_hex, Hex};
use thiserror::Error;

use crate::{
    dammsum::{compute_checksum, validate_checksum},
    emoji::{EMOJI, REVERSE_EMOJI},
    types::PublicKey,
};

const INTERNAL_SIZE: usize = 33; // number of bytes used for the internal representation

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TaijiAddress {
    network: Network,
    public_key: PublicKey,
}

#[derive(Debug, Error, PartialEq)]
pub enum TaijiAddressError {
    #[error("Invalid size")]
    InvalidSize,
    #[error("Invalid network or checksum")]
    InvalidNetworkOrChecksum,
    #[error("Invalid emoji character")]
    InvalidEmoji,
    #[error("Cannot recover public key")]
    CannotRecoverPublicKey,
}

impl TaijiAddress {
    /// Creates a new Taiji Address from the provided public key and network while using the current version
    pub fn new(public_key: PublicKey, network: Network) -> Self {
        TaijiAddress { network, public_key }
    }

    /// helper function to convert emojis to u8
    fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, TaijiAddressError> {
        // The string must be the correct size, including the checksum
        if emoji.chars().count() != INTERNAL_SIZE {
            return Err(TaijiAddressError::InvalidSize);
        }

        // Convert the emoji string to a byte array
        let mut bytes = Vec::<u8>::with_capacity(INTERNAL_SIZE);
        for c in emoji.chars() {
            if let Some(i) = REVERSE_EMOJI.get(&c) {
                bytes.push(*i);
            } else {
                return Err(TaijiAddressError::InvalidEmoji);
            }
        }
        Ok(bytes)
    }

    /// Construct an TaijiAddress from an emoji string with checksum and network
    pub fn from_emoji_string_with_network(emoji: &str, network: Network) -> Result<Self, TaijiAddressError> {
        let bytes = TaijiAddress::emoji_to_bytes(emoji)?;

        TaijiAddress::from_bytes_with_network(&bytes, network)
    }

    /// Construct an TaijiAddress from an emoji string with checksum trying to calculate the network
    pub fn from_emoji_string(emoji: &str) -> Result<Self, TaijiAddressError> {
        let bytes = TaijiAddress::emoji_to_bytes(emoji)?;

        TaijiAddress::from_bytes(&bytes)
    }

    /// Construct an Taiji Address from a public key
    pub fn from_public_key(public_key: &PublicKey, network: Network) -> Self {
        Self {
            network,
            public_key: public_key.clone(),
        }
    }

    /// Gets the network from the Taiji Address
    pub fn network(&self) -> Network {
        self.network
    }

    /// Convert Taiji Address to an emoji string with checksum
    pub fn to_emoji_string(&self) -> String {
        // Convert the public key to bytes and compute the checksum
        let bytes = self.to_bytes();
        bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>()
    }

    /// Return the public key of an Taiji Address
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// Construct Taiji Address from bytes with network
    pub fn from_bytes_with_network(bytes: &[u8], network: Network) -> Result<TaijiAddress, TaijiAddressError>
    where Self: Sized {
        if bytes.len() != INTERNAL_SIZE {
            return Err(TaijiAddressError::InvalidSize);
        }
        let mut fixed_data = bytes.to_vec();
        fixed_data[32] ^= network.as_byte();
        // Assert the checksum is valid
        if validate_checksum(&fixed_data).is_err() {
            return Err(TaijiAddressError::InvalidNetworkOrChecksum);
        }
        let key = PublicKey::from_bytes(&bytes[0..32]).map_err(|_| TaijiAddressError::CannotRecoverPublicKey)?;
        Ok(TaijiAddress {
            public_key: key,
            network,
        })
    }

    /// Construct Taiji Address from bytes and try to calculate the network
    pub fn from_bytes(bytes: &[u8]) -> Result<TaijiAddress, TaijiAddressError>
    where Self: Sized {
        if bytes.len() != INTERNAL_SIZE {
            return Err(TaijiAddressError::InvalidSize);
        }
        let checksum = compute_checksum(&bytes[0..32].to_vec());
        // if the network is a valid network number, we can assume that the checksum as valid
        let network =
            Network::try_from(checksum ^ bytes[32]).map_err(|_| TaijiAddressError::InvalidNetworkOrChecksum)?;
        let key = PublicKey::from_bytes(&bytes[0..32]).map_err(|_| TaijiAddressError::CannotRecoverPublicKey)?;
        Ok(TaijiAddress {
            public_key: key,
            network,
        })
    }

    /// Convert Taiji Address to bytes
    pub fn to_bytes(&self) -> [u8; INTERNAL_SIZE] {
        let mut buf = [0u8; INTERNAL_SIZE];
        buf[0..32].copy_from_slice(self.public_key.as_bytes());
        let checksum = compute_checksum(&buf[0..32].to_vec());
        buf[32] = self.network.as_byte() ^ checksum;
        buf
    }

    /// Construct Taiji Address from hex with network
    pub fn from_hex_with_network(hex_str: &str, network: Network) -> Result<TaijiAddress, TaijiAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TaijiAddressError::CannotRecoverPublicKey)?;
        TaijiAddress::from_bytes_with_network(buf.as_slice(), network)
    }

    /// Construct Taiji Address from hex  and try to calculate the network
    pub fn from_hex(hex_str: &str) -> Result<TaijiAddress, TaijiAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TaijiAddressError::CannotRecoverPublicKey)?;
        TaijiAddress::from_bytes(buf.as_slice())
    }

    /// Convert Taiji Address to bytes
    pub fn to_hex(&self) -> String {
        let buf = self.to_bytes();
        buf.to_hex()
    }
}

impl FromStr for TaijiAddress {
    type Err = TaijiAddressError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(address) = TaijiAddress::from_emoji_string(&key.trim().replace('|', "")) {
            Ok(address)
        } else if let Ok(address) = TaijiAddress::from_hex(key) {
            Ok(address)
        } else {
            Err(TaijiAddressError::CannotRecoverPublicKey)
        }
    }
}

impl Display for TaijiAddress {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        fmt.write_str(&self.to_emoji_string())
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::keys::{PublicKey, SecretKey};

    use super::*;
    use crate::types::PrivateKey;

    #[test]
    /// Test valid taiji address
    fn valid_emoji_id() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TaijiAddress::from_public_key(&public_key, Network::Esmeralda);
        assert_eq!(emoji_id_from_public_key.public_key(), &public_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TaijiAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_key(), &public_key);
    }

    #[test]
    /// Test encoding for taiji address
    fn encoding() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TaijiAddress::from_public_key(&public_key, Network::Esmeralda);

        let buff = address.to_bytes();
        let hex = address.to_hex();

        let address_buff = TaijiAddress::from_bytes(&buff);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_buff = TaijiAddress::from_bytes_with_network(&buff, Network::Esmeralda);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_hex = TaijiAddress::from_hex(&hex);
        assert_eq!(address_hex, Ok(address.clone()));

        let address_hex = TaijiAddress::from_hex_with_network(&hex, Network::Esmeralda);
        assert_eq!(address_hex, Ok(address));
    }

    #[test]
    /// Test invalid size
    fn invalid_size() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒";
        assert_eq!(
            TaijiAddress::from_emoji_string(emoji_string),
            Err(TaijiAddressError::InvalidSize)
        );
        // This emoji string is too long to be a valid emoji ID
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒🎒🎒🎒🎒";
        assert_eq!(
            TaijiAddress::from_emoji_string(emoji_string),
            Err(TaijiAddressError::InvalidSize)
        );
    }

    #[test]
    /// Test invalid emoji
    fn invalid_emoji() {
        // This emoji string contains an invalid emoji character
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒🎅";
        assert_eq!(
            TaijiAddress::from_emoji_string(emoji_string),
            Err(TaijiAddressError::InvalidEmoji)
        );
    }

    #[test]
    /// Test invalid checksum
    fn invalid_checksum() {
        // This emoji string contains an invalid checksum
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒🎒";
        assert_eq!(
            TaijiAddress::from_emoji_string(emoji_string),
            Err(TaijiAddressError::InvalidNetworkOrChecksum)
        );
    }

    #[test]
    /// Test invalid network
    fn invalid_network() {
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an address using a valid network and ensure it's not valid on another network
        let address = TaijiAddress::from_public_key(&public_key, Network::Esmeralda);
        assert_eq!(
            TaijiAddress::from_bytes_with_network(&address.to_bytes(), Network::Igor),
            Err(TaijiAddressError::InvalidNetworkOrChecksum)
        );

        // Generate an address using a valid network, mutate it, and ensure it's not valid on the same network
        let mut address_bytes = address.to_bytes();
        address_bytes[32] ^= 0xFF;
        assert_eq!(
            TaijiAddress::from_bytes_with_network(&address_bytes, Network::Esmeralda),
            Err(TaijiAddressError::InvalidNetworkOrChecksum)
        );
    }

    #[test]
    /// Test invalid public key
    fn invalid_public_key() {
        let mut bytes = [0; 33].to_vec();
        bytes[0] = 1;
        let checksum = compute_checksum(&bytes[0..32].to_vec());
        bytes[32] = Network::Esmeralda.as_byte() ^ checksum;
        let emoji_string = bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>();

        // This emoji string contains an invalid checksum
        assert_eq!(
            TaijiAddress::from_emoji_string(&emoji_string),
            Err(TaijiAddressError::CannotRecoverPublicKey)
        );
    }
}
