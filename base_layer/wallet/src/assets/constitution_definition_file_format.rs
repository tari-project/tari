// Copyright 2022. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{FixedHash, PublicKey};
use tari_core::transactions::transaction_components::{
    CheckpointParameters,
    ConstitutionChangeFlags,
    ConstitutionChangeRules,
    ContractAcceptanceRequirements,
    ContractConstitution,
    RequirementsForConstitutionChange,
    SideChainConsensus,
    SideChainFeatures,
};
use tari_utilities::hex::Hex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstitutionDefinitionFileFormat {
    pub contract_id: String,
    pub validator_committee: Vec<PublicKey>,
    pub consensus: SideChainConsensus,
    pub initial_reward: u64,
    pub acceptance_parameters: ContractAcceptanceRequirements,
    pub checkpoint_parameters: CheckpointParameters,
    pub constitution_change_rules: ConstitutionChangeRulesFileFormat,
}

impl TryFrom<ConstitutionDefinitionFileFormat> for ContractConstitution {
    type Error = String;

    fn try_from(value: ConstitutionDefinitionFileFormat) -> Result<Self, Self::Error> {
        Ok(Self {
            validator_committee: value.validator_committee.try_into().map_err(|e| format!("{}", e))?,
            acceptance_requirements: value.acceptance_parameters,
            consensus: value.consensus,
            checkpoint_params: value.checkpoint_parameters,
            constitution_change_rules: value.constitution_change_rules.try_into()?,
            initial_reward: value.initial_reward.into(),
        })
    }
}

impl TryFrom<ConstitutionDefinitionFileFormat> for SideChainFeatures {
    type Error = String;

    fn try_from(value: ConstitutionDefinitionFileFormat) -> Result<Self, Self::Error> {
        let contract_id = FixedHash::from_hex(&value.contract_id).map_err(|e| format!("{}", e))?;

        Ok(Self::builder(contract_id)
            .with_contract_constitution(value.try_into()?)
            .finish())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConstitutionChangeRulesFileFormat {
    pub change_flags: u8,
    pub requirements_for_constitution_change: Option<RequirementsForConstitutionChange>,
}

impl TryFrom<ConstitutionChangeRulesFileFormat> for ConstitutionChangeRules {
    type Error = String;

    fn try_from(value: ConstitutionChangeRulesFileFormat) -> Result<Self, Self::Error> {
        Ok(Self {
            change_flags: ConstitutionChangeFlags::from_bits(value.change_flags).ok_or("Invalid change_flags")?,
            requirements_for_constitution_change: value.requirements_for_constitution_change,
        })
    }
}
