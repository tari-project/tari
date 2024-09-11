// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, net::Ipv4Addr, str::FromStr};

use multiaddr::{Multiaddr, Protocol};
use serde_derive::{Deserialize, Serialize};

/// A MultiaddrRange for testing purposes that matches any IPv4 address and any port
pub const IP4_TCP_TEST_ADDR_RANGE: &str = "/ip4/127.*.*.*/tcp/*";

/// ----------------- MultiaddrRange -----------------
/// A struct containing either an Ipv4AddrRange or a Multiaddr. If a range of IP addresses and/or ports needs to be
/// specified, the MultiaddrRange can be used, but it only supports IPv4 addresses with the TCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiaddrRange {
    ipv4_addr_range: Option<Ipv4AddrRange>,
    multiaddr: Option<Multiaddr>,
}

impl FromStr for MultiaddrRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(multiaddr) = Multiaddr::from_str(s) {
            Ok(MultiaddrRange {
                ipv4_addr_range: None,
                multiaddr: Some(multiaddr),
            })
        } else if let Ok(ipv4_addr_range) = Ipv4AddrRange::from_str(s) {
            Ok(MultiaddrRange {
                ipv4_addr_range: Some(ipv4_addr_range),
                multiaddr: None,
            })
        } else {
            Err("Invalid format for both Multiaddr and Ipv4AddrRange".to_string())
        }
    }
}

impl fmt::Display for MultiaddrRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ipv4_addr_range) = &self.ipv4_addr_range {
            write!(f, "{}", ipv4_addr_range)
        } else if let Some(multiaddr) = &self.multiaddr {
            write!(f, "{}", multiaddr)
        } else {
            write!(f, "None")
        }
    }
}

impl MultiaddrRange {
    /// Check if the given Multiaddr is contained within the MultiaddrRange range
    pub fn contains(&self, addr: &Multiaddr) -> bool {
        if let Some(ipv4_addr_range) = &self.ipv4_addr_range {
            return ipv4_addr_range.contains(addr);
        }
        if let Some(multiaddr) = &self.multiaddr {
            return multiaddr == addr;
        }
        false
    }
}

// ----------------- Ipv4AddrRange -----------------
// A struct containing an Ipv4Range and a PortRange
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ipv4AddrRange {
    ip_range: Ipv4Range,
    port_range: PortRange,
}

impl FromStr for Ipv4AddrRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 5 {
            return Err("Invalid multiaddr format".to_string());
        }

        if parts[1] != "ip4" {
            return Err("Only IPv4 addresses are supported".to_string());
        }

        let ip_range = Ipv4Range::new(parts[2])?;
        if parts[3] != "tcp" {
            return Err("Only TCP protocol is supported".to_string());
        }

        let port_range = PortRange::new(parts[4])?;
        Ok(Ipv4AddrRange { ip_range, port_range })
    }
}

impl Ipv4AddrRange {
    fn contains(&self, addr: &Multiaddr) -> bool {
        let mut ip = None;
        let mut port = None;

        for protocol in addr {
            match protocol {
                Protocol::Ip4(ipv4) => ip = Some(ipv4),
                Protocol::Tcp(tcp_port) => port = Some(tcp_port),
                _ => {},
            }
        }

        if let (Some(ip), Some(port)) = (ip, port) {
            return self.ip_range.contains(ip) && self.port_range.contains(port);
        }

        false
    }
}

impl fmt::Display for Ipv4AddrRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/ip4/{}/tcp/{}", self.ip_range, self.port_range)
    }
}

// ----------------- Ipv4Range -----------------
// A struct containing the start and end Ipv4Addr
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ipv4Range {
    start: Ipv4Addr,
    end: Ipv4Addr,
}

impl Ipv4Range {
    fn new(range_str: &str) -> Result<Self, String> {
        let parts: Vec<&str> = range_str.split('.').collect();
        if parts.len() != 4 {
            return Err("Invalid IP range format".to_string());
        }

        let mut start_octets = [0u8; 4];
        let mut end_octets = [0u8; 4];

        for (i, part) in parts.iter().enumerate() {
            if i == 0 {
                start_octets[i] = part.parse().map_err(|_| "Invalid first octet".to_string())?;
                end_octets[i] = start_octets[i];
            } else if part == &"*" {
                start_octets[i] = 0;
                end_octets[i] = u8::MAX;
            } else if part.contains(':') {
                let range_parts: Vec<&str> = part.split(':').collect();
                if range_parts.len() != 2 {
                    return Err("Invalid range format".to_string());
                }
                start_octets[i] = range_parts[0].parse().map_err(|_| "Invalid range start".to_string())?;
                end_octets[i] = range_parts[1].parse().map_err(|_| "Invalid range end".to_string())?;
            } else {
                start_octets[i] = part.parse().map_err(|_| "Invalid octet".to_string())?;
                end_octets[i] = start_octets[i];
            }
        }

        Ok(Ipv4Range {
            start: Ipv4Addr::new(start_octets[0], start_octets[1], start_octets[2], start_octets[3]),
            end: Ipv4Addr::new(end_octets[0], end_octets[1], end_octets[2], end_octets[3]),
        })
    }

    fn contains(&self, addr: Ipv4Addr) -> bool {
        let octets = addr.octets();
        let start_octets = self.start.octets();
        let end_octets = self.end.octets();

        for i in 0..4 {
            if octets[i] < start_octets[i] || octets[i] > end_octets[i] {
                return false;
            }
        }
        true
    }
}

impl fmt::Display for Ipv4Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start_octets = self.start.octets();
        let end_octets = self.end.octets();
        write!(
            f,
            "{}.{}.{}.{}",
            start_octets[0],
            if start_octets[1] == 0 && end_octets[1] == u8::MAX {
                "*".to_string()
            } else if start_octets[1] == end_octets[1] {
                start_octets[1].to_string()
            } else {
                format!("{}:{}", start_octets[1], end_octets[1])
            },
            if start_octets[2] == 0 && end_octets[2] == u8::MAX {
                "*".to_string()
            } else if start_octets[2] == end_octets[2] {
                start_octets[2].to_string()
            } else {
                format!("{}:{}", start_octets[2], end_octets[2])
            },
            if start_octets[3] == 0 && end_octets[3] == u8::MAX {
                "*".to_string()
            } else if start_octets[3] == end_octets[3] {
                start_octets[3].to_string()
            } else {
                format!("{}:{}", start_octets[3], end_octets[3])
            }
        )
    }
}

// ----------------- PortRange -----------------
// A struct containing the start and end port
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortRange {
    start: u16,
    end: u16,
}

impl PortRange {
    fn new(range_str: &str) -> Result<Self, String> {
        if range_str == "*" {
            return Ok(PortRange {
                start: 0,
                end: u16::MAX,
            });
        }

        if range_str.contains(':') {
            let parts: Vec<&str> = range_str.split(':').collect();
            if parts.len() != 2 {
                return Err("Invalid port range format".to_string());
            }
            let start = parts[0].parse().map_err(|_| "Invalid port range start".to_string())?;
            let end = parts[1].parse().map_err(|_| "Invalid port range end".to_string())?;
            return Ok(PortRange { start, end });
        }

        let port = range_str.parse().map_err(|_| "Invalid port".to_string())?;
        Ok(PortRange { start: port, end: port })
    }

    fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

impl fmt::Display for PortRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start == 0 && self.end == u16::MAX {
            write!(f, "*")
        } else if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}:{}", self.start, self.end)
        }
    }
}

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv6Addr};

    use crate::{
        multiaddr::Multiaddr,
        net_address::{multiaddr_range::IP4_TCP_TEST_ADDR_RANGE, MultiaddrRange},
    };

    #[test]
    fn it_parses_properly_and_verify_inclusion() {
        // MultiaddrRange for ip4 with tcp

        let my_addr_range: MultiaddrRange = "/ip4/127.*.100:200.*/tcp/18000:19000".parse().unwrap();
        let addr: Multiaddr = "/ip4/127.0.150.1/tcp/18500".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.150.1/tcp/17500".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.50.1/tcp/18500".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));

        let my_addr_range: MultiaddrRange = "/ip4/127.*.100:200.*/tcp/*".parse().unwrap();
        let addr: Multiaddr = "/ip4/127.0.150.1/tcp/18500".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.150.1/tcp/17500".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.50.1/tcp/17500".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));

        let my_addr_range: MultiaddrRange = "/ip4/127.0.0.1/tcp/18000:19000".parse().unwrap();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/18500".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.1.1/tcp/18500".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/17500".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));

        let my_addr_range: MultiaddrRange = "/ip4/127.0.0.1/tcp/18188".parse().unwrap();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/18188".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.1.1/tcp/18188".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/18189".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));

        let my_addr_range: MultiaddrRange = IP4_TCP_TEST_ADDR_RANGE.parse().unwrap();
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/18188".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/18189".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.1.2.3/tcp/555".parse().unwrap();
        assert!(my_addr_range.contains(&addr));

        // MultiaddrRange for other protocols

        let my_addr_range: MultiaddrRange = "/ip4/127.0.0.1/udt/sctp/5678".parse().unwrap();
        let addr: Multiaddr = "/ip4/127.0.0.1/udt/sctp/5678".parse().unwrap();
        assert!(my_addr_range.contains(&addr));
        let addr: Multiaddr = "/ip4/127.0.0.1/udt/sctp/5679".parse().unwrap();
        assert!(!my_addr_range.contains(&addr));

        let my_addr_range: MultiaddrRange = Multiaddr::from(IpAddr::V6(Ipv6Addr::new(0x2001, 0x2, 0, 0, 0x1, 0, 0, 0)))
            .to_string()
            .parse()
            .unwrap();
        let addr = Multiaddr::from(IpAddr::V6(Ipv6Addr::new(0x2001, 0x2, 0, 0, 0x1, 0, 0, 0)));
        assert!(my_addr_range.contains(&addr));
        let addr = Multiaddr::from(IpAddr::V6(Ipv6Addr::new(0x2001, 0x2, 0, 0, 0, 0, 0, 0)));
        assert!(!my_addr_range.contains(&addr));
    }
}
