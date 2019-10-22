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
    error::Error,
    fmt::{Display, Formatter},
};

/// Generate the global Tari configurartino instance.
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
    cfg.set_default("base_node.blockchain-data", default_subdir("chain_data/"))
        .unwrap();
    cfg.set_default("base_node.control-address", "http://localhost:80898")
        .unwrap();
    cfg.set_default("base_node.grpc_enabled", false).unwrap();
    cfg.set_default("base_node.grpc_address", "tcp://127.0.0.1:80410")
        .unwrap();

    cfg
}

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
