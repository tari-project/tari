//  Copyright 2022. The Tari Project
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
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
use std::{error::Error, path::PathBuf};

use clap::Args;
use tari_common::configuration::{ConfigOverrideProvider, Network};

#[derive(Args, Debug)]
pub struct CommonCliArgs {
    /// A path to a directory to store your files
    #[clap(
        short,
        long,
        aliases = &["base_path", "base_dir", "base-dir"],
        default_value_t= defaults::base_path(),
        env = "TARI_BASE_DIR"
    )]
    pub base_path: String,
    /// A path to the configuration file to use (config.toml)
    #[clap(short, long, default_value_t= defaults::config())]
    pub config: String,
    /// The path to the log configuration file
    #[clap(short, long, alias = "log_config")]
    pub log_config: Option<PathBuf>,

    /// The path to where logs should be stored
    #[clap(long, alias = "log_path")]
    pub log_path: Option<PathBuf>,

    /// Supply a network (overrides existing configuration)
    #[clap(long, env = "TARI_NETWORK")]
    pub network: Option<Network>,

    /// Overrides for properties in the config file (use the fully qualified key name!!). To choose the network use
    /// --network esmeralda, not -p. Override values are visible in the process list and shell history, so secrets
    /// belong in the config file or an environment variable. Examples:
    /// -p base_node.grpc_server_allow_methods="get_tokens_in_circulation, get_sync_progress, get_mempool_stats"
    /// -p esmeralda.p2p.seeds.peer_seeds="<public_key_1>::<address_1>", "<public_key_2>::<address_2>", or,
    /// -p base_node.grpc_server_allow_methods=""
    /// -p esmeralda.p2p.seeds.peer_seeds=""
    #[clap(short = 'p', value_parser = parse_key_val::<String, String>, action = clap::ArgAction::Append)]
    pub config_property_overrides: Vec<(String, String)>,
}

// Taken from clap examples
/// Parse a single key-value pair
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("invalid override: expected key=value: no `=` found in `{s}`"))?;
    let k = k.trim();
    if k.is_empty() {
        return Err("invalid override: string empty`".into());
    }
    Ok((k.parse()?, v.parse()?))
}

impl CommonCliArgs {
    /// Returns the path to the configuration file.
    ///
    /// Note: The configuration file path is evaluated before reading the configuration file itself,
    /// so it cannot reflect any `network = "..."` entry inside that file. Its base path resolution
    /// is driven solely by the `--network` CLI flag or the `TARI_NETWORK` environment variable.
    pub fn config_path(&self) -> PathBuf {
        let config_path = PathBuf::from(&self.config);
        if config_path.is_absolute() {
            config_path
        } else {
            self.get_base_path().join(config_path)
        }
    }

    pub fn get_base_path(&self) -> PathBuf {
        let network = self.network.unwrap_or_default();
        PathBuf::from(&self.base_path).join(network.to_string())
    }

    pub fn get_base_path_for(&self, network: &Network) -> PathBuf {
        let network = self.network.unwrap_or(*network);
        PathBuf::from(&self.base_path).join(network.to_string())
    }

    pub fn log_config_path(&self, application_name: &str) -> PathBuf {
        if let Some(ref log_config) = self.log_config {
            let path = PathBuf::from(log_config);
            if path.is_absolute() {
                log_config.clone()
            } else {
                self.get_base_path().join(log_config)
            }
        } else {
            self.get_base_path()
                .join("config")
                .join(application_name)
                .join("log4rs.yml")
        }
    }
}

impl ConfigOverrideProvider for CommonCliArgs {
    fn get_config_property_overrides(&self, network: &Network) -> Vec<(String, String)> {
        let mut overrides = self.config_property_overrides.clone();
        overrides.push((
            "common.base_path".to_string(),
            self.get_base_path_for(network)
                .as_os_str()
                .to_str()
                .expect("An os string from a path")
                .into(),
        ));
        overrides
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_cli_args_uses_resolved_network_when_cli_unset() {
        let args = CommonCliArgs {
            base_path: "/tmp/tari".to_string(),
            config: "config.toml".to_string(),
            log_config: None,
            log_path: None,
            network: None,
            config_property_overrides: Vec::new(),
        };

        let overrides = args.get_config_property_overrides(&Network::StageNet);
        let base_path_override = overrides
            .into_iter()
            .find(|(k, _)| k == "common.base_path")
            .map(|(_, v)| v)
            .expect("common.base_path override missing");

        assert!(
            base_path_override.ends_with("stagenet"),
            "expected base_path to end with stagenet, got {base_path_override}"
        );
    }

    #[test]
    fn common_cli_args_cli_network_takes_precedence() {
        let args = CommonCliArgs {
            base_path: "/tmp/tari".to_string(),
            config: "config.toml".to_string(),
            log_config: None,
            log_path: None,
            network: Some(Network::Esmeralda),
            config_property_overrides: Vec::new(),
        };

        let overrides = args.get_config_property_overrides(&Network::StageNet);
        let base_path_override = overrides
            .into_iter()
            .find(|(k, _)| k == "common.base_path")
            .map(|(_, v)| v)
            .expect("common.base_path override missing");

        assert!(
            base_path_override.ends_with("esmeralda"),
            "expected base_path to end with esmeralda, got {base_path_override}"
        );
    }

    #[test]
    fn parse_key_val_handles_multiple_equals_and_preserves_whitespace() {
        let (k, v): (String, String) = parse_key_val("a.b=c=d").unwrap();
        assert_eq!(k, "a.b");
        assert_eq!(v, "c=d");

        let (k, v): (String, String) = parse_key_val("a.b= x ").unwrap();
        assert_eq!(k, "a.b");
        assert_eq!(v, " x ");

        let (k, v): (String, String) = parse_key_val(" key = val ").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, " val ");

        let (k, v): (String, String) = parse_key_val("key=").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "");

        assert!(parse_key_val::<String, String>("=val").is_err());
        assert!(parse_key_val::<String, String>("   =val").is_err());
        assert!(parse_key_val::<String, String>("no_equals").is_err());
    }
}

mod defaults {
    use tari_common::dir_utils;

    const DEFAULT_CONFIG: &str = "config/config.toml";

    pub(super) fn base_path() -> String {
        dir_utils::default_path("", None).to_string_lossy().to_string()
    }

    pub(super) fn config() -> String {
        DEFAULT_CONFIG.to_string()
    }
}
