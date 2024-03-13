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
use crate::{
    consensus::{MaxSizeBytes, MaxSizeString},
    transactions::transaction_components::{
        range_proof_type::RangeProofType,
        side_chain::SideChainFeature,
        BuildInfo,
        CodeTemplateRegistration,
        ConfidentialOutputData,
        OutputType,
        TemplateType,
        ValidatorNodeRegistration,
        ValidatorNodeSignature,
    },
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
    /// Additional data for coinbase transactions. This field MUST be empty if the output is not a coinbase
    /// transaction. This is enforced in [AggregatedBody::check_output_features].
    ///
    /// For coinbase outputs, the maximum length of this field is determined by the consensus constant,
    /// `coinbase_output_features_metadata_max_length`.
    pub coinbase_extra: Vec<u8>,
    /// Features that are specific to a side chain
    pub sidechain_feature: Option<SideChainFeature>,
    /// The type of range proof used in the output
    pub range_proof_type: RangeProofType,
}

impl OutputFeatures {
    pub fn new(
        version: OutputFeaturesVersion,
        output_type: OutputType,
        maturity: u64,
        coinbase_extra: Vec<u8>,
        sidechain_feature: Option<SideChainFeature>,
        range_proof_type: RangeProofType,
    ) -> OutputFeatures {
        OutputFeatures {
            version,
            output_type,
            maturity,
            coinbase_extra,
            sidechain_feature,
            range_proof_type,
        }
    }

    pub fn new_current_version(
        flags: OutputType,
        maturity: u64,
        coinbase_extra: Vec<u8>,
        sidechain_feature: Option<SideChainFeature>,
        range_proof_type: RangeProofType,
    ) -> OutputFeatures {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            coinbase_extra,
            sidechain_feature,
            range_proof_type,
        )
    }

    pub fn create_coinbase(
        maturity_height: u64,
        extra: Option<Vec<u8>>,
        range_proof_type: RangeProofType,
    ) -> OutputFeatures {
        let coinbase_extra = extra.unwrap_or_default();
        OutputFeatures {
            output_type: OutputType::Coinbase,
            maturity: maturity_height,
            coinbase_extra,
            range_proof_type,
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

    /// creates output features for a burned output with confidential output data
    pub fn create_burn_confidential_output(
        claim_public_key: PublicKey,
        network: Option<PublicKey>,
        network_knowledge_proof: Option<Signature>,
    ) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::Burn,
            sidechain_feature: Some(SideChainFeature::ConfidentialOutput(ConfidentialOutputData {
                claim_public_key,
                network,
                network_knowledge_proof,
            })),
            ..Default::default()
        }
    }

    /// Creates template registration output features
    pub fn for_template_registration(template_registration: CodeTemplateRegistration) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::CodeTemplateRegistration,
            sidechain_feature: Some(SideChainFeature::CodeTemplateRegistration(template_registration)),
            ..Default::default()
        }
    }

    pub fn for_validator_node_registration(
        public_key: PublicKey,
        signature: Signature,
        claim_public_key: PublicKey,
        network: Option<PublicKey>,
        network_knowledge_proof: Option<Signature>,
    ) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::ValidatorNodeRegistration,
            sidechain_feature: Some(SideChainFeature::ValidatorNodeRegistration(
                ValidatorNodeRegistration::new(
                    ValidatorNodeSignature::new(public_key, signature),
                    claim_public_key,
                    network,
                    network_knowledge_proof,
                ),
            )),
            ..Default::default()
        }
    }

    pub fn for_code_template_registration(
        author_public_key: PublicKey,
        author_signature: Signature,
        template_name: MaxSizeString<32>,
        template_version: u16,
        template_type: TemplateType,
        build_info: BuildInfo,
        binary_sha: MaxSizeBytes<32>,
        binary_url: MaxSizeString<255>,
        network: Option<PublicKey>,
        network_knowledge_proof: Option<Signature>,
    ) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::CodeTemplateRegistration,
            sidechain_feature: Some(SideChainFeature::CodeTemplateRegistration(CodeTemplateRegistration {
                author_public_key,
                author_signature,
                template_name,
                template_version,
                template_type,
                build_info,
                binary_sha,
                binary_url,
                network,
                network_knowledge_proof,
            })),
            ..Default::default()
        }
    }

    pub fn validator_node_registration(&self) -> Option<&ValidatorNodeRegistration> {
        self.sidechain_feature
            .as_ref()
            .and_then(|s| s.validator_node_registration())
    }

    pub fn code_template_registration(&self) -> Option<&CodeTemplateRegistration> {
        self.sidechain_feature
            .as_ref()
            .and_then(|s| s.code_template_registration())
    }

    pub fn confidential_output_data(&self) -> Option<&ConfidentialOutputData> {
        self.sidechain_feature
            .as_ref()
            .and_then(|s| s.confidential_output_data())
    }

    pub fn is_coinbase(&self) -> bool {
        matches!(self.output_type, OutputType::Coinbase)
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures::new_current_version(OutputType::default(), 0, vec![], None, RangeProofType::default())
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
