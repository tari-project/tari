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
use std::time::Duration;
use bollard::Docker;
use tauri::{AppHandle, Manager, State, Wry};
use crate::commands::AppState;
use log::*;
use serde::{Serialize, Deserialize};
use crate::commands::events::event_name;
use crate::docker::{BaseNodeConfig, ContainerId, DEFAULT_MINING_ADDRESS, DEFAULT_MONEROD_URL, DockerWrapperError, LaunchpadConfig, MmProxyConfig, Sha3MinerConfig, TariWorkspace, WalletConfig, XmRigConfig};
use crate::docker::TariNetwork;
use crate::docker::helpers::create_password;
use crate::error::LauncherError;
use std::path::PathBuf;
use crate::docker::ImageType;
use futures::StreamExt;

/// "Global" settings from the launcher front-end
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "snake_case"))]
pub struct ServiceSettings {
    pub tari_network: String,
    pub root_folder: String,
    pub wallet_password: String,
    pub monero_mining_address: Option<String>,
    pub num_mining_threads: i64,
    pub docker_registry: Option<String>,
    pub docker_tag: Option<String>,
    pub monerod_url: Option<String>,
    pub monero_username: Option<String>,
    pub monero_password: Option<String>,
    pub monero_use_auth: Option<bool>,
}

impl TryFrom<ServiceSettings> for LaunchpadConfig {
    type Error = LauncherError;

    fn try_from(settings: ServiceSettings) -> Result<Self, Self::Error> {
        let tari_network = TariNetwork::try_from(settings.tari_network.to_lowercase().as_str())?;
        let tor_control_password = create_password(16);
        // Since most services are launched manually, we set the delay value to zero.
        let zero_delay = Duration::from_secs(0);
        let base_node = BaseNodeConfig {
            delay: zero_delay,
        };
        let wallet = WalletConfig {
            delay: zero_delay,
            password: settings.wallet_password,
        };
        let sha3_miner = Sha3MinerConfig {
            delay: zero_delay,
            num_mining_threads: settings.num_mining_threads as usize,
        };
        let mut mm_proxy = MmProxyConfig {
            delay: zero_delay,
            monerod_url: settings.monerod_url.unwrap_or_else(|| DEFAULT_MONEROD_URL.to_string()),
            ..Default::default()
        };
        if let Some(val) = settings.monero_use_auth {
            mm_proxy.monero_use_auth = val;
            mm_proxy.monero_username = settings.monero_username.unwrap_or_else(|| "".to_string());
            mm_proxy.monero_password = settings.monero_password.unwrap_or_else(|| "".to_string());
        }
        let monero_mining_address = settings.monero_mining_address.unwrap_or_else(|| DEFAULT_MINING_ADDRESS.to_string());
        let xmrig = XmRigConfig {
            delay: Duration::from_secs(15), // Needs to wait for mm_proxy to be ready
            monero_mining_address,
        };
        Ok(LaunchpadConfig {
            data_directory: PathBuf::from(settings.root_folder),
            tari_network,
            tor_control_password,
            base_node: Some(base_node),
            wallet: Some(wallet),
            sha3_miner: Some(sha3_miner),
            mm_proxy: Some(mm_proxy),
            xmrig: Some(xmrig),
            registry: settings.docker_registry,
            tag: settings.docker_tag,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct StartServiceResult {
    name: String,
    id: String,
    action: String,
    log_events_name: String,
    stats_events_name: String,
}

/// Starts the specified service
///
/// The workspace will be created if this is the first call to `start_service`. Otherwise, the settings from the first
/// call will be used and `settings` will be ignored.
#[tauri::command]
pub async fn start_service(app: AppHandle<Wry>, service_name: String, settings: ServiceSettings) -> Result<StartServiceResult, String> {
    start_service_impl(app, service_name, settings).await
        .map_err(|e| e.to_string())
}

/// Stops the specified service
///
/// Stops the container, if it is running. action is "
/// Then, deletes the container.
/// Returns the container id
#[tauri::command]
pub async fn stop_service(state: State<'_, AppState>, service_name: String) -> Result<String, String> {
    todo!();
}

#[tauri::command]
pub async fn create_default_workspace(state: State<'_, AppState>, settings: ServiceSettings) -> Result<bool, String> {
    create_default_workspace_impl(&state, settings).await
        .map_err(|e| e.to_string())
}

async fn create_default_workspace_impl(state: &State<'_, AppState>, settings: ServiceSettings) -> Result<bool, LauncherError> {
    let config = LaunchpadConfig::try_from(settings)?;
    let should_create_workspace = {
        let wrapper = state.workspaces.read().await;
        !wrapper.workspace_exists("default")
    }; // drop read-only lock
    if should_create_workspace {
        // Only get a write-lock if we need one
        let mut wrapper = state.workspaces.write().await;
        wrapper.create_workspace("default", config)?;
    }
    Ok(should_create_workspace)
}

async fn start_service_impl(app: AppHandle<Wry>, service_name: String, settings: ServiceSettings) -> Result<StartServiceResult, LauncherError> {
    let state = app.state::<AppState>();
    let docker = state.docker_handle().await;
    let _ = create_default_workspace_impl(&state, settings).await?;
    let mut wrapper = state.workspaces.write().await;
    debug!("Starting {} service", service_name);
    // We've just checked this, so it should never fail:
    let workspace: &mut TariWorkspace = wrapper.get_workspace_mut("default")
        .ok_or(DockerWrapperError::UnexpectedError)?;
    // Check the identity requirements for the service
    let ids = workspace.create_identities()?;
    // Check network requirements for the service
    if !workspace.network_exists(&docker).await? {
        workspace.create_network(&docker).await?;
    }
    // Launch the container
    let registry = workspace.config().registry.clone();
    let tag = workspace.config().tag.clone();
    let image = ImageType::try_from(service_name.as_str())?;
    let args = workspace.config().command(image);
    let container_name = workspace.start_service(image, registry, tag, docker.clone()).await?;
    let state = workspace.container_mut(container_name.as_str())
        .ok_or(DockerWrapperError::UnexpectedError)?;
    let id = state.id().to_string();
    let stats_events_name = event_name(state.id());
    let log_events_name = log_event_name(state.name());
    // Set up event streams
    container_logs(app.clone(), log_events_name.as_str(), container_name.as_str(), docker.clone(), workspace);
    // Collect data for the return object
    let result = StartServiceResult {
        name: service_name,
        id,
        action: "unimplemented".to_string(),
        log_events_name,
        stats_events_name,
    };
    info!("Tari service {} has launched", image.container_name());
    Ok(result)
}


pub fn log_event_name(container_name: &str) -> String {
    format!("tari://docker_log_{}", container_name)
}

fn container_logs(app: AppHandle<Wry>, event_name: &str, container_name: &str, docker: Docker, workspace: &mut TariWorkspace) {
    info!("Setting up log events for {}", container_name);
    if let Some(mut stream) = workspace.logs(container_name, docker) {
        let event_name = event_name.to_string();
        tauri::async_runtime::spawn(async move {
            while let Some(message) = stream.next().await {
                let emit_result = match message {
                    Ok(payload) => app.emit_all(event_name.as_str(), payload),
                    Err(err) => app.emit_all(format!("{}_error", event_name).as_str(), err.chained_message()),
                };
                if let Err(err) = emit_result {
                    warn!("Error emitting event: {}", err.to_string());
                }
            }
            info!("Log stream for {} has closed.", event_name);
        });
        info!("Container log events configured.");
    } else {
        info!("Log events could not be configured: {} is not a running container", container_name);
    }
}
