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

use super::connection_stats::PeerConnectionStats;

/// Connectivity actor configuration
#[derive(Debug, Clone, Copy)]
pub struct ConnectivityConfig {
    /// The minimum number of connected nodes before connectivity is transitioned to ONLINE
    /// Default: 1
    pub min_connectivity: usize,
    /// Interval to check the connection pool, including reaping inactive connections and retrying failed managed peer
    /// connections. Default: 60s
    pub connection_pool_refresh_interval: Duration,
    /// True if connection reaping is enabled, otherwise false (default: true)
    pub is_connection_reaping_enabled: bool,
    /// The minimum number of connections that must exist before any connections may be reaped
    /// Default: 50
    pub reaper_min_connection_threshold: usize,
    /// The minimum age of the connection before it can be reaped. This prevents a connection that has just been
    /// established from being reaped due to inactivity. Default: 20 minutes
    pub reaper_min_inactive_age: Duration,
    /// The number of connection failures before a peer is considered offline
    /// Default: 1
    pub max_failures_mark_offline: usize,
    /// The length of time to wait before disconnecting a connection that failed tie breaking.
    /// Default: 1s
    pub connection_tie_break_linger: Duration,
    /// If the peer has not been seen within this interval, it will be removed from the peer list on the
    /// next connection attempt.
    /// Default: 24 hours
    pub expire_peer_last_seen_duration: Duration,
    /// The closest number of peer connections to maintain; connections above the threshold will be removed
    /// (default: disabled)
    pub maintain_n_closest_connections_only: Option<usize>,
    /// The connection count below which the proactive dialer is allowed to run. This is a *floor*, not a target:
    /// steady-state connection count is owned by the DHT peer pool, and the proactive dialer exists only to get a
    /// node back off the floor when the pool cannot recover unaided (cold start, total isolation, seed re-dial).
    ///
    /// It must therefore stay strictly below the DHT pool size (`num_neighbouring_nodes + num_random_nodes`), or
    /// the dialer becomes a second steady-state connection-count controller fighting the pool on the same tick.
    /// `P2pInitializer` clamps it to enforce that.
    /// Default: 8
    pub proactive_dialing_floor: usize,
    /// Enable proactive peer dialing to recover from a connection count below `proactive_dialing_floor`
    /// Default: true
    pub proactive_dialing_enabled: bool,
    /// Multiplier for calculating how many peers to dial based on success rate
    /// Default: 2.5
    pub dialing_multiplier: f32,
    /// Window for tracking connection success rates for adaptive dialing
    /// Default: 5 minutes
    pub success_rate_tracking_window: Duration,
    /// Number of consecutive failures before activating circuit breaker
    /// Default: 3
    pub circuit_breaker_failure_threshold: usize,
    /// Time to wait before retrying a circuit-broken peer
    /// Default: 2 minutes
    pub circuit_breaker_retry_interval: Duration,
    /// Maximum seed peer age
    /// Default: 15 minutes
    pub max_seed_peer_age: Duration,
}

impl ConnectivityConfig {
    /// Whether this peer's circuit breaker is currently open, i.e. it has failed
    /// `circuit_breaker_failure_threshold` times in a row and has not yet waited out
    /// `circuit_breaker_retry_interval`.
    ///
    /// The single definition of that question. There are two places that must not dial a circuit-broken peer -
    /// `ConnectivityManagerActor::handle_dial_peer`, which covers every dial routed through the connectivity
    /// actor, and `ProactiveDialer::select_dial_candidates`, whose dials go straight to the connection manager
    /// and so never reach the actor at all. They are genuinely separate call sites; keeping the *predicate* in
    /// one place is what stops them from drifting apart.
    ///
    /// A peer with no recorded stats has never failed, so it is never circuit-broken.
    pub(crate) fn is_circuit_broken(&self, stats: Option<&PeerConnectionStats>) -> bool {
        stats.is_some_and(|stats| !stats.should_allow_connection(self.circuit_breaker_retry_interval))
    }
}

impl Default for ConnectivityConfig {
    fn default() -> Self {
        Self {
            min_connectivity: 1,
            connection_pool_refresh_interval: Duration::from_secs(60),
            reaper_min_inactive_age: Duration::from_secs(20 * 60),
            reaper_min_connection_threshold: 50,
            is_connection_reaping_enabled: true,
            max_failures_mark_offline: 1,
            connection_tie_break_linger: Duration::from_secs(2),
            expire_peer_last_seen_duration: Duration::from_secs(24 * 60 * 60),
            maintain_n_closest_connections_only: None,
            proactive_dialing_floor: 8,
            proactive_dialing_enabled: true,
            dialing_multiplier: 2.5,
            success_rate_tracking_window: Duration::from_secs(5 * 60),
            circuit_breaker_failure_threshold: 3,
            circuit_breaker_retry_interval: Duration::from_secs(2 * 60),
            max_seed_peer_age: Duration::from_secs(15 * 60),
        }
    }
}
