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

use tari_common_types::types::PublicKey;
use tari_core::transactions::transaction_components::{
    CheckpointParameters,
    CommitteeMembers,
    ConstitutionChangeFlags,
    ConstitutionChangeRules,
    ContractAcceptanceRequirements,
    ContractConstitution,
    RequirementsForConstitutionChange,
    SideChainConsensus,
    SideChainFeatures,
};
use tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

impl From<SideChainFeatures> for grpc::SideChainFeatures {
    fn from(value: SideChainFeatures) -> Self {
        Self {
            contract_id: value.contract_id.to_vec(),
            constitution: value.constitution.map(Into::into),
        }
    }
}

impl TryFrom<grpc::SideChainFeatures> for SideChainFeatures {
    type Error = String;

    fn try_from(features: grpc::SideChainFeatures) -> Result<Self, Self::Error> {
        let constitution = features.constitution.map(ContractConstitution::try_from).transpose()?;

        Ok(Self {
            contract_id: features.contract_id.try_into().map_err(|_| "Invalid contract_id")?,
            constitution,
        })
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
