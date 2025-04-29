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

use std::convert::TryFrom;

use serde::{Deserialize, Serialize};
use tari_common::configuration::Network;
use tari_crypto::tari_utilities::ByteArray;
use tari_max_size::MaxSizeBytes;
use tari_utilities::hex::{from_hex, Hex};

use crate::{
    dammsum::{compute_checksum, validate_checksum},
    emoji::{EMOJI, REVERSE_EMOJI},
    tari_address::{
        TariAddressError,
        TariAddressFeatures,
        INTERNAL_DUAL_BASE58_MAX_SIZE,
        INTERNAL_DUAL_BASE58_MIN_SIZE,
        MAX_ENCRYPTED_DATA_SIZE,
        TARI_ADDRESS_INTERNAL_DUAL_SIZE,
    },
    types::CompressedPublicKey,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DualAddress {
    network: Network,
    features: TariAddressFeatures,
    public_view_key: CompressedPublicKey,
    public_spend_key: CompressedPublicKey,
    payment_id_user_data: MaxSizeBytes<MAX_ENCRYPTED_DATA_SIZE>,
}

impl DualAddress {
    /// Creates a new Tari Address from the provided public keys, network and features
    pub fn new(
        view_key: CompressedPublicKey,
        spend_key: CompressedPublicKey,
        network: Network,
        features: TariAddressFeatures,
        payment_id_user_data: Option<Vec<u8>>,
    ) -> Result<DualAddress, TariAddressError> {
        let mut features = features;
        let payment_id_user_data = match payment_id_user_data {
            Some(data) => {
                if data.len() > MAX_ENCRYPTED_DATA_SIZE {
                    return Err(TariAddressError::PaymentIdTooLarge);
                }
                features.set(TariAddressFeatures::PAYMENT_ID, true);
                MaxSizeBytes::from_bytes_truncate(data)
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
    ) -> Result<DualAddress, TariAddressError> {
        Self::new(view_key, spend_key, network, TariAddressFeatures::default(), None)
    }

    pub fn add_payment_id(&mut self, data: Vec<u8>) -> Result<(), TariAddressError> {
        if data.len() > MAX_ENCRYPTED_DATA_SIZE {
            return Err(TariAddressError::PaymentIdTooLarge);
        }
        self.features.set(TariAddressFeatures::PAYMENT_ID, true);
        self.payment_id_user_data = MaxSizeBytes::from_bytes_truncate(data);
        Ok(())
    }

    /// helper function to convert emojis to u8
    pub fn emoji_to_bytes(emoji: &str) -> Result<Vec<u8>, TariAddressError> {
        // The string must be the correct size, including the checksum
        let length = emoji.chars().count();
        if !(TARI_ADDRESS_INTERNAL_DUAL_SIZE..=TARI_ADDRESS_INTERNAL_DUAL_SIZE + MAX_ENCRYPTED_DATA_SIZE)
            .contains(&length)
        {
            return Err(TariAddressError::InvalidSize);
        }
        // Convert the emoji string to a byte array
        let mut bytes = Vec::<u8>::with_capacity(TARI_ADDRESS_INTERNAL_DUAL_SIZE);
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

    pub fn get_payment_id_bytes(&self) -> Vec<u8> {
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
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TariAddressError>
    where Self: Sized {
        let length = bytes.len();
        if !(TARI_ADDRESS_INTERNAL_DUAL_SIZE..=TARI_ADDRESS_INTERNAL_DUAL_SIZE + MAX_ENCRYPTED_DATA_SIZE)
            .contains(&length)
        {
            return Err(TariAddressError::InvalidSize);
        }
        if validate_checksum(bytes).is_err() {
            return Err(TariAddressError::InvalidChecksum);
        }
        let network = Network::try_from(bytes[0]).map_err(|_| TariAddressError::InvalidNetwork)?;
        let features = TariAddressFeatures::from_bits(bytes[1]).ok_or(TariAddressError::InvalidFeatures)?;
        let public_view_key = CompressedPublicKey::from_canonical_bytes(&bytes[2..34])
            .map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        let public_spend_key = CompressedPublicKey::from_canonical_bytes(&bytes[34..66])
            .map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
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
        buf[2..34].copy_from_slice(self.public_view_key.as_bytes());
        buf[34..66].copy_from_slice(self.public_spend_key.as_bytes());
        buf[66..(length - 1)].copy_from_slice(self.payment_id_user_data.as_bytes());
        let checksum = compute_checksum(&buf[0..(length - 1)]);
        buf[length - 1] = checksum;
        buf
    }

    /// Construct Tari Address from Base58
    pub fn from_base58(hex_str: &str) -> Result<Self, TariAddressError> {
        // Due to the byte length, it can be encoded as 90 or 91
        if hex_str.len() < INTERNAL_DUAL_BASE58_MIN_SIZE || hex_str.len() > INTERNAL_DUAL_BASE58_MAX_SIZE {
            return Err(TariAddressError::InvalidSize);
        }

        let (first, rest) = hex_str.split_at_checked(2).ok_or(TariAddressError::InvalidCharacter)?;
        let (network, features) = first.split_at_checked(1).ok_or(TariAddressError::InvalidCharacter)?;
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

    /// Convert Tari Address to Base58 string
    pub fn to_base58(&self) -> String {
        let bytes = self.to_vec();
        let mut base58 = "".to_string();
        base58.push_str(&bs58::encode(&bytes[0..1]).into_string());
        base58.push_str(&bs58::encode(&bytes[1..2].to_vec()).into_string());
        base58.push_str(&bs58::encode(&bytes[2..]).into_string());
        base58
    }

    /// Convert Tari dual Address to hex
    pub fn to_hex(&self) -> String {
        let buf = self.to_vec();
        buf.to_hex()
    }

    /// Creates Tari dual Address from hex
    pub fn from_hex(hex_str: &str) -> Result<DualAddress, TariAddressError> {
        let buf = from_hex(hex_str).map_err(|_| TariAddressError::CannotRecoverPublicKey)?;
        DualAddress::from_bytes(buf.as_slice())
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::keys::SecretKey;

    use super::*;
    use crate::types::PrivateKey;

    #[test]
    /// Test valid dual tari address
    fn valid_emoji_id() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key =
            DualAddress::new_with_default_features(view_key.clone(), spend_key.clone(), Network::Esmeralda).unwrap();
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_and_one_sided());
        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = DualAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);

        // Generate random public key
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = DualAddress::new(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
            None,
        )
        .unwrap();
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_interactive_only());

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = DualAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);

        // Generate random public key
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = DualAddress::new(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures::create_one_sided_only());

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = DualAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);
    }

    #[test]
    /// Test encoding for dual tari address
    fn encoding() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address =
            DualAddress::new_with_default_features(view_key.clone(), spend_key.clone(), Network::Esmeralda).unwrap();

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

        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = DualAddress::new(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_interactive_only(),
            None,
        )
        .unwrap();

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

        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let address = DualAddress::new(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap();

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
    /// Test invalid size
    fn invalid_size() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👃🐛🎼🛵🔮💋👙💦🍷👠🦀🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁";
        assert_eq!(
            DualAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
        // This emoji string is too long to be a valid emoji ID
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🐛👅🐔🐉🍍🥑🥑💔🚧💄🎥🎳🐛📌🚧🐊💄🎥🎓🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊🎥🎓🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👃🐛🎼🛵🔮💋👙💦🍷👠🦀👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁👂🎒";
        assert_eq!(
            DualAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidSize)
        );
    }

    #[test]
    /// Test invalid emoji
    fn invalid_emoji() {
        // This emoji string contains an invalid emoji character
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👃🐛🎼🛵🔮💋👙💦🍷👠🦀🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁🎅";
        assert_eq!(
            DualAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidEmoji)
        );
    }

    #[test]
    /// Test invalid checksum
    fn invalid_checksum() {
        // This emoji string contains an invalid checksum
        let emoji_string = "🍗🌊🦂🍎🐛🔱🍟🚦🦆👃🐛🎼🛵🔮💋👙💦🍷👠🦀🐺🍪🚀🎮🎩👅🐔🐉🍍🥑💔📌🚧🐊💄🎥🎓🚗🎳🐛🚿💉🌴🧢🐵🎩👾👽🎃🤡👍🔮👒👽🎵👀🚨😷🎒👂👶🍄🏰🚑🌸🍁🎒";
        assert_eq!(
            DualAddress::from_emoji_string(emoji_string),
            Err(TariAddressError::InvalidChecksum)
        );
    }

    #[test]
    /// Test invalid features
    fn invalid_features() {
        let mut rng = rand::thread_rng();
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let mut address =
            DualAddress::new_with_default_features(view_key.clone(), spend_key.clone(), Network::Esmeralda).unwrap();
        address.features = TariAddressFeatures(8);

        let emoji_string = address.to_emoji_string();
        assert_eq!(
            DualAddress::from_emoji_string(&emoji_string),
            Err(TariAddressError::InvalidFeatures)
        );
    }

    #[test]
    /// Test invalid network
    fn invalid_network() {
        let mut rng = rand::thread_rng();
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an address using a valid network and ensure it's not valid on another network
        let address = DualAddress::new_with_default_features(view_key, spend_key, Network::Esmeralda).unwrap();
        let mut bytes = address.to_vec();
        // this is an invalid network
        bytes[0] = 123;
        let checksum = compute_checksum(&bytes[0..66]);
        bytes[66] = checksum;
        assert_eq!(DualAddress::from_bytes(&bytes), Err(TariAddressError::InvalidNetwork));
    }

    #[test]
    fn valid_payment_id() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let payment_id = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let emoji_id_from_public_key = DualAddress::new(
            view_key.clone(),
            spend_key.clone(),
            Network::Esmeralda,
            TariAddressFeatures::default(),
            Some(payment_id.clone()),
        )
        .unwrap();
        assert_eq!(emoji_id_from_public_key.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_public_key.public_view_key(), &view_key);
        assert_eq!(
            emoji_id_from_public_key.payment_id_user_data.as_bytes(),
            payment_id.as_slice()
        );

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_emoji_string();
        assert_eq!(emoji_string.chars().count(), TARI_ADDRESS_INTERNAL_DUAL_SIZE + 8);

        let features = emoji_id_from_public_key.features();
        assert_eq!(features, TariAddressFeatures(7));
        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = DualAddress::from_emoji_string(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_emoji_string(), emoji_string);

        // Return to the original public keys for good measure
        assert_eq!(emoji_id_from_emoji_string.public_spend_key(), &spend_key);
        assert_eq!(emoji_id_from_emoji_string.public_view_key(), &view_key);
        assert_eq!(
            emoji_id_from_emoji_string.payment_id_user_data.as_bytes(),
            payment_id.as_slice()
        );
    }
}
