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
use std::{
    io,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
};

use crate::multiaddr::{Multiaddr, Protocol};

/// Convert a multiaddr to a socket address required for `TcpStream`.
/// Resolves `/dns`, `/dns4`, and `/dns6` hostnames to a matching IP socket address.
pub fn multiaddr_to_socketaddr(addr: &Multiaddr) -> io::Result<SocketAddr> {
    let mut addr_iter = addr.iter();
    let network_proto = addr_iter
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid address '{addr}'")))?;
    let transport_proto = addr_iter
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid address '{addr}'")))?;

    if addr_iter.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{addr}'"),
        ));
    }

    match (&network_proto, transport_proto) {
        (Protocol::Dns(domain) | Protocol::Dns4(domain) | Protocol::Dns6(domain), Protocol::Tcp(port)) => {
            let resolved = (domain.as_ref(), port)
                .to_socket_addrs()
                .map_err(|_e| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid domain '{domain}'")))?;
            select_dns_socketaddr(&network_proto, resolved)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Invalid domain '{domain}'")))
        },
        (Protocol::Ip4(host), Protocol::Tcp(port)) => Ok((*host, port).into()),
        (Protocol::Ip6(host), Protocol::Tcp(port)) => Ok((*host, port).into()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{addr}'"),
        )),
    }
}

fn select_dns_socketaddr(
    network_proto: &Protocol<'_>,
    mut resolved: impl Iterator<Item = SocketAddr>,
) -> Option<SocketAddr> {
    match network_proto {
        Protocol::Dns(_) => resolved.next(),
        Protocol::Dns4(_) => resolved.find(SocketAddr::is_ipv4),
        Protocol::Dns6(_) => resolved.find(SocketAddr::is_ipv6),
        _ => None,
    }
}

/// Convert a socket address to a multiaddress. Assumes the protocol is Tcp
pub fn socketaddr_to_multiaddr(socket_addr: &SocketAddr) -> Multiaddr {
    let mut addr: Multiaddr = match socket_addr.ip() {
        IpAddr::V4(addr) => Protocol::Ip4(addr).into(),
        IpAddr::V6(addr) => Protocol::Ip6(addr).into(),
    };
    addr.push(Protocol::Tcp(socket_addr.port()));
    addr
}

#[cfg(test)]
mod test {
    use std::{net::Ipv4Addr, str::FromStr};

    use multiaddr::multiaddr;

    use super::*;

    #[test]
    fn multiaddr_to_socketaddr_ok() {
        fn expect_success(addr: &str, expected_ips: &[&str]) {
            let addr = Multiaddr::from_str(addr).unwrap();
            let sock_addr = super::multiaddr_to_socketaddr(&addr).unwrap();
            assert!(expected_ips.iter().any(|ip| *ip == sock_addr.ip().to_string()));
        }

        expect_success("/ip4/254.0.1.2/tcp/1234", &["254.0.1.2"]);
        expect_success("/ip6/::1/tcp/1234", &["::1"]);
        // Test DNS name resolution across Dns, Dns4, and Dns6
        expect_success("/dns/localhost/tcp/1234", &["127.0.0.1", "::1"]);
        expect_success("/dns4/localhost/tcp/1234", &["127.0.0.1"]);

        // `/dns6/localhost` must never return an IPv4 address such as `127.0.0.1`
        let dns6_localhost = Multiaddr::from_str("/dns6/localhost/tcp/1234").unwrap();
        let host_has_ipv6_localhost = ("localhost", 1234u16)
            .to_socket_addrs()
            .map(|mut addrs| addrs.any(|a| a.is_ipv6()))
            .unwrap_or(false);
        if host_has_ipv6_localhost {
            let sock_addr = super::multiaddr_to_socketaddr(&dns6_localhost).unwrap();
            assert!(
                sock_addr.is_ipv6(),
                "expected IPv6 for /dns6/localhost, got {sock_addr}"
            );
            assert_ne!(sock_addr.ip().to_string(), "127.0.0.1");
        } else {
            assert!(super::multiaddr_to_socketaddr(&dns6_localhost).is_err());
        }

        // Deterministic dual-stack filtering checks for Dns4 and Dns6
        let v4: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        let v6: SocketAddr = "[::1]:1234".parse().unwrap();
        assert_eq!(
            super::select_dns_socketaddr(&Protocol::Dns6("localhost".into()), [v4, v6].into_iter()),
            Some(v6)
        );
        assert_eq!(
            super::select_dns_socketaddr(&Protocol::Dns6("localhost".into()), [v4].into_iter()),
            None
        );
        assert_eq!(
            super::select_dns_socketaddr(&Protocol::Dns4("localhost".into()), [v6, v4].into_iter()),
            Some(v4)
        );
    }

    #[test]
    fn multiaddr_dns_to_socketaddr_ok() {
        let addr = Multiaddr::from_str("/dns4/localhost/tcp/1234").unwrap();
        let sock_addr = super::multiaddr_to_socketaddr(&addr).unwrap();
        assert!(sock_addr.ip().is_loopback());
        assert!(sock_addr.is_ipv4());
    }

    #[test]
    fn multiaddr_to_socketaddr_err() {
        fn expect_fail(addr: &str) {
            let addr = Multiaddr::from_str(addr).unwrap();
            let err = super::multiaddr_to_socketaddr(&addr).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        }

        expect_fail("/ip4/254.0.1.2/tcp/1234/quic");
        expect_fail("/ip4/254.0.1.2");
        expect_fail("/p2p/QmcgpsyWgH8Y8ajJz1Cu72KnS5uo2Aa2LpzU7kinSupNKC");
        expect_fail("/dnsaddr/tari.com/tcp/1234");
        expect_fail("/dns4/doesntexist.theresnotldlikethis/tcp/1234")
    }

    #[test]
    fn multiaddr_from_components() {
        let ip: Ipv4Addr = "127.0.0.1".parse().unwrap();
        let addr = multiaddr!(Ip4(ip), Tcp(1456u16));
        let mut addr_iter = addr.iter();
        assert_eq!(addr_iter.next(), Some(Protocol::Ip4(ip)));
        assert_eq!(addr_iter.next(), Some(Protocol::Tcp(1456)));
        assert_eq!(addr_iter.next(), None);
    }
}
