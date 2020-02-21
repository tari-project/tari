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

use core::convert::TryFrom;
use derive_error::Error;
use serde::export::{fmt::Error, Formatter};
use std::fmt::Display;
use tari_comms::peer_manager::{NodeId, NODE_ID_ARRAY_SIZE};
use tari_crypto::tari_utilities::ByteArray;

/// The number of emoji in the emoji id dictionary.
const EMOJI_ID_DICTIONARY_LEN: usize = 256;
/// The byte size of an EmojiId.
pub const EMOJI_ID_ARRAY_SIZE: usize = 1 + NODE_ID_ARRAY_SIZE; // Version + NodeId
/// The Dictionary version used for creating an EmojiId.
pub const EMOJI_ID_VERSION: u8 = 0;

/// The total set of emoji that can be used for emoji id generation.
// TODO: This is a test dictionary and should be replaced.
const EMOJI: [char; EMOJI_ID_DICTIONARY_LEN] = [
    '💫', '😀', '😁', '😆', '😅', '🤣', '🙃', '😉', '😊', '😇', '🥰', '😍', '🤩', '😘', '😋', '🤪', '🤑', '🤗', '🤭',
    '🤫', '🤔', '🤐', '🤨', '😶', '😏', '😒', '🙄', '😬', '🤥', '😌', '🤤', '😴', '😷', '🤒', '🤢', '🤮', '🥵', '🥶',
    '🥴', '😵', '🤯', '🤠', '🥳', '😎', '🤓', '🧐', '😕', '😳', '🥺', '😰', '😥', '😭', '😱', '😖', '😡', '👻', '👽',
    '👾', '🤖', '🙈', '🙉', '🙊', '💋', '💌', '💘', '💝', '💕', '💯', '💢', '💥', '💣', '💤', '👌', '🤞', '🤟', '🤙',
    '👍', '👎', '✊', '👊', '👏', '🤝', '💪', '🦶', '👂', '🧠', '🦷', '🦴', '👀', '👄', '🤦', '🤷', '🦸', '🦹', '🧙', '🧚',
    '🧛', '🧜', '🧞', '🧟', '💃', '🕺', '🧗', '🏇', '⛷', '🏂', '🏌', '🏄', '🚣', '🏊', '⛹', '🏋', '🚴', '🤸', '🤼', '🤽',
    '🤾', '🤹', '👣', '🛀', '🛌', '🦍', '🐶', '🦊', '🐱', '🦁', '🐯', '🦄', '🐮', '🐷', '🦒', '🐘', '🦏', '🐹', '🐰',
    '🐨', '🐼', '🦨', '🦘', '🐾', '🐓', '🐣', '🐧', '🕊', '🦅', '🦢', '🦩', '🐸', '🦎', '🦖', '🐳', '🐬', '🐟', '🐙', '🐚',
    '🐌', '🦋', '🐛', '🐜', '🐝', '🐞', '💐', '🌹', '🌴', '🌵', '🍀', '🍇', '🍉', '🍌', '🍍', '🍎', '🍒', '🍓', '🥥',
    '🥑', '🥕', '🌽', '🌶', '🧅', '🍄', '🥨', '🧀', '🍔', '🍟', '🍕', '🌭', '🥪', '🍿', '🧂', '🦀', '🦑', '🍦', '🍧',
    '🍩', '🍪', '🎂', '🍫', '🍬', '🍭', '🍯', '🌍', '🏰', '🎪', '🚂', '🚒', '🚓', '🏎', '🛹', '⛵', '🪂', '🚁', '🚀',
    '🛸', '⌛', '⏰', '🌡', '⭐', '🌈', '🔥', '💧', '🧨', '🎈', '🎉', '🎀', '🎁', '🏆', '⚽', '🎳', '🥊', '🎱', '🕹',
    '🎲', '🧸', '🎨', '🕶', '👠', '👑', '🎩', '🧢', '💄', '💎', '🔈', '🔔', '🎵', '🎧', '🎷', '🎸', '🎹', '🎺', '🎻',
    '📱', '🔋', '🎬', '💰', '🖍', '🔫',
];

#[derive(Debug, Clone, Error, PartialEq)]
pub enum EmojiIdError {
    // The provided Emoji could not be found in the Emoji set.
    Notfound,
    // Could not create an EmojiId from the provided bytes.
    IncorrectByteCount,
    // Unsupported emoji dictionary version.
    UnsupportedVersion,
    // Could not convert from an EmojiId to a NodeId.
    #[error(msg_embedded, non_std, no_from)]
    ConversionError(String),
}

/// EmojiId can encode and decode a NodeId and dictionary version into a Emoji string set.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EmojiId([u8; EMOJI_ID_ARRAY_SIZE]);

impl EmojiId {
    /// The dictionary version used by the EmojiId.
    pub fn version(&self) -> u8 {
        self.0[0]
    }

    /// Extract and return the encoded NodeId from the EmojiId.
    pub fn node_id(&self) -> Result<NodeId, EmojiIdError> {
        NodeId::from_bytes(&self.0[1..=NODE_ID_ARRAY_SIZE])
            .map_err(|err| EmojiIdError::ConversionError(format!("{:?}", err)))
    }

    // Encode the internal bytes to a Emoji string set.
    fn to_emoji_string(&self) -> String {
        self.0.iter().map(|index| EMOJI[*index as usize]).collect::<String>()
    }
}

/// Create an EmojiId from a set of bytes, these bytes should include a dictionary version and NodeId.
impl TryFrom<Vec<u8>> for EmojiId {
    type Error = EmojiIdError;

    fn try_from(id_bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if id_bytes.len() != EMOJI_ID_ARRAY_SIZE {
            return Err(EmojiIdError::IncorrectByteCount);
        }

        let mut id_byte_slice = [0u8; EMOJI_ID_ARRAY_SIZE];
        id_byte_slice.copy_from_slice(id_bytes.as_slice());

        let emoji_id = Self(id_byte_slice);
        if emoji_id.version() != EMOJI_ID_VERSION {
            return Err(EmojiIdError::UnsupportedVersion);
        }
        Ok(emoji_id)
    }
}

/// Create an EmojiId from an emoji string set.
impl TryFrom<&str> for EmojiId {
    type Error = EmojiIdError;

    fn try_from(emoji_set: &str) -> Result<Self, Self::Error> {
        let mut id_bytes = Vec::<u8>::with_capacity(EMOJI_ID_ARRAY_SIZE);
        for emoji in emoji_set.chars() {
            id_bytes.push(emoji_to_index(emoji)?);
        }
        EmojiId::try_from(id_bytes)
    }
}

/// Create an EmojiId from a NodeId.
impl TryFrom<NodeId> for EmojiId {
    type Error = EmojiIdError;

    fn try_from(node_id: NodeId) -> Result<Self, Self::Error> {
        let mut id_bytes: Vec<u8> = vec![EMOJI_ID_VERSION];
        id_bytes.append(&mut node_id.as_bytes().to_vec());
        EmojiId::try_from(id_bytes)
    }
}

impl Display for EmojiId {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        fmt.write_str(&self.to_emoji_string())
    }
}

// Finds the index of the specified emoji in the dictionary.
fn emoji_to_index(emoji: char) -> Result<u8, EmojiIdError> {
    for i in 0..EMOJI.len() {
        if emoji == EMOJI[i] {
            return Ok(i as u8);
        }
    }
    Err(EmojiIdError::Notfound)
}

#[cfg(test)]
mod test {
    use crate::util::emoji::{EmojiId, EmojiIdError, EMOJI_ID_VERSION};
    use std::convert::TryFrom;
    use tari_comms::peer_manager::NodeId;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    #[test]
    fn id_from_byte_slice() {
        let key_bytes = [64, 28, 98, 64, 28, 197, 216, 115, 9, 25, 41, 76];
        let desired_emoji_set = "💘🤥🧞💘🤥🍬⭐🤽😇😒🤠👍".to_string();
        assert_eq!(EmojiId(key_bytes).to_string(), desired_emoji_set);
    }

    #[test]
    fn id_from_string() {
        let desired_emoji_set = "💫🎀🐼🍫🚣🦅🍩🤤🎲🤭🍭🐓".to_string();
        let emoji_id = EmojiId::try_from(desired_emoji_set.as_str()).unwrap();
        assert_eq!(emoji_id.to_string(), desired_emoji_set);
    }

    #[test]
    fn id_from_node_id() {
        let node_id = NodeId::new();
        let desired_emoji_set = "💫💫💫💫💫💫💫💫💫💫💫💫".to_string();
        let emoji_id = EmojiId::try_from(node_id.clone()).unwrap();
        assert_eq!(emoji_id.to_string(), desired_emoji_set);

        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let emoji_id = EmojiId::try_from(node_id.clone()).unwrap();
        assert_eq!(emoji_id.node_id().unwrap(), node_id);
        assert_eq!(emoji_id.version(), EMOJI_ID_VERSION);
    }

    #[test]
    fn check_id_version() {
        let emoji_set = "💣👾🧞💘🦋🍬⭐🤽🐣😒🤠🌭".to_string();
        assert_eq!(
            EmojiId::try_from(emoji_set.as_str()),
            Err(EmojiIdError::UnsupportedVersion)
        );
    }
}
