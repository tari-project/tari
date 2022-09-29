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
    path::{Path, PathBuf},
    time::Duration,
};

use config::Config;
use serde::{Deserialize, Serialize};
use tari_app_utilities::consts;
use tari_common::{
    configuration::{serializers, CommonConfig, Network, StringList},
    ConfigurationError,
    DefaultConfigLoader,
    SubConfigPath,
};
use tari_comms::multiaddr::Multiaddr;
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
#[allow(clippy::struct_excessive_bools)]
pub struct BaseNodeConfig {
    override_from: Option<String>,
    /// Selected network
    pub network: Network,
    /// GRPC address of base node
    pub grpc_address: Option<Multiaddr>,
    /// A path to the file that stores the base node identity and secret key
    pub identity_file: PathBuf,
    /// Spin up and use a built-in Tor instance. This only works on macos/linux - requires that the wallet was built
    /// with the optional "libtor" feature flag.
    pub use_libtor: bool,
    /// A path to the file that stores the tor hidden service private key, if using the tor transport.
    pub tor_identity_file: PathBuf,
    /// The type of database backend to use
    pub db_type: DatabaseType,
    /// The lmdb config settings
    pub lmdb: LMDBConfig,
    /// The relative path to store persistent data
    pub data_dir: PathBuf,
    /// The relative path to store the lmbd data
    pub lmdb_path: PathBuf,
    /// The maximum amount of VMs that RandomX will be use
    // TODO: This is a potential conflict with 'BaseNodeStateMachineConfig::max_randomx_vms'
    pub max_randomx_vms: usize,
    /// Bypass range proof verification to speed up validation
    // TODO: This is a potential conflict with 'BaseNodeStateMachineConfig::bypass_range_proof_verification'
    pub bypass_range_proof_verification: bool,
    /// The p2p config settings
    pub p2p: P2pConfig,
    /// If set this node will only sync to the nodes in this set
    pub force_sync_peers: StringList,
    /// The maximum amount of time to wait for remote base node responses for messaging-based requests.
    #[serde(with = "serializers::seconds")]
    pub messaging_request_timeout: Duration,
    /// The storage config settings
    pub storage: BlockchainDatabaseConfig,
    /// The mempool config settings
    pub mempool: MempoolConfig,
    /// The time interval between status line updates in the CLI
    #[serde(with = "serializers::seconds")]
    pub status_line_interval: Duration,
    /// The buffer size for the publish/subscribe connector channel, connecting comms messages to the domain layer
    pub buffer_size: usize,
    /// The rate limit for the publish/subscribe connector channel, i.e. maximum amount of inbound messages to
    /// accept - any rate attempting to exceed this limit will be throttled
    pub buffer_rate_limit: usize,
    /// Liveness meta data auto ping interval between peers
    #[serde(with = "serializers::seconds")]
    pub metadata_auto_ping_interval: Duration,
    /// The state_machine config settings
    pub state_machine: BaseNodeStateMachineConfig,
    /// Obscure GRPC error responses
    pub report_grpc_error: bool,
}

impl Default for BaseNodeConfig {
    fn default() -> Self {
        let p2p = P2pConfig {
            datastore_path: PathBuf::from("peer_db/base_node"),
            user_agent: format!("tari/basenode/{}", consts::APP_VERSION_NUMBER),
            ..Default::default()
        };
        Self {
            override_from: None,
            network: Network::default(),
            grpc_address: None,
            identity_file: PathBuf::from("config/base_node_id.json"),
            use_libtor: false,
            tor_identity_file: PathBuf::from("config/base_node_tor_id.json"),
            p2p,
            db_type: DatabaseType::Lmdb,
            lmdb: Default::default(),
            data_dir: PathBuf::from("data/base_node"),
            lmdb_path: PathBuf::from("db"),
            max_randomx_vms: 5,
            bypass_range_proof_verification: false,
            force_sync_peers: StringList::default(),
            messaging_request_timeout: Duration::from_secs(60),
            storage: Default::default(),
            mempool: Default::default(),
            status_line_interval: Duration::from_secs(5),
            buffer_size: 1_500,
            buffer_rate_limit: 1_000,
            metadata_auto_ping_interval: Duration::from_secs(30),
            state_machine: Default::default(),
            report_grpc_error: false,
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
        if !self.tor_identity_file.is_absolute() {
            self.tor_identity_file = base_path.as_ref().join(self.tor_identity_file.as_path());
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
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    Lmdb,
}
