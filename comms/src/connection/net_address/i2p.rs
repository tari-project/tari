//  Copyright 2019 The Tari Project
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

use super::{parser::AddressParser, NetAddressError};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Represents an I2P address
#[derive(Clone, PartialEq, Eq, Debug, Hash, Deserialize, Serialize)]
pub struct I2PAddress {
    pub name: String,
}

impl FromStr for I2PAddress {
    type Err = NetAddressError;

    /// Parses a string into an `I2PAddress`
    fn from_str(addr: &str) -> Result<Self, Self::Err> {
        match AddressParser::new(addr).parse_i2p() {
            Some(addr) => Ok(addr),
            None => Err(NetAddressError::ParseFailed),
        }
    }
}

impl fmt::Display for I2PAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.b32.i2p", self.name)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse() {
        let addr1 = "ukeu3k5oycgaauneqgtnvselmt4yemvoilkln7jpvamvfx7dnkdq.b32.i2p".parse::<I2PAddress>();
        assert!(addr1.is_ok(), "failed to parse valid I2P address");
        let addr1 = addr1.unwrap();
        assert_eq!("UKEU3K5OYCGAAUNEQGTNVSELMT4YEMVOILKLN7JPVAMVFX7DNKDQ", addr1.name);

        let addr2 = "UKEU3K5OYCGAAUNEQGTNVSELMT4YEMVOILKLN7JPVAMVFX7DNKDQ.b32.i2p".parse::<I2PAddress>();
        assert!(addr2.is_ok(), "failed to parse valid mixed case I2P address");
        let addr2 = addr2.unwrap();
        assert_eq!("UKEU3K5OYCGAAUNEQGTNVSELMT4YEMVOILKLN7JPVAMVFX7DNKDQ", addr2.name);

        assert_eq!(addr1, addr2);

        let addr = "invalid-address.b32.i2p".parse::<I2PAddress>();
        assert!(addr.is_err(), "successfully parsed invalid I2P address");

        let addr = "ukeu3k5oycgaauneqgtnvselmt4yemvoilkln7jpvamvfx7dnkdq.b33.i2p".parse::<I2PAddress>();
        assert!(addr.is_err(), "successfully parsed invalid I2P address");

        let addr = "ukeu3k5oycgaauneqgtnvselmt4yemvoilkln7jpvamvfx7dnkdq.b32.i3p".parse::<I2PAddress>();
        assert!(addr.is_err(), "successfully parsed invalid I2P address");
    }
}
