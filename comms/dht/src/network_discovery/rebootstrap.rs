//  Copyright 2026, The Tari Project
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

//! # Rebootstrap
//!
//! Refreshes this node's peer knowledge when DhtConnectivity reports that the peer pool cannot be filled through
//! our own outbound dials (`DhtEvent::PoolStarved`). Seeds are re-resolved and synced from, and in parallel the
//! peers that are still connected (inbound or outbound) are asked for their peer lists.

use std::{
    collections::HashSet,
    fmt,
    fmt::{Display, Formatter},
    time::Instant,
};

use futures::future;
use log::*;
use rand::prelude::SliceRandom;
use tari_comms::{
    Minimized,
    PeerConnection,
    RefKind,
    peer_manager::{NodeId, Peer, PeerFlags},
};
use tokio::time;

use crate::{
    network_discovery::{
        NetworkDiscoveryError,
        discovering::is_connected,
        seed_strap::fetch_peers_from_connection,
        state_machine::{NetworkDiscoveryContext, StateEvent},
    },
    peer_validator::PeerValidator,
    proto::rpc::PeerInfo,
    rpc::UnvalidatedPeerInfo,
};

/// Fixed log target for everything to do with rebootstrapping (the trigger, the rebootstrap itself and the pool
/// refill afterwards), so that the whole sequence can be found with a single query.
pub const REBOOTSTRAP_LOG_TARGET: &str = "comms::dht::rebootstrap";

/// Supplies this node's seed peers on demand. The DHT stays DNS-agnostic: the p2p layer injects an implementation
/// that resolves the configured `peer_seeds` and `dns_seeds`, and a rebootstrap calls it to pick up seeds that have
/// changed since start-up.
#[tari_comms::async_trait]
pub trait SeedPeerProvider: Send + Sync + 'static {
    /// Resolve the current set of seed peers. Errors are logged and swallowed by the implementation; an empty list
    /// means no seeds could be resolved.
    async fn resolve_seed_peers(&self) -> Vec<Peer>;
}

impl fmt::Debug for dyn SeedPeerProvider {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "SeedPeerProvider")
    }
}

/// The result of a rebootstrap.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RebootstrapInfo {
    /// Number of seed peers returned by the `SeedPeerProvider`.
    pub seeds_resolved: usize,
    /// Number of seeds peers were successfully synced from.
    pub seeds_synced: usize,
    /// Number of connected peers peers were successfully synced from.
    pub connected_peers_synced: usize,
    /// Number of valid peers received from seeds.
    pub num_from_seeds: usize,
    /// Number of valid peers received from connected peers.
    pub num_from_connected: usize,
    /// The distinct peers learned in this rebootstrap. DhtConnectivity prefers these when refilling its pool.
    pub learned_peers: Vec<NodeId>,
}

impl Display for RebootstrapInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "seeds resolved = {}, seeds synced = {} ({} peer(s)), connected peers synced = {} ({} peer(s)), distinct \
             peers learned = {}",
            self.seeds_resolved,
            self.seeds_synced,
            self.num_from_seeds,
            self.connected_peers_synced,
            self.num_from_connected,
            self.learned_peers.len()
        )
    }
}

#[derive(Debug)]
pub(super) struct Rebootstrap {
    context: NetworkDiscoveryContext,
}

impl Rebootstrap {
    pub fn new(context: NetworkDiscoveryContext) -> Self {
        Self { context }
    }

    pub async fn next_event(&mut self) -> StateEvent {
        let config = &self.context.config.network_discovery;
        let started = Instant::now();
        info!(
            target: REBOOTSTRAP_LOG_TARGET,
            "Rebootstrap started: re-resolving seeds and syncing peers from up to {} seed(s) and up to {} connected \
             peer(s)",
            config.max_seed_peer_sync_count,
            config.rebootstrap_connected_peers,
        );

        // The seed sync has to dial (30-60s over Tor) while the connected-peer sync does not, so they run side by
        // side rather than one after the other. Each has its own timeout, so a slow seed sync cannot throw away what
        // the connected peers returned.
        let timeout = config.bootstrap_timeout;
        let seeds = async {
            time::timeout(timeout, sync_from_seeds(&self.context))
                .await
                .unwrap_or_else(|_| {
                    // Whatever was learned before the timeout is already in the peer DB
                    warn!(target: REBOOTSTRAP_LOG_TARGET, "Rebootstrap seed sync timed out after {timeout:.0?}");
                    (0, 0, Vec::new())
                })
        };
        let connected = async {
            time::timeout(timeout, sync_from_connected_peers(&self.context))
                .await
                .unwrap_or_else(|_| {
                    warn!(
                        target: REBOOTSTRAP_LOG_TARGET,
                        "Rebootstrap connected-peer sync timed out after {timeout:.0?}"
                    );
                    (0, Vec::new())
                })
        };
        let ((seeds_resolved, seeds_synced, from_seeds), (connected_peers_synced, from_connected)) =
            future::join(seeds, connected).await;

        let mut seen = HashSet::new();
        let learned_peers = from_seeds
            .iter()
            .chain(from_connected.iter())
            .filter(|node_id| seen.insert((*node_id).clone()))
            .cloned()
            .collect();
        let info = RebootstrapInfo {
            seeds_resolved,
            seeds_synced,
            connected_peers_synced,
            num_from_seeds: from_seeds.len(),
            num_from_connected: from_connected.len(),
            learned_peers,
        };
        info!(
            target: REBOOTSTRAP_LOG_TARGET,
            "Rebootstrap finished in {:.1?}: {info}",
            started.elapsed()
        );
        StateEvent::RebootstrapComplete(info)
    }
}

/// Re-resolves the seeds, then syncs from up to `max_seed_peer_sync_count` of them. Returns the number of seeds
/// resolved, the number of seeds synced from and the peers learned.
async fn sync_from_seeds(context: &NetworkDiscoveryContext) -> (usize, usize, Vec<NodeId>) {
    let seeds_resolved = refresh_seed_peers(context).await;

    let mut seeds = match context.peer_manager.get_seed_peers().await {
        Ok(seeds) => seeds,
        Err(err) => {
            warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to load seed peers: {err}");
            Vec::new()
        },
    };
    // Real bans are honoured. The connectivity manager would refuse the dial anyway; this just avoids spending one of
    // the seed slots on it.
    seeds.retain(|seed| !seed.is_banned() && seed.node_id != *context.node_identity.node_id());
    seeds.shuffle(&mut rand::rng());
    seeds.truncate(context.config.network_discovery.max_seed_peer_sync_count);

    let results = future::join_all(seeds.into_iter().map(|seed| sync_from_seed(context, seed))).await;
    let seeds_synced = results.iter().filter(|r| r.is_some()).count();
    let learned = results.into_iter().flatten().flatten().collect();
    (seeds_resolved, seeds_synced, learned)
}

/// Asks the `SeedPeerProvider` for the current seeds and upserts them as seed peers. Returns the number resolved.
async fn refresh_seed_peers(context: &NetworkDiscoveryContext) -> usize {
    let Some(provider) = context.seed_peer_provider.as_ref() else {
        debug!(target: REBOOTSTRAP_LOG_TARGET, "No seed peer provider configured, using stored seeds only");
        return 0;
    };
    let peers = provider.resolve_seed_peers().await;
    let num_resolved = peers.len();
    for mut peer in peers {
        if peer.public_key == *context.node_identity.public_key() {
            continue;
        }
        // Seeds are deliberately not added to the allow-list. Merging keeps any ban already recorded for this peer.
        peer.add_flags(PeerFlags::SEED);
        if let Err(err) = context.peer_manager.add_or_update_peer(peer).await {
            warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to store resolved seed peer: {err}");
        }
    }
    num_resolved
}

/// Dials a seed and syncs peers from it. Returns `None` if the seed could not be synced from.
///
/// Seeds may have failed only because of our own outage, so none of the dial suppression applies here: an explicit
/// dial (one with a reply) is never circuit-broken by the connectivity manager, the dialer tries every address
/// regardless of past address failures, and the DHT pool's per-peer dial backoff only gates pool dials. Bans are
/// still enforced by the connectivity manager.
async fn sync_from_seed(context: &NetworkDiscoveryContext, seed: Peer) -> Option<Vec<NodeId>> {
    // An existing connection to the seed may be this node's only lifeline (e.g. the proactive dialer's), so only a
    // connection this rebootstrap created is hung up afterwards.
    let was_connected = is_connected(&mut context.connectivity.clone(), &seed.node_id).await;
    let dial_timeout = context.config.network_discovery.bootstrap_dial_peer_timeout;
    let dial = time::timeout(
        dial_timeout,
        context.connectivity.dial_peer(seed.node_id.clone(), RefKind::Weak),
    )
    .await;
    let mut conn = match dial {
        Ok(Ok(conn)) => conn,
        Ok(Err(err)) => {
            debug!(
                target: REBOOTSTRAP_LOG_TARGET,
                "Failed to dial seed '{}': {err}",
                seed.node_id.short_str()
            );
            return None;
        },
        Err(_) => {
            debug!(
                target: REBOOTSTRAP_LOG_TARGET,
                "Dial to seed '{}' timed out after {dial_timeout:.0?}",
                seed.node_id.short_str()
            );
            return None;
        },
    };

    let result = sync_from_connection(context, &mut conn).await;
    // Seeds are for bootstrapping, not for holding a pool slot.
    if !was_connected &&
        !conn.is_strongly_held() &&
        let Err(err) = conn.disconnect(Minimized::Yes, "Rebootstrap seed sync complete").await
    {
        debug!(
            target: REBOOTSTRAP_LOG_TARGET,
            "Failed to disconnect seed '{}': {err}",
            seed.node_id.short_str()
        );
    }
    match result {
        Ok(learned) => Some(learned),
        Err(err) => {
            debug!(
                target: REBOOTSTRAP_LOG_TARGET,
                "Failed to sync peers from seed '{}': {err}",
                seed.node_id.short_str()
            );
            None
        },
    }
}

/// Syncs peers from up to `rebootstrap_connected_peers` random connected non-client peers, inbound or outbound.
/// Returns the number of peers synced from and the peers learned.
async fn sync_from_connected_peers(context: &NetworkDiscoveryContext) -> (usize, Vec<NodeId>) {
    let max_peers = context.config.network_discovery.rebootstrap_connected_peers;
    if max_peers == 0 {
        return (0, Vec::new());
    }
    let mut connectivity = context.connectivity.clone();
    let mut conns = match connectivity.get_active_connections().await {
        Ok(conns) => conns,
        Err(err) => {
            warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to get active connections: {err}");
            return (0, Vec::new());
        },
    };
    conns.retain(|conn| conn.is_connected() && !conn.peer_features().is_client());
    conns.shuffle(&mut rand::rng());
    conns.truncate(max_peers);

    let results = future::join_all(conns.into_iter().map(|mut conn| async move {
        let result = sync_from_connection(context, &mut conn).await;
        if let Err(err) = &result {
            debug!(
                target: REBOOTSTRAP_LOG_TARGET,
                "Failed to sync peers from connected peer '{}': {err}",
                conn.peer_node_id().short_str()
            );
        }
        result
    }))
    .await;
    let num_synced = results.iter().filter(|r| r.is_ok()).count();
    let learned = results.into_iter().filter_map(Result::ok).flatten().collect();
    (num_synced, learned)
}

/// Requests peers over an existing connection and stores the valid ones. Returns the node ids of the peers stored.
async fn sync_from_connection(
    context: &NetworkDiscoveryContext,
    conn: &mut PeerConnection,
) -> Result<Vec<NodeId>, NetworkDiscoveryError> {
    let config = &context.config;
    let peers = fetch_peers_from_connection(
        conn,
        config.network_discovery.max_peers_to_sync_per_round,
        config.max_permitted_peer_claims,
        config.peer_validator_config.max_permitted_peer_addresses_per_claim,
        config.network_discovery.bootstrap_rpc_connect_timeout,
        config.network_discovery.bootstrap_rpc_get_peers_stream_timeout,
        config.network_discovery.bootstrap_rpc_streaming_timeout,
    )
    .await?;
    Ok(store_peers(context, conn.peer_node_id(), peers).await)
}

/// Validates and stores peers received from `source`. Returns the node ids of the peers stored.
async fn store_peers(context: &NetworkDiscoveryContext, source: &NodeId, peers: Vec<PeerInfo>) -> Vec<NodeId> {
    let validator = PeerValidator::new(&context.config);
    let mut stored = Vec::new();
    for peer_info in peers {
        let peer = match UnvalidatedPeerInfo::try_from(peer_info) {
            Ok(peer) => peer,
            Err(err) => {
                debug!(target: REBOOTSTRAP_LOG_TARGET, "Invalid peer info from '{source}': {err}");
                continue;
            },
        };
        if peer.public_key == *context.node_identity.public_key() {
            continue;
        }
        let existing = match context.peer_manager.find_by_public_key(&peer.public_key).await {
            Ok(existing) => existing,
            Err(err) => {
                debug!(target: REBOOTSTRAP_LOG_TARGET, "Failed to look up peer from '{source}': {err}");
                continue;
            },
        };
        let valid_peer = match validator.validate_peer(peer, existing) {
            Ok(valid_peer) => valid_peer,
            Err(err) => {
                debug!(target: REBOOTSTRAP_LOG_TARGET, "Invalid peer from '{source}': {err}");
                continue;
            },
        };
        let node_id = valid_peer.node_id.clone();
        match context.peer_manager.add_or_update_peer(valid_peer).await {
            Ok(_) => stored.push(node_id),
            Err(err) => debug!(target: REBOOTSTRAP_LOG_TARGET, "Failed to store peer from '{source}': {err}"),
        }
    }
    stored
}
