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

use serde::export::{fmt::Error, Formatter};
use std::fmt::Display;
use tari_core::transactions::types::PublicKey;
use tari_crypto::tari_utilities::{
    hex::{Hex, HexError},
    ByteArray,
    ByteArrayError,
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EmojiId(String);

impl EmojiId {
    pub fn from_pubkey(key: &PublicKey) -> Self {
        // Temp hacky approach - full spec coming shortly
        let bytes = key.as_bytes();
        let id = bytes.iter().map(|b| EMOJI[*b as usize]).collect();
        Self(id)
    }

    pub fn from_hex(hex_key: &str) -> Result<Self, HexError> {
        let key = PublicKey::from_hex(hex_key)?;
        Ok(EmojiId::from_pubkey(&key))
    }

    /// Given a emoji string in `value` returns true if this is a representable as a public key
    pub fn is_valid(value: &str) -> bool {
        EmojiId::try_convert_to_pubkey(value).is_ok()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert an emoji id to a pubic key
    pub fn try_convert_to_pubkey(value: &str) -> Result<PublicKey, ByteArrayError> {
        let bytes: Vec<u8> = value
            .chars()
            .filter_map(|c| EMOJI.iter().position(|e| *e == c))
            .map(|v| v as u8)
            .collect();
        PublicKey::from_bytes(&bytes)
    }
}

impl Display for EmojiId {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        fmt.write_str(self.as_str())
    }
}

const EMOJI: [char; 256] = [
    '😀', '😃', '😄', '😁', '😆', '😅', '🤣', '😂', '🙂', '🙃', '😉', '😊', '😇', '🥰', '😍', '🤩', '😘', '😗', '😚',
    '😙', '😋', '😛', '😜', '🤪', '😝', '🐠', '🤗', '🤭', '🤫', '🤔', '🤐', '🤨', '😐', '😑', '😶', '😏', '😒', '🙄',
    '😬', '🤥', '😌', '😔', '😪', '🤤', '😴', '😷', '🤒', '🤕', '🤢', '🤮', '🤧', '🥵', '🥶', '🥴', '😵', '🤯', '🤠', '🥳',
    '😎', '🤓', '🧐', '😕', '😟', '🙁', '😮', '😯', '😲', '😳', '🥺', '😦', '😧', '😨', '😰', '😥', '😢', '😭', '😱',
    '😖', '😣', '😞', '😓', '😩', '😫', '😤', '😡', '😠', '🤬', '😈', '👿', '💀', '🐟', '💩', '🤡', '👹', '👺', '👻',
    '👽', '👾', '🤖', '😺', '😹', '😻', '😼', '😽', '🙀', '😿', '😾', '💋', '👋', '🤚', '🖐', '✋', '🖖', '👌', '🤞',
    '🤟', '🤘', '🤙', '👈', '👉', '👆', '🖕', '👇', '👍', '👎', '✊', '👊', '🤛', '🤜', '👏', '🙌', '👐', '🤲', '🤝',
    '🙏', '💅', '🤳', '💪', '🦵', '🦶', '👂', '👃', '🧠', '🦷', '🦴', '👀', '👁', '👅', '👄', '🚶', '👣', '🧳', '🌂', '☂',
    '🧵', '🧶', '👓', '🕶', '🥽', '🥼', '👔', '👕', '👖', '🧣', '🧤', '🧥', '🧦', '👗', '👘', '👙', '👚', '👛', '👜', '👝',
    '🎒', '👞', '👟', '🥾', '🥿', '👠', '👡', '👢', '👑', '👒', '🎩', '🎓', '🧢', '⛑', '💄', '💍', '💼', '🙈', '🙉',
    '🙊', '💥', '💫', '💦', '💨', '🐵', '🐒', '🦍', '🐶', '🐕', '🐩', '🐺', '🦊', '🦝', '🐱', '🐈', '🦁', '🐯', '🐅',
    '🐆', '🐴', '🐎', '🦄', '🦓', '🦌', '🐮', '🐂', '🐃', '🐄', '🐷', '🐖', '🐗', '🐽', '🐏', '🐑', '🐐', '🐪', '🐫',
    '🦙', '🦒', '🐘', '🦏', '🦛', '🐭', '🐁', '🐀', '🐹', '🐰', '🐇', '🐿', '🦔', '🦇', '🐻', '🐨', '🐼', '🦘', '🦡', '🐾',
    '🦃', '🐓', '🐣', '🐋', '🐬',
];

#[cfg(test)]
mod test {
    use crate::util::emoji::EmojiId;
    use tari_core::transactions::types::PublicKey;
    use tari_crypto::tari_utilities::hex::Hex;

    #[test]
    fn convert_key() {
        let key = PublicKey::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a").unwrap();
        let eid = EmojiId::from_pubkey(&key);
        assert_eq!(
            eid.as_str(),
            "🖖🥴😍🙃💦🤘🤜👁🙃🙌😱🖐🙀🤳🖖👍✊🐈☂💀👚😶🤟😳👢😘😺🙌🎩🤬🐼😎"
        );
        let h_eid = EmojiId::from_hex("70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a").unwrap();
        assert_eq!(eid, h_eid);
    }

    #[test]
    fn is_valid() {
        let key =
            EmojiId::try_convert_to_pubkey("🖖🥴😍🙃💦🤘🤜👁🙃🙌😱🖐🙀🤳🖖👍✊🐈☂💀👚😶🤟😳👢😘😺🙌🎩🤬🐼😎").unwrap();
        assert_eq!(
            key.to_hex(),
            "70350e09c474809209824c6e6888707b7dd09959aa227343b5106382b856f73a"
        );
        assert!(EmojiId::is_valid(
            "🖖🥴😍🙃💦🤘🤜👁🙃🙌😱🖐🙀🤳🖖👍✊🐈☂💀👚😶🤟😳👢😘😺🙌🎩🤬🐼😎"
        ));
        assert_eq!(EmojiId::is_valid("😂"), false);
        assert_eq!(EmojiId::is_valid("Hi"), false);
    }
}
