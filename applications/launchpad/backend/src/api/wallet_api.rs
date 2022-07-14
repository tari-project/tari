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
//
use std::{convert::TryFrom, fmt::format, thread, time::Duration};

use config::Config;
use futures::StreamExt;
use log::{debug, error, info, warn};
use tari_app_grpc::tari_rpc::wallet_client;
use tauri::{http::status, AppHandle, Manager, Wry};

use crate::{
    commands::{status, AppState, DEFAULT_IMAGES},
    docker::{ContainerState, ImageType, TariNetwork},
    grpc::{GrpcWalletClient, TransferFunds, TransferFundsResult, WalletBalance, WalletIdentity, WalletTransaction},
};

#[tauri::command]
pub async fn wallet_identity() -> Result<WalletIdentity, String> {
    // Check if wallet container is running.
    let status = status(ImageType::Wallet).await;
    if "running" == status.to_lowercase() {
        let mut wallet_client = GrpcWalletClient::new();
        let identity = wallet_client.identity().await.map_err(|e| e.to_string())?;
        info!("SUCCESS: IDENTITY: {:?}", identity);
        WalletIdentity::try_from(identity)
    } else {
        error!("Wallet container[image = {}] is not running", ImageType::Wallet);
        Err("Wallet is not running".to_string())
    }
}

#[tauri::command]
pub async fn wallet_balance() -> Result<WalletBalance, String> {
    // Check if wallet container is running.
    let status = status(ImageType::Wallet).await;
    if "running" == status.to_lowercase() {
        let mut wallet_client = GrpcWalletClient::new();
        let balance = wallet_client.balance().await.map_err(|e| e.to_string())?;
        Ok(WalletBalance::from(balance))
    } else {
        error!("Wallet container[image = {}] is not running", ImageType::Wallet);
        Err("Wallet is not running".to_string())
    }
}

#[tauri::command]
pub async fn wallet_events(app: AppHandle<Wry>) -> Result<(), String> {
    info!("Setting up event stream");
    let mut wallet_client = GrpcWalletClient::new();
    let mut stream = wallet_client.stream().await.map_err(|e| e.chained_message())?;
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(response) = stream.next().await {
            if let Some(value) = response.transaction {
                let wt = WalletTransaction {
                    event: value.event,
                    tx_id: value.tx_id,
                    source_pk: value.source_pk,
                    dest_pk: value.dest_pk,
                    status: value.status,
                    direction: value.direction,
                    amount: value.amount,
                    message: value.message,
                    is_coinbase: value.is_coinbase,
                };

                if let Err(err) = app_clone.emit_all("tari://wallet_event", wt) {
                    warn!("Could not emit event to front-end, {:?}", err);
                }
            }
        }
        info!("Event stream has closed.");
    });
    Ok(())
}

#[tauri::command]
pub async fn transfer(app: AppHandle<Wry>, funds: TransferFunds) -> Result<TransferFundsResult, String> {
    // Check if wallet container is running.
    debug!("Transfer funds: {:?}", funds);
    let app_clone = app.clone();
    let status = status(ImageType::Wallet).await;
    if "running" == status.to_lowercase() {
        let mut wallet_client = GrpcWalletClient::new();
        let response = wallet_client
            .transfer_funds(funds.clone())
            .await
            .map_err(|e| e.to_string())?;
        info!("Transfering funds {:?} succeeded: {:?}", funds, response);

        if let Err(err) = app_clone.emit_all("tari://wallet_transaction", response.clone()) {
            warn!("Could not emit transaction event to front-end, {:?}", err);
        }
        Ok(response)
    } else {
        error!("Wallet container[image = {}] is not running", ImageType::Wallet);
        Err("Wallet is not running".to_string())
    }
}

#[tauri::command]
pub async fn get_seed_words() -> Result<Vec<String>, String> {
    // Check if wallet container is running.
    let status = status(ImageType::Wallet).await;
    if "running" == status.to_lowercase() {
        let mut wallet_client = GrpcWalletClient::new();
        let seed_words = wallet_client.seed_words().await.map_err(|e| e.to_string())?;
        Ok(seed_words)
    } else {
        error!("Wallet container[image = {}] is not running", ImageType::Wallet);
        Err("Wallet is not running".to_string())
    }
}

#[tauri::command]
pub async fn delete_seed_words() -> Result<(), String> {
    // Check if wallet container is running.
    let status = status(ImageType::Wallet).await;
    if "running" == status.to_lowercase() {
        let mut wallet_client = GrpcWalletClient::new();
        wallet_client.delete_seed_words().await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        error!("Wallet container[image = {}] is not running", ImageType::Wallet);
        Err("Wallet is not running".to_string())
    }
}
