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
mod wallet_api;
use std::{convert::TryFrom, fmt::format};

use config::Config;
use futures::StreamExt;
use log::{debug, error, info, warn};
use tauri::{http::status, AppHandle, Manager, Wry};
pub use wallet_api::wallet_events;

use crate::{
    commands::{status, AppState, DEFAULT_IMAGES},
    docker::{ContainerState, ImageType, TariNetwork},
    grpc::{GrpcWalletClient, WalletIdentity, WalletTransaction},
};

pub const RECEIVED: &str = "received";
pub const SENT: &str = "sent";
pub const QUEUED: &str = "queued";
pub const CONFIRMATION: &str = "confirmation";
pub const MINED: &str = "mined";
pub const CANCELLED: &str = "cancelled";
pub const NEW_BLOCK_MINED: &str = "new_block_mined";
pub static TRANSACTION_EVENTS: [&str; 7] = [RECEIVED, SENT, QUEUED, CONFIRMATION, MINED, CANCELLED, NEW_BLOCK_MINED];

pub static TARI_NETWORKS: [TariNetwork; 3] = [TariNetwork::Dibbler, TariNetwork::Igor, TariNetwork::Mainnet];

pub fn enum_to_list<T: Sized + ToString + Clone>(enums: &[T]) -> Vec<String> {
    enums.iter().map(|enum_value| enum_value.to_string()).collect()
}

#[tauri::command]
pub fn network_list() -> Vec<String> {
    enum_to_list::<TariNetwork>(&TARI_NETWORKS)
}

/// Provide a list of image names in the Tari "ecosystem"
#[tauri::command]
pub fn image_list() -> Vec<String> {
    enum_to_list::<ImageType>(&DEFAULT_IMAGES)
}

pub fn event_list() -> Vec<String> {
    TRANSACTION_EVENTS.iter().map(|&s| s.into()).collect()
}

#[tauri::command]
pub async fn health_check(image: &str) -> String {
    match ImageType::try_from(image) {
        Ok(img) => status(img).await,
        Err(_err) => format!("image {} not found", image),
    }
}
