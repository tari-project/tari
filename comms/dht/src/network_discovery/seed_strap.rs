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

use std::{collections::HashSet, convert::TryInto, sync::Arc, cmp};

use futures::StreamExt;
use log::*;
use rand::seq::SliceRandom;
use tari_comms::{
    Minimized,
    PeerConnection,
    peer_manager::NodeId,
    peer_validator::PeerValidatorError,
};

use crate::{
    network_discovery::{
        error::NetworkDiscoveryError,
        state_machine::{NetworkDiscoveryContext, StateEvent},
    },
    peer_validator::{DhtPeerValidatorError, PeerValidator},
    proto::rpc::{GetPeersRequest},
    rpc::{DhtClient, UnvalidatedPeerInfo},
    DhtConfig,
};

// Use a reasonable value based on the existing configuration
const DHT_RPC_MAX_PEERS_PER_REQUEST: u32 = 500;

const LOG_TARGET: &str = "comms::dht::network_discovery::seed_strap";

#[derive(Debug)]
pub(super) struct SeedStrap {
    context: NetworkDiscoveryContext,
}

impl SeedStrap {
    pub fn new(context: NetworkDiscoveryContext) -> Self {
        Self { context }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        debug!(target: LOG_TARGET, "Attempting to discover peers via seed nodes.");
        match self.discover_peers_via_seeds().await {
            Ok(num_added) => {
                if num_added == 0 {
                    warn!(
                        target: LOG_TARGET,
                        "No (new) peers were discovered via seed nodes. Transitioning to Ready state for other \
                         discovery methods or idling."
                    );
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Added {} peers via seed nodes. Transitioning to Ready state.", num_added
                    );
                }
                // Always transition to Ready; Ready state will decide if further discovery/idling is needed.
                StateEvent::Ready
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "Error during peer discovery via seed nodes: {}. Transitioning to Ready state regardless.", err
                );
                // If seed discovery fails, still go to Ready to allow other mechanisms or idling.
                StateEvent::Ready
            },
        }
    }

    async fn discover_peers_via_seeds(&mut self) -> Result<usize, NetworkDiscoveryError> {
        let seed_peers = self.context.connectivity.get_seeds().await?;
        if seed_peers.is_empty() {
            warn!(
                target: LOG_TARGET,
                "No seed peers configured. Unable to perform initial peer discovery via seeds."
            );
            return Ok(0);
        }

        let seed_node_ids: HashSet<NodeId> = seed_peers.iter().map(|p| p.node_id.clone()).collect();
        let mut total_peers_added = 0;

        let num_seeds_to_try = cmp::min(seed_peers.len(), self.context.config.network_discovery.max_seed_peer_sync_count);

        // Randomize the order of seed peers to distribute connection load
        // Using a separate scope to ensure ThreadRng is dropped before any await points
        let seed_peers_vec = {
            let mut seed_peers_vec = seed_peers.into_iter().collect::<Vec<_>>();
            let mut rng = rand::thread_rng();
            seed_peers_vec.shuffle(&mut rng);
            seed_peers_vec
        };

        // Create validator once outside the loop
        let validator = PeerValidator::new(&self.context.config);

        for seed_peer_candidate in seed_peers_vec.into_iter().take(num_seeds_to_try) {
            if seed_peer_candidate.node_id == *self.context.node_identity.node_id() {
                trace!(target: LOG_TARGET, "Skipping self as seed peer candidate.");
                continue;
            }

            debug!(
                target: LOG_TARGET,
                "Attempting to connect to seed peer '{}' to get their peer list", seed_peer_candidate.node_id
            );

            let mut conn = match self
                .context
                .connectivity
                .dial_peer(seed_peer_candidate.node_id.clone())
                .await
            {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to dial seed peer '{}': {}", seed_peer_candidate.node_id, e
                    );
                    continue;
                },
            };

            debug!(
                target: LOG_TARGET,
                "Connected to seed peer '{}'. Requesting peer list.", seed_peer_candidate.node_id
            );

            let peers_from_seed = match self.fetch_peers_from_connection(&mut conn).await {
                Ok(peers) => peers,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to fetch peers from seed peer '{}': {}. Disconnecting.", seed_peer_candidate.node_id, e
                    );
                    let _ = conn.disconnect(Minimized::Yes).await;
                    continue;
                },
            };

            debug!(
                target: LOG_TARGET,
                "Disconnecting from seed peer '{}'", seed_peer_candidate.node_id
            );
            if let Err(e) = conn.disconnect(Minimized::Yes).await {
                warn!(
                    target: LOG_TARGET,
                    "Failed to disconnect from seed peer '{}': {}", seed_peer_candidate.node_id, e
                );
            }

            let mut new_peers_this_round = 0;
            for peer_info_proto in peers_from_seed {
                let new_peer_candidate: UnvalidatedPeerInfo = match peer_info_proto.try_into() {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Invalid peer data received from seed peer '{}': {}",
                            seed_peer_candidate.node_id,
                            e
                        );
                        continue;
                    },
                };

                let candidate_node_id = NodeId::from_public_key(&new_peer_candidate.public_key);
                if seed_node_ids.contains(&candidate_node_id) ||
                    new_peer_candidate.public_key == *self.context.node_identity.public_key()
                {
                    trace!(
                        target: LOG_TARGET,
                        "Skipping adding known seed peer or self from seed peer's list: {}", candidate_node_id
                    );
                    continue;
                }

                let maybe_existing_peer = self
                    .context
                    .peer_manager
                    .find_by_public_key(&new_peer_candidate.public_key)
                    .await?;
                
                // Check if this is a new peer before moving maybe_existing_peer
                let is_new_peer = maybe_existing_peer.is_none();
                
                // Use reference instead of clone since UnvalidatedPeerInfo doesn't implement Clone
                match validator.validate_peer(new_peer_candidate, maybe_existing_peer) {
                    Ok(valid_peer) => {
                        debug!(
                            target: LOG_TARGET,
                            "Adding peer {} obtained from seed peer {}",
                            valid_peer.node_id,
                            seed_peer_candidate.node_id
                        );
                        self.context.peer_manager.add_peer(valid_peer).await?;
                        if is_new_peer {
                            new_peers_this_round += 1;
                        }
                    },
                    Err(
                        DhtPeerValidatorError::ValidatorError(PeerValidatorError::InvalidPeerSignature { .. }) |
                        DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerIdentityNoAddresses { .. }),
                    ) => {
                        warn!(
                            target: LOG_TARGET,
                            "Invalid peer data received from seed peer '{}' for peer {}: DhtPeerValidatorError. \
                             Banning seed peer.",
                            seed_peer_candidate.node_id,
                            candidate_node_id
                        );
                        // Best effort ban
                        let _ = self
                            .context
                            .connectivity
                            .ban_peer_until(
                                seed_peer_candidate.node_id.clone(),
                                self.config().ban_duration_short,
                                "Sent invalid peer data during seed bootstrap".to_string(),
                            )
                            .await;
                        // Since the seed peer is banned, we can stop trying to get peers from it in this round.
                        break;
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "Invalid peer data received from seed peer '{}' for peer {}: {}",
                            seed_peer_candidate.node_id,
                            candidate_node_id,
                            e
                        );
                    },
                }
            }
            total_peers_added += new_peers_this_round;
        }

        Ok(total_peers_added)
    }

    async fn fetch_peers_from_connection(
        &self,
        conn: &mut PeerConnection,
    ) -> Result<Vec<crate::proto::rpc::PeerInfo>, NetworkDiscoveryError> {
        let mut client = conn.connect_rpc::<DhtClient>().await?;
        // Request a moderate number of peers. We don't want to overwhelm the seed node.
        let num_peers_to_request = cmp::min(
            self.config().network_discovery.max_peers_to_sync_per_round,
            DHT_RPC_MAX_PEERS_PER_REQUEST,
        ).max(10) / 2; // Let's be conservative and ask for half the usual amount from a single seed
        let req = GetPeersRequest {
            n: num_peers_to_request,
            include_clients: false,          // For DHT bootstrap, we prefer nodes.
            max_claims: self.config().max_permitted_peer_claims.try_into().unwrap_or(u32::MAX),
            max_addresses_per_claim: self
                .config()
                .peer_validator_config
                .max_permitted_peer_addresses_per_claim
                .try_into()
                .unwrap_or(u32::MAX),
        };

        let mut peer_stream = client.get_peers(req).await?;
        let mut peers_from_seed = Vec::new();
        while let Some(resp) = peer_stream.next().await {
            let crate::proto::rpc::GetPeersResponse { peer } = resp?;
            if let Some(peer_info_proto) = peer {
                peers_from_seed.push(peer_info_proto);
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Seed peer '{}' sent an empty peer message part",
                    conn.peer_node_id()
                );
            }
        }
        Ok(peers_from_seed)
    }

    #[inline]
    fn config(&self) -> Arc<DhtConfig> {
        self.context.config.clone()
    }
}