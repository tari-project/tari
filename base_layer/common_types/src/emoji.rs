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
    fmt::{Display, Error, Formatter},
};

use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
use thiserror::Error;

use crate::{
    luhn::{checksum, is_valid},
    types::PublicKey,
};

const EMOJI: [char; 256] = [
    '🌀', '🌂', '🌈', '🌊', '🌋', '🌍', '🌙', '🌝', '🌞', '🌟', '🌠', '🌰', '🌴', '🌵', '🌷', '🌸', '🌹', '🌻', '🌽',
    '🍀', '🍁', '🍄', '🍅', '🍆', '🍇', '🍈', '🍉', '🍊', '🍋', '🍌', '🍍', '🍎', '🍐', '🍑', '🍒', '🍓', '🍔', '🍕',
    '🍗', '🍚', '🍞', '🍟', '🍠', '🍣', '🍦', '🍩', '🍪', '🍫', '🍬', '🍭', '🍯', '🍰', '🍳', '🍴', '🍵', '🍶', '🍷',
    '🍸', '🍹', '🍺', '🍼', '🎀', '🎁', '🎂', '🎃', '🎄', '🎈', '🎉', '🎒', '🎓', '🎠', '🎡', '🎢', '🎣', '🎤', '🎥',
    '🎧', '🎨', '🎩', '🎪', '🎬', '🎭', '🎮', '🎰', '🎱', '🎲', '🎳', '🎵', '🎷', '🎸', '🎹', '🎺', '🎻', '🎼', '🎽',
    '🎾', '🎿', '🏀', '🏁', '🏆', '🏈', '🏉', '🏠', '🏥', '🏦', '🏭', '🏰', '🐀', '🐉', '🐊', '🐌', '🐍', '🐎', '🐐',
    '🐑', '🐓', '🐖', '🐗', '🐘', '🐙', '🐚', '🐛', '🐜', '🐝', '🐞', '🐢', '🐣', '🐨', '🐩', '🐪', '🐬', '🐭', '🐮',
    '🐯', '🐰', '🐲', '🐳', '🐴', '🐵', '🐶', '🐷', '🐸', '🐺', '🐻', '🐼', '🐽', '🐾', '👀', '👅', '👑', '👒', '👓',
    '👔', '👕', '👖', '👗', '👘', '👙', '👚', '👛', '👞', '👟', '👠', '👡', '👢', '👣', '👹', '👻', '👽', '👾', '👿',
    '💀', '💄', '💈', '💉', '💊', '💋', '💌', '💍', '💎', '💐', '💔', '💕', '💘', '💡', '💣', '💤', '💦', '💨', '💩',
    '💭', '💯', '💰', '💳', '💸', '💺', '💻', '💼', '📈', '📉', '📌', '📎', '📚', '📝', '📡', '📣', '📱', '📷', '🔋',
    '🔌', '🔎', '🔑', '🔔', '🔥', '🔦', '🔧', '🔨', '🔩', '🔪', '🔫', '🔬', '🔭', '🔮', '🔱', '🗽', '😂', '😇', '😈',
    '😉', '😍', '😎', '😱', '😷', '😹', '😻', '😿', '🚀', '🚁', '🚂', '🚌', '🚑', '🚒', '🚓', '🚕', '🚗', '🚜', '🚢',
    '🚦', '🚧', '🚨', '🚪', '🚫', '🚲', '🚽', '🚿', '🛁',
];

lazy_static! {
    static ref REVERSE_EMOJI: HashMap<char, usize> = {
        let mut m = HashMap::with_capacity(256);
        EMOJI.iter().enumerate().for_each(|(i, c)| {
            m.insert(*c, i);
        });
        m
    };
}

/// Emoji IDs are 33-byte long representations of a public key. The first 32 bytes are a mapping of a 256 byte emoji
/// dictionary to each of the 32 bytes in the public key. The 33rd emoji is a checksum character of the 32-length
/// string.
///
/// Emoji IDs (32 characters minus checksum) are therefore more compact than Base58 or Base64 encodings (~44 characters)
/// or hexadecimal (64 characters) and in theory, more human readable.
///
/// The checksum is calculated using a Luhn mod 256 checksum, which guards against most transposition errors.
///
/// # Example
///
/// ```
/// use tari_common_types::emoji::EmojiId;
///
/// assert!(EmojiId::is_valid("🐎🍴🌷🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹🎒"));
/// let eid = EmojiId::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a").unwrap();
/// assert_eq!(eid.as_str(), "🐎🍴🌷🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹🎒");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EmojiId(String);

/// Returns the current emoji set as a vector of char
pub const fn emoji_set() -> [char; 256] {
    EMOJI
}

impl EmojiId {
    /// Construct an Emoji ID from the given pubkey.
    pub fn from_pubkey(key: &PublicKey) -> Self {
        EmojiId::from_bytes(key.as_bytes())
    }

    /// Try and construct an emoji ID from the given hex string. The method will fail if the hex is not a valid
    /// representation of a public key.
    pub fn from_hex(hex_key: &str) -> Result<Self, EmojiIdError> {
        let key = PublicKey::from_hex(hex_key)?;
        Ok(EmojiId::from_pubkey(&key))
    }

    /// Return the public key that this emoji ID represents
    pub fn to_pubkey(&self) -> Result<PublicKey, EmojiIdError> {
        let bytes = self.to_bytes()?;
        PublicKey::from_bytes(&bytes).map_err(EmojiIdError::from)
    }

    /// Checks whether a given string would be a valid emoji ID using the assertion that
    /// i) The string is 33 bytes long
    /// ii) The last byte is a valid checksum
    pub fn is_valid(s: &str) -> bool {
        EmojiId::str_to_pubkey(s).is_ok()
    }

    pub fn str_to_pubkey(s: &str) -> Result<PublicKey, EmojiIdError> {
        let mut indices = Vec::with_capacity(33);
        for c in s.chars() {
            let i = REVERSE_EMOJI.get(&c).ok_or_else(|| EmojiIdError::UnsupportedChar(c))?;
            indices.push(*i);
        }
        if is_valid(&indices, 256) {
            let bytes = EmojiId::byte_vec(s)?;
            PublicKey::from_bytes(&bytes).map_err(EmojiIdError::from)
        } else {
            Err(EmojiIdError::InvalidChecksum)
        }
    }

    /// Return the 33 character emoji string for this emoji ID
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert the emoji ID string into its associated public key, represented as a byte array
    pub fn to_bytes(&self) -> Result<Vec<u8>, EmojiIdError> {
        EmojiId::byte_vec(&self.0)
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut vec = Vec::<usize>::with_capacity(33);
        bytes.iter().for_each(|b| vec.push((*b) as usize));
        let checksum = checksum(&vec, 256);
        assert!(checksum < 256);
        vec.push(checksum);
        let id = vec.iter().map(|b| EMOJI[*b]).collect();
        Self(id)
    }

    fn byte_vec(s: &str) -> Result<Vec<u8>, EmojiIdError> {
        let mut v = Vec::with_capacity(32);
        for c in s.chars().take(32) {
            let index = REVERSE_EMOJI.get(&c).ok_or_else(|| EmojiIdError::UnsupportedChar(c))?;
            v.push(*index as u8);
        }
        Ok(v)
    }
}

impl Display for EmojiId {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        fmt.write_str(self.as_str())
    }
}

// TODO: We have to add more details
#[derive(Debug, Error)]
pub enum EmojiIdError {
    #[error("Byte array error: {0}")]
    ByteArray(#[from] tari_utilities::ByteArrayError),
    #[error("Hex error in EmojiId: {0}")]
    HexError(#[from] tari_crypto::tari_utilities::hex::HexError),
    #[error("Unsupported char for EmojiId: '{0}'")]
    UnsupportedChar(char),
    #[error("Invalid checksum of EmojiId")]
    InvalidChecksum,
}

#[cfg(test)]
mod test {
    use anyhow::Error;
    use tari_crypto::tari_utilities::hex::Hex;

    use crate::{emoji::EmojiId, types::PublicKey};

    #[test]
    fn convert_key() -> Result<(), Error> {
        let pubkey = PublicKey::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a")?;
        let eid = EmojiId::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a")?;
        assert_eq!(
            eid.as_str(),
            "🐎🍴🌷🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹🎒"
        );
        assert_eq!(EmojiId::from_pubkey(&pubkey), eid);
        assert_eq!(
            &eid.to_bytes()?.to_hex(),
            "70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a"
        );
        let eid = EmojiId::str_to_pubkey("🐎🍴🌷🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹🎒")?;
        assert_eq!(eid, pubkey);
        Ok(())
    }

    #[test]
    fn is_valid() -> Result<(), Error> {
        let eid = EmojiId::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a")?;
        // Valid emojiID
        assert!(EmojiId::is_valid(eid.as_str()));
        assert!(!EmojiId::is_valid(""), "Emoji ID too short");
        assert!(!EmojiId::is_valid("🌂"), "Emoji ID too short");
        assert!(
            !EmojiId::is_valid("🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹🎒"),
            "Emoji ID too short"
        );
        assert!(
            !EmojiId::is_valid("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a"),
            "Not emoji string"
        );
        assert!(
            !EmojiId::is_valid("🐎🍴🌷🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹"),
            "No checksum"
        );
        assert!(
            !EmojiId::is_valid("🐎🍴🌷🌟💻🐖🐩🐾🌟🐬🎧🐌🏦🐳🐎🐝🐢🔋👕🎸👿🍒🐓🎉💔🌹🏆🐬💡🎳🚦🍹📝"),
            "Wrong checksum"
        );
        Ok(())
    }
}
