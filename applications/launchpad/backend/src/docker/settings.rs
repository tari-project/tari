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

use std::{collections::HashMap, path::PathBuf, time::Duration};

use bollard::models::{Mount, MountTypeEnum, PortBinding, PortMap};
use config::ConfigError;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tor_hash_passwd::EncryptedKey;

use crate::docker::{models::ImageType, TariNetwork};

// TODO get a proper mining address for each network
pub const DEFAULT_MINING_ADDRESS: &str =
    "5AJ8FwQge4UjT9Gbj4zn7yYcnpVQzzkqr636pKto59jQcu85CFsuYVeFgbhUdRpiPjUCkA4sQtWApUzCyTMmSigFG2hDo48";

pub const DEFAULT_MONEROD_URL: &str = "http://monero-stagenet.exan.tech:38081";

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct BaseNodeConfig {
    /// The time delay before starting the container and running the base node executable
    pub delay: Duration,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct WalletConfig {
    /// The time delay before starting the container and running the wallet executable
    pub delay: Duration,
    /// The password to de/en-crypt the wallet database
    pub password: String,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct XmRigConfig {
    /// The time delay before starting the container and running the monero miner executable
    pub delay: Duration,
    /// The address that will accept Monero mining rewards
    pub monero_mining_address: String,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Sha3MinerConfig {
    /// The time delay before starting the container and running the SHA3 CPU miner executable
    pub delay: Duration,
    /// The number of threads to employ for SHA3 mining
    pub num_mining_threads: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MmProxyConfig {
    /// The time delay before starting the container and running the proxy executable
    pub delay: Duration,
    /// A URL specifying the Monero daemon to connect to
    pub monerod_url: String,
    /// If required, the monero username for the monero daemon
    pub monero_username: String,
    /// If required, the password needed to access the monero deamon
    pub monero_password: String,
    /// If true, provide the monero username and password to the daemon. Otherwise those strings are ignored.
    pub monero_use_auth: bool,
}

impl Default for MmProxyConfig {
    fn default() -> Self {
        MmProxyConfig {
            delay: Duration::from_secs(5),
            monerod_url: DEFAULT_MONEROD_URL.to_string(),
            monero_username: "".to_string(),
            monero_password: "".to_string(),
            monero_use_auth: false,
        }
    }
}

impl MmProxyConfig {
    pub fn monero_use_auth(&self) -> usize {
        if self.monero_use_auth {
            1
        } else {
            0
        }
    }
}

/// Tari Launchpad configuration struct. This will generally be populated from some front-end or persistent storage
/// file and is used to generate the environment variables needed to configure and run the various docker containers.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct LaunchpadConfig {
    /// The directory to use for config, id files and logs
    pub data_directory: PathBuf,
    /// The Tri network to use. Default = weatherwax
    pub tari_network: TariNetwork,
    /// The tor control password to share among containers.
    pub tor_control_password: String,
    /// Whether to spin up a base node or not, with the given configuration. Usually you want this.
    pub base_node: Option<BaseNodeConfig>,
    /// Whether to spin up a console wallet daemon, with the given configuration. Optional.
    pub wallet: Option<WalletConfig>,
    /// Whether to spin up a SHA3 miner or not, with the given configuration. If you want to mine Tari natively,
    /// include this.
    pub sha3_miner: Option<Sha3MinerConfig>,
    /// Whether to spin up a merge-mine proxy or not, with the given configuration. If included, you must also include
    /// xmrig
    pub mm_proxy: Option<MmProxyConfig>,
    /// Whether to spin up a Monero miner or not, with the given configuration. If included you should also include
    /// mm_proxy
    pub xmrig: Option<XmRigConfig>,
    /// The Docker registry to use to download images. By default we use quay.io
    pub registry: Option<String>,
    /// The docker tag to use. By default, we use 'latest'
    pub tag: Option<String>,
}

impl LaunchpadConfig {
    pub fn load() -> Result<Self, ConfigError> {
        unimplemented!()
    }

    /// Returns a list of environment variables that need to be set in the running container. For Tari containers, we
    /// use this to override settings in the `config.yml` file that are generated on the fly here (the tor control
    /// port password for example).
    pub fn environment(&self, image_type: ImageType) -> Vec<String> {
        match image_type {
            ImageType::BaseNode => self.base_node_environment(),
            ImageType::Wallet => self.wallet_environment(),
            ImageType::XmRig => self.xmrig_environment(),
            ImageType::Sha3Miner => self.sha3_miner_environment(),
            ImageType::MmProxy => self.mm_proxy_environment(),
            ImageType::Tor => self.tor_environment(),
            ImageType::Monerod => self.monerod_environment(),
            ImageType::Frontail => self.common_envars(),
        }
    }

    /// Provides a hashmap that bollard needs to mount the volumes we want for each image type.
    pub fn volumes(&self, image_type: ImageType) -> HashMap<String, HashMap<(), ()>> {
        match image_type {
            ImageType::BaseNode => self.build_volumes(true, true),
            ImageType::Wallet => self.build_volumes(true, false),
            ImageType::XmRig => self.build_volumes(true, false),
            ImageType::Sha3Miner => self.build_volumes(true, false),
            ImageType::MmProxy => self.build_volumes(true, false),
            ImageType::Tor => self.build_volumes(false, false),
            ImageType::Monerod => self.build_volumes(false, false),
            ImageType::Frontail => self.build_volumes(true, false),
        }
    }

    /// Similar to [`volumes`], provides a bollard configuration for mounting volumes.
    pub fn mounts(&self, image_type: ImageType, volume_name: String) -> Vec<Mount> {
        match image_type {
            ImageType::BaseNode => self.build_mounts(true, true, volume_name),
            ImageType::Wallet => self.build_mounts(true, true, volume_name),
            ImageType::XmRig => self.build_mounts(false, true, volume_name),
            ImageType::Sha3Miner => self.build_mounts(false, true, volume_name),
            ImageType::MmProxy => self.build_mounts(false, true, volume_name),
            ImageType::Tor => self.build_mounts(false, false, volume_name),
            ImageType::Monerod => self.build_mounts(false, false, volume_name),
            ImageType::Frontail => self.build_mounts(false, true, volume_name),
        }
    }

    fn build_mounts(&self, blockchain: bool, general: bool, volume_name: String) -> Vec<Mount> {
        let mut mounts = Vec::with_capacity(2);
        if general {
            #[cfg(not(target_os = "linux"))]
            let host = format!("/host_mnt{}", self.data_directory.to_string_lossy());
            #[cfg(target_os = "linux")]
            let host = self.data_directory.to_string_lossy().to_string();
            let mount = Mount {
                target: Some("/var/tari".to_string()),
                source: Some(host),
                typ: Some(MountTypeEnum::BIND),
                bind_options: None,
                ..Default::default()
            };
            mounts.push(mount);
        }
        if blockchain {
            let mount = Mount {
                target: Some("/blockchain".to_string()),
                source: Some(volume_name),
                typ: Some(MountTypeEnum::VOLUME),
                volume_options: None,
                ..Default::default()
            };
            mounts.push(mount);
        }
        mounts
    }

    /// Returns a map of ports to expose to the host system. TODO - remove the hardcoding so that multiple workspaces
    /// don't have colliding exposed ports.
    pub fn ports(&self, image_type: ImageType) -> HashMap<String, HashMap<(), ()>> {
        match image_type {
            ImageType::BaseNode => create_port_map(&["18142", "18189"]),
            ImageType::Wallet => create_port_map(&["18143", "18188"]),
            ImageType::XmRig => create_port_map(&[]),
            ImageType::Sha3Miner => create_port_map(&[]),
            ImageType::MmProxy => create_port_map(&[]),
            ImageType::Tor => create_port_map(&[]),
            ImageType::Monerod => create_port_map(&[]),
            ImageType::Frontail => create_port_map(&["18130"]),
        }
    }

    /// As for [`ports`] returns a bollard configuration for port mappings.
    pub fn port_map(&self, image_type: ImageType) -> PortMap {
        let ports = self.ports(image_type);
        ports
            .into_iter()
            .map(|(k, _)| {
                let binding = vec![PortBinding {
                    host_ip: Some("".to_string()),
                    host_port: Some(k.clone()),
                }];
                (k, Some(binding))
            })
            .collect()
    }

    /// Return the command line arguments we want for the given container execution.
    pub fn command(&self, image_type: ImageType) -> Vec<String> {
        match image_type {
            ImageType::BaseNode => vec!["--non-interactive-mode".to_string()],
            ImageType::Wallet => vec!["--non-interactive-mode".to_string()],
            ImageType::XmRig => self.xmrig_cmd(),
            ImageType::Sha3Miner => vec![],
            ImageType::MmProxy => vec![],
            ImageType::Tor => self.tor_cmd(),
            ImageType::Monerod => self.monerod_cmd(),
            ImageType::Frontail => self.frontail_cmd(),
        }
    }

    /// Returns the canonical path to the id files. The canonical path is defined as
    /// `{root_path}/config/{network}/{image_type}_id.json`
    pub fn id_path(&self, root_path: &str, image_type: ImageType) -> Option<PathBuf> {
        match image_type {
            ImageType::BaseNode | ImageType::Wallet => Some(
                PathBuf::from(root_path)
                    .join("config")
                    .join(self.tari_network.lower_case())
                    .join(format!("{}_id.json", image_type.image_name())),
            ),
            _ => None,
        }
    }

    fn frontail_cmd(&self) -> Vec<String> {
        let args = vec![
            "-p",
            "18130",
            "base_node/log/core.log",
            "wallet/log/core.log",
            "sha3_miner/log/core.log",
            "mm_proxy/log/core.log",
        ];
        args.into_iter().map(String::from).collect()
    }

    fn xmrig_cmd(&self) -> Vec<String> {
        let address = match &self.xmrig {
            Some(config) => config.monero_mining_address.as_str(),
            None => DEFAULT_MINING_ADDRESS,
        };
        let address = format!("--user={}", address);
        let args = vec![
            "--url=mm_proxy:18081",
            address.as_str(),
            "--coin=monero",
            "--daemon",
            "--log-file=/var/tari/xmrig/xmrig.log",
            "--verbose",
            // "--background"
        ];
        args.into_iter().map(String::from).collect()
    }

    fn monerod_cmd(&self) -> Vec<String> {
        let network = match self.tari_network {
            TariNetwork::Mainnet => "--mainnet",
            _ => "--stagenet",
        };
        let args = vec![
            "--non-interactive",
            "--restricted-rpc",
            "--rpc-bind-ip=0.0.0.0",
            "--confirm-external-bind",
            "--enable-dns-blocklist",
            "--log-file=/home/monerod/monerod.log",
            "--fast-block-sync=1",
            "--prune-blockchain",
            network,
        ];
        args.into_iter().map(String::from).collect()
    }

    fn tor_cmd(&self) -> Vec<String> {
        let hashed_password = EncryptedKey::hash_password(self.tor_control_password.as_str()).to_string();
        let args = vec![
            "/usr/bin/tor",
            "--SocksPort",
            "0.0.0.0:9050",
            "--ControlPort",
            "0.0.0.0:9051",
            "--CookieAuthentication",
            "0",
            "--ClientOnly",
            "1",
            "--ClientUseIPv6",
            "1",
            "--HashedControlPassword",
            hashed_password.as_str(),
        ];
        args.into_iter().map(String::from).collect()
    }

    /// Returns the bollard configuration map. You can specify any/all of the host-mounted data folder, of the
    /// blockchain folder to map.
    pub fn build_volumes(&self, general: bool, tari_blockchain: bool) -> HashMap<String, HashMap<(), ()>> {
        let mut volumes = HashMap::new();
        if general {
            volumes.insert("/var/tari".to_string(), HashMap::<(), ()>::new());
        }
        if tari_blockchain {
            volumes.insert("/blockchain".to_string(), HashMap::new());
        }
        volumes
    }

    fn common_envars(&self) -> Vec<String> {
        vec![
            format!("TARI_NETWORK={}", self.tari_network.lower_case()),
            format!("DATA_FOLDER={}", self.data_directory.to_str().unwrap_or("")), // TODO deal with None
            "TARI_LOG_CONFIGURATION=/var/tari/config/log4rs.yml".to_string(),
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
        ]
    }

    /// Generate the vector of ENVAR strings for the docker environment
    fn base_node_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        if let Some(base_node) = &self.base_node {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", base_node.delay.as_secs()),
                format!(
                    "TARI_BASE_NODE__{}__TOR_CONTROL_AUTH=password={}",
                    self.tari_network.upper_case(),
                    self.tor_control_password
                ),
                format!(
                    "TARI_BASE_NODE__{}__DATA_DIR=/blockchain/{}",
                    self.tari_network.upper_case(),
                    self.tari_network.lower_case()
                ),
                format!(
                    "TARI_BASE_NODE__{}__TOR_FORWARD_ADDRESS=/dns4/base_node/tcp/18189",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_BASE_NODE__{}__TCP_LISTENER_ADDRESS=/dns4/base_node/tcp/18189",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_BASE_NODE__{}__TOR_SOCKS_ADDRESS_OVERRIDE=/dns4/tor/tcp/9050",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_BASE_NODE__{}__TOR_CONTROL_ADDRESS=/dns4/tor/tcp/9051",
                    self.tari_network.upper_case()
                ),
                format!("TARI_BASE_NODE__{}__GRPC_ENABLED=1", self.tari_network.upper_case()),
                format!(
                    "TARI_BASE_NODE__{}__GRPC_BASE_NODE_ADDRESS=0.0.0.0:18142",
                    self.tari_network.upper_case()
                ),
                "APP_NAME=base_node".to_string(),
            ]);
        }
        env
    }

    fn wallet_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        if let Some(config) = &self.wallet {
            env.append(&mut vec![
                "APP_NAME=wallet".to_string(),
                "APP_EXEC=tari_console_wallet".to_string(),
                format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 3),
                "SHELL=/bin/bash".to_string(),
                "TERM=linux".to_string(),
                format!("TARI_WALLET_PASSWORD={}", config.password),
                format!(
                    "TARI_WALLET__{}__TOR_CONTROL_AUTH=password={}",
                    self.tari_network.upper_case(),
                    self.tor_control_password
                ),
                format!(
                    "TARI_WALLET__{}__TOR_CONTROL_ADDRESS=/dns4/tor/tcp/9051",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_WALLET__{}__TOR_SOCKS_ADDRESS_OVERRIDE=/dns4/tor/tcp/9050",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_WALLET__{}__TOR_FORWARD_ADDRESS=/dns4/wallet/tcp/18188",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_WALLET__{}__TCP_LISTENER_ADDRESS=/dns4/wallet/tcp/18188",
                    self.tari_network.upper_case()
                ),
                format!(
                    "TARI_BASE_NODE__{}__GRPC_CONSOLE_WALLET_ADDRESS=0.0.0.0:18143",
                    self.tari_network.upper_case()
                ),
                "TARI_WALLET__GRPC_ADDRESS=0.0.0.0:18143".to_string(),
            ]);
        }
        env
    }

    fn xmrig_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        if let Some(config) = &self.xmrig {
            env.append(&mut vec![format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 9)]);
        }
        env
    }

    fn sha3_miner_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        if let Some(config) = &self.sha3_miner {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 6),
                "APP_NAME: sha3_miner".to_string(),
                "APP_EXEC: tari_mining_node".to_string(),
                format!("TARI_MINING_NODE__NUM_MINING_THREADS: {}", config.num_mining_threads),
                "TARI_MINING_NODE__MINE_ON_TIP_ONLY: 1".to_string(),
                format!(
                    "TARI_BASE_NODE__{}__GRPC_BASE_NODE_ADDRESS=/dns4/base_node/tcp/18142",
                    self.tari_network.upper_case()
                ),
                "TARI_WALLET__GRPC_ADDRESS=/dns4/wallet/tcp/18143".to_string(),
            ]);
        }
        env
    }

    fn mm_proxy_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        if let Some(config) = &self.mm_proxy {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 6),
                "APP_NAME=mm_proxy".to_string(),
                "APP_EXEC=tari_merge_mining_proxy".to_string(),
                format!(
                    "TARI_BASE_NODE__{}__GRPC_BASE_NODE_ADDRESS=/dns4/base_node/tcp/18142",
                    self.tari_network.upper_case()
                ),
                "TARI_WALLET__GRPC_ADDRESS=/dns4/wallet/tcp/18143".to_string(),
                format!(
                    "TARI_MERGE_MINING_PROXY__{}__MONEROD_URL={}",
                    self.tari_network.upper_case(),
                    config.monerod_url
                ),
                format!(
                    "TARI_MERGE_MINING_PROXY__{}__MONEROD_USERNAME={}",
                    self.tari_network.upper_case(),
                    config.monero_username
                ),
                format!(
                    "TARI_MERGE_MINING_PROXY__{}__MONEROD_PASSWORD={}",
                    self.tari_network.upper_case(),
                    config.monero_password
                ),
                format!(
                    "TARI_MERGE_MINING_PROXY__{}__MONEROD_USE_AUTH={}",
                    self.tari_network.upper_case(),
                    config.monero_use_auth()
                ),
                format!(
                    "TARI_MERGE_MINING_PROXY__{}__PROXY_HOST_ADDRESS=0.0.0.0:18081",
                    self.tari_network.upper_case()
                ),
            ]);
        }
        env
    }

    fn tor_environment(&self) -> Vec<String> {
        self.common_envars()
    }

    fn monerod_environment(&self) -> Vec<String> {
        self.common_envars()
    }
}

#[derive(Debug, Error)]
pub enum LaunchpadConfigError {}

fn create_port_map(ports: &[&'static str]) -> HashMap<String, HashMap<(), ()>> {
    let mut result = HashMap::new();
    for &port in ports {
        result.insert(format!("{}/tcp", port), HashMap::new());
    }
    result
}
