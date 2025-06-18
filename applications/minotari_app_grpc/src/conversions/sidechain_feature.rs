//  Copyright 2022. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::convert::{TryFrom, TryInto};

use prost::Message;
use tari_common_types::{
    epoch::VnEpoch,
    types::{CompressedPublicKey, Signature},
};
use tari_core::{
    base_node::comms_interface::ValidatorNodeChange,
    transactions::{
        tari_amount::MicroMinotari,
        transaction_components::{
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
    },
};
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

use crate::tari_rpc as grpc;

//---------------------------------- SideChainFeature --------------------------------------------//
impl From<&SideChainFeature> for grpc::SideChainFeature {
    fn from(value: &SideChainFeature) -> Self {
        Self {
            feature: Some((&value.data).into()),
            sidechain_id: value.sidechain_id.as_ref().map(Into::into),
        }
    }
}

impl TryFrom<grpc::SideChainFeature> for SideChainFeature {
    type Error = String;

    fn try_from(value: grpc::SideChainFeature) -> Result<Self, Self::Error> {
        Ok(Self {
            data: value.feature.ok_or("Feature not provided")?.try_into()?,
            sidechain_id: value.sidechain_id.map(TryInto::try_into).transpose()?,
        })
    }
}

impl From<&SideChainFeatureData> for grpc::side_chain_feature::Feature {
    fn from(value: &SideChainFeatureData) -> Self {
        match value {
            SideChainFeatureData::ValidatorNodeRegistration(reg) => {
                grpc::side_chain_feature::Feature::ValidatorNodeRegistration(reg.as_ref().into())
            },
            SideChainFeatureData::CodeTemplateRegistration(template_reg) => {
                grpc::side_chain_feature::Feature::TemplateRegistration(template_reg.into())
            },
            SideChainFeatureData::ConfidentialOutput(output_data) => {
                grpc::side_chain_feature::Feature::ConfidentialOutput(output_data.into())
            },
            SideChainFeatureData::EvictionProof(proof) => {
                grpc::side_chain_feature::Feature::EvictionProof(grpc::EvictionProof::from(&**proof))
            },
            SideChainFeatureData::ValidatorNodeExit(exit) => {
                grpc::side_chain_feature::Feature::ValidatorNodeExit(exit.into())
            },
        }
    }
}

impl TryFrom<grpc::side_chain_feature::Feature> for SideChainFeatureData {
    type Error = String;

    fn try_from(features: grpc::side_chain_feature::Feature) -> Result<Self, Self::Error> {
        match features {
            grpc::side_chain_feature::Feature::ValidatorNodeRegistration(vn_reg) => Ok(
                SideChainFeatureData::ValidatorNodeRegistration(Box::new(vn_reg.try_into()?)),
            ),
            grpc::side_chain_feature::Feature::TemplateRegistration(template_reg) => {
                Ok(SideChainFeatureData::CodeTemplateRegistration(template_reg.try_into()?))
            },
            grpc::side_chain_feature::Feature::ConfidentialOutput(output_data) => {
                Ok(SideChainFeatureData::ConfidentialOutput(output_data.try_into()?))
            },
            grpc::side_chain_feature::Feature::EvictionProof(proof) => {
                Ok(SideChainFeatureData::EvictionProof(Box::new(proof.try_into()?)))
            },
            grpc::side_chain_feature::Feature::ValidatorNodeExit(exit) => {
                Ok(SideChainFeatureData::ValidatorNodeExit(exit.try_into()?))
            },
        }
    }
}

// -------------------------------- SideChainId -------------------------------- //

impl TryFrom<grpc::SideChainId> for SideChainId {
    type Error = String;

    fn try_from(value: grpc::SideChainId) -> Result<Self, Self::Error> {
        let public_key =
            CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| format!("sidechain_id: {}", e))?;
        let knowledge_proof = value
            .knowledge_proof
            .ok_or("sidechain_id knowledge_proof not provided")?;
        let knowledge_proof =
            Signature::try_from(knowledge_proof).map_err(|e| format!("sidechain_id_knowledge_proof: {}", e))?;

        Ok(Self::new(public_key, knowledge_proof))
    }
}

impl From<&SideChainId> for grpc::SideChainId {
    fn from(value: &SideChainId) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            knowledge_proof: Some(value.knowledge_proof().into()),
        }
    }
}

// -------------------------------- ValidatorNodeRegistration -------------------------------- //
impl TryFrom<grpc::ValidatorNodeRegistration> for ValidatorNodeRegistration {
    type Error = String;

    fn try_from(value: grpc::ValidatorNodeRegistration) -> Result<Self, Self::Error> {
        let public_key = CompressedPublicKey::from_canonical_bytes(&value.public_key)
            .map_err(|e| format!("Invalid public key: {}", e))?;
        let claim_public_key = CompressedPublicKey::from_canonical_bytes(&value.claim_public_key)
            .map_err(|e| format!("Invalid claim public key: {}", e))?;

        Ok(ValidatorNodeRegistration::new(
            ValidatorNodeSignature::new(
                public_key,
                value
                    .signature
                    .map(TryInto::try_into)
                    .ok_or("signature not provided")??,
            ),
            claim_public_key,
            value.max_epoch.into(),
        ))
    }
}
impl From<&ValidatorNodeRegistration> for crate::tari_rpc::ValidatorNodeRegistration {
    fn from(registration: &ValidatorNodeRegistration) -> Self {
        Self {
            public_key: registration.public_key().to_vec(),
            signature: Some(crate::tari_rpc::Signature {
                public_nonce: registration.signature().get_compressed_public_nonce().to_vec(),
                signature: registration.signature().get_signature().to_vec(),
            }),
            claim_public_key: registration.claim_public_key().to_vec(),
            max_epoch: registration.max_epoch().as_u64(),
        }
    }
}

impl From<ValidatorNodeRegistration> for grpc::ValidatorNodeRegistration {
    fn from(value: ValidatorNodeRegistration) -> Self {
        Self::from(&value)
    }
}
// -------------------------------- ValidatorNodeExit -------------------------------- //
impl TryFrom<grpc::ValidatorNodeExit> for ValidatorNodeExit {
    type Error = String;

    fn try_from(value: grpc::ValidatorNodeExit) -> Result<Self, Self::Error> {
        let public_key = CompressedPublicKey::from_canonical_bytes(&value.public_key)
            .map_err(|e| format!("Invalid public key: {}", e))?;

        Ok(ValidatorNodeExit::new(
            ValidatorNodeSignature::new(
                public_key,
                value
                    .signature
                    .map(TryInto::try_into)
                    .ok_or("signature not provided")??,
            ),
            value.max_epoch.into(),
        ))
    }
}
impl From<&ValidatorNodeExit> for crate::tari_rpc::ValidatorNodeExit {
    fn from(exit: &ValidatorNodeExit) -> Self {
        Self {
            public_key: exit.public_key().to_vec(),
            signature: Some(crate::tari_rpc::Signature {
                public_nonce: exit.signature().get_compressed_public_nonce().to_vec(),
                signature: exit.signature().get_signature().to_vec(),
            }),
            max_epoch: exit.max_epoch().as_u64(),
        }
    }
}

impl From<ValidatorNodeExit> for grpc::ValidatorNodeExit {
    fn from(value: ValidatorNodeExit) -> Self {
        Self::from(&value)
    }
}

// -------------------------------- TemplateRegistration -------------------------------- //
impl TryFrom<grpc::TemplateRegistration> for CodeTemplateRegistration {
    type Error = String;

    fn try_from(value: grpc::TemplateRegistration) -> Result<Self, Self::Error> {
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

impl From<&CodeTemplateRegistration> for grpc::TemplateRegistration {
    fn from(value: &CodeTemplateRegistration) -> Self {
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
impl TryFrom<grpc::ConfidentialOutputData> for ConfidentialOutputData {
    type Error = String;

    fn try_from(value: grpc::ConfidentialOutputData) -> Result<Self, Self::Error> {
        Ok(ConfidentialOutputData {
            claim_public_key: CompressedPublicKey::from_canonical_bytes(&value.claim_public_key)
                .map_err(|e| e.to_string())?,
        })
    }
}

impl From<&ConfidentialOutputData> for grpc::ConfidentialOutputData {
    fn from(value: &ConfidentialOutputData) -> Self {
        Self {
            claim_public_key: value.claim_public_key.to_vec(),
        }
    }
}

// -------------------------------- TemplateType -------------------------------- //
impl TryFrom<grpc::TemplateType> for TemplateType {
    type Error = String;

    fn try_from(value: grpc::TemplateType) -> Result<Self, Self::Error> {
        let template_type = value.template_type.ok_or("Template type not provided")?;
        match template_type {
            grpc::template_type::TemplateType::Wasm(wasm) => Ok(TemplateType::Wasm {
                abi_version: wasm.abi_version.try_into().map_err(|_| "abi_version overflowed")?,
            }),
            grpc::template_type::TemplateType::Flow(_flow) => Ok(TemplateType::Flow {}),
            grpc::template_type::TemplateType::Manifest(_manifest) => Ok(TemplateType::Manifest {}),
        }
    }
}

impl From<TemplateType> for grpc::TemplateType {
    fn from(value: TemplateType) -> Self {
        match value {
            TemplateType::Wasm { abi_version } => Self {
                template_type: Some(grpc::template_type::TemplateType::Wasm(grpc::WasmInfo {
                    abi_version: abi_version.into(),
                })),
            },
            TemplateType::Flow => Self {
                template_type: Some(grpc::template_type::TemplateType::Flow(grpc::FlowInfo {})),
            },
            TemplateType::Manifest => Self {
                template_type: Some(grpc::template_type::TemplateType::Manifest(grpc::ManifestInfo {})),
            },
        }
    }
}

// -------------------------------- BuildInfo -------------------------------- //

impl TryFrom<grpc::BuildInfo> for BuildInfo {
    type Error = String;

    fn try_from(value: grpc::BuildInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            repo_url: value.repo_url.try_into().map_err(|_| "Invalid repo url")?,
            commit_hash: value.commit_hash.try_into().map_err(|_| "Invalid commit hash")?,
        })
    }
}

impl From<&BuildInfo> for grpc::BuildInfo {
    fn from(value: &BuildInfo) -> Self {
        Self {
            repo_url: value.repo_url.as_str().to_string(),
            commit_hash: value.commit_hash.as_bytes().to_vec(),
        }
    }
}

// -------------------------------- EvictionProof -------------------------------- //

impl TryFrom<grpc::EvictionProof> for EvictionProof {
    type Error = String;

    fn try_from(value: grpc::EvictionProof) -> Result<Self, Self::Error> {
        let proof = value.proof.ok_or("proof not provided")?.try_into()?;
        Ok(EvictionProof::new(proof))
    }
}

impl From<&EvictionProof> for grpc::EvictionProof {
    fn from(value: &EvictionProof) -> Self {
        Self {
            proof: Some(value.proof().into()),
        }
    }
}

// -------------------------------- Commit proof -------------------------------- //

impl TryFrom<grpc::CommitProof> for CommandCommitProof<EvictNodeAtom> {
    type Error = String;

    fn try_from(value: grpc::CommitProof) -> Result<Self, Self::Error> {
        match value.version.ok_or("version not provided")? {
            grpc::commit_proof::Version::V1(v1) => Ok(Self::V1(v1.try_into()?)),
        }
    }
}

impl From<&CommandCommitProof<EvictNodeAtom>> for grpc::CommitProof {
    fn from(value: &CommandCommitProof<EvictNodeAtom>) -> Self {
        match value {
            CommandCommitProof::V1(v1) => Self {
                version: Some(grpc::commit_proof::Version::V1(v1.into())),
            },
        }
    }
}

impl TryFrom<grpc::CommitProofV1> for CommandCommitProofV1<EvictNodeAtom> {
    type Error = String;

    fn try_from(value: grpc::CommitProofV1) -> Result<Self, Self::Error> {
        let command = grpc::EvictAtom::decode(value.command.as_slice()).map_err(|e| e.to_string())?;
        Ok(CommandCommitProofV1 {
            command: command.try_into()?,
            commit_proof: value.commit_proof.ok_or("commit_proof not provided")?.try_into()?,

            inclusion_proof: borsh::from_slice(&value.encoded_inclusion_proof)
                .map_err(|e| format!("Failed to decode SparseMerkleProofExt: {e}"))?,
        })
    }
}

impl From<&CommandCommitProofV1<EvictNodeAtom>> for grpc::CommitProofV1 {
    fn from(value: &CommandCommitProofV1<EvictNodeAtom>) -> Self {
        Self {
            command: grpc::EvictAtom::from(value.command()).encode_to_vec(),
            commit_proof: Some(value.commit_proof().into()),
            // Encode since the type is complex
            // TODO: making this fallible is a pain - we may need to implement the proto for this
            encoded_inclusion_proof: borsh::to_vec(value.inclusion_proof())
                .expect("Failed to encode SparseMerkleProofExt"),
        }
    }
}

// -------------------------------- SidechainBlockCommitProof -------------------------------- //

impl TryFrom<grpc::SidechainBlockCommitProof> for SidechainBlockCommitProof {
    type Error = String;

    fn try_from(value: grpc::SidechainBlockCommitProof) -> Result<Self, Self::Error> {
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

impl From<&SidechainBlockCommitProof> for grpc::SidechainBlockCommitProof {
    fn from(value: &SidechainBlockCommitProof) -> Self {
        Self {
            header: Some(value.header().into()),
            proof_elements: value.proof_elements().iter().map(Into::into).collect(),
        }
    }
}

// -------------------------------- SidechainBlockHeader -------------------------------- //

impl TryFrom<grpc::SidechainBlockHeader> for SidechainBlockHeader {
    type Error = String;

    fn try_from(value: grpc::SidechainBlockHeader) -> Result<Self, Self::Error> {
        let network_byte = u8::try_from(value.network).map_err(|_| "Invalid network byte: overflows u8".to_string())?;
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

impl From<&SidechainBlockHeader> for grpc::SidechainBlockHeader {
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

impl TryFrom<grpc::CommitProofElement> for CommitProofElement {
    type Error = String;

    fn try_from(value: grpc::CommitProofElement) -> Result<Self, Self::Error> {
        match value.proof_element.ok_or("proof element not provided")? {
            grpc::commit_proof_element::ProofElement::QuorumCertificate(qc) => {
                Ok(CommitProofElement::QuorumCertificate(qc.try_into()?))
            },
            grpc::commit_proof_element::ProofElement::DummyChain(chain) => Ok(CommitProofElement::ChainLinks(
                chain
                    .chain_links
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }
}

impl From<&CommitProofElement> for grpc::CommitProofElement {
    fn from(value: &CommitProofElement) -> Self {
        match value {
            CommitProofElement::QuorumCertificate(qc) => Self {
                proof_element: Some(grpc::commit_proof_element::ProofElement::QuorumCertificate(qc.into())),
            },
            CommitProofElement::ChainLinks(chain) => Self {
                proof_element: Some(grpc::commit_proof_element::ProofElement::DummyChain(grpc::DummyChain {
                    chain_links: chain.iter().map(Into::into).collect(),
                })),
            },
        }
    }
}

// -------------------------------- ChainLink -------------------------------- //

impl TryFrom<grpc::ChainLink> for ChainLink {
    type Error = String;

    fn try_from(value: grpc::ChainLink) -> Result<Self, Self::Error> {
        Ok(Self {
            header_hash: value.header_hash.try_into().map_err(|_| "Invalid block id")?,
            parent_id: value.parent_id.try_into().map_err(|_| "Invalid parent id")?,
        })
    }
}

impl From<&ChainLink> for grpc::ChainLink {
    fn from(value: &ChainLink) -> Self {
        Self {
            header_hash: value.header_hash.to_vec(),
            parent_id: value.parent_id.to_vec(),
        }
    }
}

// -------------------------------- QuorumCertificate -------------------------------- //

impl TryFrom<grpc::QuorumCertificate> for QuorumCertificate {
    type Error = String;

    fn try_from(value: grpc::QuorumCertificate) -> Result<Self, Self::Error> {
        Ok(Self {
            header_hash: value.header_hash.try_into().map_err(|_| "Invalid block body hash")?,
            parent_id: value.parent_id.try_into().map_err(|_| "Invalid parent id")?,
            signatures: value
                .signatures
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            decision: grpc::QuorumDecision::try_from(value.decision)
                .map_err(|e| format!("Invalid QuorumDecision: {e}"))?
                .into(),
        })
    }
}

impl From<&QuorumCertificate> for grpc::QuorumCertificate {
    fn from(value: &QuorumCertificate) -> Self {
        Self {
            parent_id: value.parent_id.to_vec(),
            header_hash: value.header_hash.to_vec(),
            signatures: value.signatures.iter().map(Into::into).collect(),
            decision: grpc::QuorumDecision::from(value.decision).into(),
        }
    }
}

// -------------------------------- QuorumDecision -------------------------------- //

impl From<grpc::QuorumDecision> for QuorumDecision {
    fn from(value: grpc::QuorumDecision) -> Self {
        match value {
            grpc::QuorumDecision::Accept => QuorumDecision::Accept,
            grpc::QuorumDecision::Reject => QuorumDecision::Reject,
        }
    }
}

impl From<QuorumDecision> for grpc::QuorumDecision {
    fn from(value: QuorumDecision) -> Self {
        match value {
            QuorumDecision::Accept => grpc::QuorumDecision::Accept,
            QuorumDecision::Reject => grpc::QuorumDecision::Reject,
        }
    }
}

// -------------------------------- ValidatorSignature -------------------------------- //

impl TryFrom<grpc::ValidatorSignature> for ValidatorQcSignature {
    type Error = String;

    fn try_from(value: grpc::ValidatorSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| e.to_string())?,
            signature: value.signature.ok_or("signature not provided")?.try_into()?,
        })
    }
}

impl From<&ValidatorQcSignature> for grpc::ValidatorSignature {
    fn from(value: &ValidatorQcSignature) -> Self {
        Self {
            public_key: value.public_key().to_vec(),
            signature: Some(value.signature().into()),
        }
    }
}

// -------------------------------- EvictNodeAtom -------------------------------- //

impl TryFrom<grpc::EvictAtom> for EvictNodeAtom {
    type Error = String;

    fn try_from(value: grpc::EvictAtom) -> Result<Self, Self::Error> {
        Ok(Self::new(
            CompressedPublicKey::from_canonical_bytes(&value.public_key).map_err(|e| e.to_string())?,
        ))
    }
}

impl From<&EvictNodeAtom> for grpc::EvictAtom {
    fn from(value: &EvictNodeAtom) -> Self {
        Self {
            public_key: value.node_to_evict().to_vec(),
        }
    }
}

// -------------------------------- ValidatorNodeChange -------------------------------- //

impl TryFrom<grpc::ValidatorNodeChange> for ValidatorNodeChange {
    type Error = String;

    fn try_from(value: grpc::ValidatorNodeChange) -> Result<Self, Self::Error> {
        let change = value.change.ok_or("change not provided")?;
        match change {
            grpc::validator_node_change::Change::Add(add) => {
                let activation_epoch = VnEpoch(add.activation_epoch);
                let registration = add.registration.ok_or("registration not provided")?.try_into()?;
                let minimum_value_promise = MicroMinotari(add.minimum_value_promise);
                if add.shard_key.len() != 32 {
                    return Err(format!("shard_key length is not 32 (len:{})", add.shard_key.len()));
                }
                let mut shard_key = [0u8; 32];
                shard_key.copy_from_slice(&add.shard_key);

                Ok(ValidatorNodeChange::Add {
                    registration: Box::new(registration),
                    activation_epoch,
                    minimum_value_promise,
                    shard_key,
                })
            },
            grpc::validator_node_change::Change::Remove(remove) => {
                let public_key =
                    CompressedPublicKey::from_canonical_bytes(&remove.public_key).map_err(|e| e.to_string())?;
                Ok(ValidatorNodeChange::Remove { public_key })
            },
        }
    }
}

impl From<&ValidatorNodeChange> for grpc::ValidatorNodeChange {
    fn from(node_change: &ValidatorNodeChange) -> Self {
        match node_change {
            ValidatorNodeChange::Add {
                registration,
                activation_epoch,
                minimum_value_promise,
                shard_key,
            } => Self {
                change: Some(grpc::validator_node_change::Change::Add(grpc::ValidatorNodeChangeAdd {
                    activation_epoch: activation_epoch.as_u64(),
                    registration: Some((&**registration).into()),
                    minimum_value_promise: (*minimum_value_promise).into(),
                    shard_key: shard_key.to_vec(),
                })),
            },
            ValidatorNodeChange::Remove { public_key } => Self {
                change: Some(grpc::validator_node_change::Change::Remove(
                    grpc::ValidatorNodeChangeRemove {
                        public_key: public_key.to_vec(),
                    },
                )),
            },
        }
    }
}

// -------------------------------- ShardGroup -------------------------------- //

impl TryFrom<grpc::ShardGroup> for ShardGroup {
    type Error = String;

    fn try_from(value: grpc::ShardGroup) -> Result<Self, Self::Error> {
        Ok(Self {
            start: value.start,
            end_inclusive: value.end_inclusive,
        })
    }
}

impl From<ShardGroup> for grpc::ShardGroup {
    fn from(value: ShardGroup) -> Self {
        Self {
            start: value.start,
            end_inclusive: value.end_inclusive,
        }
    }
}
