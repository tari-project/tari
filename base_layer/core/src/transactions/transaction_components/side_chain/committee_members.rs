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

use std::io;

use serde::{Deserialize, Serialize};
use tari_common_types::types::PublicKey;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeVec};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq, Default)]
pub struct CommitteeMembers {
    members: MaxSizeVec<PublicKey, { CommitteeMembers::MAX_MEMBERS }>,
}

impl CommitteeMembers {
    pub const MAX_MEMBERS: usize = 512;

    pub fn new(members: MaxSizeVec<PublicKey, { Self::MAX_MEMBERS }>) -> Self {
        Self { members }
    }
}

impl ConsensusEncoding for CommitteeMembers {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.members.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for CommitteeMembers {}

impl ConsensusDecoding for CommitteeMembers {
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            members: ConsensusDecoding::consensus_decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::*;
    use crate::consensus::{check_consensus_encoding_correctness, ToConsensusBytes};

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = CommitteeMembers::new(
            vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                .try_into()
                .unwrap(),
        );
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = CommitteeMembers::default();
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_for_more_than_max_members() {
        let v = vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS + 1];
        let encoded = v.to_consensus_bytes();
        CommitteeMembers::consensus_decode(&mut encoded.as_slice()).unwrap_err();
    }
}
