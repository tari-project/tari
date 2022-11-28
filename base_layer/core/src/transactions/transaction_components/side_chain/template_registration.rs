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

use std::io::{Error, ErrorKind, Read, Write};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{PublicKey, Signature};

use crate::consensus::{
    read_byte,
    ConsensusDecoding,
    ConsensusEncoding,
    ConsensusEncodingSized,
    MaxSizeBytes,
    MaxSizeString,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct CodeTemplateRegistration {
    pub author_public_key: PublicKey,
    pub author_signature: Signature,
    pub template_name: MaxSizeString<32>,
    pub template_version: u16,
    pub template_type: TemplateType,
    pub build_info: BuildInfo,
    pub binary_sha: MaxSizeBytes<32>,
    pub binary_url: MaxSizeString<255>,
}

impl ConsensusEncoding for CodeTemplateRegistration {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.author_public_key.consensus_encode(writer)?;
        self.author_signature.consensus_encode(writer)?;
        self.template_name.consensus_encode(writer)?;
        self.template_version.consensus_encode(writer)?;
        self.template_type.consensus_encode(writer)?;
        self.build_info.consensus_encode(writer)?;
        self.binary_sha.consensus_encode(writer)?;
        self.binary_url.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for CodeTemplateRegistration {}

impl ConsensusDecoding for CodeTemplateRegistration {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let author_public_key = PublicKey::consensus_decode(reader)?;
        let author_signature = Signature::consensus_decode(reader)?;
        let template_name = MaxSizeString::consensus_decode(reader)?;
        let template_version = u16::consensus_decode(reader)?;
        let template_type = TemplateType::consensus_decode(reader)?;
        let build_info = BuildInfo::consensus_decode(reader)?;
        let binary_sha = MaxSizeBytes::consensus_decode(reader)?;
        let binary_url = MaxSizeString::consensus_decode(reader)?;

        Ok(CodeTemplateRegistration {
            author_public_key,
            author_signature,
            template_name,
            template_version,
            template_type,
            build_info,
            binary_sha,
            binary_url,
        })
    }
}

// -------------------------------- TemplateType -------------------------------- //

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub enum TemplateType {
    /// Indicates that the template is a WASM module
    Wasm { abi_version: u16 },
}

impl TemplateType {
    fn as_type_byte(&self) -> u8 {
        match self {
            TemplateType::Wasm { .. } => 0,
        }
    }
}

impl ConsensusEncoding for TemplateType {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        writer.write_all(&[self.as_type_byte()])?;
        match self {
            TemplateType::Wasm { abi_version } => {
                abi_version.consensus_encode(writer)?;
            },
        }

        Ok(())
    }
}

impl ConsensusEncodingSized for TemplateType {}

impl ConsensusDecoding for TemplateType {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let type_byte = read_byte(reader)?;
        match type_byte {
            0 => {
                let abi_version = u16::consensus_decode(reader)?;
                Ok(TemplateType::Wasm { abi_version })
            },
            _ => Err(Error::new(ErrorKind::InvalidData, "Invalid template type")),
        }
    }
}

// -------------------------------- BuildInfo -------------------------------- //

#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct BuildInfo {
    pub repo_url: MaxSizeString<255>,
    pub commit_hash: MaxSizeBytes<32>,
}

impl ConsensusEncoding for BuildInfo {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        self.repo_url.consensus_encode(writer)?;
        self.commit_hash.consensus_encode(writer)?;
        Ok(())
    }
}

impl ConsensusEncodingSized for BuildInfo {}

impl ConsensusDecoding for BuildInfo {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        let repo_url = MaxSizeString::consensus_decode(reader)?;
        let commit_hash = MaxSizeBytes::consensus_decode(reader)?;
        Ok(Self { repo_url, commit_hash })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = CodeTemplateRegistration {
            author_public_key: Default::default(),
            author_signature: Default::default(),
            template_name: "🐢 all the way down".try_into().unwrap(),
            template_version: 0xff,
            template_type: TemplateType::Wasm { abi_version: 0xffff },
            build_info: BuildInfo {
                repo_url: "https://github.com/tari-project/wasm_template.git".try_into().unwrap(),
                commit_hash: Default::default(),
            },
            binary_sha: Default::default(),
            binary_url: "/dns4/github.com/tcp/443/http/tari-project/wasm_examples/releases/download/v0.0.6/coin.zip"
                .try_into()
                .unwrap(),
        };

        check_consensus_encoding_correctness(subject).unwrap();
    }
}
