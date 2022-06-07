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
use tari_common_types::types::Signature;

use super::ContractConstitution;
use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

/// # ContractUpdateProposal
///
/// This details a proposal of changes in a contract constitution, so the committee member can accept or reject.
/// It specifies all the fields in the new constitution.
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractUpdateProposal {
    /// A unique identification of the proposal, for later reference
    pub proposal_id: u64,
    /// Signature of the proposal
    pub signature: Signature,
    /// The new constitution that is proposed
    pub updated_constitution: ContractConstitution,
}

impl ConsensusEncoding for ContractUpdateProposal {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.proposal_id.consensus_encode(writer)?;
        self.signature.consensus_encode(writer)?;
        self.updated_constitution.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractUpdateProposal {}

impl ConsensusDecoding for ContractUpdateProposal {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            proposal_id: u64::consensus_decode(reader)?,
            signature: Signature::consensus_decode(reader)?,
            updated_constitution: ContractConstitution::consensus_decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use tari_common_types::types::PublicKey;

    use super::*;
    use crate::{
        consensus::check_consensus_encoding_correctness,
        transactions::{
            tari_amount::MicroTari,
            transaction_components::{
                CheckpointParameters,
                CommitteeMembers,
                ConstitutionChangeFlags,
                ConstitutionChangeRules,
                ContractAcceptanceRequirements,
                RequirementsForConstitutionChange,
                SideChainConsensus,
            },
        },
    };

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let constitution = ContractConstitution {
            validator_committee: CommitteeMembers::new(vec![].try_into().unwrap()),
            acceptance_requirements: ContractAcceptanceRequirements {
                acceptance_period_expiry: 123,
                minimum_quorum_required: 321,
            },
            consensus: SideChainConsensus::ProofOfWork,
            checkpoint_params: CheckpointParameters {
                minimum_quorum_required: 123,
                abandoned_interval: 321,
            },
            constitution_change_rules: ConstitutionChangeRules {
                change_flags: ConstitutionChangeFlags::all(),
                requirements_for_constitution_change: Some(RequirementsForConstitutionChange {
                    minimum_constitution_committee_signatures: 321,
                    constitution_committee: Some(CommitteeMembers::new(
                        vec![PublicKey::default(); 32].try_into().unwrap(),
                    )),
                }),
            },
            initial_reward: MicroTari::from(123u64),
        };

        let constitution_update_proposal = ContractUpdateProposal {
            proposal_id: 0_u64,
            signature: Signature::default(),
            updated_constitution: constitution,
        };

        check_consensus_encoding_correctness(constitution_update_proposal).unwrap();
    }
}
