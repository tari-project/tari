//  Copyright 2021. The Tari Project
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

use integer_encoding::VarInt;
use serde::{Deserialize, Serialize};
use tari_common_types::types::PublicKey;
use tari_crypto::keys::PublicKey as PublicKeyTrait;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeVec};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct CommitteeDefinitionFeatures {
    pub committee: Vec<PublicKey>,
    pub effective_sidechain_height: u64,
}

impl ConsensusEncoding for CommitteeDefinitionFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, Error> {
        let mut written = self.committee.consensus_encode(writer)?;
        written += self.effective_sidechain_height.consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusEncodingSized for CommitteeDefinitionFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        self.committee.len().required_space() +
            self.committee.len() * PublicKey::key_length() +
            self.effective_sidechain_height.required_space()
    }
}

impl ConsensusDecoding for CommitteeDefinitionFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        const MAX_COMMITTEE_KEYS: usize = 50;
        let committee = MaxSizeVec::<PublicKey, MAX_COMMITTEE_KEYS>::consensus_decode(reader)?;
        let effective_sidechain_height = u64::consensus_decode(reader)?;

        Ok(Self {
            committee: committee.into(),
            effective_sidechain_height,
        })
    }
}

#[cfg(test)]
mod test {
    use std::{io::ErrorKind, iter};

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = CommitteeDefinitionFeatures {
            committee: iter::repeat_with(PublicKey::default).take(50).collect(),
            effective_sidechain_height: 123,
        };

        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_for_too_many_committee_pks() {
        let subject = CommitteeDefinitionFeatures {
            committee: iter::repeat_with(PublicKey::default).take(51).collect(),
            effective_sidechain_height: 321,
        };

        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
}
