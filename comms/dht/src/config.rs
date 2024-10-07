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

use std::{path::Path, str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};
use tari_common::configuration::serializers;
use tari_comms::{
    net_address::{MultiaddrRange, MultiaddrRangeList, IP4_TCP_TEST_ADDR_RANGE},
    peer_validator::PeerValidatorConfig,
};

use crate::{
    actor::OffenceSeverity,
    network_discovery::NetworkDiscoveryConfig,
    storage::DbConnectionUrl,
    store_forward::SafConfig,
    version::DhtProtocolVersion,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DhtConfig {
    /// The major protocol version to use. Default: DhtProtocolVersion::latest()
    pub protocol_version: DhtProtocolVersion,
    /// The `DbConnectionUrl` for the Dht database. Default: In-memory database
    pub database_url: DbConnectionUrl,
    /// The size of the buffer (channel) which holds pending outbound message requests.
    /// Default: 20
    pub outbound_buffer_size: usize,
    /// The maximum number of peer nodes that a message has to be closer to, to be considered a neighbour
    /// Default: 8
    pub num_neighbouring_nodes: usize,
    /// Number of random peers to include
    /// Default: 4
    pub num_random_nodes: usize,
    /// Connections above the configured number of neighbouring and random nodes will be removed
    /// (default: false)
    pub minimize_connections: bool,
    /// Send to this many peers when using the broadcast strategy
    /// Default: 8
    pub broadcast_factor: usize,
    /// Send to this many peers when using the propagate strategy
    /// Default: 4
    pub propagation_factor: usize,
    pub saf: SafConfig,
    /// The max capacity of the message hash cache
    /// Default: 2,500
    pub dedup_cache_capacity: usize,
    /// The periodic trim interval for items in the message hash cache
    /// Default: 300s (5 mins)
    #[serde(with = "serializers::seconds")]
    pub dedup_cache_trim_interval: Duration,
    /// The number of occurrences of a message is allowed to pass through the DHT pipeline before being
    /// deduped/discarded
    /// Default: 1
    pub dedup_allowed_message_occurrences: usize,
    /// The duration to wait for a peer discovery to complete before giving up.
    /// Default: 2 minutes
    #[serde(with = "serializers::seconds")]
    pub discovery_request_timeout: Duration,
    /// Set to true to automatically broadcast a join message when ready, otherwise false. Default: false
    pub auto_join: bool,
    /// The minimum time between sending a Join message to the network. Joins are only sent when the node establishes
    /// enough connections to the network as determined by comms ConnectivityManager. If a join was sent and then state
    /// change happens again after this period, another join will be sent.
    /// Default: 10 minutes
    #[serde(with = "serializers::seconds")]
    pub join_cooldown_interval: Duration,
    pub connectivity: DhtConnectivityConfig,
    /// Network discovery config
    pub network_discovery: NetworkDiscoveryConfig,
    /// Length of time to ban a peer if the peer misbehaves at the DHT-level.
    /// Default: 2 hrs
    #[serde(with = "serializers::seconds")]
    pub ban_duration: Duration,
    /// Length of time to ban a peer for a "short" duration.
    /// Default: 10 mins
    #[serde(with = "serializers::seconds")]
    pub ban_duration_short: Duration,
    /// The maximum number of messages over `flood_ban_timespan` to allow before banning the peer (for
    /// `ban_duration_short`) Default: 100_000 messages
    pub flood_ban_max_msg_count: usize,
    /// The timespan over which to calculate the max message rate.
    /// `flood_ban_max_count / flood_ban_timespan (as seconds) = avg. messages per second over the timespan`
    /// Default: 100 seconds
    #[serde(with = "serializers::seconds")]
    pub flood_ban_timespan: Duration,
    /// Once a peer has been marked as offline, wait at least this length of time before reconsidering them.
    /// In a situation where a node is not well-connected and many nodes are locally marked as offline, we can retry
    /// peers that were previously tried.
    /// Default: 24 hours
    #[serde(with = "serializers::seconds")]
    pub offline_peer_cooldown: Duration,
    /// The maximum number of peer claims accepted by this node. Only peer sync sends more than one claim.
    /// Default: 5
    pub max_permitted_peer_claims: usize,
    /// Configuration for peer validation
    /// See [PeerValidatorConfig]
    pub peer_validator_config: PeerValidatorConfig,
    /// Addresses that should never be dialed (default value = []). This can be a specific address or an IPv4/TCP
    /// range. Example: When used in conjunction with `allow_test_addresses = true` (but it could be any other
    /// range)   `excluded_dial_addresses = ["/ip4/127.*.0:49.*/tcp/*", "/ip4/127.*.101:255.*/tcp/*"]`
    ///                or
    ///   `excluded_dial_addresses = ["/ip4/127.0:0.1/tcp/122", "/ip4/127.0:0.1/tcp/1000:2000"]`
    pub excluded_dial_addresses: MultiaddrRangeList,
}

impl DhtConfig {
    /// Default testnet configuration
    pub fn default_testnet() -> Self {
        Default::default()
    }

    /// Default mainnet configuration
    pub fn default_mainnet() -> Self {
        Default::default()
    }

    /// Default local test configuration
    pub fn default_local_test() -> Self {
        Self {
            database_url: DbConnectionUrl::Memory,
            saf: SafConfig {
                auto_request: false,
                ..Default::default()
            },
            auto_join: false,
            network_discovery: NetworkDiscoveryConfig {
                // If a test requires the peer probe they should explicitly enable it
                enabled: false,
                initial_peer_sync_delay: None,
                ..Default::default()
            },
            peer_validator_config: PeerValidatorConfig {
                allow_test_addresses: true,
                ..Default::default()
            },
            excluded_dial_addresses: vec![].into(),
            ..Default::default()
        }
    }

    /// Sets relative paths to use a common base path
    pub fn set_base_path<P: AsRef<Path>>(&mut self, base_path: P) {
        self.database_url.set_base_path(base_path);
    }

    /// Returns a ban duration from the given severity
    pub fn ban_duration_from_severity(&self, severity: OffenceSeverity) -> Duration {
        match severity {
            OffenceSeverity::Low | OffenceSeverity::Medium => self.ban_duration_short,
            OffenceSeverity::High => self.ban_duration,
        }
    }
}

impl Default for DhtConfig {
    fn default() -> Self {
        // NB: please remember to update field comments to reflect these defaults
        Self {
            protocol_version: DhtProtocolVersion::latest(),
            num_neighbouring_nodes: 8,
            num_random_nodes: 4,
            minimize_connections: false,
            propagation_factor: 20,
            broadcast_factor: 8,
            outbound_buffer_size: 20,
            saf: Default::default(),
            dedup_cache_capacity: 2_500,
            dedup_cache_trim_interval: Duration::from_secs(5 * 60),
            dedup_allowed_message_occurrences: 1,
            database_url: DbConnectionUrl::Memory,
            discovery_request_timeout: Duration::from_secs(2 * 60),
            connectivity: DhtConnectivityConfig::default(),
            auto_join: false,
            join_cooldown_interval: Duration::from_secs(10 * 60),
            network_discovery: Default::default(),
            ban_duration: Duration::from_secs(2 * 60 * 60),
            ban_duration_short: Duration::from_secs(10 * 60),
            flood_ban_max_msg_count: 100_000,
            flood_ban_timespan: Duration::from_secs(100),
            max_permitted_peer_claims: 5,
            offline_peer_cooldown: Duration::from_secs(24 * 60 * 60),
            peer_validator_config: Default::default(),
            excluded_dial_addresses: vec![]
            .into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DhtConnectivityConfig {
    /// The interval to update the neighbouring and random pools, if necessary.
    /// Default: 2 minutes
    #[serde(with = "serializers::seconds")]
    pub update_interval: Duration,
    /// The interval to change the random pool peers.
    /// Default: 2 hours
    #[serde(with = "serializers::seconds")]
    pub random_pool_refresh_interval: Duration,
    /// Length of cooldown when high connection failure rates are encountered. Default: 45s
    #[serde(with = "serializers::seconds")]
    pub high_failure_rate_cooldown: Duration,
    /// The minimum desired ratio of TCPv4 to Tor connections. TCPv4 addresses have some significant cost to create,
    /// making sybil attacks costly. This setting does not guarantee this ratio is maintained.
    /// Currently, it only emits a warning if the ratio is below this setting.
    /// Default: 0.1 (10%)
    pub minimum_desired_tcpv4_node_ratio: f32,
}

impl Default for DhtConnectivityConfig {
    fn default() -> Self {
        Self {
            update_interval: Duration::from_secs(2 * 60),
            random_pool_refresh_interval: Duration::from_secs(2 * 60 * 60),
            high_failure_rate_cooldown: Duration::from_secs(45),
            minimum_desired_tcpv4_node_ratio: 0.1,
        }
    }
}
