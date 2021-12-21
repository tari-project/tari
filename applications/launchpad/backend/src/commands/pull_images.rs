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

use bollard::models::CreateImageInfo;
use futures::{future::join_all, stream::StreamExt, TryFutureExt};
use serde::Serialize;
use tauri::{AppHandle, Manager, Wry};

use crate::{
    commands::AppState,
    docker::{ImageType, TariWorkspace},
    error::LauncherError,
};

#[derive(Debug, Clone, Serialize)]
pub struct Payload {
    image: String,
    name: String,
    info: CreateImageInfo,
}

pub static DEFAULT_IMAGES: [ImageType; 8] = [
    ImageType::BaseNode,
    ImageType::Wallet,
    ImageType::Sha3Miner,
    ImageType::Tor,
    ImageType::MmProxy,
    ImageType::XmRig,
    ImageType::Monerod,
    ImageType::Frontail,
];

/// Provide a list of image names in the Tari "ecosystem"
#[tauri::command]
pub fn image_list() -> Vec<String> {
    DEFAULT_IMAGES
        .iter()
        .map(|&image| image.image_name().to_string())
        .collect()
}

/// Pulls all the images concurrently using the docker API.
#[tauri::command]
pub async fn pull_images(app: AppHandle<Wry>) -> Result<(), String> {
    let futures = DEFAULT_IMAGES
        .iter()
        .map(|image| pull_image(*image, app.clone()).map_err(|e| e.chained_message()));
    let results: Vec<Result<_, String>> = join_all(futures).await;
    let errors = results
        .into_iter()
        .filter(|r| r.is_err())
        .map(|e| e.unwrap_err())
        .collect::<Vec<String>>();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(())
}

async fn pull_image(image: ImageType, app: AppHandle<Wry>) -> Result<(), LauncherError> {
    let state = app.state::<AppState>().clone();
    let docker = state.docker.read().await;
    let image_name = TariWorkspace::fully_qualified_image(image, None, None);
    let mut stream = docker.pull_image(image_name.clone()).await;
    while let Some(update) = stream.next().await {
        match update {
            Ok(progress) => {
                let payload = Payload {
                    image: image_name.clone(),
                    name: image.image_name().to_string(),
                    info: progress,
                };
                app.emit_all("image-pull-progress", payload)?
            },
            Err(err) => return Err(err.into()),
        };
    }
    Ok(())
}
