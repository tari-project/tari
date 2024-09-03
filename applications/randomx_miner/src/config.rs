//  Copyright 2024. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. Minotari Miner Node derives all
// configuration management

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tari_common::{configuration::Network, SubConfigPath};

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RandomXMinerConfig {
    /// The address for the monero node, or merge mining proxy
    pub monero_base_node_address: Option<String>,
    /// Monero wallet address
    pub monero_wallet_address: Option<String>,
    /// An address to post hash results too
    pub universe_address: Option<String>,
    /// What mode to run in, eco or max
    pub mode: MiningMode,
    /// Selected network
    pub network: Network,
    /// The relative path to store persistent config
    pub config_dir: PathBuf,
    /// Number of mining threads
    pub num_mining_threads: usize,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub enum MiningMode {
    #[default]
    Eco,
    Max,
}

impl SubConfigPath for RandomXMinerConfig {
    fn main_key_prefix() -> &'static str {
        "randomx_miner"
    }
}

impl Default for RandomXMinerConfig {
    fn default() -> Self {
        Self {
            monero_base_node_address: None,
            monero_wallet_address: None,
            universe_address: None,
            mode: Default::default(),
            network: Default::default(),
            config_dir: PathBuf::from("config/randomx_miner"),
            num_mining_threads: num_cpus::get(),
        }
    }
}

impl RandomXMinerConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.config_dir.is_absolute() {
            self.config_dir = base_path.as_ref().join(self.config_dir.as_path());
        }
    }
}

#[cfg(test)]
mod test {
    use config::Config;
    use tari_common::DefaultConfigLoader;

    use crate::config::{MiningMode, RandomXMinerConfig};

    #[test]
    fn miner_configuration() {
        const CONFIG: &str = r#"
[miner]
monero_wallet_address="44AFFq5kSiGBoZ4NMDwYtN18obc8AemS33DBLWs3H7otXft3XjrpDtQGv7SqSsaBYBb98uNbr2VBBEt7f2wfn3RVGQBEP3A"
mode = "eco"
"#;
        let mut cfg: Config = Config::default();
        #[allow(deprecated)]
        cfg.merge(config::File::from_str(CONFIG, config::FileFormat::Toml))
            .unwrap();
        let config = RandomXMinerConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.mode, MiningMode::Eco);
        assert_eq!(
            config.monero_wallet_address,
            Some(
                "44AFFq5kSiGBoZ4NMDwYtN18obc8AemS33DBLWs3H7otXft3XjrpDtQGv7SqSsaBYBb98uNbr2VBBEt7f2wfn3RVGQBEP3A"
                    .to_string()
            )
        );
    }
}
