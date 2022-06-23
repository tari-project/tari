//  Copyright 2022, The Tari Project
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
    transactions::transaction_components::CommitteeSignatures,
};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct ContractCheckpoint {
    pub merkle_root: FixedHash,
    pub signatures: CommitteeSignatures,
}

impl ConsensusEncoding for ContractCheckpoint {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.merkle_root.consensus_encode(writer)?;
        self.signatures.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for ContractCheckpoint {}

impl ConsensusDecoding for ContractCheckpoint {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            merkle_root: ConsensusDecoding::consensus_decode(reader)?,
            signatures: ConsensusDecoding::consensus_decode(reader)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use tari_common_types::types::Signature;

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = ContractCheckpoint {
            merkle_root: FixedHash::zero(),
            signatures: vec![Signature::default(); 512].try_into().unwrap(),
        };
        check_consensus_encoding_correctness(subject).unwrap();
    }
}
