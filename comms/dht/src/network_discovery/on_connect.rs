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

use std::{
    cmp,
    collections::HashMap,
    convert::TryInto,
    time::{Duration, Instant},
};

use futures::StreamExt;
use log::*;
use tari_comms::{PeerConnection, connectivity::ConnectivityEvent, peer_manager::NodeId};
use tari_utilities::hex::Hex;
use tokio::{sync::broadcast, time};

use crate::{
    DhtConfig,
    actor::OffenceSeverity,
    event::DhtEvent,
    network_discovery::{
        DhtNetworkDiscoveryRoundInfo,
        NetworkDiscoveryError,
        admission::HostileClaims,
        discovering::{ban_peer, is_bad_data_offence},
        state_machine::{DiscoveryPhase, NetworkDiscoveryContext, StateEvent},
    },
    peer_validator::PeerValidator,
    proto::rpc::GetPeersRequest,
    rpc,
    rpc::UnvalidatedPeerInfo,
};
const LOG_TARGET: &str = "comms::dht::network_discovery:onconnect";
const NUM_FETCH_PEERS: u32 = 100;
/// The time to wait before syncing again from a peer whose sync failed, unless `on_connect_resync_ttl` is shorter. A
/// peer that keeps reconnecting is not retried on every connection, and a transient failure is still retried.
const FAILED_SYNC_BACKOFF: Duration = Duration::from_secs(5 * 60);

/// The longest a peer is left unsynced after a sync. A larger `on_connect_resync_ttl` is treated as this, so that a
/// huge TTL means "not for a week" rather than overflowing, and every entry expires, which keeps `prev_synced` bounded.
const MAX_RESYNC_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug)]
pub(super) struct OnConnect {
    context: NetworkDiscoveryContext,
    /// Peers synced from, or whose sync failed, and the time from which they may be synced from again
    pub(super) prev_synced: HashMap<NodeId, Instant>,
}

impl OnConnect {
    pub fn new(context: NetworkDiscoveryContext) -> Self {
        Self {
            context,
            prev_synced: HashMap::new(),
        }
    }

    /// Returns true if peers were synced from `node_id` within the last `on_connect_resync_ttl`, or a sync from it
    /// failed within the last `FAILED_SYNC_BACKOFF`.
    pub(super) fn is_recently_synced(&self, node_id: &NodeId, now: Instant) -> bool {
        self.prev_synced
            .get(node_id)
            .is_some_and(|resync_from| now < *resync_from)
    }

    /// Records a sync from `node_id`, which is not synced from again for `on_connect_resync_ttl`.
    pub(super) fn mark_synced(&mut self, node_id: NodeId, now: Instant) {
        let ttl = self.config().network_discovery.on_connect_resync_ttl;
        self.mark_until(node_id, now, ttl);
    }

    /// Records a failed sync from `node_id`, which is not synced from again for `FAILED_SYNC_BACKOFF`, or
    /// `on_connect_resync_ttl` if that is shorter.
    pub(super) fn mark_sync_failed(&mut self, node_id: NodeId, now: Instant) {
        let ttl = self.config().network_discovery.on_connect_resync_ttl;
        self.mark_until(node_id, now, cmp::min(FAILED_SYNC_BACKOFF, ttl));
    }

    /// Records that `node_id` is not synced from for `duration`, forgetting entries that have expired so the map stays
    /// bounded.
    fn mark_until(&mut self, node_id: NodeId, now: Instant, duration: Duration) {
        self.prev_synced.retain(|_, resync_from| now < *resync_from);
        let duration = duration.min(MAX_RESYNC_TTL);
        self.prev_synced
            .insert(node_id, now.checked_add(duration).unwrap_or(now));
    }

    pub async fn next_event(&mut self) -> StateEvent {
        let mut connectivity_events = self.context.connectivity.get_event_subscription();
        loop {
            let event = connectivity_events.recv().await;
            match event {
                Ok(ConnectivityEvent::PeerConnected(conn)) => {
                    if conn.peer_features().is_client() {
                        continue;
                    }
                    if self.is_recently_synced(conn.peer_node_id(), Instant::now()) {
                        debug!(
                            target: LOG_TARGET,
                            "Recently synced from peer `{}`. Skipping",
                            conn.peer_node_id()
                        );
                        continue;
                    }
                    self.sync_from(*conn).await;
                },
                Ok(_) => { /* Nothing to do */ },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(target: LOG_TARGET, "Lagged behind on {n} connectivity event(s)")
                },
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                },
            }
        }

        StateEvent::Shutdown
    }

    /// Syncs peers from a newly connected peer.
    ///
    /// The peer is marked as synced before the sync, so that it is not synced from twice at once. If the sync fails it
    /// is marked for `FAILED_SYNC_BACKOFF` instead, so that a transient failure does not block a retry for
    /// `on_connect_resync_ttl`, while a peer that keeps reconnecting is not synced from on every connection. A peer
    /// that relayed too many claims with invalid signatures is banned and stays marked.
    ///
    /// Inbound peers are synced from too: the per-source budget bounds what any one of them can add, so the inbound
    /// share limit of a rebootstrap is not applied here.
    pub(super) async fn sync_from(&mut self, conn: PeerConnection) {
        let node_id = conn.peer_node_id().clone();
        self.mark_synced(node_id.clone(), Instant::now());

        debug!(target: LOG_TARGET, "Node peer `{node_id}` connected. Syncing peers...");

        // The per-item timeouts alone let a peer drip records for minutes, holding up every other connection
        let deadline = self.config().network_discovery.on_connect_sync_timeout;
        let result = time::timeout(deadline, self.sync_peers(conn))
            .await
            .unwrap_or_else(|_| {
                Err(NetworkDiscoveryError::Timeout {
                    operation: "on_connect_sync".to_string(),
                    peer: node_id.to_hex(),
                    duration: format!("{deadline:.2?}"),
                })
            });
        if let Err(err) = result {
            if is_bad_data_offence(&err) {
                warn!(target: LOG_TARGET, "Peer `{node_id}` sent bad peer data: {err}. Banning peer.");
                ban_peer(&self.context, node_id, OffenceSeverity::High, err).await;
            } else {
                debug!(target: LOG_TARGET, "Failed to peer sync from `{node_id}`: {err}");
                self.mark_sync_failed(node_id, Instant::now());
            }
        }
    }

    async fn sync_peers(&self, mut conn: PeerConnection) -> Result<(), NetworkDiscoveryError> {
        let sync_peer = conn.peer_node_id().clone();
        let discovery_config = &self.config().network_discovery;
        let timed_out = |operation: &str, duration: Duration| {
            debug!(target: LOG_TARGET, "{operation} to `{sync_peer}` timed out after {duration:.0?}");
            NetworkDiscoveryError::Timeout {
                operation: operation.to_string(),
                peer: sync_peer.to_hex(),
                duration: format!("{duration:.2?}"),
            }
        };

        let connect_timeout = discovery_config.bootstrap_rpc_connect_timeout;
        let mut client = time::timeout(connect_timeout, conn.connect_rpc::<rpc::DhtClient>())
            .await
            .map_err(|_| timed_out("connect_rpc", connect_timeout))??;
        let request = GetPeersRequest {
            n: NUM_FETCH_PEERS,
            include_clients: false,
            max_claims: self.config().max_permitted_peer_claims.try_into().unwrap_or_else(|_| {
                error!(target: LOG_TARGET, "Node configured to accept more than u32::MAX claims per peer");
                u32::MAX
            }),
            max_addresses_per_claim: self
                .config()
                .peer_validator_config
                .max_permitted_peer_addresses_per_claim
                .try_into()
                .unwrap_or_else(|_| {
                    error!(target: LOG_TARGET, "Node configured to accept more than u32::MAX addresses per claim");
                    u32::MAX
                }),
        };
        let get_peers_timeout = discovery_config.bootstrap_rpc_get_peers_stream_timeout;
        let peer_stream = time::timeout(get_peers_timeout, client.get_peers(request))
            .await
            .map_err(|_| timed_out("get_peers", get_peers_timeout))??;

        // Take up to `NUM_FETCH_PEERS` then close the stream.
        let mut peer_stream = peer_stream.take(NUM_FETCH_PEERS as usize);

        let streaming_timeout = discovery_config.bootstrap_rpc_streaming_timeout;
        let mut hostile_claims = HostileClaims::default();
        let mut num_added = 0usize;
        while let Some(resp) = time::timeout(streaming_timeout, peer_stream.next())
            .await
            .map_err(|_| timed_out("get_peer_response", streaming_timeout))?
        {
            match resp {
                Ok(resp) => match resp.peer.and_then(|peer| UnvalidatedPeerInfo::try_from(peer).ok()) {
                    Some(peer) => {
                        let pub_key = peer.public_key.clone();
                        match self.validate_and_add_peer(&sync_peer, peer).await {
                            Ok(Some(true)) => {
                                debug!(target: LOG_TARGET, "Added new peer `{pub_key}` from `{sync_peer}`");
                                num_added = num_added.saturating_add(1);
                            },
                            Ok(Some(false)) => {},
                            Ok(None) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Peer `{sync_peer}` has relayed its share of new peers. Ending the sync"
                                );
                                break;
                            },
                            Err(NetworkDiscoveryError::PeerValidationError(e)) => {
                                debug!(target: LOG_TARGET, "Failed to validate peer `{pub_key}` from `{sync_peer}`: {e}");
                                hostile_claims.record(&e)?;
                            },
                            Err(e) => {
                                debug!(target: LOG_TARGET, "Failed to add peer `{pub_key}` from `{sync_peer}`: {e}");
                            },
                        }
                    },
                    None => {
                        debug!(target: LOG_TARGET, "Invalid response from peer `{sync_peer}`");
                    },
                },
                Err(err) => {
                    debug!(target: LOG_TARGET, "Error response from peer `{sync_peer}`: {err}");
                },
            }
        }

        debug!(
            target: LOG_TARGET,
            "Added {num_added} peer(s) from peer `{sync_peer}`"
        );
        if num_added > 0 {
            self.context
                .publish_event(DhtEvent::NetworkDiscoveryPeersAdded(DhtNetworkDiscoveryRoundInfo {
                    num_new_peers: num_added,
                    num_duplicate_peers: 0,
                    num_succeeded: num_added,
                    sync_peers: vec![sync_peer],
                    phase: DiscoveryPhase::General, // This is regular peer connection, not seed bootstrap
                }));
        }

        Ok(())
    }

    /// Validates and stores a peer relayed by `sync_peer`, charging a new peer to `sync_peer`'s budget. Returns
    /// whether the peer is new, or `None` (storing nothing) if that budget is used up.
    async fn validate_and_add_peer(
        &self,
        sync_peer: &NodeId,
        peer: UnvalidatedPeerInfo,
    ) -> Result<Option<bool>, NetworkDiscoveryError> {
        let peer_validator = PeerValidator::new(self.config());
        let maybe_existing_peer = self.context.peer_manager.find_by_public_key(&peer.public_key).await?;
        let is_new_peer = maybe_existing_peer.is_none();
        let valid_peer = peer_validator.validate_peer(peer, maybe_existing_peer)?;
        if is_new_peer &&
            !self.context.source_budget.lock().is_ok_and(|mut budget| {
                budget.try_charge(sync_peer, Instant::now(), &self.config().network_discovery)
            })
        {
            return Ok(None);
        }
        self.context.peer_manager.add_or_update_peer(valid_peer).await?;
        Ok(Some(is_new_peer))
    }

    #[inline]
    fn config(&self) -> &DhtConfig {
        &self.context.config
    }
}
