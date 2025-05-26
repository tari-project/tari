// Copyright 2019, The Tari Project
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

//! Impls for sidechain_feature proto

use std::convert::{TryFrom, TryInto};

use prost::Message;
use tari_common::configuration::Network;
use tari_common_types::types::{CompressedPublicKey, Signature};
use tari_max_size::MaxSizeString;
use tari_sidechain::{
    ChainLink,
    CommandCommitProof,
    CommandCommitProofV1,
    CommitProofElement,
    EvictNodeAtom,
    EvictionProof,
    QuorumCertificate,
    QuorumDecision,
    ShardGroup,
    SidechainBlockCommitProof,
    SidechainBlockHeader,
    ValidatorQcSignature,
};
use tari_utilities::ByteArray;

use crate::{
    proto,
    transactions::transaction_components::{
        BuildInfo,
        CodeTemplateRegistration,
        ConfidentialOutputData,
        SideChainFeature,
        SideChainFeatureData,
        SideChainId,
        TemplateType,
        ValidatorNodeExit,
        ValidatorNodeRegistration,
        ValidatorNodeSignature,
    },
};

//---------------------------------- SideChainFeature --------------------------------------------//
impl From<SideChainFeature> for proto::types::SideChainFeature {
    fn from(value: SideChainFeature) -> Self {
        Self {
            side_chain_feature: Some(value.data.into()),
            sidechain_id: value.sidechain_id.as_ref().map(Into::into),
        }
    }
}

impl TryFrom<proto::types::SideChainFeature> for SideChainFeature {
    type Error = String;

    fn try_from(features: proto::types::SideChainFeature) -> Result<Self, Self::Error> {
        Ok(Self {
            data: features
                .side_chain_feature
                .map(TryInto::try_into)
                .ok_or("sidec_hain_feature not provided")??,
            sidechain_id: features.sidechain_id.map(TryInto::try_into).transpose()?,
        })
    }
}

impl From<SideChainFeatureData> for proto::types::side_chain_feature::SideChainFeature {
    fn from(value: SideChainFeatureData) -> Self {
        match value {
            SideChainFeatureData::ValidatorNodeRegistration(reg) => {
                proto::types::side_chain_feature::SideChainFeature::ValidatorNodeRegistration((*reg).into())
            },
            SideChainFeatureData::CodeTemplateRegistration(template_reg) => {
                proto::types::side_chain_feature::SideChainFeature::TemplateRegistration(template_reg.into())
            },
            SideChainFeatureData::ConfidentialOutput(output_data) => {
                proto::types::side_chain_feature::SideChainFeature::ConfidentialOutput(output_data.into())
            },
            SideChainFeatureData::EvictionProof(proof) => {
                proto::types::side_chain_feature::SideChainFeature::EvictionProof(proof.as_ref().into())
            },
            SideChainFeatureData::ValidatorNodeExit(ref exit) => {
                proto::types::side_chain_feature::SideChainFeature::ValidatorNodeExit(exit.into())
            },
        }
    }
}

impl TryFrom<proto::types::side_chain_feature::SideChainFeature> for SideChainFeatureData {
    type Error = String;

    fn try_from(features: proto::types::side_chain_feature::SideChainFeature) -> Result<Self, Self::Error> {
        match features {
            proto::types::side_chain_feature::SideChainFeature::ValidatorNodeRegistration(vn_reg) => Ok(
                SideChainFeatureData::ValidatorNodeRegistration(Box::new(vn_reg.try_into()?)),
            ),
            proto::types::side_chain_feature::SideChainFeature::TemplateRegistration(template_reg) => {
                Ok(SideChainFeatureData::CodeTemplateRegistration(template_reg.try_into()?))
            },
            proto::types::side_chain_feature::SideChainFeature::ConfidentialOutput(output_data) => {
                Ok(SideChainFeatureData::ConfidentialOutput(output_data.try_into()?))
            },
            proto::types::side_chain_feature::SideChainFeature::EvictionProof(proof) => {
                Ok(SideChainFeatureData::EvictionProof(Box::new(proof.try_into()?)))
            },
            proto::types::side_chain_feature::SideChainFeature::ValidatorNodeExit(exit) => {
                Ok(SideChainFeatureData::ValidatorNodeExit(exit.try_into()?))
            },
        }
    }
}

// -------------------------------- ValidatorNodeRegistration -------------------------------- //
impl TryFrom<proto::types::ValidatorNodeRegistration> for ValidatorNodeRegistration {
    type Error = String;

    fn try_from(value: proto::types::ValidatorNodeRegistration) -> Result<Self, Self::Error> {
        let public_key =
            CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| format!("public_key: {}", e))?;
        let claim_public_key = CompressedPublicKey::from_canonical_bytes(&value.claim_public_key)
            .map_err(|e| format!("claim_public_key: {}", e))?;

        Ok(Self::new(
            ValidatorNodeSignature::new(
                public_key,
                value
                    .signature
                    .map(Signature::try_from)
                    .ok_or("signature not provided")??,
            ),
            claim_public_key,
            value.max_epoch.into(),
        ))
    }
}

impl From<ValidatorNodeRegistration> for proto::types::ValidatorNodeRegistration {
    fn from(value: ValidatorNodeRegistration) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            signature: Some(value.signature().into()),
            claim_public_key: value.claim_public_key().to_vec(),
            max_epoch: value.max_epoch().as_u64(),
        }
    }
}

// -------------------------------- ValidatorNodeExit -------------------------------- //
impl TryFrom<proto::types::ValidatorNodeExit> for ValidatorNodeExit {
    type Error = String;

    fn try_from(value: proto::types::ValidatorNodeExit) -> Result<Self, Self::Error> {
        let public_key =
            CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| format!("public_key: {}", e))?;

        Ok(Self::new(
            ValidatorNodeSignature::new(
                public_key,
                value
                    .signature
                    .map(Signature::try_from)
                    .ok_or("signature not provided")??,
            ),
            value.max_epoch.into(),
        ))
    }
}

impl From<&ValidatorNodeExit> for proto::types::ValidatorNodeExit {
    fn from(value: &ValidatorNodeExit) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            signature: Some(value.signature().into()),
            max_epoch: value.max_epoch().as_u64(),
        }
    }
}

// -------------------------------- TemplateRegistration -------------------------------- //
impl TryFrom<proto::types::TemplateRegistration> for CodeTemplateRegistration {
    type Error = String;

    fn try_from(value: proto::types::TemplateRegistration) -> Result<Self, Self::Error> {
        Ok(Self {
            author_public_key: CompressedPublicKey::from_canonical_bytes(&value.author_public_key)
                .map_err(|e| e.to_string())?,
            author_signature: value
                .author_signature
                .map(Signature::try_from)
                .ok_or("author_signature not provided")??,
            template_name: MaxSizeString::try_from(value.template_name).map_err(|e| e.to_string())?,
            template_version: value
                .template_version
                .try_into()
                .map_err(|_| "Invalid template version")?,
            template_type: value
                .template_type
                .map(TryFrom::try_from)
                .ok_or("Template type not provided")??,
            build_info: value
                .build_info
                .map(TryFrom::try_from)
                .ok_or("Build info not provided")??,
            binary_sha: value.binary_sha.try_into().map_err(|_| "Invalid commit sha")?,
            binary_url: MaxSizeString::try_from(value.binary_url).map_err(|e| e.to_string())?,
        })
    }
}

impl From<CodeTemplateRegistration> for proto::types::TemplateRegistration {
    fn from(value: CodeTemplateRegistration) -> Self {
        Self {
            author_public_key: value.author_public_key.to_vec(),
            author_signature: Some(value.author_signature().into()),
            template_name: value.template_name.to_string(),
            template_version: u32::from(value.template_version),
            template_type: Some(value.template_type.into()),
            build_info: Some(value.build_info().into()),
            binary_sha: value.binary_sha.to_vec(),
            binary_url: value.binary_url.to_string(),
        }
    }
}

// -------------------------------- ConfidentialOutputData -------------------------------- //
impl TryFrom<proto::types::ConfidentialOutputData> for ConfidentialOutputData {
    type Error = String;

    fn try_from(value: proto::types::ConfidentialOutputData) -> Result<Self, Self::Error> {
        Ok(ConfidentialOutputData {
            claim_public_key: CompressedPublicKey::from_canonical_bytes(&value.claim_public_key)
                .map_err(|e| e.to_string())?,
        })
    }
}

impl From<ConfidentialOutputData> for proto::types::ConfidentialOutputData {
    fn from(value: ConfidentialOutputData) -> Self {
        Self {
            claim_public_key: value.claim_public_key.to_vec(),
        }
    }
}

// -------------------------------- TemplateType -------------------------------- //
impl TryFrom<proto::types::TemplateType> for TemplateType {
    type Error = String;

    fn try_from(value: proto::types::TemplateType) -> Result<Self, Self::Error> {
        let template_type = value.template_type.ok_or("Template type not provided")?;
        match template_type {
            proto::types::template_type::TemplateType::Wasm(wasm) => Ok(TemplateType::Wasm {
                abi_version: wasm.abi_version.try_into().map_err(|_| "abi_version overflowed")?,
            }),
            proto::types::template_type::TemplateType::Flow(_flow) => Ok(TemplateType::Flow),
            proto::types::template_type::TemplateType::Manifest(_manifest) => Ok(TemplateType::Manifest),
        }
    }
}

impl From<TemplateType> for proto::types::TemplateType {
    fn from(value: TemplateType) -> Self {
        match value {
            TemplateType::Wasm { abi_version } => Self {
                template_type: Some(proto::types::template_type::TemplateType::Wasm(
                    proto::types::WasmInfo {
                        abi_version: abi_version.into(),
                    },
                )),
            },
            TemplateType::Flow => Self {
                template_type: Some(proto::types::template_type::TemplateType::Flow(
                    proto::types::FlowInfo {},
                )),
            },
            TemplateType::Manifest => Self {
                template_type: Some(proto::types::template_type::TemplateType::Manifest(
                    proto::types::ManifestInfo {},
                )),
            },
        }
    }
}

// -------------------------------- BuildInfo -------------------------------- //

impl TryFrom<proto::types::BuildInfo> for BuildInfo {
    type Error = String;

    fn try_from(value: proto::types::BuildInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_url: value.repo_url.try_into().map_err(|_| "Invalid repo url")?,
            commit_hash: value.commit_hash.try_into().map_err(|_| "Invalid commit hash")?,
        })
    }
}

impl From<&BuildInfo> for proto::types::BuildInfo {
    fn from(value: &BuildInfo) -> Self {
        Self {
            repo_url: value.repo_url.as_str().to_string(),
            commit_hash: value.commit_hash.as_bytes().to_vec(),
        }
    }
}

// -------------------------------- SidechainId -------------------------------- //

impl TryFrom<proto::types::SidechainId> for SideChainId {
    type Error = String;

    fn try_from(value: proto::types::SidechainId) -> Result<Self, Self::Error> {
        let public_key = CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| e.to_string())?;
        let knowledge_proof = value
            .knowledge_proof
            .map(Signature::try_from)
            .ok_or("knowledge_proof not provided")??;
        Ok(Self::new(public_key, knowledge_proof))
    }
}

impl From<&SideChainId> for proto::types::SidechainId {
    fn from(value: &SideChainId) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            knowledge_proof: Some(value.knowledge_proof().into()),
        }
    }
}

// -------------------------------- EvictionProof -------------------------------- //

impl TryFrom<proto::types::EvictionProof> for EvictionProof {
    type Error = String;

    fn try_from(value: proto::types::EvictionProof) -> Result<Self, Self::Error> {
        let proof = value.proof.ok_or("proof not provided")?.try_into()?;
        Ok(EvictionProof::new(proof))
    }
}

impl From<&EvictionProof> for proto::types::EvictionProof {
    fn from(value: &EvictionProof) -> Self {
        Self {
            proof: Some(value.proof().into()),
        }
    }
}

// -------------------------------- Commit proof -------------------------------- //

impl TryFrom<proto::types::CommitProof> for CommandCommitProof<EvictNodeAtom> {
    type Error = String;

    fn try_from(value: proto::types::CommitProof) -> Result<Self, Self::Error> {
        match value.version.ok_or("version not provided")? {
            proto::types::commit_proof::Version::V1(v1) => Ok(Self::V1(v1.try_into()?)),
        }
    }
}

impl From<&CommandCommitProof<EvictNodeAtom>> for proto::types::CommitProof {
    fn from(value: &CommandCommitProof<EvictNodeAtom>) -> Self {
        match value {
            CommandCommitProof::V1(v1) => Self {
                version: Some(proto::types::commit_proof::Version::V1(v1.into())),
            },
        }
    }
}

impl TryFrom<proto::types::CommitProofV1> for CommandCommitProofV1<EvictNodeAtom> {
    type Error = String;

    fn try_from(value: proto::types::CommitProofV1) -> Result<Self, Self::Error> {
        let command = proto::types::EvictAtom::decode(value.command.as_slice()).map_err(|e| e.to_string())?;
        Ok(CommandCommitProofV1 {
            command: command.try_into()?,
            commit_proof: value.commit_proof.ok_or("commit_proof not provided")?.try_into()?,
            inclusion_proof: borsh::from_slice(&value.encoded_inclusion_proof)
                .map_err(|e| format!("Failed to decode SparseMerkleProofExt: {e}"))?,
        })
    }
}

impl From<&CommandCommitProofV1<EvictNodeAtom>> for proto::types::CommitProofV1 {
    fn from(value: &CommandCommitProofV1<EvictNodeAtom>) -> Self {
        Self {
            // Encode since command is generic
            command: proto::types::EvictAtom::from(value.command()).encode_to_vec(),
            commit_proof: Some(value.commit_proof().into()),
            // Encode since the type is complex
            // TODO: making this fallible is a pain - we may need to implement the proto for this
            encoded_inclusion_proof: borsh::to_vec(value.inclusion_proof())
                .expect("Failed to encode SparseMerkleProofExt"),
        }
    }
}

// -------------------------------- SidechainBlockCommitProof -------------------------------- //

impl TryFrom<proto::types::SidechainBlockCommitProof> for SidechainBlockCommitProof {
    type Error = String;

    fn try_from(value: proto::types::SidechainBlockCommitProof) -> Result<Self, Self::Error> {
        Ok(Self {
            header: value.header.ok_or("header not provided")?.try_into()?,
            proof_elements: value
                .proof_elements
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl From<&SidechainBlockCommitProof> for proto::types::SidechainBlockCommitProof {
    fn from(value: &SidechainBlockCommitProof) -> Self {
        Self {
            header: Some(value.header().into()),
            proof_elements: value.proof_elements().iter().map(Into::into).collect(),
        }
    }
}

// -------------------------------- SidechainBlockHeader -------------------------------- //

impl TryFrom<proto::types::SidechainBlockHeader> for SidechainBlockHeader {
    type Error = String;

    fn try_from(value: proto::types::SidechainBlockHeader) -> Result<Self, Self::Error> {
        let network_byte = u8::try_from(value.network).map_err(|_| "Invalid network byte: overflows u8".to_string())?;
        Network::try_from(network_byte).map_err(|err| format!("Invalid network byte: {}", err))?;
        Ok(Self {
            network: network_byte,
            parent_id: value.parent_id.try_into().map_err(|_| "Invalid parent id")?,
            justify_id: value.justify_id.try_into().map_err(|_| "Invalid justify id")?,
            height: value.height,
            epoch: value.epoch,
            shard_group: value.shard_group.ok_or("missing shard_group")?.try_into()?,
            proposed_by: CompressedPublicKey::from_canonical_bytes(&value.proposed_by)
                .map_err(|_| "Invalid proposed_by public key")?,
            state_merkle_root: value
                .state_merkle_root
                .try_into()
                .map_err(|_| "Invalid state merkle root")?,
            command_merkle_root: value
                .command_merkle_root
                .try_into()
                .map_err(|_| "Invalid command merkle root")?,
            signature: value
                .signature
                .ok_or("SidechainBlockHeader signature not provided")?
                .try_into()?,
            metadata_hash: value.metadata_hash.try_into().map_err(|_| "Invalid metadata hash")?,
        })
    }
}

impl From<&SidechainBlockHeader> for proto::types::SidechainBlockHeader {
    fn from(value: &SidechainBlockHeader) -> Self {
        Self {
            network: u32::from(value.network),
            parent_id: value.parent_id.to_vec(),
            justify_id: value.justify_id.to_vec(),
            height: value.height,
            epoch: value.epoch,
            shard_group: Some(value.shard_group.into()),
            proposed_by: value.proposed_by.to_vec(),
            state_merkle_root: value.state_merkle_root.to_vec(),
            command_merkle_root: value.command_merkle_root.to_vec(),
            signature: Some(value.signature().into()),
            metadata_hash: value.metadata_hash.to_vec(),
        }
    }
}

// -------------------------------- CommitProofElement -------------------------------- //

impl TryFrom<proto::types::CommitProofElement> for CommitProofElement {
    type Error = String;

    fn try_from(value: proto::types::CommitProofElement) -> Result<Self, Self::Error> {
        match value.proof_element.ok_or("proof element not provided")? {
            proto::types::commit_proof_element::ProofElement::QuorumCertificate(qc) => {
                Ok(CommitProofElement::QuorumCertificate(qc.try_into()?))
            },
            proto::types::commit_proof_element::ProofElement::DummyChain(chain) => Ok(CommitProofElement::ChainLinks(
                chain
                    .chain_links
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }
}

impl From<&CommitProofElement> for proto::types::CommitProofElement {
    fn from(value: &CommitProofElement) -> Self {
        match value {
            CommitProofElement::QuorumCertificate(qc) => Self {
                proof_element: Some(proto::types::commit_proof_element::ProofElement::QuorumCertificate(
                    qc.into(),
                )),
            },
            CommitProofElement::ChainLinks(chain) => Self {
                proof_element: Some(proto::types::commit_proof_element::ProofElement::DummyChain(
                    proto::types::DummyChain {
                        chain_links: chain.iter().map(Into::into).collect(),
                    },
                )),
            },
        }
    }
}

// -------------------------------- ChainLink -------------------------------- //

impl TryFrom<proto::types::ChainLink> for ChainLink {
    type Error = String;

    fn try_from(value: proto::types::ChainLink) -> Result<Self, Self::Error> {
        Ok(Self {
            header_hash: value.header_hash.try_into().map_err(|_| "Invalid block id")?,
            parent_id: value.parent_id.try_into().map_err(|_| "Invalid parent id")?,
        })
    }
}

impl From<&ChainLink> for proto::types::ChainLink {
    fn from(value: &ChainLink) -> Self {
        Self {
            header_hash: value.header_hash.to_vec(),
            parent_id: value.parent_id.to_vec(),
        }
    }
}

// -------------------------------- QuorumCertificate -------------------------------- //

impl TryFrom<proto::types::QuorumCertificate> for QuorumCertificate {
    type Error = String;

    fn try_from(value: proto::types::QuorumCertificate) -> Result<Self, Self::Error> {
        Ok(Self {
            header_hash: value.header_hash.try_into().map_err(|_| "Invalid block body hash")?,
            parent_id: value.parent_id.try_into().map_err(|_| "Invalid parent id")?,
            signatures: value
                .signatures
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            decision: proto::types::QuorumDecision::try_from(value.decision)
                .map_err(|e| format!("Invalid QuorumDecision: {e}"))?
                .into(),
        })
    }
}

impl From<&QuorumCertificate> for proto::types::QuorumCertificate {
    fn from(value: &QuorumCertificate) -> Self {
        Self {
            parent_id: value.parent_id.to_vec(),
            header_hash: value.header_hash.to_vec(),
            signatures: value.signatures.iter().map(Into::into).collect(),
            decision: proto::types::QuorumDecision::from(value.decision).into(),
        }
    }
}

// -------------------------------- QuorumDecision -------------------------------- //

impl From<proto::types::QuorumDecision> for QuorumDecision {
    fn from(value: proto::types::QuorumDecision) -> Self {
        match value {
            proto::types::QuorumDecision::Accept => QuorumDecision::Accept,
            proto::types::QuorumDecision::Reject => QuorumDecision::Reject,
        }
    }
}

impl From<QuorumDecision> for proto::types::QuorumDecision {
    fn from(value: QuorumDecision) -> Self {
        match value {
            QuorumDecision::Accept => proto::types::QuorumDecision::Accept,
            QuorumDecision::Reject => proto::types::QuorumDecision::Reject,
        }
    }
}

// -------------------------------- ValidatorSignature -------------------------------- //

impl TryFrom<proto::types::ValidatorSignature> for ValidatorQcSignature {
    type Error = String;

    fn try_from(value: proto::types::ValidatorSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| e.to_string())?,
            signature: value.signature.ok_or("signature not provided")?.try_into()?,
        })
    }
}

impl From<&ValidatorQcSignature> for proto::types::ValidatorSignature {
    fn from(value: &ValidatorQcSignature) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            signature: Some(value.signature().into()),
        }
    }
}

// -------------------------------- EvictNodeAtom -------------------------------- //

impl TryFrom<proto::types::EvictAtom> for EvictNodeAtom {
    type Error = String;

    fn try_from(value: proto::types::EvictAtom) -> Result<Self, Self::Error> {
        Ok(Self::new(
            CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| e.to_string())?,
        ))
    }
}

impl From<&EvictNodeAtom> for proto::types::EvictAtom {
    fn from(value: &EvictNodeAtom) -> Self {
        Self {
            public_key: value.node_to_evict().to_vec(),
        }
    }
}
// -------------------------------- ShardGroup -------------------------------- //

impl TryFrom<proto::types::ShardGroup> for ShardGroup {
    type Error = String;

    fn try_from(value: proto::types::ShardGroup) -> Result<Self, Self::Error> {
        Ok(Self {
            start: value.start,
            end_inclusive: value.end_inclusive,
        })
    }
}

impl From<ShardGroup> for proto::types::ShardGroup {
    fn from(value: ShardGroup) -> Self {
        Self {
            start: value.start,
            end_inclusive: value.end_inclusive,
        }
    }
}
