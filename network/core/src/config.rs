//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::time::Duration;

use libp2p::Multiaddr;

#[derive(Debug, Clone)]
pub struct Config {
    pub swarm: tari_swarm::Config,
    pub listener_port: u16,
    pub reachability_mode: ReachabilityMode,
    pub announce: bool,
    pub check_connections_interval: Duration,
    pub known_local_public_address: Vec<Multiaddr>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            swarm: tari_swarm::Config::default(),
            listener_port: 0,
            reachability_mode: ReachabilityMode::default(),
            announce: false,
            check_connections_interval: Duration::from_secs(2 * 60 * 60),
            known_local_public_address: vec![],
        }
    }
}

#[derive(Debug, Clone, Default)]
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
