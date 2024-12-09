// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::{
    epoch::VnEpoch,
    types::{FixedHash, PrivateKey, PublicKey},
};
use tari_crypto::signatures::SchnorrSignature;
use tari_hashing::{
    layer2::{block_hasher, vote_signature_hasher},
    ValidatorNodeHashDomain,
};

use super::error::SidechainProofValidationError;
use crate::{
    command::{Command, ToCommand},
    shard_group::ShardGroup,
    validations::{check_command_inclusion_proof, check_proof_elements},
};

pub type ValidatorBlockSignature = SchnorrSignature<PublicKey, PrivateKey, ValidatorNodeHashDomain>;
pub type CheckVnFunc<'a> = dyn Fn(&PublicKey) -> Result<bool, SidechainProofValidationError> + 'a;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum CommandCommitProof<C> {
    V1(CommandCommitProofV1<C>),
}

impl<C: ToCommand> CommandCommitProof<C> {
    pub fn new(command: C, commit_proof: SidechainBlockCommitProof) -> Self {
        Self::V1(CommandCommitProofV1 { command, commit_proof })
    }

    pub fn command(&self) -> &C {
        match self {
            CommandCommitProof::V1(v1) => &v1.command,
        }
    }

    pub fn header(&self) -> &SidechainBlockHeader {
        match self {
            CommandCommitProof::V1(v1) => &v1.commit_proof.header,
        }
    }

    pub fn epoch(&self) -> VnEpoch {
        match self {
            CommandCommitProof::V1(v1) => VnEpoch(v1.commit_proof.header().epoch),
        }
    }

    pub fn shard_group(&self) -> ShardGroup {
        match self {
            CommandCommitProof::V1(v1) => v1.commit_proof.header().shard_group,
        }
    }

    pub fn validate_committed(
        &self,
        quorum_threshold: usize,
        check_vn: &CheckVnFunc<'_>,
    ) -> Result<(), SidechainProofValidationError> {
        #[allow(clippy::single_match)]
        match self {
            CommandCommitProof::V1(v1) => v1.validate_committed(quorum_threshold, check_vn),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct CommandCommitProofV1<C> {
    // TODO: Implement MerkleProof
    // command_merkle_proof: MerkleProof,
    pub command: C,
    pub commit_proof: SidechainBlockCommitProof,
}

impl<C: ToCommand> CommandCommitProofV1<C> {
    pub fn command(&self) -> &C {
        &self.command
    }

    pub fn commit_proof(&self) -> &SidechainBlockCommitProof {
        &self.commit_proof
    }

    pub fn validate_committed(
        &self,
        quorum_threshold: usize,
        check_vn: &CheckVnFunc<'_>,
    ) -> Result<(), SidechainProofValidationError> {
        let command = self.command.to_command();
        self.commit_proof
            .validate_committed(&command, quorum_threshold, check_vn)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct SidechainBlockCommitProof {
    pub header: SidechainBlockHeader,
    pub proof_elements: Vec<CommitProofElement>,
}

impl SidechainBlockCommitProof {
    pub fn validate_committed(
        &self,
        command: &Command,
        quorum_threshold: usize,
        check_vn: &CheckVnFunc<'_>,
    ) -> Result<(), SidechainProofValidationError> {
        check_command_inclusion_proof(&self.header, command)?;
        check_proof_elements(
            &self.header,
            &self.proof_elements,
            check_vn,
            QuorumDecision::Accept,
            quorum_threshold,
        )?;

        Ok(())
    }

    pub fn proof_elements(&self) -> &[CommitProofElement] {
        &self.proof_elements
    }

    pub fn header(&self) -> &SidechainBlockHeader {
        &self.header
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum CommitProofElement {
    QuorumCertificate(QuorumCertificate),
    DummyChain(Vec<ChainLink>),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ChainLink {
    pub header_hash: FixedHash,
    pub parent_id: FixedHash,
}

impl ChainLink {
    pub fn calc_block_id(&self) -> FixedHash {
        block_hasher()
            .chain(&self.parent_id)
            .chain(&self.header_hash)
            .finalize()
            .into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct SidechainBlockHeader {
    pub network: u8,
    pub parent_id: FixedHash,
    pub justify_id: FixedHash,
    pub height: u64,
    pub epoch: u64,
    pub shard_group: ShardGroup,
    pub proposed_by: PublicKey,
    pub total_leader_fee: u64,
    pub state_merkle_root: FixedHash,
    pub command_merkle_root: FixedHash,
    /// If the block is a dummy block.
    pub is_dummy: bool,
    pub foreign_indexes_hash: FixedHash,
    /// Signature of block by the proposer.
    pub signature: ValidatorBlockSignature,
    pub timestamp: u64,
    pub base_layer_block_height: u64,
    pub base_layer_block_hash: FixedHash,
    pub extra_data_hash: FixedHash,
}

impl SidechainBlockHeader {
    pub fn calculate_hash(&self) -> FixedHash {
        block_hasher()
            .chain(&self.network)
            .chain(&self.justify_id)
            .chain(&self.height)
            .chain(&self.total_leader_fee)
            .chain(&self.epoch)
            .chain(&self.shard_group)
            .chain(&self.proposed_by)
            .chain(&self.state_merkle_root)
            .chain(&self.is_dummy)
            .chain(&self.command_merkle_root)
            .chain(&self.foreign_indexes_hash)
            .chain(&self.timestamp)
            .chain(&self.base_layer_block_height)
            .chain(&self.base_layer_block_hash)
            .chain(&self.extra_data_hash)
            .finalize()
            .into()
    }

    pub fn calculate_block_id(&self) -> FixedHash {
        let header_hash = self.calculate_hash();
        block_hasher()
            .chain(&self.parent_id)
            .chain(&header_hash)
            .finalize()
            .into()
    }

    pub fn signature(&self) -> &ValidatorBlockSignature {
        &self.signature
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct QuorumCertificate {
    pub header_hash: FixedHash,
    pub parent_id: FixedHash,
    pub signatures: Vec<ValidatorQcSignature>,
    pub decision: QuorumDecision,
}

impl QuorumCertificate {
    pub fn calculate_justified_block(&self) -> FixedHash {
        block_hasher()
            .chain(&self.parent_id)
            .chain(&self.header_hash)
            .finalize()
            .into()
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum QuorumDecision {
    Accept,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorQcSignature {
    pub public_key: PublicKey,
    pub signature: ValidatorBlockSignature,
}

impl ValidatorQcSignature {
    #[must_use]
    pub fn verify(&self, block_id: &FixedHash, decision: QuorumDecision) -> bool {
        let message = vote_signature_hasher().chain(block_id).chain(&decision).finalize();
        self.signature.verify(&self.public_key, message)
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn signature(&self) -> &ValidatorBlockSignature {
        &self.signature
    }
}
