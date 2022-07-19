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
//

use core::fmt;
use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
};

use bollard::{container::LogOutput, models::ContainerCreateResponse};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use super::TariWorkspace;
use crate::docker::DockerWrapperError;

//-------------------------------------------     ContainerId      ----------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerId(pub String);

impl From<String> for ContainerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for ContainerId {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for ContainerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl ContainerId {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

//-------------------------------------------     ContainerStatus      ----------------------------------------------

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContainerStatus {
    Created,
    Running,
    Stopped,
    Deleted,
}

//-------------------------------------------     ContainerState      ----------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerState {
    name: String,
    id: ContainerId,
    info: ContainerCreateResponse,
    status: ContainerStatus,
}

impl ContainerState {
    pub fn new(name: String, id: ContainerId, info: ContainerCreateResponse) -> Self {
        Self {
            name,
            id,
            info,
            status: ContainerStatus::Created,
        }
    }

    pub fn running(&mut self) {
        self.status = ContainerStatus::Running;
    }

    pub fn set_stop(&mut self) {
        self.status = ContainerStatus::Stopped;
    }

    pub fn set_deleted(&mut self) {
        self.status = ContainerStatus::Deleted;
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn info(&self) -> &ContainerCreateResponse {
        &self.info
    }

    pub fn id(&self) -> &ContainerId {
        &self.id
    }

    pub fn status(&self) -> ContainerStatus {
        self.status
    }
}

//-------------------------------------------     LogMessage      ----------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub message: String,
    pub source: String,
}

impl From<LogOutput> for LogMessage {
    fn from(log: LogOutput) -> Self {
        let (source, message) = match log {
            LogOutput::StdErr { message } => ("StdErr".to_string(), String::from_utf8_lossy(&message).into_owned()),
            LogOutput::StdOut { message } => ("StdOut".to_string(), String::from_utf8_lossy(&message).into_owned()),
            LogOutput::Console { message } => ("Console".to_string(), String::from_utf8_lossy(&message).into_owned()),
            LogOutput::StdIn { message } => ("StdIn".to_string(), String::from_utf8_lossy(&message).into_owned()),
        };
        Self { source, message }
    }
}

//-------------------------------------------     TariNetwork      ----------------------------------------------

/// Supported networks for the launchpad
#[derive(Serialize, Debug, Deserialize, Clone, Copy)]
pub enum TariNetwork {
    Dibbler,
    Igor,
    Mainnet,
}

impl TariNetwork {
    pub fn lower_case(self) -> &'static str {
        match self {
            Self::Dibbler => "dibbler",
            Self::Igor => "igor",
            Self::Mainnet => "mainnet",
        }
    }

    pub fn upper_case(self) -> &'static str {
        match self {
            Self::Dibbler => "DIBBLER",
            Self::Igor => "IGOR",
            Self::Mainnet => "MAINNET",
        }
    }
}

/// Default network is Dibbler. This will change after mainnet launch
impl Default for TariNetwork {
    fn default() -> Self {
        Self::Dibbler
    }
}

impl TryFrom<&str> for TariNetwork {
    type Error = DockerWrapperError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "dibbler" => Ok(TariNetwork::Dibbler),
            "igor" => Ok(TariNetwork::Igor),
            "mainnet" => Ok(TariNetwork::Mainnet),
            _ => Err(DockerWrapperError::UnsupportedNetwork),
        }
    }
}

//-------------------------------------------     ImageType      ----------------------------------------------

#[derive(Debug, Clone, Copy, EnumIter, PartialEq, Eq, Hash, Serialize)]
pub enum ImageType {
    Tor,
    BaseNode,
    Wallet,
    XmRig,
    Sha3Miner,
    MmProxy,
    Monerod,
    Loki,
    Promtail,
    Grafana,
}

impl ImageType {
    pub fn image_name(&self) -> &str {
        match self {
            Self::Tor => "tor",
            Self::BaseNode => "tari_base_node",
            Self::Wallet => "tari_wallet",
            Self::XmRig => "xmrig",
            Self::Sha3Miner => "tari_sha3_miner",
            Self::MmProxy => "tari_mm_proxy",
            Self::Monerod => "monerod",
            Self::Loki => "loki",
            Self::Promtail => "promtail",
            Self::Grafana => "grafana",
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Tor => "Tor",
            Self::BaseNode => "Base Node",
            Self::Wallet => "Wallet",
            Self::XmRig => "Xmrig",
            Self::Sha3Miner => "SHA3 miner",
            Self::MmProxy => "MM proxy",
            Self::Monerod => "Monerod",
            Self::Loki => "Loki",
            Self::Promtail => "Promtail",
            Self::Grafana => "Grafana",
        }
    }

    pub fn container_name(&self) -> &str {
        match self {
            Self::Tor => "tor",
            Self::BaseNode => "base_node",
            Self::Wallet => "wallet",
            Self::XmRig => "xmrig",
            Self::Sha3Miner => "sha3_miner",
            Self::MmProxy => "mm_proxy",
            Self::Monerod => "monerod",
            Self::Loki => "loki",
            Self::Promtail => "promtail",
            Self::Grafana => "grafana",
        }
    }

    pub fn data_folder(&self) -> &str {
        match self {
            Self::Tor => "tor",
            Self::BaseNode => "base_node",
            Self::Wallet => "wallet",
            Self::XmRig => "xmrig",
            Self::Sha3Miner => "sha3_miner",
            Self::MmProxy => "mm_proxy",
            Self::Monerod => "monerod",
            Self::Loki => "grafana",
            Self::Promtail => "grafana",
            Self::Grafana => "grafana",
        }
    }
}

impl TryFrom<&str> for ImageType {
    type Error = DockerWrapperError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let s = value.to_lowercase();
        match s.as_str() {
            "tor" => Ok(Self::Tor),
            "base_node" | "base node" => Ok(Self::BaseNode),
            "wallet" => Ok(Self::Wallet),
            "xmrig" => Ok(Self::XmRig),
            "sha3_miner" | "sha3 miner" => Ok(Self::Sha3Miner),
            "mm_proxy" | "mm proxy" => Ok(Self::MmProxy),
            "monerod" | "monero" => Ok(Self::Monerod),
            "loki" => Ok(Self::Loki),
            "promtail" => Ok(Self::Promtail),
            "grafana" => Ok(Self::Grafana),
            _ => Err(DockerWrapperError::InvalidImageType),
        }
    }
}

impl fmt::Display for TariNetwork {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for ImageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
