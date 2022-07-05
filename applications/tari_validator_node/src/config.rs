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
use tari_common::{
    configuration::{CommonConfig, Network},
    ConfigurationError,
    DefaultConfigLoader,
    SubConfigPath,
};
use tari_comms::multiaddr::Multiaddr;
use tari_p2p::{P2pConfig, PeerSeedsConfig};

#[derive(Debug, Clone)]
pub struct ApplicationConfig {
    pub common: CommonConfig,
    pub validator_node: ValidatorNodeConfig,
    pub peer_seeds: PeerSeedsConfig,
    pub network: Network,
}

impl ApplicationConfig {
    pub fn load_from(cfg: &Config) -> Result<Self, ConfigurationError> {
        let mut config = Self {
            common: CommonConfig::load_from(cfg)?,
            validator_node: ValidatorNodeConfig::load_from(cfg)?,
            peer_seeds: PeerSeedsConfig::load_from(cfg)?,
            network: cfg.get("network")?,
        };
        config.validator_node.set_base_path(config.common.base_path());
        Ok(config)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ValidatorNodeConfig {
    override_from: Option<String>,
    /// A path to the file that stores your node identity and secret key
    pub identity_file: PathBuf,
    /// A path to the file that stores the tor hidden service private key, if using the tor transport
    pub tor_identity_file: PathBuf,
    /// The node's publicly-accessible hostname
    pub public_address: Option<Multiaddr>,
    /// The asset worker will adhere to this phased timeout for the asset
    pub phase_timeout: u64,
    /// The Tari base node's GRPC address
    pub base_node_grpc_address: SocketAddr,
    /// The Tari console wallet's GRPC address
    pub wallet_grpc_address: SocketAddr,
    /// If set to false, there will be no scanning at all
    pub scan_for_assets: bool,
    /// How often do we want to scan the base layer for changes
    pub new_asset_scanning_interval: u64,
    /// If set then only the specific assets will be checked
    pub assets_allow_list: Option<Vec<String>>,
    /// The relative path to store persistent data
    pub data_dir: PathBuf,
    /// The p2p configuration settings
    pub p2p: P2pConfig,
    /// The constitution will auto accept contracts if true
    pub constitution_auto_accept: bool,
    /// Constitution confirmation time in block height
    pub constitution_management_confirmation_time: u64,
    /// Constitution polling interval in block height
    pub constitution_management_polling_interval: u64,
    /// Constitution polling interval in time (seconds)
    pub constitution_management_polling_interval_in_seconds: u64,
    /// GRPC address of the validator node  application
    pub grpc_address: Option<Multiaddr>,
}

impl ValidatorNodeConfig {
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        if !self.identity_file.is_absolute() {
            self.identity_file = base_path.as_ref().join(&self.identity_file);
        }
        if !self.tor_identity_file.is_absolute() {
            self.tor_identity_file = base_path.as_ref().join(&self.tor_identity_file);
        }
        if !self.data_dir.is_absolute() {
            self.data_dir = base_path.as_ref().join(&self.data_dir);
        }
        self.p2p.set_base_path(base_path);
    }
}

impl Default for ValidatorNodeConfig {
    fn default() -> Self {
        let p2p = P2pConfig {
            datastore_path: PathBuf::from("peer_db/validator_node"),
            ..Default::default()
        };

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
            data_dir: PathBuf::from("data/validator_node"),
            constitution_auto_accept: false,
            constitution_management_confirmation_time: 20,
            constitution_management_polling_interval: 120,
            constitution_management_polling_interval_in_seconds: 60,
            p2p,
            grpc_address: Some("/ip4/127.0.0.1/tcp/18144".parse().unwrap()),
        }
    }
}

impl SubConfigPath for ValidatorNodeConfig {
    fn main_key_prefix() -> &'static str {
        "validator_node"
    }
}
