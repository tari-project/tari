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

use std::{
    io,
    io::{Read, Write},
};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, PublicKey};
use tari_crypto::keys::PublicKey as PublicKeyTrait;

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct MintNonFungibleFeatures {
    pub asset_public_key: PublicKey,
    pub asset_owner_commitment: Commitment,
    // pub proof_of_ownership: ComSignature
}

impl ConsensusEncoding for MintNonFungibleFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = self.asset_public_key.consensus_encode(writer)?;
        written += self.asset_owner_commitment.consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusEncodingSized for MintNonFungibleFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        PublicKey::key_length() * 2
    }
}

impl ConsensusDecoding for MintNonFungibleFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let asset_public_key = PublicKey::consensus_decode(reader)?;
        let asset_owner_commitment = Commitment::consensus_decode(reader)?;
        Ok(Self {
            asset_public_key,
            asset_owner_commitment,
        })
    }
}
