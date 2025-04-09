// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::{epoch::VnEpoch, types::CompressedPublicKey};
use tari_utilities::ByteArray;

use super::error::SidechainProofValidationError;
use crate::{
    command::{Command, ToCommand},
    commit_proof::CommandCommitProof,
    shard_group::ShardGroup,
    CheckVnFunc,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct EvictionProof {
    proof: CommandCommitProof<EvictNodeAtom>,
}

impl EvictionProof {
    pub fn new(proof: CommandCommitProof<EvictNodeAtom>) -> Self {
        Self { proof }
    }

    pub fn proof(&self) -> &CommandCommitProof<EvictNodeAtom> {
        &self.proof
    }

    pub fn epoch(&self) -> VnEpoch {
        self.proof.epoch()
    }

    pub fn shard_group(&self) -> ShardGroup {
        self.proof.shard_group()
    }

    pub fn node_to_evict(&self) -> &CompressedPublicKey {
        self.proof.command().node_to_evict()
    }

    pub fn validate(
        &self,
        quorum_threshold: usize,
        check_vn: &CheckVnFunc<'_>,
    ) -> Result<(), SidechainProofValidationError> {
        self.proof.validate_committed(quorum_threshold, check_vn)
    }

    pub fn sidechain_id_message(&self) -> &[u8] {
        self.node_to_evict().as_bytes()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct EvictNodeAtom {
    public_key: CompressedPublicKey,
}

impl EvictNodeAtom {
    pub fn new(public_key: CompressedPublicKey) -> Self {
        Self { public_key }
    }

    pub fn node_to_evict(&self) -> &CompressedPublicKey {
        &self.public_key
    }
}

impl ToCommand for EvictNodeAtom {
    fn to_command(&self) -> Command {
        Command::EvictNode(self.clone())
    }
}
