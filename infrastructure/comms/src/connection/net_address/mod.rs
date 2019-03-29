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

pub mod i2p;
pub mod ip;
pub mod onion;
pub mod parser;

use derive_error::Error;
use std::str::FromStr;

use self::{i2p::I2PAddress, ip::SocketAddress, onion::OnionAddress};

#[derive(Debug, Error)]
pub enum NetAddressError {
    /// Failed to parse address
    ParseFailed,
    /// Specified port range is invalid
    InvalidPortRange,
}

/// A Tari network address, either IP (v4 or v6), Tor Onion or I2P.
///
/// # Examples
///
/// ```
/// use self::comms::connection::NetAddress;
///
/// let address = "propub3r6espa33w.onion:1234".parse::<NetAddress>();
///
/// assert!(address.is_ok());
/// assert!(address.unwrap().is_tor());
/// ```
#[derive(Clone, PartialEq, Eq, Debug)]
/// Represents an address which can be used to reach a node on the network
pub enum NetAddress {
    /// IPv4 and IPv6
    IP(SocketAddress),
    Tor(OnionAddress),
    I2P(I2PAddress),
}

impl NetAddress {
    /// Returns true if the [`NetAddress`] is an IP address, otherwise false
    pub fn is_ip(&self) -> bool {
        match *self {
            NetAddress::IP(_) => true,
            _ => false,
        }
    }

    /// Returns true if the [`NetAddress`] is a Tor Onion address, otherwise false
    pub fn is_tor(&self) -> bool {
        match *self {
            NetAddress::Tor(_) => true,
            _ => false,
        }
    }

    /// Returns true if the [`NetAddress`] is an I2P address, otherwise false
    pub fn is_i2p(&self) -> bool {
        match *self {
            NetAddress::I2P(_) => true,
            _ => false,
        }
    }
}

impl FromStr for NetAddress {
    type Err = NetAddressError;

    /// Parses a [`str`] into a [`NetAddress`]
    fn from_str(address: &str) -> Result<Self, Self::Err> {
        if let Ok(addr) = address.parse::<SocketAddress>() {
            Ok(addr.into())
        } else if let Ok(addr) = address.parse::<OnionAddress>() {
            Ok(addr.into())
        } else if let Ok(addr) = address.parse::<I2PAddress>() {
            Ok(addr.into())
        } else {
            Err(NetAddressError::ParseFailed)
        }
    }
}

impl From<SocketAddress> for NetAddress {
    /// Converts a [`SocketAddress`] into a [`NetAddress::IP`].
    fn from(addr: SocketAddress) -> Self {
        NetAddress::IP(addr)
    }
}

impl From<OnionAddress> for NetAddress {
    /// Converts a [`OnionAddress`] into a [`NetAddress::Tor`].
    fn from(addr: OnionAddress) -> Self {
        NetAddress::Tor(addr)
    }
}

impl From<I2PAddress> for NetAddress {
    /// Converts a [`I2PAddress`] into a [`NetAddress::I2P`].
    fn from(addr: I2PAddress) -> Self {
        NetAddress::I2P(addr)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn address_parsing() {
        // Valid string addresses
        let addr = "123.0.0.123:8000".parse::<NetAddress>();
        assert!(addr.is_ok(), "Valid IPv4 loopback address parsing failed");

        let addr = "[::1]:8080".parse::<NetAddress>();
        assert!(addr.is_ok(), "Valid IPv6 loopback address parsing failed");

        let addr = "propub3r6espa33w.onion:1234".parse::<NetAddress>();
        assert!(addr.is_ok(), "Valid Tor Onion address parsing failed");

        let addr = "ukeu3k5oycgaauneqgtnvselmt4yemvoilkln7jpvamvfx7dnkdq.b32.i2p".parse::<NetAddress>();
        assert!(addr.is_ok(), "Valid I2P address parsing failed");

        let addr = "google.com:1234".parse::<NetAddress>();
        assert!(
            addr.is_err(),
            "Invalid net address string should not have successfully parsed"
        );
    }
}
