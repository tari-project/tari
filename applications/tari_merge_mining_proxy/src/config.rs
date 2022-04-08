// Copyright 2022. The Tari Project
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

use serde::{Deserialize, Serialize};
use tari_common::{configuration::StringList, SubConfigPath};
use tari_comms::multiaddr::Multiaddr;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MergeMiningProxyConfig {
    override_from: Option<String>,
    pub monerod_url: StringList,
    pub monerod_username: String,
    pub monerod_password: String,
    pub monerod_use_auth: bool,
    pub base_node_grpc_address: Multiaddr,
    pub console_wallet_grpc_address: Multiaddr,
    pub listener_address: Multiaddr,
    pub submit_to_origin: bool,
    pub wait_for_initial_sync_at_startup: bool,
}

impl Default for MergeMiningProxyConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            monerod_url: StringList::default(),
            monerod_username: String::new(),
            monerod_password: String::new(),
            monerod_use_auth: false,
            base_node_grpc_address: "/ip4/127.0.0.1/tcp/18142".parse().unwrap(),
            console_wallet_grpc_address: "/ip4/127.0.0.1/tcp/18143".parse().unwrap(),
            listener_address: "/ip4/127.0.0.1/tcp/18081".parse().unwrap(),
            submit_to_origin: true,
            wait_for_initial_sync_at_startup: true,
        }
    }
}

impl SubConfigPath for MergeMiningProxyConfig {
    fn main_key_prefix() -> &'static str {
        "merge_mining_proxy"
    }
}

#[cfg(test)]
mod test {
    use tari_common::DefaultConfigLoader;

    use crate::config::MergeMiningProxyConfig;

    fn get_config(override_from: &str) -> config::Config {
        let s = r#"
[common]
  baz = "foo"
[merge_mining_proxy]
  monerod_username = "cmot"
  console_wallet_grpc_address = "/dns4/wallet/tcp/9000"
[config_a.merge_mining_proxy]
  monerod_url = [ "http://network.a.org" ]
  monerod_password = "password_igor"
  base_node_grpc_address = "/dns4/base_node_a/tcp/8080"
  console_wallet_grpc_address = "/dns4/wallet_a/tcp/9000"
[config_b.merge_mining_proxy]
  submit_to_origin = false
  monerod_url = [ "http://network.b.org" ]
  monerod_password = "password_dibbler"
  base_node_grpc_address = "/dns4/base_node_b/tcp/8080"
"#;

        config::Config::builder()
            .set_override("merge_mining_proxy.override_from", override_from)
            .unwrap()
            .add_source(config::File::from_str(s, config::FileFormat::Toml))
            .build()
            .unwrap()
    }

    #[test]
    fn merge_mining_proxy_configuration() {
        let cfg = get_config("config_b");
        let config = MergeMiningProxyConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.monerod_url.as_slice(), &["http://network.b.org".to_string()]);
        assert!(!config.submit_to_origin);
        assert_eq!(config.monerod_username.as_str(), "cmot");
        assert_eq!(config.monerod_password.as_str(), "password_dibbler");
        assert_eq!(
            config.base_node_grpc_address.to_string().as_str(),
            "/dns4/base_node_b/tcp/8080"
        );
        assert_eq!(
            config.console_wallet_grpc_address.to_string().as_str(),
            "/dns4/wallet/tcp/9000"
        );

        let cfg = get_config("config_a");
        let config = MergeMiningProxyConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.monerod_url.as_slice(), &["http://network.a.org".to_string()]);
        assert!(config.submit_to_origin);
        assert_eq!(config.monerod_username.as_str(), "cmot");
        assert_eq!(config.monerod_password.as_str(), "password_igor");
        assert_eq!(
            config.base_node_grpc_address.to_string().as_str(),
            "/dns4/base_node_a/tcp/8080"
        );
        assert_eq!(
            config.console_wallet_grpc_address.to_string().as_str(),
            "/dns4/wallet_a/tcp/9000"
        );
    }

    #[test]
    fn default_config() {
        let config = MergeMiningProxyConfig::default();
        assert_eq!(&config.base_node_grpc_address.to_string(), "/ip4/127.0.0.1/tcp/18142");
        assert!(!config.monerod_use_auth);
        assert!(config.submit_to_origin);
    }
}
