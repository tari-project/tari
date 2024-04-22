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

use std::{
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
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
pub struct TariAddress {
    network: Network,
    public_key: PublicKey,
}

#[derive(Debug, Error, PartialEq)]
pub enum TariAddressError {
    #[error("Invalid size")]
    InvalidSize,
    #[error("Invalid network or checksum")]
    InvalidNetworkOrChecksum,
    #[error("Invalid emoji character")]
    InvalidEmoji,
    #[error("Cannot recover public key")]
    CannotRecoverPublicKey,
}

impl TariAddress {
    /// Creates a new Tari Address from the provided public key and network while using the current version
    pub fn new(public_key: PublicKey, network: Network) -> Self {
        TariAddress { network, public_key }
    }

    /// helper function to convert emojis to u8
    fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, TariAddressError> {
        // The string must be the correct size, including the checksum
        if emoji.chars().count() != INTERNAL_SIZE {
            return Err(TariAddressError::InvalidSize);
        }

        // Convert the emoji string to a byte array
        let mut bytes = Vec::<u8>::with_capacity(INTERNAL_SIZE);
        for c in emoji.chars() {
            if let Some(i) = REVERSE_EMOJI.get(&c) {
                bytes.push(*i);
            } else {
                return Err(TariAddressError::InvalidEmoji);
            }
        }
        Ok(bytes)
    }

    /// Construct an TariAddress from an emoji string with checksum and network
    pub fn from_emoji_string_with_network(emoji: &str, network: Network) -> Result<Self, TariAddressError> {
        let bytes = TariAddress::emoji_to_bytes(emoji)?;

        TariAddress::from_bytes_with_network(&bytes, network)
    }

    /// Construct an TariAddress from an emoji string with checksum trying to calculate the network
    pub fn from_emoji_string(emoji: &str) -> Result<Self, TariAddressError> {
        let bytes = TariAddress::emoji_to_bytes(emoji)?;

        TariAddress::from_bytes(&bytes)
    }

    /// Construct an Tari Address from a public key
    pub fn from_public_key(public_key: &PublicKey, network: Network) -> Self {
        Self {
            network,
            public_key: public_key.clone(),
        }
    }

    /// Gets the network from the Tari Address
    pub fn network(&self) -> Network {
        self.network
    }

    /// Convert Tari Address to an emoji string with checksum
    pub fn to_emoji_string(&self) -> String {
        // Convert the public key to bytes and compute the checksum
        let bytes = self.to_bytes();
        bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>()
    }

    /// Return the public key of an Tari Address
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    /// Construct Tari Address from bytes with network
    pub fn from_bytes_with_network(bytes: &[u8], network: Network) -> Result<TariAddress, TariAddressError>
    where Self: Sized {
        if bytes.len() != INTERNAL_SIZE {
            return Err(TariAddressError::InvalidSize);
        }
        let mut fixed_data = bytes.to_vec();
        fixed_data[32] ^= network.as_byte();
        // Assert the checksum is valid
        if validate_checksum(&fixed_data).is_err() {
            return Err(TariAddressError::InvalidNetworkOrChecksum);
        }
        let key =
            PublicKey::from_canonical_bytes(&bytes[0..32]).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        Ok(TariAddress {
            public_key: key,
            network,
        })
    }

    /// Construct Tari Address from bytes and try to calculate the network
    pub fn from_bytes(bytes: &[u8]) -> Result<TariAddress, TariAddressError>
    where Self: Sized {
        if bytes.len() != INTERNAL_SIZE {
            return Err(TariAddressError::InvalidSize);
        }
        let checksum = compute_checksum(&bytes[0..32]);
        // if the network is a valid network number, we can assume that the checksum as valid
        let network =
            Network::try_from(checksum ^ bytes[32]).map_err(|_| TariAddressError::InvalidNetworkOrChecksum)?;
        let key =
            PublicKey::from_canonical_bytes(&bytes[0..32]).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        Ok(TariAddress {
            public_key: key,
            network,
        })
    }

    /// Convert Tari Address to bytes
    pub fn to_bytes(&self) -> [u8; INTERNAL_SIZE] {
        let mut buf = [0u8; INTERNAL_SIZE];
        buf[0..32].copy_from_slice(self.public_key.as_bytes());
        let checksum = compute_checksum(&buf[0..32]);
        buf[32] = self.network.as_byte() ^ checksum;
        buf
    }

    /// Construct Tari Address from hex with network
    pub fn from_hex_with_network(hex_str: &str, network: Network) -> Result<TariAddress, TariAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        TariAddress::from_bytes_with_network(buf.as_slice(), network)
    }

    /// Construct Tari Address from hex  and try to calculate the network
    pub fn from_hex(hex_str: &str) -> Result<TariAddress, TariAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        TariAddress::from_bytes(buf.as_slice())
    }

    /// Convert Tari Address to bytes
    pub fn to_hex(&self) -> String {
        let buf = self.to_bytes();
        buf.to_hex()
    }
}

impl FromStr for TariAddress {
    type Err = TariAddressError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(address) = TariAddress::from_emoji_string(&key.trim().replace('|', "")) {
            Ok(address)
        } else if let Ok(address) = TariAddress::from_hex(key) {
            Ok(address)
        } else {
            Err(TariAddressError::CannotRecoverPublicKey)
        }
    }
}

impl Display for TariAddress {
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
    /// Test valid tari address
    fn valid_emoji_id() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = TariAddress::from_public_key(&public_key, Network::Esmeralda);
        assert_eq!(emoji_id_from_public_key.public_key(), &public_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = TariAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_key(), &public_key);
    }

    #[test]
    /// Test encoding for tari address
    fn encoding() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = TariAddress::from_public_key(&public_key, Network::Esmeralda);

        let buff = address.to_bytes();
        let hex = address.to_hex();

        let address_buff = TariAddress::from_bytes(&buff);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_buff = TariAddress::from_bytes_with_network(&buff, Network::Esmeralda);
        assert_eq!(address_buff, Ok(address.clone()));

        let address_hex = TariAddress::from_hex(&hex);
        assert_eq!(address_hex, Ok(address.clone()));

        let address_hex = TariAddress::from_hex_with_network(&hex, Network::Esmeralda);
        assert_eq!(address_hex, Ok(address));
    }

    #[test]
    /// Test invalid size
    fn invalid_size() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
        // This emoji string is too long to be a valid emoji ID
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒🎒🎒🎒🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
    }

    #[test]
    /// Test invalid emoji
    fn invalid_emoji() {
        // This emoji string contains an invalid emoji character
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒🎅";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidEmoji)
        );
    }

    #[test]
    /// Test invalid checksum
    fn invalid_checksum() {
        // This emoji string contains an invalid checksum
        let emoji_string = "🌴🐩🔌📌🚑🌰🎓🌴🐊🐌💕💡🐜📉👛🍵👛🐽🎂🐻🌀🍓😿🐭🐼🏀🎪💔💸🍅🔋🎒🎒";
        assert_eq!(
            TariAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidNetworkOrChecksum)
        );
    }

    #[test]
    /// Test invalid network
    fn invalid_network() {
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an address using a valid network and ensure it's not valid on another network
        let address = TariAddress::from_public_key(&public_key, Network::Esmeralda);
        assert_eq!(
            TariAddress::from_bytes_with_network(&address.to_bytes(), Network::Igor),
            Err(TariAddressError::InvalidNetworkOrChecksum)
        );

        // Generate an address using a valid network, mutate it, and ensure it's not valid on the same network
        let mut address_bytes = address.to_bytes();
        address_bytes[32] ^= 0xFF;
        assert_eq!(
            TariAddress::from_bytes_with_network(&address_bytes, Network::Esmeralda),
            Err(TariAddressError::InvalidNetworkOrChecksum)
        );
    }

    #[test]
    /// Test invalid public key
    fn invalid_public_key() {
        let mut bytes = [0; 33].to_vec();
        bytes[0] = 1;
        let checksum = compute_checksum(&bytes[0..32]);
        bytes[32] = Network::Esmeralda.as_byte() ^ checksum;
        let emoji_string = bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>();

        // This emoji string contains an invalid checksum
        assert_eq!(
            TariAddress::from_emoji_string(&emoji_string),
            Err(TariAddressError::CannotRecoverPublicKey)
        );
    }
}
