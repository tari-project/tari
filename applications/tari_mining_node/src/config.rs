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
//! Tari Miner Node derives all configuration management
//! from [tari_common] crate, also extending with few
//! specific options:
//! - base_node_grpc_address - is IPv4/IPv6 address including port
//! number, by which Tari Base Node can be found
//! - wallet_grpc_address - is IPv4/IPv6 address including port number,
//! where Tari Wallet Node can be found
//! - num_mining_threads - number of mining threads, defaults to number of cpu cores
//! - mine_on_tip_only - will start mining only when node is reporting bootstrapped state
//! - validate_tip_timeout_sec - will check tip with node every N seconds to validate that still
//! mining on a tip
//! All miner options configured under `[mining_node]` section of
//! Tari's `config.toml`.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tari_app_grpc::tari_rpc::{pow_algo::PowAlgos, NewBlockTemplateRequest, PowAlgo};
use tari_common::{GlobalConfig, NetworkConfigPath};

#[derive(Serialize, Deserialize, Debug)]
pub struct MinerConfig {
    pub base_node_grpc_address: Option<String>,
    pub wallet_grpc_address: Option<String>,
    pub num_mining_threads: usize,
    pub mine_on_tip_only: bool,
    pub proof_of_work_algo: ProofOfWork,
    pub validate_tip_timeout_sec: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ProofOfWork {
    Sha3,
}

impl NetworkConfigPath for MinerConfig {
    fn main_key_prefix() -> &'static str {
        "mining_node"
    }
}

impl Default for MinerConfig {
    fn default() -> Self {
        Self {
            base_node_grpc_address: None,
            wallet_grpc_address: None,
            num_mining_threads: num_cpus::get(),
            mine_on_tip_only: true,
            proof_of_work_algo: ProofOfWork::Sha3,
            validate_tip_timeout_sec: 30,
        }
    }
}

impl MinerConfig {
    pub fn base_node_addr(&self, global: &GlobalConfig) -> String {
        self.base_node_grpc_address
            .clone()
            .unwrap_or_else(|| format!("http://{}", global.grpc_base_node_address))
    }

    pub fn wallet_addr(&self, global: &GlobalConfig) -> String {
        self.wallet_grpc_address
            .clone()
            .unwrap_or_else(|| format!("http://{}", global.grpc_console_wallet_address))
    }

    pub fn pow_algo_request(&self) -> NewBlockTemplateRequest {
        let algo = match self.proof_of_work_algo {
            ProofOfWork::Sha3 => Some(PowAlgo {
                pow_algo: PowAlgos::Sha3.into(),
            }),
        };
        NewBlockTemplateRequest { algo, max_weight: 0 }
    }

    pub fn wait_timeout(&self) -> Duration {
        // TODO: add config parameter
        Duration::from_secs(10)
    }

    pub fn validate_tip_timeout_sec(&self) -> Duration {
        Duration::from_secs(self.validate_tip_timeout_sec)
    }
}
