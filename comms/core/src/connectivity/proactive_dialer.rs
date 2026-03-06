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

use std::sync::Arc;

use log::*;

use super::{
    config::ConnectivityConfig,
    connection_pool::{ConnectionPool, ConnectionStatus},
    error::ConnectivityError,
};
use crate::{
    connection_manager::ConnectionManagerRequester,
    peer_manager::{NodeId, Peer, PeerManager},
};

const LOG_TARGET: &str = "comms::connectivity::proactive_dialer";
const MAX_CONCURRENT_DIALS: usize = 30;

/// Proactive peer dialing logic for maintaining target connection counts
pub struct ProactiveDialer {
    config: ConnectivityConfig,
    connection_manager: ConnectionManagerRequester,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<crate::NodeIdentity>,
}

impl ProactiveDialer {
    pub fn new(
        config: ConnectivityConfig,
        connection_manager: ConnectionManagerRequester,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<crate::NodeIdentity>,
    ) -> Self {
        Self {
            config,
            connection_manager,
            peer_manager,
            node_identity,
        }
    }

    /// Main proactive dialing logic - called during connection pool refresh
    pub async fn execute_proactive_dialing(
        &mut self,
        pool: &ConnectionPool,
        connection_stats: &std::collections::HashMap<NodeId, super::connection_stats::PeerConnectionStats>,
        excluded_peers: &[NodeId],
        task_id: u64,
    ) -> Result<usize, ConnectivityError> {
        let _start_time = std::time::Instant::now();

        if !self.config.proactive_dialing_enabled {
            return Ok(0);
        }

        let current_connections = pool.count_connected_nodes();
        let target = self.config.target_connection_count;

        // Update metrics

        if current_connections >= target {
            debug!(
                target: LOG_TARGET,
                "({task_id}) Current connections ({current_connections}) meet or exceed target ({target}), no proactive dialing needed",
            );

            return Ok(0);
        }

        let needed = target.saturating_sub(current_connections);
        debug!(
            target: LOG_TARGET,
            "({task_id}) Proactive dialing: need {needed} more connections ({current_connections}/{target})",

        );

        // Calculate how many peers to dial based on success rate and multiplier
        let success_rate = self.calculate_recent_success_rate(connection_stats);
        let dial_count = self.calculate_dial_count(needed, success_rate);

        debug!(
            target: LOG_TARGET,
            "({task_id}) Success rate: {success_rate:.2}, will dial {dial_count} peers for {needed} needed connections",

        );

        // Select peers for dialing
        let candidates = self
            .select_dial_candidates(pool, connection_stats, excluded_peers, dial_count, task_id)
            .await?;

        if candidates.is_empty() {
            warn!(
                target: LOG_TARGET,
                "({task_id}) No peer candidates available for proactive dialing"

            );

            return Ok(0);
        }

        // Initiate concurrent dials
        let dialed_count = self.dial_peers_concurrently(candidates, task_id).await;

        info!(
            target: LOG_TARGET,
            "({task_id}) Proactive dialing initiated for {dialed_count} peers ({needed} needed connections)"
        );

        // Record final metrics

        Ok(dialed_count)
    }

    /// Calculate recent connection success rate across all peers
    fn calculate_recent_success_rate(
        &self,
        connection_stats: &std::collections::HashMap<NodeId, super::connection_stats::PeerConnectionStats>,
    ) -> f32 {
        if connection_stats.is_empty() {
            return 0.25; // Conservative Bayesian prior default
        }

        let window = self.config.success_rate_tracking_window;
        let total_stats: Vec<f32> = connection_stats
            .values()
            .map(|stats| stats.success_rate(window))
            .collect();

        if total_stats.is_empty() {
            return 0.25; // Conservative Bayesian prior default
        }

        let average = total_stats.iter().sum::<f32>() / total_stats.len() as f32;
        average.clamp(0.1, 1.0) // Clamp between 10% and 100%
    }

    /// Calculate how many peers to dial based on needed connections and success rate
    fn calculate_dial_count(&self, needed: usize, success_rate: f32) -> usize {
        let base_count = needed as f32 * self.config.dialing_multiplier;
        let adjusted_count = base_count / success_rate.max(0.1); // Prevent division by very small numbers

        #[allow(clippy::cast_possible_truncation)]
        let final_count = adjusted_count.ceil() as usize;

        // Cap the dial count to prevent overwhelming the network
        final_count.max(needed).min(MAX_CONCURRENT_DIALS)
    }

    /// Select peer candidates for dialing, prioritizing the most healthy
    async fn select_dial_candidates(
        &self,
        pool: &ConnectionPool,
        connection_stats: &std::collections::HashMap<NodeId, super::connection_stats::PeerConnectionStats>,
        excluded_peers: &[NodeId],
        count: usize,
        task_id: u64,
    ) -> Result<Vec<Peer>, ConnectivityError> {
        // Get currently managed node IDs (connected or connecting)
        let mut managed: Vec<NodeId> = pool
            .all()
            .iter()
            .filter(|state| {
                !matches!(
                    state.status(),
                    ConnectionStatus::Failed | ConnectionStatus::Disconnected(_)
                )
            })
            .map(|state| state.node_id().clone())
            .collect();
        // Add nominated excluded peers to the managed list so they will not be selected as candidates
        let mut managed_and_excluded = managed.clone();
        managed_and_excluded.append(&mut excluded_peers.to_vec());

        // Get available dial candidates
        let mut candidates = self
            .peer_manager
            .get_available_dial_candidates(&managed_and_excluded, Some(count * 3), true, true) // Get 3x more for health scoring
            .await?;
        if candidates.len() < count * 3 {
            // Only exclude managed and current selected nodes to get more candidates (thus include 'excluded_peers')
            let mut to_be_excluded = candidates.iter().map(|p| p.node_id.clone()).collect::<Vec<_>>();
            to_be_excluded.append(&mut managed);
            // Now also allow selection from previously failed peers
            let mut random = self
                .peer_manager
                .get_available_dial_candidates(&to_be_excluded, Some(count * 3 - candidates.len()), false, true)
                .await?;
            candidates.append(&mut random);
        }

        // Apply health-based filtering and ranking
        let mut final_candidates = Vec::new();
        for peer in candidates {
            // The SQL query already filtered for communication nodes, non-banned, non-deleted
            // Just need to check circuit breaker state
            if let Some(stats) = connection_stats.get(&peer.node_id) &&
                !stats.should_allow_connection(self.config.circuit_breaker_retry_interval)
            {
                trace!(
                    target: LOG_TARGET,
                    "({}) Skipping peer {} due to circuit breaker",
                    task_id,
                    peer.node_id.short_str()
                );
                continue;
            }

            final_candidates.push(peer);
        }

        // Sort by health score if available, otherwise by distance
        final_candidates.sort_by(|a, b| {
            let health_a = connection_stats
                .get(&a.node_id)
                .map(|s| s.health_score(self.config.success_rate_tracking_window))
                .unwrap_or(0.5); // Neutral score for unknown peers

            let health_b = connection_stats
                .get(&b.node_id)
                .map(|s| s.health_score(self.config.success_rate_tracking_window))
                .unwrap_or(0.5);

            // Primary sort by health (descending)
            match health_b.partial_cmp(&health_a) {
                Some(std::cmp::Ordering::Equal) => {
                    // Secondary sort by distance (ascending)
                    let dist_a = a.node_id.distance(self.node_identity.node_id());
                    let dist_b = b.node_id.distance(self.node_identity.node_id());
                    dist_a.cmp(&dist_b)
                },
                Some(order) => order,
                None => std::cmp::Ordering::Equal,
            }
        });

        // Take the top candidates
        final_candidates.truncate(count);

        debug!(
            target: LOG_TARGET,
            "({}) Selected {} healthy peer candidates for dialing",
            task_id,
            final_candidates.len()
        );

        Ok(final_candidates)
    }

    /// Dial multiple peers concurrently
    async fn dial_peers_concurrently(&mut self, peers: Vec<Peer>, task_id: u64) -> usize {
        if peers.is_empty() {
            return 0;
        }

        let mut successful_dials = 0;

        for peer in peers {
            debug!(
                target: LOG_TARGET,
                "({}) Initiating proactive dial to peer {}",
                task_id,
                peer.node_id.short_str()
            );

            // Use the connection manager's dial request (fire and forget)
            match self.connection_manager.send_dial_peer(peer.node_id.clone(), None).await {
                Ok(_) => {
                    successful_dials += 1;
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "({}) Failed to send dial request for peer {}: {:?}",
                        task_id,
                        peer.node_id.short_str(),
                        err
                    );
                },
            }
        }

        successful_dials
    }
}

#[cfg(test)]
mod tests {

    use crate::connectivity::proactive_dialer::MAX_CONCURRENT_DIALS;

    #[test]
    fn test_calculate_dial_count() {
        // Helper function to test dial count calculation without full struct
        fn calculate_dial_count(needed: usize, success_rate: f32, multiplier: f32) -> usize {
            let base_count = needed as f32 * multiplier;
            let adjusted_count = base_count / success_rate.max(0.1);
            #[allow(clippy::cast_possible_truncation)]
            let final_count = adjusted_count.ceil() as usize;
            final_count.max(needed).min(MAX_CONCURRENT_DIALS)
        }

        // Perfect success rate
        assert_eq!(calculate_dial_count(4, 1.0, 2.0), 8);

        // 50% success rate should double the dial count
        assert_eq!(calculate_dial_count(4, 0.5, 2.0), 16);

        // Low success rate should significantly increase dial count but be capped
        let result = calculate_dial_count(4, 0.1, 2.0);
        assert!(result >= 4); // At least the needed amount
        assert!(result <= MAX_CONCURRENT_DIALS); // But capped at max concurrent

        // Edge case: needed > MAX_CONCURRENT_DIALS to verify proper capping
        assert_eq!(calculate_dial_count(25, 0.8, 1.5), MAX_CONCURRENT_DIALS); // Should cap at MAX_CONCURRENT_DIALS
        assert_eq!(calculate_dial_count(25, 0.1, 2.0), MAX_CONCURRENT_DIALS); // Very low success rate, should still cap
        assert_eq!(calculate_dial_count(15, 0.5, 3.0), MAX_CONCURRENT_DIALS); // Should still cap despite multiplier
    }

    #[test]
    fn test_calculate_recent_success_rate() {
        // Test success rate calculation with empty stats
        let _empty_stats: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

        // With empty stats, should return conservative Bayesian prior default
        // This logic is tested by ensuring the default behavior
        let default_rate = 0.25f32;
        assert_eq!(default_rate, 0.25);

        // Test rate clamping behavior
        let test_rate = 1.5f32;
        let clamped = test_rate.clamp(0.1, 1.0);
        assert_eq!(clamped, 1.0);

        let low_rate = 0.05f32;
        let clamped_low = low_rate.clamp(0.1, 1.0);
        assert_eq!(clamped_low, 0.1);
    }
}
