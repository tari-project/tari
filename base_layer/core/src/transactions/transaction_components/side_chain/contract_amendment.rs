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

use super::{CommitteeMembers, CommitteeSignatures, ContractConstitution};
use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

/// # ContractAmendment
///
/// This details a ratification of a contract update proposal, accepted by all the required validator nodes
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractAmendment {
    /// The unique identification of the proposal
    pub proposal_id: u64,
    /// The committee of validator nodes accepnting the changes
    pub validator_committee: CommitteeMembers,
    /// Signatures for all the proposal acceptances of the validator committee
    pub validator_signatures: CommitteeSignatures,
    /// Reiteration of the accepted constitution changes
    pub updated_constitution: ContractConstitution,
    /// Number of blocks until the contract changes are enforced by the base layer
    pub activation_window: u64,
}

impl ConsensusEncoding for ContractAmendment {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.proposal_id.consensus_encode(writer)?;
        self.validator_committee.consensus_encode(writer)?;
        self.validator_signatures.consensus_encode(writer)?;
        self.updated_constitution.consensus_encode(writer)?;
        self.activation_window.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractAmendment {}

impl ConsensusDecoding for ContractAmendment {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            proposal_id: u64::consensus_decode(reader)?,
            validator_committee: CommitteeMembers::consensus_decode(reader)?,
            validator_signatures: CommitteeSignatures::consensus_decode(reader)?,
            updated_constitution: ContractConstitution::consensus_decode(reader)?,
            activation_window: u64::consensus_decode(reader)?,
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

        let amendment = ContractAmendment {
            proposal_id: 0_u64,
            validator_committee: CommitteeMembers::new(vec![].try_into().unwrap()),
            validator_signatures: CommitteeSignatures::new(vec![].try_into().unwrap()),
            updated_constitution: constitution,
            activation_window: 0_u64,
        };

        check_consensus_encoding_correctness(amendment).unwrap();
    }
}
