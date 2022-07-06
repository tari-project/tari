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

use std::{
    borrow::Borrow,
    convert::{TryFrom, TryInto},
};

use tari_common_types::types::{FixedHash, PublicKey};
use tari_core::transactions::transaction_components::{
    bytes_into_fixed_string,
    CheckpointParameters,
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
    PublicFunction,
    RequirementsForConstitutionChange,
    SideChainConsensus,
    SideChainFeatures,
    SignerSignature,
};
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

impl From<SideChainFeatures> for grpc::SideChainFeatures {
    fn from(value: SideChainFeatures) -> Self {
        Self {
            contract_id: value.contract_id.to_vec(),
            definition: value.definition.map(Into::into),
            constitution: value.constitution.map(Into::into),
            acceptance: value.acceptance.map(Into::into),
            update_proposal: value.update_proposal.map(Into::into),
            update_proposal_acceptance: value.update_proposal_acceptance.map(Into::into),
            amendment: value.amendment.map(Into::into),
            checkpoint: value.checkpoint.map(Into::into),
        }
    }
}

impl TryFrom<grpc::SideChainFeatures> for SideChainFeatures {
    type Error = String;

    fn try_from(features: grpc::SideChainFeatures) -> Result<Self, Self::Error> {
        let definition = features.definition.map(ContractDefinition::try_from).transpose()?;
        let constitution = features.constitution.map(ContractConstitution::try_from).transpose()?;
        let acceptance = features.acceptance.map(ContractAcceptance::try_from).transpose()?;
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
            contract_id: features.contract_id.try_into().map_err(|_| "Invalid contract_id")?,
            definition,
            constitution,
            acceptance,
            update_proposal,
            update_proposal_acceptance,
            amendment,
            checkpoint,
        })
    }
}

impl TryFrom<grpc::CreateConstitutionDefinitionRequest> for SideChainFeatures {
    type Error = String;

    fn try_from(request: grpc::CreateConstitutionDefinitionRequest) -> Result<Self, Self::Error> {
        let acceptance_period_expiry = request.acceptance_period_expiry;
        let minimum_quorum_required = request.minimum_quorum_required;
        let validator_committee = request
            .validator_committee
            .map(CommitteeMembers::try_from)
            .transpose()?
            .unwrap();

        Ok(Self {
            contract_id: request.contract_id.try_into().map_err(|_| "Invalid contract_id")?,
            definition: None,
            constitution: Some(ContractConstitution {
                validator_committee: validator_committee.clone(),
                acceptance_requirements: ContractAcceptanceRequirements {
                    minimum_quorum_required: minimum_quorum_required as u32,
                    acceptance_period_expiry,
                },
                consensus: SideChainConsensus::MerkleRoot,
                checkpoint_params: CheckpointParameters {
                    minimum_quorum_required: 5,
                    abandoned_interval: 100,
                },
                constitution_change_rules: ConstitutionChangeRules {
                    change_flags: ConstitutionChangeFlags::all(),
                    requirements_for_constitution_change: Some(RequirementsForConstitutionChange {
                        minimum_constitution_committee_signatures: 5,
                        constitution_committee: Some(validator_committee),
                    }),
                },
                initial_reward: 100.into(),
            }),
            acceptance: None,
            update_proposal: None,
            update_proposal_acceptance: None,
            amendment: None,
            checkpoint: None,
        })
    }
}

//---------------------------------- ContractDefinition --------------------------------------------//

impl TryFrom<grpc::ContractDefinition> for ContractDefinition {
    type Error = String;

    fn try_from(value: grpc::ContractDefinition) -> Result<Self, Self::Error> {
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

impl From<ContractDefinition> for grpc::ContractDefinition {
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

impl TryFrom<grpc::ContractSpecification> for ContractSpecification {
    type Error = String;

    fn try_from(value: grpc::ContractSpecification) -> Result<Self, Self::Error> {
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

impl From<ContractSpecification> for grpc::ContractSpecification {
    fn from(value: ContractSpecification) -> Self {
        let public_functions = value.public_functions.into_iter().map(|f| f.into()).collect();
        Self {
            runtime: value.runtime.as_bytes().to_vec(),
            public_functions,
        }
    }
}

impl TryFrom<grpc::PublicFunction> for PublicFunction {
    type Error = String;

    fn try_from(value: grpc::PublicFunction) -> Result<Self, Self::Error> {
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

impl From<PublicFunction> for grpc::PublicFunction {
    fn from(value: PublicFunction) -> Self {
        Self {
            name: value.name.as_bytes().to_vec(),
            function: Some(value.function.into()),
        }
    }
}

impl TryFrom<grpc::FunctionRef> for FunctionRef {
    type Error = String;

    fn try_from(value: grpc::FunctionRef) -> Result<Self, Self::Error> {
        let template_id = FixedHash::try_from(value.template_id).map_err(|err| format!("{:?}", err))?;
        let function_id = u16::try_from(value.function_id).map_err(|_| "Invalid function_id: overflowed u16")?;

        Ok(Self {
            template_id,
            function_id,
        })
    }
}

impl From<FunctionRef> for grpc::FunctionRef {
    fn from(value: FunctionRef) -> Self {
        let template_id = value.template_id.as_bytes().to_vec();

        Self {
            template_id,
            function_id: value.function_id.into(),
        }
    }
}

//---------------------------------- ContractConstitution --------------------------------------------//
impl From<ContractConstitution> for grpc::ContractConstitution {
    fn from(value: ContractConstitution) -> Self {
        Self {
            validator_committee: Some(value.validator_committee.into()),
            acceptance_requirements: Some(value.acceptance_requirements.into()),
            consensus: value.consensus.into(),
            checkpoint_params: Some(value.checkpoint_params.into()),
            constitution_change_rules: Some(value.constitution_change_rules.into()),
            initial_reward: value.initial_reward.into(),
        }
    }
}

impl TryFrom<grpc::ContractConstitution> for ContractConstitution {
    type Error = String;

    fn try_from(value: grpc::ContractConstitution) -> Result<Self, Self::Error> {
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
        let initial_reward = value.initial_reward.into();

        Ok(Self {
            validator_committee,
            acceptance_requirements,
            consensus,
            checkpoint_params,
            constitution_change_rules,
            initial_reward,
        })
    }
}

//---------------------------------- ContractCheckpoint --------------------------------------------//
impl From<ContractCheckpoint> for grpc::ContractCheckpoint {
    fn from(value: ContractCheckpoint) -> Self {
        Self {
            checkpoint_number: value.checkpoint_number,
            merkle_root: value.merkle_root.to_vec(),
            signatures: Some(value.signatures.into()),
        }
    }
}

impl TryFrom<grpc::ContractCheckpoint> for ContractCheckpoint {
    type Error = String;

    fn try_from(value: grpc::ContractCheckpoint) -> Result<Self, Self::Error> {
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
impl From<ContractAcceptanceRequirements> for grpc::ContractAcceptanceRequirements {
    fn from(value: ContractAcceptanceRequirements) -> Self {
        Self {
            acceptance_period_expiry: value.acceptance_period_expiry,
            minimum_quorum_required: value.minimum_quorum_required,
        }
    }
}

impl TryFrom<grpc::ContractAcceptanceRequirements> for ContractAcceptanceRequirements {
    type Error = String;

    fn try_from(value: grpc::ContractAcceptanceRequirements) -> Result<Self, Self::Error> {
        Ok(Self {
            acceptance_period_expiry: value.acceptance_period_expiry,
            minimum_quorum_required: value.minimum_quorum_required,
        })
    }
}

//---------------------------------- SideChainConsensus --------------------------------------------//
impl From<SideChainConsensus> for grpc::SideChainConsensus {
    fn from(value: SideChainConsensus) -> Self {
        #[allow(clippy::enum_glob_use)]
        use grpc::SideChainConsensus::*;
        match value {
            SideChainConsensus::Bft => Bft,
            SideChainConsensus::ProofOfWork => ProofOfWork,
            SideChainConsensus::MerkleRoot => MerkleRoot,
        }
    }
}

impl TryFrom<grpc::SideChainConsensus> for SideChainConsensus {
    type Error = String;

    fn try_from(value: grpc::SideChainConsensus) -> Result<Self, Self::Error> {
        #[allow(clippy::enum_glob_use)]
        use grpc::SideChainConsensus::*;
        match value {
            Unspecified => Err("Side chain consensus not specified or invalid".to_string()),
            Bft => Ok(SideChainConsensus::Bft),
            ProofOfWork => Ok(SideChainConsensus::ProofOfWork),
            MerkleRoot => Ok(SideChainConsensus::MerkleRoot),
        }
    }
}

//---------------------------------- CheckpointParameters --------------------------------------------//
impl From<CheckpointParameters> for grpc::CheckpointParameters {
    fn from(value: CheckpointParameters) -> Self {
        Self {
            minimum_quorum_required: value.minimum_quorum_required,
            abandoned_interval: value.abandoned_interval,
        }
    }
}

impl TryFrom<grpc::CheckpointParameters> for CheckpointParameters {
    type Error = String;

    fn try_from(value: grpc::CheckpointParameters) -> Result<Self, Self::Error> {
        Ok(Self {
            minimum_quorum_required: value.minimum_quorum_required,
            abandoned_interval: value.abandoned_interval,
        })
    }
}

//---------------------------------- ConstitutionChangeRules --------------------------------------------//
impl From<ConstitutionChangeRules> for grpc::ConstitutionChangeRules {
    fn from(value: ConstitutionChangeRules) -> Self {
        Self {
            change_flags: value.change_flags.bits().into(),
            requirements_for_constitution_change: value.requirements_for_constitution_change.map(Into::into),
        }
    }
}

impl TryFrom<grpc::ConstitutionChangeRules> for ConstitutionChangeRules {
    type Error = String;

    fn try_from(value: grpc::ConstitutionChangeRules) -> Result<Self, Self::Error> {
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
impl From<RequirementsForConstitutionChange> for grpc::RequirementsForConstitutionChange {
    fn from(value: RequirementsForConstitutionChange) -> Self {
        Self {
            minimum_constitution_committee_signatures: value.minimum_constitution_committee_signatures,
            constitution_committee: value.constitution_committee.map(Into::into),
        }
    }
}

impl TryFrom<grpc::RequirementsForConstitutionChange> for RequirementsForConstitutionChange {
    type Error = String;

    fn try_from(value: grpc::RequirementsForConstitutionChange) -> Result<Self, Self::Error> {
        Ok(Self {
            minimum_constitution_committee_signatures: value.minimum_constitution_committee_signatures,
            constitution_committee: value
                .constitution_committee
                .map(CommitteeMembers::try_from)
                .transpose()?,
        })
    }
}

//---------------------------------- CommitteeMembers --------------------------------------------//
impl From<CommitteeMembers> for grpc::CommitteeMembers {
    fn from(value: CommitteeMembers) -> Self {
        Self {
            members: value.members().iter().map(|pk| pk.to_vec()).collect(),
        }
    }
}

impl TryFrom<grpc::CommitteeMembers> for CommitteeMembers {
    type Error = String;

    fn try_from(value: grpc::CommitteeMembers) -> Result<Self, Self::Error> {
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
impl From<CommitteeSignatures> for grpc::CommitteeSignatures {
    fn from(value: CommitteeSignatures) -> Self {
        Self {
            signatures: value.signatures().iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<grpc::CommitteeSignatures> for CommitteeSignatures {
    type Error = String;

    fn try_from(value: grpc::CommitteeSignatures) -> Result<Self, Self::Error> {
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

//---------------------------------- SignerSignature --------------------------------------------//
impl<B: Borrow<SignerSignature>> From<B> for grpc::SignerSignature {
    fn from(value: B) -> Self {
        Self {
            signer: value.borrow().signer.to_vec(),
            signature: Some(grpc::Signature::from(&value.borrow().signature)),
        }
    }
}

impl TryFrom<grpc::SignerSignature> for SignerSignature {
    type Error = String;

    fn try_from(value: grpc::SignerSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            signer: PublicKey::from_bytes(&value.signer).map_err(|err| err.to_string())?,
            signature: value
                .signature
                .map(TryInto::try_into)
                .ok_or("signature not provided")??,
        })
    }
}
//---------------------------------- ContractAcceptance --------------------------------------------//

impl From<ContractAcceptance> for grpc::ContractAcceptance {
    fn from(value: ContractAcceptance) -> Self {
        Self {
            validator_node_public_key: value.validator_node_public_key.as_bytes().to_vec(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<grpc::ContractAcceptance> for ContractAcceptance {
    type Error = String;

    fn try_from(value: grpc::ContractAcceptance) -> Result<Self, Self::Error> {
        let validator_node_public_key =
            PublicKey::from_bytes(value.validator_node_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()
            .map_err(|_| "signature could not be converted".to_string())?;

        Ok(Self {
            validator_node_public_key,
            signature,
        })
    }
}

//---------------------------------- ContractUpdateProposal --------------------------------------------//

impl From<ContractUpdateProposal> for grpc::ContractUpdateProposal {
    fn from(value: ContractUpdateProposal) -> Self {
        Self {
            proposal_id: value.proposal_id,
            signature: Some(value.signature.into()),
            updated_constitution: Some(value.updated_constitution.into()),
        }
    }
}

impl TryFrom<grpc::ContractUpdateProposal> for ContractUpdateProposal {
    type Error = String;

    fn try_from(value: grpc::ContractUpdateProposal) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()
            .map_err(|_| "signature could not be converted".to_string())?;

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

impl From<ContractUpdateProposalAcceptance> for grpc::ContractUpdateProposalAcceptance {
    fn from(value: ContractUpdateProposalAcceptance) -> Self {
        Self {
            proposal_id: value.proposal_id,
            validator_node_public_key: value.validator_node_public_key.as_bytes().to_vec(),
            signature: Some(value.signature.into()),
        }
    }
}

impl TryFrom<grpc::ContractUpdateProposalAcceptance> for ContractUpdateProposalAcceptance {
    type Error = String;

    fn try_from(value: grpc::ContractUpdateProposalAcceptance) -> Result<Self, Self::Error> {
        let validator_node_public_key =
            PublicKey::from_bytes(value.validator_node_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let signature = value
            .signature
            .ok_or_else(|| "signature not provided".to_string())?
            .try_into()
            .map_err(|_| "signature could not be converted".to_string())?;

        Ok(Self {
            proposal_id: value.proposal_id,
            validator_node_public_key,
            signature,
        })
    }
}

//---------------------------------- ContractAmendment --------------------------------------------//

impl From<ContractAmendment> for grpc::ContractAmendment {
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

impl TryFrom<grpc::ContractAmendment> for ContractAmendment {
    type Error = String;

    fn try_from(value: grpc::ContractAmendment) -> Result<Self, Self::Error> {
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
