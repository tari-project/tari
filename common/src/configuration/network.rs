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
};

use serde::{Deserialize, Serialize};

use crate::ConfigurationError;

/// Represents the available Tari p2p networks. Only nodes with matching byte values will be able to connect, so these
/// should never be changed once released.
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Network {
    MainNet = 0x00,
    LocalNet = 0x10,
    Ridcully = 0x21,
    Stibbons = 0x22,
    Weatherwax = 0xa3,
    Igor = 0x24,
    Dibbler = 0x25,
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
            Ridcully => "ridcully",
            Stibbons => "stibbons",
            Weatherwax => "weatherwax",
            Igor => "igor",
            Dibbler => "dibbler",
            LocalNet => "localnet",
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Network::MainNet
    }
}

impl FromStr for Network {
    type Err = ConfigurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        #[allow(clippy::enum_glob_use)]
        use Network::*;
        match value.to_lowercase().as_str() {
            "ridcully" => Ok(Ridcully),
            "stibbons" => Ok(Stibbons),
            "weatherwax" => Ok(Weatherwax),
            "mainnet" => Ok(MainNet),
            "localnet" => Ok(LocalNet),
            "igor" => Ok(Igor),
            "dibbler" => Ok(Dibbler),
            invalid => Err(ConfigurationError::new(
                "network",
                Some(value.to_string()),
                &format!("Invalid network option: {}", invalid),
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

impl Display for Network {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.as_key_str())
    }
}
