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
    collections::HashMap,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use log::*;
use nom::lib::std::collections::hash_map::Entry;
use tari_shutdown::ShutdownSignal;
use tokio::{sync::mpsc, task::JoinHandle, time, time::MissedTickBehavior};
use tracing::{span, Instrument, Level};

use super::{
    config::ConnectivityConfig,
    connection_pool::{ConnectionPool, ConnectionStatus},
    connection_stats::PeerConnectionStats,
    error::ConnectivityError,
    requester::{ConnectivityEvent, ConnectivityRequest},
    selection::ConnectivitySelection,
    ConnectivityEventTx,
};
use crate::{
    connection_manager::{
        ConnectionDirection,
        ConnectionManagerError,
        ConnectionManagerEvent,
        ConnectionManagerRequester,
    },
    peer_manager::NodeId,
    runtime::task,
    utils::datetime::format_duration,
    NodeIdentity,
    PeerConnection,
    PeerManager,
};

const LOG_TARGET: &str = "comms::connectivity::manager";

/// # Connectivity Manager
///
/// The ConnectivityManager actor is responsible for tracking the state of all peer
/// connections in the system and maintaining a _managed pool_ of peer connections.
/// It provides a simple interface to fetch active peer connections.
/// Selection includes selecting a single peer, random selection and selecting connections
/// closer to a `NodeId`.
///
/// Additionally, set of managed peers can be provided. ConnectivityManager actor will
/// attempt to ensure that all provided peers have active peer connections.
/// It emits [ConnectivityEvent](crate::connectivity::ConnectivityEvent)s that can keep client components
/// in the loop with the state of the node's connectivity.
pub struct ConnectivityManager {
    pub config: ConnectivityConfig,
    pub request_rx: mpsc::Receiver<ConnectivityRequest>,
    pub event_tx: ConnectivityEventTx,
    pub connection_manager: ConnectionManagerRequester,
    pub peer_manager: Arc<PeerManager>,
    pub node_identity: Arc<NodeIdentity>,
    pub shutdown_signal: ShutdownSignal,
}

impl ConnectivityManager {
    pub fn spawn(self) -> JoinHandle<()> {
        ConnectivityManagerActor {
            config: self.config,
            status: ConnectivityStatus::Initializing,
            request_rx: self.request_rx,
            connection_manager: self.connection_manager,
            peer_manager: self.peer_manager.clone(),
            event_tx: self.event_tx,
            connection_stats: HashMap::new(),
            node_identity: self.node_identity,
            pool: ConnectionPool::new(),
            shutdown_signal: self.shutdown_signal,
            #[cfg(feature = "metrics")]
            uptime: Some(Instant::now()),
            allow_list: vec![],
        }
        .spawn()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityStatus {
    Initializing,
    Online(usize),
    Degraded(usize),
    Offline,
}

impl ConnectivityStatus {
    is_fn!(is_initializing, ConnectivityStatus::Initializing);

    is_fn!(is_online, ConnectivityStatus::Online(_));

    is_fn!(is_offline, ConnectivityStatus::Offline);

    is_fn!(is_degraded, ConnectivityStatus::Degraded(_));

    pub fn num_connected_nodes(&self) -> usize {
        use ConnectivityStatus::*;
        match self {
            Initializing | Offline => 0,
            Online(n) | Degraded(n) => *n,
        }
    }
}

impl Default for ConnectivityStatus {
    fn default() -> Self {
        ConnectivityStatus::Initializing
    }
}

impl fmt::Display for ConnectivityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

struct ConnectivityManagerActor {
    config: ConnectivityConfig,
    status: ConnectivityStatus,
    request_rx: mpsc::Receiver<ConnectivityRequest>,
    connection_manager: ConnectionManagerRequester,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    event_tx: ConnectivityEventTx,
    connection_stats: HashMap<NodeId, PeerConnectionStats>,
    pool: ConnectionPool,
    shutdown_signal: ShutdownSignal,
    #[cfg(feature = "metrics")]
    uptime: Option<Instant>,
    allow_list: Vec<NodeId>,
}

impl ConnectivityManagerActor {
    pub fn spawn(self) -> JoinHandle<()> {
        let mut mdc = vec![];
        log_mdc::iter(|k, v| mdc.push((k.to_owned(), v.to_owned())));
        task::spawn(async {
            log_mdc::extend(mdc);
            Self::run(self).await
        })
    }

    #[tracing::instrument(level = "trace", name = "connectivity_manager_actor::run", skip(self))]
    pub async fn run(mut self) {
        debug!(target: LOG_TARGET, "ConnectivityManager started");

        let mut connection_manager_events = self.connection_manager.get_event_subscription();

        let interval = self.config.connection_pool_refresh_interval;
        let mut ticker = time::interval_at(
            Instant::now()
                .checked_add(interval)
                .expect("connection_pool_refresh_interval cause overflow")
                .into(),
            interval,
        );
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        self.publish_event(ConnectivityEvent::ConnectivityStateInitialized);

        loop {
            tokio::select! {
                Some(req) = self.request_rx.recv() => {
                    self.handle_request(req).await;
                },

                event = connection_manager_events.recv() => {
                    if let Ok(event) = event {
                        if let Err(err) = self.handle_connection_manager_event(&event).await {
                            error!(target:LOG_TARGET, "Error handling connection manager event: {:?}", err);
                        }
                    }
                },

                _ = ticker.tick() => {
                    self.cleanup_connection_stats();
                    if let Err(err) = self.refresh_connection_pool().await {
                        error!(target: LOG_TARGET, "Error when refreshing connection pools: {:?}", err);
                    }
                },

                _ = self.shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "ConnectivityManager is shutting down because it received the shutdown signal");
                    self.disconnect_all().await;
                    break;
                }
            }
        }
    }

    async fn handle_request(&mut self, req: ConnectivityRequest) {
        use ConnectivityRequest::*;
        trace!(target: LOG_TARGET, "Request: {:?}", req);
        match req {
            WaitStarted(reply) => {
                let _ = reply.send(());
            },
            GetConnectivityStatus(reply) => {
                let _ = reply.send(self.status);
            },
            DialPeer { node_id, reply_tx } => {
                let tracing_id = tracing::Span::current().id();
                let span = span!(Level::TRACE, "handle_request");
                span.follows_from(tracing_id);
                async move {
                    match self.peer_manager.is_peer_banned(&node_id).await {
                        Ok(true) => {
                            if let Some(reply) = reply_tx {
                                let _ = reply.send(Err(ConnectionManagerError::PeerBanned));
                            }
                            return;
                        },
                        Ok(false) => {},
                        Err(err) => {
                            if let Some(reply) = reply_tx {
                                let _ = reply.send(Err(err.into()));
                            }
                            return;
                        },
                    }
                    match self.pool.get(&node_id) {
                        Some(state) if state.is_connected() => {
                            debug!(
                                target: LOG_TARGET,
                                "Found existing connection for peer `{}`",
                                node_id.short_str()
                            );
                            if let Some(reply_tx) = reply_tx {
                                let _ = reply_tx.send(Ok(state.connection().cloned().expect("Already checked")));
                            }
                        },
                        _ => {
                            debug!(
                                target: LOG_TARGET,
                                "No existing connection found for peer `{}`. Dialing...",
                                node_id.short_str()
                            );
                            if let Err(err) = self.connection_manager.send_dial_peer(node_id, reply_tx).await {
                                error!(
                                    target: LOG_TARGET,
                                    "Failed to send dial request to connection manager: {:?}", err
                                );
                            }
                        },
                    }
                }
                .instrument(span)
                .await
            },
            SelectConnections(selection, reply) => {
                let _ = reply.send(self.select_connections(selection).await);
            },
            GetConnection(node_id, reply) => {
                let _ = reply.send(
                    self.pool
                        .get(&node_id)
                        .filter(|c| c.status() == ConnectionStatus::Connected)
                        .and_then(|c| c.connection())
                        .filter(|conn| conn.is_connected())
                        .cloned(),
                );
            },
            GetAllConnectionStates(reply) => {
                let states = self.pool.all().into_iter().cloned().collect();
                let _ = reply.send(states);
            },
            BanPeer(node_id, duration, reason) => {
                if !self.allow_list.contains(&node_id) {
                    if let Err(err) = self.ban_peer(&node_id, duration, reason).await {
                        error!(target: LOG_TARGET, "Error when banning peer: {:?}", err);
                    }
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Peer is excluded from being banned as it was found in the AllowList, NodeId: {:?}", node_id
                    );
                }
            },
            AddPeerToAllowList(node_id) => {
                if !self.allow_list.contains(&node_id) {
                    self.allow_list.push(node_id)
                }
            },
            RemovePeerFromAllowList(node_id) => {
                if let Some(index) = self.allow_list.iter().position(|x| *x == node_id) {
                    self.allow_list.remove(index);
                }
            },
            GetActiveConnections(reply) => {
                let _ = reply.send(
                    self.pool
                        .filter_connection_states(|s| s.is_connected())
                        .into_iter()
                        .cloned()
                        .collect(),
                );
            },
        }
    }

    async fn disconnect_all(&mut self) {
        let mut node_ids = Vec::with_capacity(self.pool.count_connected());
        for mut state in self.pool.filter_drain(|_| true) {
            if let Some(conn) = state.connection_mut() {
                if !conn.is_connected() {
                    continue;
                }
                match conn.disconnect_silent().await {
                    Ok(_) => {
                        node_ids.push(conn.peer_node_id().clone());
                    },
                    Err(err) => {
                        debug!(
                            target: LOG_TARGET,
                            "In disconnect_all: Error when disconnecting peer '{}' because '{:?}'",
                            conn.peer_node_id().short_str(),
                            err
                        );
                    },
                }
            }
        }

        for node_id in node_ids {
            self.publish_event(ConnectivityEvent::PeerDisconnected(node_id));
        }
    }

    async fn refresh_connection_pool(&mut self) -> Result<(), ConnectivityError> {
        debug!(
            target: LOG_TARGET,
            "Performing connection pool cleanup/refresh. (#Peers = {}, #Connected={}, #Failed={}, #Disconnected={}, \
             #Clients={})",
            self.pool.count_entries(),
            self.pool.count_connected_nodes(),
            self.pool.count_failed(),
            self.pool.count_disconnected(),
            self.pool.count_connected_clients()
        );

        self.clean_connection_pool();
        if self.config.is_connection_reaping_enabled {
            self.reap_inactive_connections().await;
        }
        self.update_connectivity_status();
        self.update_connectivity_metrics();
        Ok(())
    }

    async fn reap_inactive_connections(&mut self) {
        let connections = self
            .pool
            .get_inactive_connections_mut(self.config.reaper_min_inactive_age);
        for conn in connections {
            if !conn.is_connected() {
                continue;
            }

            debug!(
                target: LOG_TARGET,
                "Disconnecting '{}' because connection was inactive",
                conn.peer_node_id().short_str()
            );
            if let Err(err) = conn.disconnect().await {
                // Already disconnected
                debug!(
                    target: LOG_TARGET,
                    "Peer '{}' already disconnected. Error: {:?}",
                    conn.peer_node_id().short_str(),
                    err
                );
            }
        }
    }

    fn clean_connection_pool(&mut self) {
        let cleared_states = self.pool.filter_drain(|state| {
            state.status() == ConnectionStatus::Failed || state.status() == ConnectionStatus::Disconnected
        });

        if !cleared_states.is_empty() {
            debug!(
                target: LOG_TARGET,
                "Cleared connection states: {}",
                cleared_states
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }

    async fn select_connections(
        &self,
        selection: ConnectivitySelection,
    ) -> Result<Vec<PeerConnection>, ConnectivityError> {
        trace!(target: LOG_TARGET, "Selection query: {:?}", selection);
        debug!(
            target: LOG_TARGET,
            "Selecting from {} connected node peers",
            self.pool.count_connected_nodes()
        );

        let conns = selection.select(&self.pool);
        debug!(target: LOG_TARGET, "Selected {} connections(s)", conns.len());

        Ok(conns.into_iter().cloned().collect())
    }

    fn get_connection_stat_mut(&mut self, node_id: NodeId) -> &mut PeerConnectionStats {
        match self.connection_stats.entry(node_id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(PeerConnectionStats::new()),
        }
    }

    fn mark_peer_succeeded(&mut self, node_id: NodeId) {
        let entry = self.get_connection_stat_mut(node_id);
        entry.set_connection_success();
    }

    fn mark_peer_failed(&mut self, node_id: NodeId) -> usize {
        let entry = self.get_connection_stat_mut(node_id);
        entry.set_connection_failed();
        entry.failed_attempts()
    }

    async fn handle_peer_connection_failure(&mut self, node_id: &NodeId) -> Result<(), ConnectivityError> {
        if self.status.is_offline() {
            debug!(
                target: LOG_TARGET,
                "Node is offline. Ignoring connection failure event for peer '{}'.", node_id
            );
            return Ok(());
        }

        let num_failed = self.mark_peer_failed(node_id.clone());

        if num_failed >= self.config.max_failures_mark_offline {
            debug!(
                target: LOG_TARGET,
                "Marking peer '{}' as offline because this node failed to connect to them {} times",
                node_id.short_str(),
                num_failed
            );
            if !self.peer_manager.set_offline(node_id, true).await? {
                // Only publish the `PeerOffline` event if we change from online to offline
                self.publish_event(ConnectivityEvent::PeerOffline(node_id.clone()));
            }

            if let Some(peer) = self.peer_manager.find_by_node_id(node_id).await? {
                if !peer.is_banned() &&
                    peer.last_seen_since()
                        // Haven't seen them in expire_peer_last_seen_duration
                        .map(|t| t > self.config.expire_peer_last_seen_duration)
                        // Or don't delete if never seen
                        .unwrap_or(false)
                {
                    debug!(
                        target: LOG_TARGET,
                        "Peer `{}` was marked as offline after {} attempts (last seen: {}). Removing peer from peer \
                         list",
                        node_id,
                        num_failed,
                        peer.last_seen_since()
                            .map(|d| format!("{}s ago", d.as_secs()))
                            .unwrap_or_else(|| "Never".to_string()),
                    );
                    self.peer_manager.delete_peer(node_id).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_connection_manager_event(
        &mut self,
        event: &ConnectionManagerEvent,
    ) -> Result<(), ConnectivityError> {
        use ConnectionManagerEvent::*;
        debug!(target: LOG_TARGET, "Received event: {}", event);
        match event {
            PeerConnected(new_conn) => {
                match self.pool.get_connection(new_conn.peer_node_id()).cloned() {
                    Some(existing_conn) if !existing_conn.is_connected() => {
                        debug!(
                            target: LOG_TARGET,
                            "Tie break: Existing connection (id: {}, peer: {}, direction: {}) was not connected, \
                             resolving tie break by using the new connection. (New: id: {}, peer: {}, direction: {})",
                            existing_conn.id(),
                            existing_conn.peer_node_id(),
                            existing_conn.direction(),
                            new_conn.id(),
                            new_conn.peer_node_id(),
                            new_conn.direction(),
                        );
                        self.pool.remove(existing_conn.peer_node_id());
                    },
                    Some(mut existing_conn) => {
                        if self.tie_break_existing_connection(&existing_conn, new_conn) {
                            debug!(
                                target: LOG_TARGET,
                                "Tie break: Keep new connection (id: {}, peer: {}, direction: {}). Disconnect \
                                 existing connection (id: {}, peer: {}, direction: {})",
                                new_conn.id(),
                                new_conn.peer_node_id(),
                                new_conn.direction(),
                                existing_conn.id(),
                                existing_conn.peer_node_id(),
                                existing_conn.direction(),
                            );

                            let _ = existing_conn.disconnect_silent().await;
                            self.pool.remove(existing_conn.peer_node_id());
                        } else {
                            debug!(
                                target: LOG_TARGET,
                                "Tie break: Keeping existing connection (id: {}, peer: {}, direction: {}). \
                                 Disconnecting new connection (id: {}, peer: {}, direction: {})",
                                new_conn.id(),
                                new_conn.peer_node_id(),
                                new_conn.direction(),
                                existing_conn.id(),
                                existing_conn.peer_node_id(),
                                existing_conn.direction(),
                            );

                            let _ = new_conn.clone().disconnect_silent().await;
                            // Ignore this event - state can stay as is
                            return Ok(());
                        }
                    },

                    _ => {},
                }
            },
            PeerDisconnected(id, node_id) => {
                if let Some(conn) = self.pool.get_connection(node_id) {
                    if conn.id() != *id {
                        debug!(
                            target: LOG_TARGET,
                            "Ignoring peer disconnected event for stale peer connection (id: {}) for peer '{}'",
                            id,
                            node_id
                        );
                        return Ok(());
                    }
                }
            },
            _ => {},
        }

        let (node_id, mut new_status, connection) = match event {
            PeerDisconnected(_, node_id) => (&*node_id, ConnectionStatus::Disconnected, None),
            PeerConnected(conn) => (conn.peer_node_id(), ConnectionStatus::Connected, Some(conn.clone())),

            PeerConnectFailed(node_id, ConnectionManagerError::DialCancelled) => {
                debug!(
                    target: LOG_TARGET,
                    "Dial was cancelled before connection completed to peer '{}'", node_id
                );
                (&*node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, err) => {
                debug!(
                    target: LOG_TARGET,
                    "Connection to peer '{}' failed because '{:?}'", node_id, err
                );
                self.handle_peer_connection_failure(node_id).await?;
                (&*node_id, ConnectionStatus::Failed, None)
            },
            _ => return Ok(()),
        };

        let old_status = self.pool.set_status(node_id, new_status);
        if let Some(conn) = connection {
            new_status = self.pool.insert_connection(conn);
        }
        if old_status != new_status {
            debug!(
                target: LOG_TARGET,
                "Peer connection for node '{}' transitioned from {} to {}", node_id, old_status, new_status
            );
        }

        let node_id = node_id.clone();

        use ConnectionStatus::*;
        match (old_status, new_status) {
            (_, Connected) => {
                self.mark_peer_succeeded(node_id.clone());
                match self.pool.get_connection(&node_id).cloned() {
                    Some(conn) => {
                        self.publish_event(ConnectivityEvent::PeerConnected(conn));
                    },
                    None => unreachable!(
                        "Connection transitioning to CONNECTED state must always have a connection set i.e. \
                         ConnectionPool::get_connection is Some"
                    ),
                }
            },
            (Connected, Disconnected) => {
                self.publish_event(ConnectivityEvent::PeerDisconnected(node_id));
            },
            // Was not connected so don't broadcast event
            (_, Disconnected) => {},
            (_, Failed) => {
                self.publish_event(ConnectivityEvent::PeerConnectFailed(node_id));
            },
            _ => {
                error!(
                    target: LOG_TARGET,
                    "Unexpected connection status transition ({} to {}) for peer '{}'", old_status, new_status, node_id
                );
            },
        }

        self.update_connectivity_status();
        self.update_connectivity_metrics();
        Ok(())
    }

    /// Two connections to the same peer have been created. This function deterministically determines which peer
    /// connection to close. It does this by comparing our NodeId to that of the peer. This rule enables both sides to
    /// agree which connection to disconnect
    ///
    /// Returns true if the existing connection should close, otherwise false if the new connection should be closed.
    fn tie_break_existing_connection(&self, existing_conn: &PeerConnection, new_conn: &PeerConnection) -> bool {
        debug_assert_eq!(existing_conn.peer_node_id(), new_conn.peer_node_id());
        let peer_node_id = existing_conn.peer_node_id();
        let our_node_id = self.node_identity.node_id();

        debug!(
            target: LOG_TARGET,
            "Tie-break: (Existing = {}, New = {})",
            existing_conn.direction(),
            new_conn.direction()
        );
        use ConnectionDirection::*;
        match (existing_conn.direction(), new_conn.direction()) {
            // They connected to us twice for some reason. Drop the older connection
            (Inbound, Inbound) => true,
            // They connected to us at the same time we connected to them
            (Inbound, Outbound) => peer_node_id > our_node_id,
            // We connected to them at the same time as they connected to us
            (Outbound, Inbound) => our_node_id > peer_node_id,
            // We connected to them twice for some reason. Drop the older connection.
            (Outbound, Outbound) => true,
        }
    }

    fn update_connectivity_status(&mut self) {
        // The contract we are making with online/degraded status transitions is as follows:
        // - If min_connectivity peers are connected we MUST transition to ONLINE
        // - Clients SHOULD tolerate entering a DEGRADED/OFFLINE status
        // - If a number of peers disconnect or the local system's network goes down, the status MAY transition to
        //   DEGRADED
        let min_peers = self.config.min_connectivity;
        let num_connected_nodes = self.pool.count_connected_nodes();
        let num_connected_clients = self.pool.count_connected_clients();
        debug!(
            target: LOG_TARGET,
            "#min_peers = {}, #nodes = {}, #clients = {}", min_peers, num_connected_nodes, num_connected_clients
        );

        match num_connected_nodes {
            n if n >= min_peers => {
                self.transition(ConnectivityStatus::Online(n), min_peers);
            },
            n if n > 0 && n < min_peers => {
                self.transition(ConnectivityStatus::Degraded(n), min_peers);
            },
            n if n == 0 => {
                if num_connected_clients == 0 {
                    self.transition(ConnectivityStatus::Offline, min_peers);
                }
            },
            _ => unreachable!("num_connected is unsigned and only negative pattern covered on this branch"),
        }
    }

    #[cfg(not(feature = "metrics"))]
    fn update_connectivity_metrics(&mut self) {}

    #[cfg(feature = "metrics")]
    fn update_connectivity_metrics(&mut self) {
        use std::convert::TryFrom;

        use super::metrics;

        let total = self.pool.count_connected() as i64;
        let num_inbound = self.pool.count_filtered(|state| match state.connection() {
            Some(conn) => conn.is_connected() && conn.direction().is_inbound(),
            None => false,
        }) as i64;

        metrics::connections(ConnectionDirection::Inbound).set(num_inbound);
        metrics::connections(ConnectionDirection::Outbound).set(total - num_inbound);

        let uptime = self
            .uptime
            .map(|ts| i64::try_from(ts.elapsed().as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        metrics::uptime().set(uptime);
    }

    fn transition(&mut self, next_status: ConnectivityStatus, required_num_peers: usize) {
        use ConnectivityStatus::*;
        if self.status != next_status {
            debug!(
                target: LOG_TARGET,
                "Connectivity status transitioning from {} to {}", self.status, next_status
            );
        }

        match (self.status, next_status) {
            (Online(_), Online(_)) => {},
            (_, Online(n)) => {
                info!(
                    target: LOG_TARGET,
                    "Connectivity is ONLINE ({}/{} connections)", n, required_num_peers
                );

                #[cfg(feature = "metrics")]
                if self.uptime.is_none() {
                    self.uptime = Some(Instant::now());
                }
                self.publish_event(ConnectivityEvent::ConnectivityStateOnline(n));
            },
            (Degraded(m), Degraded(n)) => {
                info!(
                    target: LOG_TARGET,
                    "Connectivity is DEGRADED ({}/{} connections)", n, required_num_peers
                );
                if m != n {
                    self.publish_event(ConnectivityEvent::ConnectivityStateDegraded(n));
                }
            },
            (_, Degraded(n)) => {
                info!(
                    target: LOG_TARGET,
                    "Connectivity is DEGRADED ({}/{} connections)", n, required_num_peers
                );
                self.publish_event(ConnectivityEvent::ConnectivityStateDegraded(n));
            },
            (Offline, Offline) => {},
            (_, Offline) => {
                warn!(
                    target: LOG_TARGET,
                    "Connectivity is OFFLINE (0/{} connections)", required_num_peers
                );
                #[cfg(feature = "metrics")]
                {
                    self.uptime = None;
                }
                self.publish_event(ConnectivityEvent::ConnectivityStateOffline);
            },
            (status, next_status) => unreachable!("Unexpected status transition ({} to {})", status, next_status),
        }
        self.status = next_status;
    }

    fn publish_event(&mut self, event: ConnectivityEvent) {
        // A send operation can only fail if there are no subscribers, so it is safe to ignore the error
        let _ = self.event_tx.send(event);
    }

    async fn ban_peer(
        &mut self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<(), ConnectivityError> {
        info!(
            target: LOG_TARGET,
            "Banning peer {} for {} because: {}",
            node_id,
            format_duration(duration),
            reason
        );

        self.peer_manager.ban_peer_by_node_id(node_id, duration, reason).await?;

        #[cfg(feature = "metrics")]
        super::metrics::banned_peers_counter(node_id).inc();

        self.publish_event(ConnectivityEvent::PeerBanned(node_id.clone()));

        if let Some(conn) = self.pool.get_connection_mut(node_id) {
            conn.disconnect().await?;
            let status = self.pool.get_connection_status(node_id);
            debug!(
                target: LOG_TARGET,
                "Disconnected banned peer {}. The peer connection status is {}", node_id, status
            );
        }
        Ok(())
    }

    fn cleanup_connection_stats(&mut self) {
        let mut to_remove = Vec::new();
        for node_id in self.connection_stats.keys() {
            let status = self.pool.get_connection_status(node_id);
            if matches!(
                status,
                ConnectionStatus::NotConnected | ConnectionStatus::Failed | ConnectionStatus::Disconnected
            ) {
                to_remove.push(node_id.clone());
            }
        }
        for node_id in to_remove {
            self.connection_stats.remove(&node_id);
        }
    }
}
