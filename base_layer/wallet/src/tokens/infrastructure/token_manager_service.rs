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
    error::WalletError,
    output_manager_service::{handle::OutputManagerHandle, storage::database::OutputManagerBackend},
    tokens::{
        infrastructure::{TokenManagerRequest, TokenManagerResponse},
        TokenManager,
    },
};

const LOG_TARGET: &str = "wallet::assets::infrastructure::asset_manager_service";

pub struct TokenManagerService<T: OutputManagerBackend + 'static> {
    manager: TokenManager<T>,
}

impl<T: OutputManagerBackend + 'static> TokenManagerService<T> {
    pub fn new(backend: T, output_manager: OutputManagerHandle) -> Self {
        Self {
            manager: TokenManager::<T>::new(backend, output_manager),
        }
    }

    pub async fn start(
        mut self,
        mut shutdown_signal: ShutdownSignal,
        request_stream: Receiver<TokenManagerRequest, Result<TokenManagerResponse, WalletError>>,
    ) -> Result<(), WalletError> {
        let request_stream = request_stream.fuse();
        pin_mut!(request_stream);

        debug!(target: LOG_TARGET, "Token Manager Service started");
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
                    info!(target: LOG_TARGET, "Token manager service shutting down because it received the shutdown signal");
                    break;
                }
                complete => {
                    info!(target: LOG_TARGET, "Token manager service shutting down");
                    break;
                }
            }
        }
        Ok(())
    }

    pub async fn handle_request(&mut self, request: TokenManagerRequest) -> Result<TokenManagerResponse, WalletError> {
        match request {
            TokenManagerRequest::ListOwned { .. } => Ok(TokenManagerResponse::ListOwned {
                tokens: self.manager.list_owned().await?,
            }),
        }
    }
}
