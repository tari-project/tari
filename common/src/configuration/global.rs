// Copyright 2019. The Tari Project
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
//! # Global configuration of tari base layer system

use super::ConfigurationError;
use config::{Config, Environment};
use multiaddr::Multiaddr;
use std::{
    convert::TryInto,
    fmt::{Display, Formatter, Result as FormatResult},
    net::SocketAddr,
    num::{NonZeroU16, TryFromIntError},
    path::PathBuf,
    str::FromStr,
};

//-------------------------------------        Main Configuration Struct      --------------------------------------//

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub network: Network,
    pub comms_transport: CommsTransport,
    pub listnener_liveness_max_sessions: usize,
    pub listener_liveness_whitelist_cidrs: Vec<String>,
    pub data_dir: PathBuf,
    pub db_type: DatabaseType,
    pub orphan_storage_capacity: usize,
    pub pruning_horizon: u64,
    pub pruned_mode_cleanup_interval: u64,
    pub core_threads: usize,
    pub blocking_threads: usize,
    pub identity_file: PathBuf,
    pub public_address: Multiaddr,
    pub grpc_enabled: bool,
    pub grpc_address: SocketAddr,
    pub peer_seeds: Vec<String>,
    pub peer_db_path: PathBuf,
    pub block_sync_strategy: String,
    pub enable_mining: bool,
    pub num_mining_threads: usize,
    pub tor_identity_file: PathBuf,
    pub wallet_db_file: PathBuf,
    pub wallet_identity_file: PathBuf,
    pub wallet_tor_identity_file: PathBuf,
    pub wallet_peer_db_path: PathBuf,
}

impl GlobalConfig {
    pub fn convert_from(mut cfg: Config) -> Result<Self, ConfigurationError> {
        let network = cfg
            .get_str("base_node.network")
            .map_err(|e| ConfigurationError::new("base_node.network", &e.to_string()))?
            .parse()?;

        // Add in settings from the environment (with a prefix of TARI_NODE)
        // Eg.. `TARI_NODE_DEBUG=1 ./target/app` would set the `debug` key
        cfg.merge(Environment::with_prefix("tari"))
            .map_err(|e| ConfigurationError::new("environment variable", &e.to_string()))?;
        convert_node_config(network, cfg)
    }
}

fn convert_node_config(network: Network, cfg: Config) -> Result<GlobalConfig, ConfigurationError> {
    let net_str = network.to_string().to_lowercase();

    let key = config_string(&net_str, "db_type");
    let db_type = cfg
        .get_str(&key)
        .map(|s| s.to_lowercase())
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    let key = config_string(&net_str, "data_dir");
    let data_dir: PathBuf = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let db_type = match db_type.as_str() {
        "memory" => Ok(DatabaseType::Memory),
        "lmdb" => Ok(DatabaseType::LMDB(data_dir.join("db"))),
        invalid_opt => Err(ConfigurationError::new(
            "base_node.db_type",
            &format!("Invalid option: {}", invalid_opt),
        )),
    }?;

    let key = config_string(&net_str, "orphan_storage_capacity");
    let orphan_storage_capacity = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = config_string(&net_str, "pruning_horizon");
    let pruning_horizon = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64;

    let key = config_string(&net_str, "pruned_mode_cleanup_interval");
    let pruned_mode_cleanup_interval = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as u64;

    // Thread counts
    let key = config_string(&net_str, "core_threads");
    let core_threads = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    let key = config_string(&net_str, "blocking_threads");
    let blocking_threads = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    // NodeIdentity path
    let key = config_string(&net_str, "identity_file");
    let identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Wallet identity path
    let key = config_string(&net_str, "wallet_identity_file");
    let wallet_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let key = config_string(&net_str, "wallet_tor_identity_file");
    let wallet_tor_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Tor private key persistence
    let key = config_string(&net_str, "tor_identity_file");
    let tor_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    // Transport
    let comms_transport = network_transport_config(&cfg, &net_str)?;

    // Public address
    let key = config_string(&net_str, "public_address");
    let public_address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|addr| {
            addr.parse::<Multiaddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    // GPRC enabled
    let key = config_string(&net_str, "grpc_enabled");
    let grpc_enabled = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as bool;

    let key = config_string(&net_str, "grpc_address");
    let grpc_address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        .and_then(|addr| {
            addr.parse::<SocketAddr>()
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))
        })?;

    // Peer seeds
    let key = config_string(&net_str, "peer_seeds");
    let peer_seeds = cfg
        .get_array(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
    let peer_seeds = peer_seeds.into_iter().map(|v| v.into_str().unwrap()).collect();

    // Peer DB path
    let peer_db_path = data_dir.join("peer_db");
    let wallet_peer_db_path = data_dir.join("wallet_peer_db");

    let key = config_string(&net_str, "block_sync_strategy");
    let block_sync_strategy = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

    // set base node mining
    let key = config_string(&net_str, "enable_mining");
    let enable_mining = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as bool;

    let key = config_string(&net_str, "num_mining_threads");
    let num_mining_threads = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;

    // set wallet_file
    let key = "wallet.wallet_file".to_string();
    let wallet_db_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?
        .into();

    let key = "common.liveness_max_sessions";
    let liveness_max_sessions = cfg
        .get_int(key)
        .map_err(|e| ConfigurationError::new(key, &e.to_string()))?
        .try_into()
        .map_err(|e: TryFromIntError| ConfigurationError::new(&key, &e.to_string()))?;

    let key = "common.liveness_whitelist_cidrs";
    let liveness_whitelist_cidrs = cfg
        .get_array(key)
        .map(|values| values.iter().map(ToString::to_string).collect())
        .unwrap_or_else(|_| vec!["127.0.0.1/32".to_string()]);

    Ok(GlobalConfig {
        network,
        comms_transport,
        listnener_liveness_max_sessions: liveness_max_sessions,
        listener_liveness_whitelist_cidrs: liveness_whitelist_cidrs,
        data_dir,
        db_type,
        orphan_storage_capacity,
        pruning_horizon,
        pruned_mode_cleanup_interval,
        core_threads,
        blocking_threads,
        identity_file,
        public_address,
        grpc_enabled,
        grpc_address,
        peer_seeds,
        peer_db_path,
        block_sync_strategy,
        enable_mining,
        num_mining_threads,
        tor_identity_file,
        wallet_identity_file,
        wallet_db_file,
        wallet_tor_identity_file,
        wallet_peer_db_path,
    })
}

fn network_transport_config(cfg: &Config, network: &str) -> Result<CommsTransport, ConfigurationError> {
    let get_conf_str = |key| {
        cfg.get_str(key)
            .map_err(|err| ConfigurationError::new(key, &err.to_string()))
    };

    let get_conf_multiaddr = |key| {
        let path_str = get_conf_str(key)?;
        path_str
            .parse::<Multiaddr>()
            .map_err(|err| ConfigurationError::new(key, &err.to_string()))
    };

    let transport_key = config_string(network, "transport");
    let transport = get_conf_str(&transport_key)?;

    match transport.to_lowercase().as_str() {
        "tcp" => {
            let key = config_string(network, "tcp_listener_address");
            let listener_address = get_conf_multiaddr(&key)?;
            let key = config_string(network, "tcp_tor_socks_address");
            let tor_socks_address = get_conf_multiaddr(&key).ok();
            let key = config_string(network, "tcp_tor_socks_auth");
            let tor_socks_auth = get_conf_str(&key).ok().and_then(|auth_str| auth_str.parse().ok());

            Ok(CommsTransport::Tcp {
                listener_address,
                tor_socks_auth,
                tor_socks_address,
            })
        },
        "tor" => {
            let key = config_string(network, "tor_control_address");
            let control_server_address = get_conf_multiaddr(&key)?;

            let key = config_string(network, "tor_control_auth");
            let auth_str = get_conf_str(&key)?;
            let auth = auth_str
                .parse()
                .map_err(|err: String| ConfigurationError::new(&key, &err))?;

            let key = config_string(network, "tor_forward_address");
            let forward_address = get_conf_multiaddr(&key)?;
            let key = config_string(network, "tor_onion_port");
            let onion_port = cfg
                .get::<NonZeroU16>(&key)
                .map_err(|err| ConfigurationError::new(&key, &err.to_string()))?;

            let key = config_string(network, "tor_socks_address_override");
            let socks_address_override = match get_conf_str(&key).ok() {
                Some(addr) => Some(
                    addr.parse::<Multiaddr>()
                        .map_err(|err| ConfigurationError::new(&key, &err.to_string()))?,
                ),
                None => None,
            };

            Ok(CommsTransport::TorHiddenService {
                control_server_address,
                auth,
                socks_address_override,
                forward_address,
                onion_port,
            })
        },
        "socks5" => {
            let key = config_string(network, "socks5_proxy_address");
            let proxy_address = get_conf_multiaddr(&key)?;

            let key = config_string(network, "socks5_auth");
            let auth_str = get_conf_str(&key)?;
            let auth = auth_str
                .parse()
                .map_err(|err: String| ConfigurationError::new(&key, &err))?;

            let key = config_string(network, "socks5_listener_address");
            let listener_address = get_conf_multiaddr(&key)?;

            Ok(CommsTransport::Socks5 {
                proxy_address,
                listener_address,
                auth,
            })
        },
        t => Err(ConfigurationError::new(
            &transport_key,
            &format!("Invalid transport type '{}'", t),
        )),
    }
}

fn config_string(network: &str, key: &str) -> String {
    format!("base_node.{}.{}", network, key)
}

//---------------------------------------------       Network type        ------------------------------------------//
#[derive(Clone, Debug, PartialEq, Copy)]
pub enum Network {
    MainNet,
    Rincewind,
}

impl FromStr for Network {
    type Err = ConfigurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "rincewind" => Ok(Self::Rincewind),
            "mainnet" => Ok(Self::MainNet),
            invalid => Err(ConfigurationError::new(
                "network",
                &format!("Invalid network option: {}", invalid),
            )),
        }
    }
}

impl Display for Network {
    fn fmt(&self, f: &mut Formatter) -> FormatResult {
        let msg = match self {
            Self::MainNet => "mainnet",
            Self::Rincewind => "rincewind",
        };
        f.write_str(msg)
    }
}

//---------------------------------------------      Database type        ------------------------------------------//
#[derive(Debug, Clone)]
pub enum DatabaseType {
    LMDB(PathBuf),
    Memory,
}

//---------------------------------------------     Network Transport     ------------------------------------------//
#[derive(Debug, Clone)]
pub enum TorControlAuthentication {
    None,
    Password(String),
}

fn parse_key_value(s: &str, split_chr: char) -> (String, Option<&str>) {
    let mut parts = s.splitn(2, split_chr);
    (
        parts
            .next()
            .expect("splitn always emits at least one part")
            .to_lowercase(),
        parts.next(),
    )
}

impl FromStr for TorControlAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(TorControlAuthentication::None),
            "password" => {
                let password = maybe_value.ok_or_else(|| {
                    "Invalid format for 'password' tor authentication type. It should be in the format \
                     'password=xxxxxx'."
                        .to_string()
                })?;
                Ok(TorControlAuthentication::Password(password.to_string()))
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SocksAuthentication {
    None,
    UsernamePassword(String, String),
}

impl FromStr for SocksAuthentication {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (auth_type, maybe_value) = parse_key_value(s, '=');
        match auth_type.as_str() {
            "none" => Ok(SocksAuthentication::None),
            "username_password" => {
                let (username, password) = maybe_value
                    .and_then(|value| {
                        let (un, pwd) = parse_key_value(value, ':');
                        // If pwd is None, return None
                        pwd.map(|p| (un, p))
                    })
                    .ok_or_else(|| {
                        "Invalid format for 'username-password' socks authentication type. It should be in the format \
                         'username_password=my_username:xxxxxx'."
                            .to_string()
                    })?;
                Ok(SocksAuthentication::UsernamePassword(username, password.to_string()))
            },
            s => Err(format!("Invalid tor auth type '{}'", s)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CommsTransport {
    /// Use TCP to join the Tari network. This transport can only communicate with TCP/IP addresses, so peers with
    /// e.g. tor onion addresses will not be contactable.
    Tcp {
        listener_address: Multiaddr,
        tor_socks_address: Option<Multiaddr>,
        tor_socks_auth: Option<SocksAuthentication>,
    },
    /// Configures the node to run over a tor hidden service using the Tor proxy. This transport recognises ip/tcp,
    /// onion v2, onion v3 and DNS addresses.
    TorHiddenService {
        /// The address of the control server
        control_server_address: Multiaddr,
        socks_address_override: Option<Multiaddr>,
        /// The address used to receive proxied traffic from the tor proxy to the Tari node. This port must be
        /// available
        forward_address: Multiaddr,
        auth: TorControlAuthentication,
        onion_port: NonZeroU16,
    },
    /// Use a SOCKS5 proxy transport. This transport recognises any addresses supported by the proxy.
    Socks5 {
        proxy_address: Multiaddr,
        auth: SocksAuthentication,
        listener_address: Multiaddr,
    },
}
