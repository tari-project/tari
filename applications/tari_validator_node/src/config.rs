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
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use config::Config;
use serde::{Deserialize, Serialize};
use tari_common::{configuration::CommonConfig, ConfigurationError, DefaultConfigLoader, SubConfigPath};
use tari_comms::multiaddr::Multiaddr;
use tari_p2p::{P2pConfig, PeerSeedsConfig};

#[derive(Debug, Clone)]
pub struct ApplicationConfig {
    pub common: CommonConfig,
    pub validator_node: ValidatorNodeConfig,
    pub peer_seeds: PeerSeedsConfig,
}

impl ApplicationConfig {
    pub fn load_from(cfg: &Config) -> Result<Self, ConfigurationError> {
        let mut config = Self {
            common: CommonConfig::load_from(cfg)?,
            validator_node: ValidatorNodeConfig::load_from(cfg)?,
            peer_seeds: PeerSeedsConfig::load_from(cfg)?,
        };
        config.validator_node.set_base_path(config.common.base_path());
        Ok(config)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ValidatorNodeConfig {
    override_from: Option<String>,
    pub identity_file: PathBuf,
    pub tor_identity_file: PathBuf,
    pub public_address: Option<Multiaddr>,
    pub phase_timeout: u64,
    pub base_node_grpc_address: SocketAddr,
    pub wallet_grpc_address: SocketAddr,
    pub scan_for_assets: bool,
    pub new_asset_scanning_interval: u64,
    pub assets_allow_list: Option<Vec<String>>,
    pub data_dir: PathBuf,
    pub p2p: P2pConfig,
    pub committee_management_polling_interval: u64,
    pub committee_management_confirmation_time: u64,
}

impl ValidatorNodeConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.identity_file.is_absolute() {
            self.identity_file = base_path.as_ref().join(&self.identity_file);
        }
        if !self.data_dir.is_absolute() {
            self.data_dir = base_path.as_ref().join(&self.data_dir);
        }
        self.p2p.set_base_path(base_path);
    }
}

impl Default for ValidatorNodeConfig {
    fn default() -> Self {
        Self {
            override_from: None,
            identity_file: PathBuf::from("validator_node_id.json"),
            tor_identity_file: PathBuf::from("validator_node_tor_id.json"),
            public_address: None,
            phase_timeout: 30,
            base_node_grpc_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 18142),
            wallet_grpc_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 18143),
            scan_for_assets: true,
            new_asset_scanning_interval: 10,
            assets_allow_list: None,
            data_dir: PathBuf::from("/data/validator_node"),
            committee_management_confirmation_time: 10,
            committee_management_polling_interval: 5,
            p2p: P2pConfig::default(),
        }
    }
}

impl SubConfigPath for ValidatorNodeConfig {
    fn main_key_prefix() -> &'static str {
        "validator_node"
    }
}
