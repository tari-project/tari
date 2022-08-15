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

//! Impls for sidechain_features proto

use std::convert::{TryFrom, TryInto};

use tari_common_types::types::{Commitment, FixedHash, PublicKey, Signature};
use tari_crypto::tari_utilities::ByteArray;

use crate::{
    consensus::MaxSizeString,
    proto,
    transactions::transaction_components::{
        bytes_into_fixed_string,
        AssetOutputFeatures,
        BuildInfo,
        CheckpointParameters,
        CodeTemplateRegistration,
        CommitteeDefinitionFeatures,
        CommitteeMembers,
        CommitteeSignatures,
        ConstitutionChangeFlags,
        ConstitutionChangeRules,
        ContractAcceptance,
        ContractAcceptanceRequirements,
        ContractAmendment,
        ContractCheckpoint,
        ContractConstitution,
        ContractDefinition,
        ContractSpecification,
        ContractUpdateProposal,
        ContractUpdateProposalAcceptance,
        FunctionRef,
        MintNonFungibleFeatures,
        PublicFunction,
        RequirementsForConstitutionChange,
        SideChainCheckpointFeatures,
        SideChainConsensus,
        SideChainFeatures,
        SignerSignature,
        TemplateParameter,
        TemplateType,
    },
};

//---------------------------------- SideChainFeatures --------------------------------------------//
impl From<SideChainFeatures> for proto::types::SideChainFeatures {
    fn from(value: SideChainFeatures) -> Self {
        Self {
            contract_id: value.contract_id.to_vec(),
            definition: value.definition.map(Into::into),
            template_registration: value.template_registration.map(Into::into),
            constitution: value.constitution.map(Into::into),
            acceptance: value.acceptance.map(Into::into),
            update_proposal: value.update_proposal.map(Into::into),
            update_proposal_acceptance: value.update_proposal_acceptance.map(Into::into),
            amendment: value.amendment.map(Into::into),
            checkpoint: value.checkpoint.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::SideChainFeatures> for SideChainFeatures {
    type Error = String;

    fn try_from(features: proto::types::SideChainFeatures) -> Result<Self, Self::Error> {
        let contract_id = features.contract_id.try_into().map_err(|_| "Invalid contract_id")?;
        let definition = features.definition.map(ContractDefinition::try_from).transpose()?;
        let constitution = features.constitution.map(ContractConstitution::try_from).transpose()?;
        let acceptance = features.acceptance.map(ContractAcceptance::try_from).transpose()?;
        let template_registration = features
            .template_registration
            .map(CodeTemplateRegistration::try_from)
            .transpose()?;
        let update_proposal = features
            .update_proposal
            .map(ContractUpdateProposal::try_from)
            .transpose()?;
        let update_proposal_acceptance = features
            .update_proposal_acceptance
            .map(ContractUpdateProposalAcceptance::try_from)
            .transpose()?;
        let amendment = features.amendment.map(ContractAmendment::try_from).transpose()?;
        let checkpoint = features.checkpoint.map(ContractCheckpoint::try_from).transpose()?;

        Ok(Self {
            contract_id,
            definition,
            template_registration,
            constitution,
            acceptance,
            update_proposal,
            update_proposal_acceptance,
            amendment,
            checkpoint,
        })
    }
}

// -------------------------------- TemplateRegistration -------------------------------- //
impl TryFrom<proto::types::TemplateRegistration> for CodeTemplateRegistration {
    type Error = String;

    fn try_from(value: proto::types::TemplateRegistration) -> Result<Self, Self::Error> {
        Ok(Self {
            author_public_key: PublicKey::from_bytes(&value.author_public_key).map_err(|e| e.to_string())?,
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
            author_signature: Some(value.author_signature.into()),
            template_name: value.template_name.to_string(),
            template_version: u32::from(value.template_version),
            template_type: Some(value.template_type.into()),
            build_info: Some(value.build_info.into()),
            binary_sha: value.binary_sha.to_vec(),
            binary_url: value.binary_url.to_string(),
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

impl From<BuildInfo> for proto::types::BuildInfo {
    fn from(value: BuildInfo) -> Self {
        Self {
            repo_url: value.repo_url.into_string(),
            commit_hash: value.commit_hash.into_vec(),
        }
    }
}

//---------------------------------- ContractConstitution --------------------------------------------//
impl From<ContractConstitution> for proto::types::ContractConstitution {
    fn from(value: ContractConstitution) -> Self {
        Self {
            validator_committee: Some(value.validator_committee.into()),
            acceptance_requirements: Some(value.acceptance_requirements.into()),
            consensus: value.consensus.into(),
            checkpoint_params: Some(value.checkpoint_params.into()),
            constitution_change_rules: Some(value.constitution_change_rules.into()),
        }
    }
}

impl TryFrom<proto::types::ContractConstitution> for ContractConstitution {
    type Error = String;

    fn try_from(value: proto::types::ContractConstitution) -> Result<Self, Self::Error> {
        use num_traits::FromPrimitive;
        let validator_committee = value
            .validator_committee
            .map(TryInto::try_into)
            .ok_or("validator_committee not provided")??;
        let acceptance_requirements = value
            .acceptance_requirements
            .map(TryInto::try_into)
            .ok_or("acceptance_requirements not provided")??;
        let consensus = SideChainConsensus::from_i32(value.consensus).ok_or("Invalid SideChainConsensus")?;
        let checkpoint_params = value
            .checkpoint_params
            .map(TryInto::try_into)
            .ok_or("checkpoint_params not provided")??;
        let constitution_change_rules = value
            .constitution_change_rules
            .map(TryInto::try_into)
            .ok_or("constitution_change_rules not provided")??;

        Ok(Self {
            validator_committee,
            acceptance_requirements,
            consensus,
            checkpoint_params,
            constitution_change_rules,
        })
    }
}

//---------------------------------- ContractCheckpoint --------------------------------------------//
impl From<ContractCheckpoint> for proto::types::ContractCheckpoint {
    fn from(value: ContractCheckpoint) -> Self {
        Self {
            checkpoint_number: value.checkpoint_number,
            merkle_root: value.merkle_root.to_vec(),
            signatures: Some(value.signatures.into()),
        }
    }
}

impl TryFrom<proto::types::ContractCheckpoint> for ContractCheckpoint {
    type Error = String;

    fn try_from(value: proto::types::ContractCheckpoint) -> Result<Self, Self::Error> {
        let merkle_root = value.merkle_root.try_into().map_err(|_| "Invalid merkle root")?;
        let signatures = value.signatures.map(TryInto::try_into).transpose()?.unwrap_or_default();
        Ok(Self {
            checkpoint_number: value.checkpoint_number,
            merkle_root,
            signatures,
        })
    }
}

//---------------------------------- ContractAcceptanceRequirements --------------------------------------------//
impl From<ContractAcceptanceRequirements> for proto::types::ContractAcceptanceRequirements {
    fn from(value: ContractAcceptanceRequirements) -> Self {
        Self {
            acceptance_period_expiry: value.acceptance_period_expiry,
            minimum_quorum_required: value.minimum_quorum_required,
        }
    }
}

impl TryFrom<proto::types::ContractAcceptanceRequirements> for ContractAcceptanceRequirements {
    type Error = String;

    fn try_from(value: proto::types::ContractAcceptanceRequirements) -> Result<Self, Self::Error> {
        Ok(Self {
            acceptance_period_expiry: value.acceptance_period_expiry,
            minimum_quorum_required: value.minimum_quorum_required,
        })
    }
}

//---------------------------------- ContractAcceptance --------------------------------------------//

impl From<ContractAcceptance> for proto::types::ContractAcceptance {
    fn from(value: ContractAcceptance) -> Self {
        Self {
            validator_node_public_key: value.validator_node_public_key.as_bytes().to_vec(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<proto::types::ContractAcceptance> for ContractAcceptance {
    type Error = String;

    fn try_from(value: proto::types::ContractAcceptance) -> Result<Self, Self::Error> {
        let validator_node_public_key =
            PublicKey::from_bytes(value.validator_node_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()?;

        Ok(Self {
            validator_node_public_key,
            signature,
        })
    }
}

//---------------------------------- ContractUpdateProposal --------------------------------------------//

impl From<ContractUpdateProposal> for proto::types::ContractUpdateProposal {
    fn from(value: ContractUpdateProposal) -> Self {
        Self {
            proposal_id: value.proposal_id,
            signature: Some(value.signature.into()),
            updated_constitution: Some(value.updated_constitution.into()),
        }
    }
}

impl TryFrom<proto::types::ContractUpdateProposal> for ContractUpdateProposal {
    type Error = String;

    fn try_from(value: proto::types::ContractUpdateProposal) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()?;

        let updated_constitution = value
            .updated_constitution
            .ok_or_else(|| "updated_constiution not provided".to_string())?
            .try_into()?;

        Ok(Self {
            proposal_id: value.proposal_id,
            signature,
            updated_constitution,
        })
    }
}

//---------------------------------- ContractUpdateProposalAcceptance --------------------------------------------//

impl From<ContractUpdateProposalAcceptance> for proto::types::ContractUpdateProposalAcceptance {
    fn from(value: ContractUpdateProposalAcceptance) -> Self {
        Self {
            proposal_id: value.proposal_id,
            validator_node_public_key: value.validator_node_public_key.as_bytes().to_vec(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<proto::types::ContractUpdateProposalAcceptance> for ContractUpdateProposalAcceptance {
    type Error = String;

    fn try_from(value: proto::types::ContractUpdateProposalAcceptance) -> Result<Self, Self::Error> {
        let validator_node_public_key =
            PublicKey::from_bytes(value.validator_node_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()?;

        Ok(Self {
            proposal_id: value.proposal_id,
            validator_node_public_key,
            signature,
        })
    }
}

//---------------------------------- ContractAmendment --------------------------------------------//

impl From<ContractAmendment> for proto::types::ContractAmendment {
    fn from(value: ContractAmendment) -> Self {
        Self {
            proposal_id: value.proposal_id,
            validator_committee: Some(value.validator_committee.into()),
            validator_signatures: Some(value.validator_signatures.into()),
            updated_constitution: Some(value.updated_constitution.into()),
            activation_window: value.activation_window,
        }
    }
}

impl TryFrom<proto::types::ContractAmendment> for ContractAmendment {
    type Error = String;

    fn try_from(value: proto::types::ContractAmendment) -> Result<Self, Self::Error> {
        let validator_committee = value
            .validator_committee
            .map(TryInto::try_into)
            .ok_or("validator_committee not provided")??;

        let validator_signatures = value
            .validator_signatures
            .map(TryInto::try_into)
            .ok_or("validator_signatures not provided")??;

        let updated_constitution = value
            .updated_constitution
            .ok_or_else(|| "updated_constiution not provided".to_string())?
            .try_into()?;

        Ok(Self {
            proposal_id: value.proposal_id,
            validator_committee,
            validator_signatures,
            updated_constitution,
            activation_window: value.activation_window,
        })
    }
}

//---------------------------------- SideChainConsensus --------------------------------------------//
impl From<SideChainConsensus> for proto::types::SideChainConsensus {
    fn from(value: SideChainConsensus) -> Self {
        #[allow(clippy::enum_glob_use)]
        use proto::types::SideChainConsensus::*;
        match value {
            SideChainConsensus::Bft => Bft,
            SideChainConsensus::ProofOfWork => ProofOfWork,
            SideChainConsensus::MerkleRoot => MerkleRoot,
        }
    }
}

impl TryFrom<proto::types::SideChainConsensus> for SideChainConsensus {
    type Error = String;

    fn try_from(value: proto::types::SideChainConsensus) -> Result<Self, Self::Error> {
        #[allow(clippy::enum_glob_use)]
        use proto::types::SideChainConsensus::*;
        match value {
            Unspecified => Err("Side chain consensus not specified or invalid".to_string()),
            Bft => Ok(SideChainConsensus::Bft),
            ProofOfWork => Ok(SideChainConsensus::ProofOfWork),
            MerkleRoot => Ok(SideChainConsensus::MerkleRoot),
        }
    }
}

//---------------------------------- CheckpointParameters --------------------------------------------//
impl From<CheckpointParameters> for proto::types::CheckpointParameters {
    fn from(value: CheckpointParameters) -> Self {
        Self {
            minimum_quorum_required: value.minimum_quorum_required,
            abandoned_interval: value.abandoned_interval,
            quarantine_interval: value.quarantine_interval,
        }
    }
}

impl TryFrom<proto::types::CheckpointParameters> for CheckpointParameters {
    type Error = String;

    fn try_from(value: proto::types::CheckpointParameters) -> Result<Self, Self::Error> {
        Ok(Self {
            minimum_quorum_required: value.minimum_quorum_required,
            abandoned_interval: value.abandoned_interval,
            quarantine_interval: value.quarantine_interval,
        })
    }
}

//---------------------------------- ConstitutionChangeRules --------------------------------------------//
impl From<ConstitutionChangeRules> for proto::types::ConstitutionChangeRules {
    fn from(value: ConstitutionChangeRules) -> Self {
        Self {
            change_flags: value.change_flags.bits().into(),
            requirements_for_constitution_change: value.requirements_for_constitution_change.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::ConstitutionChangeRules> for ConstitutionChangeRules {
    type Error = String;

    fn try_from(value: proto::types::ConstitutionChangeRules) -> Result<Self, Self::Error> {
        Ok(Self {
            change_flags: u8::try_from(value.change_flags)
                .ok()
                .and_then(ConstitutionChangeFlags::from_bits)
                .ok_or("Invalid change_flags")?,
            requirements_for_constitution_change: value
                .requirements_for_constitution_change
                .map(RequirementsForConstitutionChange::try_from)
                .transpose()?,
        })
    }
}

//---------------------------------- RequirementsForConstitutionChange --------------------------------------------//
impl From<RequirementsForConstitutionChange> for proto::types::RequirementsForConstitutionChange {
    fn from(value: RequirementsForConstitutionChange) -> Self {
        Self {
            minimum_constitution_committee_signatures: value.minimum_constitution_committee_signatures,
            constitution_committee: value.constitution_committee.map(Into::into),
            backup_keys: value.backup_keys.map(Into::into),
        }
    }
}

impl TryFrom<proto::types::RequirementsForConstitutionChange> for RequirementsForConstitutionChange {
    type Error = String;

    fn try_from(value: proto::types::RequirementsForConstitutionChange) -> Result<Self, Self::Error> {
        Ok(Self {
            minimum_constitution_committee_signatures: value.minimum_constitution_committee_signatures,
            constitution_committee: value
                .constitution_committee
                .map(CommitteeMembers::try_from)
                .transpose()?,
            backup_keys: value.backup_keys.map(CommitteeMembers::try_from).transpose()?,
        })
    }
}

//---------------------------------- CommitteeMembers --------------------------------------------//
impl From<CommitteeMembers> for proto::types::CommitteeMembers {
    fn from(value: CommitteeMembers) -> Self {
        Self {
            members: value.members().iter().map(|pk| pk.to_vec()).collect(),
        }
    }
}

impl TryFrom<proto::types::CommitteeMembers> for CommitteeMembers {
    type Error = String;

    fn try_from(value: proto::types::CommitteeMembers) -> Result<Self, Self::Error> {
        if value.members.len() > CommitteeMembers::MAX_MEMBERS {
            return Err(format!(
                "Too many committee members: expected {} but got {}",
                CommitteeMembers::MAX_MEMBERS,
                value.members.len()
            ));
        }

        let members = value
            .members
            .iter()
            .enumerate()
            .map(|(i, c)| {
                PublicKey::from_bytes(c)
                    .map_err(|err| format!("committee member #{} was not a valid public key: {}", i + 1, err))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let members = CommitteeMembers::try_from(members).map_err(|e| e.to_string())?;
        Ok(members)
    }
}

//---------------------------------- CommitteeSignatures --------------------------------------------//
impl From<CommitteeSignatures> for proto::types::CommitteeSignatures {
    fn from(value: CommitteeSignatures) -> Self {
        Self {
            signatures: value.signatures().iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<proto::types::CommitteeSignatures> for CommitteeSignatures {
    type Error = String;

    fn try_from(value: proto::types::CommitteeSignatures) -> Result<Self, Self::Error> {
        if value.signatures.len() > CommitteeSignatures::MAX_SIGNATURES {
            return Err(format!(
                "Too many committee signatures: expected {} but got {}",
                CommitteeSignatures::MAX_SIGNATURES,
                value.signatures.len()
            ));
        }

        let signatures = value
            .signatures
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                SignerSignature::try_from(s)
                    .map_err(|err| format!("committee signature #{} was not a valid signature: {}", i + 1, err))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let signatures = CommitteeSignatures::try_from(signatures).map_err(|e| e.to_string())?;
        Ok(signatures)
    }
}

// TODO: deprecated

impl TryFrom<proto::types::AssetOutputFeatures> for AssetOutputFeatures {
    type Error = String;

    fn try_from(features: proto::types::AssetOutputFeatures) -> Result<Self, Self::Error> {
        let public_key = PublicKey::from_bytes(features.public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            public_key,
            template_ids_implemented: features.template_ids_implemented,
            template_parameters: features.template_parameters.into_iter().map(|s| s.into()).collect(),
        })
    }
}

impl From<AssetOutputFeatures> for proto::types::AssetOutputFeatures {
    fn from(features: AssetOutputFeatures) -> Self {
        Self {
            public_key: features.public_key.as_bytes().to_vec(),
            template_ids_implemented: features.template_ids_implemented,
            template_parameters: features.template_parameters.into_iter().map(|tp| tp.into()).collect(),
        }
    }
}

impl From<proto::types::TemplateParameter> for TemplateParameter {
    fn from(source: proto::types::TemplateParameter) -> Self {
        Self {
            template_id: source.template_id,
            template_data_version: source.template_data_version,
            template_data: source.template_data,
        }
    }
}

impl From<TemplateParameter> for proto::types::TemplateParameter {
    fn from(source: TemplateParameter) -> Self {
        Self {
            template_id: source.template_id,
            template_data_version: source.template_data_version,
            template_data: source.template_data,
        }
    }
}

impl TryFrom<proto::types::MintNonFungibleFeatures> for MintNonFungibleFeatures {
    type Error = String;

    fn try_from(value: proto::types::MintNonFungibleFeatures) -> Result<Self, Self::Error> {
        let asset_public_key =
            PublicKey::from_bytes(value.asset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let asset_owner_commitment = value
            .asset_owner_commitment
            .map(|c| Commitment::from_bytes(&c.data))
            .ok_or_else(|| "asset_owner_commitment is missing".to_string())?
            .map_err(|err| err.to_string())?;
        Ok(Self {
            asset_public_key,
            asset_owner_commitment,
        })
    }
}

impl From<MintNonFungibleFeatures> for proto::types::MintNonFungibleFeatures {
    fn from(value: MintNonFungibleFeatures) -> Self {
        Self {
            asset_public_key: value.asset_public_key.as_bytes().to_vec(),
            asset_owner_commitment: Some(value.asset_owner_commitment.into()),
        }
    }
}

impl TryFrom<proto::types::SideChainCheckpointFeatures> for SideChainCheckpointFeatures {
    type Error = String;

    fn try_from(value: proto::types::SideChainCheckpointFeatures) -> Result<Self, Self::Error> {
        if value.merkle_root.len() != FixedHash::byte_size() {
            return Err(format!(
                "Invalid side chain checkpoint merkle length {}",
                value.merkle_root.len()
            ));
        }
        let merkle_root = FixedHash::try_from(value.merkle_root).map_err(|e| e.to_string())?;
        let committee = value
            .committee
            .into_iter()
            .map(|c| PublicKey::from_bytes(&c).map_err(|err| format!("{:?}", err)))
            .collect::<Result<_, _>>()?;
        Ok(Self { merkle_root, committee })
    }
}

impl From<SideChainCheckpointFeatures> for proto::types::SideChainCheckpointFeatures {
    fn from(value: SideChainCheckpointFeatures) -> Self {
        Self {
            merkle_root: value.merkle_root.as_bytes().to_vec(),
            committee: value.committee.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
        }
    }
}

impl TryFrom<proto::types::CommitteeDefinitionFeatures> for CommitteeDefinitionFeatures {
    type Error = String;

    fn try_from(value: proto::types::CommitteeDefinitionFeatures) -> Result<Self, Self::Error> {
        let committee = value
            .committee
            .into_iter()
            .map(|c| PublicKey::from_bytes(&c).map_err(|err| format!("{:?}", err)))
            .collect::<Result<_, _>>()?;
        let effective_sidechain_height = value.effective_sidechain_height;

        Ok(Self {
            committee,
            effective_sidechain_height,
        })
    }
}

impl From<CommitteeDefinitionFeatures> for proto::types::CommitteeDefinitionFeatures {
    fn from(value: CommitteeDefinitionFeatures) -> Self {
        Self {
            committee: value.committee.into_iter().map(|c| c.as_bytes().to_vec()).collect(),
            effective_sidechain_height: value.effective_sidechain_height,
        }
    }
}

//---------------------------------- ContractDefinition --------------------------------------------//

impl TryFrom<proto::types::ContractDefinition> for ContractDefinition {
    type Error = String;

    fn try_from(value: proto::types::ContractDefinition) -> Result<Self, Self::Error> {
        let contract_name = bytes_into_fixed_string(value.contract_name);

        let contract_issuer =
            PublicKey::from_bytes(value.contract_issuer.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let contract_spec = value
            .contract_spec
            .map(ContractSpecification::try_from)
            .ok_or_else(|| "contract_spec is missing".to_string())??;

        Ok(Self {
            contract_name,
            contract_issuer,
            contract_spec,
        })
    }
}

impl From<ContractDefinition> for proto::types::ContractDefinition {
    fn from(value: ContractDefinition) -> Self {
        let contract_name = value.contract_name.as_bytes().to_vec();
        let contract_issuer = value.contract_issuer.as_bytes().to_vec();

        Self {
            contract_name,
            contract_issuer,
            contract_spec: Some(value.contract_spec.into()),
        }
    }
}

impl TryFrom<proto::types::ContractSpecification> for ContractSpecification {
    type Error = String;

    fn try_from(value: proto::types::ContractSpecification) -> Result<Self, Self::Error> {
        let runtime = bytes_into_fixed_string(value.runtime);
        let public_functions = value
            .public_functions
            .into_iter()
            .map(PublicFunction::try_from)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            runtime,
            public_functions,
        })
    }
}

impl From<ContractSpecification> for proto::types::ContractSpecification {
    fn from(value: ContractSpecification) -> Self {
        let public_functions = value.public_functions.into_iter().map(|f| f.into()).collect();
        Self {
            runtime: value.runtime.as_bytes().to_vec(),
            public_functions,
        }
    }
}

impl TryFrom<proto::types::PublicFunction> for PublicFunction {
    type Error = String;

    fn try_from(value: proto::types::PublicFunction) -> Result<Self, Self::Error> {
        let function = value
            .function
            .map(FunctionRef::try_from)
            .ok_or_else(|| "function is missing".to_string())??;

        Ok(Self {
            name: bytes_into_fixed_string(value.name),
            function,
        })
    }
}

impl From<PublicFunction> for proto::types::PublicFunction {
    fn from(value: PublicFunction) -> Self {
        Self {
            name: value.name.as_bytes().to_vec(),
            function: Some(value.function.into()),
        }
    }
}

impl TryFrom<proto::types::FunctionRef> for FunctionRef {
    type Error = String;

    fn try_from(value: proto::types::FunctionRef) -> Result<Self, Self::Error> {
        let template_id = FixedHash::try_from(value.template_id).map_err(|err| format!("{:?}", err))?;
        let function_id = u16::try_from(value.function_id).map_err(|_| "Invalid function_id: overflowed u16")?;

        Ok(Self {
            template_id,
            function_id,
        })
    }
}

impl From<FunctionRef> for proto::types::FunctionRef {
    fn from(value: FunctionRef) -> Self {
        let template_id = value.template_id.as_bytes().to_vec();

        Self {
            template_id,
            function_id: value.function_id.into(),
        }
    }
}
