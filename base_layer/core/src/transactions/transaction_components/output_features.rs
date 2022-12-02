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
};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{PublicKey, Signature};

use super::OutputFeaturesVersion;
use crate::transactions::transaction_components::{
    side_chain::SideChainFeature,
    CodeTemplateRegistration,
    OutputType,
    ValidatorNodeRegistration,
    ValidatorNodeSignature,
};

/// Options for UTXO's
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq, BorshSerialize, BorshDeserialize)]
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
    pub fn new(
        version: OutputFeaturesVersion,
        output_type: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_feature: Option<SideChainFeature>,
    ) -> OutputFeatures {
        OutputFeatures {
            version,
            output_type,
            maturity,
            metadata,
            sidechain_feature,
        }
    }

    pub fn new_current_version(
        flags: OutputType,
        maturity: u64,
        metadata: Vec<u8>,
        sidechain_feature: Option<SideChainFeature>,
    ) -> OutputFeatures {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            metadata,
            sidechain_feature,
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
            sidechain_feature: Some(SideChainFeature::TemplateRegistration(template_registration)),
            ..Default::default()
        }
    }

    pub fn for_validator_node_registration(
        validator_node_public_key: PublicKey,
        validator_node_signature: Signature,
    ) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::ValidatorNodeRegistration,
            sidechain_feature: Some(SideChainFeature::ValidatorNodeRegistration(
                ValidatorNodeRegistration::new(ValidatorNodeSignature::new(
                    validator_node_public_key,
                    validator_node_signature,
                )),
            )),
            ..Default::default()
        }
    }

    pub fn validator_node_registration(&self) -> Option<&ValidatorNodeRegistration> {
        self.sidechain_feature
            .as_ref()
            .and_then(|s| s.validator_node_registration())
    }

    pub fn is_coinbase(&self) -> bool {
        matches!(self.output_type, OutputType::Coinbase)
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
