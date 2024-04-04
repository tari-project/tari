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

use std::path::{Path, PathBuf};

use minotari_wallet_grpc_client::GrpcAuthentication;
use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::{Network, StringList},
    SubConfigPath,
};
use tari_common_types::tari_address::TariAddress;
use tari_comms::multiaddr::Multiaddr;
use tari_core::transactions::transaction_components::RangeProofType;

// The default Monero fail URL for mainnet
const MONERO_FAIL_MAINNET_URL: &str = "https://monero.fail/?chain=monero&network=mainnet&all=true";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct MergeMiningProxyConfig {
    override_from: Option<String>,
    /// Use dynamic monerod URL obtained form the official Monero website (https://monero.fail/)
    pub use_dynamic_fail_data: bool,
    /// The monero fail URL to get the monerod URLs from - must be pointing to the official Monero website.
    /// Valid alternatives are:
    /// - mainnet:  'https://monero.fail/?chain=monero&network=mainnet&all=true'
    /// - stagenet: `https://monero.fail/?chain=monero&network=stagenet&all=true`
    /// - testnet:  `https://monero.fail/?chain=monero&network=testnet&all=true`
    pub monero_fail_url: String,
    /// URL to monerod (you can add your own server here or use public nodes from https://monero.fail/)
    pub monerod_url: StringList,
    /// Username for curl
    pub monerod_username: String,
    /// Password for curl
    pub monerod_password: String,
    /// If authentication is being used for curl
    pub monerod_use_auth: bool,
    /// The Minotari base node's GRPC address
    pub base_node_grpc_address: Option<Multiaddr>,
    /// GRPC authentication for base node
    pub base_node_grpc_authentication: GrpcAuthentication,
    /// GRPC domain name for node TLS validation
    pub base_node_grpc_tls_domain_name: Option<String>,
    /// GRPC ca cert name for TLS
    pub base_node_grpc_ca_cert_filename: String,
    /// Address of the minotari_merge_mining_proxy application
    pub listener_address: Multiaddr,
    /// In sole merged mining, the block solution is usually submitted to the Monero blockchain (monerod) as well as to
    /// the Minotari blockchain, then this setting should be "true". With pool merged mining, there is no sense in
    /// submitting the solution to the Monero blockchain as thepool does that, then this setting should be "false".
    pub submit_to_origin: bool,
    /// The merge mining proxy can either wait for the base node to achieve initial sync at startup before it enables
    /// mining, or not. If merge mining starts before the base node has achieved initial sync, those Minotari mined
    /// blocks will not be accepted.
    pub wait_for_initial_sync_at_startup: bool,
    /// When mining for minotari, you might want to check the achieved difficulty of the mined minotari block before
    /// submitting. This setting this can be disabled to allow you to always submit minotari blocks even if the
    /// difficulty does not meet the required.
    pub check_tari_difficulty_before_submit: bool,
    /// The maximum amount of VMs that RandomX will be use
    pub max_randomx_vms: usize,
    /// The extra data to store in the coinbase, usually some data about the mining pool.
    /// Note that this data is publicly readable, but it is suggested you populate it so that
    /// pool dominance can be seen before any one party has more than 51%.
    pub coinbase_extra: String,
    /// Selected network
    pub network: Network,
    /// The relative path to store persistent config
    pub config_dir: PathBuf,
    /// The Tari wallet address (valid address in hex) where the mining funds will be sent to - must be assigned
    pub wallet_payment_address: String,
    /// Stealth payment yes or no
    pub stealth_payment: bool,
    /// Range proof type - revealed_value or bullet_proof_plus: (default = revealed_value)
    pub range_proof_type: RangeProofType,
}

impl Default for MergeMiningProxyConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            use_dynamic_fail_data: true,
            monero_fail_url: MONERO_FAIL_MAINNET_URL.into(),
            monerod_url: StringList::default(),
            monerod_username: String::new(),
            monerod_password: String::new(),
            monerod_use_auth: false,
            base_node_grpc_address: None,
            base_node_grpc_authentication: GrpcAuthentication::default(),
            base_node_grpc_tls_domain_name: None,
            base_node_grpc_ca_cert_filename: "node_ca.pem".to_string(),
            listener_address: "/ip4/127.0.0.1/tcp/18081".parse().unwrap(),
            submit_to_origin: true,
            wait_for_initial_sync_at_startup: true,
            check_tari_difficulty_before_submit: true,
            max_randomx_vms: 5,
            coinbase_extra: "tari_merge_mining_proxy".to_string(),
            network: Default::default(),
            config_dir: PathBuf::from("config/merge_mining_proxy"),
            wallet_payment_address: TariAddress::default().to_hex(),
            stealth_payment: true,
            range_proof_type: RangeProofType::RevealedValue,
        }
    }
}

impl MergeMiningProxyConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.config_dir.is_absolute() {
            self.config_dir = base_path.as_ref().join(self.config_dir.as_path());
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
    use std::str::FromStr;

    use tari_common::DefaultConfigLoader;
    use tari_comms::multiaddr::Multiaddr;

    use crate::config::MergeMiningProxyConfig;

    fn get_config(override_from: &str) -> config::Config {
        let s = r#"
            [common]
              baz = "foo"
            [merge_mining_proxy]
              monerod_username = "cmot"
            [config_a.merge_mining_proxy]
              monerod_url = [ "http://network.a.org" ]
              monerod_password = "password_igor"
              base_node_grpc_address = "/dns4/base_node_a/tcp/8080"
            [config_b.merge_mining_proxy]
              submit_to_origin = false
              monerod_url = [ "http://network.b.org" ]
              monerod_password = "password_stagenet"
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
        assert_eq!(config.monerod_password.as_str(), "password_stagenet");
        assert_eq!(
            config.base_node_grpc_address,
            Some(Multiaddr::from_str("/dns4/base_node_b/tcp/8080").unwrap())
        );

        let cfg = get_config("config_a");
        let config = MergeMiningProxyConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.monerod_url.as_slice(), &["http://network.a.org".to_string()]);
        assert!(config.submit_to_origin);
        assert_eq!(config.monerod_username.as_str(), "cmot");
        assert_eq!(config.monerod_password.as_str(), "password_igor");
        assert_eq!(
            config.base_node_grpc_address,
            Some(Multiaddr::from_str("/dns4/base_node_a/tcp/8080").unwrap())
        );
    }

    #[test]
    fn default_config() {
        let config = MergeMiningProxyConfig::default();
        assert_eq!(config.base_node_grpc_address, None);
        assert!(!config.monerod_use_auth);
        assert!(config.submit_to_origin);
    }
}
