// Copyright 2026. The Tari Project
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

//! Configuration for the peer sync tool.
//!
//! This reads the *same* `config.toml` as the base node, from the same `[base_node]` and `[p2p.seeds]` sections, so
//! that the peer sync run uses exactly the seeds, transport and DHT settings that the node would use. Only the subset
//! of `[base_node]` that is relevant to peer discovery is deserialized here; all other keys in that section are
//! ignored.

use std::path::{Path, PathBuf};

use config::Config;
use serde::{Deserialize, Serialize};
use tari_common::{
    ConfigurationError,
    DefaultConfigLoader,
    SubConfigPath,
    configuration::{CommonConfig, Network},
};
use tari_p2p::{P2pConfig, PeerSeedsConfig};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerSyncConfig {
    pub common: CommonConfig,
    pub base_node: BaseNodeP2pConfig,
    pub peer_seeds: PeerSeedsConfig,
}

impl PeerSyncConfig {
    pub fn load_from(cfg: &Config) -> Result<Self, ConfigurationError> {
        let mut config = Self {
            common: CommonConfig::load_from(cfg)?,
            peer_seeds: PeerSeedsConfig::load_from(cfg)?,
            base_node: BaseNodeP2pConfig::load_from(cfg)?,
        };
        config.base_node.set_base_path(config.common.base_path());
        Ok(config)
    }
}

/// The peer-discovery relevant subset of the base node's `[base_node]` config section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseNodeP2pConfig {
    override_from: Option<String>,
    /// Selected network
    pub network: Network,
    /// A path to the file that stores the base node identity and secret key
    pub identity_file: PathBuf,
    /// A path to the file that stores the tor hidden service private key, if using the tor transport.
    pub tor_identity_file: PathBuf,
    /// The p2p (comms and DHT) configuration
    pub p2p: P2pConfig,
}

impl Default for BaseNodeP2pConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            network: Network::default(),
            identity_file: PathBuf::from("config/base_node_id.json"),
            tor_identity_file: PathBuf::from("config/base_node_tor_id.json"),
            p2p: P2pConfig {
                datastore_path: PathBuf::from("peer_db/base_node"),
                ..Default::default()
            },
        }
    }
}

impl BaseNodeP2pConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.identity_file.is_absolute() {
            self.identity_file = base_path.as_ref().join(self.identity_file.as_path());
        }
        if !self.tor_identity_file.is_absolute() {
            self.tor_identity_file = base_path.as_ref().join(self.tor_identity_file.as_path());
        }
        self.p2p.set_base_path(base_path);
    }
}

impl SubConfigPath for BaseNodeP2pConfig {
    fn main_key_prefix() -> &'static str {
        "base_node"
    }
}
