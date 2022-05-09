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
mod network;
mod settings;
mod volume;
mod workspace;
mod wrapper;

pub mod helpers;

use std::{sync::RwLock, collections::HashMap};

use bollard::Docker;
pub use error::DockerWrapperError;
pub use filesystem::create_workspace_folders;
pub use models::{ContainerId, ContainerState, ContainerStatus, ImageType, LogMessage, TariNetwork};
pub use settings::{
    BaseNodeConfig,
    LaunchpadConfig,
    MmProxyConfig,
    Sha3MinerConfig,
    WalletConfig,
    XmRigConfig,
    DEFAULT_MINING_ADDRESS,
    DEFAULT_MONEROD_URL,
};
pub use workspace::{TariWorkspace, Workspaces, 
    images_to_start, create_or_load_identities};
pub use wrapper::DockerWrapper;
pub use network::{connect_to_network, try_create_network, network_name};
pub use volume::tari_blockchain_volume_name;
pub use container::{add_container, change_container_status, container_state, remove_container};

lazy_static! {
    pub static ref DOCKER_INSTANCE: Docker = Docker::connect_with_local_defaults().unwrap();
}

lazy_static! {
    pub static ref CONTAINERS: RwLock<HashMap<String,  ContainerState>> = RwLock::new(HashMap::new());
}