//  Copyright 2026 The Tari Project
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
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use multiaddr::{Multiaddr, Protocol};

use crate::{
    net_address::{MultiaddrRange, is_external_address},
    utils::multiaddr::socketaddr_to_multiaddr,
};

/// Rules for the IP address that a DNS address resolves to, applied by the transport just before it connects. The
/// name of a DNS address is checked when the address is stored, but the IP it resolves to can be anything.
///
/// When a DNS address is dialled through a SOCKS5 proxy (Tor, or a generic proxy with `TransportType::Socks5`), the
/// proxy resolves the name, so these checks are not applied and the proxy's own egress policy decides what it connects
/// to.
#[derive(Debug, Clone)]
pub struct DialAddressPolicy {
    /// Permit resolved addresses that are not globally routable (loopback, private, link-local, ...)
    pub allow_internal: bool,
    /// Resolved addresses in these ranges are not dialled
    pub excluded: Vec<MultiaddrRange>,
}

impl DialAddressPolicy {
    /// Checks the resolved `socket_addr` of `addr`. Addresses that are not DNS names are not checked.
    pub fn check_resolved(&self, addr: &Multiaddr, socket_addr: &SocketAddr) -> io::Result<()> {
        if !is_dns_address(addr) {
            return Ok(());
        }
        // A NAT64 address reaches the IPv4 address embedded in it, so both are checked
        let mut resolved = vec![socketaddr_to_multiaddr(socket_addr)];
        if let Some(ipv4) = nat64_embedded_ipv4(socket_addr.ip()) {
            resolved.push(socketaddr_to_multiaddr(&SocketAddr::new(
                ipv4.into(),
                socket_addr.port(),
            )));
        }
        let is_internal =
            is_local_use_nat64(socket_addr.ip()) || resolved.iter().any(|resolved| !is_external_address(resolved));
        if !self.allow_internal && is_internal {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("'{addr}' resolved to a non-global address"),
            ));
        }
        if resolved
            .iter()
            .any(|resolved| self.excluded.iter().any(|range| range.contains(resolved)))
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("'{addr}' resolved to an excluded address"),
            ));
        }
        Ok(())
    }
}

/// The IPv4 address in the last 32 bits of an address in the well-known NAT64 prefix 64:ff9b::/96 (RFC 6052)
fn nat64_embedded_ipv4(ip: IpAddr) -> Option<Ipv4Addr> {
    let IpAddr::V6(ip) = ip else {
        return None;
    };
    let [s0, s1, s2, s3, s4, s5, high, low] = ip.segments();
    if [s0, s1, s2, s3, s4, s5] != [0x64, 0xff9b, 0, 0, 0, 0] {
        return None;
    }
    let [a, b] = high.to_be_bytes();
    let [c, d] = low.to_be_bytes();
    Some(Ipv4Addr::new(a, b, c, d))
}

/// The local-use NAT64 prefix 64:ff9b:1::/48 (RFC 8215) translates to addresses chosen by the local network
fn is_local_use_nat64(ip: IpAddr) -> bool {
    let IpAddr::V6(ip) = ip else {
        return false;
    };
    let [first, second, third, ..] = ip.segments();
    first == 0x64 && second == 0xff9b && third == 1
}

fn is_dns_address(addr: &Multiaddr) -> bool {
    matches!(
        addr.iter().next(),
        Some(Protocol::Dns(_) | Protocol::Dns4(_) | Protocol::Dns6(_) | Protocol::Dnsaddr(_))
    )
}

#[cfg(test)]
mod test {
    use super::*;

    fn policy(allow_internal: bool, excluded: &[&str]) -> DialAddressPolicy {
        DialAddressPolicy {
            allow_internal,
            excluded: excluded.iter().map(|s| s.parse().unwrap()).collect(),
        }
    }

    #[test]
    fn internal_resolved_address_is_rejected_unless_allowed() {
        let addr = "/dns4/example.com/tcp/18189".parse().unwrap();
        for ip in ["127.0.0.1:18189", "10.1.2.3:18189", "169.254.169.254:80", "[::1]:18189"] {
            let socket_addr = ip.parse().unwrap();
            let err = policy(false, &[]).check_resolved(&addr, &socket_addr).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
            policy(true, &[]).check_resolved(&addr, &socket_addr).unwrap();
        }
        policy(false, &[])
            .check_resolved(&addr, &"23.23.23.23:18189".parse().unwrap())
            .unwrap();
    }

    #[test]
    fn excluded_range_applies_to_resolved_ip() {
        let addr = "/dns4/example.com/tcp/18189".parse().unwrap();
        let socket_addr = "23.23.23.23:18189".parse().unwrap();
        let excluded = policy(true, &["/ip4/23.23.*.*/tcp/*"]);
        let err = excluded.check_resolved(&addr, &socket_addr).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        let excluded = policy(false, &["/ip4/23.23.23.23/tcp/18189"]);
        let err = excluded.check_resolved(&addr, &socket_addr).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        policy(false, &["/ip4/24.*.*.*/tcp/*"])
            .check_resolved(&addr, &socket_addr)
            .unwrap();
    }

    #[test]
    fn ipv4_mapped_resolved_address_is_matched_against_ipv4_exclusions() {
        let addr = "/dns4/example.com/tcp/18189".parse().unwrap();
        let socket_addr = "[::ffff:23.23.23.23]:18189".parse().unwrap();
        let err = policy(false, &["/ip4/23.23.*.*/tcp/*"])
            .check_resolved(&addr, &socket_addr)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn nat64_resolved_address_is_checked_as_its_ipv4_address() {
        let addr = "/dns6/example.com/tcp/18189".parse().unwrap();

        let embeds_loopback = "[64:ff9b::7f00:1]:18189".parse().unwrap();
        let err = policy(false, &[]).check_resolved(&addr, &embeds_loopback).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        policy(true, &[]).check_resolved(&addr, &embeds_loopback).unwrap();

        let embeds_public = "[64:ff9b::1717:1717]:18189".parse().unwrap();
        policy(false, &[]).check_resolved(&addr, &embeds_public).unwrap();
        let err = policy(false, &["/ip4/23.23.*.*/tcp/*"])
            .check_resolved(&addr, &embeds_public)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        let local_use = "[64:ff9b:1::1717:1717]:18189".parse().unwrap();
        let err = policy(false, &[]).check_resolved(&addr, &local_use).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        policy(true, &[]).check_resolved(&addr, &local_use).unwrap();
    }

    #[test]
    fn literal_ip_addresses_are_not_checked() {
        let addr = "/ip4/127.0.0.1/tcp/18189".parse().unwrap();
        policy(false, &[])
            .check_resolved(&addr, &"127.0.0.1:18189".parse().unwrap())
            .unwrap();
    }
}
