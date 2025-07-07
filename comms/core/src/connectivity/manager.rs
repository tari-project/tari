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
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time,
    time::MissedTickBehavior,
};
use tracing::{span, Instrument, Level};

use super::{
    config::ConnectivityConfig,
    connection_pool::{ConnectionPool, ConnectionStatus},
    connection_stats::PeerConnectionStats,
    error::ConnectivityError,
    proactive_dialer::ProactiveDialer,
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
    utils::datetime::format_duration,
    Minimized,
    NodeIdentity,
    PeerConnection,
    PeerConnectionError,
    PeerManager,
};

const LOG_TARGET: &str = "comms::connectivity::manager";

// Maximum time allowed for refreshing the connection pool
const POOL_REFRESH_TIMEOUT: Duration = Duration::from_millis(2500);
// Maximum time allowed to disconnect a single peer
const PEER_DISCONNECT_TIMEOUT: Duration = Duration::from_millis(250);
// Warning threshold for request processing time
const ACCEPTABLE_CONNECTIVITY_REQUEST_PROCESSING_TIME: Duration = Duration::from_millis(500);
// Warning threshold for event processing time
const ACCEPTABLE_EVENT_PROCESSING_TIME: Duration = Duration::from_millis(500);

/// # Connectivity Manager
///
/// The ConnectivityManager actor is responsible for tracking the state of all peer
/// connections in the system and maintaining a _pool_ of peer connections.
///
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
        let proactive_dialer = ProactiveDialer::new(
            self.config,
            self.connection_manager.clone(),
            self.peer_manager.clone(),
            self.node_identity.clone(),
        );

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
            proactive_dialer,
        }
        .spawn()
    }
}

/// Node connectivity status.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityStatus {
    /// Initial connectivity status before the Connectivity actor has initialized.
    #[default]
    Initializing,
    /// Connectivity is online.
    Online(usize),
    /// Connectivity is less than the required minimum, but some connections are still active.
    Degraded(usize),
    /// There are no active connections.
    Offline,
}

impl ConnectivityStatus {
    is_fn!(is_initializing, ConnectivityStatus::Initializing);

    is_fn!(is_online, ConnectivityStatus::Online(_));

    is_fn!(is_offline, ConnectivityStatus::Offline);

    is_fn!(is_degraded, ConnectivityStatus::Degraded(_));

    pub fn num_connected_nodes(&self) -> usize {
        use ConnectivityStatus::{Degraded, Initializing, Offline, Online};
        match self {
            Initializing | Offline => 0,
            Online(n) | Degraded(n) => *n,
        }
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
    proactive_dialer: ProactiveDialer,
}

impl ConnectivityManagerActor {
    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(async { Self::run(self).await })
    }

    pub async fn run(mut self) {
        debug!(target: LOG_TARGET, "ConnectivityManager started");

        let mut connection_manager_events = self.connection_manager.get_event_subscription();

        let interval = self.config.connection_pool_refresh_interval;
        let mut connection_pool_timer = time::interval_at(
            Instant::now()
                .checked_add(interval)
                .expect("connection_pool_refresh_interval cause overflow")
                .into(),
            interval,
        );
        connection_pool_timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

        self.publish_event(ConnectivityEvent::ConnectivityStateInitialized);

        loop {
            tokio::select! {
                Some(req) = self.request_rx.recv() => {
                    let timer = Instant::now();
                    let task_id = rand::random::<u64>();
                    trace!(target: LOG_TARGET, "Request ({}): {:?}", task_id, req);
                    self.handle_request(req).await;
                    if timer.elapsed() > ACCEPTABLE_CONNECTIVITY_REQUEST_PROCESSING_TIME {
                        warn!(
                            target: LOG_TARGET,
                            "Request ({}) took too long to process: {:.2?}",
                            task_id,
                            format_duration(timer.elapsed())
                        );
                    }
                    trace!(target: LOG_TARGET, "Request ({}) done", task_id);
                },

                Ok(event) = connection_manager_events.recv() => {
                    let timer = Instant::now();
                    let task_id = rand::random::<u64>();
                    trace!(target: LOG_TARGET, "Event ({}): {:?}", task_id, event);
                    if let Err(err) = self.handle_connection_manager_event(&event).await {
                        error!(target:LOG_TARGET, "Error handling connection manager event ({}): {:?}", task_id, err);
                    }
                    if timer.elapsed() > ACCEPTABLE_EVENT_PROCESSING_TIME {
                        warn!(
                            target: LOG_TARGET,
                            "Event ({}) took too long to process: {:.2?}",
                            task_id,
                            format_duration(timer.elapsed())
                        );
                    }
                    trace!(target: LOG_TARGET, "Event ({}) done", task_id);
                },

                _ = connection_pool_timer.tick() => {
                    let task_id = rand::random::<u64>();
                    trace!(target: LOG_TARGET, "Pool refresh peers task ({})", task_id);
                    self.cleanup_connection_stats();
                    match tokio::time::timeout(POOL_REFRESH_TIMEOUT, self.refresh_connection_pool(task_id)).await {
                        Ok(res) => {
                            if let Err(err) = res {
                                error!(target: LOG_TARGET, "Error refreshing connection pools ({}): {:?}", task_id, err);
                            }
                        },
                        Err(_) => {
                            warn!(
                                target: LOG_TARGET,
                                "Pool refresh task ({}) timeout",
                                task_id,
                            );
                        },
                    }
                    trace!(target: LOG_TARGET, "Pool refresh task ({}) done", task_id);
                },

                _ = self.shutdown_signal.wait() => {
                    info!(
                        target: LOG_TARGET,
                        "ConnectivityManager is shutting down because it received the shutdown signal"
                    );
                    self.disconnect_all().await;
                    break;
                }
            }
        }
    }

    async fn handle_request(&mut self, req: ConnectivityRequest) {
        #[allow(clippy::enum_glob_use)]
        use ConnectivityRequest::*;
        match req {
            WaitStarted(reply) => {
                let _ = reply.send(());
            },
            GetConnectivityStatus(reply) => {
                let _ = reply.send(self.status);
            },
            DialPeer { node_id, reply_tx } => {
                let tracing_id = tracing::Span::current().id();
                let span = span!(Level::TRACE, "handle_dial_peer");
                span.follows_from(tracing_id);
                self.handle_dial_peer(node_id.clone(), reply_tx).instrument(span).await;
            },
            SelectConnections(selection, reply) => {
                let _result = reply.send(self.select_connections(selection));
            },
            GetConnection(node_id, reply) => {
                let _result = reply.send(
                    self.pool
                        .get(&node_id)
                        .filter(|c| c.status() == ConnectionStatus::Connected)
                        .and_then(|c| c.connection())
                        .filter(|conn| conn.is_connected())
                        .cloned(),
                );
            },
            GetPeerStats(node_id, reply) => {
                let peer = match self.peer_manager.find_by_node_id(&node_id).await {
                    Ok(v) => v,
                    Err(e) => {
                        error!(target: LOG_TARGET, "Error when retrieving peer: {:?}", e);
                        None
                    },
                };
                let _result = reply.send(peer);
            },
            GetAllConnectionStates(reply) => {
                let states = self.pool.all().into_iter().cloned().collect();
                let _result = reply.send(states);
            },
            GetMinimizeConnectionsThreshold(reply) => {
                let minimize_connections_threshold = self.config.maintain_n_closest_connections_only;
                let _result = reply.send(minimize_connections_threshold);
            },
            BanPeer(node_id, duration, reason) => {
                if self.allow_list.contains(&node_id) {
                    info!(
                        target: LOG_TARGET,
                        "Peer is excluded from being banned as it was found in the AllowList, NodeId: {:?}", node_id
                    );
                } else if let Err(err) = self.ban_peer(&node_id, duration, reason).await {
                    error!(target: LOG_TARGET, "Error when banning peer: {:?}", err);
                } else {
                    // we banned the peer
                }
            },
            AddPeerToAllowList(node_id) => {
                if !self.allow_list.contains(&node_id) {
                    self.allow_list.push(node_id.clone());
                }
            },
            RemovePeerFromAllowList(node_id) => {
                if let Some(index) = self.allow_list.iter().position(|x| *x == node_id) {
                    self.allow_list.remove(index);
                }
            },
            GetAllowList(reply) => {
                let allow_list = self.allow_list.clone();
                let _result = reply.send(allow_list);
            },
            GetSeeds(reply) => {
                let seeds = self.peer_manager.get_seed_peers().await.unwrap_or_else(|e| {
                    error!(target: LOG_TARGET, "Error when retrieving seed peers: {:?}", e);
                    vec![]
                });
                let _result = reply.send(seeds);
            },
            GetActiveConnections(reply) => {
                let _result = reply.send(
                    self.pool
                        .filter_connection_states(|s| s.is_connected())
                        .into_iter()
                        .cloned()
                        .collect(),
                );
            },
            GetNodeIdentity(reply) => {
                let identity = self.node_identity.as_ref();
                let _result = reply.send(identity.clone());
            },
        }
    }

    async fn handle_dial_peer(
        &mut self,
        node_id: NodeId,
        reply_tx: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    ) {
        match self.peer_manager.is_peer_banned(&node_id).await {
            Ok(true) => {
                if let Some(reply) = reply_tx {
                    let _result = reply.send(Err(ConnectionManagerError::PeerBanned));
                }
                return;
            },
            Ok(false) => {},
            Err(err) => {
                if let Some(reply) = reply_tx {
                    let _result = reply.send(Err(err.into()));
                }
                return;
            },
        }
        match self.pool.get(&node_id) {
            // The connection pool may temporarily contain a connection that is not connected so we need to check this.
            Some(state) if state.is_connected() => {
                if let Some(reply_tx) = reply_tx {
                    let _result = reply_tx.send(Ok(state.connection().cloned().expect("Already checked")));
                }
            },
            maybe_state => {
                match maybe_state {
                    Some(state) => {
                        info!(
                            target: LOG_TARGET,
                            "Connection was previously attempted for peer {}. Current status is '{}'. Dialing again...",
                            node_id.short_str(),
                            state.status()
                        );
                    },
                    None => {
                        info!(
                            target: LOG_TARGET,
                            "No connection for peer {}. Dialing...",
                            node_id.short_str(),
                        );
                    },
                }

                if let Err(err) = self.connection_manager.send_dial_peer(node_id, reply_tx).await {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send dial request to connection manager: {:?}", err
                    );
                }
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
                match disconnect_silent_with_timeout(conn, Minimized::No, None).await {
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
            self.publish_event(ConnectivityEvent::PeerDisconnected(node_id, Minimized::No));
        }
    }

    async fn refresh_connection_pool(&mut self, task_id: u64) -> Result<(), ConnectivityError> {
        info!(
            target: LOG_TARGET,
            "CONNECTIVITY_REFRESH: Performing connection pool cleanup/refresh ({}). (#Peers = {}, #Connected={}, #Failed={}, #Disconnected={}, \
             #Clients={})",
            task_id,
            self.pool.count_entries(),
            self.pool.count_connected_nodes(),
            self.pool.count_failed(),
            self.pool.count_disconnected(),
            self.pool.count_connected_clients()
        );

        self.clean_connection_pool();
        if self.config.is_connection_reaping_enabled {
            self.reap_inactive_connections(task_id).await;
        }
        if let Some(threshold) = self.config.maintain_n_closest_connections_only {
            self.maintain_n_closest_peer_connections_only(threshold, task_id).await;
        }

        // Execute proactive dialing logic (if enabled)
        debug!(
            target: LOG_TARGET,
            "({}) Proactive dialing config check: enabled={}, target_connections={}",
            task_id,
            self.config.proactive_dialing_enabled,
            self.config.target_connection_count
        );

        if self.config.proactive_dialing_enabled {
            debug!(
                target: LOG_TARGET,
                "({}) Executing proactive dialing logic",
                task_id
            );
            if let Err(err) = self.execute_proactive_dialing(task_id).await {
                warn!(
                    target: LOG_TARGET,
                    "({}) Proactive dialing failed: {:?}",
                    task_id,
                    err
                );
            }
        } else {
            debug!(
                target: LOG_TARGET,
                "({}) Proactive dialing disabled in configuration",
                task_id
            );
        }

        self.update_connectivity_status();
        self.update_connectivity_metrics();
        Ok(())
    }

    async fn maintain_n_closest_peer_connections_only(&mut self, threshold: usize, task_id: u64) {
        let start = Instant::now();
        // Select all active peer connections (that are communication nodes) with health-aware selection
        let selection = ConnectivitySelection::healthy_closest_to(
            self.node_identity.node_id().clone(),
            self.pool.count_connected_nodes(),
            vec![],
        );
        let mut connections = match self.select_connections_with_health(selection) {
            Ok(peers) => peers,
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Connectivity error trying to maintain {} closest peers ({}) ({:?})",
                    threshold,
                    task_id,
                    e
                );
                return;
            },
        };
        let num_connections = connections.len();

        // Remove peers that are on the allow list
        connections.retain(|conn| !self.allow_list.contains(conn.peer_node_id()));
        debug!(
            target: LOG_TARGET,
            "minimize_connections: ({}) Filtered peers: {}, Handles: {}",
            task_id,
            connections.len(),
            num_connections,
        );

        // Disconnect all remaining peers above the threshold
        let len = connections.len();
        for conn in connections.iter_mut().skip(threshold) {
            debug!(
                target: LOG_TARGET,
                "minimize_connections: ({}) Disconnecting '{}' because the node is not among the {} closest peers",
                task_id,
                conn.peer_node_id(),
                threshold
            );
            match disconnect_with_timeout(conn, Minimized::Yes, Some(task_id)).await {
                Ok(_) => {
                    self.pool.remove(conn.peer_node_id());
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Peer '{}' already disconnected ({:?}). Error: {:?}",
                        conn.peer_node_id().short_str(),
                        task_id,
                        err
                    );
                },
            }
        }
        if len > 0 {
            debug!(
                "minimize_connections: ({}) Minimized {} connections in {:.2?}",
                task_id,
                len,
                start.elapsed()
            );
        }
    }

    async fn reap_inactive_connections(&mut self, task_id: u64) {
        let start = Instant::now();
        let excess_connections = self
            .pool
            .count_connected()
            .saturating_sub(self.config.reaper_min_connection_threshold);
        if excess_connections == 0 {
            return;
        }

        let mut connections = self
            .pool
            .get_inactive_outbound_connections_mut(self.config.reaper_min_inactive_age);
        connections.truncate(excess_connections);
        let mut nodes_to_remove = Vec::new();
        for conn in &mut connections {
            if !conn.is_connected() {
                continue;
            }

            debug!(
                target: LOG_TARGET,
                "({}) Disconnecting '{}' because connection was inactive ({} handles)",
                task_id,
                conn.peer_node_id().short_str(),
                conn.handle_count()
            );
            match disconnect_with_timeout(conn, Minimized::Yes, Some(task_id)).await {
                Ok(_) => {
                    nodes_to_remove.push(conn.peer_node_id().clone());
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Peer '{}' already disconnected ({:?}). Error: {:?}",
                        conn.peer_node_id().short_str(),
                        task_id,
                        err
                    );
                },
            }
        }
        let len = nodes_to_remove.len();
        if len > 0 {
            for node_id in nodes_to_remove {
                self.pool.remove(&node_id);
            }
            debug!(
                "({}) Reaped {} inactive connections in {:.2?}",
                task_id,
                len,
                start.elapsed()
            );
        }
    }

    fn clean_connection_pool(&mut self) {
        let cleared_states = self.pool.filter_drain(|state| {
            matches!(
                state.status(),
                ConnectionStatus::Failed | ConnectionStatus::Disconnected(_)
            )
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

    fn select_connections(&self, selection: ConnectivitySelection) -> Result<Vec<PeerConnection>, ConnectivityError> {
        trace!(target: LOG_TARGET, "Selection query: {:?}", selection);
        trace!(
            target: LOG_TARGET,
            "Selecting from {} connected node peers",
            self.pool.count_connected_nodes()
        );

        let conns = selection.select(&self.pool);
        debug!(target: LOG_TARGET, "Selected {} connections(s)", conns.len());

        Ok(conns.into_iter().cloned().collect())
    }

    fn select_connections_with_health(
        &self,
        selection: ConnectivitySelection,
    ) -> Result<Vec<PeerConnection>, ConnectivityError> {
        trace!(target: LOG_TARGET, "Health-aware selection query: {:?}", selection);
        trace!(
            target: LOG_TARGET,
            "Selecting from {} connected node peers with health metrics",
            self.pool.count_connected_nodes()
        );

        let conns = selection.select_with_health(
            &self.pool,
            &self.connection_stats,
            self.config.success_rate_tracking_window,
        );
        debug!(target: LOG_TARGET, "Selected {} healthy connections(s)", conns.len());

        Ok(conns.into_iter().cloned().collect())
    }

    fn get_connection_stat_mut(&mut self, node_id: NodeId) -> &mut PeerConnectionStats {
        match self.connection_stats.entry(node_id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(PeerConnectionStats::new()),
        }
    }

    fn mark_connection_success(&mut self, node_id: NodeId) {
        let entry = self.get_connection_stat_mut(node_id);
        entry.set_connection_success();

        // Update proactive dialing success metrics
    }

    fn mark_peer_failed(&mut self, node_id: NodeId) -> usize {
        let threshold = self.config.circuit_breaker_failure_threshold;
        let entry = self.get_connection_stat_mut(node_id);
        entry.set_connection_failed_with_threshold(threshold);

        entry.failed_attempts()
    }

    async fn on_peer_connection_failure(&mut self, node_id: &NodeId) -> Result<(), ConnectivityError> {
        if self.status.is_offline() {
            info!(
                target: LOG_TARGET,
                "Node is offline. Ignoring connection failure event for peer '{}'.", node_id
            );
            self.publish_event(ConnectivityEvent::ConnectivityStateOffline);
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
                    self.peer_manager.soft_delete_peer(node_id).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_connection_manager_event(
        &mut self,
        event: &ConnectionManagerEvent,
    ) -> Result<(), ConnectivityError> {
        self.update_state_on_connectivity_event(event).await?;
        self.update_connectivity_status();
        self.update_connectivity_metrics();
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn update_state_on_connectivity_event(
        &mut self,
        event: &ConnectionManagerEvent,
    ) -> Result<(), ConnectivityError> {
        use ConnectionManagerEvent::*;
        match event {
            PeerConnected(new_conn) => {
                match self.on_new_connection(new_conn).await {
                    TieBreak::KeepExisting => {
                        debug!(
                            target: LOG_TARGET,
                            "Discarding new connection to peer '{}' because we already have an existing connection",
                            new_conn.peer_node_id().short_str()
                        );
                        // Ignore event, we discarded the new connection and keeping the current one
                        return Ok(());
                    },
                    TieBreak::UseNew | TieBreak::None => {},
                }
            },
            PeerDisconnected(id, node_id, _minimized) => {
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
            PeerViolation { peer_node_id, details } => {
                self.ban_peer(
                    peer_node_id,
                    Duration::from_secs(2 * 60 * 60),
                    format!("Peer violation: {details}"),
                )
                .await?;
                return Ok(());
            },
            _ => {},
        }

        let (node_id, mut new_status, connection) = match event {
            PeerDisconnected(_, node_id, minimized) => (node_id, ConnectionStatus::Disconnected(*minimized), None),
            PeerConnected(conn) => (conn.peer_node_id(), ConnectionStatus::Connected, Some(conn.clone())),
            PeerConnectFailed(node_id, ConnectionManagerError::AllPeerAddressesAreExcluded(msg)) => {
                debug!(
                    target: LOG_TARGET,
                    "Peer '{}' contains only excluded addresses ({})",
                    node_id,
                    msg
                );
                (node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, ConnectionManagerError::NoiseHandshakeError(msg)) => {
                if let Some(conn) = self.pool.get_connection(node_id) {
                    debug!(
                        target: LOG_TARGET,
                        "Handshake error to peer '{}', disconnecting for a fresh retry ({})",
                        node_id,
                        msg
                    );
                    let mut conn = conn.clone();
                    disconnect_with_timeout(&mut conn, Minimized::No, None).await?;
                }
                (node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, ConnectionManagerError::DialCancelled) => {
                if let Some(conn) = self.pool.get_connection(node_id) {
                    if conn.is_connected() && conn.direction().is_inbound() {
                        debug!(
                            target: LOG_TARGET,
                            "Ignoring DialCancelled({}) event because an inbound connection already exists", node_id
                        );

                        return Ok(());
                    }
                }
                debug!(
                    target: LOG_TARGET,
                    "Dial was cancelled before connection completed to peer '{}'", node_id
                );
                (node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, err) => {
                debug!(
                    target: LOG_TARGET,
                    "Connection to peer '{}' failed because '{:?}'", node_id, err
                );
                self.on_peer_connection_failure(node_id).await?;
                (node_id, ConnectionStatus::Failed, None)
            },
            _ => return Ok(()),
        };

        let old_status = self.pool.set_status(node_id, new_status);
        if let Some(conn) = connection {
            new_status = self.pool.insert_connection(*conn);
        }
        if old_status != new_status {
            debug!(
                target: LOG_TARGET,
                "Peer connection for node '{}' transitioned from {} to {}", node_id, old_status, new_status
            );
        }

        let node_id = node_id.clone();

        use ConnectionStatus::{Connected, Disconnected, Failed};
        match (old_status, new_status) {
            (_, Connected) => match self.pool.get_connection_mut(&node_id).cloned() {
                Some(conn) => {
                    self.mark_connection_success(conn.peer_node_id().clone());
                    self.publish_event(ConnectivityEvent::PeerConnected(conn.into()));
                },
                None => unreachable!(
                    "Connection transitioning to CONNECTED state must always have a connection set i.e. \
                     ConnectionPool::get_connection is Some"
                ),
            },
            (Connected, Disconnected(..)) => {
                self.publish_event(ConnectivityEvent::PeerDisconnected(node_id, match new_status {
                    ConnectionStatus::Disconnected(reason) => reason,
                    _ => Minimized::No,
                }));
            },
            // Was not connected so don't broadcast event
            (_, Disconnected(..)) => {},
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

        Ok(())
    }

    async fn on_new_connection(&mut self, new_conn: &PeerConnection) -> TieBreak {
        match self.pool.get(new_conn.peer_node_id()).cloned() {
            Some(existing_state) if !existing_state.is_connected() => {
                debug!(
                    target: LOG_TARGET,
                    "Tie break: Existing connection (id: {}, peer: {}, direction: {}) was not connected, resolving \
                     tie break by using the new connection. (New: id: {}, peer: {}, direction: {})",
                    existing_state.connection().map(|c| c.id()).unwrap_or_default(),
                    existing_state.node_id(),
                    existing_state.connection().map(|c| c.direction().as_str()).unwrap_or("--"),
                    new_conn.id(),
                    new_conn.peer_node_id(),
                    new_conn.direction(),
                );
                self.pool.remove(existing_state.node_id());
                TieBreak::UseNew
            },
            Some(mut existing_state) => {
                let Some(existing_conn) = existing_state.connection_mut() else {
                    error!(
                        target: LOG_TARGET,
                        "INVARIANT ERROR in Tie break: PeerConnection is None but state is CONNECTED: Existing \
                        connection (id: {}, peer: {}, direction: {}), new connection. (id: {}, peer: {}, direction: {})",
                        existing_state.connection().map(|c| c.id()).unwrap_or_default(),
                        existing_state.node_id(),
                        existing_state.connection().map(|c| c.direction().as_str()).unwrap_or("--"),
                        new_conn.id(),
                        new_conn.peer_node_id(),
                        new_conn.direction(),
                    );
                    return TieBreak::UseNew;
                };
                if self.tie_break_existing_connection(existing_conn, new_conn) {
                    info!(
                        target: LOG_TARGET,
                        "Tie break: Keep new connection (id: {}, peer: {}, direction: {}). Disconnect existing \
                         connection (id: {}, peer: {}, direction: {})",
                        new_conn.id(),
                        new_conn.peer_node_id(),
                        new_conn.direction(),
                        existing_conn.id(),
                        existing_conn.peer_node_id(),
                        existing_conn.direction(),
                    );

                    let _result = disconnect_silent_with_timeout(existing_conn, Minimized::Yes, None).await;
                    self.pool.remove(existing_conn.peer_node_id());
                    TieBreak::UseNew
                } else {
                    debug!(
                        target: LOG_TARGET,
                        "Tie break: Keeping existing connection (id: {}, peer: {}, direction: {}). Disconnecting new \
                         connection (id: {}, peer: {}, direction: {})",
                        new_conn.id(),
                        new_conn.peer_node_id(),
                        new_conn.direction(),
                        existing_conn.id(),
                        existing_conn.peer_node_id(),
                        existing_conn.direction(),
                    );

                    let _result = disconnect_silent_with_timeout(&mut new_conn.clone(), Minimized::Yes, None).await;
                    TieBreak::KeepExisting
                }
            },

            None => TieBreak::None,
        }
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
        use ConnectionDirection::{Inbound, Outbound};
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
                } else {
                    self.transition(ConnectivityStatus::Degraded(n), min_peers);
                }
            },
            _ => unreachable!("num_connected is unsigned and only negative pattern covered on this branch"),
        }
    }

    #[cfg(not(feature = "metrics"))]
    fn update_connectivity_metrics(&mut self) {}

    #[allow(clippy::cast_possible_wrap)]
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
        use ConnectivityStatus::{Degraded, Offline, Online};
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
        let _result = self.event_tx.send(event);
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
        let ban_result = self.peer_manager.ban_peer_by_node_id(node_id, duration, reason).await;

        #[cfg(feature = "metrics")]
        super::metrics::banned_peers_counter().inc();

        self.publish_event(ConnectivityEvent::PeerBanned(node_id.clone()));

        if let Some(conn) = self.pool.get_connection_mut(node_id) {
            disconnect_with_timeout(conn, Minimized::Yes, None).await?;
            let status = self.pool.get_connection_status(node_id);
            debug!(
                target: LOG_TARGET,
                "Disconnected banned peer {}. The peer connection status is {}", node_id, status
            );
        }
        ban_result?;
        Ok(())
    }

    async fn execute_proactive_dialing(&mut self, task_id: u64) -> Result<(), ConnectivityError> {
        debug!(
            target: LOG_TARGET,
            "({}) Starting proactive dialing execution - current connections: {}, target: {}",
            task_id,
            self.pool.count_connected_nodes(),
            self.config.target_connection_count
        );

        // First, clean up old health data to keep metrics accurate
        for stats in self.connection_stats.values_mut() {
            stats.cleanup_old_health_data(self.config.success_rate_tracking_window);
        }

        // Update circuit breaker metrics
        self.update_circuit_breaker_metrics();

        // Execute proactive dialing logic
        match self
            .proactive_dialer
            .execute_proactive_dialing(&self.pool, &self.connection_stats, task_id)
            .await
        {
            Ok(dialed_count) => {
                if dialed_count > 0 {
                    debug!(
                        target: LOG_TARGET,
                        "({}) Proactive dialing initiated {} peer connections",
                        task_id,
                        dialed_count
                    );
                }
                Ok(())
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "({}) Proactive dialing failed: {:?}",
                    task_id,
                    err
                );
                Err(err)
            },
        }
    }

    fn update_circuit_breaker_metrics(&self) {
        let _circuit_breaker_open_count = self
            .connection_stats
            .values()
            .filter(|stats| stats.health_metrics().circuit_breaker_state().is_open())
            .count();

        // Calculate average peer health score
        if !self.connection_stats.is_empty() {
            let total_health: f32 = self
                .connection_stats
                .values()
                .map(|stats| stats.health_score(self.config.success_rate_tracking_window))
                .sum();
            let _avg_health = total_health / self.connection_stats.len() as f32;
        }
    }

    fn cleanup_connection_stats(&mut self) {
        let mut to_remove = Vec::new();
        for node_id in self.connection_stats.keys() {
            let status = self.pool.get_connection_status(node_id);
            if matches!(
                status,
                ConnectionStatus::NotConnected | ConnectionStatus::Failed | ConnectionStatus::Disconnected(_)
            ) {
                to_remove.push(node_id.clone());
            }
        }
        for node_id in to_remove {
            self.connection_stats.remove(&node_id);
        }
    }
}

enum TieBreak {
    None,
    UseNew,
    KeepExisting,
}

async fn disconnect_with_timeout(
    connection: &mut PeerConnection,
    minimized: Minimized,
    task_id: Option<u64>,
) -> Result<(), PeerConnectionError> {
    match tokio::time::timeout(PEER_DISCONNECT_TIMEOUT, connection.disconnect(minimized)).await {
        Ok(res) => res,
        Err(_) => {
            warn!(
                target: LOG_TARGET,
                "Timeout disconnecting peer ({:?}) '{}'",
                task_id,
                connection.peer_node_id().short_str(),
            );
            Err(PeerConnectionError::DisconnectTimeout)
        },
    }
}

async fn disconnect_silent_with_timeout(
    connection: &mut PeerConnection,
    minimized: Minimized,
    task_id: Option<u64>,
) -> Result<(), PeerConnectionError> {
    match tokio::time::timeout(PEER_DISCONNECT_TIMEOUT, connection.disconnect_silent(minimized)).await {
        Ok(res) => res,
        Err(_) => {
            warn!(
                target: LOG_TARGET,
                "Timeout disconnecting peer ({:?}) '{}'",
                task_id,
                connection.peer_node_id().short_str(),
            );
            Err(PeerConnectionError::DisconnectTimeout)
        },
    }
}
