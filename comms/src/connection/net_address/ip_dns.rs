// Copyright 2019, The Tari Project
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

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use super::{parser::AddressParser, NetAddressError};

/// Represents a Tor Onion address
#[derive(Clone, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub struct IpDnsAddress {
    pub host: String,
    pub port: u16,
}

impl IpDnsAddress {
    pub fn host(&self) -> String {
        self.host.clone()
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl FromStr for IpDnsAddress {
    type Err = NetAddressError;

    /// String parsing to an `IpDns`
    fn from_str(addr: &str) -> Result<Self, Self::Err> {
        match AddressParser::new(addr).parse_ip_dns() {
            Some(a) => Ok(a),
            None => Err(NetAddressError::ParseFailed),
        }
    }
}

impl fmt::Display for IpDnsAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse() {
        let addr1 = "foo.com:8080".parse::<IpDnsAddress>();
        assert!(addr1.is_ok(), "failed to parse valid address");
        let addr1 = addr1.unwrap();
        assert_eq!("foo.com", addr1.host);
        assert_eq!(8080, addr1.port);

        let addr1 = "nqblqa3x7ddnkp664cowka6jx4mlc26vpgdksj6uya2kbyvi77aqpqqd.onion:1234".parse::<IpDnsAddress>();
        assert!(addr1.is_ok(), "failed to parse valid address");
        let addr1 = addr1.unwrap();
        assert_eq!(
            "nqblqa3x7ddnkp664cowka6jx4mlc26vpgdksj6uya2kbyvi77aqpqqd.onion",
            addr1.host
        );
        assert_eq!(1234, addr1.port);

        let addr = "".parse::<IpDnsAddress>();
        assert!(addr.is_err(), "erroneously parsed a blank string");

        let addr = "文字漢字漢字字字字字字字字字字字.com:2020".parse::<IpDnsAddress>();
        assert!(addr.is_err(), "erroneously parsed an invalid string");

        let addr = "localhost:9999".parse::<IpDnsAddress>();
        assert!(addr.is_ok(), "failed to parse valid DNS address");

        let addr = "my-site.com:99999".parse::<IpDnsAddress>();
        assert!(addr.is_err(), "erroneously parsed invalid address");

        let addr = "my-site:9999".parse::<IpDnsAddress>();
        assert!(addr.is_err(), "erroneously parsed invalid address");
    }
}
