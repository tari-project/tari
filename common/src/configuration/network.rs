//  Copyright 2021, The Taiji Project
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
    convert::TryFrom,
    fmt,
    fmt::{Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

use crate::ConfigurationError;

/// Represents the available Taiji p2p networks. Only nodes with matching byte values will be able to connect, so these
/// should never be changed once released.
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Network {
    MainNet = 0x00,
    StageNet = 0x01,
    NextNet = 0x02,
    LocalNet = 0x10,
    Igor = 0x24,
    Esmeralda = 0x26,
}

impl Network {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub const fn as_key_str(self) -> &'static str {
        #[allow(clippy::enum_glob_use)]
        use Network::*;
        match self {
            MainNet => "mainnet",
            StageNet => "stagenet",
            NextNet => "nextnet",
            Igor => "igor",
            Esmeralda => "esmeralda",
            LocalNet => "localnet",
        }
    }
}

/// The default network for all applications
impl Default for Network {
    #[cfg(taiji_network_mainnet)]
    fn default() -> Self {
        Network::StageNet
    }

    #[cfg(taiji_network_nextnet)]
    fn default() -> Self {
        Network::NextNet
    }

    #[cfg(all(not(taiji_network_mainnet), not(taiji_network_nextnet)))]
    fn default() -> Self {
        Network::Esmeralda
    }
}

impl FromStr for Network {
    type Err = ConfigurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        #[allow(clippy::enum_glob_use)]
        use Network::*;
        match value.to_lowercase().as_str() {
            "mainnet" => Ok(MainNet),
            "nextnet" => Ok(NextNet),
            "stagenet" => Ok(StageNet),
            "localnet" => Ok(LocalNet),
            "igor" => Ok(Igor),
            "esmeralda" | "esme" => Ok(Esmeralda),
            invalid => Err(ConfigurationError::new(
                "network",
                Some(value.to_string()),
                format!("Invalid network option: {}", invalid),
            )),
        }
    }
}
impl TryFrom<String> for Network {
    type Error = ConfigurationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl From<Network> for String {
    fn from(n: Network) -> Self {
        n.to_string()
    }
}

impl TryFrom<u8> for Network {
    type Error = ConfigurationError;

    fn try_from(v: u8) -> Result<Self, ConfigurationError> {
        match v {
            x if x == Network::MainNet as u8 => Ok(Network::MainNet),
            x if x == Network::StageNet as u8 => Ok(Network::StageNet),
            x if x == Network::NextNet as u8 => Ok(Network::NextNet),
            x if x == Network::LocalNet as u8 => Ok(Network::LocalNet),
            x if x == Network::Igor as u8 => Ok(Network::Igor),
            x if x == Network::Esmeralda as u8 => Ok(Network::Esmeralda),
            _ => Err(ConfigurationError::new(
                "network",
                Some(v.to_string()),
                format!("Invalid network option: {}", v),
            )),
        }
    }
}

impl Display for Network {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.as_key_str())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn network_bytes() {
        // get networks
        let mainnet = Network::MainNet;
        let stagenet = Network::StageNet;
        let nextnet = Network::NextNet;
        let localnet = Network::LocalNet;
        let igor = Network::Igor;
        let esmeralda = Network::Esmeralda;

        // test .as_byte()
        assert_eq!(mainnet.as_byte(), 0x00_u8);
        assert_eq!(stagenet.as_byte(), 0x01_u8);
        assert_eq!(nextnet.as_byte(), 0x02_u8);
        assert_eq!(localnet.as_byte(), 0x10_u8);
        assert_eq!(igor.as_byte(), 0x24_u8);
        assert_eq!(esmeralda.as_byte(), 0x26_u8);

        // test .as_key_str()
        assert_eq!(mainnet.as_key_str(), "mainnet");
        assert_eq!(stagenet.as_key_str(), "stagenet");
        assert_eq!(nextnet.as_key_str(), "nextnet");
        assert_eq!(localnet.as_key_str(), "localnet");
        assert_eq!(igor.as_key_str(), "igor");
        assert_eq!(esmeralda.as_key_str(), "esmeralda");
    }

    #[test]
    fn network_default() {
        let network = Network::default();
        assert_eq!(network, Network::Esmeralda);
    }

    #[test]
    fn network_from_str() {
        // test .from_str()
        assert_eq!(Network::from_str("mainnet").unwrap(), Network::MainNet);
        assert_eq!(Network::from_str("stagenet").unwrap(), Network::StageNet);
        assert_eq!(Network::from_str("nextnet").unwrap(), Network::NextNet);
        assert_eq!(Network::from_str("localnet").unwrap(), Network::LocalNet);
        assert_eq!(Network::from_str("igor").unwrap(), Network::Igor);
        assert_eq!(Network::from_str("esmeralda").unwrap(), Network::Esmeralda);
        assert_eq!(Network::from_str("esme").unwrap(), Network::Esmeralda);
        // catch error case
        let err_network = Network::from_str("invalid network");
        assert!(err_network.is_err());
    }

    #[test]
    fn network_from_byte() {
        assert_eq!(Network::try_from(0x00).unwrap(), Network::MainNet);
        assert_eq!(Network::try_from(0x01).unwrap(), Network::StageNet);
        assert_eq!(Network::try_from(0x02).unwrap(), Network::NextNet);
        assert_eq!(Network::try_from(0x10).unwrap(), Network::LocalNet);
        assert_eq!(Network::try_from(0x24).unwrap(), Network::Igor);
        assert_eq!(Network::try_from(0x26).unwrap(), Network::Esmeralda);
    }
}
