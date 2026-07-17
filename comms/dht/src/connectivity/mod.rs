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
//! Responsible for ensuring DHT network connectivity to a random peer set. This includes joining the
//! network when the node has established some peer connections (e.g to seed peers). It maintains a
//! random peer pool and instructs the comms `ConnectivityManager` to establish those connections. Once a configured
//! percentage of these peers is online, the node is established on the DHT network.
//!
//! The DHT connectivity actor monitors the connectivity state (using `ConnectivityEvent`s) and attempts
//! to maintain connectivity to the network as peers come and go.

#[cfg(test)]
mod test;

mod metrics;
use std::{
    cmp,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
pub use metrics::{MetricsCollector, MetricsCollectorHandle};
use rand::RngExt;
use tari_comms::{
    Minimized,
    PeerConnection,
    PeerConnectionError,
    PeerManager,
    connection_manager::ConnectionDirection,
    connectivity::{
        ConnectivityError,
        ConnectivityEvent,
        ConnectivityEventRx,
        ConnectivityRequester,
        ConnectivitySelection,
    },
    multiaddr,
    peer_manager::{NodeId, Peer, PeerManagerError},
};
use tari_shutdown::ShutdownSignal;
use thiserror::Error;
use tokio::{sync::broadcast, task, task::JoinHandle, time, time::MissedTickBehavior};

use crate::{DhtActorError, DhtConfig, DhtRequester, connectivity::metrics::MetricsError, event::DhtEvent};

const LOG_TARGET: &str = "comms::dht::connectivity";

/// How long a peer may sit in the pool with an in-flight dial before we give up on it and free the
/// slot. A dial that neither connects nor reports a failure within this window would otherwise
/// occupy a pool slot indefinitely.
const PENDING_DIAL_GRACE: Duration = Duration::from_secs(60);

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
    #[error("Peer connection error: {0}")]
    PeerConnectionError(#[from] PeerConnectionError),
}

/// DHT connectivity actor.
pub(crate) struct DhtConnectivity {
    config: Arc<DhtConfig>,
    peer_manager: Arc<PeerManager>,
    connectivity: ConnectivityRequester,
    dht_requester: DhtRequester,
    /// A randomly-selected set of peers managed by this actor.
    random_pool: Vec<NodeId>,
    /// Used to track when the random peer pool was last refreshed
    random_pool_last_refresh: Option<Instant>,
    /// Peers in the pool that have been dialed but have not yet connected or reported a failure,
    /// and when the dial was issued. A pool entry without a connection handle is only meaningful
    /// when paired with this: it distinguishes "still dialing" from "stale slot".
    pending_dials: HashMap<NodeId, Instant>,
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
        connectivity: ConnectivityRequester,
        dht_requester: DhtRequester,
        dht_events: broadcast::Receiver<Arc<DhtEvent>>,
        metrics_collector: MetricsCollectorHandle,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let pool_size = config.num_neighbouring_nodes + config.num_random_nodes;
        Self {
            random_pool: Vec::with_capacity(pool_size),
            pending_dials: HashMap::with_capacity(pool_size),
            connection_handles: Vec::with_capacity(pool_size),
            config,
            peer_manager,
            connectivity,
            dht_requester,
            metrics_collector,
            random_pool_last_refresh: None,
            stats: Stats::new(),
            dht_events,
            cooldown_in_effect: None,
            shutdown_signal,
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
        self.refresh_random_pool().await?;

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
        let pool_size = self.config.num_neighbouring_nodes + self.config.num_random_nodes;
        let (pool_connected, pool_pending) = self
            .random_pool
            .iter()
            .partition::<Vec<_>, _>(|peer| self.connection_handles.iter().any(|c| c.peer_node_id() == *peer));
        debug!(
            target: LOG_TARGET,
            "DHT connectivity status: {}peer pool: {}/{} ({} connected, last refreshed {}), active DHT connections: \
             {}/{}",
            self.cooldown_in_effect
                .map(|ts| format!(
                    "COOLDOWN({:.2?} remaining) ",
                    self.config
                        .connectivity
                        .high_failure_rate_cooldown
                        .saturating_sub(ts.elapsed())
                ))
                .unwrap_or_default(),
            self.random_pool.len(),
            pool_size,
            pool_connected.len(),
            self.random_pool_last_refresh
                .map(|i| format!("{:.0?} ago", i.elapsed()))
                .unwrap_or_else(|| "<never>".to_string()),
            self.connection_handles.len(),
            pool_size,
        );
        if !pool_pending.is_empty() {
            debug!(
                target: LOG_TARGET,
                "Pending connections: {}",
                pool_pending
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }

    async fn handle_dht_event(&mut self, event: &DhtEvent) -> Result<(), DhtConnectivityError> {
        #[allow(clippy::single_match)]
        match event {
            DhtEvent::NetworkDiscoveryPeersAdded(info) if info.num_new_peers > 0 => {
                self.refresh_peer_pools(false).await?;
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

    async fn refresh_peer_pools(&mut self, refresh_entire_pool: bool) -> Result<(), DhtConnectivityError> {
        info!(
            target: LOG_TARGET,
            "Reinitializing peer pool. (size={}, try_revive_connections {refresh_entire_pool})",
            self.random_pool.len(),
        );

        if refresh_entire_pool {
            let should_refresh = self
                .random_pool_last_refresh
                .map(|instant| instant.elapsed() >= self.config.connectivity.random_pool_refresh_interval)
                .unwrap_or(true);
            if should_refresh {
                self.refresh_entire_pool().await?;
            } else {
                debug!(
                    target: LOG_TARGET,
                    "Attempting to refresh entire pool, but time has not expired yet. Last refresh was {:.0?} ago.",
                    self.random_pool_last_refresh.map(|instant| instant.elapsed()).unwrap_or_default()
                );
            }
        } else {
            self.refresh_random_pool().await?;
        }

        Ok(())
    }

    async fn refresh_entire_pool(&mut self) -> Result<(), DhtConnectivityError> {
        let pool_size = self.config.num_neighbouring_nodes + self.config.num_random_nodes;
        // we have no peers, so we need to get peers, so lets double our chances of dialing twice the number we want, we
        // can close them later on And we only select known healty ones
        debug!(
            target: LOG_TARGET,
            "We have no connections asking for {} nodes",
            pool_size * 2,
        );
        let new_peers = self.fetch_random_peers(pool_size * 2, &Vec::new(), true).await?;
        debug!(
            target: LOG_TARGET,
            "Refreshing peer pool (#new = {})",
            new_peers.len(),
        );
        let old_peers = self.random_pool.drain(..).collect::<Vec<_>>();
        for peer in &new_peers {
            self.insert_random_peer(peer.clone());
        }
        // Make sure we drop any connection handles that removed from the pool
        for old_peer in &old_peers {
            self.remove_connection_handle(old_peer).await?;
        }
        self.dial_multiple_peers(&new_peers).await?;
        self.random_pool_last_refresh = Some(Instant::now());
        Ok(())
    }

    async fn dial_multiple_peers(&mut self, peers_to_dial: &[NodeId]) -> Result<(), DhtConnectivityError> {
        if !peers_to_dial.is_empty() {
            // Record the dial before issuing it: until the peer connects or fails it holds a pool
            // slot with no connection handle, and only this marks it as in flight rather than stale.
            let dialed_at = Instant::now();
            for peer in peers_to_dial {
                self.pending_dials.insert(peer.clone(), dialed_at);
            }
            self.connectivity.request_many_dials(peers_to_dial.to_vec()).await?;
        }

        Ok(())
    }

    async fn refresh_random_pool_if_required(&mut self) -> Result<(), DhtConnectivityError> {
        let pool_size = self.config.num_neighbouring_nodes + self.config.num_random_nodes;
        // Topping the pool back up is cheap (it dials only what is actually missing) and must happen
        // on every tick, otherwise a pool that loses peers stays short until the next refresh
        // interval — hours away. Only the churn is bound to `random_pool_refresh_interval`, and
        // `refresh_random_pool` gates that itself.
        if pool_size > 0 {
            self.refresh_random_pool().await?;
        }

        Ok(())
    }

    async fn refresh_random_pool(&mut self) -> Result<(), DhtConnectivityError> {
        let pool_size = self.config.num_neighbouring_nodes + self.config.num_random_nodes;

        // Churn deliberately drops healthy peers to explore new ones, so it may only run once per
        // refresh cycle. This function also runs on every `ConnectivityStateOnline` event (every few
        // seconds), where the only goal is to top the pool back up.
        let churn_due = self
            .random_pool_last_refresh
            .map(|instant| instant.elapsed() >= self.config.connectivity.random_pool_refresh_interval)
            .unwrap_or(true);

        // A pool entry only counts as connected when we hold a handle that is actually connected.
        // An entry with no handle at all is either still being dialed or is a stale slot; counting
        // those as connected makes a decaying pool look full, so we dial no replacements and
        // measure the churn floor against peers that do not exist.
        let live_peers = self
            .connection_handles
            .iter()
            .filter(|c| c.is_connected())
            .map(|c| c.peer_node_id().clone())
            .collect::<Vec<_>>();
        let (mut connected, remainder) = self
            .random_pool
            .drain(..)
            .partition::<Vec<_>, _>(|n| live_peers.contains(n));

        // Of the peers we hold no live connection to, keep the ones whose dial is still in flight —
        // dropping those would cancel dials that have not had time to complete. Everything else is a
        // stale slot and is released so it can be replaced below.
        let pending_dials = &self.pending_dials;
        let (mut pending, mut disconnected_peers) = remainder.into_iter().partition::<Vec<_>, _>(|n| {
            pending_dials
                .get(n)
                .is_some_and(|dialed_at| dialed_at.elapsed() < PENDING_DIAL_GRACE)
        });

        // The pool holds at most `pool_size` slots and live connections take precedence, so surplus
        // dials are released. `refresh_entire_pool` deliberately dials twice `pool_size` to improve
        // its odds of filling the pool, and without this those extra slots are never given back:
        // the pool stays oversubscribed, `needed` is always zero and no further peers are dialed.
        let free_slots = pool_size.saturating_sub(connected.len());
        if pending.len() > free_slots {
            disconnected_peers.extend(pending.split_off(free_slots));
        }

        // We add at most `churn_rate`% new peers per refresh (or at least 1), and only start
        // churning healthy peers once the pool has more connected peers than the churn floor.
        let max_churn = cmp::max(pool_size * self.config.connectivity.churn_rate / 100, 1);
        let keep_size = pool_size.saturating_sub(max_churn);
        // The pool is only healthy enough to churn/explore once we hold more than the churn floor
        // of connected peers. Below that we are short on peers.
        let is_pool_healthy = connected.len() > keep_size;
        let should_churn = churn_due && is_pool_healthy;

        info!(
            target: LOG_TARGET,
            "refreshing random peer list, asking for {pool_size} peers (is_pool_healthy = {is_pool_healthy}, \
             churn_due = {churn_due}, #connected = {}, #pending = {}, #stale = {})",
            connected.len(),
            pending.len(),
            disconnected_peers.len(),
        );

        if should_churn {
            // this is safety counter so we dont loop endlessly here
            let mut disconnect_counter = connected.len() / 2;
            while connected.len() > keep_size && disconnect_counter > 0 {
                disconnect_counter = disconnect_counter.saturating_sub(1);
                // we remove a random peer so as not to keep swapping the same peer each time.
                let mut rng = rand::rng();
                let index = rng.random_range(0..connected.len());
                if self
                    .connection_handles
                    .iter()
                    .any(|c| c.peer_node_id() == connected.get(index).expect("should exist") && c.is_strongly_held())
                {
                    debug!(
                        target: LOG_TARGET,
                        "Skipping disconnect of peer '{}' because it is strongly held",
                        connected.get(index).expect("should exist").short_str()
                    );
                    continue;
                }
                let disconnect_peer = connected.swap_remove(index);
                disconnected_peers.push(disconnect_peer);
            }
        }

        // Peers we hold, are dialing, or have just released are all poor dial candidates: the first
        // two would become duplicate pool entries and the last we dropped on purpose.
        let mut exclude = connected.clone();
        exclude.extend(pending.iter().cloned());
        exclude.extend(disconnected_peers.iter().cloned());

        // Only the slots we hold neither a connection nor an in-flight dial for need filling.
        let needed = pool_size.saturating_sub(connected.len() + pending.len());
        // When the pool is healthy we can afford to probe/revive arbitrary (possibly never-seen)
        // peers. When we are short on peers we prefer known-good peers (ones we have successfully
        // connected to before) instead of wasting dials on never-seen/dead peers.
        let mut new_peers = if needed == 0 {
            Vec::new()
        } else if should_churn {
            self.fetch_random_peers(needed, &exclude, false).await?
        } else {
            // Prefer known-good peers, but fall back to any peers if we don't have enough known-good
            // ones (e.g. fresh start or after being offline, when every known-good peer may itself be
            // offline). Without this fallback the node could get stuck never dialling anyone and never
            // recover to a healthy state or discover new peers.
            let mut peers = self.fetch_random_peers(needed, &exclude, true).await?;
            if peers.len() < needed {
                let remaining = needed - peers.len();
                debug!(
                    target: LOG_TARGET,
                    "Only {} known-good peer(s) available, falling back to {remaining} any-status peer(s)",
                    peers.len(),
                );
                let mut exclude_any = exclude.clone();
                exclude_any.extend(peers.clone());
                let extra = self.fetch_random_peers(remaining, &exclude_any, false).await?;
                peers.extend(extra);
            }
            peers
        };
        new_peers.truncate(needed);

        // In-flight dials keep their slot; only connected and pending peers stay in the pool.
        self.random_pool = connected;
        self.random_pool.extend(pending);
        debug!(
            target: LOG_TARGET,
            "Adding new peers to peer pool (#new = {}, #keeping = {})",
            new_peers.len(),
            self.random_pool.len(),
        );
        for peer in &new_peers {
            self.insert_random_peer(peer.clone());
        }
        // Drop any connection handles that were removed from the pool
        for peer_to_disconnect in &disconnected_peers {
            self.pending_dials.remove(peer_to_disconnect);
            self.remove_connection_handle(peer_to_disconnect).await?;
        }
        // Clean up any connection handles not in the current pool (e.g. accumulated between refresh cycles)
        let stale_handles: Vec<NodeId> = self
            .connection_handles
            .iter()
            .filter(|c| {
                !self.random_pool.contains(c.peer_node_id()) && c.direction().is_outbound() && !c.is_strongly_held()
            })
            .map(|c| c.peer_node_id().clone())
            .collect();
        for node_id in &stale_handles {
            debug!(
                target: LOG_TARGET,
                "Removing stale connection handle for '{}' (not in current pool)",
                node_id.short_str()
            );
            self.remove_connection_handle(node_id).await?;
        }
        self.dial_multiple_peers(&new_peers).await?;

        // Forget dials for peers that have since left the pool, or that outlived the grace period
        // without resolving either way.
        let random_pool = &self.random_pool;
        self.pending_dials
            .retain(|node_id, dialed_at| random_pool.contains(node_id) && dialed_at.elapsed() < PENDING_DIAL_GRACE);

        // Only advance the cycle when we actually churned. This function runs on every
        // `ConnectivityStateOnline` event, so stamping it unconditionally would push the deadline
        // out of reach and the exploration churn would never come due. A churn skipped because the
        // pool was unhealthy has not happened, so it stays due and runs once the pool recovers.
        if should_churn {
            self.random_pool_last_refresh = Some(Instant::now());
        }
        Ok(())
    }

    async fn handle_new_peer_connected(&mut self, conn: PeerConnection) -> Result<(), DhtConnectivityError> {
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

        // Peers currently held by a strong reference (e.g. an in-progress sync) must not be
        // disconnected by pool pruning. The strong-count is shared across all clones of the
        // PeerConnection — including the one delivered to us in this event — so consulting it
        // directly is the source of truth.
        if conn.is_strongly_held() {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' connected — leaving connection alone while a strong handle is held \
                 (strong_count={})",
                conn.peer_node_id(),
                conn.strong_count(),
            );
            return Ok(());
        }

        if self.is_pool_peer(conn.peer_node_id()) {
            debug!(
                target: LOG_TARGET,
                "Added pool peer '{}' to connection handles",
                conn.peer_node_id()
            );
            self.insert_connection_handle(conn).await?;
            return Ok(());
        }

        let pool_size = self.config.num_neighbouring_nodes + self.config.num_random_nodes;
        if self.connected_random_pool_peers() < pool_size {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' connected. Adding to peer pool.",
                conn.peer_node_id().short_str()
            );
            self.random_pool.push(conn.peer_node_id().clone());
            self.insert_connection_handle(conn).await?;
            return Ok(());
        }
        debug!(
            target: LOG_TARGET,
            "Peer '{}' connected with direction: {} and is strongly connected: {}, but peer pool is full ({}/{})",
            conn.peer_node_id().short_str(),
            conn.direction(),
            conn.is_strongly_held(),
            self.random_pool.len(),
            pool_size
        );
        if conn.direction() == ConnectionDirection::Outbound && !conn.is_strongly_held() {
            debug!(
                target: LOG_TARGET,
                "Peer '{}' is an outbound connection, disconnecting it. Peer pool is full ({}/{})",
                conn.peer_node_id().short_str(),
                self.random_pool.len(),
                pool_size
            );
            let mut conn = conn;
            if let Err(err) = conn.disconnect(Minimized::Yes, "DhtConnectivity pool full").await {
                debug!(
                target: LOG_TARGET,
                "Failed to disconnect excess peer '{}': {:?}",
                conn.peer_node_id().short_str(),
                err
                );
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "Cant disconnect peer '{}', so adding to peer pool to be managed",
                conn.peer_node_id().short_str()
            );
            self.random_pool.push(conn.peer_node_id().clone());
            self.insert_connection_handle(conn).await?;
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
        // Connections currently pinned by a strong handle (e.g. an in-progress sync) are
        // identified via the per-connection strong-count rather than an out-of-band list.
        let strongly_held: std::collections::HashSet<NodeId> = self
            .connection_handles
            .iter()
            .filter(|c| c.is_strongly_held())
            .map(|c| c.peer_node_id().clone())
            .collect();
        peers_by_distance.retain(|p| !peer_allow_list.contains(&p.node_id) && !strongly_held.contains(&p.node_id));

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
            self.remove_connection_handle(&peer.node_id).await?;
        }

        Ok(())
    }

    async fn insert_connection_handle(&mut self, conn: PeerConnection) -> Result<(), PeerConnectionError> {
        // Remove any existing connection for this peer
        self.remove_connection_handle(conn.peer_node_id()).await?;
        trace!(target: LOG_TARGET, "Insert new peer connection {conn}" );
        // The dial resolved — the handle is now the source of truth for this peer.
        self.pending_dials.remove(conn.peer_node_id());
        self.connection_handles.push(conn);
        Ok(())
    }

    async fn remove_connection_handle(&mut self, node_id: &NodeId) -> Result<(), PeerConnectionError> {
        if let Some(idx) = self.connection_handles.iter().position(|c| c.peer_node_id() == node_id) {
            let mut conn = self.connection_handles.swap_remove(idx);
            if conn.is_connected() {
                conn.disconnect(Minimized::No, "DhtConnectivty").await?;
            }
            trace!(target: LOG_TARGET, "Removing peer connection {conn}" );
        }
        Ok(())
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
                self.pending_dials.remove(&node_id);
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{node_id}`. Metric collector is shut down."
                    );
                };
                if !self.is_pool_peer(&node_id) {
                    debug!(target: LOG_TARGET, "{node_id} is not managed by the DHT. Ignoring");
                    return Ok(());
                }
                self.replace_pool_peer(&node_id).await?;
                self.log_status();
            },
            PeerDisconnected(node_id, _) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer: node_id '{}', allow_list '{}', connected 'false'",
                    node_id,
                    self.is_allow_list_peer(&node_id).await?,
                );
                self.connection_handles.retain(|c| *c.peer_node_id() != node_id);
                self.pending_dials.remove(&node_id);
                if self.metrics_collector.clear_metrics(node_id.clone()).await.is_err() {
                    debug!(
                        target: LOG_TARGET,
                        "Failed to clear metrics for peer `{node_id}`. Metric collector is shut down."
                    );
                };
                if !self.is_pool_peer(&node_id) {
                    debug!(target: LOG_TARGET, "{node_id} is not managed by the DHT. Ignoring");
                    return Ok(());
                }
                debug!(target: LOG_TARGET, "Pool peer {node_id} disconnected. Replacing with a new peer.");
                self.replace_pool_peer(&node_id).await?;
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

    async fn replace_pool_peer(&mut self, current_peer: &NodeId) -> Result<(), DhtConnectivityError> {
        if self.random_pool.contains(current_peer) {
            let exclude = self.get_pool_peers();

            self.random_pool.retain(|n| n != current_peer);
            self.pending_dials.remove(current_peer);
            self.remove_connection_handle(current_peer).await?;

            debug!(
                target: LOG_TARGET,
                "Peer '{current_peer}' in peer pool is unavailable. Adding a new peer if possible"
            );
            // Reactive backfill after a peer dropped: prefer known-good peers so we don't refill
            // the pool with never-seen/dead peers. Exploration of new peers happens in the periodic
            // `refresh_random_pool` churn instead.
            match self.fetch_random_peers(1, &exclude, true).await?.pop() {
                Some(new_peer) => {
                    self.insert_random_peer(new_peer.clone());
                    self.dial_multiple_peers(&[new_peer]).await?;
                },
                None => {
                    debug!(
                        target: LOG_TARGET,
                        "Unable to fetch new peer to replace disconnected peer '{}' because not enough peers \
                         are known. Pool size is {}.",
                        current_peer,
                        self.random_pool.len()
                    );
                },
            }
        }

        self.log_status();

        Ok(())
    }

    fn insert_random_peer(&mut self, node_id: NodeId) {
        // The pool is a set: a duplicate entry would consume a second slot for a peer that can only
        // ever hold one connection, inflating the pool past its configured size.
        if !self.random_pool.contains(&node_id) {
            self.random_pool.push(node_id);
        }
    }

    async fn is_allow_list_peer(&mut self, node_id: &NodeId) -> Result<bool, DhtConnectivityError> {
        Ok(self.peer_allow_list().await?.contains(node_id))
    }

    fn is_pool_peer(&self, node_id: &NodeId) -> bool {
        self.random_pool.contains(node_id)
    }

    fn connected_random_pool_peers(&self) -> usize {
        self.random_pool
            .iter()
            .filter(|node| {
                self.connection_handles
                    .iter()
                    .any(|c| c.is_connected() && c.peer_node_id() == *node)
            })
            .count()
    }

    fn get_pool_peers(&self) -> Vec<NodeId> {
        self.random_pool.clone()
    }

    async fn fetch_random_peers(
        &mut self,
        n: usize,
        excluded: &[NodeId],
        known_good: bool,
    ) -> Result<Vec<NodeId>, DhtConnectivityError> {
        let mut excluded = excluded.to_vec();
        excluded.extend(self.peer_allow_list().await?);
        let peers = self.peer_manager.random_peers(n, &excluded, None, known_good).await?;
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
