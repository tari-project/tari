use std::{fmt::Debug, thread::spawn};

use bollard::{
    container::Stats,
    models::{CreateImageInfo, Image, SystemEventsResponse},
};
use log::{debug, info};
use serde::Serialize;
use tauri::{api::clap::command, AppHandle, Manager, Wry};
use tokio::sync::RwLockWriteGuard;

use super::{
    events::{self, docker_events, stream_docker_events},
    launch_docker::{launch_docker_impl, WorkspaceLaunchOptions},
    network::{enum_to_list, TARI_NETWORKS},
    pull_images::{self, pull_all_images, Payload, DEFAULT_IMAGES},
    service::{ServiceSettings, StartServiceResult, start_docker_service},
    AppState,
};
use crate::{
    docker::{ImageType, LogMessage, TariNetwork, Workspaces},
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

#[tauri::command]
pub async fn start_service (
    app: AppHandle<Wry>,
    service_name: String,
    settings: ServiceSettings,
) -> Result<StartServiceResult, String>
{
    let app1 = app.clone();
    let app2 = app.clone();
    let app3 = app.clone();
    let app4 = app.clone();

    let f1 = move |d: String, m: LogMessage| send_tauri_message::<LogMessage>(app1.clone(), d, m);
    let f2 = move |d: String, m: String| send_tauri_message::<String>(app2.clone(), d, m);
    let f3 = move |d: String, m: Stats| send_tauri_message::<Stats>(app3.clone(), d, m);
    let f4 = move |d: String, m: String| send_tauri_message::<String>(app4.clone(), d, m);
    start_docker_service(f1, f3, f2, service_name, settings).await
}

pub fn send_tauri_message<P: Debug + Clone + Serialize>(
    app: AppHandle<Wry>,
    event: String,
    payload: P,
) -> Result<(), tauri::Error> {
    debug!("sending: {:?} to {}", payload, event.clone());
    app.emit_all(event.as_str(), payload)
}
