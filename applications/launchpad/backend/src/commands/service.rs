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

use std::{convert::TryFrom, fmt::Debug, path::PathBuf, time::Duration};

use bollard::{
    container::{LogsOptions, Stats, StatsOptions},
    Docker,
};
use derivative::Derivative;
use futures::{Stream, StreamExt, TryFutureExt, TryStreamExt, future::ok};
use log::*;
use serde::{ser, Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, Wry};

use crate::{
    commands::{create_workspace::copy_config_file, pull_images::DOCKER_INSTANCE, AppState},
    docker::{
        create_workspace_folders,
        helpers::create_password,
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
        DEFAULT_MINING_ADDRESS,
        DEFAULT_MONEROD_URL,
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
    pub docker_compose: String,
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

pub async fn start_docker_service <FunSendMsg: 'static, FunSendStat: 'static, FunSendErr: 'static>(
    send_logs: FunSendMsg,
    send_stat: FunSendStat,
    send_err: FunSendErr,
    service_name: String,
    settings: ServiceSettings,
) -> Result<StartServiceResult, String> 
    where   FunSendMsg: Fn(String, LogMessage)  -> Result<(), tauri::Error> + std::marker::Send + Clone,
            FunSendStat: Fn(String, Stats) -> Result<(), tauri::Error> + std::marker::Send + Clone,
            FunSendErr: Fn(String, String) -> Result<(), tauri::Error> + std::marker::Send + Clone,  {
    info!("==> settings {:?}", settings);
    info!("==> service name {:?}", service_name.clone());
    let status = start_docker_compose_service(service_name.clone(), settings);
    if status.is_ok() {
        let container_id = status.unwrap();
        info!("Service {} has been started with id: {}", service_name.clone(), &container_id);
        let docker = DOCKER_INSTANCE.clone();
        let container_name = container_name(service_name.clone());
        let stats_events_name = stats_event_name(container_id.clone());
        let log_events_name = log_event_name(&service_name.clone());
        info!("==> container_name: {}", &container_name);
        info!("==> stats_events_name: {}", &stats_events_name);
        info!("==> log_events_name: {}", &log_events_name);
        // Set up event streams
        
        
        let log_destination = log_events_name.clone();
        let log_error_destination = format!("{}_error", log_events_name.clone());
        let stats_destination = stats_events_name.clone();
        let stats_error_destination = format!("{}_error", stats_events_name.clone());

        
        container_logs(send_logs,
             send_err.clone(),
              container_name.as_str(),
              log_destination.clone(),
              log_error_destination.clone(),
               docker.clone()).await;
        container_statistic(send_stat, 
            send_err,
             stats_events_name.as_str(),
              container_name.as_str(), 
              stats_destination.clone(),
              stats_error_destination.clone(),
              docker).await;

        start_service_impl(service_name, container_id, log_events_name, stats_events_name)
            .await
            .map_err(|e| {
                let error = e.chained_message();
                error!("{}", error);
                error
            })
    } else {
        Err("oppsssss...".to_string())
    }
}

fn container_name(service_name: String) -> String{
    match service_name.to_lowercase().as_str() {
        "tor" => "config-tor-1".to_string(),
        "base-node" => "config-base_node-1".to_string(),
        "base_node" => "config-base_node-1".to_string(),
        "wallet" => "config-wallet-1".to_string(),
        "sha3-miner" => "config-sha3_miner-1".to_string(),
        "sha3_miner" => "config-sha3_miner-1".to_string(),
        "mm_proxy" => "config-mm_proxy-1".to_string(),
        "mm-proxy" => "config-mm_proxy-1".to_string(),
        "xmrig" => "config-xmrig-1".to_string(),
        _ => "not_found".to_string()
    }
} 

fn start_docker_compose_service(service_name: String, settings: ServiceSettings) -> Result<String, LauncherError> {
    let status = std::process::Command::new("docker-compose")
        .env("DATA_FOLDER", settings.root_folder.clone())
        .env("TARI_NETWORK", settings.tari_network.clone())
        .arg("-f")
        .arg(settings.docker_compose.clone())
        .arg("up")
        .arg(service_name.clone())
        .arg("-d")
        .status()
        .expect("something went wrong");
    if status.success() {
        let container_id = container_id(service_name)?;
        Ok(container_id)
    } else {
        Err(LauncherError::from(DockerWrapperError::UnexpectedError))
    }
}

fn container_id(service_name: String) -> Result<String, LauncherError> {
    let status = std::process::Command::new("docker-compose")
    .env("DATA_FOLDER", "/Users/matkat/launchpad/config")
    .env("TARI_NETWORK", "dibbler")
    .env("TARI_MONEROD_PASSWORD", "tari")
    .env("TARI_MONEROD_USERNAME", "tari")
    .arg("-f")
    .arg("/Users/matkat/launchpad/config/docker-compose.yml")
    .arg("ps")
    .arg("-a")
    .arg(service_name)
    .arg("-q")
    .output();
    match status {
        Ok(output) => {
            let container_id  = String::from_utf8_lossy(&output.stdout).replace("\n", "");
            Ok(container_id)
        },
        Err(_) => Err(LauncherError::from(DockerWrapperError::UnexpectedError))
    }
}

async fn stop_docker_service(service_name: String) -> Result<(), LauncherError> {
    let status = std::process::Command::new("docker-compose")
    .env("DATA_FOLDER", "/Users/matkat/launchpad/config")
    .env("TARI_NETWORK", "dibbler")
    .env("TARI_MONEROD_PASSWORD", "tari")
    .env("TARI_MONEROD_USERNAME", "tari")
    .arg("-f")
    .arg("/Users/matkat/launchpad/config/docker-compose.yml")
    .arg("stop")
    .arg(service_name)
    .status()?;
    if status.success() {
              Ok(())
    } else{
        Err(LauncherError::from(DockerWrapperError::UnexpectedError))
    }
}

/// Stops the specified service
///
/// Stops the container, if it is running. action is "
/// Then, deletes the container.
/// Returns the container id
#[tauri::command]
pub async fn stop_service(service_name: String) -> Result<(), String> {
    info!("Stopping: {:?}", service_name.clone());
    stop_docker_service(service_name).await.map_err(|e| e.to_string())
}


async fn start_service_impl(
    service_name: String,
    container_id: String,
    log_events_name: String,
    stats_events_name: String,
) -> Result<StartServiceResult, LauncherError> {
    // debug!("Starting {} service", service_name);

    // Collect data for the return object
    let result = StartServiceResult {
        name: service_name.clone(),
        id: container_id,
        action: "unimplemented".to_string(),
        log_events_name,
        stats_events_name,
    };
    let image = ImageType::try_from(service_name.as_str()).unwrap();
    info!("Tari service {} has launched", image.container_name());
    Ok(result)
}

pub fn log_event_name(container_name: &str) -> String {
    format!("tari://docker_log_{}", container_name)
}

pub fn stats_event_name(container_id: String) -> String {
    format!("tari://docker_stats_{}", container_id.as_str())
}




async fn stop_service_impl(state: State<'_, AppState>, service_name: String) -> Result<(), LauncherError> {
    let docker = state.docker_handle().await;
    let mut wrapper = state.workspaces.write().await;
    debug!("Stopping {} service", service_name);
    // We've just checked this, so it should never fail:
    let workspace: &mut TariWorkspace = wrapper
        .get_workspace_mut("default")
        .ok_or_else(|| DockerWrapperError::WorkspaceDoesNotExist("default".into()))?;
    workspace.stop_container(service_name.as_str(), true, &docker).await;
    Ok(())
}

async fn process_stream<FM, FE, T: Debug + Clone + Serialize>(
    send_message: FM,
    send_error: FE,
    message_destination: String,
    error_destination: String,
    mut stream: impl Stream<Item = Result<T, DockerWrapperError>> + Unpin,
) where
    FM: Fn(String, T) -> Result<(), tauri::Error>,
    FE: Fn(String, String) -> Result<(), tauri::Error>,
{
    while let Some(message) = stream.next().await {
        let emit_result = match message {
            Ok(payload) => {
                send_message(message_destination.clone(), payload)
            },
            Err(err) => send_error(error_destination.clone(), err.chained_message()),
        };
        if let Err(err) = emit_result {
            warn!("Error emitting event: {}", err.to_string());
        }
    }
}

fn send_tauri_message<P: Debug + Clone + Serialize>(
    app: AppHandle<Wry>,
    event: String,
    payload: P,
) -> Result<(), tauri::Error> {
    debug!("sending: {:?} to {}", payload, event.clone());
    app.emit_all(event.as_str(), payload)
}

async fn container_statistic<FM: 'static, FE: 'static>(
    send_message: FM,
    send_error: FE,
    event_name: &str,
    container_name: &str,
    message_destination:String,
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
        let _ = process_stream(send_message, send_error, message_destination, error_destination, stream).await;
        info!("Resource stats stream for {} has closed.", event_name);
    });
    info!("Resource stats events configured.");
}

async fn container_logs<FM: 'static, FE: 'static>(
    send_message: FM,
    send_error: FE,
    container_name: &str,
    message_destination:String,
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
