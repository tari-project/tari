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
        state_machine::{NetworkDiscoveryContext, StateEvent, DhtNetworkDiscoveryRoundInfo},
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
        debug!(target: LOG_TARGET, "Attempting to discover peers via seed nodes (SeedStrap).");

        let mut round_info = DhtNetworkDiscoveryRoundInfo {
            num_new_peers: 0,
            num_duplicate_peers: 0,
            num_succeeded: 0,
            sync_peers: Vec::new(),
        };

        match self.discover_peers_via_seeds(&mut round_info).await {
            Ok(num_added) => {
                round_info.num_new_peers = num_added;

                if round_info.num_succeeded == 0 && num_added == 0 {
                     warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Failed to contact any seed nodes or retrieve new peers."
                    );
                } else if num_added == 0 {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: No (new) peers were discovered via seed nodes ({} successful seed node contacts).",
                         round_info.num_succeeded
                    );
                } else {
                    info!(
                        target: LOG_TARGET,
                        "SeedStrap: Added {} (new) peers via seed nodes ({} successful seed node contacts).",
                        num_added,
                        round_info.num_succeeded
                    );
                }
                
                debug!(
                    target: LOG_TARGET,
                    "SeedStrap: Round info at completion - new_peers: {}, duplicate_peers: {}, succeeded: {}, sync_peers: {}",
                    round_info.num_new_peers,
                    round_info.num_duplicate_peers,
                    round_info.num_succeeded,
                    round_info.sync_peers.len()
                );
                
                StateEvent::DiscoveryComplete(round_info)
            },
            Err(err) => {
                warn!(
                    target: LOG_TARGET,
                    "SeedStrap: Error during peer discovery via seed nodes: {}. SeedStrap round considered failed.", err
                );
                
                debug!(
                    target: LOG_TARGET,
                    "SeedStrap: Round info at failure - new_peers: {}, duplicate_peers: {}, succeeded: {}, sync_peers: {}",
                    round_info.num_new_peers,
                    round_info.num_duplicate_peers,
                    round_info.num_succeeded,
                    round_info.sync_peers.len()
                );
                
                StateEvent::DiscoveryComplete(round_info)
            },
        }
    }

    async fn discover_peers_via_seeds(&mut self, round_info: &mut DhtNetworkDiscoveryRoundInfo) -> Result<usize, NetworkDiscoveryError> {
        let seed_peers_available = self.context.connectivity.get_seeds().await?;
        debug!(
            target: LOG_TARGET,
            "SeedStrap: Available seed peers from connectivity.get_seeds(): {}. Max to try (config): {}",
            seed_peers_available.len(),
            self.context.config.network_discovery.max_seed_peer_sync_count
        );

        if seed_peers_available.is_empty() {
            warn!(
                target: LOG_TARGET,
                "SeedStrap: No seed peers configured. Unable to perform initial peer discovery via seeds."
            );
            return Ok(0);
        }

        let seed_node_ids: HashSet<NodeId> = seed_peers_available.iter().map(|p| p.node_id.clone()).collect();
        let mut total_peers_added = 0;
        let mut total_duplicates = 0;

        let num_seeds_to_try = cmp::min(
            seed_peers_available.len(),
            self.context.config.network_discovery.max_seed_peer_sync_count,
        );

        let seed_peers_vec = {
            let mut seed_peers_vec = seed_peers_available.into_iter().collect::<Vec<_>>();
            let mut rng = rand::thread_rng();
            seed_peers_vec.shuffle(&mut rng);
            seed_peers_vec
        };
        
        // Store the seed peers we're attempting to contact in round_info
        round_info.sync_peers = seed_peers_vec.iter().take(num_seeds_to_try).map(|p| p.node_id.clone()).collect();

        debug!(
            target: LOG_TARGET,
            "SeedStrap: Preparing to sync from up to {} seed peers. Selected peer IDs: {:?}",
            num_seeds_to_try,
            round_info.sync_peers
        );

        let validator = PeerValidator::new(&self.context.config);

        for (idx, seed_peer_candidate) in seed_peers_vec.into_iter().take(num_seeds_to_try).enumerate() {
            debug!(
                target: LOG_TARGET,
                "SeedStrap: Iteration {}/{} with seed peer '{}'",
                idx + 1,
                num_seeds_to_try,
                seed_peer_candidate.node_id
            );

            if self.context.node_identity.node_id() == &seed_peer_candidate.node_id {
                info!(
                    target: LOG_TARGET, 
                    "SeedStrap: Skipping self as seed peer candidate (node_id: {}).",
                    seed_peer_candidate.node_id
                );
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
                        "SeedStrap: Failed to dial seed peer '{}': {}. Continuing to next seed candidate.", 
                        seed_peer_candidate.node_id, 
                        e
                    );
                    continue;
                },
            };

            debug!(
                target: LOG_TARGET,
                "SeedStrap: Connected to seed peer '{}'. Requesting peer list.", seed_peer_candidate.node_id
            );

            let peers_from_seed = match self.fetch_peers_from_connection(&mut conn).await {
                Ok(peers) => {
                    round_info.num_succeeded += 1; // Successfully contacted this seed
                    peers
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Failed to fetch peers from seed peer '{}': {}. Disconnecting and continuing.", 
                        seed_peer_candidate.node_id, 
                        e
                    );
                    debug!(
                        target: LOG_TARGET,
                        "SeedStrap: Attempting to disconnect from seed peer '{}' after fetch failure", 
                        seed_peer_candidate.node_id
                    );
                    if let Err(disc_err) = conn.disconnect(Minimized::Yes).await {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: Also failed to disconnect from seed peer '{}' after fetch failure: {}", 
                            seed_peer_candidate.node_id, 
                            disc_err
                        );
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "SeedStrap: Successfully disconnected from seed peer '{}' after fetch failure", 
                            seed_peer_candidate.node_id
                        );
                    }
                    continue;
                },
            };

            if peers_from_seed.is_empty() {
                info!(
                    target: LOG_TARGET,
                    "SeedStrap: Seed peer '{}' returned an empty peer list. Disconnecting.",
                    seed_peer_candidate.node_id
                );
                if let Err(e) = conn.disconnect(Minimized::Yes).await {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Failed to disconnect from seed peer '{}' after receiving empty list: {}", 
                        seed_peer_candidate.node_id, 
                        e
                    );
                }
                continue;
            }

            debug!(
                target: LOG_TARGET,
                "SeedStrap: Successfully fetched {} peer entries from seed peer '{}'. Processing peers...",
                peers_from_seed.len(),
                seed_peer_candidate.node_id
            );

            debug!(
                target: LOG_TARGET,
                "SeedStrap: Disconnecting from seed peer '{}' before processing peers",
                seed_peer_candidate.node_id
            );
            if let Err(e) = conn.disconnect(Minimized::Yes).await {
                warn!(
                    target: LOG_TARGET,
                    "SeedStrap: Failed to disconnect from seed peer '{}': {}", seed_peer_candidate.node_id, e
                );
            } else {
                debug!(
                    target: LOG_TARGET,
                    "SeedStrap: Successfully disconnected from seed peer '{}'", seed_peer_candidate.node_id
                );
            }

            let mut new_peers_this_seed = 0;
            let mut duplicates_this_seed = 0;
            let mut ban_this_seed = false;

            let peers_count = peers_from_seed.len();
            
            debug!(
                target: LOG_TARGET,
                "SeedStrap: Beginning to process {} peers from seed peer '{}'",
                peers_count,
                seed_peer_candidate.node_id
            );

            for (peer_idx, peer_info_proto) in peers_from_seed.into_iter().enumerate() {
                if peer_idx % 10 == 0 || peer_idx == 0 {
                    trace!(
                        target: LOG_TARGET,
                        "SeedStrap: Processing peer {}/{} from seed '{}'",
                        peer_idx + 1,
                        peers_count,
                        seed_peer_candidate.node_id
                    );
                }

                let new_peer_candidate: UnvalidatedPeerInfo = match peer_info_proto.try_into() {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: Invalid peer data received from seed peer '{}': {}",
                            seed_peer_candidate.node_id,
                            e
                        );
                        continue;
                    },
                };

                let candidate_node_id = NodeId::from_public_key(&new_peer_candidate.public_key);
                
                // Skip self
                if new_peer_candidate.public_key == *self.context.node_identity.public_key() {
                    trace!(
                        target: LOG_TARGET,
                        "SeedStrap: Skipping self (node_id: {}) from seed peer's list.", 
                        candidate_node_id
                    );
                    continue;
                }
                
                // Skip known seed peers
                if seed_node_ids.contains(&candidate_node_id) {
                    trace!(
                        target: LOG_TARGET,
                        "SeedStrap: Skipping known seed peer ({}) from seed peer's list.", 
                        candidate_node_id
                    );
                    continue;
                }

                let maybe_existing_peer = match self
                    .context
                    .peer_manager
                    .find_by_public_key(&new_peer_candidate.public_key)
                    .await 
                {
                    Ok(peer) => peer,
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: Error searching for existing peer by public key: {}. Skipping this peer candidate.",
                            e
                        );
                        continue;
                    }
                };
                
                let is_new_peer = maybe_existing_peer.is_none();
                
                match validator.validate_peer(new_peer_candidate, maybe_existing_peer) {
                    Ok(valid_peer) => {
                        debug!(
                            target: LOG_TARGET,
                            "SeedStrap: Adding peer {} obtained from seed peer {}",
                            valid_peer.node_id,
                            seed_peer_candidate.node_id
                        );
                        
                        match self.context.peer_manager.add_peer(valid_peer).await {
                            Ok(_) => {
                                if is_new_peer {
                                    new_peers_this_seed += 1;
                                } else {
                                    duplicates_this_seed += 1;
                                }
                            },
                            Err(e) => {
                                warn!(
                                    target: LOG_TARGET,
                                    "SeedStrap: Failed to add validated peer {}: {}",
                                    candidate_node_id,
                                    e
                                );
                            }
                        }
                    },
                    Err(
                        DhtPeerValidatorError::ValidatorError(PeerValidatorError::InvalidPeerSignature { .. }) |
                        DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerIdentityNoAddresses { .. }),
                    ) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: Ban-worthy invalid peer data received from seed peer '{}' for peer {}. Will ban seed peer.",
                            seed_peer_candidate.node_id,
                            candidate_node_id
                        );
                        ban_this_seed = true;
                        break;
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: Invalid peer data received from seed peer '{}' for peer {}: {}",
                            seed_peer_candidate.node_id,
                            candidate_node_id,
                            e
                        );
                    },
                }
            }
            
            if ban_this_seed {
                warn!(
                    target: LOG_TARGET,
                    "SeedStrap: Banning seed peer '{}' for providing invalid peer data.",
                    seed_peer_candidate.node_id
                );
                
                match self
                    .context
                    .connectivity
                    .ban_peer_until(
                        seed_peer_candidate.node_id.clone(),
                        self.config().ban_duration_short,
                        "Sent invalid peer data during seed bootstrap".to_string(),
                    )
                    .await 
                {
                    Ok(_) => {
                        debug!(
                            target: LOG_TARGET,
                            "SeedStrap: Successfully banned seed peer '{}'",
                            seed_peer_candidate.node_id
                        );
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: Failed to ban seed peer '{}': {}",
                            seed_peer_candidate.node_id,
                            e
                        );
                    }
                }
            }
            
            total_peers_added += new_peers_this_seed;
            total_duplicates += duplicates_this_seed;
            round_info.num_duplicate_peers += duplicates_this_seed;
            
            info!(
                target: LOG_TARGET, 
                "SeedStrap: Finished processing peers from seed {}. New peers from this seed: {}. Duplicates: {}. Total new peers so far: {}.", 
                seed_peer_candidate.node_id, 
                new_peers_this_seed,
                duplicates_this_seed,
                total_peers_added
            );
        }

        info!(
            target: LOG_TARGET,
            "SeedStrap: Completed loop over seed peers. Total new peers added: {}. Total duplicates: {}. Seeds successfully contacted: {}/{}",
            total_peers_added,
            total_duplicates,
            round_info.num_succeeded,
            round_info.sync_peers.len()
        );
        Ok(total_peers_added)
    }

    async fn fetch_peers_from_connection(
        &self,
        conn: &mut PeerConnection,
    ) -> Result<Vec<crate::proto::rpc::PeerInfo>, NetworkDiscoveryError> {
        debug!(
            target: LOG_TARGET,
            "SeedStrap: Beginning RPC connection to seed peer '{}'", 
            conn.peer_node_id()
        );
        
        let mut client = match conn.connect_rpc::<DhtClient>().await {
            Ok(client) => {
                debug!(
                    target: LOG_TARGET,
                    "SeedStrap: Successfully connected RPC client to seed peer '{}'", 
                    conn.peer_node_id()
                );
                client
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "SeedStrap: Failed to connect RPC client to seed peer '{}': {}", 
                    conn.peer_node_id(),
                    e
                );
                return Err(e.into());
            }
        };
        
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

        debug!(
            target: LOG_TARGET,
            "SeedStrap: Calling get_peers RPC to request {} peers from seed '{}'", 
            num_peers_to_request,
            conn.peer_node_id()
        );
        
        let mut peer_stream = match client.get_peers(req).await {
            Ok(stream) => {
                debug!(
                    target: LOG_TARGET,
                    "SeedStrap: Successfully initiated get_peers stream from seed peer '{}'", 
                    conn.peer_node_id()
                );
                stream
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "SeedStrap: Failed to initiate get_peers stream from seed peer '{}': {}", 
                    conn.peer_node_id(),
                    e
                );
                return Err(e.into());
            }
        };

        debug!(
            target: LOG_TARGET,
            "SeedStrap: Beginning to collect peer stream items from seed '{}'", 
            conn.peer_node_id()
        );
        
        let mut peers_from_seed = Vec::new();
        let mut stream_item_count = 0;
        
        while let Some(resp) = peer_stream.next().await {
            stream_item_count += 1;
            trace!(
                target: LOG_TARGET,
                "SeedStrap: Received stream item #{} from seed '{}'", 
                stream_item_count,
                conn.peer_node_id()
            );
            
            match resp {
                Ok(crate::proto::rpc::GetPeersResponse { peer }) => {
                    if let Some(peer_info_proto) = peer {
                        trace!(
                            target: LOG_TARGET,
                            "SeedStrap: Stream item #{} from seed '{}' contains a peer", 
                            stream_item_count,
                            conn.peer_node_id()
                        );
                        peers_from_seed.push(peer_info_proto);
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "SeedStrap: Stream item #{} from seed '{}' contains empty peer message", 
                            stream_item_count,
                            conn.peer_node_id()
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Error in stream item #{} from seed '{}': {}", 
                        stream_item_count,
                        conn.peer_node_id(),
                        e
                    );
                    return Err(e.into());
                }
            }
        }
        
        info!(
            target: LOG_TARGET,
            "SeedStrap: Received {} total peer entries from seed peer '{}' (in {} stream items)",
            peers_from_seed.len(),
            conn.peer_node_id(),
            stream_item_count
        );
        
        Ok(peers_from_seed)
    }

    #[inline]
    fn config(&self) -> Arc<DhtConfig> {
        self.context.config.clone()
    }
}