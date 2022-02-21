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
    str::FromStr,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tari_common::{
    configuration::{CommonConfig, Network},
    SubConfigPath,
};
use tari_comms::multiaddr::Multiaddr;
use tari_core::{chain_storage::BlockchainDatabaseConfig, mempool::MempoolConfig};
use tari_p2p::initialization::P2pConfig;
use tari_storage::lmdb_store::LMDBConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseType {
    Lmdb,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BaseNodeConfig {
    override_from: Option<String>,
    pub network: Network,
    pub grpc_address: Option<Multiaddr>,
    pub identity_file: PathBuf,
    pub tor_identity_file: PathBuf,
    // TODO: Move to p2p or comms config
    pub public_address: Option<Multiaddr>,
    pub db_type: DatabaseType,
    pub lmdb: LMDBConfig,
    pub lmdb_path: PathBuf,
    pub max_randomx_vms: usize,
    pub bypass_range_proof_verification: bool,
    pub orphan_db_clean_out_threshold: usize,
    pub cleanup_orphans_at_startup: bool,
    pub force_sync_peers: Vec<String>,
    /// The allocated waiting time for a general request waiting for service responses from remote base nodes.
    /// Used for old messaging requests. Could be possible to remove
    pub service_request_timeout: Duration,
    pub storage: BlockchainDatabaseConfig,
    pub mempool: MempoolConfig,
    pub p2p: P2pConfig,
    // TODO: move to p2p config or rpc config
    pub rpc_max_simultaneous_sessions: usize,
}

impl Default for BaseNodeConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            network: Network::LocalNet,
            grpc_address: Some(Multiaddr::from_str("/ip4/127.0.0.1/tcp/18142").unwrap()),
            identity_file: PathBuf::from("config/base_node_id.json"),
            tor_identity_file: PathBuf::from("config/tor_id.json"),
            public_address: None,
            db_type: DatabaseType::Lmdb,
            lmdb_path: PathBuf::from("db"),
            max_randomx_vms: 5,
            bypass_range_proof_verification: false,
            orphan_db_clean_out_threshold: 0,
            force_sync_peers: vec![],
            service_request_timeout: Duration::from_secs(60),
            rpc_max_simultaneous_sessions: 100,
            ..Default::default()
        }
    }
}

impl SubConfigPath for BaseNodeConfig {
    fn main_key_prefix() -> &'static str {
        "base_node"
    }
}

impl BaseNodeConfig {
    pub fn identity_file(&self, common: &CommonConfig) -> PathBuf {
        common.base_path().join(self.identity_file.as_path())
    }

    pub fn tor_identity_file(&self, common: &CommonConfig) -> PathBuf {
        common.base_path().join(self.tor_identity_file.as_path())
    }

    pub fn lmdb_path(&self, common: &CommonConfig) -> PathBuf {
        common.data_dir().join(self.lmdb_path.as_path())
    }
}
