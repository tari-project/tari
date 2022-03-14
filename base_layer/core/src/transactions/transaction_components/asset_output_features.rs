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
use tari_common_types::types::PublicKey;

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeVec},
    transactions::transaction_components::TemplateParameter,
};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct AssetOutputFeatures {
    pub public_key: PublicKey,
    // TODO: remove in favour of template args
    pub template_ids_implemented: Vec<u32>,
    pub template_parameters: Vec<TemplateParameter>,
}

impl ConsensusEncoding for AssetOutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = self.public_key.consensus_encode(writer)?;
        written += self.template_ids_implemented.consensus_encode(writer)?;
        written += self.template_parameters.consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusEncodingSized for AssetOutputFeatures {}

impl ConsensusDecoding for AssetOutputFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let public_key = PublicKey::consensus_decode(reader)?;
        const MAX_TEMPLATES: usize = 50;
        let template_ids_implemented = MaxSizeVec::<u32, MAX_TEMPLATES>::consensus_decode(reader)?;

        const MAX_TEMPLATE_PARAMS: usize = 50;
        let template_parameters = MaxSizeVec::<TemplateParameter, MAX_TEMPLATE_PARAMS>::consensus_decode(reader)?;

        Ok(Self {
            public_key,
            template_ids_implemented: template_ids_implemented.into(),
            template_parameters: template_parameters.into(),
        })
    }
}
