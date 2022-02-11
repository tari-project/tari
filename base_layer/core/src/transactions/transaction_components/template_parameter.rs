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

use integer_encoding::{VarIntReader, VarIntWriter};
use serde::{Deserialize, Serialize};

use crate::consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct TemplateParameter {
    pub template_id: u32,
    pub template_data_version: u32,
    pub template_data: Vec<u8>,
}

impl ConsensusEncoding for TemplateParameter {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = writer.write_varint(self.template_id)?;
        written += writer.write_varint(self.template_data_version)?;
        written += self.template_data.consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusEncodingSized for TemplateParameter {}

impl ConsensusDecoding for TemplateParameter {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let template_id = reader.read_varint()?;
        let template_data_version = reader.read_varint()?;
        const MAX_TEMPLATE_DATA_LEN: usize = 1024;
        let template_data = MaxSizeBytes::<MAX_TEMPLATE_DATA_LEN>::consensus_decode(reader)?;

        Ok(Self {
            template_id,
            template_data_version,
            template_data: template_data.into(),
        })
    }
}

#[cfg(test)]
mod test {
    use std::io::ErrorKind;

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let params = TemplateParameter {
            template_id: u32::MAX,
            template_data_version: u32::MAX,
            template_data: vec![1u8; 1024],
        };

        check_consensus_encoding_correctness(params).unwrap();
    }

    #[test]
    fn it_fails_for_large_template_data_vec() {
        let params = TemplateParameter {
            template_id: 123,
            template_data_version: 1,
            template_data: vec![1u8; 1025],
        };

        let err = check_consensus_encoding_correctness(params).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
}
