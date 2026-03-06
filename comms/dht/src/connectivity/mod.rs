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

//! # DHT Connectivity Actor
//!
//! Responsible for ensuring DHT network connectivity to a neighbouring and random peer set. This includes joining the
//! network when the node has established some peer connections (e.g to seed peers). It maintains neighbouring and
//! random peer pools and instructs the comms `ConnectivityManager` to establish those connections. Once a configured
//! percentage of these peers is online, the node is established on the DHT network.
//!
//! The DHT connectivity actor monitors the connectivity state (using `ConnectivityEvent`s) and attempts
//! to maintain connectivity to the network as peers come and go.

#[cfg(test)]
mod test;

mod metrics;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
pub use metrics::{MetricsCollector, MetricsCollectorHandle};
use tari_comms::{
    Minimized,
    NodeIdentity,
    PeerConnection,
    PeerManager,
    connectivity::{
        ConnectivityError,
        ConnectivityEvent,
        ConnectivityEventRx,
        ConnectivityRequester,
        ConnectivitySelection,
    },
    multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerManagerError},
};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{sync::broadcast, task, task::JoinHandle, time, time::MissedTickBehavior};

use crate::{DhtActorError, DhtConfig, DhtRequester, connectivity::metrics::MetricsError, event::DhtEvent};

const LOG_TARGET: &str = "comms::dht::connectivity";

/// Error type for the DHT connectivity actor.
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

/// DHT connectivity actor.
pub(crate) struct DhtConnectivity {
    config: Arc<DhtConfig>,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    connectivity: ConnectivityRequester,
    dht_requester: DhtRequester,
    /// List of neighbours managed by DhtConnectivity ordered by distance from this node
    neighbours: Vec<NodeId>,
    /// A randomly-selected set of peers, excluding neighbouring peers.
    random_pool: Vec<NodeId>,
    /// The random pool history.
    previous_random: Vec<NodeId>,
    /// Used to track when the random peer pool was last refreshed
    random_pool_last_refresh: Option<Instant>,
    /// Holds references to peer connections that should be kept alive
    connection_handles: Vec<PeerConnection>,
    stats: Stats,
    dht_events: broadcast::Receiver<Arc<DhtEvent>>,
    metrics_collector: MetricsCollectorHandle,
    cooldown_in_effect: Option<Instant>,
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
            cooldown_in_effect: None,
            shutdown_signal,
            previous_random: vec![],
        }
    }

    /// Spawn a DhtConnectivity actor. This will immediately subscribe to the connection manager event stream to
    /// prevent unexpected missed events.
    pub fn spawn(mut self) -> JoinHandle<Result<(), DhtConnectivityError>> {
        // Listen to events as early as possible
        let connectivity_events = self.connectivity.get_event_subscription();
        task::spawn(async move {
            debug!(target: LOG_TARGET, "Waiting for connectivity manager to start");
            if let Err(err) = self.connectivity.wait_started().await {
                error!(target: LOG_TARGET, "Comms connectivity failed to start: {err}");
            }
            match self.run(connectivity_events).await {
                Ok(_) => Ok(()),
                Err(err) => {
                    error!(target: LOG_TARGET, "DhtConnectivity exited with error: {err:?}");
                    Err(err)
                },
            }
        })
    }

    pub async fn run(mut self, mut connectivity_events: ConnectivityEventRx) -> Result<(), DhtConnectivityError> {
        // Initial discovery and refresh sync peers delay period, when a configured connection needs preference,
        // usually needed for the wallet to connect to its own base node first.
        if let Some(delay) = self.config.network_discovery.initial_peer_sync_delay {
            tokio::time::sleep(delay).await;
            debug!(target: LOG_TARGET, "DHT connectivity starting after delayed for {delay:.0?}");
        }
        self.refresh_neighbour_pool(true).await?;

        let mut ticker = time::interval(self.config.connectivity.update_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                Ok(event) = connectivity_events.recv() => {
                    if let Err(err) = self.handle_connectivity_event(event).await {
                        error!(target: LOG_TARGET, "Error handling connectivity event: {err:?}");
                    }
                    if self.connection_handles.is_empty() {
                        debug!(target: LOG_TARGET, "No active peer connections detected. Triggering aggressive peer pool refresh.");
                        if let Err(err) = self.refresh_peer_pools(true).await {
                             error!(target: LOG_TARGET, "Error during aggressive peer pool refresh: {err:?}");
                        }
                    }
               },

               Ok(event) = self.dht_events.recv() => {
                    if let Err(err) = self.handle_dht_event(&event).await {
                        error!(target: LOG_TARGET, "Error handling DHT event: {err:?}");
                    }
               },

               _ = ticker.tick() => {
                    if let Err(err) = self.check_and_ban_flooding_peers().await {
                        error!(target: LOG_TARGET, "Error checking for peer flooding: {err:?}");
                    }
                    if let Err(err) = self.refresh_neighbour_pool_if_required().await {
                        error!(target: LOG_TARGET, "Error refreshing neighbour peer pool: {err:?}");
                    }
                    if let Err(err) = self.refresh_random_pool_if_required().await {
                        error!(target: LOG_TARGET, "Error refreshing random peer pool: {err:?}");
                    }
                    self.log_status();
                    if let Err(err) = self.check_minimum_required_tcp_nodes().await {
                        error!(target: LOG_TARGET, "Error checking minimum required TCP nodes: {err:?}");
                    }
               },

               _ = self.shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "DhtConnectivity shutting down because the shutdown signal was received");
                    break;
               }
            }
        }

        Ok(())
    }

    async fn check_minimum_required_tcp_nodes(&mut self) -> Result<(), DhtConnectivityError> {
        let desired_ratio = self.config.connectivity.minimum_desired_tcpv4_node_ratio;
        if desired_ratio == 0.0 {
            return Ok(());
        }

        let conns = self
            .connectivity
            .select_connections(ConnectivitySelection::all_nodes(vec![]))
            .await?;
        if conns.len() <= 1 {
            return Ok(());
        }

        let num_tcp_nodes = conns
            .iter()
            .filter(|conn| {
                let ip = conn.address().iter().next();
                let tcp = conn.address().iter().nth(2);
                matches!(ip, Some(multiaddr::Protocol::Ip4(_))) && matches!(tcp, Some(multiaddr::Protocol::Tcp(_)))
            })
            .count();

        let current_ratio = num_tcp_nodes as f32 / conns.len() as f32;
        if current_ratio < desired_ratio {
            debug!(
                target: LOG_TARGET,
                "{:.1?}% of this node's {} connections are using TCPv4. This node requires at least {:.1?}% of nodes \
                 to be TCP nodes.",
                (current_ratio * 100.0).round(),
                conns.len(),
                (desired_ratio * 100.0).round(),
            );
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
                        .connectivity
                        .high_failure_rate_cooldown
                        .saturating_sub(ts.elapsed())
                ))
                .unwrap_or_default(),
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
                if info.num_new_peers > 0 {
                    self.refresh_peer_pools(false).await?;
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
                "Banning peer `{peer}` because of flooding. Message rate: {mps:.2}m/s"
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

    async fn refresh_peer_pools(&mut self, try_revive_connections: bool) -> Result<(), DhtConnectivityError> {
        info!(
            target: LOG_TARGET,
            "Reinitializing neighbour pool. (size={})",
            self.neighbours.len(),
        );

        self.refresh_neighbour_pool(try_revive_connections).await?;
        self.refresh_random_pool().await?;

        Ok(())
    }

    async fn refresh_neighbour_pool_if_required(&mut self) -> Result<(), DhtConnectivityError> {
        if self.num_connected_neighbours() < self.config.num_neighbouring_nodes {
            self.refresh_neighbour_pool(false).await?;
        }

        Ok(())
    }

    fn num_connected_neighbours(&self) -> usize {
        self.neighbours
            .iter()
            .filter(|peer| self.connection_handles.iter().any(|c| c.peer_node_id() == *peer))
            .count()
    }

    fn connected_pool_peers_iter(&self) -> impl Iterator<Item = &NodeId> {
        self.connection_handles.iter().map(|c| c.peer_node_id())
    }

    async fn refresh_neighbour_pool(&mut self, try_revive_connections: bool) -> Result<(), DhtConnectivityError> {
        self.remove_unmanaged_peers_from_pools().await?;
        let mut new_neighbours = self
            .fetch_neighbouring_peers(self.config.num_neighbouring_nodes, &[], try_revive_connections)
            .await?;

        if new_neighbours.is_empty() {
            debug!(
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
            "Adding {} neighbouring peer(s), removing {} peers: {}",
            new_neighbours.len(),
            difference.len(),
            new_neighbours
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );

        new_neighbours.iter().cloned().for_each(|peer| {
            self.insert_neighbour(peer);
        });
        self.dial_multiple_peers(&new_neighbours).await?;

        Ok(())
    }

    async fn dial_multiple_peers(&self, peers_to_dial: &[NodeId]) -> Result<(), DhtConnectivityError> {
        if !peers_to_dial.is_empty() {
            self.connectivity.request_many_dials(peers_to_dial.to_vec()).await?;
        }

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
            self.dial_multiple_peers(&to_redial).await?;
        }

        Ok(())
    }

    async fn refresh_random_pool_if_required(&mut self) -> Result<(), DhtConnectivityError> {
        let should_refresh = self.config.num_random_nodes > 0 &&
            self.random_pool_last_refresh
                .map(|instant| instant.elapsed() >= self.config.connectivity.random_pool_refresh_interval)
                .unwrap_or(true);
        if should_refresh {
            self.refresh_random_pool().await?;
        }

        Ok(())
    }

    async fn refresh_random_pool(&mut self) -> Result<(), DhtConnectivityError> {
        self.remove_unmanaged_peers_from_pools().await?;
        let mut exclude = self.neighbours.clone();
        if self.config.minimize_connections {
            exclude.extend(self.previous_random.iter().cloned());
        }
        let mut random_peers = self.fetch_random_peers(self.config.num_random_nodes, &exclude).await?;
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
            "Random peers: Adding = {random_peers:?}, Removing = {difference:?}"

        );
        for peer in &random_peers {
            self.insert_random_peer(peer.clone());
        }
        // Drop any connection handles that removed from the random pool
        difference.iter().for_each(|peer| {
            self.remove_connection_handle(peer);
        });
        self.dial_multiple_peers(&random_peers).await?;

        self.random_pool_last_refresh = Some(Instant::now());
        Ok(())
    }

    async fn handle_new_peer_connected(&mut self, conn: PeerConnection) -> Result<(), DhtConnectivityError> {
        self.remove_unmanaged_peers_from_pools().await?;
        if conn.peer_features().is_client() {
            debug!(
                target: LOG_TARGET,
                "Client node '{}' connected",
                conn.peer_node_id().short_str()
            );
            return Ok(());
        }

        if self.is_allow_list_peer(conn.peer_node_id()).await? {
            debug!(
                target: LOG_TARGET,
                "Unmanaged peer '{}' connected",
                conn.peer_node_id()
            );
            return Ok(());
        }

        if self.is_pool_peer(conn.peer_node_id()) {
            debug!(
                target: LOG_TARGET,
                "Added pool peer '{}' to connection handles",
                conn.peer_node_id()
            );
            self.insert_connection_handle(conn);
            return Ok(());
        }

        if self.neighbours.len() < self.config.num_neighbouring_nodes {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' connected. Adding to neighbour pool.",
                conn.peer_node_id().short_str()
            );
            self.neighbours.push(conn.peer_node_id().clone());
            self.insert_connection_handle(conn);
        }

        Ok(())
    }

    async fn pool_peers_with_active_connections(&self) -> Result<Vec<Peer>, DhtConnectivityError> {
        let peer_list = self
            .connection_handles
            .iter()
            .map(|conn| conn.peer_node_id())
            .cloned()
            .collect::<Vec<_>>();
        let peers = self.peer_manager.get_peers_by_node_ids(&peer_list).await?;
        debug!(
            target: LOG_TARGET,
            "minimize_connections: Filtered peers: {}, Handles: {}",
            peers.len(),
            self.connection_handles.len(),
        );
        Ok(peers)
    }

    async fn minimize_connections(&mut self) -> Result<(), DhtConnectivityError> {
        // Retrieve all communication node peers with an active connection status
        let mut peers_by_distance = self.pool_peers_with_active_connections().await?;
        let peer_allow_list = self.peer_allow_list().await?;
        peers_by_distance.retain(|p| !peer_allow_list.contains(&p.node_id));

        // Remove all above threshold connections
        let threshold = self.config.num_neighbouring_nodes + self.config.num_random_nodes;
        for peer in peers_by_distance.iter_mut().skip(threshold) {
            debug!(
                target: LOG_TARGET,
                "minimize_connections: Disconnecting '{}' because the node is not among the {} managed peers",
                peer.node_id,
                threshold
            );
            self.replace_pool_peer(&peer.node_id).await?;
            self.remove_connection_handle(&peer.node_id);
        }

        Ok(())
    }

    fn insert_connection_handle(&mut self, conn: PeerConnection) {
        // Remove any existing connection for this peer
        self.remove_connection_handle(conn.peer_node_id());
        trace!(target: LOG_TARGET, "Insert new peer connection {conn}" );
        self.connection_handles.push(conn);
    }

    fn remove_connection_handle(&mut self, node_id: &NodeId) {
        if let Some(idx) = self.connection_handles.iter().position(|c| c.peer_node_id() == node_id) {
            let conn = self.connection_handles.swap_remove(idx);
            trace!(target: LOG_TARGET, "Removing peer connection {conn}" );
        }
    }

    async fn handle_connectivity_event(&mut self, event: ConnectivityEvent) -> Result<(), DhtConnectivityError> {
        #[allow(clippy::enum_glob_use)]
        use ConnectivityEvent::*;
        debug!(target: LOG_TARGET, "Connectivity event: {event}");
        match event {
            PeerConnected(conn) => {
                self.handle_new_peer_connected(*conn.clone()).await?;
                trace!(
                    target: LOG_TARGET,
                    "Peer: node_id '{}', allow_list '{}', connected '{}'",
                    conn.peer_node_id(),
                    self.is_allow_list_peer(conn.peer_node_id()).await?,
                    conn.is_connected(),
                );

                if self.config.minimize_connections {
                    self.minimize_connections().await?;
                }
            },
            PeerConnectFailed(node_id) => {
                self.connection_handles.retain(|c| *c.peer_node_id() != node_id);
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{node_id}`. Metric collector is shut down."
                    );
                };
                self.remove_unmanaged_peers_from_pools().await?;
                if !self.is_pool_peer(&node_id) {
                    debug!(target: LOG_TARGET, "{node_id} is not managed by the DHT. Ignoring");
                    return Ok(());
                }
                self.replace_pool_peer(&node_id).await?;
                self.log_status();
            },
            PeerDisconnected(node_id, minimized) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer: node_id '{}', allow_list '{}', connected 'false'",
                    node_id,
                    self.is_allow_list_peer(&node_id).await?,
                );
                self.connection_handles.retain(|c| *c.peer_node_id() != node_id);
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{node_id}`. Metric collector is shut down."
                    );
                };
                self.remove_unmanaged_peers_from_pools().await?;
                if !self.is_pool_peer(&node_id) {
                    debug!(target: LOG_TARGET, "{node_id} is not managed by the DHT. Ignoring");
                    return Ok(());
                }
                if minimized == Minimized::Yes || self.config.minimize_connections {
                    debug!(
                        target: LOG_TARGET,
                        "Peer '{node_id}' was disconnected because it was minimized, will not reconnect."

                    );
                    // Remove from managed pool if applicable
                    self.replace_pool_peer(&node_id).await?;
                    // In case the connections was not managed, remove the connection handle
                    self.remove_connection_handle(&node_id);
                    return Ok(());
                }
                debug!(target: LOG_TARGET, "Pool peer {node_id} disconnected. Redialling...");
                // Attempt to reestablish the lost connection to the pool peer. If reconnection fails,
                // it is replaced with another peer (replace_pool_peer via PeerConnectFailed)
                self.dial_multiple_peers(&[node_id]).await?;
            },
            ConnectivityStateOnline(n) => {
                self.refresh_peer_pools(false).await?;
                if self.config.auto_join && self.should_send_join() {
                    debug!(
                        target: LOG_TARGET,
                        "Node is online ({n} peer(s) connected). Sending network join message."
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
                tokio::time::sleep(Duration::from_secs(15)).await;
                self.refresh_peer_pools(true).await?;
            },
            _ => {},
        }

        Ok(())
    }

    async fn peer_allow_list(&mut self) -> Result<Vec<NodeId>, DhtConnectivityError> {
        Ok(self.connectivity.get_allow_list().await?)
    }

    async fn all_connected_comms_nodes(&mut self) -> Result<Vec<NodeId>, DhtConnectivityError> {
        let all_connections = self
            .connectivity
            .select_connections(ConnectivitySelection::all_nodes(vec![]))
            .await?;
        let comms_nodes = all_connections
            .iter()
            .filter(|p| p.peer_features().is_node())
            .map(|p| p.peer_node_id().clone())
            .collect();
        Ok(comms_nodes)
    }

    async fn replace_pool_peer(&mut self, current_peer: &NodeId) -> Result<(), DhtConnectivityError> {
        self.remove_unmanaged_peers_from_pools().await?;
        if self.is_allow_list_peer(current_peer).await? {
            debug!(
                target: LOG_TARGET,
                "Peer '{current_peer}' is on the allow list, ignoring replacement."

            );
            return Ok(());
        }

        if self.random_pool.contains(current_peer) {
            let mut exclude = self.get_pool_peers();
            if self.config.minimize_connections {
                exclude.extend(self.previous_random.iter().cloned());
                self.previous_random.push(current_peer.clone());
            }

            self.random_pool.retain(|n| n != current_peer);
            self.remove_connection_handle(current_peer);

            debug!(
                target: LOG_TARGET,
                "Peer '{current_peer}' in random pool is unavailable. Adding a new random peer if possible"
            );
            match self.fetch_random_peers(1, &exclude).await?.pop() {
                Some(new_peer) => {
                    self.insert_random_peer(new_peer.clone());
                    self.dial_multiple_peers(&[new_peer]).await?;
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

            self.neighbours.retain(|n| n != current_peer);
            self.remove_connection_handle(current_peer);

            debug!(
                target: LOG_TARGET,
                "Peer '{current_peer}' in neighbour pool is offline. Adding a new peer if possible"
            );
            match self.fetch_neighbouring_peers(1, &exclude, false).await?.pop() {
                Some(new_peer) => {
                    self.insert_neighbour(new_peer.clone());
                    self.dial_multiple_peers(&[new_peer]).await?;
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

        self.log_status();

        Ok(())
    }

    fn insert_neighbour(&mut self, node_id: NodeId) -> Option<NodeId> {
        self.neighbours.push(node_id);
        if self.neighbours.len() > self.config.num_neighbouring_nodes {
            self.neighbours.pop()
        } else {
            None
        }
    }

    fn insert_random_peer(&mut self, node_id: NodeId) {
        self.random_pool.push(node_id);
        if self.random_pool.len() > self.config.num_random_nodes &&
            let Some(removed_peer) = self.random_pool.pop() &&
            self.config.minimize_connections
        {
            self.previous_random.push(removed_peer.clone());
        }
    }

    async fn remove_unmanaged_peers_from_pools(&mut self) -> Result<(), DhtConnectivityError> {
        self.remove_allow_list_peers_from_pools().await?;
        self.remove_exlcuded_peers_from_pools().await
    }

    async fn remove_allow_list_peers_from_pools(&mut self) -> Result<(), DhtConnectivityError> {
        let allow_list = self.peer_allow_list().await?;
        self.neighbours.retain(|n| !allow_list.contains(n));
        self.random_pool.retain(|n| !allow_list.contains(n));
        Ok(())
    }

    async fn remove_exlcuded_peers_from_pools(&mut self) -> Result<(), DhtConnectivityError> {
        if !self.config.excluded_dial_addresses.is_empty() {
            let mut neighbours = Vec::with_capacity(self.neighbours.len());
            for peer in &self.neighbours {
                if let Ok(addresses) = self.peer_manager.get_peer_multi_addresses(peer).await &&
                    !addresses.iter().all(|addr| {
                        self.config
                            .excluded_dial_addresses
                            .iter()
                            .any(|v| v.contains(addr.address()))
                    })
                {
                    neighbours.push(peer.clone());
                }
            }
            self.neighbours = neighbours;

            let mut random_pool = Vec::with_capacity(self.random_pool.len());
            for peer in &self.random_pool {
                if let Ok(addresses) = self.peer_manager.get_peer_multi_addresses(peer).await &&
                    !addresses.iter().all(|addr| {
                        self.config
                            .excluded_dial_addresses
                            .iter()
                            .any(|v| v.contains(addr.address()))
                    })
                {
                    random_pool.push(peer.clone());
                }
            }
            self.random_pool = random_pool;
        }
        Ok(())
    }

    async fn is_allow_list_peer(&mut self, node_id: &NodeId) -> Result<bool, DhtConnectivityError> {
        Ok(self.peer_allow_list().await?.contains(node_id))
    }

    fn is_pool_peer(&self, node_id: &NodeId) -> bool {
        self.neighbours.contains(node_id) || self.random_pool.contains(node_id)
    }

    fn get_pool_peers(&self) -> Vec<NodeId> {
        self.neighbours.iter().chain(self.random_pool.iter()).cloned().collect()
    }

    async fn fetch_neighbouring_peers(
        &mut self,
        n: usize,
        excluded: &[NodeId],
        _try_revive_connections: bool,
    ) -> Result<Vec<NodeId>, DhtConnectivityError> {
        let peer_allow_list = self.peer_allow_list().await?;
        let mut excluded = excluded.to_vec();
        excluded.extend(peer_allow_list);
        excluded.extend(self.connected_pool_peers_iter().cloned());

        let peers = self
            .peer_manager
            .discovery_syncing(n, &excluded, Some(PeerFeatures::COMMUNICATION_NODE), true)
            .await?;

        Ok(peers.into_iter().map(|p| p.node_id).collect())
    }

    async fn fetch_random_peers(&mut self, n: usize, excluded: &[NodeId]) -> Result<Vec<NodeId>, DhtConnectivityError> {
        let mut excluded = excluded.to_vec();
        excluded.extend(self.peer_allow_list().await?);
        let peers = self.peer_manager.random_peers(n, &excluded, None).await?;
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
