//  Copyright 2021, The Tari Project
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
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

use crate::ConfigurationError;

static CURRENT_NETWORK: OnceLock<Network> = OnceLock::new();

/// Represents the available Tari p2p networks. Only nodes with matching byte values will be able to connect, so these
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
    /// The reserved wire byte for liveness ('LIVENESS_WIRE_MODE')
    pub const RESERVED_WIRE_BYTE: u8 = 0xa7;

    pub fn get_current_or_user_setting_or_default() -> Self {
        match CURRENT_NETWORK.get() {
            Some(&network) => network,
            None => {
                // Check to see if the network has been set by the environment, otherwise use the default
                match std::env::var("TARI_NETWORK") {
                    Ok(network) => Network::from_str(network.as_str()).unwrap_or(Network::default()),
                    Err(_) => Network::default(),
                }
            },
        }
    }

    pub fn set_current(network: Network) -> Result<(), Network> {
        CURRENT_NETWORK.set(network)
    }

    pub fn is_set() -> bool {
        CURRENT_NETWORK.get().is_some()
    }

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

    /// This function returns the network wire byte for any chosen network. Increase these numbers for any given network
    /// when network traffic separation is required.
    /// Note: Do not re-use previous values.
    pub fn as_wire_byte(self) -> u8 {
        let wire_byte = match self {
            // Choose a value in 'MAIN_NET_RANGE' or assign 'self.as_byte()'
            Network::MainNet => self.as_byte(),
            // Choose a value in 'STAGE_NET_RANGE' or assign 'self.as_byte()'
            Network::StageNet => self.as_byte(),
            // Choose a value in 'NEXT_NET_RANGE' or assign 'self.as_byte()'
            Network::NextNet => self.as_byte(),
            // Choose a value in 'LOCAL_NET_RANGE' or assign 'self.as_byte()'
            Network::LocalNet => self.as_byte(),
            // Choose a value in 'IGOR_RANGE' or assign 'self.as_byte()'
            Network::Igor => self.as_byte(),
            // Choose a value in 'ESMERALDA_RANGE' or assign 'self.as_byte()'
            Network::Esmeralda => 200,
        };
        // The reserved wire byte for liveness ('LIVENESS_WIRE_MODE') is defined in another module, which is not
        // accessible from here.
        debug_assert!(wire_byte != Network::RESERVED_WIRE_BYTE);
        wire_byte
    }
}

/// The default network for all applications
impl Default for Network {
    #[cfg(tari_target_network_mainnet)]
    fn default() -> Self {
        match std::env::var("TARI_NETWORK") {
            Ok(network) => Network::from_str(network.as_str()).unwrap_or(Network::StageNet),
            Err(_) => Network::StageNet,
        }
    }

    #[cfg(tari_target_network_nextnet)]
    fn default() -> Self {
        Network::NextNet
    }

    #[cfg(not(any(tari_target_network_mainnet, tari_target_network_nextnet)))]
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
        #[cfg(tari_target_network_mainnet)]
        assert!(matches!(network, Network::MainNet | Network::StageNet));
        #[cfg(tari_target_network_nextnet)]
        assert_eq!(network, Network::NextNet);
        #[cfg(not(any(tari_target_network_mainnet, tari_target_network_nextnet)))]
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

    // Do not change these ranges
    const MAIN_NET_RANGE: std::ops::Range<u8> = 0..40;
    const STAGE_NET_RANGE: std::ops::Range<u8> = 40..80;
    const NEXT_NET_RANGE: std::ops::Range<u8> = 80..120;
    const LOCAL_NET_RANGE: std::ops::Range<u8> = 120..160;
    const IGOR_RANGE: std::ops::Range<u8> = 160..200;
    const ESMERALDA_RANGE: std::ops::Range<u8> = 200..240;
    const LEGACY_RANGE: [u8; 6] = [0x00, 0x01, 0x02, 0x10, 0x24, 0x26];

    /// Helper function to verify the network wire byte range
    pub fn verify_network_wire_byte_range(network_wire_byte: u8, network: Network) -> Result<(), String> {
        if network_wire_byte == Network::RESERVED_WIRE_BYTE {
            return Err(format!(
                "Invalid network wire byte, cannot be '{}', reserved for 'LIVENESS_WIRE_MODE'",
                Network::RESERVED_WIRE_BYTE
            ));
        }

        // Legacy compatibility
        if network_wire_byte == network.as_byte() {
            return Ok(());
        }
        if LEGACY_RANGE.contains(&network_wire_byte) {
            return Err(format!(
                "Invalid network wire byte `{}` for network `{}`",
                network_wire_byte, network
            ));
        }

        // Verify binned values
        let valid = match network {
            Network::MainNet => MAIN_NET_RANGE.contains(&network_wire_byte),
            Network::StageNet => STAGE_NET_RANGE.contains(&network_wire_byte),
            Network::NextNet => NEXT_NET_RANGE.contains(&network_wire_byte),
            Network::LocalNet => LOCAL_NET_RANGE.contains(&network_wire_byte),
            Network::Igor => IGOR_RANGE.contains(&network_wire_byte),
            Network::Esmeralda => ESMERALDA_RANGE.contains(&network_wire_byte),
        };
        if !valid {
            return Err(format!(
                "Invalid network wire byte `{}` for network `{}`",
                network_wire_byte, network
            ));
        }
        Ok(())
    }

    #[test]
    fn test_as_wire_byte() {
        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::NextNet,
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            assert!(verify_network_wire_byte_range(Network::RESERVED_WIRE_BYTE, network).is_err());

            let wire_byte = Network::as_wire_byte(network);
            assert!(verify_network_wire_byte_range(wire_byte, network).is_ok());

            for val in 0..255 {
                match network {
                    Network::MainNet => {
                        if val == Network::RESERVED_WIRE_BYTE {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if val == Network::MainNet.as_byte() {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else if LEGACY_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if MAIN_NET_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        }
                    },
                    Network::StageNet => {
                        if val == Network::RESERVED_WIRE_BYTE {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if val == Network::StageNet.as_byte() {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else if LEGACY_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if STAGE_NET_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        }
                    },
                    Network::NextNet => {
                        if val == Network::RESERVED_WIRE_BYTE {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if val == Network::NextNet.as_byte() {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else if LEGACY_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if NEXT_NET_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        }
                    },
                    Network::LocalNet => {
                        if val == Network::RESERVED_WIRE_BYTE {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if val == Network::LocalNet.as_byte() {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else if LEGACY_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if LOCAL_NET_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        }
                    },
                    Network::Igor => {
                        if val == Network::RESERVED_WIRE_BYTE {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if val == Network::Igor.as_byte() {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else if LEGACY_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if IGOR_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        }
                    },
                    Network::Esmeralda => {
                        if val == Network::RESERVED_WIRE_BYTE {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if val == Network::Esmeralda.as_byte() {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else if LEGACY_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        } else if ESMERALDA_RANGE.contains(&val) {
                            assert!(verify_network_wire_byte_range(val, network).is_ok());
                        } else {
                            assert!(verify_network_wire_byte_range(val, network).is_err());
                        }
                    },
                }
            }
        }
    }
}
