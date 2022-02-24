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
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use config::{Config, ConfigError};
use serde::Deserialize;

use crate::ConfigurationError;

#[derive(Debug, Clone, Deserialize)]
pub struct ValidatorNodeConfig {
    pub committee: Vec<String>,
    pub phase_timeout: u64,
    pub template_id: String,
    #[serde(default = "default_asset_config_directory")]
    pub asset_config_directory: PathBuf,
    #[serde(default = "default_base_node_grpc_address")]
    pub base_node_grpc_address: SocketAddr,
    #[serde(default = "default_wallet_grpc_address")]
    pub wallet_grpc_address: SocketAddr,
    #[serde(default = "default_true")]
    pub scan_for_assets: bool,
    #[serde(default = "default_asset_scanning_interval")]
    pub new_asset_scanning_interval: u64,
    pub assets_allow_list: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}
fn default_asset_scanning_interval() -> u64 {
    10
}

fn default_asset_config_directory() -> PathBuf {
    PathBuf::from("assets")
}

fn default_base_node_grpc_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 18142)
}

fn default_wallet_grpc_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 18143)
}

impl ValidatorNodeConfig {
    pub fn convert_if_present(cfg: &Config) -> Result<Option<ValidatorNodeConfig>, ConfigurationError> {
        let section: Self = match cfg.get("validator_node") {
            Ok(s) => s,
            Err(ConfigError::NotFound(_)) => {
                return Ok(None);
            },
            Err(err) => {
                return Err(err.into());
            },
        };
        Ok(Some(section))
        // dbg!(&section);
        // if section.is_empty() {
        //     Ok(None)
        // } else {
        //     Ok(Some(Self {
        //         committee: section
        //             .get("committee")
        //             .ok_or_else(|| ConfigurationError::new("dan_node.committee", "missing committee"))?
        //             .into_array()?
        //             .into_iter()
        //             .map(|c| c.into_str())
        //             .collect::<Result<Vec<_>, ConfigError>>()?,
        //     }))
    }
}
