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

use super::default_subdir;
use config::Config;
use std::{
    convert::TryFrom,
    error::Error,
    fmt::{Display, Formatter, Result as FormatResult},
    path::PathBuf,
};

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
            Self::MainNet => "MainNet",
            Self::TestNet => "TestNet",
        };
        f.write_str(msg)
    }
}

//---------------------------------------------      Database type        ------------------------------------------//

pub enum DatabaseType {
    LMDB(PathBuf),
    Memory,
}

//-------------------------------------        Main Configuration Struct      --------------------------------------//

pub struct NodeBuilderConfig {
    pub network: Network,
    pub db_type: DatabaseType,
    pub core_threads: usize,
    pub blocking_threads: usize,
}

impl NodeBuilderConfig {
    pub fn convert_from(cfg: Config) -> Result<Self, ConfigurationError> {
        let network = cfg
            .get_str("base_node.network")
            .map_err(|e| ConfigurationError::new("base_node.network", &e.to_string()))?;
        let network = Network::try_from(network)?;
        convert_node_config(network, cfg)
    }
}

fn convert_node_config(network: Network, cfg: Config) -> Result<NodeBuilderConfig, ConfigurationError> {
    let net_str = network.to_string().to_lowercase();
    let key = config_string(&net_str, "db_type");
    let db_type = cfg
        .get_str(&key)
        .map(|s| s.to_lowercase())
        .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
    let db_type = if &db_type == "memory" {
        DatabaseType::Memory
    } else if &db_type == "lmdb" {
        let key = config_string(&net_str, "db");
        let path = cfg
            .get_str(&key)
            .map_err(|e| ConfigurationError::new(&key, &e.to_string()))?;
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

    Ok(NodeBuilderConfig {
        network,
        db_type,
        core_threads,
        blocking_threads,
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
    cfg.set_default("wallet.grpc_address", "tcp://127.0.0.1:80400").unwrap();
    cfg.set_default("wallet.wallet_file", default_subdir("wallet/wallet.dat"))
        .unwrap();

    // Base Node settings
    cfg.set_default("base_node.network", "mainnet").unwrap();
    cfg.set_default("base_node.mainnet.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.mainnet.blocking_threads", 4).unwrap();
    cfg.set_default("base_node.mainnet.core_threads", 6).unwrap();
    cfg.set_default("base_node.mainnet.db", default_subdir("mainnet/db/"))
        .unwrap();
    cfg.set_default("base_node.mainnet.control-address", "http://localhost:80898")
        .unwrap();
    cfg.set_default("base_node.mainnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.mainnet.grpc_address", "tcp://127.0.0.1:80410")
        .unwrap();

    cfg.set_default("base_node.testnet.blocking_threads", 4).unwrap();
    cfg.set_default("base_node.mainnet.db_type", "lmdb").unwrap();
    cfg.set_default("base_node.testnet.core_threads", 4).unwrap();
    cfg.set_default("base_node.testnet.db", default_subdir("testnet/db/"))
        .unwrap();
    cfg.set_default("base_node.testnet.control-address", "http://localhost:81898")
        .unwrap();
    cfg.set_default("base_node.testnet.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.testnet.grpc_address", "tcp://127.0.0.1:81410")
        .unwrap();

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
