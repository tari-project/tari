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

use std::io::{Error, Read, Write};

use serde::{Deserialize, Serialize};
use tari_common_types::types::FixedHash;

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized},
    transactions::transaction_components::ContractConstitution,
};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct SideChainFeatures {
    pub contract_id: FixedHash,
    pub constitution: Option<ContractConstitution>,
}

impl SideChainFeatures {
    pub fn new(contract_id: FixedHash) -> Self {
        Self::builder(contract_id).finish()
    }

    pub fn builder(contract_id: FixedHash) -> SideChainFeaturesBuilder {
        SideChainFeaturesBuilder::new(contract_id)
    }
}

impl ConsensusEncoding for SideChainFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.contract_id.consensus_encode(writer)?;
        self.constitution.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for SideChainFeatures {}

impl ConsensusDecoding for SideChainFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            contract_id: FixedHash::consensus_decode(reader)?,
            constitution: ConsensusDecoding::consensus_decode(reader)?,
        })
    }
}

pub struct SideChainFeaturesBuilder {
    features: SideChainFeatures,
}

impl SideChainFeaturesBuilder {
    pub fn new(contract_id: FixedHash) -> Self {
        Self {
            features: SideChainFeatures {
                contract_id,
                constitution: None,
            },
        }
    }

    pub fn with_contract_constitution(mut self, contract_constitution: ContractConstitution) -> Self {
        self.features.constitution = Some(contract_constitution);
        self
    }

    pub fn finish(self) -> SideChainFeatures {
        self.features
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use tari_common_types::types::PublicKey;

    use super::*;
    use crate::{
        consensus::check_consensus_encoding_correctness,
        transactions::transaction_components::{
            CheckpointParameters,
            CommitteeMembers,
            ConstitutionChangeFlags,
            ConstitutionChangeRules,
            ContractAcceptanceRequirements,
            RequirementsForConstitutionChange,
            SideChainConsensus,
        },
    };

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = SideChainFeatures {
            contract_id: FixedHash::zero(),
            constitution: Some(ContractConstitution {
                validator_committee: vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                    .try_into()
                    .unwrap(),
                acceptance_requirements: ContractAcceptanceRequirements {
                    acceptance_period_expiry: 100,
                    minimum_quorum_required: 5,
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
                        constitution_committee: Some(
                            vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                                .try_into()
                                .unwrap(),
                        ),
                    }),
                },
                initial_reward: 100.into(),
            }),
        };

        check_consensus_encoding_correctness(subject).unwrap();
    }
}
