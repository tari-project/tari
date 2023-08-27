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

use std::sync::Arc;

use multiaddr::{Multiaddr, Protocol};

use crate::transports::predicate::Predicate;

#[derive(Debug, Clone)]
pub struct TorProxyOpts {
    /// If the dialed address matches any of these addresses, the SOCKS proxy is bypassed and direct TCP connection is
    /// used.
    pub bypass_addresses: Arc<Vec<Multiaddr>>,
    /// Use a direct TCP/IP connection if a TCP address is given instead of the tor proxy.
    pub bypass_for_tcpip: bool,
}

impl TorProxyOpts {
    pub fn to_bypass_predicate(&self) -> impl Predicate<Multiaddr> {
        let config = self.clone();
        move |addr: &Multiaddr| -> bool {
            config.bypass_addresses.contains(addr) || (config.bypass_for_tcpip && is_tcp_address(addr))
        }
    }
}

impl Default for TorProxyOpts {
    fn default() -> Self {
        Self {
            bypass_addresses: Arc::new(vec![]),
            // Private by default
            bypass_for_tcpip: false,
        }
    }
}

fn is_tcp_address(addr: &Multiaddr) -> bool {
    use Protocol::{Dns4, Dns6, Ip4, Ip6, Tcp};
    let mut iter = addr.iter();
    let protocol = iter.next();
    if !matches!(protocol, Some(Ip4(_)) | Some(Ip6(_)) | Some(Dns4(_)) | Some(Dns6(_))) {
        return false;
    }

    let protocol = iter.next();
    matches!(protocol, Some(Tcp(_)))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn is_tcpip_address() {
        let expect_false = [
            "/onion/aaimaq4ygg2iegci:1234",
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234",
        ];

        let expect_true = [
            "/ip4/1.2.3.4/tcp/1234",
            "/ip4/127.0.0.1/tcp/9998",
            "/dns4/taiji.com/tcp/80",
        ];

        expect_true.iter().for_each(|addr| {
            let addr = addr.parse().unwrap();
            assert!(super::is_tcp_address(&addr));
        });

        expect_false.iter().for_each(|addr| {
            let addr = addr.parse().unwrap();
            assert!(!super::is_tcp_address(&addr));
        });
    }

    #[test]
    fn proxy_opts() {
        let expect_false = [
            "/onion/aaimaq4ygg2iegci:1234",
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234",
        ]
        .iter()
        .map(|a| a.parse().unwrap())
        .collect::<Vec<Multiaddr>>();

        let expect_true = [
            "/ip4/1.2.3.4/tcp/1234",
            "/ip4/127.0.0.1/tcp/9998",
            "/dns4/taiji.com/tcp/80",
        ]
        .iter()
        .map(|a| a.parse().unwrap())
        .collect::<Vec<Multiaddr>>();

        let opts = TorProxyOpts {
            bypass_addresses: expect_false.clone().into(),
            ..Default::default()
        };
        let predicate = opts.to_bypass_predicate();
        expect_false.iter().for_each(|addr| {
            assert!(predicate.check(addr));
        });

        expect_true.iter().for_each(|addr| {
            assert!(!predicate.check(addr));
        });

        let opts = TorProxyOpts {
            bypass_for_tcpip: true,
            ..Default::default()
        };
        let predicate = opts.to_bypass_predicate();
        expect_true.iter().for_each(|addr| {
            assert!(predicate.check(addr));
        });

        expect_false.iter().for_each(|addr| {
            assert!(!predicate.check(addr));
        });
    }
}
