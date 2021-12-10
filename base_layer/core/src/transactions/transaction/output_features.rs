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

use integer_encoding::{VarInt, VarIntReader, VarIntWriter};
use serde::{Deserialize, Serialize};
use tari_common_types::types::{Commitment, PublicKey};
use tari_utilities::ByteArray;

use crate::{
    consensus::{ConsensusDecoding, ConsensusEncoding, ConsensusEncodingSized},
    transactions::transaction::{
        AssetOutputFeatures,
        MintNonFungibleFeatures,
        OutputFlags,
        SideChainCheckpointFeatures,
        TemplateParameter,
    },
};

/// Options for UTXO's
#[derive(Debug, Clone, Hash, PartialEq, Deserialize, Serialize, Eq)]
pub struct OutputFeatures {
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
}

impl OutputFeatures {
    /// The version number to use in consensus encoding. In future, this value could be dynamic.
    const CONSENSUS_ENCODING_VERSION: u8 = 0;

    /// Encodes output features using deprecated bincode encoding
    pub fn to_v1_bytes(&self) -> Vec<u8> {
        // unreachable panic: serialized_size is infallible because it uses DefaultOptions
        let encode_size = bincode::serialized_size(self).expect("unreachable");
        let mut buf = Vec::with_capacity(encode_size as usize);
        // unreachable panic: Vec's Write impl is infallible
        bincode::serialize_into(&mut buf, self).expect("unreachable");
        buf
    }

    /// Encodes output features using consensus encoding
    pub fn to_consensus_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.consensus_encode_exact_size());
        // unreachable panic: Vec's Write impl is infallible
        self.consensus_encode(&mut buf).expect("unreachable");
        buf
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
        merkle_root: Vec<u8>,
        committee: Vec<PublicKey>,
    ) -> OutputFeatures {
        const CHECKPOINT_UNIQUE_ID: [u8; 32] = [3u8; 32];
        Self {
            flags: OutputFlags::SIDECHAIN_CHECKPOINT,
            sidechain_checkpoint: Some(SideChainCheckpointFeatures { merkle_root, committee }),
            parent_public_key: Some(parent_public_key),
            unique_id: Some(CHECKPOINT_UNIQUE_ID.to_vec()),
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
}

impl ConsensusEncoding for OutputFeatures {
    fn consensus_encode<W: Write>(&self, writer: &mut W) -> Result<usize, io::Error> {
        let mut written = writer.write_varint(Self::CONSENSUS_ENCODING_VERSION)?;
        written += writer.write_varint(self.maturity)?;
        written += self.flags.consensus_encode(writer)?;
        Ok(written)
    }
}

impl ConsensusEncodingSized for OutputFeatures {
    fn consensus_encode_exact_size(&self) -> usize {
        Self::CONSENSUS_ENCODING_VERSION.required_space() +
            self.flags.consensus_encode_exact_size() +
            self.maturity.required_space()
    }
}

impl ConsensusDecoding for OutputFeatures {
    fn consensus_decode<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        // Changing the order of these operations is consensus breaking
        let version = reader.read_varint::<u8>()?;
        if version != Self::CONSENSUS_ENCODING_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "Invalid version. Expected {} but got {}",
                    Self::CONSENSUS_ENCODING_VERSION,
                    version
                ),
            ));
        }
        // Decode safety: read_varint will stop reading the varint after 10 bytes
        let maturity = reader.read_varint()?;
        let flags = OutputFlags::consensus_decode(reader)?;
        Ok(Self {
            flags,
            maturity,
            ..Default::default()
        })
    }
}

impl Default for OutputFeatures {
    fn default() -> Self {
        OutputFeatures {
            flags: OutputFlags::empty(),
            maturity: 0,
            metadata: vec![],
            unique_id: None,
            parent_public_key: None,
            asset: None,
            mint_non_fungible: None,
            sidechain_checkpoint: None,
        }
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
