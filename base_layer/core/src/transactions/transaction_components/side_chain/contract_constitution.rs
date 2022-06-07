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
    io,
    io::{Error, ErrorKind, Read, Write},
};

use bitflags::bitflags;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};

use super::CommitteeMembers;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized},
    transactions::tari_amount::MicroTari,
};

/// # ContractConstitution
///
/// This details the rules that a validator node committee must follow with respect to the operation (the "how") of the
/// contract and base layer consensus.
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractConstitution {
    /// The committee of validator nodes responsible for execution of the contract.
    pub validator_committee: CommitteeMembers,
    /// The requirements for the contract to pass the acceptance period and become active.
    pub acceptance_requirements: ContractAcceptanceRequirements,
    /// The consensus mechanism that the validator committee is expected to employ. This indicates the proofs required
    /// for checkpointing.
    pub consensus: SideChainConsensus,
    /// The requirements for contract checkpoints.
    pub checkpoint_params: CheckpointParameters,
    /// The rules or restrictions on how and if a constitution may be changed.
    pub constitution_change_rules: ConstitutionChangeRules,
    /// The initial reward paid to validator committee members.
    pub initial_reward: MicroTari,
}

impl ConsensusEncoding for ContractConstitution {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.validator_committee.consensus_encode(writer)?;
        self.acceptance_requirements.consensus_encode(writer)?;
        self.consensus.consensus_encode(writer)?;
        self.checkpoint_params.consensus_encode(writer)?;
        self.constitution_change_rules.consensus_encode(writer)?;
        self.initial_reward.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for ContractConstitution {}

impl ConsensusDecoding for ContractConstitution {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            validator_committee: CommitteeMembers::consensus_decode(reader)?,
            acceptance_requirements: ContractAcceptanceRequirements::consensus_decode(reader)?,
            consensus: SideChainConsensus::consensus_decode(reader)?,
            checkpoint_params: CheckpointParameters::consensus_decode(reader)?,
            constitution_change_rules: ConstitutionChangeRules::consensus_decode(reader)?,
            initial_reward: MicroTari::consensus_decode(reader)?,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractAcceptanceRequirements {
    /// The acceptance expiry period as a relative block height.
    pub acceptance_period_expiry: u64,
    /// The minimum number of acceptance UTXOs required for the contract acceptance period to succeed.
    pub minimum_quorum_required: u32,
}

impl ConsensusEncoding for ContractAcceptanceRequirements {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.acceptance_period_expiry.consensus_encode(writer)?;
        self.minimum_quorum_required.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for ContractAcceptanceRequirements {}

impl ConsensusDecoding for ContractAcceptanceRequirements {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            acceptance_period_expiry: u64::consensus_decode(reader)?,
            minimum_quorum_required: u32::consensus_decode(reader)?,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct CheckpointParameters {
    /// The minimum number of votes (signatures) required on each checkpoint.
    pub minimum_quorum_required: u32,
    /// If this number of blocks have passed without a checkpoint, the contract becomes abandoned.
    pub abandoned_interval: u64,
}

impl ConsensusEncoding for CheckpointParameters {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.minimum_quorum_required.consensus_encode(writer)?;
        self.abandoned_interval.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for CheckpointParameters {}

impl ConsensusDecoding for CheckpointParameters {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            minimum_quorum_required: u32::consensus_decode(reader)?,
            abandoned_interval: u64::consensus_decode(reader)?,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ConstitutionChangeRules {
    /// Bitflag that indicates the constitution changes that are permitted.
    pub change_flags: ConstitutionChangeFlags,
    /// Requirements for amendments to the contract constitution. If None, then the `ContractConstitution` cannot be
    /// changed.
    pub requirements_for_constitution_change: Option<RequirementsForConstitutionChange>,
}

impl ConsensusEncoding for ConstitutionChangeRules {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.change_flags.consensus_encode(writer)?;
        self.requirements_for_constitution_change.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for ConstitutionChangeRules {}

impl ConsensusDecoding for ConstitutionChangeRules {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            change_flags: ConstitutionChangeFlags::consensus_decode(reader)?,
            requirements_for_constitution_change: ConsensusDecoding::consensus_decode(reader)?,
        })
    }
}

bitflags! {
    #[derive(Deserialize, Serialize)]
    pub struct ConstitutionChangeFlags: u8 {
        const COMMITTEE = 0x01;
        const ACCEPTANCE_REQUIREMENTS = 0x02;
        const CONSENSUS = 0x04;
        const CHECKPOINT_PARAMS = 0x08;
        const CONSTITUTION_CHANGE_RULES = 0x10;
    }
}

impl ConsensusEncoding for ConstitutionChangeFlags {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&[self.bits])?;
        Ok(())
    }
}

impl ConsensusEncodingSized for ConstitutionChangeFlags {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for ConstitutionChangeFlags {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let flags = ConstitutionChangeFlags::from_bits(buf[0])
            .ok_or_else(|| io::Error::new(ErrorKind::Other, "Invalid change flag"))?;
        Ok(flags)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Deserialize, Serialize, Eq, FromPrimitive)]
#[repr(u8)]
pub enum SideChainConsensus {
    /// BFT consensus e.g. HotStuff
    Bft = 1,
    /// Proof of work consensus.
    ProofOfWork = 2,
    /// Custom consensus that uses the base layer as a notary for side-chain state. This mode requires that the
    /// checkpoint provides some merklish commitment to the state.
    MerkleRoot = 3,
}

impl ConsensusEncoding for SideChainConsensus {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&[*self as u8])?;
        Ok(())
    }
}

impl ConsensusEncodingSized for SideChainConsensus {
    fn consensus_encode_exact_size(&self) -> usize {
        1
    }
}

impl ConsensusDecoding for SideChainConsensus {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        SideChainConsensus::from_u8(buf[0]).ok_or_else(|| {
            io::Error::new(
                ErrorKind::Other,
                format!("Invalid byte '{}' for SideChainConsensus", buf[0]),
            )
        })
    }
}

impl From<SideChainConsensus> for i32 {
    fn from(value: SideChainConsensus) -> Self {
        value as i32
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct RequirementsForConstitutionChange {
    /// The minimum required constitution committee signatures required for a constitution change proposal to pass.
    pub minimum_constitution_committee_signatures: u32,
    /// An allowlist of keys that are able to accept and ratify the initial constitution and its amendments. If this is
    /// None, the constitution cannot be amended.
    pub constitution_committee: Option<CommitteeMembers>,
}

impl ConsensusEncoding for RequirementsForConstitutionChange {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.minimum_constitution_committee_signatures
            .consensus_encode(writer)?;
        self.constitution_committee.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for RequirementsForConstitutionChange {}

impl ConsensusDecoding for RequirementsForConstitutionChange {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            minimum_constitution_committee_signatures: u32::consensus_decode(reader)?,
            constitution_committee: ConsensusDecoding::consensus_decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use tari_common_types::types::PublicKey;

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = ContractConstitution {
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

        check_consensus_encoding_correctness(subject).unwrap();
    }
}
