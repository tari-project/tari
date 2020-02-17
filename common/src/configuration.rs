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
use std::{
    convert::TryFrom,
    env,
    error::Error,
    fmt::{Display, Formatter, Result as FormatResult},
    path::{Path, PathBuf},
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
/// When bundled as a binary, the config sample file must be bundled in `./config`.
pub fn install_default_config_file(path: &Path) -> Result<u64, std::io::Error> {
    let mut source = env::current_dir()?;
    source.push(Path::new("config/tari_config_sample.toml"));
    std::fs::copy(source, path)
}

//---------------------------------------------       Network type        ------------------------------------------//
#[derive(Clone, Debug, PartialEq)]
pub enum Network {
    MainNet,
    TestNet,
}

impl TryFrom<String> for Network {
    type Error = ConfigurationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let val = value.to_lowercase();
        if &val == "testnet" {
            Ok(Self::TestNet)
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
            Self::TestNet => "testnet",
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
///             Network::TestNet => "test.foo",
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

pub enum DatabaseType {
    LMDB(PathBuf),
    Memory,
}

//-------------------------------------        Main Configuration Struct      --------------------------------------//

pub struct GlobalConfig {
    pub network: Network,
    pub data_dir: PathBuf,
    pub db_type: DatabaseType,
    pub core_threads: usize,
    pub blocking_threads: usize,
    pub identity_file: PathBuf,
    pub address: String,
    pub peer_seeds: Vec<String>,
    pub peer_db_path: String,
    pub enable_mining: bool,
    pub wallet_file: String,
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
    let db_type = if &db_type == "memory" {
        DatabaseType::Memory
    } else if &db_type == "lmdb" {
        let path = sub_dir(&data_dir, "db")?;
        DatabaseType::LMDB(PathBuf::from(path))
    } else {
        return Err(ConfigurationError::new("base_node.db_type", "Invalid option"));
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

    // Address
    let key = config_string(&net_str, "address");
    let address = cfg
        .get_str(&key)
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;

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
        data_dir,
        db_type,
        core_threads,
        blocking_threads,
        identity_file,
        address,
        peer_seeds,
        peer_db_path,
        enable_mining,
        wallet_file,
    })
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
    cfg.set_default("base_node.mainnet.address", "http://localhost:18089")
        .unwrap();
    cfg.set_default("base_node.mainnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.mainnet.grpc_address", "tcp://127.0.0.1:18041")
        .unwrap();
    cfg.set_default("base_node.mainnet.enable_mining", false).unwrap();

    // Testnet base node defaults
    cfg.set_default("base_node.testnet.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.testnet.peer_seeds", Vec::<String>::new())
        .unwrap();
    cfg.set_default("base_node.testnet.blocking_threads", 4).unwrap();
    cfg.set_default("base_node.testnet.core_threads", 4).unwrap();
    cfg.set_default("base_node.testnet.data_dir", default_subdir("testnet/"))
        .unwrap();
    cfg.set_default(
        "base_node.testnet.identity_file",
        default_subdir("testnet/node_id.json"),
    )
    .unwrap();
    cfg.set_default("base_node.testnet.address", "http://localhost:18189")
        .unwrap();
    cfg.set_default("base_node.testnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.testnet.grpc_address", "tcp://127.0.0.1:18141")
        .unwrap();
    cfg.set_default("base_node.testnet.enable_mining", true).unwrap();

    cfg
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
