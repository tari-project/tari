// Copyright 2020. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use tari_common_types::{
    array::copy_into_fixed_array,
    types::{Commitment, PublicKey},
};
use tari_core::transactions::transaction_components::{
    AssetOutputFeatures,
    CommitteeDefinitionFeatures,
    MintNonFungibleFeatures,
    OutputFeatures,
    OutputFeaturesVersion,
    OutputFlags,
    SideChainCheckpointFeatures,
    TemplateParameter,
};
use tari_crypto::tari_utilities::ByteArray;

use crate::tari_rpc as grpc;

impl TryFrom<grpc::OutputFeatures> for OutputFeatures {
    type Error = String;

    fn try_from(features: grpc::OutputFeatures) -> Result<Self, Self::Error> {
        let unique_id = if features.unique_id.is_empty() {
            None
        } else {
            Some(features.unique_id.clone())
        };
        let parent_public_key = if features.parent_public_key.is_empty() {
            None
        } else {
            Some(PublicKey::from_bytes(features.parent_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?)
        };

        Ok(OutputFeatures::new(
            OutputFeaturesVersion::try_from(
                u8::try_from(features.version).map_err(|_| "Invalid version: overflowed u8")?,
            )?,
            OutputFlags::from_bits(features.flags as u8)
                .ok_or_else(|| "Invalid or unrecognised output flags".to_string())?,
            features.maturity,
            features.metadata,
            unique_id,
            parent_public_key,
            features.asset.map(|a| a.try_into()).transpose()?,
            features.mint_non_fungible.map(|m| m.try_into()).transpose()?,
            features.sidechain_checkpoint.map(|s| s.try_into()).transpose()?,
            features.committee_definition.map(|c| c.try_into()).transpose()?,
        ))
    }
}

impl From<OutputFeatures> for grpc::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            flags: features.flags.bits() as u32,
            maturity: features.maturity,
            metadata: features.metadata,
            unique_id: features.unique_id.unwrap_or_default(),
            parent_public_key: features
                .parent_public_key
                .map(|a| a.as_bytes().to_vec())
                .unwrap_or_default(),
            asset: features.asset.map(|a| a.into()),
            mint_non_fungible: features.mint_non_fungible.map(|m| m.into()),
            sidechain_checkpoint: features.sidechain_checkpoint.map(|m| m.into()),
            version: features.version as u32,
            committee_definition: features.committee_definition.map(|c| c.into()),
        }
    }
}

impl TryFrom<grpc::AssetOutputFeatures> for AssetOutputFeatures {
    type Error = String;

    fn try_from(features: grpc::AssetOutputFeatures) -> Result<Self, Self::Error> {
        let public_key = PublicKey::from_bytes(features.public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            public_key,
            template_ids_implemented: features.template_ids_implemented,
            template_parameters: features.template_parameters.into_iter().map(|tp| tp.into()).collect(),
        })
    }
}

impl From<AssetOutputFeatures> for grpc::AssetOutputFeatures {
    fn from(features: AssetOutputFeatures) -> Self {
        Self {
            public_key: features.public_key.as_bytes().to_vec(),
            template_ids_implemented: features.template_ids_implemented,
            template_parameters: features.template_parameters.into_iter().map(|tp| tp.into()).collect(),
        }
    }
}

impl From<grpc::TemplateParameter> for TemplateParameter {
    fn from(source: grpc::TemplateParameter) -> Self {
        Self {
            template_id: source.template_id,
            template_data_version: source.template_data_version,
            template_data: source.template_data,
        }
    }
}

impl From<TemplateParameter> for grpc::TemplateParameter {
    fn from(source: TemplateParameter) -> Self {
        Self {
            template_id: source.template_id,
            template_data_version: source.template_data_version,
            template_data: source.template_data,
        }
    }
}
impl TryFrom<grpc::MintNonFungibleFeatures> for MintNonFungibleFeatures {
    type Error = String;

    fn try_from(value: grpc::MintNonFungibleFeatures) -> Result<Self, Self::Error> {
        let asset_public_key =
            PublicKey::from_bytes(value.asset_public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        let asset_owner_commitment =
            Commitment::from_bytes(&value.asset_owner_commitment).map_err(|err| err.to_string())?;

        Ok(Self {
            asset_public_key,
            asset_owner_commitment,
        })
    }
}

impl From<MintNonFungibleFeatures> for grpc::MintNonFungibleFeatures {
    fn from(value: MintNonFungibleFeatures) -> Self {
        Self {
            asset_public_key: value.asset_public_key.as_bytes().to_vec(),
            asset_owner_commitment: value.asset_owner_commitment.to_vec(),
        }
    }
}

impl From<SideChainCheckpointFeatures> for grpc::SideChainCheckpointFeatures {
    fn from(value: SideChainCheckpointFeatures) -> Self {
        Self {
            merkle_root: value.merkle_root.as_bytes().to_vec(),
            committee: value.committee.iter().map(|c| c.as_bytes().to_vec()).collect(),
        }
    }
}

impl TryFrom<grpc::SideChainCheckpointFeatures> for SideChainCheckpointFeatures {
    type Error = String;

    fn try_from(value: grpc::SideChainCheckpointFeatures) -> Result<Self, Self::Error> {
        let committee = value
            .committee
            .iter()
            .map(|c| {
                PublicKey::from_bytes(c).map_err(|err| format!("committee member was not a valid public key: {}", err))
            })
            .collect::<Result<_, _>>()?;
        let merkle_root = copy_into_fixed_array(&value.merkle_root).map_err(|_| "Invalid merkle_root length")?;

        Ok(Self { merkle_root, committee })
    }
}

impl From<CommitteeDefinitionFeatures> for grpc::CommitteeDefinitionFeatures {
    fn from(value: CommitteeDefinitionFeatures) -> Self {
        Self {
            committee: value.committee.iter().map(|c| c.as_bytes().to_vec()).collect(),
            effective_sidechain_height: value.effective_sidechain_height,
        }
    }
}

impl TryFrom<grpc::CommitteeDefinitionFeatures> for CommitteeDefinitionFeatures {
    type Error = String;

    fn try_from(value: grpc::CommitteeDefinitionFeatures) -> Result<Self, Self::Error> {
        let committee = value
            .committee
            .iter()
            .map(|c| {
                PublicKey::from_bytes(c).map_err(|err| format!("committee member was not a valid public key: {}", err))
            })
            .collect::<Result<_, _>>()?;
        let effective_sidechain_height = value.effective_sidechain_height;

        Ok(Self {
            committee,
            effective_sidechain_height,
        })
    }
}
