use crate::{
    assets::{
        infrastructure::{AssetManagerRequest, AssetManagerResponse},
        Asset,
    },
    error::WalletError,
};
use tari_service_framework::{reply_channel::SenderService, Service};

use tari_core::transactions::transaction::Transaction;
use tari_core::transactions::transaction_protocol::TxId;
use tari_core::transactions::types::PublicKey;

#[derive(Clone)]
pub struct AssetManagerHandle {
    handle: SenderService<AssetManagerRequest, Result<AssetManagerResponse, WalletError>>,
}

impl AssetManagerHandle {

    pub fn new(sender: SenderService<AssetManagerRequest, Result<AssetManagerResponse, WalletError>>) -> Self {
        Self{handle: sender}
    }
    pub async fn list_owned_assets(&mut self) -> Result<Vec<Asset>, WalletError> {
        match self.handle.call(AssetManagerRequest::ListOwned {}).await?? {
            AssetManagerResponse::ListOwned { assets } => Ok(assets),
             _ => Err(WalletError::UnexpectedApiResponse{ method: "list_owned_assets".to_string(), api: "AssetManagerService".to_string()}),
        }
    }

    pub async fn get_owned_asset_by_pub_key(&mut self, public_key: PublicKey) -> Result<Asset, WalletError> {
       match self.handle.call(AssetManagerRequest::GetOwnedAsset{ public_key}).await?? {
           AssetManagerResponse::GetOwnedAsset {asset } => Ok(asset),
           _ => Err(WalletError::UnexpectedApiResponse {method: "get_owned_asset_by_pub_key".to_string(), api: "AssetManagerService".to_string()})
       }
    }

    pub async fn create_registration_transaction(&mut self, name: String) -> Result<(TxId, Transaction), WalletError> {
        match self.handle.call(AssetManagerRequest::CreateRegistrationTransaction{name}).await?? {
            AssetManagerResponse::CreateRegistrationTransaction{transaction, tx_id}
            => Ok((tx_id, transaction)),
            _ => Err(WalletError::UnexpectedApiResponse{ method: "create_registration_transaction".to_string(), api: "AssetManagerService".to_string()}),
        }
    }
}
