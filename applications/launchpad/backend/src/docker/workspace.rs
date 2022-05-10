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
        LogsOptions,
        NetworkingConfig,
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

use super::CONTAINERS;
use crate::{
    commands::stop_containers,
    docker::{
        container::{add_container, change_container_status},
        models::{ContainerId, ContainerState},
        DockerWrapperError,
        ImageType,
        LaunchpadConfig,
        LogMessage,
        DOCKER_INSTANCE,
    },
};

static DEFAULT_REGISTRY: &str = "quay.io/tarilabs";
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
        debug!("$$$$ workspace size {}", self.workspaces.len());
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
/// * Frontail - 18130
#[derive(Clone, Debug)]
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
    pub fn fully_qualified_image(image: ImageType, registry: Option<&str>, tag: Option<&str>) -> String {
        let reg = registry.unwrap_or(DEFAULT_REGISTRY);
        let tag = Self::arch_specific_tag(tag);
        format!("{}/{}:{}", reg, image.image_name(), tag)
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
}

// helper function for start recipe. This will be overhauled to be more flexible in future
pub fn images_to_start(config: LaunchpadConfig) -> Vec<ImageType> {
    let mut images = Vec::with_capacity(6);
    // Always use Tor for now
    images.push(ImageType::Tor);
    if config.base_node.is_some() {
        images.push(ImageType::BaseNode);
    }
    if config.wallet.is_some() {
        images.push(ImageType::Wallet);
    }
    if config.xmrig.is_some() {
        images.push(ImageType::XmRig);
    }
    if config.sha3_miner.is_some() {
        images.push(ImageType::Sha3Miner);
    }
    if config.mm_proxy.is_some() {
        images.push(ImageType::MmProxy);
    }
    // TODO - add monerod support
    images
}

/// Bootstraps a node identity for the container, typically a base node or wallet instance. If an identity file
/// already exists at the canonical path location, it is loaded and returned instead.
///
/// The canonical path is defined as `{root_path}/{image_type}/config/{network}/{image_type}_id.json`
pub fn create_or_load_identity(
    config: LaunchpadConfig,
    root_path: &str,
    image: ImageType,
) -> Result<Option<NodeIdentity>, DockerWrapperError> {
    if let Some(id_file_path) = config.id_path(root_path, image) {
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
pub fn create_or_load_identities(
    config: LaunchpadConfig,
) -> Result<HashMap<ImageType, NodeIdentity>, DockerWrapperError> {
    let root_path = config.data_directory.to_string_lossy().to_string();
    let mut ids = HashMap::new();
    for image in ImageType::iter() {
        if let Some(id) = create_or_load_identity(config.clone(), root_path.as_str(), image)? {
            let _node_identity = ids.insert(image, id);
        }
    }
    Ok(ids)
}
