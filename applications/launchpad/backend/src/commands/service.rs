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

use std::{convert::TryFrom, path::PathBuf, time::Duration};

use bollard::{
    container::{LogsOptions, Stats, StatsOptions},
    Docker,
};
use derivative::Derivative;
use futures::{StreamExt, TryStreamExt};
use log::*;
use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle, Manager, State, Wry};

use crate::{
    commands::{create_workspace::copy_config_file, AppState},
    docker::{
        self,
        create_workspace_folders,
        helpers::{create_password, process_stream},
        BaseNodeConfig,
        ContainerId,
        DockerWrapperError,
        ImageType,
        LaunchpadConfig,
        LogMessage,
        MmProxyConfig,
        Sha3MinerConfig,
        TariNetwork,
        TariWorkspace,
        WalletConfig,
        XmRigConfig,
        CONTAINERS,
        DEFAULT_MINING_ADDRESS,
        DEFAULT_MONEROD_URL,
        DOCKER_INSTANCE,
    },
    error::LauncherError,
};

/// "Global" settings from the launcher front-end
#[derive(Clone, Derivative, Deserialize)]
#[derivative(Debug)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettings {
    pub tari_network: String,
    pub root_folder: String,
    #[derivative(Debug = "ignore")]
    pub wallet_password: String,
    pub monero_mining_address: Option<String>,
    pub num_mining_threads: i64,
    pub docker_registry: Option<String>,
    pub docker_tag: Option<String>,
    pub monerod_url: Option<String>,
    pub monero_username: Option<String>,
    #[derivative(Debug = "ignore")]
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
        let base_node = BaseNodeConfig { delay: zero_delay };
        let wallet = WalletConfig {
            delay: zero_delay,
            password: settings.wallet_password,
        };
        let sha3_miner = Sha3MinerConfig {
            delay: zero_delay,
            num_mining_threads: usize::try_from(settings.num_mining_threads).unwrap(),
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
        let monero_mining_address = settings
            .monero_mining_address
            .unwrap_or_else(|| DEFAULT_MINING_ADDRESS.to_string());
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

/// [`start_service`] returns some useful info that the front-end can use to control the docker environment.
/// In particular, the log and stats event names tell the front end which channels events will be broadcast on.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct StartServiceResult {
    /// The name of the service that was created
    name: String,
    /// The docker id of the container. Some commands need an id rather than the name.
    id: String,
    /// What action was taken. Currently not used.
    action: String,
    /// The name of the event stream to subscribe to for log events. These are the _docker_ logs and not the base node
    /// _et. al._ logs. Those are saved to disk and accessible using the usual means, or with frontail.
    log_events_name: String,
    /// The name of the event stream to subscribe to for resource events (CPU, memory etc).
    stats_events_name: String,
}

/// The "default" workspace is one that is used in the manual front-end configuration (each container is started and
/// stopped manually)
#[tauri::command]
pub async fn create_default_workspace(app: AppHandle<Wry>, settings: ServiceSettings) -> Result<bool, String> {
    create_default_workspace_impl(app, settings).await.map_err(|e| {
        let error = e.chained_message();
        error!("{}", error);
        error
    })
}

async fn create_default_workspace_impl(app: AppHandle<Wry>, settings: ServiceSettings) -> Result<bool, LauncherError> {
    let config = LaunchpadConfig::try_from(settings)?;
    let state = app.state::<AppState>();
    let app_config = app.config();
    let should_create_workspace = {
        let wrapper = state.workspaces.read().await;
        !wrapper.workspace_exists("default")
    }; // drop read-only lock
    if should_create_workspace {
        let package_info = &state.package_info;
        create_workspace_folders(&config.data_directory)?;
        copy_config_file(&config.data_directory, app_config.as_ref(), package_info, "log4rs.yml")?;
        copy_config_file(&config.data_directory, app_config.as_ref(), package_info, "config.toml")?;
        // Only get a write-lock if we need one
        let mut wrapper = state.workspaces.write().await;
        wrapper.create_workspace("default", config)?;
    }
    Ok(should_create_workspace)
}

pub async fn start_service_impl<FunSendMsg: 'static, FunSendStat: 'static, FunSendErr: 'static>(
    send_logs: FunSendMsg,
    send_stat: FunSendStat,
    send_err: FunSendErr,
    service_name: String,
    workspace: TariWorkspace,
) -> Result<StartServiceResult, String>
where
    FunSendMsg: Fn(String, LogMessage) -> Result<(), tauri::Error> + std::marker::Send + Clone,
    FunSendStat: Fn(String, Stats) -> Result<(), tauri::Error> + std::marker::Send + Clone,
    FunSendErr: Fn(String, String) -> Result<(), tauri::Error> + std::marker::Send + Clone,
{
    debug!("Starting {} service", service_name);
    let registry = workspace.config().registry.clone();
    let tag = workspace.config().tag.clone();

    let ids = workspace.create_or_load_identities().unwrap();
    for id in ids.values() {
        info!("@@@ Identity loaded: {}", id);
    }

    let image = ImageType::try_from(service_name.as_str()).unwrap();
    let mut wrk1 = workspace.clone();
    let _container_name = wrk1
        .start_service(image, registry, tag, DOCKER_INSTANCE.clone())
        .await
        .unwrap();

    let found_state = match CONTAINERS.try_read().unwrap().get(service_name.as_str()){
        Some(reference) => Some((*reference).to_owned()),
        None => None,
    };
    
    if found_state.is_some() {
        
        let container_state = found_state.unwrap();
        let stats_events_name = stats_event_name(container_state.id());
        let log_events_name = log_event_name(container_state.name());
        info!("==> stats_events_name: {}", stats_events_name);
        info!("==> log_events_name: {}", log_events_name);
        let log_error_destination = format!("{}_error", log_events_name.clone());
        let stats_error_destination = format!("{}_error", stats_events_name.clone());
        
        container_logs(
            send_logs,
            send_err.clone(),
            container_state.id().clone().as_str(),
            log_events_name.clone(),
            log_error_destination,
            DOCKER_INSTANCE.clone(),
        );

        container_statistic(
            send_stat,
            send_err,
            stats_events_name.as_str(),
            container_state.id().clone().as_str(),
            stats_error_destination.clone(),
            DOCKER_INSTANCE.clone(),
        );
        // Collect data for the return object
        let result = StartServiceResult {
            name: service_name,
            id: container_state.id().clone().to_string(),
            action: "unimplemented".to_string(),
            log_events_name,
            stats_events_name,
        };
        info!("Tari service {} has launched", image.container_name());
        Ok(result)
    } else {
        Err(DockerWrapperError::ContainerNotFound(format!("Container {} is not found", service_name)).to_string())
    }
    
}

fn container_statistic<FM: 'static, FE: 'static>(
    send_message: FM,
    send_error: FE,
    event_name: &str,
    container_name: &str,
    error_destination: String,
    docker: Docker,
) where
    FM: Fn(String, Stats) -> Result<(), tauri::Error> + std::marker::Send,
    FE: Fn(String, String) -> Result<(), tauri::Error> + std::marker::Send,
{
    info!("Setting up Resource stats events for {}", container_name);
    let options = StatsOptions {
        stream: true,
        one_shot: false,
    };
    let stream = docker
        .stats(container_name, Some(options))
        .map_err(DockerWrapperError::from);
    let event_name = event_name.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = process_stream(send_message, send_error, event_name.clone(), error_destination, stream).await;
        info!("Resource stats stream for {} has closed.", event_name);
    });
    info!("Resource stats events configured.");
}

pub async fn create_workspace(app: AppHandle<Wry>, settings: ServiceSettings) -> Result<(), LauncherError> {
    let state = app.state::<AppState>();
    let docker = state.docker_handle().await;
    let _ = create_default_workspace_impl(app.clone(), settings).await?;
    let mut wrapper = state.workspaces.write().await;
    // We've just checked this, so it should never fail:
    let workspace: &mut TariWorkspace = wrapper
        .get_workspace_mut("default")
        .ok_or(DockerWrapperError::UnexpectedError)?;
    // Check the identity requirements for the service
    let ids = workspace.create_or_load_identities()?;
    for id in ids.values() {
        debug!("Identity loaded: {}", id);
    }
    // Check network requirements for the service
    if !workspace.network_exists(&docker).await? {
        workspace.create_network(&docker).await?;
    }
    // Launch the container
    let _registry = workspace.config().registry.clone();
    let _tag = workspace.config().tag.clone();
    Ok(())
}

pub fn log_event_name(container_name: &str) -> String {
    format!("tari://docker_log_{}", container_name)
}

pub fn stats_event_name(container_id: &ContainerId) -> String {
    format!("tari://docker_stats_{}", container_id.as_str())
}

fn container_logs<FM: 'static, FE: 'static>(
    send_message: FM,
    send_error: FE,
    container_name: &str,
    message_destination: String,
    error_destination: String,
    docker: Docker,
) where
    FM: Fn(String, LogMessage) -> Result<(), tauri::Error> + std::marker::Send,
    FE: Fn(String, String) -> Result<(), tauri::Error> + std::marker::Send,
{
    info!(" $$$$ Setting up log events for {}", container_name);
    let options = LogsOptions::<String> {
        follow: true,
        stdout: true,
        stderr: true,
        ..Default::default()
    };
    let stream = docker
        .logs(container_name, Some(options))
        .map(|log| log.map(LogMessage::from).map_err(DockerWrapperError::from));
    tauri::async_runtime::spawn(async move {
        let _ = process_stream(send_message, send_error, message_destination, error_destination, stream).await;
        info!("Resource logs stream for has closed.");
    });
    info!("Resource logs events configured.");
}

fn container_stats(
    app: AppHandle<Wry>,
    event_name: &str,
    container_name: &str,
    docker: Docker,
    workspace: &mut TariWorkspace,
) {
    info!("Setting up Resource stats events for {}", container_name);
    if let Some(mut stream) = workspace.resource_stats(container_name, docker) {
        let event_name = event_name.to_string();
        tauri::async_runtime::spawn(async move {
            while let Some(message) = stream.next().await {
                trace!("log event: {:?}", message);
                let emit_result = match message {
                    Ok(payload) => app.emit_all(event_name.as_str(), payload),
                    Err(err) => app.emit_all(format!("{}_error", event_name).as_str(), err.chained_message()),
                };
                if let Err(err) = emit_result {
                    warn!("Error emitting event: {}", err.to_string());
                }
            }
            info!("Resource stats stream for {} has closed.", event_name);
        });
        info!("Resource stats events configured.");
    } else {
        info!(
            "Resource stats events could not be configured: {} is not a running container",
            container_name
        );
    }
}

pub async fn stop_service_impl(state: State<'_, AppState>, service_name: String) -> Result<(), LauncherError> {
    let mut wrapper = state.workspaces.write().await;
    debug!("Stopping {} service", service_name);
    // We've just checked this, so it should never fail:
    let workspace: &mut TariWorkspace = wrapper
        .get_workspace_mut("default")
        .ok_or_else(|| DockerWrapperError::WorkspaceDoesNotExist("default".into()))?;
    workspace
        .stop_container(service_name.as_str(), true, &DOCKER_INSTANCE)
        .await;
    Ok(())
}
