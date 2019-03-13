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

use derive_error::Error;
use std::str::FromStr;

use crate::connection::{i2p::I2PAddress, onion::OnionAddress, p2p::SocketAddress};

#[derive(Debug, Error)]
pub enum NetAddressError {
    /// Failed to parse address
    ParseFailed,
    /// Specified port range is invalid
    InvalidPortRange,
}

/// Represents an address which can be used to reach a node on the network
pub enum NetAddress {
    /// IPv4 and IPv6
    IP(SocketAddress),
    Tor(OnionAddress),
    I2P(I2PAddress),
}

impl FromStr for NetAddress {
    type Err = NetAddressError;

    fn from_str(address: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = address.parse::<SocketAddress>() {
            Ok(NetAddress::IP(addr))
        } else if let Ok(addr) = address.parse::<OnionAddress>() {
            Ok(NetAddress::Tor(addr))
        } else if let Ok(addr) = address.parse::<I2PAddress>() {
            Ok(NetAddress::I2P(addr))
        } else {
            Err(NetAddressError::ParseFailed)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn string_address_parsing() {
        // Valid string addresses
        let addr = "127.0.0.1:8000".parse::<SocketAddress>();
        assert!(addr.is_ok(), "Valid IPv4 loopback address parsing failed");

        let addr = "[::1]:8080".parse::<SocketAddress>();
        assert!(addr.is_ok(), "Valid IPv6 loopback address parsing failed");

        // Invalid string addresses
        macro_rules! check_addr_fail {
            ($addr:ident) => {
                match $addr.err().unwrap() {
                    NetAddressError::ParseFailed => {},
                    _ => panic!("Address parsing returned unexpected error type"),
                }
            };
        }

        let addr = "123.123.123.123".parse::<SocketAddress>();
        assert!(addr.is_err(), "Invalid IPv4 address was erroneously successfully parsed");
        check_addr_fail!(addr);

        let addr = "fe80::1ff:fe23:4567:890a:8080".parse::<SocketAddress>();
        assert!(addr.is_err(), "Invalid IPv6 address was erroneously successfully parsed");
        check_addr_fail!(addr);
    }
}
