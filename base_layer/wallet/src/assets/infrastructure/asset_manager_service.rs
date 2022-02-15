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

use futures::{pin_mut, StreamExt};
use log::*;
use tari_service_framework::reply_channel::Receiver;
use tari_shutdown::ShutdownSignal;

use crate::{
    assets::{
        infrastructure::{AssetManagerRequest, AssetManagerResponse},
        AssetManager,
    },
    error::WalletError,
    output_manager_service::{handle::OutputManagerHandle, storage::database::OutputManagerBackend},
};

const LOG_TARGET: &str = "wallet::assets::infrastructure::asset_manager_service";

pub struct AssetManagerService<T: OutputManagerBackend + 'static> {
    manager: AssetManager<T>,
}

impl<T: OutputManagerBackend + 'static> AssetManagerService<T> {
    pub fn new(backend: T, output_manager: OutputManagerHandle) -> Self {
        Self {
            manager: AssetManager::<T>::new(backend, output_manager),
        }
    }

    pub async fn start(
        mut self,
        mut shutdown_signal: ShutdownSignal,
        request_stream: Receiver<AssetManagerRequest, Result<AssetManagerResponse, WalletError>>,
    ) -> Result<(), WalletError> {
        let request_stream = request_stream.fuse();
        pin_mut!(request_stream);

        debug!(target: LOG_TARGET, "Asset Manager Service started");
        loop {
            futures::select! {
                request_context = request_stream.select_next_some() => {
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
        trace!(target: LOG_TARGET, "Handling Service API Request {:?}", request);
        match request {
            AssetManagerRequest::ListOwned { .. } => Ok(AssetManagerResponse::ListOwned {
                assets: self.manager.list_owned().await?,
            }),
            AssetManagerRequest::CreateRegistrationTransaction {
                name,
                public_key,
                template_ids_implemented,
                description,
                image,
                template_parameters,
            } => {
                let (tx_id, transaction) = self
                    .manager
                    .create_registration_transaction(
                        name,
                        *public_key,
                        description,
                        image,
                        template_ids_implemented,
                        template_parameters,
                    )
                    .await?;
                Ok(AssetManagerResponse::CreateRegistrationTransaction {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
            AssetManagerRequest::GetOwnedAsset { public_key } => {
                let asset = self.manager.get_owned_asset_by_pub_key(public_key).await?;
                Ok(AssetManagerResponse::GetOwnedAsset { asset: Box::new(asset) })
            },
            AssetManagerRequest::CreateMintingTransaction {
                asset_public_key,
                asset_owner_commitment,
                features,
            } => {
                let (tx_id, transaction) = self
                    .manager
                    .create_minting_transaction(*asset_public_key, *asset_owner_commitment, features)
                    .await?;
                Ok(AssetManagerResponse::CreateMintingTransaction {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
            AssetManagerRequest::CreateInitialCheckpoint {
                asset_public_key,
                merkle_root,
                committee_public_keys,
            } => {
                let (tx_id, transaction) = self
                    .manager
                    .create_initial_asset_checkpoint(*asset_public_key, merkle_root, committee_public_keys)
                    .await?;
                Ok(AssetManagerResponse::CreateInitialCheckpoint {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
            AssetManagerRequest::CreateFollowOnCheckpoint {
                asset_public_key,
                unique_id,
                merkle_root,
                committee_public_keys,
            } => {
                let (tx_id, transaction) = self
                    .manager
                    .create_follow_on_asset_checkpoint(*asset_public_key, unique_id, merkle_root, committee_public_keys)
                    .await?;
                Ok(AssetManagerResponse::CreateFollowOnCheckpoint {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
            AssetManagerRequest::CreateCommitteeCheckpoint {
                asset_public_key,
                committee_public_keys,
                effective_sidechain_height,
            } => {
                let (tx_id, transaction) = self
                    .manager
                    .create_committee_definition(*asset_public_key, committee_public_keys, effective_sidechain_height)
                    .await?;
                Ok(AssetManagerResponse::CreateCommitteeCheckpoint {
                    transaction: Box::new(transaction),
                    tx_id,
                })
            },
        }
    }
}
