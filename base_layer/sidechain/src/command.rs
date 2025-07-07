// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::types::FixedHash;
use tari_hashing::layer2::command_hasher;

use crate::eviction_proof::EvictNodeAtom;

pub trait ToCommand {
    fn to_command(&self) -> Command;
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum Command {
    LocalOnly,
    LocalPrepare,
    LocalAccept,
    AllAccept,
    SomeAccept,
    ForeignProposal,
    MintConfidentialOutput,
    EvictNode(EvictNodeAtom),
    EndEpoch,
}

impl Command {
    pub fn evict_node(&self) -> Option<&EvictNodeAtom> {
        match self {
            Self::EvictNode(evict_node_atom) => Some(evict_node_atom),
            _ => None,
        }
    }

    pub fn hash(&self) -> FixedHash {
        command_hasher().chain(self).finalize().into()
    }
}
