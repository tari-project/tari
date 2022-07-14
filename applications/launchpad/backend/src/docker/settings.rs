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
use derivative::Derivative;
use log::info;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tor_hash_passwd::EncryptedKey;

use crate::docker::{models::ImageType, mounts::Mounts, TariNetwork};

// TODO get a proper mining address for each network
pub const DEFAULT_MINING_ADDRESS: &str =
    "5AJ8FwQge4UjT9Gbj4zn7yYcnpVQzzkqr636pKto59jQcu85CFsuYVeFgbhUdRpiPjUCkA4sQtWApUzCyTMmSigFG2hDo48";

pub const DEFAULT_MONEROD_URL: &str = "http://stagenet.xmr-tw.org:38081,\
http://stagenet.community.xmr.to:38081,\
http://monero-stagenet.exan.tech:38081,\
http://xmr-lux.boldsuck.org:38081,\
http://singapore.node.xmr.pm:38081";

pub const WALLET_GRPC_ADDRESS_URL: &str = "http://127.0.0.1:18143";
pub const BASE_NODE_GRPC_ADDRESS_URL: &str = "http://127.0.0.1:18142";

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct BaseNodeConfig {
    /// The time delay before starting the container and running the base node executable
    pub delay: Duration,
}

#[derive(Default, Derivative, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct WalletConfig {
    /// The time delay before starting the container and running the wallet executable
    pub delay: Duration,
    /// The password to de/en-crypt the wallet database
    #[serde(skip_serializing)]
    #[derivative(Debug = "ignore")]
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

#[derive(Derivative, Serialize, Deserialize)]
#[derivative(Debug)]
pub struct MmProxyConfig {
    /// The time delay before starting the container and running the proxy executable
    pub delay: Duration,
    /// A URL specifying the Monero daemon to connect to
    pub monerod_url: String,
    /// If required, the monero username for the monero daemon
    pub monero_username: String,
    /// If required, the password needed to access the monero deamon
    #[serde(skip_serializing)]
    #[derivative(Debug = "ignore")]
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
    /// The Tari network to use. Default = dibbler
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
            ImageType::Loki => self.grafana_environment(),
            ImageType::Promtail => self.grafana_environment(),
            ImageType::Grafana => self.grafana_environment(),
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
            ImageType::Loki => self.build_grafana_volumes(),
            ImageType::Promtail => self.build_grafana_volumes(),
            ImageType::Grafana => self.build_grafana_volumes(),
        }
    }

    /// Similar to [`volumes`], provides a bollard configuration for mounting volumes.
    pub fn mounts(&self, image_type: ImageType, volume_name: String) -> Vec<Mount> {
        let mounts = match image_type {
            ImageType::BaseNode => Mounts::with_general(&self.data_directory).with_blockchain(volume_name),
            ImageType::Wallet => Mounts::with_general(&self.data_directory).with_blockchain(volume_name),
            ImageType::XmRig => Mounts::with_general(&self.data_directory),
            ImageType::Sha3Miner => Mounts::with_general(&self.data_directory),
            ImageType::MmProxy => Mounts::with_general(&self.data_directory),
            ImageType::Tor => Mounts::empty(),
            ImageType::Monerod => Mounts::empty(),
            ImageType::Loki => Mounts::with_general(&self.data_directory).bind(
                self.data_directory.join("config").join("loki_config.yml"),
                "/etc/loki/local-config.yaml",
            ),
            ImageType::Promtail => Mounts::with_general(&self.data_directory)
                .with_grafana(volume_name)
                .bind(
                    self.data_directory.join("config").join("promtail.config.yml"),
                    "/etc/promtail/config.yml",
                ),
            ImageType::Grafana => Mounts::with_general(&self.data_directory)
                .bind(
                    self.data_directory.join("config").join("defaults.ini"),
                    "/usr/share/grafana/conf/defaults.ini",
                )
                .bind(
                    self.data_directory.join("config").join("sources_provision.yml"),
                    "/etc/grafana/provisioning/datasources/all.yml",
                ),
        };
        mounts.into_docker_mounts()
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
            ImageType::Loki => create_port_map(&["18310"]),
            ImageType::Promtail => create_port_map(&["18980"]),
            ImageType::Grafana => create_port_map(&["18300"]),
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
            ImageType::BaseNode => self.base_node_cmd(),
            ImageType::Wallet => self.wallet_cmd(),
            ImageType::XmRig => self.xmrig_cmd(),
            ImageType::Sha3Miner => self.miner_cmd(),
            ImageType::MmProxy => self.mm_proxy_cmd(),
            ImageType::Tor => self.tor_cmd(),
            ImageType::Monerod => self.monerod_cmd(),
            ImageType::Loki => self.loki_cmd(),
            ImageType::Promtail => self.promtail_cmd(),
            ImageType::Grafana => self.grafana_cmd(),
        }
    }

    /// Returns the canonical path to the id files. The canonical path is defined as
    /// `{root_path}/{image_data_folder}/config/{network}/{image_type}_id.json`
    pub fn id_path(&self, root_path: &str, image_type: ImageType) -> Option<PathBuf> {
        match image_type {
            ImageType::BaseNode | ImageType::Wallet => Some(
                PathBuf::from(root_path)
                    .join(image_type.data_folder())
                    .join("config")
                    .join(self.tari_network.lower_case())
                    .join(format!("{}_id.json", image_type.image_name())),
            ),
            _ => None,
        }
    }

    fn base_node_cmd(&self) -> Vec<String> {
        let args = vec!["--log-config=/var/tari/config/log4rs.yml"];
        args.into_iter().map(String::from).collect()
    }

    fn wallet_cmd(&self) -> Vec<String> {
        let args = vec![
            "--log-config=/var/tari/config/log4rs.yml",
            "--seed-words-file=/var/tari/config/seed_words.txt",
        ];
        args.into_iter().map(String::from).collect()
    }

    fn miner_cmd(&self) -> Vec<String> {
        let args = vec!["--log-config=/var/tari/config/log4rs.yml"];
        args.into_iter().map(String::from).collect()
    }

    fn mm_proxy_cmd(&self) -> Vec<String> {
        let args = vec!["--log-config=/var/tari/config/log4rs.yml"];
        args.into_iter().map(String::from).collect()
    }

    fn xmrig_cmd(&self) -> Vec<String> {
        let args = vec![
            "--url=mm_proxy:18081",
            "--user=${TARI_MONERO_WALLET_ADDRESS}",
            "--coin=monero",
            "--daemon",
            "--log-file=/var/tari/xmrig/xmrig.log",
            "--verbose",
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
            "--allow-missing-torrc",
        ];
        args.into_iter().map(String::from).collect()
    }

    fn loki_cmd(&self) -> Vec<String> {
        let args = vec!["-config.file=/etc/loki/local-config.yaml"];
        args.into_iter().map(String::from).collect()
    }

    fn promtail_cmd(&self) -> Vec<String> {
        let args = vec!["-config.file=/etc/promtail/config.yml"];
        args.into_iter().map(String::from).collect()
    }

    fn grafana_cmd(&self) -> Vec<String> {
        vec![]
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

    pub fn build_grafana_volumes(&self) -> HashMap<String, HashMap<(), ()>> {
        let mut volumes = self.build_volumes(true, false);
        volumes.insert("/var/grafana".to_string(), HashMap::new());
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

    fn base_node_tor_config(&self, env: &mut Vec<String>) {
        env.append(&mut vec![format!(
            "TARI_BASE_NODE__P2P__TRANSPORT__TOR__CONTROL_AUTH=password={}",
            self.tor_control_password
        )]);
    }

    /// Generate the vector of ENVAR strings for the docker environment
    fn base_node_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        self.base_node_tor_config(&mut env);
        if let Some(base_node) = &self.base_node {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", base_node.delay.as_secs()),
                format!(
                    "TARI_BASE_NODE__DATA_DIR=/blockchain/{}",
                    self.tari_network.lower_case()
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
                    "TARI_WALLET__P2P__TRANSPORT__TOR__CONTROL_AUTH=password={}",
                    self.tor_control_password
                ),
            ]);
        }
        env
    }

    fn xmrig_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        let address = match &self.xmrig {
            Some(config) if config.monero_mining_address.len() > 12 => config.monero_mining_address.as_str(),
            _ => DEFAULT_MINING_ADDRESS,
        };
        if let Some(config) = &self.xmrig {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 9),
                format!("TARI_MONERO_WALLET_ADDRESS={}", address),
            ]);
        }
        env
    }

    fn sha3_miner_environment(&self) -> Vec<String> {
        let mut env = self.common_envars();
        self.base_node_tor_config(&mut env);
        if let Some(config) = &self.sha3_miner {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 6),
                "APP_NAME: sha3_miner".to_string(),
                "APP_EXEC: tari_miner".to_string(),
                format!("TARI_MINER__NUM_MINING_THREADS: {}", config.num_mining_threads),
                "TARI_MINER__MINE_ON_TIP_ONLY: 1".to_string(),
                // This setting should be made obsolete soon:
                format!(
                    "TARI_BASE_NODE__{}__BASE_NODE_GRPC_ADDRESS=/dns4/base_node/tcp/18142",
                    self.tari_network.upper_case()
                ),
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
        self.base_node_tor_config(&mut env);
        if let Some(config) = &self.mm_proxy {
            env.append(&mut vec![
                format!("WAIT_FOR_TOR={}", config.delay.as_secs() + 6),
                "APP_NAME=mm_proxy".to_string(),
                "APP_EXEC=tari_merge_mining_proxy".to_string(),
                format!("TARI_MERGE_MINING_PROXY__MONEROD_URL={}", config.monerod_url),
                format!("TARI_MERGE_MINING_PROXY__MONEROD_USERNAME={}", config.monero_username),
                format!("TARI_MERGE_MINING_PROXY__MONEROD_PASSWORD={}", config.monero_password),
                format!("TARI_MERGE_MINING_PROXY__MONEROD_USE_AUTH={}", config.monero_use_auth()),
            ]);
        }
        env
    }

    fn tor_environment(&self) -> Vec<String> {
        self.common_envars()
    }

    fn grafana_environment(&self) -> Vec<String> {
        vec![
            format!("DATA_FOLDER={}", self.data_directory.to_str().unwrap_or("")), // TODO deal with None
            "PATH=/usr/share/grafana/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
        ]
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
