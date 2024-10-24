//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{num::NonZeroU32, time::Duration};

use libp2p::ping;

use crate::protocol_version::ProtocolVersion;

#[derive(Debug, Clone)]
pub struct Config {
    pub protocol_version: ProtocolVersion,
    pub user_agent: String,
    pub messaging_protocol: String,
    pub ping: ping::Config,
    pub max_connections_per_peer: Option<u32>,
    pub enable_mdns: bool,
    pub enable_relay: bool,
    pub enable_messaging: bool,
    pub idle_connection_timeout: Duration,
    pub relay_circuit_limits: RelayCircuitLimits,
    pub relay_reservation_limits: RelayReservationLimits,
    pub identify_interval: Duration,
    pub gossipsub_max_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            protocol_version: "/tari/localnet/0.0.1".parse().unwrap(),
            user_agent: "/tari/unknown/0.0.1".to_string(),
            messaging_protocol: "/tari/messaging/0.0.1".to_string(),
            ping: ping::Config::default(),
            max_connections_per_peer: Some(3),
            enable_mdns: false,
            enable_relay: false,
            enable_messaging: true,
            idle_connection_timeout: Duration::from_secs(10 * 60),
            relay_circuit_limits: RelayCircuitLimits::default(),
            relay_reservation_limits: RelayReservationLimits::default(),
            // This is the default for identify
            identify_interval: Duration::from_secs(5 * 60),
            // Double the libp2p default
            gossipsub_max_message_size: 128 * 1024,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RelayCircuitLimits {
    pub max_limit: usize,
    pub max_per_peer: usize,
    pub max_duration: Duration,
    pub per_peer: Option<LimitPerInterval>,
    pub per_ip: Option<LimitPerInterval>,
    pub max_byte_limit: u64,
}

impl RelayCircuitLimits {
    pub fn high() -> Self {
        Self {
            max_limit: 64,
            max_per_peer: 8,
            max_duration: Duration::from_secs(4 * 60),
            per_peer: Some(LimitPerInterval {
                limit: NonZeroU32::new(60).expect("30 > 0"),
                interval: Duration::from_secs(2 * 60),
            }),
            per_ip: Some(LimitPerInterval {
                limit: NonZeroU32::new(120).expect("60 > 0"),
                interval: Duration::from_secs(60),
            }),
            max_byte_limit: 1 << 19, // 512KB
        }
    }
}

impl Default for RelayCircuitLimits {
    fn default() -> Self {
        // These reflect the default circuit limits in libp2p relay
        Self {
            max_limit: 16,
            max_per_peer: 4,
            max_duration: Duration::from_secs(2 * 60),
            per_peer: Some(LimitPerInterval {
                limit: NonZeroU32::new(30).expect("30 > 0"),
                interval: Duration::from_secs(2 * 60),
            }),
            per_ip: Some(LimitPerInterval {
                limit: NonZeroU32::new(60).expect("60 > 0"),
                interval: Duration::from_secs(60),
            }),
            max_byte_limit: 1 << 17, // 128KB
        }
    }
}

#[derive(Debug, Clone)]
pub struct RelayReservationLimits {
    pub max_limit: usize,
    pub max_per_peer: usize,
    pub max_duration: Duration,
    pub per_peer: Option<LimitPerInterval>,
    pub per_ip: Option<LimitPerInterval>,
}

impl RelayReservationLimits {
    pub fn high() -> Self {
        Self {
            max_limit: 128,
            max_per_peer: 8,
            max_duration: Duration::from_secs(4 * 60),
            per_peer: Some(LimitPerInterval {
                limit: NonZeroU32::new(60).expect("30 > 0"),
                interval: Duration::from_secs(2 * 60),
            }),
            per_ip: Some(LimitPerInterval {
                limit: NonZeroU32::new(120).expect("60 > 0"),
                interval: Duration::from_secs(60),
            }),
        }
    }
}

impl Default for RelayReservationLimits {
    fn default() -> Self {
        // These reflect the default reservation limits in libp2p relay
        Self {
            max_limit: 128,
            max_per_peer: 4,
            max_duration: Duration::from_secs(60 * 60),
            per_peer: Some(LimitPerInterval {
                limit: NonZeroU32::new(30).expect("30 > 0"),
                interval: Duration::from_secs(2 * 60),
            }),
            per_ip: Some(LimitPerInterval {
                limit: NonZeroU32::new(60).expect("60 > 0"),
                interval: Duration::from_secs(60),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LimitPerInterval {
    pub limit: NonZeroU32,
    pub interval: Duration,
}
