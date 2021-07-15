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

use std::cmp::Ordering;

mod block;
mod committee;
pub mod domain_events;
mod hot_stuff_message;
mod hot_stuff_tree_node;
mod instruction;
mod instruction_set;
mod quorum_certificate;
mod replica_info;
mod view;
mod view_id;

pub use block::Block;
pub use committee::Committee;
pub use hot_stuff_message::HotStuffMessage;
pub use hot_stuff_tree_node::HotStuffTreeNode;
pub use instruction::Instruction;
pub use instruction_set::InstructionSet;
pub use quorum_certificate::QuorumCertificate;
pub use replica_info::ReplicaInfo;
use std::{
    fmt,
    fmt::{Debug, Formatter},
    hash::Hash,
};
pub use view::View;
pub use view_id::ViewId;

pub struct InstructionId(u64);

pub struct InstructionCaller {
    owner_token_id: TokenId,
}

impl InstructionCaller {
    pub fn owner_token_id(&self) -> &TokenId {
        &self.owner_token_id
    }
}

pub enum TemplateId {
    EditableMetadata,
}

#[derive(Clone, Debug, Hash)]
pub struct TokenId(pub Vec<u8>);

impl TokenId {
    fn as_bytes(&self) -> &[u8] {
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

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct TreeNodeHash(pub Vec<u8>);

impl TreeNodeHash {
    fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// TODO: Perhaps should be CoW instead of Clone
pub trait Payload: Hash + Debug + Clone + AsRef<[u8]> + Send + Sync + PartialEq {}

impl Payload for &str {}

impl Payload for String {}

pub trait Event: Clone + Send + Sync {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConsensusWorkerState {
    Starting,
    Prepare,
    PreCommit,
    Commit,
    Decide,
    NextView,
}

#[derive(Clone, Debug)]
pub struct Signature {}

impl Signature {
    pub fn combine(&self, other: &Signature) -> Signature {
        other.clone()
    }
}
