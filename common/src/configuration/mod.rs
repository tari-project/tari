// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Configuration of tari applications
//!
//! Tari application consist of `common`, `base_node`, `wallet` and `application` configuration sections.
//! All tari apps follow traits implemented in this crate for ease and automation, for instance managing config files,
//! defaults configuration, overloading settings from subsections.
//!
//! ## Submodules
//!
//! - [bootstrap] - build CLI and manage/load configuration with [ConfigBootsrap] struct
//! - [global] - load GlobalConfig for Tari
//! - [loader] - build and load configuration modules in a tari-way
//! - [utils] - utilities for working with configuration
//!
//! ## Configuration file
//!
//! The tari configuration file (config.yml) is intended to be a single config file for all Tari desktop apps to use
//! to pull configuration variables, whether it's a testnet base node; wallet; validator node etc.
//!
//! The file lives in ~/.tari by default and has sections which will allow a specific app to determine
//! the config values it needs, e.g.
//!
//! ```toml
//! [common]
//! # Globally common variables
//! ...
//! [base_node]
//! # common vars for all base_node instances
//! [base_node.esmeralda]
//! # overrides for rincewnd testnet
//! [base_node.mainnet]
//! # overrides for mainnet
//! [wallet]
//! [wallet.esmeralda]
//! # etc..
//! ```

pub mod bootstrap;
pub mod error;
pub mod loader;
mod network;
pub use network::Network;
mod common_config;
mod config_list;
mod dns_name_server_list;
mod multiaddr_list;
pub mod name_server;
pub mod serializers;
mod string_list;
pub mod utils;

use std::{iter::FromIterator, net::SocketAddr};

pub use common_config::CommonConfig;
pub use config_list::ConfigList;
pub use dns_name_server_list::{deserialize_dns_name_server_list, DnsNameServerList};
use multiaddr::{Error, Multiaddr, Protocol};
pub use multiaddr_list::MultiaddrList;
pub use string_list::StringList;

/// Interpret a string as either a socket address (first) or a multiaddr format string.
/// If the former, it gets converted into a MultiAddr before being returned.
pub fn socket_or_multi(addr: &str) -> Result<Multiaddr, Error> {
    addr.parse::<SocketAddr>()
        .map(|socket| match socket {
            SocketAddr::V4(ip4) => Multiaddr::from_iter([Protocol::Ip4(*ip4.ip()), Protocol::Tcp(ip4.port())]),
            SocketAddr::V6(ip6) => Multiaddr::from_iter([Protocol::Ip6(*ip6.ip()), Protocol::Tcp(ip6.port())]),
        })
        .or_else(|_| addr.parse::<Multiaddr>())
}

/// Implement this trait to specify custom configuration overrides for a network when loading the config
pub trait ConfigOverrideProvider {
    fn get_config_property_overrides(&self, network: &Network) -> Vec<(String, String)>;
}

pub struct NoConfigOverrides;

impl ConfigOverrideProvider for NoConfigOverrides {
    fn get_config_property_overrides(&self, _network: &Network) -> Vec<(String, String)> {
        Vec::new()
    }
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn socket_or_multi_test() {
        let v4_addr = "127.0.0.1:8080";
        let multi_v4_addr = socket_or_multi(v4_addr).unwrap();
        // ipv4 testing
        assert_eq!(
            multi_v4_addr,
            Multiaddr::from_iter([Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)), Protocol::Tcp(8080)])
        );

        let v6_addr = "[2001:db8::1]:8080";
        let multi_v6_addr = socket_or_multi(v6_addr).unwrap();
        // ipv6 testing
        assert_eq!(
            multi_v6_addr,
            Multiaddr::from_iter([
                Protocol::Ip6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
                Protocol::Tcp(8080)
            ])
        );
    }
}
