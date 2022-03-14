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

use std::{path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use tari_common::{configuration::Network, SubConfigPath};
use tari_comms::multiaddr::Multiaddr;
use tari_core::{consensus::NetworkConsensus, transactions::CryptoFactories};
use tari_p2p::{auto_update::AutoUpdateConfig, initialization::P2pConfig};

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    output_manager_service::config::OutputManagerServiceConfig,
    transaction_service::config::TransactionServiceConfig,
};

pub const KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY: &str = "comms";

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct WalletConfig {
    pub comms_config: P2pConfig,
    // pub factories: CryptoFactories,
    pub transaction_service_config: TransactionServiceConfig,
    pub output_manager_service_config: OutputManagerServiceConfig,
    pub buffer_size: usize,
    pub rate_limit: usize,
    pub network: Network,
    pub base_node_service_config: BaseNodeServiceConfig,
    pub updater_config: AutoUpdateConfig,
    pub db_file: PathBuf,
    pub connection_manager_pool_size: usize,
    pub password: Option<String>, // TODO: Make clear on drop
    pub public_address: Option<Multiaddr>,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            buffer_size: 0,
            rate_limit: 0,
            db_file: PathBuf::from_str("to_populate").unwrap(),
            connection_manager_pool_size: 5, // TODO: get actual default
            ..Default::default()
        }
    }
}

impl SubConfigPath for WalletConfig {
    fn main_key_prefix() -> &'static str {
        "wallet"
    }
}
