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
use futures_util::stream::FuturesUnordered;
use log::*;
use rand::prelude::SliceRandom;
use tari_comms::{
    peer_manager::{NodeId, Peer},
    Minimized,
    PeerConnection,
};

use crate::{
    network_discovery::{
        error::NetworkDiscoveryError,
        state_machine::{DhtNetworkDiscoveryRoundInfo, DiscoveryPhase, NetworkDiscoveryContext, StateEvent},
    },
    peer_validator::PeerValidator,
    proto::rpc::{GetPeersRequest, PeerInfo},
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

        let mut seed_peers = seed_peers_available.clone();
        seed_peers.shuffle(&mut rand::thread_rng());
        let mut seed_peers_iter = seed_peers.iter();

        let max_peers_to_sync_per_round = self.config().network_discovery.max_peers_to_sync_per_round;
        let max_permitted_peer_claims = self.config().max_permitted_peer_claims;
        let max_permitted_peer_addresses_per_claim = self
            .config()
            .peer_validator_config
            .max_permitted_peer_addresses_per_claim;

        // Select the permitted concurrent number of seed peers to try
        let candidates: Vec<Peer> = seed_peers_iter.by_ref().take(num_seeds_to_try).cloned().collect();
        let num_seeds_this_round = candidates.len();
        round_info.sync_peers = candidates.iter().map(|p| p.node_id.clone()).collect();
        debug!(
            target: LOG_TARGET,
            "SeedStrap: Preparing to sync from up to {} seed peers. Selected peer IDs for this round: {:?}",
            num_seeds_this_round,
            round_info.sync_peers,
        );

        // Get peers from seeds concurrently
        let mut task_stream = FuturesUnordered::new();
        for (idx, seed_peer_candidate) in candidates.into_iter().enumerate() {
            attempted_seed_contacts += 1;
            let seed_node_ids_set_clone = seed_node_ids_set.clone();
            let context_clone = self.context.clone();
            let handle = tokio::task::spawn(async move {
                get_peers(
                    context_clone,
                    seed_peer_candidate,
                    num_seeds_this_round,
                    idx,
                    &seed_node_ids_set_clone,
                    max_peers_to_sync_per_round,
                    max_permitted_peer_claims,
                    max_permitted_peer_addresses_per_claim,
                )
                .await
            });
            task_stream.push(handle);
        }

        while let Some(result) = task_stream.next().await {
            let (peers_from_seed, new_peers_this_seed, duplicates_this_seed, spawn_another_task) = match result {
                Ok((peers, n_new, n_dup)) => (peers, n_new, n_dup, false),
                Err(e) => {
                    debug!(target: LOG_TARGET, "SeedStrap: get_peers task unsuccessful, starting a new one: {}", e);
                    (Vec::new(), 0, 0, true)
                },
            };

            if spawn_another_task || peers_from_seed.is_empty() {
                // Add a new task to the stream if the previous one failed
                if let Some(seed_peer_candidate) = seed_peers_iter.next().cloned() {
                    attempted_seed_contacts += 1;
                    let seed_node_ids_set_clone = seed_node_ids_set.clone();
                    let context_clone = self.context.clone();
                    let handle = tokio::task::spawn(async move {
                        get_peers(
                            context_clone,
                            seed_peer_candidate,
                            num_seeds_this_round,
                            1,
                            &seed_node_ids_set_clone,
                            max_peers_to_sync_per_round,
                            max_permitted_peer_claims,
                            max_permitted_peer_addresses_per_claim,
                        )
                        .await
                    });
                    task_stream.push(handle);
                }
                continue;
            } else {
                // This seed successfully provided peers
                round_info.num_succeeded += 1;
                successful_seed_contacts += 1;
            }

            total_peers_added_this_round += new_peers_this_seed;
            total_duplicates_this_round += duplicates_this_seed;

            // Exit condition
            if self.early_exit_conditions_met(total_peers_added_this_round, successful_seed_contacts) {
                break;
            }
        }

        round_info.num_duplicate_peers = total_duplicates_this_round;

        info!(
            target: LOG_TARGET,
            "SeedStrap: discover_peers_via_seeds finished. Attempted to contact: {}/{}. Successfully synced from: {}. \
            Total new peers added in this round: {}. Total duplicates processed in this round: {}.",
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

    fn early_exit_conditions_met(&self, total_peers_added_this_round: usize, successful_seed_contacts: usize) -> bool {
        // EARLY EXIT CONDITION: if min_desired_peers AND max_peers_to_sync_per_round are met
        // after at least one successful contact.
        if successful_seed_contacts > 0 && // Ensure we've actually talked to at least one seed successfully
            total_peers_added_this_round >= self.context.config.network_discovery.min_desired_peers &&
            total_peers_added_this_round >= self.context.config.network_discovery.max_peers_to_sync_per_round.try_into().unwrap_or(usize::MAX)
        {
            info!(
                target: LOG_TARGET,
                "SeedStrap: Early exit: Found sufficient peers. Total new peers ({}) >= min_desired_peers ({}) \
                AND >= max_peers_to_sync_per_round ({}). Exiting seed node loop after {} successful sync(s). \
                Signaling round as complete.",
                total_peers_added_this_round,
                self.context.config.network_discovery.min_desired_peers,
                self.context.config.network_discovery.max_peers_to_sync_per_round,
                successful_seed_contacts
            );
            return true;
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
                "SeedStrap: Original early exit conditions met. Total peers added ({}) >= needed ({}). Successful \
                seed contacts ({}) >= min ({}). Exiting seed node loop.",
                total_peers_added_this_round,
                self.context.config.network_discovery.seed_peer_min_initial_sync_peers_needed,
                successful_seed_contacts,
                self.context.config.network_discovery.min_successful_seed_contacts_for_early_exit
            );
            return true;
        }

        false
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}

#[allow(clippy::too_many_lines)]
async fn get_peers(
    context: NetworkDiscoveryContext,
    seed_peer_candidate: Peer,
    num_seeds_this_round: usize,
    idx: usize,
    seed_node_ids_set: &HashSet<NodeId>,
    max_peers_to_sync_per_round: u32,
    max_permitted_peer_claims: usize,
    max_permitted_peer_addresses_per_claim: usize,
) -> (Vec<PeerInfo>, usize, usize) {
    let seed_peer_node_id_str = seed_peer_candidate.node_id.to_string();

    if context.node_identity.node_id() == &seed_peer_candidate.node_id {
        info!(
            target: LOG_TARGET,
            "SeedStrap: Attempt {}/{}: Skipping self as seed peer candidate (node_id: {}).",
            idx + 1,
            num_seeds_this_round,
            seed_peer_node_id_str
        );
        return (vec![], 0, 0);
    }

    debug!(
        target: LOG_TARGET,
        "SeedStrap: Attempt {}/{}: Attempting to connect to seed peer '{}' to get their peer list",
        idx + 1,
        num_seeds_this_round,
        seed_peer_node_id_str
    );

    const NUM_RETRIES: usize = 3;
    let mut peers_from_seed = vec![];
    for attempt in 1..=NUM_RETRIES {
        let mut conn = match context
            .connectivity
            .dial_peer(seed_peer_candidate.node_id.clone())
            .await
        {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "SeedStrap: Attempt {}/{}: Failed to dial seed peer '{}': {}.",
                    idx + 1,
                    num_seeds_this_round,
                    seed_peer_node_id_str,
                    e
                );
                return (vec![], 0, 0);
            },
        };

        debug!(
            target: LOG_TARGET,
            "SeedStrap: Connected to seed peer '{}'. Requesting peer list. Try {} of {}.",
            seed_peer_node_id_str, attempt, NUM_RETRIES,
        );

        match fetch_peers_from_connection(
            &mut conn,
            max_peers_to_sync_per_round,
            max_permitted_peer_claims,
            max_permitted_peer_addresses_per_claim,
        )
        .await
        {
            Ok(peers) => {
                if peers.is_empty() && !conn.is_connected() && attempt < NUM_RETRIES {
                    debug!(
                        target: LOG_TARGET,
                        "SeedStrap: Connection to seed peer '{}' lost on try {}. Will retry.",
                        seed_peer_node_id_str, attempt,
                    );
                    continue;
                }
                if let Err(e) = conn.disconnect(Minimized::Yes, "SeedStrap disconnect seed on Ok").await {
                    warn!(target: LOG_TARGET, "SeedStrap: Failed to disconnect from seed peer '{}': {}", seed_peer_node_id_str, e);
                } else {
                    debug!(target: LOG_TARGET, "SeedStrap: Successfully disconnected from seed peer '{}'", seed_peer_node_id_str);
                }
                peers_from_seed = peers;
                break;
            },
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "SeedStrap: Attempt {}/{}: Failed to fetch peers from seed peer '{}': {}. Disconnecting and continuing.",
                    idx + 1,
                    num_seeds_this_round,
                    seed_peer_node_id_str,
                    e
                );
                // Attempt to disconnect, log if it fails but continue
                if let Err(disc_err) = conn
                    .disconnect(Minimized::Yes, "SeedStrap disconnect seed on Error")
                    .await
                {
                    warn!(
                        target: LOG_TARGET,
                        "SeedStrap: Also failed to disconnect from seed peer '{}' after fetch failure: {}",
                        seed_peer_node_id_str, disc_err
                    );
                }
                return (vec![], 0, 0);
            },
        };
    }

    if peers_from_seed.is_empty() {
        info!(
            target: LOG_TARGET,
            "SeedStrap: Attempt {}/{}: Seed peer '{}' returned an empty peer list. Disconnecting.",
            idx + 1,
            num_seeds_this_round,
            seed_peer_node_id_str
        );
        return (vec![], 0, 0);
    }

    debug!(
        target: LOG_TARGET,
        "SeedStrap: Successfully fetched {} peer entries from seed peer '{}'. Disconnecting before processing.",
        peers_from_seed.len(),
        seed_peer_node_id_str
    );

    let mut new_peers_this_seed = 0;
    let mut duplicates_this_seed = 0;

    let peers_count = peers_from_seed.len();
    debug!(
        target: LOG_TARGET,
        "SeedStrap: Attempt {}/{}: Beginning to process {} peers from seed peer '{}'",
        idx + 1,
        num_seeds_this_round,
        peers_count,
        seed_peer_node_id_str
    );

    for (peer_idx_loop, peer_info_proto) in peers_from_seed.iter().enumerate() {
        let new_peer_candidate: UnvalidatedPeerInfo = match peer_info_proto.clone().try_into() {
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

        if new_peer_candidate.public_key == *context.node_identity.public_key() {
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

        let maybe_existing_peer = match context
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

        let validator = PeerValidator::new(&context.config);
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

                match context.peer_manager.add_or_update_peer(valid_peer).await {
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
        "SeedStrap: Attempt {}/{}: Finished processing peers from seed '{}'. New peers from this seed: {}. \
        Duplicates from this seed: {}.",
        idx + 1,
        num_seeds_this_round,
        seed_peer_node_id_str,
        new_peers_this_seed,
        duplicates_this_seed
    );

    (peers_from_seed, new_peers_this_seed, duplicates_this_seed)
}

async fn fetch_peers_from_connection(
    conn: &mut PeerConnection,
    max_peers_to_sync_per_round: u32,
    max_permitted_peer_claims: usize,
    max_permitted_peer_addresses_per_claim: usize,
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

    let base = cmp::min(max_peers_to_sync_per_round, DHT_RPC_MAX_PEERS_PER_REQUEST);
    // Ask for at most half the configured value but never zero
    let num_peers_to_request = (base / 2).max(1);

    let req = GetPeersRequest {
        n: num_peers_to_request,
        include_clients: false,
        max_claims: max_permitted_peer_claims.try_into().unwrap_or(u32::MAX),
        max_addresses_per_claim: max_permitted_peer_addresses_per_claim.try_into().unwrap_or(u32::MAX),
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
    let peers_from_seed = collect_peer_stream(&seed_node_id_str, &mut peer_stream).await?;

    debug!(
        target: LOG_TARGET,
        "SeedStrap: fetch_peers_from_connection for seed '{}' is returning {} peer entries.",
        conn.peer_node_id(),
        peers_from_seed.len()
    );
    Ok(peers_from_seed)
}

async fn collect_peer_stream<S>(
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
                    "SeedStrap: Timeout after {:.2?} waiting for next peer from stream for seed '{}'. Breaking from \
                    peer stream collection.",
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
                                "SeedStrap: Stream item #{} from seed '{}' contains an empty (None) \
                                GetPeersResponse.peer field.",
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
                            "SeedStrap: Exiting collect_peer_stream for seed '{}' early due to RPC stream error. Peers \
                            collected so far: {}. Total items processed from stream: {}.",
                            seed_node_id_str,
                            peers_from_seed.len(),
                            stream_items_processed_total
                        );
                        return Err(e.into());
                    },
                    None => {
                        debug!(
                            target: LOG_TARGET,
                            "SeedStrap: Peer stream ended for seed '{}'. Processed {} items in total, {} of which \
                            contained peers.",
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
