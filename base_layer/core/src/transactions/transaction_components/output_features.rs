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

use super::OutputFeaturesVersion;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes},
    transactions::transaction_components::{side_chain::SideChainFeatures, OutputType},
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
    pub sidechain_features: Option<Box<SideChainFeatures>>,
}

impl OutputFeatures {
    pub fn new(
        version: OutputFeaturesVersion,
        flags: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_features: Option<SideChainFeatures>,
    ) -> OutputFeatures {
        let boxed_sidechain_features = sidechain_features.map(Box::new);
        OutputFeatures {
            version,
            output_type: flags,
            maturity,
            metadata,
            sidechain_features: boxed_sidechain_features,
        }
    }

    pub fn new_current_version(
        flags: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_features: Option<SideChainFeatures>,
    ) -> OutputFeatures {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            metadata,
            sidechain_features,
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
        let sidechain_features = <Option<Box<SideChainFeatures>> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_METADATA_SIZE: usize = 1024;
        let metadata = <MaxSizeBytes<MAX_METADATA_SIZE> as ConsensusDecoding>::consensus_decode(reader)?;
        Ok(Self {
            version,
            output_type: flags,
            maturity,
            sidechain_features,
            metadata: metadata.into(),
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures::new_current_version(OutputType::default(), 0, vec![], None)
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
    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    #[allow(clippy::too_many_lines)]
    fn make_fully_populated_output_features(version: OutputFeaturesVersion) -> OutputFeatures {
        OutputFeatures {
            version,
            output_type: OutputType::Standard,
            maturity: u64::MAX,
            metadata: vec![1; 1024],
            sidechain_features: Some(Box::new(SideChainFeatures {})),
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
