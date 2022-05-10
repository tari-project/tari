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

use std::{collections::HashMap, convert::TryFrom, path::PathBuf, time::Duration};

use bollard::{
    container::{
        Config,
        CreateContainerOptions,
        LogsOptions,
        NetworkingConfig,
        Stats,
        StatsOptions,
        StopContainerOptions,
    },
    models::{EndpointSettings, HostConfig},
    Docker,
};
use derivative::Derivative;
use futures::{StreamExt, TryStreamExt};
use log::*;
use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle, Manager, State, Wry};

use super::DEFAULT_WORKSPACE;
use crate::{
    commands::{create_workspace::copy_config_file, AppState},
    docker::{
        self,
        add_container,
        change_container_status,
        container_state,
        create_or_load_identities,
        create_workspace_folders,
        helpers::{create_password, process_stream},
        images_to_start,
        network_name,
        remove_container,
        tari_blockchain_volume_name,
        try_create_network,
        BaseNodeConfig,
        ContainerId,
        ContainerState,
        ContainerStatus,
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
    let config = LaunchpadConfig::try_from(settings.clone())?;
    let state = app.state::<AppState>();
    let app_config = app.config();
    let should_create_workspace = {
        let wrapper = state.workspaces.read().await;
        !wrapper.workspace_exists(DEFAULT_WORKSPACE)
    }; // drop read-only lock
    if should_create_workspace {
        let package_info = &state.package_info;
        create_workspace_folders(&config.data_directory)?;
        copy_config_file(&config.data_directory, app_config.as_ref(), package_info, "log4rs.yml")?;
        copy_config_file(&config.data_directory, app_config.as_ref(), package_info, "config.toml")?;
        // Only get a write-lock if we need one
        let mut wrapper = state.workspaces.write().await;
        wrapper.create_workspace(DEFAULT_WORKSPACE, config)?;
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
    let image = ImageType::try_from(service_name.as_str()).unwrap();
    let config = workspace.config().clone();
    start_container(image, config).await.map_err(|e| {
        let error = e.chained_message();
        error!("{}", error);
        error
    })?;

    let container_state = container_state(&service_name).map_err(|e| {
        let error = e.chained_message();
        error!("{}", error);
        error
    })?;
    let stats_events_name = stats_event_name(container_state.id());
    let log_events_name = log_event_name(container_state.name());
    info!("==> stats_events_name: {}", stats_events_name);
    info!("==> log_events_name: {}", log_events_name);
    let log_error_destination = format!("{}_error", log_events_name);
    let stats_error_destination = format!("{}_error", stats_events_name);

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
        stats_error_destination,
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
        process_stream(send_message, send_error, event_name.clone(), error_destination, stream).await;
        info!("Resource stats stream for {} has closed.", event_name);
    });
    info!("Resource stats events configured.");
}

pub async fn create_workspace(app: AppHandle<Wry>, settings: ServiceSettings) -> Result<(), LauncherError> {
    let state = app.state::<AppState>();

    let _ = create_default_workspace_impl(app.clone(), settings.clone()).await?;
    let mut wrapper = state.workspaces.write().await;
    // We've just checked this, so it should never fail:
    let workspace: &mut TariWorkspace = wrapper
        .get_workspace_mut(settings.tari_network.as_str())
        .ok_or(DockerWrapperError::UnexpectedError)?;
    let launchpad_config = workspace.config().clone();
    // Check the identity requirements for the service
    let ids = create_or_load_identities(launchpad_config.clone())?;
    for id in ids.values() {
        debug!("Identity loaded: {}", id);
    }
    try_create_network(launchpad_config.tari_network.lower_case()).await?;
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
        process_stream(send_message, send_error, message_destination, error_destination, stream).await;
        info!("Resource logs stream for has closed.");
    });
    info!("Resource logs events configured.");
}

pub async fn stop_service_impl(service_name: String) -> Result<(), LauncherError> {
    stop_container(service_name.as_str(), true).await;
    Ok(())
}

/// Stop all running containers and optionally delete them
pub async fn stop_containers(delete: bool) -> Result<(), DockerWrapperError> {
    let names: Vec<String> = CONTAINERS
        .read()
        .unwrap()
        .clone()
        .into_iter()
        .filter(|(_k, v)| ((*v).clone()).status() == ContainerStatus::Running)
        .map(|(k, _v)| k)
        .collect();

    info!("Stopping containers: [{:?}] ...", &names);
    for name in names.clone() {
        // Stop the container immediately
        info!("Stopping container: {} ...", name);
        stop_container(name.as_str(), delete).await;
    }
    Ok(())
}

/// Stop the container with the given `name` and optionally delete it
pub async fn stop_container(name: &str, delete: bool) {
    if let Ok(container) = remove_container(name) {
        let id = container.id();
        let options = StopContainerOptions { t: 0 };
        match &DOCKER_INSTANCE.stop_container(id.as_str(), Some(options)).await {
            Ok(_res) => {
                info!("Container {} stopped", id);
                if !delete {
                    let updated =
                        ContainerState::new(container.name().to_string(), id.clone(), container.info().clone());
                    add_container(name, updated);
                }
            },
            Err(err) => {
                warn!("Could not stop container {} due to {}", id, err.to_string());
            },
        }
    }
}

/// Create and run a docker container.
///
/// ## Arguments
/// * `image`: The type of image to start. See [`ImageType`].
/// * `registry`: An optional docker registry path to use. The default is `quay.io/tarilabs`
/// * `tag`: The image tag to use. The default is `latest`.
/// * `docker`: a [`Docker`] instance.
///
/// ## Return
///
/// The method returns a future that resolves to a [`DockerWrapperError`] on an error, or the container name on
/// success.
///
/// `start_service` creates a new docker container and runs it. As part of this process,
/// * it pulls configuration data from the [`LaunchConfig`] instance attached to this [`DockerWRapper`] to construct the
///   Environment, Volume configuration, and exposed Port configuration.
/// * creates a new container
/// * starts the container
/// * adds the container reference to the current list of containers being managed
/// * Returns the container name
pub async fn start_container(image: ImageType, config: LaunchpadConfig) -> Result<String, DockerWrapperError> {
    let args = config.command(image);
    let registry = config.clone().registry;
    let tag = config.clone().tag;
    let tari_network = config.tari_network.lower_case();
    let blockchain_volume = tari_blockchain_volume_name(DEFAULT_WORKSPACE, tari_network);
    let docker_network = network_name(tari_network);
    let image_name = TariWorkspace::fully_qualified_image(image, registry.as_deref(), tag.as_deref());

    let options = Some(CreateContainerOptions {
        name: format!("{}_{}", image.container_name(), image.image_name()),
    });
    let envars = config.environment(image);
    let volumes = config.volumes(image);
    let ports = config.ports(image);
    let port_map = config.port_map(image);
    let mounts = config.mounts(image, blockchain_volume);
    let mut endpoints = HashMap::new();
    let endpoint = EndpointSettings {
        aliases: Some(vec![image.container_name().to_string()]),
        ..Default::default()
    };
    endpoints.insert(docker_network, endpoint);
    let docker_config = Config::<String> {
        image: Some(image_name.clone()),
        attach_stdin: Some(false),
        attach_stdout: Some(false),
        attach_stderr: Some(false),
        exposed_ports: Some(ports),
        open_stdin: Some(true),
        stdin_once: Some(false),
        tty: Some(true),
        env: Some(envars),
        volumes: Some(volumes),
        cmd: Some(args),
        host_config: Some(HostConfig {
            binds: Some(vec![]),
            network_mode: Some("bridge".to_string()),
            port_bindings: Some(port_map),
            mounts: Some(mounts),
            ..Default::default()
        }),
        networking_config: Some(NetworkingConfig {
            endpoints_config: endpoints,
        }),
        ..Default::default()
    };
    info!("Creating {}", image_name);
    debug!("Options: {:?}", options);
    debug!("{} has configuration object: {:#?}", image_name, config);
    let container = DOCKER_INSTANCE.clone().create_container(options, docker_config).await?;
    let name = image.container_name();
    let id = container.id.clone();
    let id = ContainerId::from(id.clone());
    let state = ContainerState::new(name.to_string(), id.clone(), container.clone());
    add_container(name, state);
    info!("Starting {}.", image_name);
    let _ = &DOCKER_INSTANCE.start_container::<String>(id.as_str(), None).await?;
    change_container_status(name, crate::docker::ContainerStatus::Running)?;
    info!("{} started with id {}", image_name, id);

    Ok(name.to_string())
}

pub async fn start_recipe(config: LaunchpadConfig) -> Result<(), DockerWrapperError> {
    // Set up the local network
    try_create_network(config.tari_network.lower_case()).await?;
    // Create or restart the volume

    for image in images_to_start(config.clone()) {
        // Start each container
        let name = start_container(image, config.clone()).await?;
        info!(
            "Docker container {} ({}) successfully started",
            image.image_name(),
            name
        );
    }
    Ok(())
}
