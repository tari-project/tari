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

use crate::{
    network_discovery::NetworkDiscoveryConfig,
    storage::DbConnectionUrl,
    store_forward::SafConfig,
    version::DhtProtocolVersion,
};
use std::time::Duration;

#[derive(Debug, Clone)]
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
    /// Send to this many peers when using the broadcast strategy
    /// Default: 8
    pub broadcast_factor: usize,
    /// Send to this many peers when using the propagate strategy
    /// Default: 4
    pub propagation_factor: usize,
    pub saf_config: SafConfig,
    /// The max capacity of the message hash cache
    /// Default: 2,500
    pub dedup_cache_capacity: usize,
    /// The periodic trim interval for items in the message hash cache
    /// Default: 300s (5 mins)
    pub dedup_cache_trim_interval: Duration,
    /// The number of occurrences of a message is allowed to pass through the DHT pipeline before being
    /// deduped/discarded
    /// Default: 1
    pub dedup_allowed_message_occurrences: usize,
    /// The duration to wait for a peer discovery to complete before giving up.
    /// Default: 2 minutes
    pub discovery_request_timeout: Duration,
    /// Set to true to automatically broadcast a join message when ready, otherwise false. Default: false
    pub auto_join: bool,
    /// The minimum time between sending a Join message to the network. Joins are only sent when the node establishes
    /// enough connections to the network as determined by comms ConnectivityManager. If a join was sent and then state
    /// change happens again after this period, another join will be sent.
    /// Default: 10 minutes
    pub join_cooldown_interval: Duration,
    /// The interval to update the neighbouring and random pools, if necessary.
    /// Default: 2 minutes
    pub connectivity_update_interval: Duration,
    /// The interval to change the random pool peers.
    /// Default: 2 hours
    pub connectivity_random_pool_refresh: Duration,
    /// Network discovery config
    pub network_discovery: NetworkDiscoveryConfig,
    /// Length of time to ban a peer if the peer misbehaves at the DHT-level.
    /// Default: 6 hrs
    pub ban_duration: Duration,
    /// This allows the use of test addresses in the network.
    /// Default: false
    pub allow_test_addresses: bool,
    /// The maximum number of messages over `flood_ban_timespan` to allow before banning the peer (for `ban_duration`)
    /// Default: 1000 messages
    pub flood_ban_max_msg_count: usize,
    /// The timespan over which to calculate the max message rate.
    /// `flood_ban_max_count / flood_ban_timespan (as seconds) = avg. messages per second over the timespan`
    /// Default: 100 seconds
    pub flood_ban_timespan: Duration,
    /// Once a peer has been marked as offline, wait at least this length of time before reconsidering them.
    /// In a situation where a node is not well-connected and many nodes are locally marked as offline, we can retry
    /// peers that were previously tried.
    /// Default: 2 hours
    pub offline_peer_cooldown: Duration,
}

impl DhtConfig {
    pub fn default_testnet() -> Self {
        Default::default()
    }

    pub fn default_mainnet() -> Self {
        Default::default()
    }

    pub fn default_local_test() -> Self {
        Self {
            database_url: DbConnectionUrl::Memory,
            saf_config: SafConfig {
                auto_request: false,
                ..Default::default()
            },
            auto_join: false,
            network_discovery: NetworkDiscoveryConfig {
                // If a test requires the peer probe they should explicitly enable it
                enabled: false,
                ..Default::default()
            },
            allow_test_addresses: true,
            ..Default::default()
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
            propagation_factor: 4,
            broadcast_factor: 8,
            outbound_buffer_size: 20,
            saf_config: Default::default(),
            dedup_cache_capacity: 2_500,
            dedup_cache_trim_interval: Duration::from_secs(5 * 60),
            dedup_allowed_message_occurrences: 1,
            database_url: DbConnectionUrl::Memory,
            discovery_request_timeout: Duration::from_secs(2 * 60),
            connectivity_update_interval: Duration::from_secs(2 * 60),
            connectivity_random_pool_refresh: Duration::from_secs(2 * 60 * 60),
            auto_join: false,
            join_cooldown_interval: Duration::from_secs(10 * 60),
            network_discovery: Default::default(),
            ban_duration: Duration::from_secs(6 * 60 * 60),
            allow_test_addresses: false,
            flood_ban_max_msg_count: 10000,
            flood_ban_timespan: Duration::from_secs(100),
            offline_peer_cooldown: Duration::from_secs(2 * 60 * 60),
        }
    }
}
