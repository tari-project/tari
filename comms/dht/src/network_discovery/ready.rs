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

use log::*;
use tari_comms::peer_manager::{PeerFeatures, STALE_PEER_THRESHOLD_DURATION};

use super::{
    state_machine::{DiscoveryParams, NetworkDiscoveryContext, StateEvent},
    NetworkDiscoveryError,
};
use crate::{network_discovery::DhtNetworkDiscoveryRoundInfo, DhtConfig};

const LOG_TARGET: &str = "comms::dht::network_discovery::ready";

#[derive(Debug)]
pub(super) struct DiscoveryReady {
    context: NetworkDiscoveryContext,
    last_discovery: Option<DhtNetworkDiscoveryRoundInfo>,
}

// New helper function to select peers for discovery
async fn select_peers_for_discovery_round(
    context: &NetworkDiscoveryContext,
    last_round_info: Option<&DhtNetworkDiscoveryRoundInfo>,
    excluded_peers: &[tari_comms::peer_manager::NodeId],
    config: &DhtConfig,
) -> Result<Vec<tari_comms::peer_manager::NodeId>, NetworkDiscoveryError> {
    let peers_to_request_from = match last_round_info {
        Some(stats) if stats.has_new_peers() => {
            // If the last round had new peers, try to sync from those first or closest ones
            debug!(
                target: LOG_TARGET,
                "Last peer sync round found {} new peer(s). Selecting peers for discovery based on a 'closest' strategy.",
                stats.num_new_peers,
            );
            context
                .peer_manager
                .closest_n_active_peers(
                    context.node_identity.node_id(),
                    config.network_discovery.max_sync_peers,
                    excluded_peers,
                    Some(PeerFeatures::COMMUNICATION_NODE),
                    Some(STALE_PEER_THRESHOLD_DURATION),
                    true,
                    None,
                )
                .await?
        },
        _ => {
            // Default to random peers if no new peers from last round, or no last round info
            debug!(
                target: LOG_TARGET,
                "Selecting {} random peers for discovery (last round info available: {}, new peers in last round: {}).",
                config.network_discovery.max_sync_peers,
                last_round_info.is_some(),
                last_round_info.map(|s| s.has_new_peers()).unwrap_or(false),
            );
            context
                .peer_manager
                .random_peers(config.network_discovery.max_sync_peers, excluded_peers)
                .await?
        },
    };
    Ok(peers_to_request_from.into_iter().map(|p| p.node_id).collect::<Vec<_>>())
}

impl DiscoveryReady {
    pub fn new(context: NetworkDiscoveryContext) -> Self {
        Self {
            context,
            last_discovery: None,
        }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        self.last_discovery = self.context.last_round().await;

        // Get current number of rounds before processing
        let current_num_rounds = self.context.num_rounds();

        match self.process(current_num_rounds).await {
            Ok(event) => event,
            Err(err) => err.into(),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn process(&mut self, current_num_rounds: usize) -> Result<StateEvent, NetworkDiscoveryError> {
        let num_peers = self.context.peer_manager.count().await;
        debug!(
            target: LOG_TARGET,
            "NetworkDiscovery::Ready: Peer list contains {} entries. Current discovery rounds in this cycle: {}.",
            num_peers,
            current_num_rounds
        );

        let min_desired_peers = self.config().network_discovery.min_desired_peers;

        // Scenario 1: Not enough peers overall. Must discover.
        if num_peers < min_desired_peers {
            debug!(
                target: LOG_TARGET,
                "Number of peers ({}) is less than min_desired_peers ({}). Attempting discovery.",
                num_peers,
                min_desired_peers
            );

            if current_num_rounds >= self.config().network_discovery.idle_after_num_rounds {
                warn!(
                    target: LOG_TARGET,
                    "Still unable to obtain minimum desired peers ({}) after {} rounds. Idling...",
                    min_desired_peers,
                    current_num_rounds,
                );
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }

            let excluded_peers = self.context.all_attempted_peers.read().await.clone();
            // Use helper to get peers
            let peers_for_discovery =
                select_peers_for_discovery_round(&self.context, None, &excluded_peers, self.config()).await?;

            if peers_for_discovery.is_empty() {
                debug!(target: LOG_TARGET, "No peers available to attempt discovery (num_peers < min_desired_peers path). Idling.");
                return Ok(StateEvent::Idle);
            }

            return Ok(StateEvent::BeginDiscovery(DiscoveryParams {
                num_peers_to_request: self.config().network_discovery.max_peers_to_sync_per_round,
                peers: peers_for_discovery,
            }));
        }

        // Scenario 2: Enough peers overall (num_peers >= min_desired_peers).
        // Check if this is the first round of decision-making in this "active" discovery cycle.
        if current_num_rounds == 0 {
            debug!(
                target: LOG_TARGET,
                "First active round (current_num_rounds = 0) and num_peers ({}) >= min_desired_peers ({}). Forcing DHT discovery.",
                num_peers, min_desired_peers
            );

            let excluded_peers = self.context.all_attempted_peers.read().await.clone();
            let peers_for_discovery =
                select_peers_for_discovery_round(&self.context, None, &excluded_peers, self.config()).await?;

            if peers_for_discovery.is_empty() {
                debug!(
                    target: LOG_TARGET,
                    "No suitable peers found for the forced DHT discovery round (current_num_rounds = 0 path). Transitioning to Idle."
                );
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }

            return Ok(StateEvent::BeginDiscovery(DiscoveryParams {
                num_peers_to_request: self.config().network_discovery.max_peers_to_sync_per_round,
                peers: peers_for_discovery,
            }));
        }

        // Scenario 3: Enough peers overall (num_peers >= min_desired_peers), and this is NOT the first
        // round based on current_num_rounds > 0 (i.e. some discovery has happened in this cycle).
        let last_round_info_option = self.context.last_round().await;
        if let Some(ref info) = last_round_info_option {
            debug!(
                target: LOG_TARGET,
                "Processing after completed round #{}: {}",
                current_num_rounds, info
            );

            // NEW: Special handling if this is the first actual discovery phase after SeedStrap
            //      (i.e., current_num_rounds == 1 indicates SeedStrap was the completed round)
            //      and SeedStrap was very successful.
            //      A "very successful" SeedStrap is one that found at least `max_peers_to_sync_per_round`
            //      new peers and also met the `min_desired_peers` benchmark.
            if current_num_rounds == 1 && // Signifies that the previous round was SeedStrap
               info.is_success() &&
               info.num_new_peers >= self.config().network_discovery.max_peers_to_sync_per_round as usize &&
               info.num_new_peers >= self.config().network_discovery.min_desired_peers
            {
                info!(
                    target: LOG_TARGET,
                    "SeedStrap round was very successful (new peers: {}, which is >= max_peers_to_sync_per_round config: {} and >= min_desired_peers config: {}). Transitioning to OnConnectMode for less aggressive discovery.",
                    info.num_new_peers,
                    self.config().network_discovery.max_peers_to_sync_per_round,
                    self.config().network_discovery.min_desired_peers
                );
                self.context.reset_num_rounds(); // Reset rounds as this bootstrap + initial check implies discovery cycle could pause or shift mode.
                return Ok(StateEvent::OnConnectMode);
            }
            // Existing logic:
            // If the last round was a success, but we didn't get any new peers, let's go to on connect or idle
            // depending on the_ number of peers we have
            if info.is_success() && !info.has_new_peers() {
                debug!(
                    target: LOG_TARGET,
                    "Round #{} was successful but found no new peers. num_peers ({}) >= min_desired ({}). Transitioning to OnConnectMode.",
                    current_num_rounds, num_peers, min_desired_peers
                );
                self.context.reset_num_rounds();
                return Ok(StateEvent::OnConnectMode);
            }

            // If we have performed enough rounds...
            if current_num_rounds >= self.config().network_discovery.idle_after_num_rounds {
                debug!(
                    target: LOG_TARGET,
                    "Sufficient number of discovery rounds ({}) completed ({}/{}). Idling.",
                    current_num_rounds, current_num_rounds, self.config().network_discovery.idle_after_num_rounds
                );
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }
        }
        // Fallthrough: continue discovery if:
        // - last_round_info_option is None (but current_num_rounds > 0 - should not happen if SeedStrap always sets
        //   last_round_info, this path is more for re-entry from Idle/OnConnect where last_round might be old/cleared)
        //   OR
        // - last_round_info_option showed new peers or failed (and the new SeedStrap success condition above wasn't
        //   met), AND
        // - idle_after_num_rounds not yet reached.
        let excluded_peers = self.context.all_attempted_peers.read().await.clone();
        let last_round_info_exists = last_round_info_option.is_some();
        debug!(
            target: LOG_TARGET,
            "Proceeding with further DHT discovery (num_rounds = {}, last_round_info_exists = {}).",
            current_num_rounds, last_round_info_exists
        );

        let peers_for_discovery = select_peers_for_discovery_round(
            &self.context,
            last_round_info_option.as_ref(),
            &excluded_peers,
            self.config(),
        )
        .await?;

        if peers_for_discovery.is_empty() {
            debug!(target: LOG_TARGET, "No peers available to attempt discovery (after all checks in 'Ready' state). Idling. num_rounds = {}", current_num_rounds);
            self.context.reset_num_rounds();
            return Ok(StateEvent::Idle);
        }

        Ok(StateEvent::BeginDiscovery(DiscoveryParams {
            num_peers_to_request: self.config().network_discovery.max_peers_to_sync_per_round,
            peers: peers_for_discovery,
        }))
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
