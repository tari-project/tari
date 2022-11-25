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
    transactions::transaction_components::{
        side_chain::SideChainFeature,
        CodeTemplateRegistration,
        OutputType,
        TransactionError,
        ValidatorNodeRegistration,
    },
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
    pub sidechain_feature: Option<SideChainFeature>,
}

impl OutputFeatures {
    const MAX_METADATA_LENGTH: usize = 64;

    pub fn new(
        version: OutputFeaturesVersion,
        output_type: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_feature: Option<SideChainFeature>,
    ) -> Result<OutputFeatures, TransactionError> {
        let features = OutputFeatures {
            version,
            output_type,
            maturity,
            metadata,
            sidechain_feature,
        };

        features.validate()?;

        Ok(features)
    }

    pub fn new_current_version(
        flags: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_feature: Option<SideChainFeature>,
    ) -> Result<OutputFeatures, TransactionError> {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            metadata,
            sidechain_feature,
        )
    }

    pub fn create_coinbase(maturity_height: u64) -> Result<OutputFeatures, TransactionError> {
        let features = OutputFeatures {
            output_type: OutputType::Coinbase,
            maturity: maturity_height,
            ..Default::default()
        };

        features.validate()?;

        Ok(features)
    }

    /// creates output features for a burned output
    pub fn create_burn_output() -> Result<OutputFeatures, TransactionError> {
        let features = OutputFeatures {
            output_type: OutputType::Burn,
            ..Default::default()
        };

        features.validate()?;

        Ok(features)
    }

    /// Creates template registration output features
    pub fn for_template_registration(
        template_registration: CodeTemplateRegistration,
    ) -> Result<OutputFeatures, TransactionError> {
        let features = OutputFeatures {
            output_type: OutputType::CodeTemplateRegistration,
            sidechain_feature: Some(SideChainFeature::TemplateRegistration(template_registration)),
            ..Default::default()
        };

        features.validate()?;

        Ok(features)
    }

    pub fn for_validator_node_registration(
        validator_node_public_key: PublicKey,
        validator_node_signature: Signature,
    ) -> Result<OutputFeatures, TransactionError> {
        let features = OutputFeatures {
            output_type: OutputType::ValidatorNodeRegistration,
            sidechain_feature: Some(SideChainFeature::ValidatorNodeRegistration(ValidatorNodeRegistration {
                public_key: validator_node_public_key,
                signature: validator_node_signature,
            })),
            ..Default::default()
        };

        features.validate()?;

        Ok(features)
    }

    #[inline]
    pub fn validate(&self) -> Result<(), TransactionError> {
        // This field should be optional for coinbases (mining pools and
        // other merge mined coins can use it), but it should be empty for non-coinbases
        if self.output_type != OutputType::Coinbase && !self.metadata.is_empty() {
            return Err(TransactionError::NonCoinbaseHasMetadata);
        }

        // For coinbases, the maximum length should be 64 bytes (2x hashes), so that arbitrary data cannot be included
        if self.output_type == OutputType::Coinbase && self.metadata.len() > Self::MAX_METADATA_LENGTH {
            return Err(TransactionError::InvalidMetadataSize {
                len: self.metadata.len(),
                max: Self::MAX_METADATA_LENGTH,
            });
        }

        Ok(())
    }

    #[inline]
    pub fn is_coinbase(&self) -> bool {
        matches!(self.output_type, OutputType::Coinbase)
    }
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.version.consensus_encode(writer)?;
        self.maturity.consensus_encode(writer)?;
        self.output_type.consensus_encode(writer)?;
        self.sidechain_feature.consensus_encode(writer)?;
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
        let sidechain_feature = ConsensusDecoding::consensus_decode(reader)?;
        const MAX_METADATA_SIZE: usize = 1024;
        let metadata = <MaxSizeBytes<MAX_METADATA_SIZE> as ConsensusDecoding>::consensus_decode(reader)?;
        Ok(Self {
            version,
            output_type: flags,
            maturity,
            sidechain_feature,
            metadata: metadata.into(),
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures::new_current_version(OutputType::default(), 0, vec![], None).expect("default output features")
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
            sidechain_feature: Some(SideChainFeature::TemplateRegistration(CodeTemplateRegistration {
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
        subject.sidechain_feature = None;
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn test_output_features_metadata() {
        assert_eq!(Ok(()), OutputFeatures::default().validate());
        assert_eq!(Ok(()), OutputFeatures::create_burn_output().unwrap().validate());
        assert_eq!(Ok(()), OutputFeatures::create_coinbase(123).unwrap().validate());

        // ----------------------------------------------------------------------------
        // coinbase

        assert_eq!(
            Ok(()),
            OutputFeatures {
                output_type: OutputType::Coinbase,
                maturity: 123,
                metadata: vec![1; 64],
                ..Default::default()
            }
            .validate()
        );

        assert_eq!(
            Err(TransactionError::InvalidMetadataSize {
                len: 65,
                max: OutputFeatures::MAX_METADATA_LENGTH
            }),
            OutputFeatures {
                output_type: OutputType::Coinbase,
                maturity: 123,
                metadata: vec![1; 65],
                ..Default::default()
            }
            .validate()
        );

        // ----------------------------------------------------------------------------
        // non-coinbase

        assert_eq!(
            Err(TransactionError::NonCoinbaseHasMetadata),
            OutputFeatures {
                output_type: OutputType::Standard,
                maturity: 123,
                metadata: vec![1],
                ..Default::default()
            }
            .validate()
        );

        assert_eq!(
            Err(TransactionError::NonCoinbaseHasMetadata),
            OutputFeatures {
                output_type: OutputType::CodeTemplateRegistration,
                maturity: 123,
                metadata: vec![1],
                ..Default::default()
            }
            .validate()
        );

        assert_eq!(
            Err(TransactionError::NonCoinbaseHasMetadata),
            OutputFeatures {
                output_type: OutputType::ValidatorNodeRegistration,
                maturity: 123,
                metadata: vec![1],
                ..Default::default()
            }
            .validate()
        );

        assert_eq!(
            Err(TransactionError::NonCoinbaseHasMetadata),
            OutputFeatures {
                output_type: OutputType::Burn,
                maturity: 123,
                metadata: vec![1],
                ..Default::default()
            }
            .validate()
        );
    }
}
