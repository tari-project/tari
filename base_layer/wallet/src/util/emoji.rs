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

use crate::util::{luhn, luhn::checksum};
use core::convert::TryFrom;
use derive_error::Error;
use serde::export::{fmt::Error, Formatter};
use std::fmt::Display;
use tari_comms::peer_manager::{NodeId, NODE_ID_ARRAY_SIZE};
use tari_crypto::tari_utilities::{
    bit::{bits_to_bytes, uint_to_bits},
    ByteArray,
};

/// The number of emoji in the dictionary.
const EMOJI_ID_DICTIONARY_LEN: usize = 1024;
/// The Dictionary version encoded into EmojiIds created from NodeIds.
const NODE_ID_TO_EMOJI_ID_VERSION: u8 = 1;
/// The Dictionary version bit count encoded into EmojiIds created from NodeIds.
const NODE_ID_TO_EMOJI_ID_VERSION_BIT_COUNT: u8 = 6;

/// The total set of emoji that can be used for emoji id generation.
// TODO: This is a test dictionary and should be replaced.
const EMOJI: [char; EMOJI_ID_DICTIONARY_LEN] = [
    '😀', '😃', '😄', '😁', '😆', '😅', '🤣', '😂', '🙂', '🙃', '😉', '😊', '😇', '🥰', '😍', '🤩', '😘', '😗', '😚',
    '😙', '😋', '😛', '😜', '🤪', '😝', '🤑', '🤗', '🤭', '🤫', '🤔', '🤐', '🤨', '😐', '😑', '😶', '😏', '😒', '🙄',
    '😬', '🤥', '😌', '😔', '😪', '🤤', '😴', '😷', '🤒', '🤕', '🤢', '🤮', '🤧', '🥵', '🥶', '🥴', '😵', '🤯', '🤠', '🥳',
    '😎', '🤓', '🧐', '😕', '😟', '🙁', '😮', '😲', '😳', '🥺', '😦', '😧', '😨', '😰', '😥', '😢', '😭', '😱', '😖',
    '😣', '😞', '😓', '😩', '😫', '🥱', '😤', '😡', '😠', '🤬', '😈', '👿', '💀', '💩', '🤡', '👹', '👺', '👻', '👽',
    '👾', '🤖', '😺', '😸', '😹', '😻', '😼', '😽', '🙀', '😿', '😾', '🙈', '🙉', '🙊', '💋', '💌', '💘', '💝', '💞',
    '💕', '💔', '💯', '💢', '💥', '💫', '💦', '💨', '🕳', '💣', '💬', '🗨', '🗯', '💭', '💤', '👋', '🤚', '🖐', '✋', '🖖',
    '👌', '🤏', '🤞', '🤟', '🤘', '🤙', '👈', '👉', '👆', '🖕', '👇', '👍', '👎', '✊', '👊', '🤛', '🤜', '👏', '🙌',
    '👐', '🤲', '🤝', '🙏', '💅', '🤳', '💪', '🦾', '🦿', '🦵', '🦶', '👂', '👃', '🧠', '🦷', '🦴', '👀', '👁', '👅', '👄',
    '👶', '🧒', '👦', '👧', '🧑', '👱', '👨', '🧔', '👩', '👵', '🙍', '🙅', '🙆', '💁', '🙋', '🧏', '🙇', '🤦', '🤷',
    '👮', '🕵', '💂', '👷', '🤴', '👸', '👳', '👲', '🧕', '👰', '🤰', '🤱', '👼', '🎅', '🤶', '🦸', '🦹', '🧙', '🧚',
    '🧛', '🧜', '🧝', '🧞', '🧟', '💆', '💇', '🚶', '🧍', '🧎', '🏃', '💃', '🕺', '🕴', '👯', '🧖', '🧗', '🤺', '🏇', '⛷',
    '🏂', '🏌', '🏄', '🚣', '🏊', '⛹', '🏋', '🚴', '🚵', '🤸', '🤼', '🤽', '🤾', '🤹', '🧘', '🛀', '🛌', '💏', '👪', '🗣',
    '👤', '👥', '👣', '🐵', '🐒', '🦍', '🦧', '🐶', '🐕', '🦮', '🐩', '🐺', '🦊', '🦝', '🐱', '🐈', '🦁', '🐯', '🐅',
    '🐆', '🐴', '🐎', '🦄', '🦓', '🦌', '🐮', '🐂', '🐃', '🐄', '🐷', '🐖', '🐗', '🐽', '🐏', '🐑', '🐐', '🐪', '🐫',
    '🦙', '🦒', '🐘', '🦏', '🦛', '🐭', '🐁', '🐀', '🐹', '🐰', '🐇', '🐿', '🦔', '🦇', '🐻', '🐨', '🐼', '🦥', '🦦', '🦨',
    '🦘', '🦡', '🐾', '🦃', '🐔', '🐓', '🐣', '🐤', '🐥', '🐦', '🐧', '🕊', '🦅', '🦆', '🦢', '🦉', '🦩', '🦚', '🦜', '🐸',
    '🐊', '🐢', '🦎', '🐍', '🐲', '🐉', '🦕', '🦖', '🐳', '🐋', '🐬', '🐟', '🐠', '🐡', '🦈', '🐙', '🐚', '🐌', '🦋',
    '🐛', '🐜', '🐝', '🐞', '🦗', '🕷', '🕸', '🦂', '🦟', '🦠', '💐', '🌸', '💮', '🏵', '🌹', '🥀', '🌺', '🌻', '🌼', '🌷',
    '🌱', '🌲', '🌳', '🌴', '🌵', '🌾', '🌿', '🍀', '🍁', '🍂', '🍃', '🍇', '🍈', '🍉', '🍊', '🍋', '🍌', '🍍', '🥭',
    '🍎', '🍏', '🍐', '🍑', '🍒', '🍓', '🥝', '🍅', '🥥', '🥑', '🍆', '🥔', '🥕', '🌽', '🌶', '🥒', '🥬', '🥦', '🧄', '🧅',
    '🍄', '🥜', '🌰', '🍞', '🥐', '🥖', '🥨', '🥯', '🥞', '🧇', '🧀', '🍖', '🍗', '🥩', '🥓', '🍔', '🍟', '🍕', '🌭',
    '🥪', '🌮', '🌯', '🥙', '🧆', '🥚', '🍳', '🥘', '🍲', '🥣', '🥗', '🍿', '🧈', '🧂', '🥫', '🍱', '🍘', '🍙', '🍚',
    '🍛', '🍜', '🍝', '🍠', '🍢', '🍣', '🍤', '🍥', '🥮', '🍡', '🥟', '🥠', '🥡', '🦀', '🦞', '🦐', '🦑', '🦪', '🍦',
    '🍧', '🍨', '🍩', '🍪', '🎂', '🍰', '🧁', '🥧', '🍫', '🍬', '🍭', '🍮', '🍯', '🍼', '🥛', '🍶', '🍾', '🍷', '🍸',
    '🍹', '🍺', '🍻', '🥂', '🥃', '🥤', '🧃', '🧉', '🧊', '🥢', '🍽', '🍴', '🥄', '🔪', '🏺', '🌍', '🗺', '🧭', '🏔', '🌋',
    '🏕', '🏖', '🏜', '🏝', '🏟', '🏛', '🏗', '🧱', '🏘', '🏚', '🏠', '🏡', '🏢', '🏣', '🏤', '🏥', '🏦', '🏨', '🏩', '🏪', '🏫',
    '🏬', '🏭', '🏯', '🏰', '💒', '🗼', '🗽', '⛪', '🕌', '🛕', '🕍', '⛩', '🕋', '⛲', '⛺', '🌁', '🌃', '🏙', '🌄',
    '🌅', '🌆', '🌉', '🎠', '🎡', '🎢', '💈', '🎪', '🚂', '🚄', '🚋', '🚌', '🚐', '🚑', '🚒', '🚓', '🚕', '🚗', '🚙',
    '🚚', '🚜', '🏎', '🏍', '🛵', '🦼', '🛺', '🚲', '🛴', '🛹', '🚏', '🛣', '🛤', '🛢', '⛽', '🚨', '🚦', '🛑', '🚧', '⛵',
    '🛶', '🚤', '🛳', '🛥', '🛩', '🪂', '💺', '🚁', '🚠', '🛰', '🚀', '🛸', '🛎', '🧳', '⌛', '⌚', '⏰', '⏲', '🕰', '🕛',
    '🕧', '🕐', '🕜', '🕑', '🕝', '🕒', '🕞', '🕓', '🕟', '🕔', '🕠', '🕕', '🕡', '🕖', '🕢', '🕗', '🕣', '🕘', '🕤',
    '🕙', '🕥', '🕚', '🕦', '🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘', '🌙', '🌚', '🌛', '🌡', '🌝', '🌞', '🪐',
    '⭐', '🌠', '🌌', '⛅', '⛈', '🌨', '🌪', '🌫', '🌬', '🌀', '🌈', '🌂', '⛱', '🔥', '💧', '🌊', '🎃', '🎄', '🎇', '🧨',
    '✨', '🎈', '🎉', '🎊', '🎋', '🎍', '🎎', '🎏', '🎐', '🎑', '🧧', '🎀', '🎁', '🎗', '🎟', '🎖', '🏆', '🥇', '🥈', '🥉',
    '⚽', '⚾', '🥎', '🏀', '🏐', '🏈', '🏉', '🎾', '🥏', '🎳', '🏏', '🏑', '🏒', '🥍', '🏓', '🏸', '🥊', '🥋', '🥅',
    '⛳', '⛸', '🎣', '🤿', '🎽', '🎿', '🛷', '🥌', '🎯', '🪀', '🪁', '🎱', '🔮', '🧿', '🎮', '🕹', '🎰', '🎲', '🧩', '🧸',
    '♠', '♥', '♦', '♣', '🃏', '🀄', '🎴', '🎭', '🖼', '🎨', '🧵', '🧶', '👓', '🕶', '🥽', '🥼', '🦺', '👔', '👕', '👖', '🧣',
    '🧤', '🧥', '🧦', '👗', '👘', '🥻', '🩱', '🩲', '🩳', '👙', '👚', '👛', '👜', '👝', '🛍', '🎒', '👞', '👟', '🥾', '🥿',
    '👠', '👡', '🩰', '👢', '👑', '👒', '🎩', '🎓', '🧢', '⛑', '📿', '💄', '💍', '💎', '🔈', '📣', '📯', '🔔', '🎼',
    '🎵', '🎶', '🎛', '🎤', '🎧', '📻', '🎷', '🎸', '🎹', '🎺', '🎻', '🪕', '🥁', '📱', '📞', '📟', '📠', '🔋', '🔌',
    '💻', '🖥', '🖨', '🖱', '💽', '💾', '🧮', '🎥', '🎞', '📽', '🎬', '📺', '📷', '📹', '📼', '🔍', '🕯', '💡', '🔦', '🏮',
    '🪔', '📕', '📖', '📚', '📰', '🏷', '💰', '💳', '📦', '📫', '📮', '🗳', '🖍', '💼', '📅', '📈', '📉', '📊', '📋', '📌',
    '📎', '🖇', '📏', '📐', '🗄', '🗑', '🔒', '🔑', '🗝', '🔨', '🪓', '⛏', '🛠', '🗡', '🔫', '🏹', '🛡', '🔧', '🔩', '🗜', '🦯',
    '🔗', '⛓', '🧰', '🧲', '🧪', '🧫', '🧬', '🔬', '🔭', '📡', '💉', '🩸', '💊', '🩹', '🩺', '🚪', '🛏', '🛋', '🪑', '🚽', '🚿',
    '🛁', '🪒', '🧴', '🧷', '🧹', '🧺', '🧻', '🧼', '🧽', '🧯', '🛒', '🚬', '🗿', '🏁', '!', '"', '#', '$', '%', '&', '(', ')',
    '*', '+', ',', '-', '.', '/', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?', '@',
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'S', 'U', 'V', 'W',
    'X', 'Y', 'Z', '[', ']', '^', '_', '`', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o',
    'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'u', 'z', '{', '|', '}', '€', '‚', 'ƒ', '…', '†', '‡', 'ˆ', '‰', 'Š',
    '‹', 'Œ', 'Ž', '•', '˜', '™', 'š', '›', 'œ', 'ž', 'Ÿ', '¢', '£', '¤', '¥', '¦', '§', '©', 'ª', '«', '¬', '®', '¯',
    '°', '±', '²', '³', 'µ', '¶',
];

#[derive(Debug, Clone, Error, PartialEq)]
pub enum EmojiIdError {
    // The provided Emoji could not be found in the Emoji set.
    Notfound,
    // The checksum of the EmojiId was invalid.
    InvalidChecksum,
    // Emoji index out of bounds.
    IndexOutOfBounds,
    // Could not converting from a different format
    #[error(msg_embedded, non_std, no_from)]
    ConversionError(String),
}

/// The EmojiId can encode and decode a set of bytes into emoji, and back again. It contains also includes a version and
/// checksum for the encoded information.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EmojiId(Vec<u8>);

impl EmojiId {
    /// Create a new EmojiID from a set of unchecked bytes, a version and checksum for the provided bytes will be
    /// included in the EmojiId. Only the first bits upto the specified bit_count will be considered.
    pub fn new(unchecked_bytes: Vec<u8>, bit_count: usize) -> Result<Self, EmojiIdError> {
        let unchecked_indices = bytes_to_indices(unchecked_bytes, bit_count);
        EmojiId::new_from_indices(unchecked_indices)
    }

    /// Create a new EmojiID from a set of unchecked emoji dictionary indices, a version and checksum or the provided
    /// emoji dictionary indices will be included in the EmojiId.
    pub fn new_from_indices(mut unchecked_indices: Vec<usize>) -> Result<Self, EmojiIdError> {
        check_valid_indices(&unchecked_indices)?;
        unchecked_indices.push(checksum(&unchecked_indices, EMOJI_ID_DICTIONARY_LEN));
        Ok(Self(indices_to_bytes(&unchecked_indices)))
    }

    // Returns the number of bits used for indices.
    fn num_indice_bits(&self) -> usize {
        let num_indices = (self.0.len() * 8) / 10;
        num_indices * 10
    }
}

/// Create an EmojiId from a set of bytes, these bytes should include a version and checksum.
impl TryFrom<Vec<u8>> for EmojiId {
    type Error = EmojiIdError;

    fn try_from(key_bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let num_indices = (key_bytes.len() * 8) / 10;
        let bitcount = num_indices * 10;
        let indices = bytes_to_indices(key_bytes.clone(), bitcount);
        check_valid_indices(&indices)?;
        if !luhn::is_valid(&indices, EMOJI_ID_DICTIONARY_LEN) {
            return Err(EmojiIdError::InvalidChecksum);
        }
        Ok(Self(key_bytes))
    }
}

/// Create an EmojiId from a emoji string, this string should include a version and checksum.
impl TryFrom<&str> for EmojiId {
    type Error = EmojiIdError;

    fn try_from(emoji_set: &str) -> Result<Self, Self::Error> {
        let indices = emoji_set_to_indices(emoji_set)?;
        EmojiId::try_from(indices_to_bytes(&indices))
    }
}

impl TryFrom<NodeId> for EmojiId {
    type Error = EmojiIdError;

    fn try_from(node_id: NodeId) -> Result<Self, Self::Error> {
        let mut unchecked_bytes = node_id.as_bytes().to_vec();
        let bit_count = unchecked_bytes.len() * 8 + NODE_ID_TO_EMOJI_ID_VERSION_BIT_COUNT as usize;
        unchecked_bytes.push(NODE_ID_TO_EMOJI_ID_VERSION);
        EmojiId::new(unchecked_bytes, bit_count)
    }
}

// Decode the NodeId and dictionary version from a EmojiId that encoded a NodeId.
fn emoji_id_to_node_id(emoji_id: EmojiId) -> Result<(NodeId, u8), EmojiIdError> {
    let emoji_id_bytes = emoji_id.0.as_bytes();
    if emoji_id_bytes.len() <= NODE_ID_ARRAY_SIZE {
        // NodeID + Version
        return Err(EmojiIdError::ConversionError("Insufficient bytes".into()));
    }
    let node_id = NodeId::from_bytes(&emoji_id_bytes[0..NODE_ID_ARRAY_SIZE])
        .map_err(|err| EmojiIdError::ConversionError(format!("{:?}", err)))?;
    let version = emoji_id_bytes[NODE_ID_ARRAY_SIZE] << 2 >> 2; // Erase unused bits
    Ok((node_id, version))
}

/// An Iterator for traversing a set of bytes by grouping 10 bits at a time that can be used as the index in the emoji
/// dictionary. It will apply zero padding when needed.
pub struct EmojiIterator {
    cursor: usize,
    bit_count: usize,
    key: Vec<u8>,
}

impl EmojiIterator {
    /// Construct a new EmojiIterator from a set of bytes. The bit count limits the number of bits that will be used
    /// from the byte set.
    pub fn new(key: Vec<u8>, bit_count: usize) -> Self {
        Self {
            cursor: 0,
            bit_count,
            key,
        }
    }
}

impl Iterator for EmojiIterator {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor < self.bit_count {
            let index = self.cursor / 8;
            if index < self.key.len() {
                let offset = (self.cursor % 8) as u16;
                let key2 = if index + 1 < self.key.len() {
                    self.key[index + 1] as u16
                } else {
                    0u16
                };
                let bit_set = self.key[index] as u16 + (key2 << 8);
                let index = (bit_set >> offset) & 1023;
                self.cursor += 10;
                return Some(index as usize);
            }
        }
        None
    }
}

impl Display for EmojiId {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        fmt.write_str(
            &EmojiIterator::new(self.0.clone(), self.num_indice_bits())
                .map(|index| EMOJI[index as usize])
                .collect::<String>(),
        )
    }
}
// Converts a set of indices to bytes.
fn indices_to_bytes(indices: &Vec<usize>) -> Vec<u8> {
    let mut bits = Vec::<bool>::new();
    for index in indices {
        bits.append(&mut uint_to_bits(*index, 10));
    }
    // Zero padding
    let byte_aligment = bits.len() % 8;
    if byte_aligment > 0 {
        (0..(8 - byte_aligment)).for_each(|_| bits.push(false));
    }
    bits_to_bytes(&bits)
}

// Converts a set of bytes to emoji indices, only the bits upto the specified bit count will be considered.
fn bytes_to_indices(key_bytes: Vec<u8>, bit_count: usize) -> Vec<usize> {
    EmojiIterator::new(key_bytes, bit_count).collect::<Vec<usize>>()
}

// Finds the index of the specified emoji in the dictionary.
fn emoji_to_index(emoji: char) -> Result<usize, EmojiIdError> {
    for i in 0..EMOJI.len() {
        if emoji == EMOJI[i] {
            return Ok(i);
        }
    }
    Err(EmojiIdError::Notfound)
}

// Converts a set of emoji, provided in a string, to a list of dictionary indices.
fn emoji_set_to_indices(emoji_set: &str) -> Result<Vec<usize>, EmojiIdError> {
    let mut indices = Vec::<usize>::new();
    for emoji in emoji_set.chars() {
        indices.push(emoji_to_index(emoji)?);
    }
    Ok(indices)
}

// Checks that the provided indices exist in the emoji dictionary.
fn check_valid_indices(key_indices: &Vec<usize>) -> Result<(), EmojiIdError> {
    if key_indices.iter().any(|index| *index >= EMOJI_ID_DICTIONARY_LEN) {
        return Err(EmojiIdError::IndexOutOfBounds);
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::util::emoji::{emoji_id_to_node_id, EmojiId, EmojiIdError, NODE_ID_TO_EMOJI_ID_VERSION};
    use std::convert::TryFrom;
    use tari_comms::peer_manager::NodeId;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey, tari_utilities::byte_array::ByteArray};

    #[test]
    fn id_from_bytes() {
        let unchecked_bytes = [
            64, 28, 98, 64, 28, 197, 216, 115, 9, 25, 41, 76, 147, 195, 53, 207, 0, 145, 5, 55, 235, 244, 160, 195, 48,
            48, 144, 160, 71, 15, 241, 52,
        ];
        let desired_emoji_set = "😮👌🤣💝🤴🧘🤜😹😔🧚🥳🧞🤶😮💀🧍🚣😕😎💂🤢😒💨😕🤸🥰🦊";

        let emoji_id = EmojiId::new(unchecked_bytes.to_vec(), unchecked_bytes.len() * 8).unwrap();
        let emoji_set = emoji_id.to_string();
        assert_eq!(emoji_set, desired_emoji_set);
        let emoji_id = EmojiId::try_from(emoji_set.as_str()).unwrap();
        assert_eq!(emoji_id.to_string(), desired_emoji_set);

        let checked_bytes = [
            64, 28, 98, 64, 28, 197, 216, 115, 9, 25, 41, 76, 147, 195, 53, 207, 0, 145, 5, 55, 235, 244, 160, 195, 48,
            48, 144, 160, 71, 15, 241, 52, 128, 16,
        ];
        let emoji_id = EmojiId::try_from(checked_bytes.to_vec()).unwrap();
        assert_eq!(emoji_id.to_string(), desired_emoji_set);

        // Valid emoji set with invalid checksum
        let emoji_id = EmojiId::try_from("😮👌🤣💝🤴🧘🤜😹😔🧚🥳🧞🤶😮💀🧍🚣😕😎💂🤢😒💨😕🤸🥰💣");
        assert_eq!(emoji_id, Err(EmojiIdError::InvalidChecksum));

        // Invalid emoji set with valid checksum
        let emoji_id = EmojiId::try_from("😮👌🤣💝🤴💣🤜😹😔🧚🥳🧞🤶😮💀🧍🚣😕😎💂🤢😒💨😕🤸🥰🦊");
        assert_eq!(emoji_id, Err(EmojiIdError::InvalidChecksum));
    }

    #[test]
    fn id_from_indices() {
        let key_indices = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
        let desired_emoji_set = "😀😃😄😁😆😅🤣😂🙂🙃😉😊😇🥰😍🤩😘😗😚😙😋🎽";

        let emoji_id = EmojiId::new_from_indices(key_indices).unwrap();
        let emoji_set = emoji_id.to_string();
        assert_eq!(emoji_set, desired_emoji_set);
        let emoji_id = EmojiId::try_from(emoji_set.as_str()).unwrap();
        assert_eq!(emoji_id.to_string(), desired_emoji_set);

        // Invalid Indices
        let key_indices = vec![0, 1, 2, 3, 4, 1025, 5000];
        assert_eq!(
            EmojiId::new_from_indices(key_indices),
            Err(EmojiIdError::IndexOutOfBounds)
        );

        // Valid emoji set with invalid checksum
        let emoji_id = EmojiId::try_from("😀😃😄😁😆😅🤣😂🙂🙃😉😊😇🥰😍🤩😘😗😚😙😋💣");
        assert_eq!(emoji_id, Err(EmojiIdError::InvalidChecksum));

        // Invalid emoji set with valid checksum
        let emoji_id = EmojiId::try_from("😀😃😄😁😆😅🤣😂🙂🙃😉😊😇🥰😍🤩💣😗😚😙😋🎽");
        assert_eq!(emoji_id, Err(EmojiIdError::InvalidChecksum));
    }

    #[test]
    fn id_from_node_id() {
        let node_id = NodeId::new();
        let desired_emoji_set = "😀😀😀😀😀😀😀😀😀😀😘ˆ";
        let emoji_id = EmojiId::try_from(node_id.clone()).unwrap();
        assert_eq!(emoji_id.to_string(), desired_emoji_set);

        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let desired_node_id = NodeId::from_key(&pk).unwrap();
        let emoji_id = EmojiId::try_from(desired_node_id.clone()).unwrap();

        let (node_id, version) = emoji_id_to_node_id(emoji_id).unwrap();
        assert_eq!(node_id, desired_node_id);
        assert_eq!(version, NODE_ID_TO_EMOJI_ID_VERSION);
    }
}
