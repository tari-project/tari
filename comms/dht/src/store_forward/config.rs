//  Copyright 2021, The Taiji Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use taiji_common::configuration::serializers;

/// Store and forward configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafConfig {
    /// The amount of time added to the current time will be used to check if the message has expired or not
    /// Default: 3 hours
    #[serde(with = "serializers::seconds")]
    pub msg_validity: Duration,
    /// The maximum number of messages that can be stored using the Store-and-forward middleware.
    /// Default: 100,000
    pub msg_storage_capacity: usize,
    /// A request to retrieve stored messages will be ignored if the requesting node is
    /// not within one of this nodes _n_ closest nodes.
    /// Default 8
    pub num_closest_nodes: usize,
    /// The maximum number of messages to return from a store and forward retrieval request.
    /// Default: 100
    pub max_returned_messages: usize,
    /// The time-to-live duration used for storage of low priority messages by the Store-and-forward middleware.
    /// Default: 6 hours
    #[serde(with = "serializers::seconds")]
    pub low_priority_msg_storage_ttl: Duration,
    /// The time-to-live duration used for storage of high priority messages by the Store-and-forward middleware.
    /// Default: 3 days
    #[serde(with = "serializers::seconds")]
    pub high_priority_msg_storage_ttl: Duration,
    /// The limit on the message size to store in SAF storage in bytes. Default 500 KiB
    pub max_message_size: usize,
    /// When true, store and forward messages are requested from peers on connect (Default: true)
    pub auto_request: bool,
    /// The maximum allowed time between asking for a message and accepting a response
    #[serde(with = "serializers::seconds")]
    pub max_inflight_request_age: Duration,
    /// The maximum number of peer nodes that a message must be closer than to get stored by SAF
    /// Default: 8
    pub num_neighbouring_nodes: usize,
}

impl Default for SafConfig {
    fn default() -> Self {
        Self {
            msg_validity: Duration::from_secs(3 * 60 * 60), // 3 hours
            num_closest_nodes: 10,
            max_returned_messages: 50,
            msg_storage_capacity: 100_000,
            low_priority_msg_storage_ttl: Duration::from_secs(6 * 60 * 60), // 6 hours
            high_priority_msg_storage_ttl: Duration::from_secs(3 * 24 * 60 * 60), // 3 days
            auto_request: true,
            max_message_size: 512 * 1024,
            max_inflight_request_age: Duration::from_secs(120),
            num_neighbouring_nodes: 8,
        }
    }
}
