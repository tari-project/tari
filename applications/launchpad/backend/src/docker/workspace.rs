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

use std::collections::HashMap;

use bollard::{
    container::{
        Config,
        CreateContainerOptions,
        ListContainersOptions,
        LogsOptions,
        NetworkingConfig,
        RemoveContainerOptions,
        Stats,
        StatsOptions,
        StopContainerOptions,
    },
    models::{ContainerCreateResponse, EndpointSettings, HostConfig, Network},
    network::{ConnectNetworkOptions, CreateNetworkOptions, InspectNetworkOptions},
    volume::CreateVolumeOptions,
    Docker,
};
use futures::{Stream, StreamExt, TryStreamExt};
use log::*;
use strum::IntoEnumIterator;
use tari_app_utilities::identity_management::setup_node_identity;
use tari_comms::{peer_manager::PeerFeatures, NodeIdentity};

use super::{add_container, container_state, CONTAINERS};
use crate::docker::{
    change_container_status,
    models::{ContainerId, ContainerState},
    try_create_container,
    try_destroy_container,
    ContainerStatus,
    DockerWrapperError,
    ImageType,
    LaunchpadConfig,
    LogMessage,
};

static DEFAULT_REGISTRY: &str = "quay.io/tarilabs";
static GRAFANA_REGISTRY: &str = "grafana";
static DEFAULT_TAG: &str = "latest";

/// `Workspaces` allows us to spin up multiple [TariWorkspace] recipes or configurations at once. Most users will only
/// ever need one at a time, and the default one at that, but developers and testers will likely find this useful.
///
/// For example, you might want to run a node and wallet on two different networks concurrently.
/// Or, you might want to spin up two wallets on the same network to test some new feature. Here you would have one
/// recipe with just a base node and a wallet, and another recipe with just a wallet.
///
/// Workspaces are referenced by a name, which is an arbitrary string, but by convention, you'll want to keep workspace
/// names short and do not use spaces in workspace names.
///
/// Docker containers are named using a `{workspace_name}_{image_type}` convention. See [TariWorkspace] for more
/// details.
///
/// Each workspace should also have a unique data folder to keep logs and configuration separate, but this is not a hard
/// requirement.
#[derive(Default)]
pub struct Workspaces {
    workspaces: HashMap<String, TariWorkspace>,
}

impl Workspaces {
    /// Returns a mutable reference to the `name`d workspace. For an immutable reference, see [workspace_mut].
    pub fn get_workspace_mut(&mut self, name: &str) -> Option<&mut TariWorkspace> {
        self.workspaces.get_mut(name)
    }

    /// Returns an immtable reference to the `name`d workspace. For a mutable reference, see [get_workspace_mut].
    pub fn get_workspace(&self, name: &str) -> Option<&TariWorkspace> {
        self.workspaces.get(name)
    }

    /// Checks if a workspace with the given name exists.
    pub fn workspace_exists(&self, name: &str) -> bool {
        self.workspaces.contains_key(name)
    }

    /// Create a new Tari workspace. It must not previously exist
    pub fn create_workspace(&mut self, name: &str, config: LaunchpadConfig) -> Result<(), DockerWrapperError> {
        if self.workspaces.contains_key(name) {
            return Err(DockerWrapperError::WorkspaceAlreadyExists(name.to_string()));
        }
        let new_system = TariWorkspace::new(name, config);
        self.workspaces.insert(name.to_string(), new_system);
        Ok(())
    }

    /// Gracefully shut down the docker images and delete them
    /// The volumes are kept, since if we restart, we don't want to re-sync the entire blockchain again
    pub async fn shutdown(&mut self, docker: &Docker) -> Result<(), DockerWrapperError> {
        for (name, system) in &mut self.workspaces {
            info!("Shutting down {}", name);
            system.stop_containers(true, docker).await;
        }
        Ok(())
    }
}

/// A TariWorkspace is a configuration of docker images (node, wallet, miner etc), configuration files and secrets,
/// and log files.
///
/// ### Data locations
/// A workspace manages data in several places
/// * **Blockchain data** is stored in a [Docker volume](https://docs.docker.com/storage/volumes/) that is not directly
///   accessible to the host system. This is primarily done for performance reasons, but should not be a major issue
///   since you seldom need direct access to the LMDB database. If you do need the database for some reason, you can
///   mount the volume and run a bash command, for instance: `docker run --rm -v $(pwd):/backup -v
///   blockchain:/blockchain ubuntu tar czvf /backup/backup.tar.gz /blockchain`. As currently written, the blockchain
///   data is namespaced by workspace but it is also possible in principle for different workspaces to (TODO) share
///   blockchain data (for the same network), thereby preventing the need to keep multiple copies of the blockchain. In
///   this case, you need to be careful not to introduce contention from multiple nodes running against the same LMDB
///   database, leading to corrupt db files.
/// * **Configuration** and log files are stored of the workspace `root_folder`. A typical folder structure looks like
/// ```text
///      {root_folder}
///      ├── base_node
///      │   └── log
///      │       ├── core.log
///      │       ├── network.log
///      │       └── other.log
///      ├── config
///      │   ├── config.toml
///      │   ├── log4rs.yml
///      │   └── dibbler
///      │       ├── base_node_tor.json
///      │       ├── tari_base_node_id.json
///      │       └── tari_console_wallet_id.json
///      ├── mm_proxy
///      ├── monerod
///      ├── sha3_miner
///      │   └── log
///      │       ├── core.log
///      │       ├── network.log
///      │       └── other.log
///      ├── tor
///      ├── wallet
///      │   ├── log
///      │   │   ├── core.log
///      │   │   ├── network.log
///      │   │   └── other.log
///      │   └── wallet
///      │       ├── console-wallet.dat
///      │       ├── console-wallet.dat-shm
///      │       └── console-wallet.dat-wal
///      └── xmrig
/// ```
/// the `{root_folder}` is mounted into the docker filesystem and is accessible to all containers in the workspace.
/// TODO - investigate security issues for this. In particular, we almost certainly want to isolate the wallet data
/// This means that changes to `config/log4rs.yml` take effect in all running containers immediately.
/// * **Docker containers** are ephemeral and are created and destroyed at will.
///
/// ### Networking
/// Tor is used by several other containers. The control password for Tor is randomly generated when the workspace is
/// set up, and shared as an environment variable between containers.
///
/// A dedicated, namespaced network is created for each workspace. This means that each container in the network can
/// talk to any other container in the same workspace, but not to containers in other workspaces.
///
/// We also expose some ports to the host system for host-container communication.
///
/// * Base node - ports 18142 and 18149. TODO - currently the same port is mapped to the host. For multiple Workspaces
///   we'd need to expose a different port to the host. e.g. 1n142 where n is the workspace number.
/// * Wallet - 18143 and 18188
/// * Loki (18130), promtail (18980) and grafana (18300) and
pub struct TariWorkspace {
    name: String,
    config: LaunchpadConfig,
}

impl TariWorkspace {
    /// Create a new Tari system using the provided configuration
    pub fn new(name: &str, config: LaunchpadConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
        }
    }

    /// Return a reference to the immutable configuration object for this workspace.
    pub fn config(&self) -> &LaunchpadConfig {
        &self.config
    }

    /// Returns the name of this system, which is generally used as the workspace name when running multiple Tari
    /// systems
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the full image name for the given image type. for the default registry this is typically something like
    /// `quay.io/tarilabs/tari_base_node:latest`. One should be able to use this string to do a successful
    /// `docker pull {image_name}`.
    ///
    /// It also lets power users customise which version of docker images they want to run in the workspace.
    pub fn fully_qualified_image(image: ImageType, registry: Option<&str>) -> String {
        let reg = match image {
            ImageType::Tor |
            ImageType::BaseNode |
            ImageType::Wallet |
            ImageType::XmRig |
            ImageType::Sha3Miner |
            ImageType::MmProxy |
            ImageType::Monerod => registry.unwrap_or(DEFAULT_REGISTRY),
            ImageType::Loki | ImageType::Promtail | ImageType::Grafana => GRAFANA_REGISTRY,
        };
        format!("{}/{}:{}", reg, image.image_name(), "latest")
    }

    /// Returns an architecture-specific tag based on the current CPU and the given label. e.g.
    ///  `arch_specific_tag(Some("v1.0"))` returns `"v1.0-arm64"` on M1 chips, and `v1.0-amd64` on Intel and AMD chips.
    pub fn arch_specific_tag(label: Option<&str>) -> String {
        let label = label.unwrap_or(DEFAULT_TAG);
        let platform = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            _ => "unsupported",
        };
        format!("{}-{}", label, platform)
    }

    /// Starts the Tari workspace recipe.
    ///
    /// This is an MVP / PoC version that starts everything in one go, but TODO, should really take some sort of recipe
    /// object to allow us to build up different recipes (wallet only, full miner, SHA3-mining only etc)
    pub async fn start_recipe(&mut self, docker: Docker) -> Result<(), DockerWrapperError> {
        // Create or load identities
        let _ids = self.create_or_load_identities()?;
        // Set up the local network
        if !self.network_exists(&docker).await? {
            self.create_network(&docker).await?;
        }
        // Create or restart the volume

        for image in self.images_to_start() {
            // Start each container
            let name = self.start_service(image, docker.clone()).await?;
            info!(
                "Docker container {} ({}) successfully started",
                image.image_name(),
                name
            );
        }
        Ok(())
    }

    /// Bootstraps a node identity for the container, typically a base node or wallet instance. If an identity file
    /// already exists at the canonical path location, it is loaded and returned instead.
    ///
    /// The canonical path is defined as `{root_path}/{image_type}/config/{network}/{image_type}_id.json`
    pub fn create_or_load_identity(
        &self,
        root_path: &str,
        image: ImageType,
    ) -> Result<Option<NodeIdentity>, DockerWrapperError> {
        if let Some(id_file_path) = self.config.id_path(root_path, image) {
            debug!("Loading or creating identity file {}", id_file_path.to_string_lossy());
            let id = setup_node_identity(id_file_path, None, true, PeerFeatures::COMMUNICATION_NODE)?
                .as_ref()
                .clone();
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    /// A convenience method that calls [create_or_load_identity] for each image type.
    pub fn create_or_load_identities(&self) -> Result<HashMap<ImageType, NodeIdentity>, DockerWrapperError> {
        let root_path = self.config.data_directory.to_string_lossy().to_string();
        let mut ids = HashMap::new();
        for image in ImageType::iter() {
            if let Some(id) = self.create_or_load_identity(root_path.as_str(), image)? {
                let _node_identity = ids.insert(image, id);
            }
        }
        Ok(ids)
    }

    /// Create and return a [`Stream`] of [`LogMessage`] instances for the `name`d container in the workspace.
    pub fn logs(
        &self,
        container_name: &str,
        docker: &Docker,
    ) -> Option<impl Stream<Item = Result<LogMessage, DockerWrapperError>>> {
        let options = LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            ..Default::default()
        };
        CONTAINERS.read().unwrap().get(container_name).map(move |container| {
            let id = container.id();
            docker
                .logs(id.as_str(), Some(options))
                .map(|log| log.map(LogMessage::from).map_err(DockerWrapperError::from))
        })
    }

    /// Returns a [`Stream`] of resource stats for the container `name`, if it exists
    pub fn resource_stats(
        &self,
        name: &str,
        docker: &Docker,
    ) -> Option<impl Stream<Item = Result<Stats, DockerWrapperError>>> {
        if let Some(container) = CONTAINERS.read().unwrap().get(name) {
            let options = StatsOptions {
                stream: true,
                one_shot: false,
            };
            let id = container.id();
            let stream = docker
                .stats(id.as_str(), Some(options))
                .map_err(DockerWrapperError::from);
            Some(stream)
        } else {
            None
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
    /// * it pulls configuration data from the [`LaunchConfig`] instance attached to this [`DockerWRapper`] to construct
    ///   the Environment, Volume configuration, and exposed Port configuration.
    /// * creates a new container
    /// * starts the container
    /// * adds the container reference to the current list of containers being managed
    /// * Returns the container name
    pub async fn start_service(&mut self, image: ImageType, docker: Docker) -> Result<String, DockerWrapperError> {
        let fully_qualified_image_name = TariWorkspace::fully_qualified_image(image, self.config.registry.as_deref());
        info!("Creating {}", &fully_qualified_image_name);
        let workspace_image_name = format!("{}_{}", self.name, image.image_name());
        let _unused = try_destroy_container(workspace_image_name.as_str(), docker.clone()).await;
        let container = try_create_container(
            image,
            fully_qualified_image_name.clone(),
            self.name().to_string(),
            &self.config,
            docker.clone(),
        )
        .await?;
        let name = image.container_name();
        let id = container.id.clone();
        self.add_container(name, container);

        info!("Starting {}.", fully_qualified_image_name);
        docker.start_container::<String>(id.as_str(), None).await?;

        self.mark_container_running(name)?;
        info!("{} started with id {}", fully_qualified_image_name, id);

        Ok(name.to_string())
    }

    // helper function for start recipe. This will be overhauled to be more flexible in future
    fn images_to_start(&self) -> Vec<ImageType> {
        let mut images = Vec::with_capacity(6);
        // Always use Tor for now
        images.push(ImageType::Tor);
        if self.config.base_node.is_some() {
            images.push(ImageType::BaseNode);
        }
        if self.config.wallet.is_some() {
            images.push(ImageType::Wallet);
        }
        if self.config.xmrig.is_some() {
            images.push(ImageType::XmRig);
        }
        if self.config.sha3_miner.is_some() {
            images.push(ImageType::Sha3Miner);
        }
        if self.config.mm_proxy.is_some() {
            images.push(ImageType::MmProxy);
        }
        // TODO - add monerod support
        images
    }

    /// Add the container info to the list of containers the wrapper is managing
    fn add_container(&mut self, name: &str, container: ContainerCreateResponse) {
        let id = ContainerId::from(container.id.clone());
        let state = ContainerState::new(name.to_string(), id, container);
        add_container(name, state);
    }

    // Tag the container with id `id` as Running
    fn mark_container_running(&mut self, name: &str) -> Result<(), DockerWrapperError> {
        if let Some(container) = CONTAINERS.write().unwrap().get_mut(name) {
            container.running();
            Ok(())
        } else {
            Err(DockerWrapperError::ContainerNotFound(name.to_string()))
        }
    }

    /// Stop the container with the given `name` and optionally delete it
    pub async fn stop_container(&mut self, name: &str, delete: bool, docker: &Docker) {
        let container = match container_state(name) {
            Some(c) => c,
            None => {
                info!(
                    "Cannot stop container {}. It was not found in the current workspace",
                    name
                );
                return;
            },
        };
        let options = StopContainerOptions { t: 0 };
        let id = container.id().clone();
        match docker.stop_container(id.as_str(), Some(options)).await {
            Ok(_res) => {
                info!("Container {} stopped", id.clone());
                match change_container_status(id.as_str(), ContainerStatus::Stopped) {
                    Ok(_) => (),
                    Err(err) => warn!(
                        "Could not update launchpad cache for {} due to: {}",
                        id,
                        err.to_string()
                    ),
                }
            },
            Err(err) => {
                warn!("Could not stop container {} due to {}", id, err.to_string());
            },
        }
        // Even if stopping failed (maybe it was already stopped), try and delete it
        if delete {
            match docker.remove_container(id.as_str(), None).await {
                Ok(()) => {
                    info!("Container {} deleted", id);
                    match change_container_status(id.as_str(), ContainerStatus::Deleted) {
                        Ok(_) => (),
                        Err(err) => warn!(
                            "Could not update launchpad cache for {} due to: {}",
                            id,
                            err.to_string()
                        ),
                    }
                },
                Err(err) => {
                    warn!("Could not delete container {} due to: {}", id, err.to_string())
                },
            }
        }
    }

    /// Stop all running containers and optionally delete them
    pub async fn stop_containers(&mut self, delete: bool, docker: &Docker) {
        let names = CONTAINERS.write().unwrap().keys().cloned().collect::<Vec<String>>();
        for name in names {
            // Stop the container immediately
            self.stop_container(name.as_str(), delete, docker).await;
        }
    }

    /// Returns the network name
    pub fn network_name(&self) -> String {
        format!("{}_network", self.name)
    }

    /// Returns the name of the volume holding blockchain data. Currently this is namespaced to the workspace. We might
    /// want to change this to be shareable across networks, i.e. only namespace across the network type.
    pub fn tari_blockchain_volume_name(&self) -> String {
        format!("{}_{}_volume", self.name, self.config.tari_network.lower_case())
    }

    /// Checks if the network for this docker configuration exists
    pub async fn network_exists(&self, docker: &Docker) -> Result<bool, DockerWrapperError> {
        let name = self.network_name();
        let options = InspectNetworkOptions {
            verbose: false,
            scope: "local",
        };
        let network = docker.inspect_network(name.as_str(), Some(options)).await;
        // hardcore pattern matching yo!
        if let Ok(Network {
            name: Some(name),
            id: Some(id),
            ..
        }) = network
        {
            info!("Network {} (id:{}) exists", name, id);
            Ok(true)
        } else {
            info!("Network {} does not exist", name);
            Ok(false)
        }
    }

    /// Create a network in docker to allow the containers in this workspace to communicate with each other.
    pub async fn create_network(&self, docker: &Docker) -> Result<(), DockerWrapperError> {
        let name = self.network_name();
        let options = CreateNetworkOptions {
            name: name.as_str(),
            check_duplicate: true,
            driver: "bridge",
            internal: false,
            attachable: false,
            ingress: false,
            ipam: Default::default(),
            enable_ipv6: false,
            options: Default::default(),
            labels: Default::default(),
        };
        let res = docker.create_network(options).await?;
        if let Some(id) = &res.id {
            info!("Network {} (id:{}) created", name, id);
        }
        if let Some(warn) = res.warning {
            warn!("Creating {} network had warnings: {}", name, warn);
        }
        Ok(())
    }

    /// Checks whether the blockchain data volume exists
    pub async fn volume_exists(&self, docker: &Docker) -> Result<bool, DockerWrapperError> {
        let name = self.tari_blockchain_volume_name();
        let volume = docker.inspect_volume(name.as_str()).await?;
        trace!("Volume {} exists at {}", name, volume.mountpoint);
        Ok(true)
    }

    /// Tries to create a new blockchain data volume for this workspace.
    pub async fn create_volume(&self, docker: &Docker) -> Result<(), DockerWrapperError> {
        let name = self.tari_blockchain_volume_name();
        let config = CreateVolumeOptions {
            name,
            driver: "local".to_string(),
            ..Default::default()
        };
        let volume = docker.create_volume(config).await?;
        info!("Docker volume {} created at {}", volume.name, volume.mountpoint);
        Ok(())
    }

    /// Connects a container to the workspace network. This is not typically needed, since the container will
    /// automatically be connected to the network when it is created in [`start_service`].
    pub async fn connect_to_network(
        &self,
        id: &ContainerId,
        image: ImageType,
        docker: &Docker,
    ) -> Result<(), DockerWrapperError> {
        let network = self.network_name();
        let options = ConnectNetworkOptions {
            container: id.as_str(),
            endpoint_config: EndpointSettings {
                aliases: Some(vec![image.container_name().to_string()]),
                ..Default::default()
            },
        };
        info!(
            "Connecting container {} ({}) to network {}...",
            image.image_name(),
            id,
            network
        );
        docker.connect_network(network.as_str(), options).await?;
        info!(
            "Docker container {} ({}) connected to network {}",
            image.image_name(),
            id,
            network
        );
        Ok(())
    }
}
