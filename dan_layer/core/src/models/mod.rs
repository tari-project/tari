// Copyright 2021. The Tari Project
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

use std::{convert::TryFrom, fmt::Debug, hash::Hash};

mod asset_definition;
mod base_layer_metadata;
mod base_layer_output;
mod checkpoint_challenge;
mod committee;
pub mod domain_events;
mod error;
mod hot_stuff_message;
mod hot_stuff_tree_node;
mod instruction_set;
mod node;
mod payload;
mod quorum_certificate;
mod sidechain_block;
mod sidechain_metadata;
mod tari_dan_payload;
mod tree_node_hash;
mod view;
mod view_id;

pub use asset_definition::{AssetDefinition, InitialState};
pub use base_layer_metadata::BaseLayerMetadata;
pub use base_layer_output::{BaseLayerOutput, CheckpointOutput, CommitteeOutput};
pub use checkpoint_challenge::CheckpointChallenge;
pub use committee::Committee;
pub use error::ModelError;
pub use hot_stuff_message::HotStuffMessage;
pub use hot_stuff_tree_node::HotStuffTreeNode;
pub use instruction_set::InstructionSet;
pub use node::Node;
pub use payload::Payload;
pub use quorum_certificate::QuorumCertificate;
pub use sidechain_block::SideChainBlock;
pub use sidechain_metadata::SidechainMetadata;
pub use tari_dan_payload::{CheckpointData, TariDanPayload};
pub use tree_node_hash::TreeNodeHash;
pub use view::View;
pub use view_id::ViewId;

// TODO: encapsulate
pub struct InstructionCaller {
    pub owner_token_id: TokenId,
}

impl InstructionCaller {
    pub fn _owner_token_id(&self) -> &TokenId {
        &self.owner_token_id
    }
}

#[derive(Clone, Debug, Hash)]
pub struct TokenId(pub Vec<u8>);

impl TokenId {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl AsRef<[u8]> for TokenId {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum HotStuffMessageType {
    NewView,
    Prepare,
    PreCommit,
    Commit,
    Decide,
    // Special type
    Genesis,
}

impl HotStuffMessageType {
    pub fn as_u8(&self) -> u8 {
        match self {
            HotStuffMessageType::NewView => 1,
            HotStuffMessageType::Prepare => 2,
            HotStuffMessageType::PreCommit => 3,
            HotStuffMessageType::Commit => 4,
            HotStuffMessageType::Decide => 5,
            HotStuffMessageType::Genesis => 255,
        }
    }
}

impl TryFrom<u8> for HotStuffMessageType {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(HotStuffMessageType::NewView),
            2 => Ok(HotStuffMessageType::Prepare),
            3 => Ok(HotStuffMessageType::PreCommit),
            4 => Ok(HotStuffMessageType::Commit),
            5 => Ok(HotStuffMessageType::Decide),
            255 => Ok(HotStuffMessageType::Genesis),
            _ => Err("Not a value message type".to_string()),
        }
    }
}

pub trait ConsensusHash {
    fn consensus_hash(&self) -> &[u8];
}

impl ConsensusHash for &str {
    fn consensus_hash(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl ConsensusHash for String {
    fn consensus_hash(&self) -> &[u8] {
        self.as_bytes()
    }
}

pub trait Event: Clone + Send + Sync {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusWorkerState {
    Starting,
    Synchronizing,
    Prepare,
    PreCommit,
    Commit,
    Decide,
    NextView,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorSignature {}

impl ValidatorSignature {
    pub fn from_bytes(_source: &[u8]) -> Self {
        Self {}
    }

    pub fn combine(&self, other: &ValidatorSignature) -> ValidatorSignature {
        other.clone()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        vec![]
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ChainHeight(u64);

impl From<ChainHeight> for u64 {
    fn from(c: ChainHeight) -> Self {
        c.0
    }
}

impl From<u64> for ChainHeight {
    fn from(v: u64) -> Self {
        ChainHeight(v)
    }
}
