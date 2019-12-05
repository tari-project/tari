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

use multiaddr::{AddrComponent, Multiaddr};
use std::{
    io,
    net::{IpAddr, SocketAddr},
};

/// Convert a socket address to a multiaddress
pub fn socketaddr_to_multiaddr(socket_addr: SocketAddr) -> Multiaddr {
    let mut addr: Multiaddr = match socket_addr.ip() {
        IpAddr::V4(addr) => AddrComponent::IP4(addr).into(),
        IpAddr::V6(addr) => AddrComponent::IP6(addr).into(),
    };
    addr.append(AddrComponent::TCP(socket_addr.port()));
    addr
}

/// Convert a multiaddr to a socket address required for `TcpStream`
pub fn multiaddr_to_socketaddr(addr: Multiaddr) -> io::Result<SocketAddr> {
    let mut addr_iter = addr.iter();
    let network_proto = addr_iter.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Invalid address '{}'", addr),
    ))?;
    let transport_proto = addr_iter.next().ok_or(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Invalid address '{}'", addr),
    ))?;

    if addr_iter.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{}'", addr),
        ));
    }

    match (network_proto, transport_proto) {
        (AddrComponent::IP4(host), AddrComponent::TCP(port)) => Ok((host, port).into()),
        (AddrComponent::IP6(host), AddrComponent::TCP(port)) => Ok((host, port).into()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid address '{}'", addr),
        )),
    }
}

/// Creates a Multiaddr from AddrComponents. This macro currently only supports tuple AddrComponents
macro_rules! multiaddr_from_components {
    ($first:ident ( $($first_vars:expr),+ )$(,)? $($parts:ident ( $($var:expr),+ )),* ) => {{
        let mut addr: multiaddr::Multiaddr = multiaddr::AddrComponent::$first($($first_vars),*).into();
        $(
            addr.append(multiaddr::AddrComponent::$parts($($var),+));
        )*
        addr
    }};
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{net::Ipv4Addr, str::FromStr};

    #[test]
    fn multiaddr_to_socketaddr_ok() {
        fn expect_success(addr: &str, expected_ip: &str) {
            let addr = Multiaddr::from_str(addr).unwrap();
            let sock_addr = super::multiaddr_to_socketaddr(addr).unwrap();
            assert_eq!(sock_addr.ip().to_string(), expected_ip);
        }

        expect_success("/ip4/254.0.1.2/tcp/1234", "254.0.1.2");
        expect_success("/ip6/::1/tcp/1234", "::1");
    }

    #[test]
    fn multiaddr_to_socketaddr_err() {
        fn expect_fail(addr: &str) {
            let addr = Multiaddr::from_str(addr).unwrap();
            let err = super::multiaddr_to_socketaddr(addr).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        }

        expect_fail("/ip4/254.0.1.2/tcp/1234/quic");
        expect_fail("/ip4/254.0.1.2");
        expect_fail("/p2p/QmcgpsyWgH8Y8ajJz1Cu72KnS5uo2Aa2LpzU7kinSupNKC");
    }

    #[test]
    fn multiaddr_from_components() {
        let ip: Ipv4Addr = "127.0.0.1".parse().unwrap();
        let addr = multiaddr_from_components!(IP4(ip.clone()), TCP(1456));
        let mut addr_iter = addr.iter();
        assert_eq!(addr_iter.next(), Some(AddrComponent::IP4(ip)));
        assert_eq!(addr_iter.next(), Some(AddrComponent::TCP(1456)));
        assert_eq!(addr_iter.next(), None);
    }
}
