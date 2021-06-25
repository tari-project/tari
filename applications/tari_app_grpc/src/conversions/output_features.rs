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

use crate::tari_rpc as grpc;
use std::convert::TryFrom;
use tari_core::transactions::transaction::{OutputFeatures, OutputFlags, AssetOutputFeatures, MintNonFungibleFeatures};
use tari_core::transactions::types::{PublicKey, Commitment};
use tari_crypto::tari_utilities::ByteArray;
use std::convert::TryInto;

impl TryFrom<grpc::OutputFeatures> for OutputFeatures {
    type Error = String;

    fn try_from(features: grpc::OutputFeatures) -> Result<Self, Self::Error> {
        Ok(Self {
            flags: OutputFlags::from_bits(features.flags as u8)
                .ok_or_else(|| "Invalid or unrecognised output flags".to_string())?,
            maturity: features.maturity,
            metadata: features.metadata,
            asset: features.asset.map(|a| a.try_into()).transpose()?,
            mint_non_fungible: features.mint_non_fungible.map(|m| m.try_into()).transpose()?
        })
    }
}

impl From<OutputFeatures> for grpc::OutputFeatures {
    fn from(features: OutputFeatures) -> Self {
        Self {
            flags: features.flags.bits() as u32,
            maturity: features.maturity,
            metadata: features.metadata,
            asset: features.asset.map(|a| a.into()),
            mint_non_fungible: features.mint_non_fungible.map(|m| m.into())
        }
    }
}

impl TryFrom<grpc::AssetOutputFeatures> for AssetOutputFeatures {
    type Error = String;

    fn try_from(features: grpc::AssetOutputFeatures) -> Result<Self, Self::Error> {
        let public_key = PublicKey::from_bytes(features.public_key.as_bytes()).map_err(|err| format!("{:?}", err))?;

        Ok(Self {
            public_key
        })
    }
}

impl From<AssetOutputFeatures> for grpc::AssetOutputFeatures {
    fn from(features: AssetOutputFeatures) -> Self {
        Self{
            public_key: features.public_key.as_bytes().to_vec()
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


