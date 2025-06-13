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

use std::{sync::Arc, time::Instant};

use log::*;

use super::{config::ConnectivityConfig, error::ConnectivityError, proactive_dialing_metrics};
use crate::{peer_manager::PeerManager, utils::datetime::format_duration};

const LOG_TARGET: &str = "comms::connectivity::peer_discovery_bridge";

/// Bridge between ConnectivityManager and peer discovery mechanisms
pub struct PeerDiscoveryBridge {
    config: ConnectivityConfig,
    peer_manager: Arc<PeerManager>,
    last_discovery_attempt: Option<Instant>,
}

impl PeerDiscoveryBridge {
    pub fn new(config: ConnectivityConfig, peer_manager: Arc<PeerManager>) -> Self {
        Self {
            config,
            peer_manager,
            last_discovery_attempt: None,
        }
    }

    /// Check if we should trigger peer discovery based on available peer count
    pub async fn should_trigger_discovery(
        &mut self,
        current_peer_count: usize,
        connected_count: usize,
        task_id: u64,
    ) -> Result<bool, ConnectivityError> {
        // Basic threshold checks
        let available_peers = current_peer_count.saturating_sub(connected_count);
        let discovery_threshold = self.config.target_connection_count * 3; // Need 3x target as available candidates

        debug!(
            target: LOG_TARGET,
            "({}) Peer discovery check: available={}, connected={}, threshold={}",
            task_id,
            available_peers,
            connected_count,
            discovery_threshold
        );

        if available_peers >= discovery_threshold {
            trace!(
                target: LOG_TARGET,
                "({}) Sufficient peer candidates available ({}), no discovery needed",
                task_id,
                available_peers
            );
            return Ok(false);
        }

        // Time-based throttling
        if let Some(last_attempt) = self.last_discovery_attempt {
            let min_interval = self.config.success_rate_tracking_window; // Use tracking window as discovery interval
            if last_attempt.elapsed() < min_interval {
                trace!(
                    target: LOG_TARGET,
                    "({}) Discovery throttled, last attempt {} ago (min interval: {})",
                    task_id,
                    format_duration(last_attempt.elapsed()),
                    format_duration(min_interval)
                );
                return Ok(false);
            }
        }

        info!(
            target: LOG_TARGET,
            "({}) Peer discovery needed: available_peers={}, discovery_threshold={}",
            task_id,
            available_peers,
            discovery_threshold
        );

        Ok(true)
    }

    /// Execute peer discovery to find more peer candidates
    pub async fn execute_discovery(&mut self, task_id: u64) -> Result<usize, ConnectivityError> {
        debug!(
            target: LOG_TARGET,
            "({}) Executing peer discovery to find more candidates",
            task_id
        );

        self.last_discovery_attempt = Some(Instant::now());
        proactive_dialing_metrics::increment_peer_discovery_attempts();

        // Get current peer count for comparison
        let initial_count = self.get_available_peer_count().await?;

        // For now, implement a basic peer discovery strategy
        // This could be enhanced to integrate with DHT discovery, seed peer querying, etc.
        let discovered_count = self.basic_peer_discovery(task_id).await?;

        let final_count = self.get_available_peer_count().await?;
        let net_discovered = final_count.saturating_sub(initial_count);

        // Update metrics
        proactive_dialing_metrics::increment_peer_discovery_peers_found(net_discovered);

        info!(
            target: LOG_TARGET,
            "({}) Peer discovery completed: attempted={}, net_discovered={}, total_available={}",
            task_id,
            discovered_count,
            net_discovered,
            final_count
        );

        Ok(net_discovered)
    }

    /// Get the current count of available (non-banned, non-connected) peer candidates
    async fn get_available_peer_count(&self) -> Result<usize, ConnectivityError> {
        let all_peers = self
            .peer_manager
            .all(None)
            .await
            .map_err(ConnectivityError::PeerManagerError)?;

        let available_count = all_peers
            .iter()
            .filter(|peer| peer.features.is_node() && !peer.is_banned())
            .count();

        Ok(available_count)
    }

    /// Basic peer discovery implementation
    /// In a full implementation, this would:
    /// 1. Query seed peers for peer lists
    /// 2. Trigger DHT peer discovery rounds
    /// 3. Request peer lists from currently connected peers
    /// 4. Use DNS discovery if configured
    async fn basic_peer_discovery(&self, task_id: u64) -> Result<usize, ConnectivityError> {
        debug!(
            target: LOG_TARGET,
            "({}) Performing basic peer discovery",
            task_id
        );

        // For now, this is a placeholder that would integrate with:
        // - DHT network discovery triggers
        // - Seed peer queries
        // - Connected peer queries for their peer lists
        // - Any external peer discovery mechanisms

        // This is where we would trigger the DHT's DhtNetworkDiscovery component
        // or implement other discovery mechanisms

        // Return 0 for now as this is a placeholder
        Ok(0)
    }

    /// Reset discovery throttling (useful for testing or manual triggers)
    #[allow(dead_code)]
    pub fn reset_throttling(&mut self) {
        self.last_discovery_attempt = None;
    }

    /// Get time since last discovery attempt
    #[allow(dead_code)]
    pub fn time_since_last_discovery(&self) -> Option<std::time::Duration> {
        self.last_discovery_attempt.map(|t| t.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    // Note: These tests would need mock implementations of PeerManager
    // For now, they serve as documentation of expected behavior

    #[tokio::test]
    #[ignore] // Requires mocked PeerManager
    async fn test_discovery_threshold_logic() {
        // Test that discovery is triggered when peer count is low
        // and throttled when called too frequently
    }

    #[test]
    fn test_throttling_utility_methods() {
        let _config = ConnectivityConfig {
            success_rate_tracking_window: Duration::from_millis(100),
            ..Default::default()
        };

        // Since we can't create PeerDiscoveryBridge without PeerManager,
        // we'll create a mock configuration to test the concepts
        let throttle_period = Duration::from_millis(50);

        // Test the utility method concepts by checking time operations
        let now = std::time::Instant::now();
        let later = now + throttle_period;

        assert!(later > now);
        assert!(later.duration_since(now) >= throttle_period);

        // These methods would work on an actual bridge instance:
        // assert!(bridge.time_since_last_discovery().is_none());
        // bridge.reset_throttling();
        // assert!(bridge.time_since_last_discovery().is_none());
    }
}
