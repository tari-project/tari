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

use std::{
    convert::{TryFrom, TryInto},
    fmt::{Debug, Display, Formatter},
    hash::Hash,
    str::FromStr,
};

mod asset_definition;
mod base_layer_metadata;
mod base_layer_output;
mod committee;
pub mod domain_events;
mod error;
mod hot_stuff_message;
mod hot_stuff_tree_node;
mod instruction;
mod instruction_set;
mod node;
mod op_log;
mod payload;
mod quorum_certificate;
mod sidechain_block;
mod sidechain_metadata;
mod state_root;
mod tari_dan_payload;
mod tree_node_hash;
mod view;
mod view_id;

pub use asset_definition::{AssetDefinition, InitialState, KeyValue, SchemaState};
pub use base_layer_metadata::BaseLayerMetadata;
pub use base_layer_output::{BaseLayerOutput, CheckpointOutput};
pub use committee::Committee;
pub use error::ModelError;
pub use hot_stuff_message::HotStuffMessage;
pub use hot_stuff_tree_node::HotStuffTreeNode;
pub use instruction::Instruction;
pub use instruction_set::InstructionSet;
pub use node::Node;
pub use op_log::{StateOpLogEntry, StateOperation};
pub use payload::Payload;
pub use quorum_certificate::QuorumCertificate;
pub use sidechain_block::SideChainBlock;
pub use sidechain_metadata::SidechainMetadata;
pub use state_root::StateRoot;
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

#[derive(Copy, Clone, Debug)]
pub enum TemplateId {
    Tip002 = 2,
    Tip003 = 3,
    Tip004 = 4,
    Tip721 = 721,
    EditableMetadata = 20,
}

impl FromStr for TemplateId {
    type Err = ModelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Tip002" => Ok(TemplateId::Tip002),
            "Tip003" => Ok(TemplateId::Tip003),
            "Tip004" => Ok(TemplateId::Tip004),
            "Tip721" => Ok(TemplateId::Tip721),
            "EditableMetadata" => Ok(TemplateId::EditableMetadata),
            _ => {
                dbg!("Unrecognised template");
                Err(ModelError::StringParseError {
                    details: format!("Unrecognised template ID '{}'", s),
                })
            },
        }
    }
}

impl TryFrom<u32> for TemplateId {
    type Error = ModelError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            2 => Ok(TemplateId::Tip002),
            3 => Ok(TemplateId::Tip003),
            4 => Ok(TemplateId::Tip004),
            721 => Ok(TemplateId::Tip721),
            _ => Err(ModelError::InvalidTemplateIdNumber { value: value as i64 }),
        }
    }
}

impl TryFrom<i32> for TemplateId {
    type Error = ModelError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map_err(|_| ModelError::InvalidTemplateIdNumber { value: value as i64 })?
            .try_into()
    }
}

impl Display for TemplateId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct Signature {}

impl Signature {
    pub fn from_bytes(_source: &[u8]) -> Self {
        Self {}
    }

    pub fn combine(&self, other: &Signature) -> Signature {
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
