// Copyright 2021. The Tari Project
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

use tari_common_types::{
    transaction::TxId,
    types::{Commitment, FixedHash, PublicKey},
};
use tari_core::transactions::transaction_components::{OutputFeatures, TemplateParameter, Transaction};
use tari_service_framework::{reply_channel::SenderService, Service};

use crate::{
    assets::{
        infrastructure::{AssetManagerRequest, AssetManagerResponse},
        Asset,
    },
    error::WalletError,
};

#[derive(Clone)]
pub struct AssetManagerHandle {
    handle: SenderService<AssetManagerRequest, Result<AssetManagerResponse, WalletError>>,
}

impl AssetManagerHandle {
    pub fn new(sender: SenderService<AssetManagerRequest, Result<AssetManagerResponse, WalletError>>) -> Self {
        Self { handle: sender }
    }

    pub async fn list_owned_assets(&mut self) -> Result<Vec<Asset>, WalletError> {
        match self.handle.call(AssetManagerRequest::ListOwned {}).await?? {
            AssetManagerResponse::ListOwned { assets } => Ok(assets),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "list_owned_assets".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }

    pub async fn get_owned_asset_by_pub_key(&mut self, public_key: &PublicKey) -> Result<Asset, WalletError> {
        match self
            .handle
            .call(AssetManagerRequest::GetOwnedAsset {
                public_key: public_key.clone(),
            })
            .await??
        {
            AssetManagerResponse::GetOwnedAsset { asset } => Ok(*asset),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "get_owned_asset_by_pub_key".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }

    pub async fn create_initial_asset_checkpoint(
        &mut self,
        asset_public_key: &PublicKey,
        merkle_root: FixedHash,
    ) -> Result<(TxId, Transaction), WalletError> {
        match self
            .handle
            .call(AssetManagerRequest::CreateInitialCheckpoint {
                asset_public_key: Box::new(asset_public_key.clone()),
                merkle_root,
                committee_public_keys: Vec::new(),
            })
            .await??
        {
            AssetManagerResponse::CreateInitialCheckpoint { transaction, tx_id } => Ok((tx_id, *transaction)),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "create_initial_asset_checkpoint".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }

    pub async fn create_follow_on_asset_checkpoint(
        &mut self,
        public_key: &PublicKey,
        unique_id: &[u8],
        merkle_root: FixedHash,
    ) -> Result<(TxId, Transaction), WalletError> {
        match self
            .handle
            .call(AssetManagerRequest::CreateFollowOnCheckpoint {
                asset_public_key: Box::new(public_key.clone()),
                merkle_root,
                unique_id: unique_id.to_vec(),
                committee_public_keys: Vec::new(),
            })
            .await??
        {
            AssetManagerResponse::CreateFollowOnCheckpoint { transaction, tx_id } => Ok((tx_id, *transaction)),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "create_follow_on_asset_checkpoint".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }

    pub async fn create_committee_definition(
        &mut self,
        public_key: &PublicKey,
        committee_public_keys: &[PublicKey],
        effective_sidechain_height: u64,
    ) -> Result<(TxId, Transaction), WalletError> {
        match self
            .handle
            .call(AssetManagerRequest::CreateCommitteeCheckpoint {
                asset_public_key: Box::new(public_key.clone()),
                committee_public_keys: committee_public_keys.to_vec(),
                effective_sidechain_height,
            })
            .await??
        {
            AssetManagerResponse::CreateCommitteeCheckpoint { transaction, tx_id } => Ok((tx_id, *transaction)),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "create_committee_definition".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }

    pub async fn create_registration_transaction(
        &mut self,
        name: String,
        public_key: PublicKey,
        template_ids_implemented: Vec<u32>,
        description: Option<String>,
        image: Option<String>,
        template_parameters: Vec<TemplateParameter>,
    ) -> Result<(TxId, Transaction), WalletError> {
        match self
            .handle
            .call(AssetManagerRequest::CreateRegistrationTransaction {
                name,
                public_key: Box::new(public_key),
                template_ids_implemented,
                description,
                image,
                template_parameters,
            })
            .await??
        {
            AssetManagerResponse::CreateRegistrationTransaction { transaction, tx_id } => Ok((tx_id, *transaction)),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "create_registration_transaction".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }

    pub async fn create_minting_transaction(
        &mut self,
        asset_public_key: &PublicKey,
        asset_owner_commitment: &Commitment,
        features: Vec<(Vec<u8>, Option<OutputFeatures>)>,
    ) -> Result<(TxId, Transaction), WalletError> {
        match self
            .handle
            .call(AssetManagerRequest::CreateMintingTransaction {
                asset_public_key: Box::new(asset_public_key.clone()),
                asset_owner_commitment: Box::new(asset_owner_commitment.clone()),
                features,
            })
            .await??
        {
            AssetManagerResponse::CreateMintingTransaction { transaction, tx_id } => Ok((tx_id, *transaction)),
            _ => Err(WalletError::UnexpectedApiResponse {
                method: "create_minting_transaction".to_string(),
                api: "AssetManagerService".to_string(),
            }),
        }
    }
}
