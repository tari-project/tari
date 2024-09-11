// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt, net::Ipv4Addr, ops::Deref, slice, str::FromStr};

use multiaddr::{Multiaddr, Protocol};
use serde::{
    de,
    de::{Error, SeqAccess, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
};

/// A MultiaddrRange for testing purposes that matches any IPv4 address and any port
pub const IP4_TCP_TEST_ADDR_RANGE: &str = "/ip4/127.*.*.*/tcp/*";

/// ----------------- MultiaddrRange -----------------
/// A struct containing either an Ipv4AddrRange or a Multiaddr. If a range of IP addresses and/or ports needs to be
/// specified, the MultiaddrRange can be used, but it only supports IPv4 addresses with the TCP protocol.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
                start_octets[i] = part.parse().map_err(|_| "Invalid first IPv4 octet".to_string())?;
                end_octets[i] = start_octets[i];
            } else if part == &"*" {
                start_octets[i] = 0;
                end_octets[i] = u8::MAX;
            } else if part.contains(':') {
                let range_parts: Vec<&str> = part.split(':').collect();
                if range_parts.len() != 2 {
                    return Err(format!("Invalid range format for IPv4 octet {}", i));
                }
                start_octets[i] = range_parts[0]
                    .parse()
                    .map_err(|_| format!("Invalid range start for IPv4 octet {}", i))?;
                end_octets[i] = range_parts[1]
                    .parse()
                    .map_err(|_| format!("Invalid range end for IPv4 octet {}", i))?;
            } else {
                start_octets[i] = part.parse().map_err(|_| format!("Invalid IPv4 octet {}", i))?;
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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
            let start = parts[0]
                .parse()
                .map_err(|_| format!("Invalid port range start '{}'", parts[0]))?;
            let end = parts[1]
                .parse()
                .map_err(|_| format!("Invalid port range end '{}'", parts[1]))?;
            if end < start {
                return Err(format!(
                    "Invalid port range '{}', end `{}` is less than start `{}`",
                    range_str, end, start
                ));
            }
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
        if self.start <= 1 && self.end == u16::MAX {
            write!(f, "*")
        } else if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}:{}", self.start, self.end)
        }
    }
}

/// Supports deserialization from a sequence of strings or comma-delimited strings
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct MultiaddrRangeList(Vec<MultiaddrRange>);

impl MultiaddrRangeList {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn with_capacity(size: usize) -> Self {
        Self(Vec::with_capacity(size))
    }

    pub fn into_vec(self) -> Vec<MultiaddrRange> {
        self.0
    }

    pub fn as_slice(&self) -> &[MultiaddrRange] {
        self.0.as_slice()
    }
}

impl Deref for MultiaddrRangeList {
    type Target = [MultiaddrRange];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[MultiaddrRange]> for MultiaddrRangeList {
    fn as_ref(&self) -> &[MultiaddrRange] {
        self.0.as_ref()
    }
}

impl From<Vec<MultiaddrRange>> for MultiaddrRangeList {
    fn from(v: Vec<MultiaddrRange>) -> Self {
        Self(v)
    }
}

impl IntoIterator for MultiaddrRangeList {
    type IntoIter = <Vec<MultiaddrRange> as IntoIterator>::IntoIter;
    type Item = <Vec<MultiaddrRange> as IntoIterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MultiaddrRangeList {
    type IntoIter = slice::Iter<'a, MultiaddrRange>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for MultiaddrRangeList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct MultiaddrRangeListVisitor;

        impl<'de> Visitor<'de> for MultiaddrRangeListVisitor {
            type Value = MultiaddrRangeList;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a comma delimited string or multiple string elements")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: de::Error {
                let strings = v
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                let multiaddr_ranges: Result<Vec<_>, _> = strings
                    .into_iter()
                    .map(|item| MultiaddrRange::from_str(item).map_err(E::custom))
                    .collect();
                Ok(MultiaddrRangeList(multiaddr_ranges.map_err(E::custom)?))
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where D: Deserializer<'de> {
                deserializer.deserialize_seq(MultiaddrRangeListVisitor)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let mut buf = seq.size_hint().map(Vec::with_capacity).unwrap_or_default();
                while let Some(v) = seq.next_element::<MultiaddrRange>()? {
                    buf.push(v)
                }
                Ok(MultiaddrRangeList(buf))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_seq(MultiaddrRangeListVisitor)
        } else {
            deserializer.deserialize_newtype_struct("MultiaddrRangeList", MultiaddrRangeListVisitor)
        }
    }
}

impl<'de> Deserialize<'de> for MultiaddrRange {
    fn deserialize<D>(deserializer: D) -> Result<MultiaddrRange, D::Error>
    where D: Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        MultiaddrRange::from_str(&s).map_err(D::Error::custom)
    }
}

pub mod serde_multiaddr_range {
    use std::str::FromStr;

    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    use crate::net_address::MultiaddrRange;

    pub fn serialize<S>(value: &[MultiaddrRange], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        let strings: Vec<String> = value.iter().map(|v| v.to_string()).collect();
        serializer.serialize_str(&strings.join(","))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<MultiaddrRange>, D::Error>
    where D: Deserializer<'de> {
        let strings: Vec<String> = Vec::deserialize(deserializer)?;
        strings
            .into_iter()
            .map(|item| MultiaddrRange::from_str(&item).map_err(D::Error::custom))
            .collect()
    }
}

#[cfg(test)]
mod test {
    use std::{
        net::{IpAddr, Ipv6Addr},
        str::FromStr,
    };

    use serde::Deserialize;

    use crate::{
        multiaddr::Multiaddr,
        net_address::{multiaddr_range::IP4_TCP_TEST_ADDR_RANGE, MultiaddrRange, MultiaddrRangeList},
    };

    #[derive(Deserialize)]
    struct Test {
        something: MultiaddrRangeList,
    }

    #[test]
    fn it_parses_with_serde() {
        // Random tests
        let config_str = r#"something = [
            "/ip4/127.*.100:200.*/tcp/18000:19000",
            "/ip4/127.0.150.1/tcp/18500",
            "/ip4/127.0.0.1/udt/sctp/5678",
            "/ip4/127.*.*.*/tcp/*"
        ]"#;
        let item_vec = toml::from_str::<Test>(config_str).unwrap().something.into_vec();
        assert_eq!(item_vec, vec![
            MultiaddrRange::from_str("/ip4/127.*.100:200.*/tcp/18000:19000").unwrap(),
            MultiaddrRange::from_str("/ip4/127.0.150.1/tcp/18500").unwrap(),
            MultiaddrRange::from_str("/ip4/127.0.0.1/udt/sctp/5678").unwrap(),
            MultiaddrRange::from_str(IP4_TCP_TEST_ADDR_RANGE).unwrap()
        ]);

        // Allowing only '/ip4/127.0.0.1/tcp/0:18189'
        let config_str = r#"something = [
            "/ip4/127.*.*.*/tcp/0:18188",
            "/ip4/127.*.*.*/tcp/18190:65535",
            "/ip4/127.0.0.0/tcp/18189",
            "/ip4/127.1:255.1:255.2:255/tcp/18189"
        ]"#;
        let item_vec = toml::from_str::<Test>(config_str).unwrap().something.into_vec();
        assert_eq!(item_vec, vec![
            MultiaddrRange::from_str("/ip4/127.*.*.*/tcp/0:18188").unwrap(),
            MultiaddrRange::from_str("/ip4/127.*.*.*/tcp/18190:65535").unwrap(),
            MultiaddrRange::from_str("/ip4/127.0.0.0/tcp/18189").unwrap(),
            MultiaddrRange::from_str("/ip4/127.1:255.1:255.2:255/tcp/18189").unwrap(),
        ]);

        for item in item_vec {
            assert!(!item.contains(&Multiaddr::from_str("/ip4/127.0.0.1/tcp/18189").unwrap()));
        }
    }

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
