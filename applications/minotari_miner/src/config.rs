// Copyright 2021. The Tari Project
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
//! Miner specific configuration
//!
//! Minotari Miner Node derives all configuration management
//! from [tari_common] crate, also extending with few
//! specific options:
//! - base_node_grpc_address - is IPv4/IPv6 address including port
//! number, by which Minotari Base Node can be found
//! - wallet_grpc_address - is IPv4/IPv6 address including port number,
//! where Minotari Wallet Node can be found
//! - num_mining_threads - number of mining threads, defaults to number of cpu cores
//! - mine_on_tip_only - will start mining only when node is reporting bootstrapped state
//! - validate_tip_timeout_sec - will check tip with node every N seconds to validate that still
//! mining on a tip
//! All miner options configured under `[miner]` section of
//! Minotari's `config.toml`.

use std::{path::PathBuf, time::Duration};

use minotari_app_grpc::tari_rpc::{pow_algo::PowAlgos, NewBlockTemplateRequest, PowAlgo};
use serde::{Deserialize, Serialize};
use tari_common::{configuration::Network, SubConfigPath};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_comms::multiaddr::Multiaddr;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct MinerConfig {
    /// GRPC address of base node
    pub base_node_grpc_address: Option<Multiaddr>,
    /// GRPC authentication for base node
    pub base_node_grpc_authentication: GrpcAuthentication,
    /// GRPC domain name for node TLS validation
    pub base_node_grpc_tls_domain_name: Option<String>,
    /// GRPC address of console wallet
    pub wallet_grpc_address: Option<Multiaddr>,
    /// GRPC authentication for console wallet
    pub wallet_grpc_authentication: GrpcAuthentication,
    /// GRPC domain name for wallet TLS validation
    pub wallet_grpc_tls_domain_name: Option<String>,
    /// Number of mining threads
    pub num_mining_threads: usize,
    /// Start mining only when base node is bootstrapped and current block height is on the tip of network
    pub mine_on_tip_only: bool,
    /// The proof of work algorithm to use
    #[serde(skip)]
    pub proof_of_work_algo: ProofOfWork,
    /// Will check tip with node every N seconds and restart mining if height already taken and option
    /// `mine_on_tip_only` is set to true
    pub validate_tip_timeout_sec: u64,
    /// Stratum Mode configuration - mining pool address
    pub mining_pool_address: String,
    /// Stratum Mode configuration - mining wallet address/public key
    pub mining_wallet_address: String,
    /// Stratum Mode configuration - mining worker name
    pub mining_worker_name: String,
    /// The extra data to store in the coinbase, usually some data about the mining pool.
    /// Note that this data is publicly readable, but it is suggested you populate it so that
    /// pool dominance can be seen before any one party has more than 51%.
    pub coinbase_extra: String,
    /// Selected network
    pub network: Network,
    /// Base node reconnect timeout after any GRPC or miner error
    pub wait_timeout_on_error: u64,
    /// The relative path to store persistent config
    pub config_dir: PathBuf,
}

/// The proof of work data structure that is included in the block header. For the Minotari miner only `Sha3x` is
/// allowed.
#[derive(Serialize, Deserialize, Debug, Default)]
pub enum ProofOfWork {
    #[default]
    Sha3x,
}

impl SubConfigPath for MinerConfig {
    fn main_key_prefix() -> &'static str {
        "miner"
    }
}

impl Default for MinerConfig {
    fn default() -> Self {
        Self {
            base_node_grpc_address: None,
            base_node_grpc_authentication: GrpcAuthentication::default(),
            base_node_grpc_tls_domain_name: None,
            wallet_grpc_address: None,
            wallet_grpc_authentication: GrpcAuthentication::default(),
            wallet_grpc_tls_domain_name: None,
            num_mining_threads: num_cpus::get(),
            mine_on_tip_only: true,
            proof_of_work_algo: ProofOfWork::Sha3x,
            validate_tip_timeout_sec: 30,
            mining_pool_address: String::new(),
            mining_wallet_address: String::new(),
            mining_worker_name: String::new(),
            coinbase_extra: "minotari_miner".to_string(),
            network: Default::default(),
            wait_timeout_on_error: 10,
            config_dir: PathBuf::from("config"),
        }
    }
}

impl MinerConfig {
    pub fn pow_algo_request(&self) -> NewBlockTemplateRequest {
        let algo = match self.proof_of_work_algo {
            ProofOfWork::Sha3x => Some(PowAlgo {
                pow_algo: PowAlgos::Sha3x.into(),
            }),
        };
        NewBlockTemplateRequest { algo, max_weight: 0 }
    }

    pub fn wait_timeout(&self) -> Duration {
        Duration::from_secs(self.wait_timeout_on_error)
    }

    pub fn validate_tip_interval(&self) -> Duration {
        Duration::from_secs(self.validate_tip_timeout_sec)
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use tari_common::DefaultConfigLoader;
    use tari_comms::multiaddr::Multiaddr;

    use crate::config::MinerConfig;

    #[test]
    fn miner_configuration() {
        const CONFIG: &str = r#"
[miner]
num_mining_threads=2
base_node_grpc_address = "/dns4/my_base_node/tcp/1234"
mine_on_tip_only = false
"#;
        let mut cfg: config::Config = config::Config::default();
        #[allow(deprecated)]
        cfg.merge(config::File::from_str(CONFIG, config::FileFormat::Toml))
            .unwrap();
        let config = MinerConfig::load_from(&cfg).expect("Failed to load config");
        assert_eq!(config.num_mining_threads, 2);
        assert_eq!(config.wallet_grpc_address, MinerConfig::default().wallet_grpc_address);
        assert_eq!(
            config.base_node_grpc_address,
            Some(Multiaddr::from_str("/dns4/my_base_node/tcp/1234").unwrap())
        );
        assert!(!config.mine_on_tip_only);
    }
}
