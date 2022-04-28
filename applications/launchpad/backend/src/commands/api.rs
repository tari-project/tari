use std::thread::spawn;

use bollard::models::{CreateImageInfo, Image, SystemEventsResponse};
use tauri::{api::clap::command, AppHandle, Manager, Wry};
use tokio::sync::RwLockWriteGuard;

use super::{
    events::{self, docker_events, stream_docker_events},
    launch_docker::{launch_docker_impl, WorkspaceLaunchOptions},
    network::{enum_to_list, TARI_NETWORKS},
    pull_images::{self, pull_all_images, Payload, DEFAULT_IMAGES},
    AppState,
};
use crate::{
    docker::{ImageType, TariNetwork, Workspaces},
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

/// This is an example of how we might launch a recipe. It's not currently used in the front-end, but essentially it
/// launches all the containers in a choreographed fashion to stand up the entire mining infra.
#[tauri::command]
pub async fn launch_docker(app: AppHandle<Wry>, name: String, config: WorkspaceLaunchOptions) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut wrapper: RwLockWriteGuard<Workspaces> = state.workspaces.write().await;
    let workspaces = wrapper.get_workspace_mut(name.as_str());
    // launch_docker_impl(state.clone(), workspaces, name, config)
    //     .await
    //     .map_err(|e| e.chained_message());

    todo!()
}

/// Subscribe to system-level docker events, such as container creation, volume, network events etc.
/// This function does not actually return a result, but async commands have a [bug](https://github.com/tauri-apps/tauri/issues/2533)
/// which is avoided by returning a `Result`.
///
/// A side effect of this command is that events are emitted to the `tari://docker-system-event` channel. Front-ends
/// can listen to this event stream to read in the event messages.
#[tauri::command]
pub async fn events(app: AppHandle<Wry>) -> Result<(), String> {
    let broadcast = |event: SystemEventsResponse| app.emit_all("tari://docker-system-event", event);
    stream_docker_events(broadcast).await;
    Ok(())
}
