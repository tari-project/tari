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

use super::{
    state_machine::{DiscoveryParams, NetworkDiscoveryContext, StateEvent},
    DhtNetworkDiscoveryRoundInfo,
    NetworkDiscoveryError,
};
use crate::DhtConfig;
use log::*;
use std::cmp;
use tari_comms::peer_manager::{NodeId, PeerFeatures};

const LOG_TARGET: &str = "comms::dht::network_discovery";

#[derive(Debug)]
pub(super) struct DiscoveryReady {
    context: NetworkDiscoveryContext,
    last_discovery: Option<DhtNetworkDiscoveryRoundInfo>,
}

impl DiscoveryReady {
    pub fn new(context: NetworkDiscoveryContext, last_discovery: DhtNetworkDiscoveryRoundInfo) -> Self {
        Self {
            context,
            last_discovery: Some(last_discovery),
        }
    }

    pub fn initial(context: NetworkDiscoveryContext) -> Self {
        Self {
            context,
            last_discovery: None,
        }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        match self.process().await {
            Ok(event) => event,
            Err(err) => err.into(),
        }
    }

    async fn process(&mut self) -> Result<StateEvent, NetworkDiscoveryError> {
        let num_peers = self.context.peer_manager.count().await;
        debug!(target: LOG_TARGET, "Peer list currently contains {} entries", num_peers);

        // We don't have many peers - let's aggressively probe for them
        if num_peers < self.context.config.network_discovery.min_desired_peers {
            let peers = self
                .context
                .peer_manager
                .random_peers(
                    self.config().network_discovery.max_sync_peers,
                    self.previous_sync_peers(),
                )
                .await?;
            return Ok(StateEvent::BeginDiscovery(DiscoveryParams {
                // All peers
                num_peers_to_request: None,
                peers: peers.into_iter().map(|p| p.node_id).collect(),
            }));
        }

        if let Some(info) = self.last_discovery.as_ref() {
            // A discovery round just completed
            let round_num = self.context.increment_num_rounds();
            debug!(target: LOG_TARGET, "Completed peer round #{} ({})", round_num + 1, info);

            if !info.has_new_neighbours() {
                debug!(
                    target: LOG_TARGET,
                    "No new neighbours found this round {}. Going to on connect mode", info,
                );
                return Ok(StateEvent::OnConnectMode);
            }

            // If the last round was a success, but we didnt get any new peers, let's IDLE
            if info.is_success() && !info.has_new_peers() && self.context.num_rounds() > 0 {
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }

            if self.context.num_rounds() >= self.config().network_discovery.idle_after_num_rounds {
                self.context.reset_num_rounds();
                return Ok(StateEvent::Idle);
            }
        }

        let peers = match self.last_discovery {
            Some(ref stats) => {
                let num_peers_to_select =
                    cmp::min(stats.num_new_neighbours, self.config().network_discovery.max_sync_peers);

                if stats.has_new_neighbours() {
                    self.context
                        .peer_manager
                        .closest_peers(
                            self.context.node_identity.node_id(),
                            num_peers_to_select,
                            self.previous_sync_peers(),
                            Some(PeerFeatures::COMMUNICATION_NODE),
                        )
                        .await?
                        .into_iter()
                        .map(|p| p.node_id)
                        .collect()
                } else {
                    // As soon as we cannot find any more neighbours, enter passive mode
                    return Ok(StateEvent::OnConnectMode);
                }
            },
            None => self
                .context
                .peer_manager
                .random_peers(
                    self.config().network_discovery.max_sync_peers,
                    self.previous_sync_peers(),
                )
                .await?
                .into_iter()
                .map(|p| p.node_id)
                .collect(),
        };

        // let max_accept_closer_peers = cmp::max(
        //     // Want to get to the min_desired_peers as quickly as possible
        //     self.config()
        //         .network_discovery
        //         .min_desired_peers
        //         .saturating_sub(num_peers),
        //     // Otherwise we want to be a bit more 'cautious' about accepting neighbouring peers
        //     self.config().num_neighbouring_nodes,
        // );
        Ok(StateEvent::BeginDiscovery(DiscoveryParams {
            // Request all peers
            num_peers_to_request: None,
            peers,
        }))
    }

    fn previous_sync_peers(&self) -> &[NodeId] {
        self.last_discovery
            .as_ref()
            .map(|info| info.sync_peers.as_slice())
            .unwrap_or(&[])
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
