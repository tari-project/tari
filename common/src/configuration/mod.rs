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
//! [base_node.weatherwax]
//! # overrides for rincewnd testnet
//! [base_node.mainnet]
//! # overrides for mainnet
//! [wallet]
//! [wallet.weatherwax]
//! # etc..
//! ```

pub mod bootstrap;
pub mod error;
pub mod loader;
mod network;
mod tor_control_authentication;
pub use network::Network;
mod collectibles_config;
mod common_config;
mod comms_transport;
pub mod name_server;
pub mod serializers;
mod socks_authentication;
pub mod utils;

use std::{iter::FromIterator, net::SocketAddr};

pub use collectibles_config::CollectiblesConfig;
pub use common_config::CommonConfig;
pub use comms_transport::{CommsTransport, CommsTransportType, Socks5Config, TcpTransportConfig, TorConfig};
use multiaddr::{Error, Multiaddr, Protocol};
pub use socks_authentication::SocksAuthentication;
pub use tor_control_authentication::TorControlAuthentication;

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
