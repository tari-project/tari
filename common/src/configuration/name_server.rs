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
    fmt::{Display, Formatter},
    net::SocketAddr,
    str::FromStr,
};

use anyhow::anyhow;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DnsNameServer {
    pub addr: SocketAddr,
    pub dns_name: String,
}

impl DnsNameServer {
    pub fn new(addr: SocketAddr, dns_name: String) -> Self {
        Self { addr, dns_name }
    }
}

impl Display for DnsNameServer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.addr, self.dns_name)
    }
}

impl FromStr for DnsNameServer {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.splitn(2, '/');
        let addr = split.next().ok_or_else(|| anyhow!("failed to parse DNS name server"))?;
        let dns_name = split.next().ok_or_else(|| anyhow!("failed to parse name server"))?;
        Ok(Self {
            addr: addr.parse()?,
            dns_name: dns_name.to_string(),
        })
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use super::*;

    #[test]
    fn dns_name_server_test() {
        // create dns name server
        let ipv4 = Ipv4Addr::new(127, 0, 0, 1);
        let ip = IpAddr::V4(ipv4);
        let socket = SocketAddr::new(ip, 8080);
        let dns = DnsNameServer::new(socket, String::from("my_dns"));

        // test formatting
        assert_eq!(format!("{}", dns), "127.0.0.1:8080/my_dns");

        // from str
        let new_dns = DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap();
        assert_eq!(new_dns, dns);
    }
}
