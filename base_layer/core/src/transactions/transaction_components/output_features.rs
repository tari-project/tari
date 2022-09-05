// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    cmp::Ordering,
    fmt,
    fmt::{Display, Formatter},
    io,
    io::{Read, Write},
};

use serde::{Deserialize, Serialize};
use tari_common_types::types::{PublicKey, Signature};

use super::OutputFeaturesVersion;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes},
    transactions::transaction_components::{side_chain::SideChainFeatures, CodeTemplateRegistration, OutputType},
};

/// Options for UTXO's
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct OutputFeatures {
    pub version: OutputFeaturesVersion,
    /// Flags are the feature flags that differentiate between outputs, eg Coinbase all of which has different rules
    pub output_type: OutputType,
    /// the maturity of the specific UTXO. This is the min lock height at which an UTXO can be spent. Coinbase UTXO
    /// require a min maturity of the Coinbase_lock_height, this should be checked on receiving new blocks.
    pub maturity: u64,
    pub metadata: Vec<u8>,
    pub sidechain_features: Option<SideChainFeatures>,
    pub validator_node_public_key: Option<PublicKey>,
    pub validator_node_signature: Option<Signature>,
}

impl OutputFeatures {
    pub fn new(
        version: OutputFeaturesVersion,
        output_type: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_features: Option<SideChainFeatures>,
        validator_node_public_key: Option<PublicKey>,
        validator_node_signature: Option<Signature>,
    ) -> OutputFeatures {
        OutputFeatures {
            version,
            output_type,
            maturity,
            metadata,
            sidechain_features,
            validator_node_public_key,
            validator_node_signature,
        }
    }

    pub fn new_current_version(
        flags: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_features: Option<SideChainFeatures>,
        validator_node_public_key: Option<PublicKey>,
        validator_node_signature: Option<Signature>,
    ) -> OutputFeatures {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            metadata,
            sidechain_features,
            validator_node_public_key,
            validator_node_signature,
        )
    }

    pub fn create_coinbase(maturity_height: u64) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::Coinbase,
            maturity: maturity_height,
            ..Default::default()
        }
    }

    /// creates output features for a burned output
    pub fn create_burn_output() -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::Burn,
            ..Default::default()
        }
    }

    /// Creates template registration output features
    pub fn for_template_registration(template_registration: CodeTemplateRegistration) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::CodeTemplateRegistration,
            sidechain_features: Some(SideChainFeatures::TemplateRegistration(template_registration)),
            ..Default::default()
        }
    }

    pub fn create_validator_node_registration(
        validator_node_public_key: PublicKey,
        validator_node_signature: Signature,
    ) -> OutputFeatures {
        OutputFeatures {
            validator_node_public_key: Some(validator_node_public_key),
            validator_node_signature: Some(validator_node_signature),
            ..Default::default()
        }
    }

    pub fn is_coinbase(&self) -> bool {
        matches!(self.output_type, OutputType::Coinbase)
    }
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.version.consensus_encode(writer)?;
        self.maturity.consensus_encode(writer)?;
        self.output_type.consensus_encode(writer)?;
        self.sidechain_features.consensus_encode(writer)?;
        self.metadata.consensus_encode(writer)?;

        Ok(())
    }
}

impl ConsensusEncodingSized for OutputFeatures {}

impl ConsensusDecoding for OutputFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        // Changing the order of these operations is consensus breaking
        // Decode safety: consensus_decode will stop reading the varint after 10 bytes
        let version = OutputFeaturesVersion::consensus_decode(reader)?;
        let maturity = u64::consensus_decode(reader)?;
        let flags = OutputType::consensus_decode(reader)?;
        let sidechain_features = ConsensusDecoding::consensus_decode(reader)?;
        const MAX_METADATA_SIZE: usize = 1024;
        let metadata = <MaxSizeBytes<MAX_METADATA_SIZE> as ConsensusDecoding>::consensus_decode(reader)?;
        let validator_node_public_key = None;
        let validator_node_signature = None;
        Ok(Self {
            version,
            output_type: flags,
            maturity,
            sidechain_features,
            metadata: metadata.into(),
            validator_node_public_key,
            validator_node_signature,
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures::new_current_version(OutputType::default(), 0, vec![], None, None, None)
    }
}

impl PartialOrd for OutputFeatures {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OutputFeatures {
    fn cmp(&self, other: &Self) -> Ordering {
        self.maturity.cmp(&other.maturity)
    }
}

impl Display for OutputFeatures {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OutputFeatures: Flags = {:?}, Maturity = {}",
            self.output_type, self.maturity
        )
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;

    use tari_utilities::hex::from_hex;

    use super::*;
    use crate::{
        consensus::{check_consensus_encoding_correctness, MaxSizeString},
        transactions::transaction_components::{BuildInfo, TemplateType},
    };

    fn make_fully_populated_output_features(version: OutputFeaturesVersion) -> OutputFeatures {
        OutputFeatures {
            version,
            output_type: OutputType::Standard,
            maturity: u64::MAX,
            metadata: vec![1; 1024],
            sidechain_features: Some(SideChainFeatures::TemplateRegistration(CodeTemplateRegistration {
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
            })),
            validator_node_public_key: None,
            validator_node_signature: None,
        }
    }

    #[test]
    fn it_encodes_and_decodes_correctly() {
        let subject = make_fully_populated_output_features(OutputFeaturesVersion::V0);
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_encodes_and_decodes_correctly_in_none_case() {
        let mut subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        subject.sidechain_features = None;
        check_consensus_encoding_correctness(subject).unwrap();
    }
}
