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

#[cfg(test)]
mod test;

mod metrics;
use std::{sync::Arc, time::Instant};

use log::*;
pub use metrics::{MetricsCollector, MetricsCollectorHandle};
use tari_comms::{
    connectivity::{ConnectivityError, ConnectivityEvent, ConnectivityEventRx, ConnectivityRequester},
    peer_manager::{NodeDistance, NodeId, PeerManagerError, PeerQuery, PeerQuerySortBy},
    NodeIdentity,
    PeerConnection,
    PeerManager,
};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{sync::broadcast, task, task::JoinHandle, time, time::MissedTickBehavior};

use crate::{connectivity::metrics::MetricsError, event::DhtEvent, DhtActorError, DhtConfig, DhtRequester};

const LOG_TARGET: &str = "comms::dht::connectivity";

#[derive(Debug, Error)]
pub enum DhtConnectivityError {
    #[error("ConnectivityError: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Failed to send network Join message: {0}")]
    SendJoinFailed(#[from] DhtActorError),
    #[error("Metrics error: {0}")]
    MetricError(#[from] MetricsError),
}

/// # DHT Connectivity Actor
///
/// Responsible for ensuring DHT network connectivity to a neighbouring and random peer set. This includes joining the
/// network when the node has established some peer connections (e.g to seed peers). It maintains neighbouring and
/// random peer pools and instructs the comms `ConnectivityManager` to establish those connections. Once a configured
/// percentage of these peers is online, the node is established on the DHT network.
///
/// The DHT connectivity actor monitors the connectivity state (using `ConnectivityEvent`s) and attempts
/// to maintain connectivity to the network as peers come and go.
pub struct DhtConnectivity {
    config: Arc<DhtConfig>,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    connectivity: ConnectivityRequester,
    dht_requester: DhtRequester,
    /// List of neighbours managed by DhtConnectivity ordered by distance from this node
    neighbours: Vec<NodeId>,
    /// A randomly-selected set of peers, excluding neighbouring peers.
    random_pool: Vec<NodeId>,
    /// Used to track when the random peer pool was last refreshed
    random_pool_last_refresh: Option<Instant>,
    /// Holds references to peer connections that should be kept alive
    connection_handles: Vec<PeerConnection>,
    stats: Stats,
    dht_events: broadcast::Receiver<Arc<DhtEvent>>,
    metrics_collector: MetricsCollectorHandle,
    cooldown_in_effect: Option<Instant>,
    recent_connection_failure_count: usize,
    shutdown_signal: ShutdownSignal,
}

impl DhtConnectivity {
    pub fn new(
        config: Arc<DhtConfig>,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        connectivity: ConnectivityRequester,
        dht_requester: DhtRequester,
        dht_events: broadcast::Receiver<Arc<DhtEvent>>,
        metrics_collector: MetricsCollectorHandle,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            neighbours: Vec::with_capacity(config.num_neighbouring_nodes),
            random_pool: Vec::with_capacity(config.num_random_nodes),
            connection_handles: Vec::with_capacity(config.num_neighbouring_nodes + config.num_random_nodes),
            config,
            peer_manager,
            node_identity,
            connectivity,
            dht_requester,
            metrics_collector,
            random_pool_last_refresh: None,
            stats: Stats::new(),
            dht_events,
            recent_connection_failure_count: 0,
            cooldown_in_effect: None,
            shutdown_signal,
        }
    }

    /// Spawn a DhtConnectivity actor. This will immediately subscribe to the connection manager event stream to
    /// prevent unexpected missed events.
    pub fn spawn(mut self) -> JoinHandle<Result<(), DhtConnectivityError>> {
        // Listen to events as early as possible
        let connectivity_events = self.connectivity.get_event_subscription();
        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        task::spawn(async move {
            log_mdc::extend(mdc.clone());
            debug!(target: LOG_TARGET, "Waiting for connectivity manager to start");
            let _ = self.connectivity.wait_started().await;
            log_mdc::extend(mdc.clone());
            match self.run(connectivity_events).await {
                Ok(_) => Ok(()),
                Err(err) => {
                    error!(target: LOG_TARGET, "DhtConnectivity exited with error: {:?}", err);
                    Err(err)
                },
            }
        })
    }

    pub async fn run(mut self, mut connectivity_events: ConnectivityEventRx) -> Result<(), DhtConnectivityError> {
        debug!(target: LOG_TARGET, "DHT connectivity starting");
        self.refresh_neighbour_pool().await?;

        let mut ticker = time::interval(self.config.connectivity_update_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                Ok(event) = connectivity_events.recv() => {
                    if let Err(err) = self.handle_connectivity_event(event).await {
                        debug!(target: LOG_TARGET, "Error handling connectivity event: {:?}", err);
                    }
               },

               Ok(event) = self.dht_events.recv() => {
                    if let Err(err) = self.handle_dht_event(&event).await {
                        debug!(target: LOG_TARGET, "Error handling DHT event: {:?}", err);
                    }
               },

               _ = ticker.tick() => {
                    if let Err(err) = self.check_and_ban_flooding_peers().await {
                        debug!(target: LOG_TARGET, "Error checking for peer flooding: {:?}", err);
                    }
                    if let Err(err) = self.refresh_neighbour_pool_if_required().await {
                        debug!(target: LOG_TARGET, "Error refreshing neighbour peer pool: {:?}", err);
                    }
                    if let Err(err) = self.refresh_random_pool_if_required().await {
                        debug!(target: LOG_TARGET, "Error refreshing random peer pool: {:?}", err);
                    }
                    self.log_status();
               },

               _ = self.shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "DhtConnectivity shutting down because the shutdown signal was received");
                    break;
               }
            }
        }

        Ok(())
    }

    fn log_status(&self) {
        let (neighbour_connected, neighbour_pending) = self
            .neighbours
            .iter()
            .partition::<Vec<_>, _>(|peer| self.connection_handles.iter().any(|c| c.peer_node_id() == *peer));
        let (random_connected, random_pending) = self
            .random_pool
            .iter()
            .partition::<Vec<_>, _>(|peer| self.connection_handles.iter().any(|c| c.peer_node_id() == *peer));
        debug!(
            target: LOG_TARGET,
            "DHT connectivity status: {}neighbour pool: {}/{} ({} connected), random pool: {}/{} ({} connected, last \
             refreshed {}), active DHT connections: {}/{}",
            self.cooldown_in_effect
                .map(|ts| format!(
                    "COOLDOWN({:.2?} remaining) ",
                    self.config
                        .connectivity_high_failure_rate_cooldown
                        .saturating_sub(ts.elapsed())
                ))
                .unwrap_or_else(String::new),
            self.neighbours.len(),
            self.config.num_neighbouring_nodes,
            neighbour_connected.len(),
            self.random_pool.len(),
            self.config.num_random_nodes,
            random_connected.len(),
            self.random_pool_last_refresh
                .map(|i| format!("{:.0?} ago", i.elapsed()))
                .unwrap_or_else(|| "<never>".to_string()),
            self.connection_handles.len(),
            self.config.num_neighbouring_nodes + self.config.num_random_nodes,
        );
        if !neighbour_pending.is_empty() || !random_pending.is_empty() {
            debug!(
                target: LOG_TARGET,
                "Pending connections: neighbouring({}), random({})",
                neighbour_pending
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                random_pending
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    async fn handle_dht_event(&mut self, event: &DhtEvent) -> Result<(), DhtConnectivityError> {
        #[allow(clippy::single_match)]
        match event {
            DhtEvent::NetworkDiscoveryPeersAdded(info) => {
                if info.has_new_neighbours() {
                    debug!(
                        target: LOG_TARGET,
                        "Network discovery discovered {} more neighbouring peers. Reinitializing pools",
                        info.num_new_peers
                    );
                    self.refresh_peer_pools().await?;
                }
            },
            _ => {},
        }

        Ok(())
    }

    async fn check_and_ban_flooding_peers(&mut self) -> Result<(), DhtConnectivityError> {
        let nodes = self
            .metrics_collector
            .get_message_rates_exceeding(self.config.flood_ban_max_msg_count, self.config.flood_ban_timespan)
            .await?;

        for (peer, mps) in nodes {
            warn!(
                target: LOG_TARGET,
                "Banning peer `{}` because of flooding. Message rate: {:.2}m/s", peer, mps
            );
            self.connectivity
                .ban_peer_until(
                    peer,
                    self.config.ban_duration_short,
                    format!(
                        "Exceeded maximum message rate. Config: {}/{:#?}. Rate: {:.2} m/s",
                        self.config.flood_ban_max_msg_count, self.config.flood_ban_timespan, mps
                    ),
                )
                .await?;
        }
        Ok(())
    }

    async fn refresh_peer_pools(&mut self) -> Result<(), DhtConnectivityError> {
        info!(
            target: LOG_TARGET,
            "Reinitializing neighbour pool. (size={})",
            self.neighbours.len(),
        );

        self.refresh_neighbour_pool().await?;
        self.refresh_random_pool().await?;

        Ok(())
    }

    async fn refresh_neighbour_pool_if_required(&mut self) -> Result<(), DhtConnectivityError> {
        if self.num_connected_neighbours() < self.config.num_neighbouring_nodes {
            self.refresh_neighbour_pool().await?;
        }

        Ok(())
    }

    fn num_connected_neighbours(&self) -> usize {
        self.neighbours
            .iter()
            .filter(|peer| self.connection_handles.iter().any(|c| c.peer_node_id() == *peer))
            .count()
    }

    fn connected_peers_iter(&self) -> impl Iterator<Item = &NodeId> {
        self.connection_handles.iter().map(|c| c.peer_node_id())
    }

    async fn refresh_neighbour_pool(&mut self) -> Result<(), DhtConnectivityError> {
        let mut new_neighbours = self
            .fetch_neighbouring_peers(self.config.num_neighbouring_nodes, &[])
            .await?;

        if new_neighbours.is_empty() {
            info!(
                target: LOG_TARGET,
                "Unable to refresh neighbouring peer pool because there are insufficient known/online peers",
            );
            self.redial_neighbours_as_required().await?;
            return Ok(());
        }

        let (intersection, difference) = self
            .neighbours
            .iter()
            .cloned()
            .partition::<Vec<_>, _>(|n| new_neighbours.contains(n));
        // Only retain the peers that aren't already added
        new_neighbours.retain(|n| !intersection.contains(n));
        self.neighbours.retain(|n| intersection.contains(n));

        debug!(
            target: LOG_TARGET,
            "Adding {} neighbouring peer(s), removing {} peers",
            new_neighbours.len(),
            difference.len()
        );
        debug!(
            target: LOG_TARGET,
            "Adding {} peer(s) to DHT connectivity manager: {}",
            new_neighbours.len(),
            new_neighbours
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );

        new_neighbours.iter().cloned().for_each(|peer| {
            self.insert_neighbour(peer);
        });

        // Drop any connection handles that removed from the neighbour pool
        difference.iter().for_each(|peer| {
            self.remove_connection_handle(peer);
        });

        if !new_neighbours.is_empty() {
            self.connectivity.request_many_dials(new_neighbours).await?;
        }

        self.redial_neighbours_as_required().await?;

        Ok(())
    }

    async fn redial_neighbours_as_required(&mut self) -> Result<(), DhtConnectivityError> {
        let disconnected = self
            .connection_handles
            .iter()
            .filter(|c| !c.is_connected())
            .collect::<Vec<_>>();
        let to_redial = self
            .neighbours
            .iter()
            .filter(|n| disconnected.iter().any(|c| c.peer_node_id() == *n))
            .cloned()
            .collect::<Vec<_>>();

        if !to_redial.is_empty() {
            debug!(
                target: LOG_TARGET,
                "Redialling {} disconnected peer(s)",
                to_redial.len()
            );
            self.connectivity.request_many_dials(to_redial).await?;
        }

        Ok(())
    }

    async fn refresh_random_pool_if_required(&mut self) -> Result<(), DhtConnectivityError> {
        let should_refresh = self.config.num_random_nodes > 0 &&
            self.random_pool_last_refresh
                .map(|instant| instant.elapsed() >= self.config.connectivity_random_pool_refresh)
                .unwrap_or(true);
        if should_refresh {
            self.refresh_random_pool().await?;
        }

        Ok(())
    }

    async fn refresh_random_pool(&mut self) -> Result<(), DhtConnectivityError> {
        let mut random_peers = self
            .fetch_random_peers(self.config.num_random_nodes, &self.neighbours)
            .await?;
        if random_peers.is_empty() {
            info!(
                target: LOG_TARGET,
                "Unable to refresh random peer pool because there are insufficient known peers",
            );
            return Ok(());
        }

        let (intersection, difference) = self
            .random_pool
            .drain(..)
            .partition::<Vec<_>, _>(|n| random_peers.contains(n));
        // Remove the peers that we want to keep from the `random_peers` to be added
        random_peers.retain(|n| !intersection.contains(n));
        self.random_pool = intersection;
        debug!(
            target: LOG_TARGET,
            "Adding new peers to random peer pool (#new = {}, #keeping = {}, #removing = {})",
            random_peers.len(),
            self.random_pool.len(),
            difference.len()
        );
        trace!(
            target: LOG_TARGET,
            "Random peers: Adding = {:?}, Removing = {:?}",
            random_peers,
            difference
        );
        self.random_pool.extend(random_peers.clone());
        // Drop any connection handles that removed from the random pool
        difference.iter().for_each(|peer| {
            self.remove_connection_handle(peer);
        });
        self.connectivity.request_many_dials(random_peers).await?;

        self.random_pool_last_refresh = Some(Instant::now());
        Ok(())
    }

    async fn handle_new_peer_connected(&mut self, conn: PeerConnection) -> Result<(), DhtConnectivityError> {
        self.peer_manager.mark_last_seen(conn.peer_node_id()).await?;
        if conn.peer_features().is_client() {
            debug!(
                target: LOG_TARGET,
                "Client node '{}' connected",
                conn.peer_node_id().short_str()
            );
            return Ok(());
        }

        if self.is_pool_peer(conn.peer_node_id()) {
            debug!(
                target: LOG_TARGET,
                "Added peer {} to connection handles",
                conn.peer_node_id()
            );
            self.insert_connection_handle(conn);
            return Ok(());
        }

        let current_dist = conn.peer_node_id().distance(self.node_identity.node_id());
        let neighbour_distance = self.get_neighbour_max_distance();
        if current_dist < neighbour_distance {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' connected that is closer than any current neighbour. Adding to neighbours.",
                conn.peer_node_id().short_str()
            );

            let peer_to_insert = conn.peer_node_id().clone();
            self.insert_connection_handle(conn);
            if let Some(node_id) = self.insert_neighbour(peer_to_insert.clone()) {
                // If we kicked a neighbour out of our neighbour pool but the random pool is not full.
                // Add the neighbour to the random pool, otherwise remove the handle from the connection pool
                if self.random_pool.len() < self.config.num_random_nodes {
                    debug!(
                        target: LOG_TARGET,
                        "Moving peer '{}' from neighbouring pool to random pool", peer_to_insert
                    );
                    self.random_pool.push(node_id);
                } else {
                    self.remove_connection_handle(&node_id)
                }
            }
        }

        Ok(())
    }

    fn insert_connection_handle(&mut self, conn: PeerConnection) {
        // Remove any existing connection for this peer
        self.remove_connection_handle(conn.peer_node_id());
        debug!(target: LOG_TARGET, "Insert new peer connection {}", conn);
        self.connection_handles.push(conn);
    }

    fn remove_connection_handle(&mut self, node_id: &NodeId) {
        if let Some(idx) = self.connection_handles.iter().position(|c| c.peer_node_id() == node_id) {
            let conn = self.connection_handles.swap_remove(idx);
            debug!(target: LOG_TARGET, "Removing peer connection {}", conn);
        }
    }

    async fn handle_connectivity_event(&mut self, event: ConnectivityEvent) -> Result<(), DhtConnectivityError> {
        use ConnectivityEvent::*;
        debug!(target: LOG_TARGET, "Connectivity event: {}", event);
        match event {
            PeerConnected(conn) => {
                self.handle_new_peer_connected(conn).await?;
            },
            PeerConnectFailed(node_id) => {
                self.connection_handles.retain(|c| *c.peer_node_id() != node_id);
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{}`. Metric collector is shut down.", node_id
                    );
                };
                if !self.is_pool_peer(&node_id) {
                    debug!(target: LOG_TARGET, "{} is not managed by the DHT. Ignoring", node_id);
                    return Ok(());
                }

                const TOLERATED_CONNECTION_FAILURES: usize = 40;
                if self.recent_connection_failure_count < TOLERATED_CONNECTION_FAILURES {
                    self.recent_connection_failure_count += 1;
                }

                if self.recent_connection_failure_count == TOLERATED_CONNECTION_FAILURES &&
                    self.cooldown_in_effect.is_none()
                {
                    warn!(
                        target: LOG_TARGET,
                        "Too many ({}) connection failures, cooldown is in effect", TOLERATED_CONNECTION_FAILURES
                    );
                    self.cooldown_in_effect = Some(Instant::now());
                }

                if self
                    .cooldown_in_effect
                    .map(|ts| ts.elapsed() >= self.config.connectivity_high_failure_rate_cooldown)
                    .unwrap_or(true)
                {
                    if self.cooldown_in_effect.is_some() {
                        self.cooldown_in_effect = None;
                        self.recent_connection_failure_count = 1;
                    }
                    self.replace_pool_peer(&node_id).await?;
                }
                self.log_status();
            },
            PeerDisconnected(node_id) => {
                self.connection_handles.retain(|c| *c.peer_node_id() != node_id);
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{}`. Metric collector is shut down.", node_id
                    );
                };
                if !self.is_pool_peer(&node_id) {
                    debug!(target: LOG_TARGET, "{} is not managed by the DHT. Ignoring", node_id);
                    return Ok(());
                }
                debug!(target: LOG_TARGET, "Pool peer {} disconnected. Redialling...", node_id);
                // Attempt to reestablish the lost connection to the pool peer. If reconnection fails,
                // it is replaced with another peer (replace_pool_peer via PeerConnectFailed)
                self.connectivity.request_many_dials([node_id]).await?;
            },
            ConnectivityStateOnline(n) => {
                self.refresh_peer_pools().await?;
                if self.config.auto_join && self.should_send_join() {
                    debug!(
                        target: LOG_TARGET,
                        "Node is online ({} peer(s) connected). Sending network join message.", n
                    );
                    self.dht_requester
                        .send_join()
                        .await
                        .map_err(DhtConnectivityError::SendJoinFailed)?;

                    self.stats.mark_join_sent();
                }
            },
            ConnectivityStateOffline => {
                debug!(target: LOG_TARGET, "Node is OFFLINE");
                self.refresh_peer_pools().await?;
            },
            _ => {},
        }

        Ok(())
    }

    async fn replace_pool_peer(&mut self, current_peer: &NodeId) -> Result<(), DhtConnectivityError> {
        if self.random_pool.contains(current_peer) {
            let exclude = self.get_pool_peers();
            let pos = self
                .random_pool
                .iter()
                .position(|n| n == current_peer)
                .expect("unreachable panic");
            self.random_pool.swap_remove(pos);

            debug!(
                target: LOG_TARGET,
                "Peer '{}' in random pool is unavailable. Adding a new random peer if possible", current_peer
            );
            match self.fetch_random_peers(1, &exclude).await?.pop() {
                Some(new_peer) => {
                    self.remove_connection_handle(current_peer);
                    if let Some(pos) = self.random_pool.iter().position(|n| n == current_peer) {
                        self.random_pool.swap_remove(pos);
                    }
                    self.random_pool.push(new_peer.clone());
                    self.connectivity.request_many_dials([new_peer]).await?;
                },
                None => {
                    debug!(
                        target: LOG_TARGET,
                        "Unable to fetch new random peer to replace disconnected peer '{}' because not enough peers \
                         are known. Random pool size is {}.",
                        current_peer,
                        self.random_pool.len()
                    );
                },
            }
        }

        if self.neighbours.contains(current_peer) {
            let exclude = self.get_pool_peers();
            let pos = self
                .neighbours
                .iter()
                .position(|n| n == current_peer)
                .expect("unreachable panic");
            self.neighbours.remove(pos);

            debug!(
                target: LOG_TARGET,
                "Peer '{}' in neighbour pool is offline. Adding a new peer if possible", current_peer
            );
            match self.fetch_neighbouring_peers(1, &exclude).await?.pop() {
                Some(node_id) => {
                    self.remove_connection_handle(current_peer);
                    if let Some(pos) = self.neighbours.iter().position(|n| n == current_peer) {
                        self.neighbours.remove(pos);
                    }
                    self.insert_neighbour(node_id.clone());
                    self.connectivity.request_many_dials([node_id]).await?;
                },
                None => {
                    info!(
                        target: LOG_TARGET,
                        "Unable to fetch new neighbouring peer to replace disconnected peer '{}'. Neighbour pool size \
                         is {}.",
                        current_peer,
                        self.neighbours.len()
                    );
                },
            }
        }

        Ok(())
    }

    fn insert_neighbour(&mut self, node_id: NodeId) -> Option<NodeId> {
        let dist = node_id.distance(self.node_identity.node_id());
        let pos = self
            .neighbours
            .iter()
            .position(|node_id| node_id.distance(self.node_identity.node_id()) > dist);

        let removed_peer = if self.neighbours.len() + 1 > self.config.num_neighbouring_nodes {
            self.neighbours.pop()
        } else {
            None
        };

        match pos {
            Some(idx) => {
                self.neighbours.insert(idx, node_id);
            },
            None => {
                self.neighbours.push(node_id);
            },
        }

        removed_peer
    }

    fn is_pool_peer(&self, node_id: &NodeId) -> bool {
        self.neighbours.contains(node_id) || self.random_pool.contains(node_id)
    }

    fn get_pool_peers(&self) -> Vec<NodeId> {
        self.neighbours.iter().chain(self.random_pool.iter()).cloned().collect()
    }

    fn get_neighbour_max_distance(&self) -> NodeDistance {
        assert!(
            self.config.num_neighbouring_nodes > 0,
            "DhtConfig::num_neighbouring_nodes must be greater than zero"
        );

        if self.neighbours.len() < self.config.num_neighbouring_nodes {
            return NodeDistance::max_distance();
        }

        self.neighbours
            .last()
            .map(|node_id| node_id.distance(self.node_identity.node_id()))
            .expect("already checked")
    }

    async fn fetch_neighbouring_peers(
        &self,
        n: usize,
        excluded: &[NodeId],
    ) -> Result<Vec<NodeId>, DhtConnectivityError> {
        let peer_manager = &self.peer_manager;
        let node_id = self.node_identity.node_id();
        let connected = self.connected_peers_iter().collect::<Vec<_>>();
        // Fetch to all n nearest neighbour Communication Nodes
        // which are eligible for connection.
        // Currently that means:
        // - The peer isn't banned,
        // - it has the required features
        // - it didn't recently fail to connect, and
        // - it is not in the exclusion list in closest_request
        let mut connect_ineligable_count = 0;
        let mut banned_count = 0;
        let mut excluded_count = 0;
        let mut filtered_out_node_count = 0;
        let mut already_connected = 0;
        let query = PeerQuery::new()
            .select_where(|peer| {
                if peer.is_banned() {
                    banned_count += 1;
                    return false;
                }

                if peer.features.is_client() {
                    filtered_out_node_count += 1;
                    return false;
                }

                if connected.contains(&&peer.node_id) {
                    already_connected += 1;
                    return false;
                }

                if peer
                    .offline_since()
                    .map(|since| since <= self.config.offline_peer_cooldown)
                    .unwrap_or(false)
                {
                    connect_ineligable_count += 1;
                    return false;
                }

                let is_excluded = excluded.contains(&peer.node_id);
                if is_excluded {
                    excluded_count += 1;
                    return false;
                }

                true
            })
            .sort_by(PeerQuerySortBy::DistanceFromLastConnected(node_id))
            // Fetch double here so that there is a bigger closest peer set that can be ordered by last seen
            .limit(n * 2);

        let peers = peer_manager.perform_query(query).await?;
        let total_excluded = banned_count + connect_ineligable_count + excluded_count + filtered_out_node_count;
        if total_excluded > 0 {
            debug!(
                target: LOG_TARGET,
                "\n====================================\n Closest Peer Selection\n\n {num_peers} peer(s) selected\n \
                 {total} peer(s) were not selected \n\n {banned} banned\n {filtered_out} not communication node\n \
                 {not_connectable} are not connectable\n {excluded} explicitly excluded\n {already_connected} already \
                 connected
                 \n====================================\n",
                num_peers = peers.len(),
                total = total_excluded,
                banned = banned_count,
                filtered_out = filtered_out_node_count,
                not_connectable = connect_ineligable_count,
                excluded = excluded_count,
                already_connected = already_connected
            );
        }

        Ok(peers.into_iter().map(|p| p.node_id).take(n).collect())
    }

    async fn fetch_random_peers(&self, n: usize, excluded: &[NodeId]) -> Result<Vec<NodeId>, DhtConnectivityError> {
        let peers = self.peer_manager.random_peers(n, excluded).await?;
        Ok(peers.into_iter().map(|p| p.node_id).collect())
    }

    fn should_send_join(&self) -> bool {
        let cooldown = self.config.join_cooldown_interval;
        self.stats
            .join_last_sent_at()
            .map(|at| at.elapsed() > cooldown)
            .unwrap_or(true)
    }
}

/// Basic connectivity stats. Right now, it is only used to track the last time a join message was sent to prevent the
/// node spamming the network if local connectivity changes.
#[derive(Debug, Default)]
struct Stats {
    join_last_sent_at: Option<Instant>,
}

impl Stats {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn join_last_sent_at(&self) -> Option<Instant> {
        self.join_last_sent_at
    }

    pub fn mark_join_sent(&mut self) {
        self.join_last_sent_at = Some(Instant::now());
    }
}
