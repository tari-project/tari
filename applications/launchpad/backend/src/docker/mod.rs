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

mod container;
mod error;
mod filesystem;
mod models;
pub mod mounts;
mod settings;
mod workspace;
mod wrapper;

pub mod helpers;
use std::{collections::HashMap, sync::RwLock};

use bollard::{
    container::{Config, CreateContainerOptions, ListContainersOptions, NetworkingConfig, RemoveContainerOptions},
    image,
    models::{ContainerCreateResponse, EndpointSettings, HostConfig},
    Docker,
};
pub use container::{add_container, change_container_status, container_state, filter, remove_container};
pub use error::DockerWrapperError;
pub use filesystem::create_workspace_folders;
use futures::StreamExt;
use log::{debug, info};
pub use models::{ContainerId, ContainerState, ContainerStatus, ImageType, LogMessage, TariNetwork};
pub use settings::{
    BaseNodeConfig,
    LaunchpadConfig,
    MmProxyConfig,
    Sha3MinerConfig,
    WalletConfig,
    XmRigConfig,
    BASE_NODE_GRPC_ADDRESS_URL,
    DEFAULT_MINING_ADDRESS,
    DEFAULT_MONEROD_URL,
    WALLET_GRPC_ADDRESS_URL,
};
pub use workspace::{TariWorkspace, Workspaces};
pub use wrapper::DockerWrapper;

use crate::{
    commands::DEFAULT_IMAGES,
    grpc::{GrpcBaseNodeClient, SyncProgress, SyncProgressInfo, SyncType},
};

lazy_static! {
    pub static ref DOCKER_INSTANCE: Docker = Docker::connect_with_local_defaults().unwrap();
}

lazy_static! {
    pub static ref CONTAINERS: RwLock<HashMap<String, ContainerState>> = RwLock::new(HashMap::new());
}

pub static DEFAULT_WORKSPACE_NAME: &str = "default";

fn tari_blockchain_volume_name(tari_workspace: String, tari_network: TariNetwork) -> String {
    format!("{}_{}_volume", tari_workspace, tari_network.lower_case())
}

pub async fn try_create_container(
    image: ImageType,
    fully_qualified_image_name: String,
    tari_workspace: String,
    config: &LaunchpadConfig,
    docker: Docker,
) -> Result<ContainerCreateResponse, DockerWrapperError> {
    debug!("{} has configuration object: {:#?}", fully_qualified_image_name, config);
    let args = config.command(image);
    let envars = config.environment(image);
    let volumes = config.volumes(image);
    let ports = config.ports(image);
    let port_map = config.port_map(image);
    let mounts = config.mounts(
        image,
        tari_blockchain_volume_name(tari_workspace.clone(), config.tari_network),
    );
    let mut endpoints = HashMap::new();
    let endpoint = EndpointSettings {
        aliases: Some(vec![image.container_name().to_string()]),
        ..Default::default()
    };
    endpoints.insert(format!("{}_network", tari_workspace), endpoint);
    let options = Some(CreateContainerOptions {
        name: format!("{}_{}", tari_workspace, image.image_name()),
    });
    let config = Config::<String> {
        image: Some(fully_qualified_image_name.to_string()),
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

    Ok(docker.create_container(options, config).await?)
}

pub async fn try_destroy_container(image_name: &str, docker: Docker) -> Result<(), DockerWrapperError> {
    let mut list_container_filters = HashMap::new();
    list_container_filters.insert("name".to_string(), vec![image_name.to_string()]);
    debug!("Searching for container {}", image_name);
    let containers = &docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: list_container_filters,
            ..Default::default()
        }))
        .await?;

    for c_id in containers.iter() {
        if let Some(id) = c_id.id.clone() {
            debug!("Removing countainer {}", id);
            docker
                .remove_container(
                    id.as_str(),
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await?;
        }
    }
    Ok(())
}

pub async fn shutdown_all_containers(workspace_name: String, docker: &Docker) -> Result<(), DockerWrapperError> {
    for image in DEFAULT_IMAGES {
        let image_name = format!("{}_{}", workspace_name, image.image_name());
        match try_destroy_container(image_name.as_str(), docker.clone()).await {
            Ok(_) => info!("Docker image {} is being stopped.", image_name),
            Err(_) => debug!("Docker image {} has not been found", image_name),
        }
    }
    Ok(())
}
