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

use std::convert::TryFrom;

use bollard::{image::CreateImageOptions, models::CreateImageInfo};
use futures::{future::join_all, stream::StreamExt, Stream, TryFutureExt, TryStreamExt};
use log::{debug, error, warn};
use serde::Serialize;
use tauri::{AppHandle, Manager, Wry};

use crate::{
    commands::AppState,
    docker::{DockerWrapper, DockerWrapperError, ImageType, TariWorkspace, DOCKER_INSTANCE},
    error::LauncherError,
    rest::DockerImageError,
};

const LOG_TARGET: &str = "tari::launchpad::commands::pull_images";

#[derive(Debug, Clone, Serialize)]
pub struct Payload {
    image: String,
    name: String,
    info: CreateImageInfo,
}

pub static DEFAULT_IMAGES: [ImageType; 9] = [
    ImageType::Tor,
    ImageType::BaseNode,
    ImageType::Wallet,
    ImageType::Sha3Miner,
    ImageType::XmRig,
    ImageType::MmProxy,
    ImageType::Loki,
    ImageType::Promtail,
    ImageType::Grafana,
];

#[derive(Debug, Clone, Serialize)]
pub struct PullProgressInfo {
    image_name: Option<String>,
    id: Option<String>,
    status: Option<String>,
    error: Option<String>,
    progress: Option<String>,
    current: Option<i64>,
    total: Option<i64>,
}

pub async fn pull_latest_image(
    full_image_name: String,
) -> impl Stream<Item = Result<CreateImageInfo, DockerImageError>> {
    let docker = DOCKER_INSTANCE.clone();
    let opts = Some(CreateImageOptions {
        from_image: full_image_name,
        ..Default::default()
    });
    docker.create_image(opts, None, None).map_err(DockerImageError::from)
}
fn from(image_name: String, source: CreateImageInfo) -> PullProgressInfo {
    PullProgressInfo {
        image_name: Some(image_name),
        id: source.id,
        status: source.status,
        error: source.error,
        progress: source.progress,
        current: if let Some(details) = source.progress_detail.clone() {
            details.current
        } else {
            None
        },
        total: if let Some(details) = source.progress_detail {
            details.total
        } else {
            None
        },
    }
}

/// Pulls all the images concurrently using the docker API.
#[tauri::command]
pub async fn pull_images(app: AppHandle<Wry>) -> Result<(), String> {
    debug!("Command pull_images invoked");
    let futures = DEFAULT_IMAGES
        .iter()
        .map(|image| pull_image(app.clone(), image.image_name()).map_err(|e| format!("error pulling image: {}", e)));
    let results: Vec<Result<_, String>> = join_all(futures).await;
    let errors = results
        .into_iter()
        .filter(|r| r.is_err())
        .map(|e| e.unwrap_err())
        .collect::<Vec<String>>();
    if !errors.is_empty() {
        error!("Error pulling images:{}", errors.join("\n"));
        return Err(errors.join("\n"));
    }
    Ok(())
}

#[tauri::command]
pub async fn pull_image(app: AppHandle<Wry>, image_name: &str) -> Result<(), String> {
    let image = ImageType::try_from(image_name).map_err(|_err| format!("invalid image name: {}", image_name))?;
    let full_image_name = match image {
        ImageType::Loki | ImageType::Promtail | ImageType::Grafana => format!("grafana/{}:latest", image.image_name()),
        _ => TariWorkspace::fully_qualified_image(image, None),
    };
    let mut stream = pull_latest_image(full_image_name.clone()).await;
    while let Some(update) = stream.next().await {
        match update {
            Ok(source) => {
                let progress = from(full_image_name.clone(), source);
                debug!("Image pull progress:{:?}", progress);
                if let Err(err) = app.emit_all("tari://image_pull_progress", progress) {
                    warn!("Could not emit event to front-end, {:?}", err);
                }
            },
            Err(err) => return Err(format!("Error reading docker progress: {}", err)),
        };
    }
    Ok(())
}

#[tokio::test]
#[ignore]
async fn pull_image_test() {
    let fully_qualified_image = TariWorkspace::fully_qualified_image(ImageType::BaseNode, None);
    let mut stream = pull_latest_image(fully_qualified_image.clone()).await;
    while let Some(update) = stream.next().await {
        match update {
            Ok(source) => println!("Image pull progress:{:?}", from(fully_qualified_image.clone(), source)),
            Err(_err) => (),
        };
    }
}
