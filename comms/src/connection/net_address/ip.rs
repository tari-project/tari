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

use crate::connection::NetAddressError;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    str::FromStr,
};

/// Represents an {IPv4, IPv6} address and port
#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct SocketAddress(SocketAddr);

impl SocketAddress {
    pub fn ip(&self) -> IpAddr {
        self.0.ip()
    }

    pub fn host(&self) -> String {
        self.0.ip().to_string()
    }

    pub fn port(&self) -> u16 {
        self.0.port()
    }
}

impl FromStr for SocketAddress {
    type Err = NetAddressError;

    fn from_str(addr: &str) -> Result<Self, Self::Err> {
        let socket_addr = addr.parse::<SocketAddr>().map_err(|_| NetAddressError::ParseFailed)?;

        Ok(Self(socket_addr))
    }
}

impl<I: Into<IpAddr>> From<(I, u16)> for SocketAddress {
    fn from(v: (I, u16)) -> Self {
        Self(v.into())
    }
}

impl ToSocketAddrs for SocketAddress {
    type Iter = std::option::IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
        self.0.to_socket_addrs()
    }
}

impl fmt::Display for SocketAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.ip(), self.port())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn string_address_parsing() {
        // Testing our implementation of address parsing which uses SocketAddr internally.
        // The SocketAddr is used "as is" which means technically we're testing SocketAddr,
        // however this shows the expected usage of SocketAddress without knowing about SocketAddr

        // Valid string addresses
        let addr = "127.0.0.1:8000".parse::<SocketAddress>();
        assert!(addr.is_ok(), "Valid IPv4 loopback address parsing failed");

        let addr = "[::1]:8080".parse::<SocketAddress>();
        assert!(addr.is_ok(), "Valid IPv6 loopback address parsing failed");

        let addr = "123.122.234.100:8080".parse::<SocketAddress>();
        assert!(addr.is_ok(), "Valid IPv4 address parsing failed");

        let addr = "[fe80::1ff:fe23:4567:890a]:8080".parse::<SocketAddress>();
        assert!(addr.is_ok(), "Valid IPv6 address parsing failed");

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
        assert!(
            addr.is_err(),
            "Invalid IPv4 address was erroneously successfully parsed"
        );
        check_addr_fail!(addr);

        let addr = "fe80::1ff:fe23:4567:890a:8080".parse::<SocketAddress>();
        assert!(
            addr.is_err(),
            "Invalid IPv6 address was erroneously successfully parsed"
        );
        check_addr_fail!(addr);
    }
}
