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
    convert::{TryFrom, TryInto},
    io,
    slice,
};

use serde::{Deserialize, Serialize};

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeVec},
    transactions::transaction_components::{SignerSignature, TransactionError},
};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq, Default)]
pub struct CommitteeSignatures {
    signatures: MaxSizeVec<SignerSignature, { CommitteeSignatures::MAX_SIGNATURES }>,
}

impl CommitteeSignatures {
    pub const MAX_SIGNATURES: usize = 512;

    pub fn new(signatures: MaxSizeVec<SignerSignature, { Self::MAX_SIGNATURES }>) -> Self {
        Self { signatures }
    }

    pub fn empty() -> Self {
        Self {
            // Panic: vec is size 0 < 512
            signatures: vec![].try_into().unwrap(),
        }
    }

    pub fn signatures(&self) -> &[SignerSignature] {
        &self.signatures
    }

    #[must_use = "resulting bool must be checked to ensure that the item was added"]
    pub fn push<T: Into<SignerSignature>>(&mut self, signature: T) -> bool {
        self.signatures.push(signature.into())
    }
}

impl IntoIterator for CommitteeSignatures {
    type IntoIter = <Vec<SignerSignature> as IntoIterator>::IntoIter;
    type Item = SignerSignature;

    fn into_iter(self) -> Self::IntoIter {
        self.signatures.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a CommitteeSignatures {
    type IntoIter = slice::Iter<'a, SignerSignature>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.signatures.iter()
    }
}

impl TryFrom<Vec<SignerSignature>> for CommitteeSignatures {
    type Error = TransactionError;

    fn try_from(signatures: Vec<SignerSignature>) -> Result<Self, Self::Error> {
        let len = signatures.len();
        let signatures = signatures
            .try_into()
            .map_err(|_| TransactionError::InvalidCommitteeLength {
                len,
                max: Self::MAX_SIGNATURES,
            })?;
        Ok(Self { signatures })
    }
}

impl ConsensusEncoding for CommitteeSignatures {
    fn consensus_encode<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.signatures.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for CommitteeSignatures {}

impl ConsensusDecoding for CommitteeSignatures {
    fn consensus_decode<R: io::Read>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            signatures: ConsensusDecoding::consensus_decode(reader)?,
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
        let subject = CommitteeSignatures::new(
            vec![SignerSignature::default(); CommitteeSignatures::MAX_SIGNATURES]
                .try_into()
                .unwrap(),
        );
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = CommitteeSignatures::default();
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_for_more_than_max_signatures() {
        let v = vec![SignerSignature::default(); CommitteeSignatures::MAX_SIGNATURES + 1];
        let encoded = v.to_consensus_bytes();
        CommitteeSignatures::consensus_decode(&mut encoded.as_slice()).unwrap_err();
    }
}
