//  Copyright 2023. The Tari Project
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
use tari_common::{
    configuration::{serializers, CommonConfig, Network, StringList},
    ConfigurationError,
    DefaultConfigLoader,
    SubConfigPath,
};
use tari_comms_dht::{store_forward::SafConfig, DbConnectionUrl, DhtConfig, NetworkDiscoveryConfig};
use tari_p2p::{P2pConfig, PeerSeedsConfig, TcpTransportConfig, TransportConfig};
use tari_storage::lmdb_store::LMDBConfig;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApplicationConfig {
    pub common: CommonConfig,
    pub chat_client: ChatClientConfig,
    pub peer_seeds: PeerSeedsConfig,
}

impl ApplicationConfig {
    pub fn load_from(cfg: &Config) -> Result<Self, ConfigurationError> {
        let mut config = Self {
            common: CommonConfig::load_from(cfg)?,
            peer_seeds: PeerSeedsConfig::load_from(cfg)?,
            chat_client: ChatClientConfig::load_from(cfg)?,
        };

        config.chat_client.set_base_path(config.common.base_path());
        Ok(config)
    }

    pub fn network(&self) -> Network {
        self.chat_client.network
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct ChatClientConfig {
    override_from: Option<String>,
    /// Selected network
    pub network: Network,
    /// A path to the file that stores the base node identity and secret key
    pub identity_file: PathBuf,
    /// A path to the file that stores the tor hidden service private key, if using the tor transport.
    pub tor_identity_file: PathBuf,
    /// The type of database backend to use
    pub db_type: DatabaseType,
    /// The lmdb config settings
    pub lmdb: LMDBConfig,
    /// The relative path to store persistent data
    pub data_dir: PathBuf,
    /// The name of the storage db
    pub db_file: PathBuf,
    /// The relative path to store the lmbd data
    pub lmdb_path: PathBuf,
    /// The p2p config settings
    pub p2p: P2pConfig,
    /// If set this node will only sync to the nodes in this set
    pub force_sync_peers: StringList,
    /// Liveness meta data auto ping interval between peers
    #[serde(with = "serializers::seconds")]
    pub metadata_auto_ping_interval: Duration,
    /// The location of the log path
    pub log_path: Option<PathBuf>,
    /// The log verbosity
    pub log_verbosity: Option<u8>,
}

impl Default for ChatClientConfig {
    fn default() -> Self {
        let p2p = P2pConfig {
            datastore_path: PathBuf::from("peer_db/chat_client"),
            user_agent: format!("tari/chat_client/{}", env!("CARGO_PKG_VERSION")),
            dht: DhtConfig {
                database_url: DbConnectionUrl::file("data/chat_client/dht.sqlite"),
                ..Default::default()
            },
            ..Default::default()
        };
        Self {
            override_from: None,
            network: Network::default(),
            identity_file: PathBuf::from("config/chat_client_id.json"),
            tor_identity_file: PathBuf::from("config/chat_client_tor_id.json"),
            p2p,
            db_type: DatabaseType::Lmdb,
            db_file: PathBuf::from("db/chat_client.db"),
            lmdb: Default::default(),
            data_dir: PathBuf::from("data/chat_client"),
            lmdb_path: PathBuf::from("db"),
            force_sync_peers: StringList::default(),
            metadata_auto_ping_interval: Duration::from_secs(30),
            log_path: None,
            log_verbosity: Some(2), // Warn
        }
    }
}

impl SubConfigPath for ChatClientConfig {
    fn main_key_prefix() -> &'static str {
        "chat_client"
    }
}

impl ChatClientConfig {
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
        if !self.db_file.is_absolute() {
            self.db_file = self.data_dir.join(self.db_file.as_path());
        }
        if let Some(path) = self.log_path.as_ref() {
            if path.is_absolute() {
                self.log_path = Some(base_path.as_ref().join(path));
            }
        }
        self.p2p.set_base_path(base_path);
    }

    pub fn default_local_test() -> Self {
        Self {
            network: Network::LocalNet,
            log_verbosity: Some(5), // Trace
            p2p: P2pConfig {
                datastore_path: PathBuf::from("peer_db/chat_client"),
                user_agent: format!("tari/chat_client/{}", env!("CARGO_PKG_VERSION")),
                dht: DhtConfig {
                    database_url: DbConnectionUrl::file("data/chat_client/dht.sqlite"),
                    network_discovery: NetworkDiscoveryConfig {
                        enabled: true,
                        ..NetworkDiscoveryConfig::default()
                    },
                    saf: SafConfig {
                        auto_request: true,
                        ..Default::default()
                    },
                    ..DhtConfig::default_local_test()
                },
                transport: TransportConfig::new_tcp(TcpTransportConfig {
                    ..TcpTransportConfig::default()
                }),
                allow_test_addresses: true,
                ..P2pConfig::default()
            },
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    Lmdb,
}
