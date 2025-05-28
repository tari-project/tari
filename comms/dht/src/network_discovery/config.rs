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

use serde::{Deserialize, Serialize};
use tari_common::configuration::serializers;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkDiscoveryConfig {
    /// True to enable network discovery, false to disable it.
    /// Default: true
    pub enabled: bool,
    /// A threshold for the minimum number of peers this node should ideally be aware of. If below this threshold a
    /// more "aggressive" strategy is employed.
    /// Default: 50
    pub min_desired_peers: usize,
    /// The period to wait once the number of rounds given by `idle_after_num_rounds` has completed.
    /// Default: 30 mins
    #[serde(with = "serializers::seconds")]
    pub idle_period: Duration,
    /// The minimum number of network discovery rounds to perform before idling (going to sleep). If there are less
    /// than `min_desired_peers` then the actual number of rounds performed will exceed this value. Default: 10
    pub idle_after_num_rounds: usize,
    /// Time to idle after a failed round.
    /// Default: 5 secs
    #[serde(with = "serializers::seconds")]
    pub on_failure_idle_period: Duration,
    /// The maximum number of sync peer to select for each round. The selection strategy varies depending on the
    /// current state.
    /// Default: 5
    pub max_sync_peers: usize,
    /// The maximum number of peers we allow per round of sync.
    /// Default: 500
    pub max_peers_to_sync_per_round: u32,
    /// Maximum number of seed peers to try during bootstrap phase
    /// Default: 5
    #[serde(default)]
    pub max_seed_peer_sync_count: usize,
    /// Initial refresh sync peers delay period, when a configured connection needs preference.
    /// Default: None
    #[serde(default)]
    #[serde(with = "serializers::optional_seconds")]
    pub initial_peer_sync_delay: Option<Duration>,

    /// The minimum number of peers to attempt to sync with during each seed peer sync operation.
    /// If this many peers are successfully added to the peer DB (across all seed peers attempted
    /// in one round), the current seed_strap round will end early, provided that
    /// `min_successful_seed_contacts_for_early_exit` is also met.
    /// Set to 0 to disable this early exit condition (it will always try up to `max_seed_peer_sync_count`
    /// seed peers unless an error occurs or `max_peers_to_sync_per_round` is hit repeatedly).
    /// Default: 15
    #[serde(default)]
    pub seed_peer_min_initial_sync_peers_needed: usize,

    /// The minimum number of seed peers that must be successfully contacted (i.e., returned at least one peer)
    /// before an early exit due to `seed_peer_min_initial_sync_peers_needed` can occur.
    /// Default: 5
    #[serde(default)]
    pub min_successful_seed_contacts_for_early_exit: usize,

    /// Maximum time to wait for bootstrap to complete before forcing completion
    /// Default: 5 minutes
    #[serde(default)]
    #[serde(with = "serializers::seconds")]
    pub bootstrap_timeout: Duration,
}

impl Default for NetworkDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_desired_peers: 50,
            idle_period: Duration::from_secs(30 * 60),
            idle_after_num_rounds: 10,
            on_failure_idle_period: Duration::from_secs(5),
            max_sync_peers: 5,
            max_peers_to_sync_per_round: 500,
            max_seed_peer_sync_count: 5,
            initial_peer_sync_delay: None,
            seed_peer_min_initial_sync_peers_needed: 15,
            min_successful_seed_contacts_for_early_exit: 5,
            bootstrap_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}
