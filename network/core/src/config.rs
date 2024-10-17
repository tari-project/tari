//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt::{Display, Formatter},
    net::Ipv4Addr,
    str::FromStr,
    time::Duration,
};

use libp2p::{multiaddr::multiaddr, Multiaddr};

#[derive(Debug, Clone)]
pub struct Config {
    pub swarm: tari_swarm::Config,
    pub listener_addrs: Vec<Multiaddr>,
    pub reachability_mode: ReachabilityMode,
    pub check_connections_interval: Duration,
    pub known_local_public_address: Vec<Multiaddr>,
}

impl Config {
    pub fn default_listen_addrs() -> Vec<Multiaddr> {
        vec![multiaddr![Ip4(Ipv4Addr::new(0, 0, 0, 0)), Tcp(0u16)], multiaddr![
            Ip4(Ipv4Addr::new(0, 0, 0, 0)),
            Udp(0u16),
            QuicV1
        ]]
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            swarm: tari_swarm::Config::default(),
            // Listen on /ip4/0.0.0.0 for TCP (os-assigned port) and UDP quic
            listener_addrs: Self::default_listen_addrs(),
            reachability_mode: ReachabilityMode::default(),
            check_connections_interval: Duration::from_secs(2 * 60 * 60),
            known_local_public_address: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum ReachabilityMode {
    #[default]
    Auto,
    Private,
}

impl ReachabilityMode {
    pub fn is_private(&self) -> bool {
        matches!(self, ReachabilityMode::Private)
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, ReachabilityMode::Auto)
    }
}

impl Display for ReachabilityMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReachabilityMode::Auto => write!(f, "auto"),
            ReachabilityMode::Private => write!(f, "private"),
        }
    }
}

impl FromStr for ReachabilityMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            return Ok(ReachabilityMode::Auto);
        }
        if s.eq_ignore_ascii_case("private") {
            return Ok(ReachabilityMode::Private);
        }

        Err(anyhow::Error::msg(format!("Invalid reachability mode '{s}'")))
    }
}
