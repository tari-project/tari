use crate::{
    assets::{
        infrastructure::{AssetManagerRequest, AssetManagerResponse},
        AssetManager,
    },
    error::WalletError,
    output_manager_service::storage::{
        database::{OutputManagerBackend},
    },
};
use tari_service_framework::{
    reply_channel::{Receiver, },
};
use futures::{pin_mut, StreamExt};
use tari_shutdown::ShutdownSignal;
use log::*;
use crate::output_manager_service::handle::OutputManagerHandle;
use crate::types::MockPersistentKeyManager;

const LOG_TARGET: &str = "wallet::assets::infrastructure::asset_manager_service";

pub struct AssetManagerService<T: OutputManagerBackend + 'static> {
    manager: AssetManager<T, MockPersistentKeyManager>,
}

impl<T: OutputManagerBackend + 'static> AssetManagerService<T> {
    pub fn new(backend: T, output_manager: OutputManagerHandle) -> Self {
        Self {
            manager: AssetManager::<T,_>::new(backend, output_manager, MockPersistentKeyManager::new()),
        }
    }

    pub async fn start(
        mut self,
        mut shutdown_signal: ShutdownSignal,
        request_stream: Receiver<AssetManagerRequest, Result<AssetManagerResponse, WalletError>>,
    ) -> Result<(), WalletError> {
        let request_stream = request_stream.fuse();
        pin_mut!(request_stream);

        info!(target: LOG_TARGET, "Output Manager Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
                trace!(target: LOG_TARGET, "Handling Service API Request");
                    let (request, reply_tx) = request_context.split();
                    let response = self.handle_request(request).await.map_err(|e| {
                        warn!(target: LOG_TARGET, "Error handling request: {:?}", e);
                        e
                    });
                    let _ = reply_tx.send(response).map_err(|e| {
                        warn!(target: LOG_TARGET, "Failed to send reply");
                        e
                    });
                },
                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Asset manager service shutting down because it received the shutdown signal");
                    break;
                }
                complete => {
                    info!(target: LOG_TARGET, "Asset manager service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn handle_request(&mut self, request: AssetManagerRequest) -> Result<AssetManagerResponse, WalletError> {
        match request {
            AssetManagerRequest::ListOwned { .. } => Ok(AssetManagerResponse::ListOwned {
                assets: self.manager.list_owned().await?,
            }),
            AssetManagerRequest::CreateRegistrationTransaction {name} => {
                let (tx_id, transaction) =self.manager.create_registration_transaction(name).await?;
                Ok(AssetManagerResponse::CreateRegistrationTransaction {transaction, tx_id})
            }
            AssetManagerRequest::GetOwnedAsset { public_key } => {
                let asset = self.manager.get_owned_asset_by_pub_key(public_key).await?;
                Ok(AssetManagerResponse::GetOwnedAsset { asset})
            },
            AssetManagerRequest::CreateMintingTransaction { public_key, unique_ids } => {
                let (tx_id, transaction) =self.manager.create_minting_transaction(public_key, unique_ids).await?;
                Ok(AssetManagerResponse::CreateMintingTransaction {transaction, tx_id})
            }
        }
    }
}
