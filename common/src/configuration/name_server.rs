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
    fmt::{Display, Formatter},
    net::SocketAddr,
    str::FromStr,
};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DnsNameServer {
    #[default]
    System,
    Custom {
        addr: SocketAddr,
        dns_name: Option<String>,
    },
}

impl DnsNameServer {
    pub fn custom(addr: SocketAddr, dns_name: Option<String>) -> Self {
        Self::Custom { addr, dns_name }
    }
}

impl Display for DnsNameServer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DnsNameServer::System => write!(f, "system"),
            DnsNameServer::Custom {
                addr,
                dns_name: Some(dns_name),
            } => {
                write!(f, "{}/{}", addr, dns_name)
            },
            DnsNameServer::Custom { addr, .. } => {
                write!(f, "{}", addr)
            },
        }
    }
}

impl FromStr for DnsNameServer {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s
            .to_string()
            .replace(" ", "")
            .replace("\"", "")
            .replace("'", "")
            .to_ascii_lowercase();
        let mut split = s.splitn(2, '/');
        let addr = split
            .next()
            .ok_or_else(|| anyhow!("failed to parse DNS name server 'addr'"))?;
        if addr == "system" {
            return Ok(Self::System);
        }
        let dns_name = split.next();
        Ok(Self::Custom {
            addr: addr.parse()?,
            dns_name: dns_name.map(ToString::to_string),
        })
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    #[test]
    fn dns_name_server_test() {
        // create dns name server
        let ipv4 = Ipv4Addr::new(127, 0, 0, 1);
        let ip = IpAddr::V4(ipv4);
        let socket = SocketAddr::new(ip, 8080);
        let dns = DnsNameServer::custom(socket, Some(String::from("my_dns")));

        // test formatting
        assert_eq!(format!("{}", dns), "127.0.0.1:8080/my_dns");

        // from str
        let new_dns = DnsNameServer::from_str("'127.0.0.1:8080/my_dns'").unwrap();
        assert_eq!(new_dns, dns);
        let new_dns = DnsNameServer::from_str("\"127.0.0.1:8080/my_dns\"").unwrap();
        assert_eq!(new_dns, dns);
        let new_dns = DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap();
        assert_eq!(new_dns, dns);

        // default
        assert_eq!(DnsNameServer::default(), DnsNameServer::System);
    }

    #[test]
    fn to_string_from_str() {
        let ipv4 = Ipv4Addr::new(127, 0, 0, 1);
        let ip = IpAddr::V4(ipv4);
        let socket = SocketAddr::new(ip, 8080);
        let dns = DnsNameServer::custom(socket, Some(String::from("my_dns")));
        let parsed = dns.to_string().parse::<DnsNameServer>().unwrap();
        assert_eq!(dns, parsed);

        let parsed = DnsNameServer::System.to_string().parse::<DnsNameServer>().unwrap();
        assert_eq!(parsed, DnsNameServer::System);
    }
}
