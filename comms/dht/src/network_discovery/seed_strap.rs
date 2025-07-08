// Copyright 2020, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{cmp, collections::HashSet, convert::TryInto, time::Duration};

use futures::StreamExt;
use log::*;
use rand::seq::IteratorRandom;
use tari_comms::{peer_manager::NodeId, Minimized, PeerConnection};

use crate::{
    network_discovery::{
        error::NetworkDiscoveryError,
        state_machine::{DhtNetworkDiscoveryRoundInfo, DiscoveryPhase, NetworkDiscoveryContext, StateEvent},
    },
    peer_validator::PeerValidator,
    proto::rpc::GetPeersRequest,
    rpc::{DhtClient, UnvalidatedPeerInfo},
    DhtConfig,
};

// Use a reasonable value based on the existing configuration
const DHT_RPC_MAX_PEERS_PER_REQUEST: u32 = 500;

// Define a timeout for individual stream items
const STREAM_ITEM_TIMEOUT: Duration = Duration::from_secs(10);

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
            phase: DiscoveryPhase::SeedStrap,
            round_number: None, // Will be updated in discover_peers_via_seeds
            total_rounds: None, // Will be updated in discover_peers_via_seeds
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
                    "SeedStrap: Round info at completion - new_peers: {}, duplicate_peers: {}, succeeded: {}, sync_peers_contacted: {}",
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
                    "SeedStrap: Round info at failure - new_peers: {}, duplicate_peers: {}, succeeded: {}, sync_peers_contacted: {}",
                    round_info.num_new_peers,
                    round_info.num_duplicate_peers,
                    round_info.num_succeeded,
                    round_info.sync_peers.len()
                );
                // Return DiscoveryComplete even on error, but with stats reflecting failure
                StateEvent::DiscoveryComplete(round_info)
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn discover_peers_via_seeds(
        &mut self,
        round_info: &mut DhtNetworkDiscoveryRoundInfo,
    ) -> Result<usize, NetworkDiscoveryError> {
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

        let seed_node_ids_set: HashSet<NodeId> = seed_peers_available.iter().map(|p| p.node_id.clone()).collect();
        let mut total_peers_added_this_round = 0;
        let mut total_duplicates_this_round = 0;
        let mut attempted_seed_contacts = 0;
        let mut successful_seed_contacts = 0usize;

        let num_seeds_to_try = cmp::min(
            seed_peers_available.len(),
            self.context.config.network_discovery.max_seed_peer_sync_count,
        );

        // Update round info with total rounds
        round_info.total_rounds = Some(num_seeds_to_try.max(1)); // Ensure at least 1, even if num_seeds_to_try is 0 to avoid div by zero display

        let selected_seed_peers_for_sync = {
            // Create the RNG and use it immediately within this scope
            let mut rng = rand::thread_rng();
            seed_peers_available
                .into_iter()
                .choose_multiple(&mut rng, num_seeds_to_try)
        };

        round_info.sync_peers = selected_seed_peers_for_sync.iter().map(|p| p.node_id.clone()).collect();
        debug!(
            target: LOG_TARGET,
            "SeedStrap: Preparing to sync from up to {} seed peers. Selected peer IDs for this round: {:?}",
            num_seeds_to_try,
            round_info.sync_peers
        );

        let validator = PeerValidator::new(&self.context.config);

        for (idx, seed_peer_candidate) in selected_seed_peers_for_sync.into_iter().enumerate() {
            attempted_seed_contacts += 1;
            // Update round info with current round number
            round_info.round_number = Some(idx + 1); // 1-based round numbers
            let seed_peer_node_id_str = seed_peer_candidate.node_id.to_string();

            if self.context.node_identity.node_id() == &seed_peer_candidate.node_id {
                info!(
                    target: LOG_TARGET,
                    "SeedStrap: Iteration {}/{}: Skipping self as seed peer candidate (node_id: {}).",
                    idx + 1,
                    num_seeds_to_try,
                    seed_peer_node_id_str
                );
                continue;
            }

            debug!(
                target: LOG_TARGET,
                "SeedStrap: Iteration {}/{}: Attempting to connect to seed peer '{}' to get their peer list",
                idx + 1,
                num_seeds_to_try,
                seed_peer_node_id_str
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
                        "SeedStrap: Iteration {}/{}: Failed to dial seed peer '{}': {}. Continuing to next seed candidate.",
                        idx + 1,
                        num_seeds_to_try,
                        seed_peer_node_id_str,
                        e
                    );
                    continue;
                },
            };

            debug!(
                target: LOG_TARGET,
                "SeedStrap: Connected to seed peer '{}'. Requesting peer list.",
                seed_peer_node_id_str
            );

            let peers_from_seed = match self.fetch_peers_from_connection(&mut conn).await {
                Ok(peers) => peers,
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Iteration {}/{}: Failed to fetch peers from seed peer '{}': {}. Disconnecting and continuing.",
                        idx + 1,
                        num_seeds_to_try,
                        seed_peer_node_id_str,
                        e
                    );
                    // Attempt to disconnect, log if it fails but continue
                    if let Err(disc_err) = conn.disconnect(Minimized::Yes).await {
                        warn!(target: LOG_TARGET, "SeedStrap: Also failed to disconnect from seed peer '{}' after fetch failure: {}", seed_peer_node_id_str, disc_err);
                    }
                    continue;
                },
            };

            if peers_from_seed.is_empty() {
                info!(
                    target: LOG_TARGET,
                    "SeedStrap: Iteration {}/{}: Seed peer '{}' returned an empty peer list. Disconnecting.",
                    idx + 1,
                    num_seeds_to_try,
                    seed_peer_node_id_str
                );
                if let Err(e) = conn.disconnect(Minimized::Yes).await {
                    warn!(target: LOG_TARGET, "SeedStrap: Failed to disconnect from seed peer '{}' after receiving empty list: {}", seed_peer_node_id_str, e);
                }
                continue;
            }

            // This seed successfully provided peers
            round_info.num_succeeded += 1;
            successful_seed_contacts += 1;

            debug!(
                target: LOG_TARGET,
                "SeedStrap: Successfully fetched {} peer entries from seed peer '{}'. Disconnecting before processing.",
                peers_from_seed.len(),
                seed_peer_node_id_str
            );

            if let Err(e) = conn.disconnect(Minimized::Yes).await {
                warn!(target: LOG_TARGET, "SeedStrap: Failed to disconnect from seed peer '{}': {}", seed_peer_node_id_str, e);
            } else {
                debug!(target: LOG_TARGET, "SeedStrap: Successfully disconnected from seed peer '{}'", seed_peer_node_id_str);
            }

            let mut new_peers_this_seed = 0;
            let mut duplicates_this_seed = 0;

            let peers_count = peers_from_seed.len();
            debug!(
                target: LOG_TARGET,
                "SeedStrap: Iteration {}/{}: Beginning to process {} peers from seed peer '{}'",
                idx + 1,
                num_seeds_to_try,
                peers_count,
                seed_peer_node_id_str
            );

            for (peer_idx_loop, peer_info_proto) in peers_from_seed.into_iter().enumerate() {
                let new_peer_candidate: UnvalidatedPeerInfo = match peer_info_proto.try_into() {
                    Ok(p) => p,
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Invalid peer data received, skipping: {}",
                            seed_peer_node_id_str,
                            peer_idx_loop + 1,
                            peers_count,
                            e
                        );
                        continue; // Skip this invalid entry
                    },
                };

                let candidate_node_id = NodeId::from_public_key(&new_peer_candidate.public_key);
                trace!(
                    target: LOG_TARGET,
                    "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Processing candidate NodeId = {}",
                    seed_peer_node_id_str,
                    peer_idx_loop + 1,
                    peers_count,
                    candidate_node_id
                );

                if new_peer_candidate.public_key == *self.context.node_identity.public_key() {
                    trace!(target: LOG_TARGET, "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Skipping self.", seed_peer_node_id_str, peer_idx_loop+1, peers_count);
                    continue;
                }

                if seed_node_ids_set.contains(&candidate_node_id) {
                    trace!(
                        target: LOG_TARGET,
                        "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Skipping known seed peer {}.",
                        seed_peer_node_id_str,
                        peer_idx_loop + 1,
                        peers_count,
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
                            "SeedStrap: (Seed '{}', Peer Candidate {}/{}): Error searching for existing peer candidate {} by public key: {}. Skipping.",
                            seed_peer_node_id_str, peer_idx_loop + 1, peers_count, candidate_node_id, e
                        );
                        continue;
                    },
                };

                let is_new_peer = maybe_existing_peer.is_none();

                match validator.validate_peer(new_peer_candidate, maybe_existing_peer) {
                    Ok(valid_peer) => {
                        debug!(
                            target: LOG_TARGET,
                            "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Peer {} from seed '{}' is valid. New: {}. Adding to peer manager.",
                            seed_peer_node_id_str,
                            peer_idx_loop + 1,
                            peers_count,
                            valid_peer.node_id,
                            seed_peer_node_id_str,
                            is_new_peer
                        );

                        match self.context.peer_manager.add_or_update_peer(valid_peer).await {
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
                                    "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Failed to add validated peer {}: {}",
                                    seed_peer_node_id_str,
                                    peer_idx_loop + 1,
                                    peers_count,
                                    candidate_node_id,
                                    e
                                );
                            },
                        }
                    },
                    Err(e) => {
                        warn!(
                            target: LOG_TARGET,
                            "SeedStrap: (From Seed '{}', Peer Candidate {}/{}): Invalid peer data for {} received from seed peer '{}': {}",
                            seed_peer_node_id_str,
                            peer_idx_loop + 1,
                            peers_count,
                            candidate_node_id,
                            seed_peer_node_id_str,
                            e
                        );
                    },
                }
            }

            info!(
                target: LOG_TARGET,
                "SeedStrap: Iteration {}/{}: Finished processing peers from seed '{}'. New peers from this seed: {}. Duplicates from this seed: {}.",
                idx + 1,
                num_seeds_to_try,
                seed_peer_node_id_str,
                new_peers_this_seed,
                duplicates_this_seed
            );
            total_peers_added_this_round += new_peers_this_seed;
            total_duplicates_this_round += duplicates_this_seed;

            // EARLY EXIT CONDITION: if min_desired_peers AND max_peers_to_sync_per_round are met
            // after at least one successful contact.
            if successful_seed_contacts > 0 && // Ensure we've actually talked to at least one seed successfully
               total_peers_added_this_round >= self.context.config.network_discovery.min_desired_peers &&
               total_peers_added_this_round >= self.context.config.network_discovery.max_peers_to_sync_per_round.try_into().unwrap_or(usize::MAX)
            {
                info!(
                    target: LOG_TARGET,
                    "SeedStrap: Early exit: Found sufficient peers. Total new peers ({}) >= min_desired_peers ({}) AND >= max_peers_to_sync_per_round ({}). Exiting seed node loop after {} successful sync(s). Signaling round as complete.",
                    total_peers_added_this_round,
                    self.context.config.network_discovery.min_desired_peers,
                    self.context.config.network_discovery.max_peers_to_sync_per_round,
                    successful_seed_contacts
                );
                // If we early exit because we found enough peers, make the round_number reflect completion
                // of the seed strap phase.
                round_info.round_number = round_info.total_rounds;
                break; // Exit the loop over seed peers
            }

            // Additional early exit conditions:
            // 1. The soft limit for total peers needed must be enabled (> 0).
            // 2. The total number of peers added in this round must meet or exceed this soft limit.
            // 3. The number of successfully contacted seed peers must meet or exceed the configured minimum.
            let soft_peer_limit_enabled = self
                .context
                .config
                .network_discovery
                .seed_peer_min_initial_sync_peers_needed >
                0;
            let enough_total_peers_added = total_peers_added_this_round >=
                self.context
                    .config
                    .network_discovery
                    .seed_peer_min_initial_sync_peers_needed;
            let enough_successful_seed_contacts = successful_seed_contacts >=
                self.context
                    .config
                    .network_discovery
                    .min_successful_seed_contacts_for_early_exit;

            if soft_peer_limit_enabled && enough_total_peers_added && enough_successful_seed_contacts {
                info!(
                    target: LOG_TARGET,
                    "SeedStrap: Original early exit conditions met. Total peers added ({}) >= needed ({}). Successful seed contacts ({}) >= min ({}). Exiting seed node loop.",
                    total_peers_added_this_round,
                    self.context.config.network_discovery.seed_peer_min_initial_sync_peers_needed,
                    successful_seed_contacts,
                    self.context.config.network_discovery.min_successful_seed_contacts_for_early_exit
                );
                // If we early exit because we found enough peers, make the round_number reflect completion
                // of the seed strap phase.
                round_info.round_number = round_info.total_rounds;
                break;
            }
        }

        round_info.num_duplicate_peers = total_duplicates_this_round;

        info!(
            target: LOG_TARGET,
            "SeedStrap: discover_peers_via_seeds finished. Attempted to contact: {}/{}. Successfully synced from: {}. Total new peers added in this round: {}. Total duplicates processed in this round: {}.",
            attempted_seed_contacts,
            num_seeds_to_try,
            round_info.num_succeeded,
            total_peers_added_this_round,
            total_duplicates_this_round
        );

        // Since count() returns usize directly, not a Result
        let total_peer_db_size = self.context.peer_manager.count().await;

        // For all(), we need to check if it returns a Result
        let non_seed_peers_from_db_count = match self.context.peer_manager.all(None).await {
            Ok(all_peers) => all_peers
                .iter()
                .filter(|p| !seed_node_ids_set.contains(&p.node_id))
                .count(),
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "SeedStrap: Error getting all peers after seed bootstrap: {}",
                    e
                );
                0 // Default to 0 if we can't get the peers
            },
        };

        info!(
            target: LOG_TARGET,
            "SeedStrap: Peer DB counts after seed bootstrap completion: Total peers in DB = {}, Non-seed peers in DB = {}",
            total_peer_db_size,
            non_seed_peers_from_db_count
        );

        Ok(total_peers_added_this_round)
    }

    async fn fetch_peers_from_connection(
        &self,
        conn: &mut PeerConnection,
    ) -> Result<Vec<crate::proto::rpc::PeerInfo>, NetworkDiscoveryError> {
        debug!(
            target: LOG_TARGET,
            "SeedStrap: Beginning RPC client connection to seed peer '{}'",
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
                    "SeedStrap: Failed to connect RPC client to seed peer {}: {}",
                    conn.peer_node_id(),
                    e
                );
                return Err(e.into());
            },
        };

        let base = cmp::min(
            self.config().network_discovery.max_peers_to_sync_per_round,
            DHT_RPC_MAX_PEERS_PER_REQUEST,
        );
        // Ask for at most half the configured value but never zero
        let num_peers_to_request = (base / 2).max(1);

        let req = GetPeersRequest {
            n: num_peers_to_request,
            include_clients: false,
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
                    "SeedStrap: Failed to initiate get_peers stream from seed peer '{}': {}. This seed peer will be skipped.",
                    conn.peer_node_id(),
                    e
                );
                return Err(e.into());
            },
        };

        let seed_node_id_str = conn.peer_node_id().to_string(); // Used for logging
        let peers_from_seed = self.collect_peer_stream(&seed_node_id_str, &mut peer_stream).await?;

        debug!(
            target: LOG_TARGET,
            "SeedStrap: fetch_peers_from_connection for seed '{}' is returning {} peer entries.",
            conn.peer_node_id(),
            peers_from_seed.len()
        );
        Ok(peers_from_seed)
    }

    async fn collect_peer_stream<S>(
        &self,
        seed_node_id_str: &str,
        peer_stream: &mut S,
    ) -> Result<Vec<crate::proto::rpc::PeerInfo>, NetworkDiscoveryError>
    where
        S: StreamExt<Item = Result<crate::proto::rpc::GetPeersResponse, tari_comms::protocol::rpc::RpcStatus>> + Unpin,
    {
        let mut peers_from_seed = Vec::new();
        let mut stream_items_processed_total = 0; // Total items received from stream
        let mut stream_items_with_peers = 0; // Items that actually contained peer data

        debug!(
            target: LOG_TARGET,
            "SeedStrap: Beginning to collect peer stream items from seed '{}'", seed_node_id_str
        );

        loop {
            debug!(
                target: LOG_TARGET,
                "SeedStrap: Attempting to get next peer from stream for seed '{}'. Processed {} items so far ({} with peers).",
                seed_node_id_str,
                stream_items_processed_total,
                stream_items_with_peers
            );

            // Add timeout to prevent hanging indefinitely on a stalled stream
            match tokio::time::timeout(STREAM_ITEM_TIMEOUT, peer_stream.next()).await {
                // Timeout occurred while waiting for the next item
                Err(_) => {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Timeout after {:.2?} waiting for next peer from stream for seed '{}'. Breaking from peer stream collection.",
                        STREAM_ITEM_TIMEOUT,
                        seed_node_id_str
                    );
                    // Treat as if the stream ended, but it was due to a timeout
                    // This allows the outer loop to try another seed peer
                    break;
                },
                // Got an item from the stream (or stream ended)
                Ok(item_result) => {
                    match item_result {
                        Some(Ok(crate::proto::rpc::GetPeersResponse { peer })) => {
                            stream_items_processed_total += 1;
                            if let Some(peer_info_proto) = peer {
                                debug!(
                                    target: LOG_TARGET,
                                    "SeedStrap: Stream item #{} (peer item #{}) from seed '{}' contains a peer",
                                    stream_items_processed_total,
                                    stream_items_with_peers + 1, // +1 because this one is a peer
                                    seed_node_id_str
                                );
                                peers_from_seed.push(peer_info_proto);
                                stream_items_with_peers += 1;
                            } else {
                                debug!(
                                    target: LOG_TARGET,
                                    "SeedStrap: Stream item #{} from seed '{}' contains an empty (None) GetPeersResponse.peer field.",
                                    stream_items_processed_total,
                                    seed_node_id_str
                                );
                            }
                        },
                        Some(Err(e)) => {
                            stream_items_processed_total += 1;
                            warn!(
                                target: LOG_TARGET,
                                "SeedStrap: Error in stream item #{} from seed '{}': {}. Breaking from peer stream collection.",
                                stream_items_processed_total,
                                seed_node_id_str,
                                e
                            );
                            debug!(
                                target: LOG_TARGET,
                                "SeedStrap: Exiting collect_peer_stream for seed '{}' early due to RPC stream error. Peers collected so far: {}. Total items processed from stream: {}.",
                                seed_node_id_str,
                                peers_from_seed.len(),
                                stream_items_processed_total
                            );
                            return Err(e.into());
                        },
                        None => {
                            debug!(
                                target: LOG_TARGET,
                                "SeedStrap: Peer stream ended for seed '{}'. Processed {} items in total, {} of which contained peers.",
                                seed_node_id_str,
                                stream_items_processed_total,
                                stream_items_with_peers
                            );
                            break; // Stream ended gracefully
                        },
                    }
                },
            }
        }

        info!(
            target: LOG_TARGET,
            "SeedStrap: Received {} total peer entries from seed peer '{}' (in {} stream items, {} containing actual peers).",
            peers_from_seed.len(),
            seed_node_id_str,
            stream_items_processed_total,
            stream_items_with_peers,
        );

        Ok(peers_from_seed)
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
