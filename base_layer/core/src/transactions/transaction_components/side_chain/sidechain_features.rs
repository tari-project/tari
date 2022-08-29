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

use crate::{
    consensus::{read_byte, write_byte, ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized},
    transactions::transaction_components::CodeTemplateRegistration,
};

#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub enum SideChainFeatures {
    TemplateRegistration(CodeTemplateRegistration),
}
impl SideChainFeatures {
    pub fn as_byte(&self) -> u8 {
        #[allow(clippy::enum_glob_use)]
        use SideChainFeatures::*;
        match self {
            TemplateRegistration(_) => 0x01,
        }
    }
}

impl ConsensusEncoding for SideChainFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        #[allow(clippy::enum_glob_use)]
        use SideChainFeatures::*;
        write_byte(writer, self.as_byte())?;
        match self {
            TemplateRegistration(template_registration) => {
                template_registration.consensus_encode(writer)?;
            },
        }
        Ok(())
    }
}

impl ConsensusEncodingSized for SideChainFeatures {}

impl ConsensusDecoding for SideChainFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, Error> {
        #[allow(clippy::enum_glob_use)]
        use SideChainFeatures::*;
        let byte = read_byte(reader)?;
        match byte {
            0x01 => Ok(TemplateRegistration(ConsensusDecoding::consensus_decode(reader)?)),
            _ => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid SideChainFeatures byte '{}'", byte),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use tari_utilities::hex::from_hex;

    use super::*;
    use crate::{
        consensus::{check_consensus_encoding_correctness, MaxSizeString},
        transactions::transaction_components::{BuildInfo, TemplateType},
    };

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = SideChainFeatures::TemplateRegistration(CodeTemplateRegistration {
            author_public_key: Default::default(),
            author_signature: Default::default(),
            template_name: MaxSizeString::from_str_checked("🚀🚀🚀🚀🚀🚀🚀🚀").unwrap(),
            template_version: 1,
            template_type: TemplateType::Wasm { abi_version: 123 },
            build_info: BuildInfo {
                repo_url: "/dns/github.com/https/tari_project/wasm_examples".try_into().unwrap(),
                commit_hash: from_hex("ea29c9f92973fb7eda913902ff6173c62cb1e5df")
                    .unwrap()
                    .try_into()
                    .unwrap(),
            },
            binary_sha: from_hex("c93747637517e3de90839637f0ce1ab7c8a3800b")
                .unwrap()
                .try_into()
                .unwrap(),
            binary_url: "/dns4/github.com/https/tari_project/wasm_examples/releases/download/v0.0.6/coin.zip"
                .try_into()
                .unwrap(),
        });

        check_consensus_encoding_correctness(subject).unwrap();
    }
}
