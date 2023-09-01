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
use tari_comms::peer_manager::PeerFeatures;

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

impl DiscoveryReady {
    pub fn new(context: NetworkDiscoveryContext) -> Self {
        Self {
            context,
            last_discovery: None,
        }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        self.last_discovery = self.context.last_round().await;

        match self.process().await {
            Ok(event) => event,
            Err(err) => err.into(),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn process(&mut self) -> Result<StateEvent, NetworkDiscoveryError> {
        let num_peers = self.context.peer_manager.count().await;
        debug!(target: LOG_TARGET, "Peer list currently contains {} entries", num_peers);

        // We don't have many peers - let's aggressively probe for them
        if num_peers < self.context.config.network_discovery.min_desired_peers {
            if self.context.num_rounds() >= self.config().network_discovery.idle_after_num_rounds {
                warn!(
                    target: LOG_TARGET,
                    "Still unable to obtain at minimum desired peers ({}) after {} rounds. Idling...",
                    self.config().network_discovery.min_desired_peers,
                    self.context.num_rounds(),
                );
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }

            warn!(
                target: LOG_TARGET,
                "DHT - Not enough current peers, choosing random peers to sync with"
            );

            let peers = self
                .context
                .peer_manager
                .random_peers(
                    self.config().network_discovery.max_sync_peers,
                    self.context.all_attempted_peers.read().await.as_slice(),
                )
                .await?;
            let peers = peers.into_iter().map(|p| p.node_id).collect::<Vec<_>>();

            if peers.is_empty() {
                debug!(
                    target: LOG_TARGET,
                    "No more sync peers after round #{}. Idling...",
                    self.context.num_rounds()
                );
                return Ok(StateEvent::Idle);
            }

            return Ok(StateEvent::BeginDiscovery(DiscoveryParams {
                num_peers_to_request: self.config().network_discovery.max_peers_to_sync_per_round,
                peers,
            }));
        }

        let last_round = self.context.last_round().await;

        if let Some(ref info) = last_round {
            // A discovery round just completed
            let round_num = self.context.increment_num_rounds();
            debug!(target: LOG_TARGET, "Completed peer round #{} ({})", round_num + 1, info);

            // If the last round was a success, but we didnt get any new peers, let's go to on connect or idle
            // depending on the number of peers we have
            if info.is_success() && !info.has_new_peers() {
                self.context.reset_num_rounds();
                if num_peers < self.context.config.network_discovery.min_desired_peers {
                    return Ok(StateEvent::Idle);
                } else {
                    // We have enough peers, so we can go to on connect mode
                    return Ok(StateEvent::OnConnectMode);
                }
            }

            if self.context.num_rounds() >= self.config().network_discovery.idle_after_num_rounds {
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }
        }

        let peers = match last_round {
            Some(ref stats) => {
                let num_peers_to_select = self.config().network_discovery.max_sync_peers;

                if stats.has_new_peers() {
                    debug!(
                        target: LOG_TARGET,
                        "Last peer sync round found {} new peer(s). Attempting to sync from those peers if they are \
                         closer than existing peers",
                        stats.num_new_peers,
                    );
                    self.context
                        .peer_manager
                        .closest_peers(
                            self.context.node_identity.node_id(),
                            num_peers_to_select,
                            self.context.all_attempted_peers.read().await.as_slice(),
                            Some(PeerFeatures::COMMUNICATION_NODE),
                        )
                        .await?
                        .into_iter()
                        .map(|p| p.node_id)
                        .collect::<Vec<_>>()
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Last peer sync round found no new peers. Transitioning to OnConnectMode",
                    );
                    return Ok(StateEvent::OnConnectMode);
                }
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "No previous round, selecting {} random peers for peer sync",
                    self.config().network_discovery.max_sync_peers,
                );
                self.context
                    .peer_manager
                    .random_peers(
                        self.config().network_discovery.max_sync_peers,
                        self.context.all_attempted_peers.read().await.as_slice(),
                    )
                    .await?
                    .into_iter()
                    .map(|p| p.node_id)
                    .collect::<Vec<_>>()
            },
        };

        if peers.is_empty() {
            debug!(
                target: LOG_TARGET,
                "No more sync peers after round #{}. Idling...",
                self.context.num_rounds()
            );
            return Ok(StateEvent::Idle);
        }

        Ok(StateEvent::BeginDiscovery(DiscoveryParams {
            num_peers_to_request: self.config().network_discovery.max_peers_to_sync_per_round,
            peers,
        }))
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
