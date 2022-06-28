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

use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, FixedHash, PublicKey, Signature};
use tari_crypto::ristretto::pedersen::PedersenCommitment;
use tari_utilities::ByteArray;

use super::{
    ContractAcceptance,
    ContractAmendment,
    ContractConstitution,
    ContractDefinition,
    ContractUpdateProposal,
    ContractUpdateProposalAcceptance,
    OutputFeaturesVersion,
    SideChainFeaturesBuilder,
};
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes},
    transactions::{
        transaction_components::{
            side_chain::SideChainFeatures,
            AssetOutputFeatures,
            CommitteeDefinitionFeatures,
            CommitteeMembers,
            ContractCheckpoint,
            MintNonFungibleFeatures,
            OutputType,
            SideChainCheckpointFeatures,
            TemplateParameter,
        },
        transaction_protocol::RewindData,
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
    /// The recovery byte - not consensus critical - can help reduce the bandwidth with wallet recovery or in other
    /// instances when a wallet needs to request the complete UTXO set from a base node.
    #[serde(default)]
    pub recovery_byte: u8,
    pub metadata: Vec<u8>,
    pub sidechain_features: Option<SideChainFeatures>,
    pub unique_id: Option<Vec<u8>>,

    // TODO: Deprecated
    pub parent_public_key: Option<PublicKey>,
    pub asset: Option<AssetOutputFeatures>,
    pub mint_non_fungible: Option<MintNonFungibleFeatures>,
    pub sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
    pub committee_definition: Option<CommitteeDefinitionFeatures>,
}

impl OutputFeatures {
    pub fn new(
        version: OutputFeaturesVersion,
        flags: OutputType,
        maturity: u64,
        recovery_byte: u8,
        metadata: Vec<u8>,
        unique_id: Option<Vec<u8>>,
        sidechain_features: Option<SideChainFeatures>,
        // TODO: Deprecated
        parent_public_key: Option<PublicKey>,
        asset: Option<AssetOutputFeatures>,
        mint_non_fungible: Option<MintNonFungibleFeatures>,
        sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
        committee_definition: Option<CommitteeDefinitionFeatures>,
    ) -> OutputFeatures {
        OutputFeatures {
            version,
            output_type: flags,
            maturity,
            recovery_byte,
            metadata,
            unique_id,
            sidechain_features,
            // Deprecated
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        }
    }

    pub fn new_current_version(
        flags: OutputType,
        maturity: u64,
        recovery_byte: u8,
        metadata: Vec<u8>,
        unique_id: Option<Vec<u8>>,
        sidechain_features: Option<SideChainFeatures>,
        // TODO: Deprecated
        parent_public_key: Option<PublicKey>,
        asset: Option<AssetOutputFeatures>,
        mint_non_fungible: Option<MintNonFungibleFeatures>,
        sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
        committee_definition: Option<CommitteeDefinitionFeatures>,
    ) -> OutputFeatures {
        OutputFeatures::new(
            OutputFeaturesVersion::get_current_version(),
            flags,
            maturity,
            recovery_byte,
            metadata,
            unique_id,
            sidechain_features,
            // TODO: Deprecated
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        )
    }

    pub fn create_coinbase(maturity_height: u64, recovery_byte: u8) -> OutputFeatures {
        OutputFeatures {
            output_type: OutputType::Coinbase,
            maturity: maturity_height,
            recovery_byte,
            ..Default::default()
        }
    }

    /// Helper function to create a unique recovery byte based on the commitment and a private recovery byte key,
    /// with the value '0' never obtainable as it is being reserved as a default value.
    pub fn create_unique_recovery_byte(commitment: &PedersenCommitment, rewind_data: Option<&RewindData>) -> u8 {
        let commitment_bytes = commitment.as_bytes();
        let recovery_key_bytes = if let Some(data) = rewind_data {
            data.recovery_byte_key.as_bytes()
        } else {
            &[]
        };
        const RECOVERY_BYTE_SIZE: usize = 1;
        let blake2_hasher = VarBlake2b::new(RECOVERY_BYTE_SIZE)
            .expect("Should be able to create blake2 hasher; will only panic if output size is 0 or greater than 64");
        let mut hash = [0u8; RECOVERY_BYTE_SIZE];
        blake2_hasher
            .chain(commitment_bytes)
            .chain(recovery_key_bytes)
            .chain(b"hash my recovery byte")
            .finalize_variable(|res| hash.copy_from_slice(res));
        hash[0]
    }

    /// Helper function to update the unique recovery byte
    pub fn update_recovery_byte(&mut self, commitment: &PedersenCommitment, rewind_data: Option<&RewindData>) {
        let recovery_byte = OutputFeatures::create_unique_recovery_byte(commitment, rewind_data);
        self.set_recovery_byte(recovery_byte);
    }

    /// Helper function to return features with updated unique recovery byte
    pub fn features_with_updated_recovery_byte(
        commitment: &PedersenCommitment,
        rewind_data: Option<&RewindData>,
        features: &OutputFeatures,
    ) -> OutputFeatures {
        let recovery_byte = OutputFeatures::create_unique_recovery_byte(commitment, rewind_data);
        let mut updated_features = features.clone();
        updated_features.set_recovery_byte(recovery_byte);
        updated_features
    }

    /// Provides the ability to update the recovery byte after the commitment has become known
    pub fn set_recovery_byte(&mut self, recovery_byte: u8) {
        self.recovery_byte = recovery_byte;
    }

    pub fn for_asset_registration(
        metadata: Vec<u8>,
        public_key: PublicKey,
        template_ids_implemented: Vec<u32>,
        template_parameters: Vec<TemplateParameter>,
    ) -> OutputFeatures {
        let unique_id = Some(public_key.as_bytes().to_vec());
        Self {
            output_type: OutputType::AssetRegistration,
            maturity: 0,
            metadata,
            asset: Some(AssetOutputFeatures {
                public_key,
                template_ids_implemented,
                template_parameters,
            }),
            unique_id,
            ..Default::default()
        }
    }

    pub fn for_minting(
        asset_public_key: PublicKey,
        asset_owner_commitment: Commitment,
        unique_id: Vec<u8>,
        other_features: Option<OutputFeatures>,
    ) -> OutputFeatures {
        Self {
            output_type: OutputType::MintNonFungible,
            mint_non_fungible: Some(MintNonFungibleFeatures {
                asset_public_key: asset_public_key.clone(),
                asset_owner_commitment,
            }),
            parent_public_key: Some(asset_public_key),
            unique_id: Some(unique_id),
            ..other_features.unwrap_or_default()
        }
    }

    pub fn for_contract_checkpoint(contract_id: FixedHash, checkpoint: ContractCheckpoint) -> OutputFeatures {
        let features = SideChainFeatures::builder(contract_id)
            .with_contract_checkpoint(checkpoint)
            .finish();

        Self {
            output_type: OutputType::ContractCheckpoint,
            sidechain_features: Some(features),
            ..Default::default()
        }
    }

    pub fn for_committee(
        parent_public_key: PublicKey,
        unique_id: Vec<u8>,
        committee: Vec<PublicKey>,
        effective_sidechain_height: u64,
        is_initial: bool,
    ) -> OutputFeatures {
        Self {
            output_type: if is_initial {
                OutputType::CommitteeInitialDefinition
            } else {
                OutputType::CommitteeDefinition
            },
            committee_definition: Some(CommitteeDefinitionFeatures {
                committee,
                effective_sidechain_height,
            }),
            parent_public_key: Some(parent_public_key),
            unique_id: Some(unique_id),
            ..Default::default()
        }
    }

    pub fn for_contract_definition(commitment: &Commitment, definition: ContractDefinition) -> OutputFeatures {
        let contract_id = definition.calculate_contract_id(commitment);

        Self {
            output_type: OutputType::ContractDefinition,
            sidechain_features: Some(
                SideChainFeaturesBuilder::new(contract_id)
                    .with_contract_definition(definition)
                    .finish(),
            ),
            ..Default::default()
        }
    }

    pub fn for_contract_constitution(contract_id: FixedHash, constitution: ContractConstitution) -> OutputFeatures {
        Self {
            output_type: OutputType::ContractConstitution,
            sidechain_features: Some(
                SideChainFeaturesBuilder::new(contract_id)
                    .with_contract_constitution(constitution)
                    .finish(),
            ),
            ..Default::default()
        }
    }

    pub fn for_contract_acceptance(
        contract_id: FixedHash,
        validator_node_public_key: PublicKey,
        signature: Signature,
    ) -> OutputFeatures {
        Self {
            output_type: OutputType::ContractValidatorAcceptance,
            sidechain_features: Some(
                SideChainFeatures::builder(contract_id)
                    .with_contract_acceptance(ContractAcceptance {
                        validator_node_public_key,
                        signature,
                    })
                    .finish(),
            ),
            ..Default::default()
        }
    }

    pub fn for_contract_update_proposal_acceptance(
        contract_id: FixedHash,
        proposal_id: u64,
        validator_node_public_key: PublicKey,
        signature: Signature,
    ) -> OutputFeatures {
        Self {
            output_type: OutputType::ContractConstitutionChangeAcceptance,
            sidechain_features: Some(
                SideChainFeatures::builder(contract_id)
                    .with_contract_update_proposal_acceptance(ContractUpdateProposalAcceptance {
                        proposal_id,
                        validator_node_public_key,
                        signature,
                    })
                    .finish(),
            ),
            ..Default::default()
        }
    }

    pub fn for_contract_update_proposal(
        contract_id: FixedHash,
        update_proposal: ContractUpdateProposal,
    ) -> OutputFeatures {
        Self {
            output_type: OutputType::ContractConstitutionProposal,
            sidechain_features: Some(
                SideChainFeaturesBuilder::new(contract_id)
                    .with_update_proposal(update_proposal)
                    .finish(),
            ),
            ..Default::default()
        }
    }

    pub fn for_contract_amendment(contract_id: FixedHash, amendment: ContractAmendment) -> OutputFeatures {
        Self {
            output_type: OutputType::ContractAmendment,
            sidechain_features: Some(
                SideChainFeaturesBuilder::new(contract_id)
                    .with_contract_amendment(amendment)
                    .finish(),
            ),
            ..Default::default()
        }
    }

    pub fn unique_asset_id(&self) -> Option<&[u8]> {
        self.unique_id.as_deref()
    }

    pub fn is_non_fungible_mint(&self) -> bool {
        matches!(self.output_type, OutputType::MintNonFungible)
    }

    pub fn is_non_fungible_burn(&self) -> bool {
        matches!(self.output_type, OutputType::BurnNonFungible)
    }

    pub fn is_coinbase(&self) -> bool {
        matches!(self.output_type, OutputType::Coinbase)
    }

    fn consensus_encode_recovery_byte<W: Write>(recovery_byte: u8, writer: &mut W) -> Result<usize, io::Error> {
        writer.write_all(&[recovery_byte])?;
        Ok(1)
    }

    fn consensus_decode_recovery_byte<R: Read>(reader: &mut R) -> Result<u8, io::Error> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let recovery_byte = buf[0] as u8;
        Ok(recovery_byte)
    }

    pub fn contract_id(&self) -> Option<FixedHash> {
        self.sidechain_features.as_ref().map(|f| f.contract_id)
    }

    pub fn is_sidechain_contract(&self) -> bool {
        self.sidechain_features.is_some()
    }

    pub fn constitution_committee(&self) -> Option<&CommitteeMembers> {
        self.sidechain_features
            .as_ref()
            .and_then(|f| f.constitution.as_ref())
            .map(|c| &c.validator_committee)
    }
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        self.version.consensus_encode(writer)?;
        self.maturity.consensus_encode(writer)?;
        self.output_type.consensus_encode(writer)?;
        match self.version {
            OutputFeaturesVersion::V0 => (),
            _ => {
                OutputFeatures::consensus_encode_recovery_byte(self.recovery_byte, writer)?;
            },
        }
        self.parent_public_key.consensus_encode(writer)?;
        self.unique_id.consensus_encode(writer)?;
        self.sidechain_features.consensus_encode(writer)?;
        self.asset.consensus_encode(writer)?;
        self.mint_non_fungible.consensus_encode(writer)?;
        self.sidechain_checkpoint.consensus_encode(writer)?;
        self.metadata.consensus_encode(writer)?;
        match self.version {
            OutputFeaturesVersion::V0 => (),
            _ => {
                self.committee_definition.consensus_encode(writer)?;
            },
        }
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
        let recovery_byte = match version {
            OutputFeaturesVersion::V0 => 0,
            _ => OutputFeatures::consensus_decode_recovery_byte(reader)?,
        };
        let parent_public_key = <Option<PublicKey> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_UNIQUE_ID_SIZE: usize = 256;
        let unique_id = <Option<MaxSizeBytes<MAX_UNIQUE_ID_SIZE>> as ConsensusDecoding>::consensus_decode(reader)?;
        let sidechain_features = <Option<SideChainFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let asset = <Option<AssetOutputFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let mint_non_fungible = <Option<MintNonFungibleFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let sidechain_checkpoint =
            <Option<SideChainCheckpointFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_METADATA_SIZE: usize = 1024;
        let metadata = <MaxSizeBytes<MAX_METADATA_SIZE> as ConsensusDecoding>::consensus_decode(reader)?;
        let committee_definition = match version {
            OutputFeaturesVersion::V0 => None,
            _ => <Option<CommitteeDefinitionFeatures> as ConsensusDecoding>::consensus_decode(reader)?,
        };
        Ok(Self {
            version,
            output_type: flags,
            maturity,
            recovery_byte,
            parent_public_key,
            unique_id: unique_id.map(Into::into),
            sidechain_features,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            metadata: metadata.into(),
            committee_definition,
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures::new_current_version(
            OutputType::default(),
            0,
            0,
            vec![],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
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
            "OutputFeatures: Flags = {:?}, Maturity = {}, recovery byte = {:#08b}",
            self.output_type, self.maturity, self.recovery_byte
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
                }),
            },
            initial_reward: 100.into(),
        };

        OutputFeatures {
            version,
            output_type: OutputType::ContractDefinition,
            maturity: u64::MAX,
            recovery_byte: match version {
                OutputFeaturesVersion::V0 => 0,
                _ => u8::MAX,
            },
            metadata: vec![1; 1024],
            unique_id: Some(vec![0u8; 256]),
            sidechain_features: Some(SideChainFeatures {
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
                    validator_signatures: vec![Signature::default(); CommitteeSignatures::MAX_SIGNATURES]
                        .try_into()
                        .unwrap(),
                    updated_constitution: constitution,
                    activation_window: 0_u64,
                }),
                checkpoint: Some(ContractCheckpoint {
                    checkpoint_number: u64::MAX,
                    merkle_root: FixedHash::zero(),
                    signatures: vec![Signature::default(); 512].try_into().unwrap(),
                }),
            }),
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
            signatures: vec![Signature::default()].try_into().unwrap(),
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
