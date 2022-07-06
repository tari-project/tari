// Copyright 2021. The Tari Project
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
mod base_node_api;
mod wallet_api;
use std::{convert::TryFrom, fmt::format};

use config::Config;
use futures::StreamExt;
use log::{debug, error, info, warn};
use serde::Serialize;
use tari_app_grpc::tari_rpc::wallet_client;
use tauri::{http::status, AppHandle, Manager, Wry};
pub use wallet_api::{wallet_balance, wallet_events, wallet_identity};

use crate::{
    commands::{status, AppState, ServiceSettings, DEFAULT_IMAGES},
    docker::{ContainerState, ImageType, TariNetwork, TariWorkspace},
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    image_name: String,
    container_name: String,
    display_name: String,
    docker_image: String,
    updated: bool,
    pending: bool,
    error: String,
    progress: i32,
    status: DockerImagePullStatus,
}

#[derive(Debug, Serialize)]
enum DockerImagePullStatus {
    Waiting,
    Pulling,
    Ready,
}

impl DockerImagePullStatus {
    pub fn lower_case(self) -> &'static str {
        match self {
            Self::Pulling => "pulling",
            Self::Ready => "ready",
            Self::Waiting => "waiting",
        }
    }

    pub fn upper_case(self) -> &'static str {
        match self {
            Self::Pulling => "PULLING",
            Self::Ready => "READY",
            Self::Waiting => "WAITING",
        }
    }
}

/// Default image status is Ready.
impl Default for DockerImagePullStatus {
    fn default() -> Self {
        Self::Ready
    }
}

impl Default for ImageInfo {
    fn default() -> Self {
        ImageInfo {
            image_name: String::default(),
            display_name: String::default(),
            container_name: String::default(),
            docker_image: String::default(),
            updated: false,
            pending: false,
            error: String::default(),
            progress: 0,
            status: DockerImagePullStatus::default(),
        }
    }
}

impl TryFrom<&str> for DockerImagePullStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pulling" => Ok(DockerImagePullStatus::Pulling),
            "ready" => Ok(DockerImagePullStatus::Ready),
            "waiting" => Ok(DockerImagePullStatus::Waiting),
            _ => Err("unsupported image status".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageListDto {
    image_info: Vec<ImageInfo>,
    service_recipes: Vec<Vec<String>>,
}

pub fn enum_to_list<T: Sized + ToString + Clone>(enums: &[T]) -> Vec<String> {
    enums.iter().map(|enum_value| enum_value.to_string()).collect()
}

#[tauri::command]
pub fn network_list() -> Vec<String> {
    enum_to_list::<TariNetwork>(&TARI_NETWORKS)
}

/// Provide information about docker images and service recipes in Tari "ecosystem"
#[tauri::command]
pub fn image_info(settings: ServiceSettings) -> ImageListDto {
    let registry = settings.docker_registry.as_ref().map(String::as_str);
    let tag = settings.docker_tag.as_ref().map(String::as_str);

    let images: Vec<ImageInfo> = DEFAULT_IMAGES
        .iter()
        .map(|value| ImageInfo {
            image_name: value.image_name().to_string(),
            container_name: value.container_name().to_string(),
            display_name: value.display_name().to_string(),
            docker_image: TariWorkspace::fully_qualified_image(*value, registry, tag),
            ..Default::default()
        })
        .collect();

    let recipes: Vec<Vec<String>> = [
        [ImageType::BaseNode, ImageType::Tor]
            .iter()
            .map(|image_type| image_type.container_name().to_string())
            .collect(),
        [ImageType::Wallet, ImageType::BaseNode, ImageType::Tor]
            .iter()
            .map(|image_type| image_type.container_name().to_string())
            .collect(),
        [
            ImageType::Sha3Miner,
            ImageType::Wallet,
            ImageType::BaseNode,
            ImageType::Tor,
        ]
        .iter()
        .map(|image_type| image_type.container_name().to_string())
        .collect(),
        [
            ImageType::MmProxy,
            ImageType::XmRig,
            ImageType::Wallet,
            ImageType::BaseNode,
            ImageType::Tor,
        ]
        .iter()
        .map(|image_type| image_type.container_name().to_string())
        .collect(),
    ]
    .to_vec();

    ImageListDto {
        image_info: images,
        service_recipes: recipes,
    }
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
