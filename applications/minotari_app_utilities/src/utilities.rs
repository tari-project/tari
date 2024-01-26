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

use futures::future::Either;
use log::*;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_common_types::{
    emoji::EmojiId,
    tari_address::TariAddress,
    types::{BlockHash, PublicKey},
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_utilities::hex::Hex;
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
pub fn parse_emoji_id_or_public_key(key: &str) -> Option<CommsPublicKey> {
    EmojiId::from_emoji_string(&key.trim().replace('|', ""))
        .map(|emoji_id| emoji_id.to_public_key())
        .or_else(|_| CommsPublicKey::from_hex(key))
        .ok()
}

/// Returns a hash from a hex string
pub fn parse_hash(hash_string: &str) -> Option<BlockHash> {
    BlockHash::from_hex(hash_string).ok()
}

/// Returns a CommsPublicKey from either a emoji id, a public key or node id
pub fn parse_emoji_id_or_public_key_or_node_id(key: &str) -> Option<Either<CommsPublicKey, NodeId>> {
    parse_emoji_id_or_public_key(key)
        .map(Either::Left)
        .or_else(|| NodeId::from_hex(key).ok().map(Either::Right))
}

pub fn either_to_node_id(either: Either<CommsPublicKey, NodeId>) -> NodeId {
    match either {
        Either::Left(pk) => NodeId::from_public_key(&pk),
        Either::Right(n) => n,
    }
}

#[derive(Debug, Clone)]
pub struct UniPublicKey(PublicKey);

impl FromStr for UniPublicKey {
    type Err = UniIdError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(emoji_id) = EmojiId::from_emoji_string(&key.trim().replace('|', "")) {
            Ok(Self(emoji_id.to_public_key()))
        } else if let Ok(public_key) = PublicKey::from_hex(key) {
            Ok(Self(public_key))
        } else if let Ok(tari_address) = TariAddress::from_hex(key) {
            Ok(Self(tari_address.public_key().clone()))
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

#[derive(Debug)]
pub enum UniNodeId {
    PublicKey(PublicKey),
    NodeId(NodeId),
    TariAddress(TariAddress),
}

#[derive(Debug, Error)]
pub enum UniIdError {
    #[error("unknown id type, expected emoji-id, public-key or node-id")]
    UnknownIdType,
    #[error("impossible convert a value to the expected type")]
    Nonconvertible,
}

impl FromStr for UniNodeId {
    type Err = UniIdError;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if let Ok(emoji_id) = EmojiId::from_emoji_string(&key.trim().replace('|', "")) {
            Ok(Self::PublicKey(emoji_id.to_public_key()))
        } else if let Ok(public_key) = PublicKey::from_hex(key) {
            Ok(Self::PublicKey(public_key))
        } else if let Ok(node_id) = NodeId::from_hex(key) {
            Ok(Self::NodeId(node_id))
        } else if let Ok(tari_address) = TariAddress::from_hex(key) {
            Ok(Self::TariAddress(tari_address))
        } else {
            Err(UniIdError::UnknownIdType)
        }
    }
}

impl TryFrom<UniNodeId> for PublicKey {
    type Error = UniIdError;

    fn try_from(id: UniNodeId) -> Result<Self, Self::Error> {
        match id {
            UniNodeId::PublicKey(public_key) => Ok(public_key),
            UniNodeId::TariAddress(tari_address) => Ok(tari_address.public_key().clone()),
            UniNodeId::NodeId(_) => Err(UniIdError::Nonconvertible),
        }
    }
}

impl From<UniNodeId> for NodeId {
    fn from(id: UniNodeId) -> Self {
        match id {
            UniNodeId::PublicKey(public_key) => NodeId::from_public_key(&public_key),
            UniNodeId::NodeId(node_id) => node_id,
            UniNodeId::TariAddress(tari_address) => NodeId::from_public_key(tari_address.public_key()),
        }
    }
}
