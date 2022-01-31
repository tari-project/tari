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
//

use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use tari_common::{NetworkConfigPath};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MergeMiningProxyConfig {
    pub monerod_url: Vec<String>,
    pub monerod_username: String,
    pub monerod_password: String,
    pub monerod_use_auth: bool,
    pub grpc_base_node_address: Multiaddr,
    pub grpc_console_wallet_address: Multiaddr,
    pub proxy_host_address: Multiaddr,
    pub proxy_submit_to_origin: bool,
    pub wait_for_initial_sync_at_startup: bool,
}

impl Default for MergeMiningProxyConfig {
    fn default() -> Self {
        Self {
            monerod_url: vec![],
            monerod_username: "".to_string(),
            monerod_password: "".to_string(),
            monerod_use_auth: false,
            grpc_base_node_address: "/ip4/127.0.0.1/tcp/18142".parse().unwrap(),
            grpc_console_wallet_address: "/ip4/127.0.0.1/tcp/18143".parse().unwrap(),
            proxy_host_address: "/ip4/127.0.0.1/tcp/7878".parse().unwrap(),
            proxy_submit_to_origin: true,
            wait_for_initial_sync_at_startup: true,
        }
    }
}

impl NetworkConfigPath for MergeMiningProxyConfig {
    fn main_key_prefix() -> &'static str {
        "merge_mining_proxy"
    }
}

#[cfg(test)]
mod test {
    use tari_common::{configuration::config, DefaultConfigLoader};

    use crate::config::MergeMiningProxyConfig;

    fn get_config(network: &str) -> config::Config {
        let mut cfg: config::Config = config::Config::default();
        let s = format!(
            r#"
[common]
  network = "foo"
[merge_mining_proxy]
  network = "{}"
  monerod_username = "cmot"
  grpc_console_wallet_address = "/dns4/wallet/tcp/9000"
[merge_mining_proxy.igor]
  monerod_url = [ "http://network.a.org" ]
  monerod_password = "password_igor"
  grpc_base_node_address = "/dns4/base_node_a/tcp/8080"
  grpc_console_wallet_address = "/dns4/wallet_a/tcp/9000"
[merge_mining_proxy.dibbler]
  proxy_submit_to_origin = false
 monerod_url = [ "http://network.b.org" ]
  monerod_password = "password_dibbler"
  grpc_base_node_address = "/dns4/base_node_b/tcp/8080"
"#,
            network
        );
        cfg.merge(config::File::from_str(s.as_str(), config::FileFormat::Toml))
            .unwrap();
        cfg
    }

    #[test]
    fn merge_mining_proxy_configuration() {
        let cfg = get_config("dibbler");
        let config = <MergeMiningProxyConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load config");
        assert_eq!(&config.monerod_url, &["http://network.b.org".to_string()]);
        assert_eq!(config.proxy_submit_to_origin, false);
        assert_eq!(config.monerod_username.as_str(), "cmot");
        assert_eq!(config.monerod_password.as_str(), "password_dibbler");
        assert_eq!(
            config.grpc_base_node_address.to_string().as_str(),
            "/dns4/base_node_b/tcp/8080"
        );
        assert_eq!(
            config.grpc_console_wallet_address.to_string().as_str(),
            "/dns4/wallet/tcp/9000"
        );

        let cfg = get_config("igor");
        let config = <MergeMiningProxyConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load config");
        assert_eq!(&config.monerod_url, &["http://network.a.org".to_string()]);
        assert_eq!(config.proxy_submit_to_origin, true);
        assert_eq!(config.monerod_username.as_str(), "cmot");
        assert_eq!(config.monerod_password.as_str(), "password_igor");
        assert_eq!(
            config.grpc_base_node_address.to_string().as_str(),
            "/dns4/base_node_a/tcp/8080"
        );
        assert_eq!(
            config.grpc_console_wallet_address.to_string().as_str(),
            "/dns4/wallet_a/tcp/9000"
        );
    }

    #[test]
    fn default_config() {
        let config = MergeMiningProxyConfig::default();
        assert_eq!(&config.grpc_base_node_address.to_string(), "/ip4/127.0.0.1/tcp/18142");
        assert_eq!(config.monerod_use_auth, false);
        assert_eq!(config.proxy_submit_to_origin, true);
    }
}
