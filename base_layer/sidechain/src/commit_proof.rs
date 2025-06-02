// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt::Display;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, FixedHash, PrivateKey, UncompressedPublicKey},
};
use tari_crypto::signatures::CompressedSchnorrSignature;
use tari_hashing::{layer2, ValidatorNodeHashDomain};
use tari_jellyfish::{LeafKey, SparseMerkleProofExt, TreeHash};
use tari_utilities::ByteArray;

use super::error::SidechainProofValidationError;
use crate::{
    command::{Command, ToCommand},
    serde::hex_or_bytes,
    shard_group::ShardGroup,
    validations::check_proof_elements,
};

pub type ValidatorBlockSignature =
    CompressedSchnorrSignature<UncompressedPublicKey, PrivateKey, ValidatorNodeHashDomain>;
pub type CheckVnFunc<'a> = dyn Fn(&CompressedPublicKey) -> Result<bool, SidechainProofValidationError> + 'a;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum CommandCommitProof<C> {
    V1(CommandCommitProofV1<C>),
}

impl<C: ToCommand> CommandCommitProof<C> {
    pub fn new(command: C, commit_proof: SidechainBlockCommitProof, inclusion_proof: SparseMerkleProofExt) -> Self {
        Self::V1(CommandCommitProofV1 {
            command,
            commit_proof,
            inclusion_proof,
        })
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
    pub command: C,
    pub commit_proof: SidechainBlockCommitProof,
    pub inclusion_proof: SparseMerkleProofExt,
}

impl<C: ToCommand> CommandCommitProofV1<C> {
    pub fn command(&self) -> &C {
        &self.command
    }

    pub fn commit_proof(&self) -> &SidechainBlockCommitProof {
        &self.commit_proof
    }

    pub fn inclusion_proof(&self) -> &SparseMerkleProofExt {
        &self.inclusion_proof
    }

    fn validate_inclusion_proof(&self, command: &Command) -> Result<(), SidechainProofValidationError> {
        let command_hash = TreeHash::new(command.hash().into_array());
        // Command JMT uses an identity mapping between hashes and keys.
        let key = LeafKey::new(command_hash);
        let root_hash = TreeHash::new(self.commit_proof.header.command_merkle_root.into_array());
        self.inclusion_proof.verify_inclusion(&root_hash, &key, &command_hash)?;
        Ok(())
    }

    pub fn validate_committed(
        &self,
        quorum_threshold: usize,
        check_vn: &CheckVnFunc<'_>,
    ) -> Result<(), SidechainProofValidationError> {
        let command = self.command.to_command();
        self.validate_inclusion_proof(&command)?;
        self.commit_proof.validate_committed(quorum_threshold, check_vn)
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
        quorum_threshold: usize,
        check_vn: &CheckVnFunc<'_>,
    ) -> Result<(), SidechainProofValidationError> {
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

    pub fn last_qc(&self) -> Option<&QuorumCertificate> {
        self.proof_elements
            .iter()
            .filter_map(|elem| match elem {
                CommitProofElement::QuorumCertificate(qc) => Some(qc),
                _ => None,
            })
            .next_back()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub enum CommitProofElement {
    QuorumCertificate(QuorumCertificate),
    ChainLinks(Vec<ChainLink>),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ChainLink {
    #[serde(with = "hex_or_bytes")]
    pub header_hash: FixedHash,
    #[serde(with = "hex_or_bytes")]
    pub parent_id: FixedHash,
}

impl ChainLink {
    pub fn calc_block_id(&self) -> FixedHash {
        layer2::block_hasher()
            .chain(&self.parent_id)
            .chain(&self.header_hash)
            .finalize()
            .into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct SidechainBlockHeader {
    pub network: u8,
    #[serde(with = "hex_or_bytes")]
    pub parent_id: FixedHash,
    #[serde(with = "hex_or_bytes")]
    pub justify_id: FixedHash,
    pub height: u64,
    pub epoch: u64,
    pub shard_group: ShardGroup,
    pub proposed_by: CompressedPublicKey,
    #[serde(with = "hex_or_bytes")]
    pub state_merkle_root: FixedHash,
    #[serde(with = "hex_or_bytes")]
    pub command_merkle_root: FixedHash,
    /// Signature of block by the proposer.
    pub signature: ValidatorBlockSignature,
    #[serde(with = "hex_or_bytes")]
    pub metadata_hash: FixedHash,
}

impl SidechainBlockHeader {
    pub fn calculate_hash(&self) -> FixedHash {
        let fields = BlockHeaderHashFields::V1(BlockHeaderHashFieldsV1 {
            network: self.network,
            justify_id: &self.justify_id,
            height: self.height,
            epoch: self.epoch,
            shard_group: self.shard_group,
            proposed_by: self.proposed_by.as_bytes(),
            state_merkle_root: &self.state_merkle_root,
            command_merkle_root: &self.command_merkle_root,
            metadata_hash: &self.metadata_hash,
        });

        layer2::block_hasher().chain(&fields).finalize().into()
    }

    pub fn calculate_block_id(&self) -> FixedHash {
        let header_hash = self.calculate_hash();
        layer2::block_hasher()
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
    #[serde(with = "hex_or_bytes")]
    pub header_hash: FixedHash,
    #[serde(with = "hex_or_bytes")]
    pub parent_id: FixedHash,
    pub signatures: Vec<ValidatorQcSignature>,
    pub decision: QuorumDecision,
}

impl QuorumCertificate {
    pub fn calculate_justified_block(&self) -> FixedHash {
        layer2::block_hasher()
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

impl QuorumDecision {
    pub fn is_accept(&self) -> bool {
        matches!(self, QuorumDecision::Accept)
    }

    pub fn is_reject(&self) -> bool {
        matches!(self, QuorumDecision::Reject)
    }
}

impl QuorumDecision {
    pub fn as_u8(&self) -> u8 {
        match self {
            QuorumDecision::Accept => 0,
            QuorumDecision::Reject => 1,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(QuorumDecision::Accept),
            1 => Some(QuorumDecision::Reject),
            _ => None,
        }
    }
}

impl TryFrom<u8> for QuorumDecision {
    type Error = InvalidQuorumDecisionByteError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_u8(value).ok_or(InvalidQuorumDecisionByteError(value))
    }
}

impl Display for QuorumDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuorumDecision::Accept => write!(f, "Accept"),
            QuorumDecision::Reject => write!(f, "Reject"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid quorum decision byte: {0}")]
pub struct InvalidQuorumDecisionByteError(u8);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
pub struct ValidatorQcSignature {
    pub public_key: CompressedPublicKey,
    pub signature: ValidatorBlockSignature,
}

impl ValidatorQcSignature {
    #[must_use]
    pub fn verify(&self, block_id: &FixedHash, decision: QuorumDecision) -> bool {
        let Ok(public_key) = self.public_key.to_public_key() else {
            return false;
        };

        let Ok(signature) = self.signature.to_schnorr_signature() else {
            return false;
        };

        let fields = ProposalCertificateSignatureFields { block_id, decision };

        let message = layer2::proposal_vote_signature_hasher().chain(&fields).finalize();
        signature.verify(&public_key, message)
    }

    pub fn public_key(&self) -> &CompressedPublicKey {
        &self.public_key
    }

    pub fn signature(&self) -> &ValidatorBlockSignature {
        &self.signature
    }
}

#[derive(Debug, BorshSerialize)]
pub struct ProposalCertificateSignatureFields<'a> {
    pub block_id: &'a FixedHash,
    pub decision: QuorumDecision,
}

#[derive(Debug, BorshSerialize)]
pub enum BlockHeaderHashFields<'a> {
    V1(BlockHeaderHashFieldsV1<'a>),
}

#[derive(Debug, BorshSerialize)]
pub struct BlockHeaderHashFieldsV1<'a> {
    pub network: u8,
    pub justify_id: &'a FixedHash,
    pub height: u64,
    pub epoch: u64,
    pub shard_group: ShardGroup,
    // NOTE this is borsh encoded as variable length bytes - technically should always be 32
    pub proposed_by: &'a [u8],
    pub state_merkle_root: &'a FixedHash,
    pub command_merkle_root: &'a FixedHash,
    pub metadata_hash: &'a FixedHash,
}
