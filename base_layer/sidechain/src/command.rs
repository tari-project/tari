// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::types::FixedHash;
use tari_hashing::layer2::command_hasher;

use crate::{eviction_proof::EvictNodeAtom, serde::hex_or_bytes};

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
    EvictNode(EvictNodeAtom),
    EndEpoch(EndEpochAtom),
}

impl Command {
    pub fn evict_node(&self) -> Option<&EvictNodeAtom> {
        match self {
            Self::EvictNode(evict_node_atom) => Some(evict_node_atom),
            _ => None,
        }
    }

    pub fn end_epoch(&self) -> Option<&EndEpochAtom> {
        match self {
            Self::EndEpoch(end_epoch_atom) => Some(end_epoch_atom),
            _ => None,
        }
    }

    pub fn hash(&self) -> FixedHash {
        command_hasher().chain(self).finalize().into()
    }
}

/// The atom committed by an `EndEpoch` command.
///
/// It carries the base-layer boundary-block hash of the *next* epoch so that the value is ratified
/// by the committee: a validator only votes for the end-of-epoch block if `next_epoch_hash` matches
/// its own (lagged, reorg-stable) view of the next epoch's boundary block. Because this hash is part
/// of the command, it is committed in the block's command merkle root and attested by the quorum
/// that commits the block — a node can no longer unilaterally lock an epoch hash the committee never
/// agreed on (which is what wedges consensus when a base-layer reorg deeper than the confirmation
/// depth straddles the epoch boundary).
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct EndEpochAtom {
    #[serde(with = "hex_or_bytes")]
    next_epoch_hash: FixedHash,
}

impl EndEpochAtom {
    pub fn new(next_epoch_hash: FixedHash) -> Self {
        Self { next_epoch_hash }
    }

    pub fn next_epoch_hash(&self) -> FixedHash {
        self.next_epoch_hash
    }
}

impl ToCommand for EndEpochAtom {
    fn to_command(&self) -> Command {
        Command::EndEpoch(self.clone())
    }
}
