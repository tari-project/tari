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
//! peers that are still connected (outbound first, then inbound) are asked for their peer lists.

use std::{
    collections::HashSet,
    fmt,
    fmt::{Display, Formatter},
    time::{Duration, Instant},
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
        discovering::{MAX_HOSTILE_RELAYED_CLAIMS, ban_peer, is_connected, offence_severity},
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

/// The most learned peers a rebootstrap hands to DhtConnectivity. Far more than a pool refill can use (a pool of 12
/// dials at most 36 at once), but small enough that DhtConnectivity can take from the list cheaply.
pub const MAX_LEARNED_PEERS: usize = 200;

/// Each source contributes at most `MAX_LEARNED_PEERS / number of sources` peers, but never fewer than this.
const MIN_PEERS_PER_SOURCE: usize = 10;

/// Peers relayed by inbound connections make up at most this share (1/n) of the learned list. Anyone can dial in, so
/// inbound peers are the source an attacker controls most easily.
const INBOUND_SHARE_DIVISOR: usize = 4;

/// Where a batch of learned peers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceKind {
    Seed,
    Outbound,
    Inbound,
}

impl Display for SourceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SourceKind::Seed => write!(f, "seed"),
            SourceKind::Outbound => write!(f, "outbound"),
            SourceKind::Inbound => write!(f, "inbound"),
        }
    }
}

/// The peers stored from one source, in the order received.
#[derive(Debug, Clone)]
pub(super) struct SourceResult {
    pub node_id: NodeId,
    pub kind: SourceKind,
    pub stored: Vec<NodeId>,
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
        // side rather than one after the other. Each source has its own deadline (see `source_timeout`), so a slow
        // one only loses its own results; this outer bound is a backstop.
        let both = future::join(sync_from_seeds(&self.context), sync_from_connected_peers(&self.context));
        let ((seeds_resolved, mut sources), connected) = match time::timeout(config.bootstrap_timeout, both).await {
            Ok(results) => results,
            Err(_) => {
                // Whatever was learned before the timeout is already in the peer DB
                warn!(
                    target: REBOOTSTRAP_LOG_TARGET,
                    "Rebootstrap timed out after {:.0?}", config.bootstrap_timeout
                );
                ((0, Vec::new()), Vec::new())
            },
        };
        let seeds_synced = sources.len();
        sources.extend(connected);

        let (learned_peers, kept) = interleave_sources(&sources);
        let count = |kind: SourceKind| -> usize {
            sources
                .iter()
                .filter(|source| source.kind == kind)
                .map(|source| source.stored.len())
                .sum()
        };
        let info = RebootstrapInfo {
            seeds_resolved,
            seeds_synced,
            connected_peers_synced: sources.len().saturating_sub(seeds_synced),
            num_from_seeds: count(SourceKind::Seed),
            num_from_connected: count(SourceKind::Outbound).saturating_add(count(SourceKind::Inbound)),
            learned_peers,
        };
        let per_source = sources
            .iter()
            .zip(kept)
            .map(|(source, kept)| {
                format!(
                    "{} {} stored={} kept={kept}",
                    source.node_id.short_str(),
                    source.kind,
                    source.stored.len()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        info!(
            target: REBOOTSTRAP_LOG_TARGET,
            "Rebootstrap finished in {:.1?}: {info}. Sources: [{per_source}]",
            started.elapsed()
        );
        StateEvent::RebootstrapComplete(info)
    }
}

/// The deadline for syncing from any one source, dial included.
fn source_timeout(context: &NetworkDiscoveryContext) -> Duration {
    context
        .config
        .network_discovery
        .bootstrap_timeout
        .checked_div(2)
        .unwrap_or_default()
}

/// Builds the learned list from all sources, taking one peer from each source in turn (seeds, then outbound, then
/// inbound connected peers) so that no single source can fill the list. Each source contributes at most its share of
/// `MAX_LEARNED_PEERS`, and inbound sources together at most `1 / INBOUND_SHARE_DIVISOR` of it. Returns the list and
/// how many peers each source contributed to it.
pub(super) fn interleave_sources(sources: &[SourceResult]) -> (Vec<NodeId>, Vec<usize>) {
    let mut learned = Vec::new();
    let mut kept = vec![0usize; sources.len()];
    if sources.is_empty() {
        return (learned, kept);
    }
    let per_source_cap = MAX_LEARNED_PEERS
        .checked_div(sources.len())
        .unwrap_or(0)
        .max(MIN_PEERS_PER_SOURCE);
    let inbound_cap = MAX_LEARNED_PEERS / INBOUND_SHARE_DIVISOR;
    let mut num_inbound = 0usize;
    let mut seen = HashSet::new();

    let mut order = (0..sources.len()).collect::<Vec<_>>();
    // Stable, so sources of the same kind keep their order
    order.sort_by_key(|&i| match sources.get(i).map(|source| source.kind) {
        Some(SourceKind::Seed) => 0,
        Some(SourceKind::Outbound) => 1,
        _ => 2,
    });

    for position in 0..per_source_cap {
        for &i in &order {
            if learned.len() >= MAX_LEARNED_PEERS {
                return (learned, kept);
            }
            let Some(source) = sources.get(i) else { continue };
            let Some(node_id) = source.stored.get(position) else {
                continue;
            };
            let is_inbound = source.kind == SourceKind::Inbound;
            if is_inbound && num_inbound >= inbound_cap {
                continue;
            }
            if !seen.insert(node_id.clone()) {
                continue;
            }
            learned.push(node_id.clone());
            if let Some(count) = kept.get_mut(i) {
                *count = count.saturating_add(1);
            }
            if is_inbound {
                num_inbound = num_inbound.saturating_add(1);
            }
        }
    }
    (learned, kept)
}

/// Re-resolves the seeds, then syncs from up to `max_seed_peer_sync_count` of them. Returns the number of seeds
/// resolved and a result for each seed synced from.
async fn sync_from_seeds(context: &NetworkDiscoveryContext) -> (usize, Vec<SourceResult>) {
    let resolved = refresh_seed_peers(context).await;
    let seeds_resolved = resolved.as_ref().map_or(0, Vec::len);

    // With a provider, only the seeds of the current resolution are used, so that seeds no longer published (or
    // injected into an earlier resolution) do not linger in the selection. If nothing resolved at all, the stored
    // seeds are better than none.
    let seeds = match resolved {
        Some(node_ids) if !node_ids.is_empty() => context.peer_manager.get_peers_by_node_ids(&node_ids).await,
        _ => context.peer_manager.get_seed_peers().await,
    };
    let mut seeds = match seeds {
        Ok(seeds) => seeds,
        Err(err) => {
            warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to load seed peers: {err}");
            Vec::new()
        },
    };
    // Real bans are honoured. The connectivity manager would refuse the dial anyway; this just avoids spending one of
    // the seed slots on it.
    seeds.retain(|seed| seed.is_seed() && !seed.is_banned() && seed.node_id != *context.node_identity.node_id());
    seeds.shuffle(&mut rand::rng());
    seeds.truncate(context.config.network_discovery.max_seed_peer_sync_count);

    let timeout = source_timeout(context);
    let results = future::join_all(seeds.into_iter().map(|seed| async move {
        let node_id = seed.node_id.clone();
        match time::timeout(timeout, sync_from_seed(context, seed)).await {
            Ok(result) => result.map(|stored| SourceResult {
                node_id,
                kind: SourceKind::Seed,
                stored,
            }),
            Err(_) => {
                debug!(
                    target: REBOOTSTRAP_LOG_TARGET,
                    "Sync from seed '{}' timed out after {timeout:.0?}",
                    node_id.short_str()
                );
                None
            },
        }
    }))
    .await;
    (seeds_resolved, results.into_iter().flatten().collect())
}

/// Asks the `SeedPeerProvider` for the current seeds and upserts them as seed peers. Returns the node ids of the
/// seeds accepted from this resolution, or `None` if there is no provider.
///
/// A resolved record whose public key is already stored as an ordinary (non-seed) peer is skipped: a DNS answer must
/// not be able to promote a known peer to a seed or overwrite what is stored for it.
pub(super) async fn refresh_seed_peers(context: &NetworkDiscoveryContext) -> Option<Vec<NodeId>> {
    let Some(provider) = context.seed_peer_provider.as_ref() else {
        debug!(target: REBOOTSTRAP_LOG_TARGET, "No seed peer provider configured, using stored seeds only");
        return None;
    };
    let mut accepted = Vec::new();
    for mut peer in provider.resolve_seed_peers().await {
        if peer.public_key == *context.node_identity.public_key() {
            continue;
        }
        match context.peer_manager.find_by_public_key(&peer.public_key).await {
            Ok(Some(existing)) if !existing.is_seed() => {
                warn!(
                    target: REBOOTSTRAP_LOG_TARGET,
                    "Resolved seed '{}' is already known as an ordinary peer. Skipping it.",
                    existing.node_id.short_str()
                );
                continue;
            },
            Ok(_) => {},
            Err(err) => {
                warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to look up resolved seed peer: {err}");
                continue;
            },
        }
        // Seeds are deliberately not added to the allow-list. Merging keeps any ban already recorded for this peer.
        peer.add_flags(PeerFlags::SEED);
        let node_id = peer.node_id.clone();
        match context.peer_manager.add_or_update_peer(peer).await {
            Ok(_) => accepted.push(node_id),
            Err(err) => warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to store resolved seed peer: {err}"),
        }
    }
    Some(accepted)
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

/// Syncs peers from up to `rebootstrap_connected_peers` random connected non-client peers. Outbound connections are
/// picked first, and only the remaining slots go to inbound ones: we chose our outbound peers, while anyone can dial
/// in. (A refinement of "inbound or outbound" in the rebootstrap spec.) Peers that relay invalid peer data are banned
/// the same way Discovering bans them. Returns a result for each peer synced from, outbound ones first.
async fn sync_from_connected_peers(context: &NetworkDiscoveryContext) -> Vec<SourceResult> {
    let max_peers = context.config.network_discovery.rebootstrap_connected_peers;
    if max_peers == 0 {
        return Vec::new();
    }
    let mut connectivity = context.connectivity.clone();
    let conns = match connectivity.get_active_connections().await {
        Ok(conns) => conns,
        Err(err) => {
            warn!(target: REBOOTSTRAP_LOG_TARGET, "Failed to get active connections: {err}");
            return Vec::new();
        },
    };
    let (mut outbound, mut inbound) = conns
        .into_iter()
        .filter(|conn| conn.is_connected() && !conn.peer_features().is_client())
        .partition::<Vec<_>, _>(|conn| conn.direction().is_outbound());
    outbound.shuffle(&mut rand::rng());
    inbound.shuffle(&mut rand::rng());
    let mut conns = outbound;
    conns.extend(inbound);
    conns.truncate(max_peers);

    let timeout = source_timeout(context);
    let results = future::join_all(conns.into_iter().map(|mut conn| async move {
        let node_id = conn.peer_node_id().clone();
        let kind = if conn.direction().is_outbound() {
            SourceKind::Outbound
        } else {
            SourceKind::Inbound
        };
        let result = match time::timeout(timeout, sync_from_connection(context, &mut conn)).await {
            Ok(result) => result,
            Err(_) => {
                debug!(
                    target: REBOOTSTRAP_LOG_TARGET,
                    "Sync from connected peer '{}' timed out after {timeout:.0?}",
                    node_id.short_str()
                );
                return None;
            },
        };
        match result {
            Ok(stored) => Some(SourceResult { node_id, kind, stored }),
            Err(err) => {
                debug!(
                    target: REBOOTSTRAP_LOG_TARGET,
                    "Failed to sync peers from connected peer '{}': {err}",
                    node_id.short_str()
                );
                ban_on_offence(context, node_id, &err).await;
                None
            },
        }
    }))
    .await;
    results.into_iter().flatten().collect()
}

/// Bans a connected peer whose sync failed in a way that is its fault, using the same classification as
/// Discovering. Not used for seeds.
pub(super) async fn ban_on_offence(context: &NetworkDiscoveryContext, node_id: NodeId, err: &NetworkDiscoveryError) {
    if let Some(severity) = offence_severity(err) {
        ban_peer(context, node_id, severity, err).await;
    }
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
    store_peers(context, conn.peer_node_id(), peers).await
}

/// Validates and stores peers received from `source`. Returns the node ids of the peers stored.
///
/// Like Discovering, an occasional unusable claim is skipped, but a source that relays more than
/// `MAX_HOSTILE_RELAYED_CLAIMS` claims only a broken or hostile node could produce fails with
/// `TooManyInvalidPeersReceived`, which is a ban offence.
pub(super) async fn store_peers(
    context: &NetworkDiscoveryContext,
    source: &NodeId,
    peers: Vec<PeerInfo>,
) -> Result<Vec<NodeId>, NetworkDiscoveryError> {
    let validator = PeerValidator::new(&context.config);
    let mut stored = Vec::new();
    let mut hostile_claims = 0usize;
    for peer_info in peers {
        let peer = match UnvalidatedPeerInfo::try_from(peer_info) {
            Ok(peer) => peer,
            Err(err) => {
                debug!(target: REBOOTSTRAP_LOG_TARGET, "Invalid peer info from '{source}': {err}");
                hostile_claims = hostile_claims.saturating_add(1);
                if hostile_claims > MAX_HOSTILE_RELAYED_CLAIMS {
                    return Err(NetworkDiscoveryError::TooManyInvalidPeersReceived);
                }
                continue;
            },
        };
        if peer.public_key == *context.node_identity.public_key() {
            continue;
        }
        let existing = context.peer_manager.find_by_public_key(&peer.public_key).await?;
        let valid_peer = match validator.validate_peer(peer, existing) {
            Ok(valid_peer) => valid_peer,
            Err(err) => {
                debug!(target: REBOOTSTRAP_LOG_TARGET, "Invalid peer from '{source}': {err}");
                if err.is_ban_offence() {
                    hostile_claims = hostile_claims.saturating_add(1);
                    if hostile_claims > MAX_HOSTILE_RELAYED_CLAIMS {
                        return Err(NetworkDiscoveryError::TooManyInvalidPeersReceived);
                    }
                }
                continue;
            },
        };
        let node_id = valid_peer.node_id.clone();
        context.peer_manager.add_or_update_peer(valid_peer).await?;
        stored.push(node_id);
    }
    Ok(stored)
}
