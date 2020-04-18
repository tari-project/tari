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

use crate::{envelope::Network, storage::DbConnectionUrl};
use std::time::Duration;

/// The default maximum number of messages that can be stored using the Store-and-forward middleware
pub const SAF_MSG_CACHE_STORAGE_CAPACITY: usize = 10_000;
/// The default time-to-live duration used for storage of low priority messages by the Store-and-forward middleware
pub const SAF_LOW_PRIORITY_MSG_STORAGE_TTL: Duration = Duration::from_secs(6 * 60 * 60); // 6 hours
/// The default time-to-live duration used for storage of high priority messages by the Store-and-forward middleware
pub const SAF_HIGH_PRIORITY_MSG_STORAGE_TTL: Duration = Duration::from_secs(3 * 24 * 60 * 60); // 3 days
/// The default number of peer nodes that a message has to be closer to, to be considered a neighbour
pub const DEFAULT_NUM_NEIGHBOURING_NODES: usize = 10;

#[derive(Debug, Clone)]
pub struct DhtConfig {
    /// The `DbConnectionUrl` for the Dht database. Default: In-memory database
    pub database_url: DbConnectionUrl,
    /// The size of the buffer (channel) which holds pending outbound message requests.
    /// Default: 20
    pub outbound_buffer_size: usize,
    /// The maximum number of peer nodes that a message has to be closer to, to be considered a neighbour
    /// Default: 10
    pub num_neighbouring_nodes: usize,
    /// A request to retrieve stored messages will be ignored if the requesting node is
    /// not within one of this nodes _n_ closest nodes.
    /// Default 8
    pub saf_num_closest_nodes: usize,
    /// The maximum number of messages to return from a store and forward retrieval request.
    /// Default: 100
    pub saf_max_returned_messages: usize,
    /// The maximum number of messages that can be stored using the Store-and-forward middleware. Default: 10_000
    pub saf_msg_cache_storage_capacity: usize,
    /// The time-to-live duration used for storage of low priority messages by the Store-and-forward middleware.
    /// Default: 6 hours
    pub saf_low_priority_msg_storage_ttl: Duration,
    /// The time-to-live duration used for storage of high priority messages by the Store-and-forward middleware.
    /// Default: 3 days
    pub saf_high_priority_msg_storage_ttl: Duration,
    /// The limit on the message size to store in SAF storage in bytes. Default 500 KiB
    pub saf_max_message_size: usize,
    /// The max capacity of the message hash cache
    /// Default: 10000
    pub msg_hash_cache_capacity: usize,
    /// The time-to-live for items in the message hash cache
    /// Default: 300s (5 mins)
    pub msg_hash_cache_ttl: Duration,
    /// Sets the number of failed attempts in-a-row to tolerate before temporarily excluding this peer from broadcast
    /// messages.
    /// Default: 3
    pub broadcast_cooldown_max_attempts: usize,
    /// Sets the period to wait before including this peer in broadcast messages after
    /// `broadcast_cooldown_max_attempts` failed attempts. This helps prevent thrashing the comms layer
    /// with connection attempts to a peer which is offline.
    /// Default: 30 minutes
    pub broadcast_cooldown_period: Duration,
    /// The duration to wait for a peer discovery to complete before giving up.
    /// Default: 2 minutes
    pub discovery_request_timeout: Duration,
    /// The active Network. Default: TestNet
    pub network: Network,
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
            ..Default::default()
        }
    }
}

impl Default for DhtConfig {
    fn default() -> Self {
        Self {
            num_neighbouring_nodes: DEFAULT_NUM_NEIGHBOURING_NODES,
            saf_num_closest_nodes: 10,
            saf_max_returned_messages: 50,
            outbound_buffer_size: 20,
            saf_msg_cache_storage_capacity: SAF_MSG_CACHE_STORAGE_CAPACITY,
            saf_low_priority_msg_storage_ttl: SAF_LOW_PRIORITY_MSG_STORAGE_TTL,
            saf_high_priority_msg_storage_ttl: SAF_HIGH_PRIORITY_MSG_STORAGE_TTL,
            saf_max_message_size: 512 * 1024, // 500 KiB
            msg_hash_cache_capacity: 10_000,
            msg_hash_cache_ttl: Duration::from_secs(5 * 60),
            broadcast_cooldown_max_attempts: 3,
            database_url: DbConnectionUrl::Memory,
            broadcast_cooldown_period: Duration::from_secs(60 * 30),
            discovery_request_timeout: Duration::from_secs(2 * 60),
            network: Network::TestNet,
        }
    }
}
