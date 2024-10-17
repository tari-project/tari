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

use std::{convert::TryFrom, str::FromStr};

use log::*;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_types::{
    emoji::EmojiId,
    tari_address::TariAddress,
    types::{PrivateKey, PublicKey, Signature},
};
use tari_network::{identity::PeerId, ToPeerId};
use tari_utilities::hex::{Hex, HexError};
use thiserror::Error;
use tokio::{runtime, runtime::Runtime};

pub const LOG_TARGET: &str = "minotari::application";

pub fn setup_runtime() -> Result<Runtime, ExitError> {
    let mut builder = runtime::Builder::new_multi_thread();
    builder.enable_all().build().map_err(|e| {
        let msg = format!("There was an error while building the node runtime. {}", e);
        ExitError::new(ExitCode::UnknownError, msg)
    })
}

/// Returns a CommsPublicKey from either a emoji id or a public key
pub fn parse_emoji_id_or_public_key(key: &str) -> Option<PublicKey> {
    EmojiId::from_str(&key.trim().replace('|', ""))
        .map(|emoji_id| PublicKey::from(&emoji_id))
        .or_else(|_| PublicKey::from_hex(key))
        .ok()
}

#[derive(Debug, Clone)]
pub struct UniPublicKey(PublicKey);

impl UniPublicKey {
    pub fn into_public_key(self) -> PublicKey {
        self.0
    }
}

impl FromStr for UniPublicKey {
    type Err = UniIdError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(emoji_id) = EmojiId::from_str(&key.trim().replace('|', "")) {
            Ok(Self(PublicKey::from(&emoji_id)))
        } else if let Ok(public_key) = PublicKey::from_hex(key) {
            Ok(Self(public_key))
        } else {
            Err(UniIdError::UnknownIdType)
        }
    }
}

impl From<UniPublicKey> for PublicKey {
    fn from(id: UniPublicKey) -> Self {
        id.0
    }
}

#[derive(Debug, Clone)]
pub enum UniPeerId {
    PublicKey(PublicKey),
    PeerId(PeerId),
    TariAddress(TariAddress),
}

impl ToPeerId for UniPeerId {
    fn to_peer_id(&self) -> PeerId {
        match self {
            UniPeerId::PublicKey(pk) => pk.to_peer_id(),
            UniPeerId::PeerId(p) => *p,
            UniPeerId::TariAddress(addr) => addr.comms_public_key().to_peer_id(),
        }
    }
}

#[derive(Debug, Error)]
pub enum UniIdError {
    #[error("unknown id type, expected emoji-id, public-key or node-id")]
    UnknownIdType,
    #[error("impossible to convert a value to the expected type")]
    NotConvertible,
}

impl FromStr for UniPeerId {
    type Err = UniIdError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(emoji_id) = EmojiId::from_str(&key.trim().replace('|', "")) {
            Ok(Self::PublicKey(PublicKey::from(&emoji_id)))
        } else if let Ok(peer_id) = PeerId::from_str(key.trim()) {
            Ok(Self::PeerId(peer_id))
        } else if let Ok(public_key) = PublicKey::from_hex(key) {
            Ok(Self::PublicKey(public_key))
        } else if let Ok(tari_address) = TariAddress::from_str(key) {
            Ok(Self::TariAddress(tari_address))
        } else {
            Err(UniIdError::UnknownIdType)
        }
    }
}

impl TryFrom<UniPeerId> for PublicKey {
    type Error = UniIdError;

    fn try_from(id: UniPeerId) -> Result<Self, Self::Error> {
        match id {
            UniPeerId::PublicKey(public_key) => Ok(public_key),
            UniPeerId::TariAddress(tari_address) => Ok(tari_address.public_spend_key().clone()),
            _ => Err(UniIdError::NotConvertible),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UniSignature(Signature);

impl FromStr for UniSignature {
    type Err = HexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let data = s.split(',').collect::<Vec<_>>();
        let signature = PrivateKey::from_hex(data[0])?;
        let public_nonce = PublicKey::from_hex(data[1])?;

        let signature = Signature::new(public_nonce, signature);
        Ok(Self(signature))
    }
}

impl From<UniSignature> for Signature {
    fn from(id: UniSignature) -> Self {
        id.0
    }
}
