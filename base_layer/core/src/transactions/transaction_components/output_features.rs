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
    use std::{convert::TryInto, io::ErrorKind, iter};

    use tari_common_types::types::Signature;

    use super::*;
    use crate::{
        consensus::check_consensus_encoding_correctness,
        transactions::transaction_components::{
            bytes_into_fixed_string,
            side_chain::{
                CheckpointParameters,
                CommitteeMembers,
                ConstitutionChangeFlags,
                ConstitutionChangeRules,
                ContractAcceptanceRequirements,
                RequirementsForConstitutionChange,
                SideChainConsensus,
            },
            CommitteeSignatures,
            ContractAcceptance,
            ContractAmendment,
            ContractConstitution,
            ContractDefinition,
            ContractSpecification,
            ContractUpdateProposal,
            ContractUpdateProposalAcceptance,
            FunctionRef,
            PublicFunction,
            SignerSignature,
        },
    };

    #[allow(clippy::too_many_lines)]
    fn make_fully_populated_output_features(version: OutputFeaturesVersion) -> OutputFeatures {
        let constitution = ContractConstitution {
            validator_committee: vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                .try_into()
                .unwrap(),
            acceptance_requirements: ContractAcceptanceRequirements {
                acceptance_period_expiry: 100,
                minimum_quorum_required: 5,
            },
            consensus: SideChainConsensus::MerkleRoot,
            checkpoint_params: CheckpointParameters {
                minimum_quorum_required: 5,
                abandoned_interval: 100,
                quarantine_interval: 100,
            },
            constitution_change_rules: ConstitutionChangeRules {
                change_flags: ConstitutionChangeFlags::all(),
                requirements_for_constitution_change: Some(RequirementsForConstitutionChange {
                    minimum_constitution_committee_signatures: 5,
                    constitution_committee: Some(
                        vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                            .try_into()
                            .unwrap(),
                    ),
                    backup_keys: Some(
                        vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                            .try_into()
                            .unwrap(),
                    ),
                }),
            },
        };

        OutputFeatures {
            version,
            output_type: OutputType::ContractDefinition,
            maturity: u64::MAX,
            metadata: vec![1; 1024],
            unique_id: Some(vec![0u8; 256]),
            sidechain_features: Some(Box::new(SideChainFeatures {
                contract_id: FixedHash::zero(),
                constitution: Some(constitution.clone()),
                definition: Some(ContractDefinition {
                    contract_name: bytes_into_fixed_string("name"),
                    contract_issuer: PublicKey::default(),
                    contract_spec: ContractSpecification {
                        runtime: bytes_into_fixed_string("runtime"),
                        public_functions: vec![
                            PublicFunction {
                                name: bytes_into_fixed_string("foo"),
                                function: FunctionRef {
                                    template_id: FixedHash::zero(),
                                    function_id: 0_u16,
                                },
                            },
                            PublicFunction {
                                name: bytes_into_fixed_string("bar"),
                                function: FunctionRef {
                                    template_id: FixedHash::zero(),
                                    function_id: 1_u16,
                                },
                            },
                        ],
                    },
                }),
                acceptance: Some(ContractAcceptance {
                    validator_node_public_key: PublicKey::default(),
                    signature: Signature::default(),
                }),
                update_proposal: Some(ContractUpdateProposal {
                    proposal_id: 0_u64,
                    signature: Signature::default(),
                    updated_constitution: constitution.clone(),
                }),
                update_proposal_acceptance: Some(ContractUpdateProposalAcceptance {
                    proposal_id: 0_u64,
                    validator_node_public_key: PublicKey::default(),
                    signature: Signature::default(),
                }),
                amendment: Some(ContractAmendment {
                    proposal_id: 0_u64,
                    validator_committee: vec![PublicKey::default(); CommitteeMembers::MAX_MEMBERS]
                        .try_into()
                        .unwrap(),
                    validator_signatures: vec![SignerSignature::default(); CommitteeSignatures::MAX_SIGNATURES]
                        .try_into()
                        .unwrap(),
                    updated_constitution: constitution,
                    activation_window: 0_u64,
                }),
                checkpoint: Some(ContractCheckpoint {
                    checkpoint_number: u64::MAX,
                    merkle_root: FixedHash::zero(),
                    signatures: vec![SignerSignature::default(); 512].try_into().unwrap(),
                }),
            })),
            // Deprecated
            parent_public_key: Some(PublicKey::default()),
            asset: Some(AssetOutputFeatures {
                public_key: Default::default(),
                template_ids_implemented: vec![1u32; 50],
                template_parameters: iter::repeat_with(|| TemplateParameter {
                    template_id: 0,
                    template_data_version: 0,
                    template_data: vec![],
                })
                .take(50)
                .collect(),
            }),
            mint_non_fungible: Some(MintNonFungibleFeatures {
                asset_public_key: Default::default(),
                asset_owner_commitment: Default::default(),
            }),
            sidechain_checkpoint: Some(SideChainCheckpointFeatures {
                merkle_root: [1u8; 32].into(),
                committee: iter::repeat_with(PublicKey::default).take(50).collect(),
            }),
            committee_definition: match version {
                OutputFeaturesVersion::V0 => None,
                _ => Some(CommitteeDefinitionFeatures {
                    committee: iter::repeat_with(PublicKey::default).take(50).collect(),
                    effective_sidechain_height: u64::MAX,
                }),
            },
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
        subject.unique_id = None;
        subject.sidechain_features = None;
        subject.asset = None;
        subject.mint_non_fungible = None;
        subject.sidechain_checkpoint = None;
        subject.committee_definition = None;
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_for_large_metadata() {
        let mut subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        subject.metadata = vec![1u8; 1025];
        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn it_fails_for_large_unique_id() {
        let mut subject = make_fully_populated_output_features(OutputFeaturesVersion::V1);
        subject.unique_id = Some(vec![0u8; 257]);

        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn test_for_asset_registration() {
        let metadata = vec![1, 2, 3, 4];
        let template_ids_implemented = vec![1, 1, 2, 3];
        let tp = TemplateParameter {
            template_id: 2,
            template_data_version: 3,
            template_data: vec![3, 2, 1],
        };
        assert_eq!(
            OutputFeatures {
                output_type: OutputType::AssetRegistration,
                maturity: 0,
                metadata: metadata.clone(),
                asset: Some(AssetOutputFeatures {
                    public_key: PublicKey::default(),
                    template_ids_implemented: template_ids_implemented.clone(),
                    template_parameters: vec![tp.clone()]
                }),
                unique_id: Some(PublicKey::default().as_bytes().to_vec()),
                ..Default::default()
            },
            OutputFeatures::for_asset_registration(metadata, PublicKey::default(), template_ids_implemented, vec![tp])
        );
    }

    #[test]
    fn test_for_minting() {
        let metadata = vec![1, 2, 3, 4];
        let template_ids_implemented = vec![1, 1, 2, 3];
        let tp = TemplateParameter {
            template_id: 2,
            template_data_version: 3,
            template_data: vec![3, 2, 1],
        };
        let other_features =
            OutputFeatures::for_asset_registration(metadata, PublicKey::default(), template_ids_implemented, vec![tp]);
        let unique_id = vec![7, 2, 3, 4];
        assert_eq!(
            OutputFeatures {
                output_type: OutputType::MintNonFungible,
                mint_non_fungible: Some(MintNonFungibleFeatures {
                    asset_public_key: PublicKey::default(),
                    asset_owner_commitment: Commitment::from_public_key(&PublicKey::default())
                }),
                parent_public_key: Some(PublicKey::default()),
                unique_id: Some(unique_id.clone()),
                ..other_features.clone()
            },
            OutputFeatures::for_minting(
                PublicKey::default(),
                Commitment::from_public_key(&PublicKey::default()),
                unique_id,
                Some(other_features)
            )
        );
    }

    #[test]
    fn test_for_checkpoint() {
        let contract_id = FixedHash::hash_bytes("CONTRACT");
        let hash = FixedHash::hash_bytes("MERKLE");
        let checkpoint = ContractCheckpoint {
            checkpoint_number: 123,
            merkle_root: hash,
            signatures: vec![SignerSignature::default()].try_into().unwrap(),
        };

        let features = OutputFeatures::for_contract_checkpoint(contract_id, checkpoint.clone());
        let sidechain_features = features.sidechain_features.as_ref().unwrap();
        assert_eq!(features.output_type, OutputType::ContractCheckpoint);
        assert_eq!(sidechain_features.contract_id, contract_id);
        assert_eq!(*sidechain_features.checkpoint.as_ref().unwrap(), checkpoint);
    }

    #[test]
    fn test_for_committee() {
        let unique_id = vec![7, 2, 3, 4];
        let committee = vec![PublicKey::default()];
        let effective_sidechain_height = 123;
        assert_eq!(
            OutputFeatures {
                output_type: OutputType::CommitteeInitialDefinition,
                committee_definition: Some(CommitteeDefinitionFeatures {
                    committee: committee.clone(),
                    effective_sidechain_height
                }),
                parent_public_key: Some(PublicKey::default()),
                unique_id: Some(unique_id.clone()),
                ..Default::default()
            },
            OutputFeatures::for_committee(
                PublicKey::default(),
                unique_id.clone(),
                committee.clone(),
                effective_sidechain_height,
                true
            )
        );
        assert_eq!(
            OutputFeatures {
                output_type: OutputType::CommitteeDefinition,
                committee_definition: Some(CommitteeDefinitionFeatures {
                    committee: committee.clone(),
                    effective_sidechain_height
                }),
                parent_public_key: Some(PublicKey::default()),
                unique_id: Some(unique_id.clone()),
                ..Default::default()
            },
            OutputFeatures::for_committee(
                PublicKey::default(),
                unique_id,
                committee,
                effective_sidechain_height,
                false
            )
        );
    }
}
