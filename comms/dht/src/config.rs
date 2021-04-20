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

use crate::{envelope::Network, network_discovery::NetworkDiscoveryConfig, storage::DbConnectionUrl};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DhtConfig {
    /// The `DbConnectionUrl` for the Dht database. Default: In-memory database
    pub database_url: DbConnectionUrl,
    /// The size of the buffer (channel) which holds pending outbound message requests.
    /// Default: 20
    pub outbound_buffer_size: usize,
    /// The maximum number of peer nodes that a message has to be closer to, to be considered a neighbour
    pub num_nodes_in_home_bucket: usize,
    /// Number of peers to include in each non-home bucket
    pub num_nodes_in_other_buckets: usize,
    /// Send to this many peers when using the broadcast strategy
    /// Default: 8
    pub broadcast_factor: usize,
    /// Send to this many peers when using the propagate strategy
    /// Default: 4
    pub propagation_factor: usize,
    /// The amount of seconds added to the current time (Utc) which will then be used to check if the message has
    /// expired or not when processing the message
    /// Default: 10800
    pub saf_msg_validity: Duration,
    /// The maximum number of messages that can be stored using the Store-and-forward middleware.
    /// Default: 100,000
    pub saf_msg_storage_capacity: usize,
    /// A request to retrieve stored messages will be ignored if the requesting node is
    /// not within one of this nodes _n_ closest nodes.
    /// Default 8
    pub saf_num_closest_nodes: usize,
    /// The maximum number of messages to return from a store and forward retrieval request.
    /// Default: 100
    pub saf_max_returned_messages: usize,
    /// The time-to-live duration used for storage of low priority messages by the Store-and-forward middleware.
    /// Default: 6 hours
    pub saf_low_priority_msg_storage_ttl: Duration,
    /// The time-to-live duration used for storage of high priority messages by the Store-and-forward middleware.
    /// Default: 3 days
    pub saf_high_priority_msg_storage_ttl: Duration,
    /// The limit on the message size to store in SAF storage in bytes. Default 500 KiB
    pub saf_max_message_size: usize,
    /// When true, store and forward messages are requested from peers on connect (Default: true)
    pub saf_auto_request: bool,
    /// The minimum period used to request SAF messages from a peer. When requesting SAF messages,
    /// it will request messages since the DHT last went offline, but this may be a small amount of
    /// time, so `minimum_request_period` can be used so that messages aren't missed.
    /// Default: 3 days
    pub saf_minimum_request_period: Duration,
    /// The max capacity of the message hash cache
    /// Default: 100,000
    pub msg_hash_cache_capacity: usize,
    /// The time-to-live for items in the message hash cache
    /// Default: 300s (5 mins)
    pub msg_hash_cache_ttl: Duration,
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
    /// The active Network. Default: TestNet
    pub network: Network,
    /// Network discovery config
    pub network_discovery: NetworkDiscoveryConfig,
    /// Length of time to ban a peer if the peer misbehaves at the DHT-level.
    /// Default: 6 hrs
    pub ban_duration: Duration,
    /// This allows the use of test addresses in the network.
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
    /// Default: 24 hours
    pub offline_peer_cooldown: Duration,

    pub num_network_buckets: u32,
    pub connectivity_peer_buckets_refresh: Duration,
}

impl DhtConfig {
    pub fn default_testnet() -> Self {
        Default::default()
    }

    pub fn default_mainnet() -> Self {
        Self {
            network: Network::MainNet,
            ..Default::default()
        }
    }

    pub fn default_local_test() -> Self {
        Self {
            network: Network::LocalTest,
            database_url: DbConnectionUrl::Memory,
            saf_auto_request: false,
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
        Self {
            num_nodes_in_home_bucket: 8,
            num_nodes_in_other_buckets: 4,
            propagation_factor: 4,
            broadcast_factor: 8,
            outbound_buffer_size: 20,
            saf_num_closest_nodes: 10,
            saf_max_returned_messages: 50,
            saf_msg_storage_capacity: 100_000,
            saf_low_priority_msg_storage_ttl: Duration::from_secs(6 * 60 * 60), // 6 hours
            saf_high_priority_msg_storage_ttl: Duration::from_secs(3 * 24 * 60 * 60), // 3 days
            saf_auto_request: true,
            saf_max_message_size: 512 * 1024,
            saf_minimum_request_period: Duration::from_secs(3 * 24 * 60 * 60), // 3 days
            msg_hash_cache_capacity: 1,
            msg_hash_cache_ttl: Duration::from_secs(5 * 60),
            database_url: DbConnectionUrl::Memory,
            discovery_request_timeout: Duration::from_secs(2 * 60),
            connectivity_update_interval: Duration::from_secs(2 * 60),
            connectivity_random_pool_refresh: Duration::from_secs(2 * 60 * 60),
            auto_join: false,
            join_cooldown_interval: Duration::from_secs(10 * 60),
            network: Network::TestNet,
            network_discovery: Default::default(),
            ban_duration: Duration::from_secs(6 * 60 * 60),
            allow_test_addresses: false,
            flood_ban_max_msg_count: 10000,
            flood_ban_timespan: Duration::from_secs(100),
            offline_peer_cooldown: Duration::from_secs(20),
            saf_msg_validity: Duration::from_secs(10800),
            num_network_buckets: 3,
            connectivity_peer_buckets_refresh: Duration::from_secs(3 * 60),
        }
    }
}
