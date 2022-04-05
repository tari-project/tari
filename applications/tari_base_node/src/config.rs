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

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use config::Config;
use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::{CommonConfig, Network},
    ConfigurationError,
    DefaultConfigLoader,
    SubConfigPath,
};
use tari_core::{
    base_node::BaseNodeStateMachineConfig,
    chain_storage::BlockchainDatabaseConfig,
    mempool::MempoolConfig,
};
use tari_p2p::{auto_update::AutoUpdateConfig, P2pConfig, PeerSeedsConfig};
use tari_storage::lmdb_store::LMDBConfig;

#[cfg(feature = "metrics")]
use crate::metrics::MetricsConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplicationConfig {
    pub common: CommonConfig,
    pub auto_update: AutoUpdateConfig,
    pub base_node: BaseNodeConfig,
    pub peer_seeds: PeerSeedsConfig,
    #[cfg(feature = "metrics")]
    pub metrics: MetricsConfig,
}

impl ApplicationConfig {
    pub fn load_from(cfg: &Config) -> Result<Self, ConfigurationError> {
        let mut config = Self {
            common: CommonConfig::load_from(cfg)?,
            auto_update: AutoUpdateConfig::load_from(cfg)?,
            peer_seeds: PeerSeedsConfig::load_from(cfg)?,
            base_node: BaseNodeConfig::load_from(cfg)?,
            #[cfg(feature = "metrics")]
            metrics: MetricsConfig::load_from(cfg)?,
        };

        config.base_node.set_base_path(config.common.base_path());
        Ok(config)
    }

    pub fn network(&self) -> Network {
        self.base_node.network
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BaseNodeConfig {
    override_from: Option<String>,
    pub network: Network,
    pub grpc_address: Option<SocketAddr>,
    pub identity_file: PathBuf,
    pub use_libtor: bool,
    pub tor_identity_file: PathBuf,
    pub db_type: DatabaseType,
    pub lmdb: LMDBConfig,
    pub data_dir: PathBuf,
    pub lmdb_path: PathBuf,
    pub max_randomx_vms: usize,
    pub bypass_range_proof_verification: bool,
    pub orphan_db_clean_out_threshold: usize,
    pub cleanup_orphans_at_startup: bool,
    pub p2p: P2pConfig,
    pub force_sync_peers: Vec<String>,
    /// The maximum amount of time to wait for remote base node responses for messaging-based requests.
    pub messaging_request_timeout: Duration,
    pub storage: BlockchainDatabaseConfig,
    pub mempool: MempoolConfig,
    pub status_line_interval: Duration,
    pub buffer_size: usize,
    pub buffer_rate_limit: usize,
    pub metadata_auto_ping_interval: Duration,
    pub state_machine: BaseNodeStateMachineConfig,
    pub resize_terminal_on_startup: bool,
}

impl Default for BaseNodeConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            network: Network::LocalNet,
            grpc_address: Some(([127, 0, 0, 1], 18142).into()),
            identity_file: PathBuf::from("config/base_node_id.json"),
            use_libtor: false,
            tor_identity_file: PathBuf::from("config/tor_id.json"),
            p2p: P2pConfig::default(),
            db_type: DatabaseType::Lmdb,
            lmdb: Default::default(),
            data_dir: PathBuf::from("data/base_node"),
            lmdb_path: PathBuf::from("db"),
            max_randomx_vms: 5,
            bypass_range_proof_verification: false,
            orphan_db_clean_out_threshold: 0,
            cleanup_orphans_at_startup: false,
            force_sync_peers: vec![],
            messaging_request_timeout: Duration::from_secs(60),
            storage: Default::default(),
            mempool: Default::default(),
            status_line_interval: Duration::from_secs(5),
            buffer_size: 100,
            buffer_rate_limit: 10,
            metadata_auto_ping_interval: Duration::from_secs(30),
            state_machine: Default::default(),
            resize_terminal_on_startup: true,
        }
    }
}

impl SubConfigPath for BaseNodeConfig {
    fn main_key_prefix() -> &'static str {
        "base_node"
    }
}

impl BaseNodeConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.identity_file.is_absolute() {
            self.identity_file = base_path.as_ref().join(self.identity_file.as_path());
        }
        if !self.data_dir.is_absolute() {
            self.data_dir = base_path.as_ref().join(self.data_dir.as_path());
        }
        if !self.lmdb_path.is_absolute() {
            self.lmdb_path = self.data_dir.join(self.lmdb_path.as_path());
        }
        self.p2p.set_base_path(base_path);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseType {
    Lmdb,
}
