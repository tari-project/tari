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

use crate::{dir_utils::default_subdir, ConfigBootstrap};
use config::{Config, Environment};
use log::*;
use multiaddr::{Multiaddr, Protocol};
use std::{
    convert::TryFrom,
    error::Error,
    fmt::{Display, Formatter, Result as FormatResult},
    fs,
    net::IpAddr,
    num::NonZeroU16,
    path::{Path, PathBuf},
    str::FromStr,
};

const LOG_TARGET: &str = "common::config";

//-------------------------------------           Main API functions         --------------------------------------//

pub fn load_configuration(bootstrap: &ConfigBootstrap) -> Result<Config, String> {
    debug!(
        target: LOG_TARGET,
        "Loading configuration file from  {}",
        bootstrap.config.to_str().unwrap_or("[??]")
    );
    let mut cfg = default_config();
    // Load the configuration file
    let filename = bootstrap
        .config
        .to_str()
        .ok_or_else(|| "Invalid config file path".to_string())?;
    let config_file = config::File::with_name(filename);
    match cfg.merge(config_file) {
        Ok(_) => {
            info!(target: LOG_TARGET, "Configuration file loaded.");
            Ok(cfg)
        },
        Err(e) => Err(format!(
            "There was an error loading the configuration file. {}",
            e.to_string()
        )),
    }
}

/// Installs a new configuration file template, copied from `tari_config_sample.toml` to the given path.
pub fn install_default_config_file(path: &Path) -> Result<(), std::io::Error> {
    let source = include_str!("../../config/tari_config_sample.toml");
    fs::write(path, source)
}

//---------------------------------------------       Network type        ------------------------------------------//
#[derive(Clone, Debug, PartialEq)]
pub enum Network {
    MainNet,
    Rincewind,
}

impl TryFrom<String> for Network {
    type Error = ConfigurationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let val = value.to_lowercase();
        if &val == "rincewind" {
            Ok(Self::Rincewind)
        } else if &val == "mainnet" {
            Ok(Self::MainNet)
        } else {
            Err(ConfigurationError::new(
                "network",
                &format!("Invalid network option: {}", value),
            ))
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

//-------------------------------------------    ConfigExtractor trait    ------------------------------------------//
/// Extract parts of the global Config file into custom configuration objects that are more specific and localised.
/// The expected use case for this is to use `load_configuration` to load the global configuration file into a Config
/// object. This is then used to generate other, localised configuration objects, for example, `MempoolConfig` etc.
///
/// # Example
///
/// ```edition2018
/// # use tari_common::*;
/// # use config::Config;
/// struct MyConf {
///     foo: usize,
/// }
///
/// impl ConfigExtractor for MyConf {
///     fn set_default(cfg: &mut Config) {
///         cfg.set_default("main.foo", 5);
///         cfg.set_default("test.foo", 6);
///     }
///
///     fn extract_configuration(cfg: &Config, network: Network) -> Result<Self, ConfigurationError> {
///         let key = match network {
///             Network::MainNet => "main.foo",
///             Network::Rincewind => "test.foo",
///         };
///         let foo = cfg.get_int(key).map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
///         Ok(MyConf { foo })
///     }
/// }
/// ```
pub trait ConfigExtractor {
    /// Provides the default values for the Config object. This is used before `load_configuration` and ensures that
    /// all config parameters have at least the default value set.
    fn set_default(cfg: &mut Config);
    /// After `load_configuration` has been called, you can construct a specific configuration object by calling
    /// `extract_configuration` and it will create the object using values from the config file / environment variables
    fn extract_configuration(cfg: &Config, network: Network) -> Result<Self, ConfigurationError>
    where Self: Sized;
}
//---------------------------------------------      Database type        ------------------------------------------//
#[derive(Debug)]
pub enum DatabaseType {
    LMDB(PathBuf),
    Memory,
    Postgres { connection_string: String },
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
    Tcp { listener_address: Multiaddr },
    /// Configures the node to run over a tor hidden service using the Tor proxy. This transport recognises ip/tcp,
    /// onion v2, onion v3 and dns addresses.
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
        listener_address: Multiaddr,
        auth: SocksAuthentication,
    },
}

//-------------------------------------        Main Configuration Struct      --------------------------------------//

#[derive(Debug)]
pub struct GlobalConfig {
    pub network: Network,
    pub comms_transport: CommsTransport,
    pub data_dir: PathBuf,
    pub db_type: DatabaseType,
    pub core_threads: usize,
    pub blocking_threads: usize,
    pub identity_file: PathBuf,
    pub public_address: Multiaddr,
    pub peer_seeds: Vec<String>,
    pub peer_db_path: String,
    pub enable_mining: bool,
    pub wallet_file: String,
    pub tor_identity_file: String,
}

impl GlobalConfig {
    pub fn convert_from(mut cfg: Config) -> Result<Self, ConfigurationError> {
        let network = cfg
            .get_str("base_node.network")
            .map_err(|e| ConfigurationError::new("base_node.network", &e.to_string()))?;
        let network = Network::try_from(network)?;

        // Add in settings from the environment (with a prefix of TARI_NODE)
        // Eg.. `TARI_NODE_DEBUG=1 ./target/app` would set the `debug` key
        cfg.merge(Environment::with_prefix("tari"))
            .map_err(|e| ConfigurationError::new("environment variable", &e.to_string()))?;
        convert_node_config(network, cfg)
    }
}

/// Returns an OS-dependent string of the data subdirectory
pub fn sub_dir(data_dir: &Path, sub_dir: &str) -> Result<String, ConfigurationError> {
    let mut dir = data_dir.to_path_buf();
    dir.push(sub_dir);
    dir.to_str()
        .map(String::from)
        .ok_or_else(|| ConfigurationError::new("data_dir", "Not a valid UTF-8 string"))
}

fn convert_node_config(network: Network, cfg: Config) -> Result<GlobalConfig, ConfigurationError> {
    let net_str = network.to_string().to_lowercase();
    let key = config_string(&net_str, "db_type");
    let db_type = cfg
        .get_str(&key)
        .map(|s| s.to_lowercase())
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
    let key = config_string(&net_str, "data_dir");
    let data_dir = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
    let data_dir = PathBuf::from(data_dir);
    let db_type = match db_type.as_str() {
        "memory" => DatabaseType::Memory,
        "lmdb" => {
            let path = sub_dir(&data_dir, "db")?;
            DatabaseType::LMDB(PathBuf::from(path))
        },
        "postgres" => {
            let key = config_string(&net_str, "postgres_connection");
            let connection_string = cfg
                .get_str(&key)
                .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

            DatabaseType::Postgres { connection_string }
        },
        _ => return Err(ConfigurationError::new("base_node.db_type", "Invalid option")),
    };
    // Thread counts
    let key = config_string(&net_str, "core_threads");
    let core_threads = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
    let key = config_string(&net_str, "blocking_threads");
    let blocking_threads = cfg
        .get_int(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as usize;
    // Node id path
    let key = config_string(&net_str, "identity_file");
    let identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
    let identity_file = PathBuf::from(identity_file);

    // Tor private key persistence
    let key = config_string(&net_str, "tor_identity_file");
    let tor_identity_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

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

    // Peer seeds
    let key = config_string(&net_str, "peer_seeds");
    let peer_seeds = cfg
        .get_array(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
    let peer_seeds = peer_seeds.into_iter().map(|v| v.into_str().unwrap()).collect();

    // Peer DB path
    let peer_db_path = sub_dir(&data_dir, "peer_db")?;

    // set base node mining
    let key = config_string(&net_str, "enable_mining");
    let enable_mining = cfg
        .get_bool(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as bool;

    // set wallet_file
    let key = "wallet.wallet_file".to_string();
    let wallet_file = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))? as String;

    Ok(GlobalConfig {
        network,
        comms_transport,
        data_dir,
        db_type,
        core_threads,
        blocking_threads,
        identity_file,
        public_address,
        peer_seeds,
        peer_db_path,
        enable_mining,
        wallet_file,
        tor_identity_file,
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

            Ok(CommsTransport::Tcp { listener_address })
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

            let key = config_string(network, "socks5_listener_address");
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

//-------------------------------------      Configuration file defaults      --------------------------------------//

/// Generate the global Tari configuration instance.
///
/// The `Config` object that is returned holds _all_ the default values possible in the `~/.tari.config.toml` file.
/// These will typically be overridden by userland settings in envars, the config file, or the command line.
pub fn default_config() -> Config {
    let mut cfg = Config::new();
    let local_ip_addr = get_local_ip().unwrap_or_else(|| "/ip4/1.2.3.4".parse().unwrap());

    // Common settings
    cfg.set_default("common.message_cache_size", 10).unwrap();
    cfg.set_default("common.message_cache_ttl", 1440).unwrap();
    cfg.set_default("common.peer_whitelist", Vec::<String>::new()).unwrap();
    cfg.set_default("common.peer_database ", default_subdir("peers"))
        .unwrap();
    cfg.set_default("common.blacklist_ban_period ", 1440).unwrap();

    // Wallet settings
    cfg.set_default("wallet.grpc_enabled", false).unwrap();
    cfg.set_default("wallet.grpc_address", "tcp://127.0.0.1:18040").unwrap();
    cfg.set_default("wallet.wallet_file", default_subdir("wallet/wallet.dat"))
        .unwrap();

    // Base Node settings
    cfg.set_default("base_node.network", "mainnet").unwrap();

    // Mainnet base node defaults
    cfg.set_default("base_node.mainnet.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.mainnet.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default("base_node.mainnet.blocking_threads", 4).unwrap();
    cfg.set_default("base_node.mainnet.core_threads", 6).unwrap();
    cfg.set_default("base_node.mainnet.data_dir", default_subdir("mainnet/"))
        .unwrap();
    cfg.set_default(
        "base_node.mainnet.identity_file",
        default_subdir("mainnet/node_id.json"),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.tor_identity_file",
        default_subdir("mainnet/tor.json"),
    )
    .unwrap();
    cfg.set_default(
        "base_node.mainnet.public_address",
        format!("{}/tcp/18041", local_ip_addr),
    )
    .unwrap();
    cfg.set_default("base_node.mainnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.mainnet.grpc_address", "tcp://127.0.0.1:18041")
        .unwrap();
    cfg.set_default("base_node.mainnet.enable_mining", false).unwrap();

    // Rincewind base node defaults
    cfg.set_default("base_node.rincewind.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.rincewind.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default("base_node.rincewind.blocking_threads", 4).unwrap();
    cfg.set_default("base_node.rincewind.core_threads", 4).unwrap();
    cfg.set_default("base_node.rincewind.data_dir", default_subdir("rincewind/"))
        .unwrap();
    cfg.set_default(
        "base_node.rincewind.tor_identity_file",
        default_subdir("rincewind/tor.json"),
    )
    .unwrap();
    cfg.set_default(
        "base_node.rincewind.identity_file",
        default_subdir("rincewind/node_id.json"),
    )
    .unwrap();
    cfg.set_default(
        "base_node.rincewind.public_address",
        format!("{}/tcp/18141", local_ip_addr),
    )
    .unwrap();
    cfg.set_default("base_node.rincewind.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.rincewind.grpc_address", "tcp://127.0.0.1:18141")
        .unwrap();
    cfg.set_default("base_node.rincewind.enable_mining", false).unwrap();

    set_transport_defaults(&mut cfg);

    cfg
}

fn set_transport_defaults(cfg: &mut Config) {
    // Mainnet
    // Default transport for mainnet is tcp
    cfg.set_default("base_node.mainnet.transport", "tcp").unwrap();
    cfg.set_default("base_node.mainnet.tcp_listener_address", "/ip4/0.0.0.0/tcp/18089")
        .unwrap();

    cfg.set_default("base_node.mainnet.tor_control_address", "/ip4/127.0.0.1/tcp/9051")
        .unwrap();
    cfg.set_default("base_node.mainnet.tor_control_auth", "none").unwrap();
    cfg.set_default("base_node.mainnet.tor_forward_address", "/ip4/127.0.0.1/tcp/18141")
        .unwrap();
    cfg.set_default("base_node.mainnet.tor_onion_port", "18141").unwrap();

    cfg.set_default("base_node.mainnet.socks5_proxy_address", "/ip4/0.0.0.0/tcp/9050")
        .unwrap();
    cfg.set_default("base_node.mainnet.socks5_listener_address", "/ip4/0.0.0.0/tcp/18099")
        .unwrap();
    cfg.set_default("base_node.mainnet.socks5_auth", "none").unwrap();

    // rincewind
    // Default transport for rincewind is tcp
    cfg.set_default("base_node.rincewind.transport", "tcp").unwrap();
    cfg.set_default("base_node.rincewind.tcp_listener_address", "/ip4/0.0.0.0/tcp/18189")
        .unwrap();

    cfg.set_default("base_node.rincewind.tor_control_address", "/ip4/127.0.0.1/tcp/9051")
        .unwrap();
    cfg.set_default("base_node.rincewind.tor_control_auth", "none").unwrap();
    cfg.set_default("base_node.rincewind.tor_forward_address", "/ip4/127.0.0.1/tcp/18041")
        .unwrap();
    cfg.set_default("base_node.rincewind.tor_onion_port", "18141").unwrap();

    cfg.set_default("base_node.rincewind.socks5_proxy_address", "/ip4/0.0.0.0/tcp/9150")
        .unwrap();
    cfg.set_default("base_node.rincewind.socks5_listener_address", "/ip4/0.0.0.0/tcp/18199")
        .unwrap();
    cfg.set_default("base_node.rincewind.socks5_auth", "none").unwrap();
}

fn get_local_ip() -> Option<Multiaddr> {
    get_if_addrs::get_if_addrs().ok().and_then(|if_addrs| {
        if_addrs
            .into_iter()
            .find(|if_addr| !if_addr.is_loopback())
            .map(|if_addr| {
                let mut addr = Multiaddr::empty();
                match if_addr.ip() {
                    IpAddr::V4(ip) => {
                        addr.push(Protocol::Ip4(ip));
                    },
                    IpAddr::V6(ip) => {
                        addr.push(Protocol::Ip6(ip));
                    },
                }
                addr
            })
    })
}

//-------------------------------------      Configuration errors      --------------------------------------//

#[derive(Debug)]
pub struct ConfigurationError {
    field: String,
    message: String,
}

impl ConfigurationError {
    pub fn new(field: &str, msg: &str) -> Self {
        ConfigurationError {
            field: String::from(field),
            message: String::from(msg),
        }
    }
}

impl Display for ConfigurationError {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        f.write_str(&format!("Invalid value for {}: {}", self.field, self.message))
    }
}

impl Error for ConfigurationError {}

#[cfg(test)]
mod test {
    use crate::ConfigurationError;

    #[test]
    fn configuration_error() {
        let e = ConfigurationError::new("test", "is a string");
        assert_eq!(e.to_string(), "Invalid value for test: is a string");
    }
}
