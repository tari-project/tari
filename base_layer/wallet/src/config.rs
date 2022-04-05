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

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tari_common::{configuration::Network, SubConfigPath};
use tari_p2p::{auto_update::AutoUpdateConfig, P2pConfig};

use crate::{
    base_node_service::config::BaseNodeServiceConfig,
    output_manager_service::config::OutputManagerServiceConfig,
    transaction_service::config::TransactionServiceConfig,
};

pub const KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY: &str = "comms";

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct WalletConfig {
    pub override_from: Option<String>,
    pub p2p: P2pConfig,
    // pub factories: CryptoFactories,
    pub transaction_service_config: TransactionServiceConfig,
    pub output_manager_service_config: OutputManagerServiceConfig,
    pub buffer_size: usize,
    pub rate_limit: usize,
    pub network: Network,
    pub base_node_service_config: BaseNodeServiceConfig,
    pub updater_config: AutoUpdateConfig,
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    pub connection_manager_pool_size: usize,
    pub password: Option<String>, // TODO: Make clear on drop
    pub contacts_auto_ping_interval: Duration,
    pub contacts_online_ping_window: usize,
    pub command_send_wait_timeout: Duration,
    pub command_send_wait_stage: String,
    pub notify_file: Option<PathBuf>,
    pub grpc_address: Option<SocketAddr>,
    pub custom_base_node: Option<String>,
    pub base_node_service_peers: Vec<String>,
    pub recovery_retry_limit: usize,
    pub fee_per_gram: u64,
    pub num_required_confirmations: u64,
    pub use_libtor: bool,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            p2p: Default::default(),
            transaction_service_config: Default::default(),
            output_manager_service_config: Default::default(),
            buffer_size: 100,
            rate_limit: 10,
            network: Default::default(),
            base_node_service_config: Default::default(),
            updater_config: Default::default(),
            data_dir: PathBuf::from_str("data/wallet").unwrap(),
            db_file: PathBuf::from_str("console_wallet").unwrap(),
            connection_manager_pool_size: 5, // TODO: get actual default
            password: None,
            contacts_auto_ping_interval: Duration::from_secs(30),
            contacts_online_ping_window: 30,
            command_send_wait_stage: String::new(),
            command_send_wait_timeout: Duration::from_secs(300),
            notify_file: None,
            grpc_address: Some(([127, 0, 0, 1], 18143).into()),
            custom_base_node: None,
            base_node_service_peers: vec![],
            recovery_retry_limit: 3,
            fee_per_gram: 5,
            num_required_confirmations: 3,
            use_libtor: false,
        }
    }
}

impl SubConfigPath for WalletConfig {
    fn main_key_prefix() -> &'static str {
        "wallet"
    }
}

impl WalletConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.db_file.is_absolute() {
            self.db_file = base_path.as_ref().join(self.db_file.as_path());
        }
        if !self.data_dir.is_absolute() {
            self.data_dir = base_path.as_ref().join(self.data_dir.as_path());
        }
        self.p2p.set_base_path(self.data_dir.as_path());
    }
}
