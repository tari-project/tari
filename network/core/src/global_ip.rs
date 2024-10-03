//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

//! Copied from autonat due to stdlib ipv4/6 is_global() being unstable

use libp2p::{multiaddr::Protocol, Multiaddr};

pub(crate) trait GlobalIp {
    fn is_global_ip(&self) -> bool;
}

impl GlobalIp for Multiaddr {
    fn is_global_ip(&self) -> bool {
        match self.iter().next() {
            Some(Protocol::Ip4(a)) => a.is_global_ip(),
            Some(Protocol::Ip6(a)) => a.is_global_ip(),
            _ => false,
        }
    }
}

impl GlobalIp for std::net::Ipv4Addr {
    // NOTE: The below logic is copied from `std::net::Ipv4Addr::is_global`, which is at the time of
    // writing behind the unstable `ip` feature.
    // See https://github.com/rust-lang/rust/issues/27709 for more info.
    fn is_global_ip(&self) -> bool {
        // Check if this address is 192.0.0.9 or 192.0.0.10. These addresses are the only two
        // globally routable addresses in the 192.0.0.0/24 range.
        if u32::from_be_bytes(self.octets()) == 0xc0000009 || u32::from_be_bytes(self.octets()) == 0xc000000a {
            return true;
        }

        // Copied from the unstable method `std::net::Ipv4Addr::is_shared`.
        fn is_shared(addr: std::net::Ipv4Addr) -> bool {
            addr.octets()[0] == 100 && (addr.octets()[1] & 0b1100_0000 == 0b0100_0000)
        }

        // Copied from the unstable method `std::net::Ipv4Addr::is_reserved`.
        //
        // **Warning**: As IANA assigns new addresses, this logic will be
        // updated. This may result in non-reserved addresses being
        // treated as reserved in code that relies on an outdated version
        // of this method.
        fn is_reserved(addr: std::net::Ipv4Addr) -> bool {
            addr.octets()[0] & 240 == 240 && !addr.is_broadcast()
        }

        // Copied from the unstable method `std::net::Ipv4Addr::is_benchmarking`.
        fn is_benchmarking(addr: std::net::Ipv4Addr) -> bool {
            addr.octets()[0] == 198 && (addr.octets()[1] & 0xfe) == 18
        }

        !self.is_private()
            && !self.is_loopback()
            && !self.is_link_local()
            && !self.is_broadcast()
            && !self.is_documentation()
            && !is_shared(*self)
            // addresses reserved for future protocols (`192.0.0.0/24`)
            && !(self.octets()[0] == 192 && self.octets()[1] == 0 && self.octets()[2] == 0)
            && !is_reserved(*self)
            && !is_benchmarking(*self)
            // Make sure the address is not in 0.0.0.0/8
            && self.octets()[0] != 0
    }
}

impl GlobalIp for std::net::Ipv6Addr {
    // NOTE: The below logic is copied from `std::net::Ipv6Addr::is_global`, which is at the time of
    // writing behind the unstable `ip` feature.
    // See https://github.com/rust-lang/rust/issues/27709 for more info.
    //
    // Note that contrary to `Ipv4Addr::is_global_ip` this currently checks for global scope
    // rather than global reachability.
    fn is_global_ip(&self) -> bool {
        // Copied from the unstable method `std::net::Ipv6Addr::is_unicast`.
        fn is_unicast(addr: &std::net::Ipv6Addr) -> bool {
            !addr.is_multicast()
        }
        // Copied from the unstable method `std::net::Ipv6Addr::is_unicast_link_local`.
        fn is_unicast_link_local(addr: &std::net::Ipv6Addr) -> bool {
            (addr.segments()[0] & 0xffc0) == 0xfe80
        }
        // Copied from the unstable method `std::net::Ipv6Addr::is_unique_local`.
        fn is_unique_local(addr: &std::net::Ipv6Addr) -> bool {
            (addr.segments()[0] & 0xfe00) == 0xfc00
        }
        // Copied from the unstable method `std::net::Ipv6Addr::is_documentation`.
        fn is_documentation(addr: &std::net::Ipv6Addr) -> bool {
            (addr.segments()[0] == 0x2001) && (addr.segments()[1] == 0xdb8)
        }

        // Copied from the unstable method `std::net::Ipv6Addr::is_unicast_global`.
        fn is_unicast_global(addr: &std::net::Ipv6Addr) -> bool {
            is_unicast(addr) &&
                !addr.is_loopback() &&
                !is_unicast_link_local(addr) &&
                !is_unique_local(addr) &&
                !addr.is_unspecified() &&
                !is_documentation(addr)
        }

        // Variation of unstable method [`std::net::Ipv6Addr::multicast_scope`] that instead of the
        // `Ipv6MulticastScope` just returns if the scope is global or not.
        // Equivalent to `Ipv6Addr::multicast_scope(..).map(|scope| matches!(scope, Ipv6MulticastScope::Global))`.
        fn is_multicast_scope_global(addr: &std::net::Ipv6Addr) -> Option<bool> {
            match addr.segments()[0] & 0x000f {
                14 => Some(true),         // Global multicast scope.
                1..=5 | 8 => Some(false), // Local multicast scope.
                _ => None,                // Unknown multicast scope.
            }
        }

        match is_multicast_scope_global(self) {
            Some(true) => true,
            None => is_unicast_global(self),
            _ => false,
        }
    }
}
