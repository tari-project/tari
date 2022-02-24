//  Copyright 2020, The Tari Project
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

use tari_comms::peer_manager::NodeId;

#[derive(Debug, Clone)]
pub struct BlockchainSyncConfig {
    /// The initial max sync latency. If a peer fails to stream a header/block within this deadline another sync peer
    /// will be selected. If there are no further peers the sync will be restarted with an increased by
    /// `max_latency_increase`.
    pub initial_max_sync_latency: Duration,
    /// If all sync peers exceed latency, increase allowed latency by this value
    pub max_latency_increase: Duration,
    /// Longer ban period for potentially malicious infractions (protocol violations etc.)
    pub ban_period: Duration,
    /// Short ban period for infractions that are likely not malicious (slow to respond, spotty connections etc)
    pub short_ban_period: Duration,
    /// An allowlist of sync peers from which to sync. No other peers will be selected for sync. If empty, sync peers
    /// are chosen based on their advertised chain metadata.
    pub forced_sync_peers: Vec<NodeId>,
    /// Number of threads to use for validation
    pub validation_concurrency: usize,
}

impl Default for BlockchainSyncConfig {
    fn default() -> Self {
        Self {
            initial_max_sync_latency: Duration::from_secs(3),
            max_latency_increase: Duration::from_secs(2),
            ban_period: Duration::from_secs(30 * 60),
            short_ban_period: Duration::from_secs(60),
            forced_sync_peers: Default::default(),
            validation_concurrency: 6,
        }
    }
}
