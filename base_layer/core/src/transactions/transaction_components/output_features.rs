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
use tari_common_types::types::{Commitment, FixedHash, PublicKey};
use tari_crypto::ristretto::pedersen::PedersenCommitment;
use tari_utilities::ByteArray;

use super::OutputFeaturesVersion;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes},
    transactions::{
        transaction_components::{
            AssetOutputFeatures,
            CommitteeDefinitionFeatures,
            MintNonFungibleFeatures,
            OutputFlags,
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
    pub flags: OutputFlags,
    /// the maturity of the specific UTXO. This is the min lock height at which an UTXO can be spent. Coinbase UTXO
    /// require a min maturity of the Coinbase_lock_height, this should be checked on receiving new blocks.
    pub maturity: u64,
    /// The recovery byte - not consensus critical - can help reduce the bandwidth with wallet recovery or in other
    /// instances when a wallet needs to request the complete UTXO set from a base node.
    #[serde(default)]
    pub recovery_byte: u8,
    pub metadata: Vec<u8>,
    pub unique_id: Option<Vec<u8>>,
    pub parent_public_key: Option<PublicKey>,
    pub asset: Option<AssetOutputFeatures>,
    pub mint_non_fungible: Option<MintNonFungibleFeatures>,
    pub sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
    pub committee_definition: Option<CommitteeDefinitionFeatures>,
}

impl OutputFeatures {
    pub fn new(
        version: OutputFeaturesVersion,
        flags: OutputFlags,
        maturity: u64,
        recovery_byte: u8,
        metadata: Vec<u8>,
        unique_id: Option<Vec<u8>>,
        parent_public_key: Option<PublicKey>,
        asset: Option<AssetOutputFeatures>,
        mint_non_fungible: Option<MintNonFungibleFeatures>,
        sidechain_checkpoint: Option<SideChainCheckpointFeatures>,
        committee_definition: Option<CommitteeDefinitionFeatures>,
    ) -> OutputFeatures {
        OutputFeatures {
            version,
            flags,
            maturity,
            recovery_byte,
            metadata,
            unique_id,
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        }
    }

    pub fn new_current_version(
        flags: OutputFlags,
        maturity: u64,
        recovery_byte: u8,
        metadata: Vec<u8>,
        unique_id: Option<Vec<u8>>,
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
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        )
    }

    pub fn create_coinbase(maturity_height: u64, recovery_byte: u8) -> OutputFeatures {
        OutputFeatures {
            flags: OutputFlags::COINBASE_OUTPUT,
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
            flags: OutputFlags::ASSET_REGISTRATION,
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
            flags: OutputFlags::MINT_NON_FUNGIBLE |
                other_features
                    .as_ref()
                    .map(|of| of.flags)
                    .unwrap_or_else(OutputFlags::empty),
            mint_non_fungible: Some(MintNonFungibleFeatures {
                asset_public_key: asset_public_key.clone(),
                asset_owner_commitment,
            }),
            parent_public_key: Some(asset_public_key),
            unique_id: Some(unique_id),
            ..other_features.unwrap_or_default()
        }
    }

    pub fn for_checkpoint(
        parent_public_key: PublicKey,
        unique_id: Vec<u8>,
        merkle_root: FixedHash,
        committee: Vec<PublicKey>,
        is_initial: bool,
    ) -> OutputFeatures {
        Self {
            flags: if is_initial {
                OutputFlags::SIDECHAIN_CHECKPOINT | OutputFlags::MINT_NON_FUNGIBLE
            } else {
                OutputFlags::SIDECHAIN_CHECKPOINT
            },
            sidechain_checkpoint: Some(SideChainCheckpointFeatures { merkle_root, committee }),
            parent_public_key: Some(parent_public_key),
            unique_id: Some(unique_id),
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
            flags: if is_initial {
                OutputFlags::COMMITTEE_DEFINITION | OutputFlags::MINT_NON_FUNGIBLE
            } else {
                OutputFlags::COMMITTEE_DEFINITION
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

    pub fn unique_asset_id(&self) -> Option<&[u8]> {
        self.unique_id.as_deref()
    }

    pub fn is_non_fungible_mint(&self) -> bool {
        self.flags.contains(OutputFlags::MINT_NON_FUNGIBLE)
    }

    pub fn is_non_fungible_burn(&self) -> bool {
        self.flags.contains(OutputFlags::BURN_NON_FUNGIBLE)
    }

    pub fn is_coinbase(&self) -> bool {
        self.flags.contains(OutputFlags::COINBASE_OUTPUT)
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
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = self.version.consensus_encode(writer)?;
        written += self.maturity.consensus_encode(writer)?;
        written += self.flags.consensus_encode(writer)?;
        match self.version {
            OutputFeaturesVersion::V0 => (),
            OutputFeaturesVersion::V1 => {
                written += OutputFeatures::consensus_encode_recovery_byte(self.recovery_byte, writer)?;
            },
        }
        written += self.parent_public_key.consensus_encode(writer)?;
        written += self.unique_id.consensus_encode(writer)?;
        written += self.asset.consensus_encode(writer)?;
        written += self.mint_non_fungible.consensus_encode(writer)?;
        written += self.sidechain_checkpoint.consensus_encode(writer)?;
        written += self.metadata.consensus_encode(writer)?;
        match self.version {
            OutputFeaturesVersion::V0 => (),
            OutputFeaturesVersion::V1 => {
                written += self.committee_definition.consensus_encode(writer)?;
            },
        }
        Ok(written)
    }
}

impl ConsensusEncodingSized for OutputFeatures {}

impl ConsensusDecoding for OutputFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        // Changing the order of these operations is consensus breaking
        // Decode safety: consensus_decode will stop reading the varint after 10 bytes
        let version = OutputFeaturesVersion::consensus_decode(reader)?;
        let maturity = u64::consensus_decode(reader)?;
        let flags = OutputFlags::consensus_decode(reader)?;
        let recovery_byte = match version {
            OutputFeaturesVersion::V0 => 0,
            OutputFeaturesVersion::V1 => OutputFeatures::consensus_decode_recovery_byte(reader)?,
        };
        let parent_public_key = <Option<PublicKey> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_UNIQUE_ID_SIZE: usize = 256;
        let unique_id = <Option<MaxSizeBytes<MAX_UNIQUE_ID_SIZE>> as ConsensusDecoding>::consensus_decode(reader)?;
        let asset = <Option<AssetOutputFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let mint_non_fungible = <Option<MintNonFungibleFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        let sidechain_checkpoint =
            <Option<SideChainCheckpointFeatures> as ConsensusDecoding>::consensus_decode(reader)?;
        const MAX_METADATA_SIZE: usize = 1024;
        let metadata = <MaxSizeBytes<MAX_METADATA_SIZE> as ConsensusDecoding>::consensus_decode(reader)?;
        let committee_definition = match version {
            OutputFeaturesVersion::V0 => None,
            OutputFeaturesVersion::V1 => {
                <Option<CommitteeDefinitionFeatures> as ConsensusDecoding>::consensus_decode(reader)?
            },
        };
        Ok(Self {
            version,
            flags,
            maturity,
            recovery_byte,
            parent_public_key,
            unique_id: unique_id.map(Into::into),
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
        OutputFeatures::new_current_version(OutputFlags::empty(), 0, 0, vec![], None, None, None, None, None, None)
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
            self.flags, self.maturity, self.recovery_byte
        )
    }
}

#[cfg(test)]
mod test {
    use std::{io::ErrorKind, iter};

    use tari_common_types::types::BLOCK_HASH_LENGTH;

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    fn make_fully_populated_output_features(version: OutputFeaturesVersion) -> OutputFeatures {
        OutputFeatures {
            version,
            flags: OutputFlags::all(),
            maturity: u64::MAX,
            recovery_byte: match version {
                OutputFeaturesVersion::V0 => 0,
                OutputFeaturesVersion::V1 => u8::MAX,
            },
            metadata: vec![1; 1024],
            unique_id: Some(vec![0u8; 256]),
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
                merkle_root: [1u8; 32],
                committee: iter::repeat_with(PublicKey::default).take(50).collect(),
            }),
            committee_definition: match version {
                OutputFeaturesVersion::V0 => None,
                OutputFeaturesVersion::V1 => Some(CommitteeDefinitionFeatures {
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
                flags: OutputFlags::ASSET_REGISTRATION,
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
                flags: OutputFlags::MINT_NON_FUNGIBLE | other_features.flags,
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
        let unique_id = vec![7, 2, 3, 4];
        let hash = [13; BLOCK_HASH_LENGTH];
        let committee = vec![PublicKey::default()];
        // Initial
        assert_eq!(
            OutputFeatures {
                flags: OutputFlags::SIDECHAIN_CHECKPOINT | OutputFlags::MINT_NON_FUNGIBLE,
                sidechain_checkpoint: Some(SideChainCheckpointFeatures {
                    merkle_root: hash,
                    committee: committee.clone()
                }),
                parent_public_key: Some(PublicKey::default()),
                unique_id: Some(unique_id.clone()),
                ..Default::default()
            },
            OutputFeatures::for_checkpoint(PublicKey::default(), unique_id.clone(), hash, committee.clone(), true)
        );

        // Not initial
        assert_eq!(
            OutputFeatures {
                flags: OutputFlags::SIDECHAIN_CHECKPOINT,
                sidechain_checkpoint: Some(SideChainCheckpointFeatures {
                    merkle_root: hash,
                    committee: committee.clone()
                }),
                parent_public_key: Some(PublicKey::default()),
                unique_id: Some(unique_id.clone()),
                ..Default::default()
            },
            OutputFeatures::for_checkpoint(PublicKey::default(), unique_id, hash, committee, false)
        );
    }

    #[test]
    fn test_for_committee() {
        let unique_id = vec![7, 2, 3, 4];
        let committee = vec![PublicKey::default()];
        let effective_sidechain_height = 123;
        assert_eq!(
            OutputFeatures {
                flags: OutputFlags::COMMITTEE_DEFINITION | OutputFlags::MINT_NON_FUNGIBLE,
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
                flags: OutputFlags::COMMITTEE_DEFINITION,
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
