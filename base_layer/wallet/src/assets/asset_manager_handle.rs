use crate::{
    assets::{
        infrastructure::{AssetManagerRequest, AssetManagerResponse},
        Asset,
    },
    error::WalletError,
};
use tari_service_framework::{reply_channel::SenderService, Service};

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
            // _ => Err(WalletError::UnexpectedApiResponse{ method: "list_owned_assets".to_string(), api: "AssetManagerService".to_string()}),
        }
    }
}
