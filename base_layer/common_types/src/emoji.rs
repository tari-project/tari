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
    collections::HashMap,
    convert::TryFrom,
    fmt::{Display, Error, Formatter},
    iter,
    str::FromStr,
};

use once_cell::sync::Lazy;
use tari_crypto::tari_utilities::ByteArray;
use thiserror::Error;

use crate::{
    dammsum::{compute_checksum, validate_checksum, CHECKSUM_BYTES},
    types::PublicKey,
};

/// An emoji ID is a 33-character emoji representation of a public key that includes a checksum for safety.
/// Each character corresponds to a byte; the first 32 bytes are an encoding of the underlying public key.
/// The last byte is a DammSum checksum of all preceding bytes.
///
/// Because the emoji character set contains 256 elements, it is more compact (in character count, not necessarily
/// in display width!) than other common encodings would provide, and is in theory easier for humans to examine.
///
/// An emoji ID can be instantiated either from a public key or from a string of emoji characters, and can be
/// converted to either form as well. Checksum validation is done automatically on instantiation.
///
/// # Example
///
/// ```
/// use std::str::FromStr;
/// use tari_common_types::emoji::EmojiId;
///
/// // Construct an emoji ID from an emoji string (this can fail)
/// let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🐢🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🥊";
/// let emoji_id_from_emoji_string = EmojiId::from_str(emoji_string);
/// assert!(emoji_id_from_emoji_string.is_ok());
///
/// // Get the public key
/// let public_key = emoji_id_from_emoji_string.unwrap().as_public_key().clone();
///
/// // Reconstruct the emoji ID from the public key (this cannot fail)
/// let emoji_id_from_public_key = EmojiId::from(&public_key);
///
/// // An emoji ID is deterministic
/// assert_eq!(emoji_id_from_public_key.to_string(), emoji_string);
///
/// // Oh no! We swapped the first two emoji characters by mistake, so this should fail
/// let invalid_emoji_string = "🦀🌴🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🐢🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🥊";
/// assert!(EmojiId::from_str(invalid_emoji_string).is_err());
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EmojiId(PublicKey);

const DICT_SIZE: usize = 256; // number of elements in the symbol dictionary
const DATA_BYTES: usize = 32; // number of bytes used for the key data

// The emoji table, mapping byte values to emoji characters
pub const EMOJI: [char; DICT_SIZE] = [
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

// The reverse table, mapping emoji to characters to byte values
pub static REVERSE_EMOJI: Lazy<HashMap<char, u8>> = Lazy::new(|| {
    let mut m = HashMap::with_capacity(DICT_SIZE);
    EMOJI.iter().enumerate().for_each(|(i, c)| {
        m.insert(*c, u8::try_from(i).expect("Invalid emoji"));
    });
    m
});

/// Returns the current emoji set as a character array
pub const fn emoji_set() -> [char; DICT_SIZE] {
    EMOJI
}

#[derive(Debug, Error, PartialEq)]
pub enum EmojiIdError {
    #[error("Invalid size")]
    InvalidSize,
    #[error("Invalid emoji character")]
    InvalidEmoji,
    #[error("Invalid checksum")]
    InvalidChecksum,
    #[error("Cannot recover public key")]
    CannotRecoverPublicKey,
}

impl EmojiId {
    /// Get the public key from an emoji ID
    pub fn as_public_key(&self) -> &PublicKey {
        &self.0
    }
}

impl FromStr for EmojiId {
    type Err = EmojiIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // The string must be the correct size, including the checksum
        if s.chars().count() != DATA_BYTES + CHECKSUM_BYTES {
            return Err(EmojiIdError::InvalidSize);
        }

        // Convert the emoji string to a byte array
        let mut bytes = Vec::<u8>::with_capacity(DATA_BYTES + CHECKSUM_BYTES);
        for c in s.chars() {
            if let Some(i) = REVERSE_EMOJI.get(&c) {
                bytes.push(*i);
            } else {
                return Err(EmojiIdError::InvalidEmoji);
            }
        }

        // Assert the checksum is valid and get the underlying data
        let data = validate_checksum(&bytes).map_err(|_| EmojiIdError::InvalidChecksum)?;

        // Convert to a public key
        match PublicKey::from_canonical_bytes(data) {
            Ok(public_key) => Ok(Self(public_key)),
            Err(_) => Err(EmojiIdError::CannotRecoverPublicKey),
        }
    }
}

impl From<&PublicKey> for EmojiId {
    fn from(value: &PublicKey) -> Self {
        Self::from(value.clone())
    }
}

impl From<PublicKey> for EmojiId {
    fn from(value: PublicKey) -> Self {
        Self(value)
    }
}

impl From<&EmojiId> for PublicKey {
    fn from(value: &EmojiId) -> Self {
        value.as_public_key().clone()
    }
}

impl Display for EmojiId {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        // Convert the public key to bytes and compute the checksum
        let bytes = self.as_public_key().as_bytes();
        let emoji = bytes
            .iter()
            .chain(iter::once(&compute_checksum(bytes)))
            .map(|b| EMOJI[*b as usize])
            .collect::<String>();

        fmt.write_str(&emoji)
    }
}

#[cfg(test)]
mod test {
    use std::{iter, str::FromStr};

    use tari_crypto::{
        keys::{PublicKey as PublicKeyTrait, SecretKey},
        tari_utilities::ByteArray,
    };

    use crate::{
        dammsum::{compute_checksum, CHECKSUM_BYTES},
        emoji::{emoji_set, EmojiId, EmojiIdError, DATA_BYTES},
        types::{PrivateKey, PublicKey},
    };

    #[test]
    /// Test valid emoji ID
    fn valid_emoji_id() {
        // Generate random public key
        let mut rng = rand::thread_rng();
        let public_key = PublicKey::from_secret_key(&PrivateKey::random(&mut rng));

        // Generate an emoji ID from the public key and ensure we recover it
        let emoji_id_from_public_key = EmojiId::from(&public_key);
        assert_eq!(emoji_id_from_public_key.as_public_key(), &public_key);

        // Check the size of the corresponding emoji string
        let emoji_string = emoji_id_from_public_key.to_string();
        assert_eq!(emoji_string.chars().count(), DATA_BYTES + CHECKSUM_BYTES);

        // Generate an emoji ID from the emoji string and ensure we recover it
        let emoji_id_from_emoji_string = EmojiId::from_str(&emoji_string).unwrap();
        assert_eq!(emoji_id_from_emoji_string.to_string(), emoji_string);

        // Return to the original public key for good measure
        assert_eq!(emoji_id_from_emoji_string.as_public_key(), &public_key);
    }

    #[test]
    /// Test invalid size
    fn invalid_size() {
        // This emoji string is too short to be a valid emoji ID
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🐢🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒";
        assert_eq!(EmojiId::from_str(emoji_string), Err(EmojiIdError::InvalidSize));
    }

    #[test]
    /// Test invalid emoji
    fn invalid_emoji() {
        // This emoji string contains an invalid emoji character
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🐢🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🎅";
        assert_eq!(EmojiId::from_str(emoji_string), Err(EmojiIdError::InvalidEmoji));
    }

    #[test]
    /// Test invalid checksum
    fn invalid_checksum() {
        // This emoji string contains an invalid checksum
        let emoji_string = "🌴🦀🔌📌🚑🌰🎓🌴🐊🐌🔒💡🐜📜👛🍵👛🐽🎂🐻🐢🍓👶🐭🐼🏀🎪💔💵🥑🔋🎒🎒";
        assert_eq!(EmojiId::from_str(emoji_string), Err(EmojiIdError::InvalidChecksum));
    }

    #[test]
    /// Test invalid public key
    fn invalid_public_key() {
        // This byte representation does not represent a valid public key
        let mut bytes = vec![0u8; DATA_BYTES];
        bytes[0] = 1;
        assert!(PublicKey::from_canonical_bytes(&bytes).is_err());

        // Convert to an emoji string and manually add a valid checksum
        let emoji_set = emoji_set();
        let emoji_string = bytes
            .iter()
            .chain(iter::once(&compute_checksum(&bytes)))
            .map(|b| emoji_set[*b as usize])
            .collect::<String>();

        assert_eq!(
            EmojiId::from_str(&emoji_string),
            Err(EmojiIdError::CannotRecoverPublicKey)
        );
    }

    #[test]
    /// Test that the data size is correct for the underlying key type
    fn data_size() {
        assert_eq!(PublicKey::default().as_bytes().len(), DATA_BYTES);
    }
}
