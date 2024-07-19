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

use std::{convert::TryFrom, panic};

use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_crypto::tari_utilities::ByteArray;
use tari_utilities::hex::{from_hex, Hex};

use crate::{
    dammsum::{compute_checksum, validate_checksum},
    emoji::{EMOJI, REVERSE_EMOJI},
    tari_address::{TariAddressError, TariAddressFeatures, INTERNAL_SINGLE_SIZE},
    types::PublicKey,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SingleAddress {
    network: Network,
    features: TariAddressFeatures,
    public_spend_key: PublicKey,
}

impl SingleAddress {
    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new(spend_key: PublicKey, network: Network, features: TariAddressFeatures) -> SingleAddress {
        Self {
            network,
            features,
            public_spend_key: spend_key,
        }
    }

    /// Creates a new Tari Address from the provided public keys and network while using the default features
    pub fn new_with_interactive_only(spend_key: PublicKey, network: Network) -> SingleAddress {
        Self {
            network,
            features: TariAddressFeatures::create_interactive_only(),
            public_spend_key: spend_key,
        }
    }

    /// helper function to convert emojis to u8
    pub fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, TariAddressError> {
        // The string must be the correct size, including the checksum
        if emoji.chars().count() != INTERNAL_SINGLE_SIZE {
            return Err(TariAddressError::InvalidSize);
        }

        // Convert the emoji string to a byte array
        let mut bytes = Vec::<u8>::with_capacity(INTERNAL_SINGLE_SIZE);
        for c in emoji.chars() {
            if let Some(i) = REVERSE_EMOJI.get(&c) {
                bytes.push(*i);
            } else {
                return Err(TariAddressError::InvalidEmoji);
            }
        }
        Ok(bytes)
    }

    /// Construct an TariAddress from an emoji string
    pub fn from_emoji_string(emoji: &str) -> Result<Self, TariAddressError> {
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
        let bytes = self.to_bytes();
        bytes.iter().map(|b| EMOJI[*b as usize]).collect::<String>()
    }

    /// Return the public spend key of a Tari Address
    pub fn public_spend_key(&self) -> &PublicKey {
        &self.public_spend_key
    }

    /// Construct Tari Address from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TariAddressError>
    where Self: Sized {
        if bytes.len() != INTERNAL_SINGLE_SIZE {
            return Err(TariAddressError::InvalidSize);
        }
        if validate_checksum(bytes).is_err() {
            return Err(TariAddressError::InvalidChecksum);
        }
        let network = Network::try_from(bytes[0]).map_err(|_| TariAddressError::InvalidNetwork)?;
        let features = TariAddressFeatures::from_bits(bytes[1]).ok_or(TariAddressError::InvalidFeatures)?;
        let public_spend_key =
            PublicKey::from_canonical_bytes(&bytes[2..34]).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        Ok(Self {
            network,
            features,
            public_spend_key,
        })
    }

    /// Convert Tari Address to bytes
    pub fn to_bytes(&self) -> [u8; INTERNAL_SINGLE_SIZE] {
        let mut buf = [0u8; INTERNAL_SINGLE_SIZE];
        buf[0] = self.network.as_byte();
        buf[1] = self.features.0;
        buf[2..34].copy_from_slice(self.public_spend_key.as_bytes());
        let checksum = compute_checksum(&buf[0..34]);
        buf[34] = checksum;
        buf
    }

    /// Construct Tari Address from Base58
    pub fn from_base58(hex_str: &str) -> Result<Self, TariAddressError> {
        // Due to the byte length, it can be encoded as 46, 47 or 48 chars
        if hex_str.len() != 46 && hex_str.len() != 47 && hex_str.len() != 48 {
            return Err(TariAddressError::InvalidSize);
        }
        let result = panic::catch_unwind(|| hex_str.split_at(2));
        let (first, rest) = match result {
            Ok((first, rest)) => (first, rest),
            Err(_) => return Err(TariAddressError::InvalidCharacter),
        };
        let result = panic::catch_unwind(|| first.split_at(1));
        let (network, features) = match result {
            Ok((network, features)) => (network, features),
            Err(_) => return Err(TariAddressError::InvalidCharacter),
        };
        // let (first, rest) = hex_str.split_at_checked(2).ok_or(TariAddressError::InvalidCharacter)?;
        // let (network, features) = first.split_at_checked(1).ok_or(TariAddressError::InvalidCharacter)?;
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

    /// Convert Tari Address to Base58
    pub fn to_base58(&self) -> String {
        let bytes = self.to_bytes();
        let mut network = bs58::encode(&bytes[0..1]).into_string();
        let features = bs58::encode(&bytes[1..2].to_vec()).into_string();
        let rest = bs58::encode(&bytes[2..]).into_string();
        network.push_str(&features);
        network.push_str(&rest);
        network
    }

    /// Convert Tari single Address to hex
    pub fn to_hex(&self) -> String {
        let buf = self.to_bytes();
        buf.to_hex()
    }

    /// Creates Tari single Address from hex
    pub fn from_hex(hex_str: &str) -> Result<SingleAddress, TariAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        SingleAddress::from_bytes(buf.as_slice())
    }
}
#[cfg(test)]
mod test {
    use tari_crypto::keys::{PublicKey as pk, SecretKey};

    use super::*;
    use crate::types::PrivateKey;

    #[test]
    /// Test valid single tari address
    fn valid_emoji_id() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = SingleAddress::new_with_interactive_only(public_key.clone(), Network::Esmeralda);
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &public_key);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_only());

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), INTERNAL_SINGLE_SIZE);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = SingleAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = SingleAddress::new(
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
        let emoji_id_from_emoji_string = SingleAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = SingleAddress::new(
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
        let emoji_id_from_emoji_string = SingleAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &public_key);
    }

    #[test]
    /// Test encoding for single tari address
    fn encoding() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = SingleAddress::new_with_interactive_only(public_key.clone(), Network::Esmeralda);

        let buff = address.to_bytes();
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

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = SingleAddress::new(
            public_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
        );

        let buff = address.to_bytes();
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

        // Generate random public key
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = SingleAddress::new(
            public_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
        );

        let buff = address.to_bytes();
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
    /// Test invalid size
    fn invalid_size() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🦋🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🎒🎒";
        assert_eq!(
            SingleAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
        // This emoji string is too long to be a valid emoji ID
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🦋🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🎒🎒🎒🎒";
        assert_eq!(
            SingleAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
    }

    #[test]
    /// Test invalid emoji
    fn invalid_emoji() {
        // This emoji string contains an invalid emoji character
        let emoji_string = "🍗🌊🐉🦋🎪👛🌲🐭🦂🔨💺🎺🌕💦🚨🎼🍪⏰🍬🍚🎱💳🔱🐵🛵💡📱🌻📎🎻🐌😎👙🎹🎅";
        assert_eq!(
            SingleAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidEmoji)
        );
    }

    #[test]
    /// Test invalid features
    fn invalid_features() {
        let mut rng = rand::thread_rng();
        let spend_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let mut address = SingleAddress::new_with_interactive_only(spend_key.clone(), Network::Esmeralda);
        address.features = TariAddressFeatures(5);

        let emoji_string = address.to_emoji_string();
        assert_eq!(
            SingleAddress::from_emoji_string(&emoji_string),
            Err(TariAddressError::InvalidFeatures)
        );
    }

    #[test]
    /// Test invalid checksum
    fn invalid_checksum() {
        // This emoji string contains an invalid checksum
        let emoji_string = "🍗🌈🚓🧲📌🐺🐣🙈💰🍇🎓👂📈⚽🚧🚧🚢🍫💋👽🌈🎪🚽🍪🎳💼🙈🎪😎🏠🎳👍📷🎲🎒";
        assert_eq!(
            SingleAddress::from_emoji_string(emoji_string),
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
        assert_eq!(SingleAddress::from_bytes(&bytes), Err(TariAddressError::InvalidNetwork));
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
    }
}
