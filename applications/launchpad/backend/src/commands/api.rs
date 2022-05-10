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

use std::{fmt::Debug, thread::spawn};

use bollard::{
    container::Stats,
    models::{CreateImageInfo, Image, SystemEventsResponse},
};
use log::{debug, error, info};
use serde::Serialize;
use tauri::{api::clap::command, AppHandle, Manager, Wry};
use tokio::sync::RwLockWriteGuard;

use super::{
    launch_docker::WorkspaceLaunchOptions,
    network::{enum_to_list, TARI_NETWORKS},
    pull_images::{self, pull_all_images, Payload, DEFAULT_IMAGES},
    service::{ServiceSettings, StartServiceResult},
    AppState,
};
use crate::{
    commands::{
        create_workspace::{self, configure_workspace},
        service::{self, start_service_impl},
        DEFAULT_WORKSPACE,
    },
    docker::{
        create_or_load_identities,
        DockerWrapperError,
        ImageType,
        LogMessage,
        TariNetwork,
        TariWorkspace,
        Workspaces,
        DOCKER_INSTANCE,
    },
    error::LauncherError,
};

/// Provide a list of network names available in the Tari Launchpad env.
#[tauri::command]
pub fn network_list() -> Vec<String> {
    enum_to_list::<TariNetwork>(&TARI_NETWORKS)
}

/// Provide a list of image names in the Tari "ecosystem"
#[tauri::command]
pub fn image_list() -> Vec<String> {
    enum_to_list::<ImageType>(&DEFAULT_IMAGES)
}

/// Checks and downloads the latest docker images.
/// Prints the progress and status to the app console.
#[tauri::command]
pub async fn pull_images(app: AppHandle<Wry>) -> Result<(), String> {
    let broadcast = |payload: Payload| app.emit_all("image-pull-progress", payload);
    pull_all_images(broadcast).await?;
    Ok(())
}

/// Starts the specified service
///
/// The workspace will be created if this is the first call to `start_service`. Otherwise, the settings from the first
/// call will be used and `settings` will be ignored.
///
/// Starting a service also:
/// - creates a new workspace, if required
/// - creates a network for the network, if required
/// - creates new volumes, if required
/// - creates new node identities, if required
/// - launches the container
/// - creates the log stream
/// - creates the resource stats stream
#[tauri::command]
pub async fn start_service(
    app: AppHandle<Wry>,
    service_name: String,
    settings: ServiceSettings,
) -> Result<StartServiceResult, String> {
    info!("starting docker container for {}", service_name);
    let created = service::create_workspace(app.clone(), settings.clone()).await;
    if created.is_ok() {
        info!("New workspace [{}] has been created.", settings.root_folder.as_str());
    } else {
        info!("Workspace: [{}] already exists.", settings.root_folder.as_str());
    }
    let workspace = configure_workspace(app.clone(), DEFAULT_WORKSPACE).await.unwrap();

    let app1 = app.clone();
    let app2 = app.clone();
    let app3 = app.clone();

    let send_msg =
        move |destination: String, msg: LogMessage| send_tauri_message::<LogMessage>(app1.clone(), destination, msg);

    let send_stats = move |destination: String, msg: Stats| send_tauri_message::<Stats>(app3.clone(), destination, msg);

    let send_err = move |destination: String, msg: String| send_tauri_message::<String>(app2.clone(), destination, msg);

    start_service_impl(send_msg, send_stats, send_err, service_name, workspace)
        .await
        .map_err(|_| "ooppss...Do not panic! Something went very very wrong...".to_string())
}

/// Stops the specified service
///
/// Stops the container, if it is running. action is "
/// Then, deletes the container.
/// Returns the container id
#[tauri::command]
pub async fn stop_service(service_name: String) -> Result<(), String> {
    service::stop_service_impl(service_name)
        .await
        .map_err(|e| e.to_string())
}

/// Sending messages to the Tauri
pub fn send_tauri_message<P: Debug + Clone + Serialize>(
    app: AppHandle<Wry>,
    event: String,
    payload: P,
) -> Result<(), tauri::Error> {
    debug!("sending: {:?} to {}", payload, event);
    app.emit_all(event.as_str(), payload)
}
