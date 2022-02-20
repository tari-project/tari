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
use tari_common_types::types::{Commitment, FixedHash, PublicKey};
use tari_utilities::ByteArray;

use super::OutputFeaturesVersion;
use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized, MaxSizeBytes},
    transactions::transaction_components::{
        AssetOutputFeatures,
        CommitteeDefinitionFeatures,
        MintNonFungibleFeatures,
        OutputFlags,
        SideChainCheckpointFeatures,
        TemplateParameter,
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
            metadata,
            unique_id,
            parent_public_key,
            asset,
            mint_non_fungible,
            sidechain_checkpoint,
            committee_definition,
        )
    }

    pub fn create_coinbase(maturity_height: u64) -> OutputFeatures {
        OutputFeatures {
            flags: OutputFlags::COINBASE_OUTPUT,
            maturity: maturity_height,
            ..Default::default()
        }
    }

    /// Create an `OutputFeatures` with the given maturity and all other values at their default setting
    pub fn with_maturity(maturity: u64) -> OutputFeatures {
        OutputFeatures {
            maturity,
            ..Default::default()
        }
    }

    pub fn custom(flags: OutputFlags, metadata: Vec<u8>) -> OutputFeatures {
        Self {
            flags,
            metadata,
            ..Default::default()
        }
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
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = self.version.consensus_encode(writer)?;
        written += self.maturity.consensus_encode(writer)?;
        written += self.flags.consensus_encode(writer)?;
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
        let version = OutputFeaturesVersion::consensus_decode(reader)?;
        // Decode safety: consensus_decode will stop reading the varint after 10 bytes
        let maturity = u64::consensus_decode(reader)?;
        let flags = OutputFlags::consensus_decode(reader)?;
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
        OutputFeatures::new_current_version(OutputFlags::empty(), 0, vec![], None, None, None, None, None, None)
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
            self.flags, self.maturity
        )
    }
}

#[cfg(test)]
mod test {
    use std::{io::ErrorKind, iter};

    use super::*;
    use crate::consensus::check_consensus_encoding_correctness;

    fn make_fully_populated_output_features() -> OutputFeatures {
        OutputFeatures {
            version: OutputFeaturesVersion::V1,
            flags: OutputFlags::all(),
            maturity: u64::MAX,
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
            committee_definition: Some(CommitteeDefinitionFeatures {
                committee: iter::repeat_with(PublicKey::default).take(50).collect(),
                effective_sidechain_height: u64::MAX,
            }),
        }
    }

    #[test]
    fn it_encodes_and_decodes_correctly() {
        // v0 committee_definition decodes to None
        let mut subject = make_fully_populated_output_features();
        subject.version = OutputFeaturesVersion::V0;
        subject.committee_definition = None;
        check_consensus_encoding_correctness(subject).unwrap();

        let subject = make_fully_populated_output_features();
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_encodes_and_decodes_correctly_in_none_case() {
        let mut subject = make_fully_populated_output_features();
        subject.unique_id = None;
        subject.asset = None;
        subject.mint_non_fungible = None;
        subject.sidechain_checkpoint = None;
        subject.committee_definition = None;
        check_consensus_encoding_correctness(subject).unwrap();
    }

    #[test]
    fn it_fails_for_large_metadata() {
        let mut subject = make_fully_populated_output_features();
        subject.metadata = vec![1u8; 1025];
        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn it_fails_for_large_unique_id() {
        let mut subject = make_fully_populated_output_features();
        subject.unique_id = Some(vec![0u8; 257]);

        let err = check_consensus_encoding_correctness(subject).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
}
