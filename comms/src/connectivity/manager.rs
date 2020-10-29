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
use super::{
    config::ConnectivityConfig,
    connection_pool::{ConnectionPool, ConnectionStatus},
    connection_stats::PeerConnectionStats,
    error::ConnectivityError,
    requester::{ConnectivityEvent, ConnectivityRequest},
    selection::ConnectivitySelection,
};
use crate::{
    connection_manager::{ConnectionDirection, ConnectionManagerError, ConnectionManagerRequester},
    peer_manager::NodeId,
    runtime::task,
    utils::datetime::format_duration,
    ConnectionManagerEvent,
    NodeIdentity,
    PeerConnection,
    PeerManager,
};
use futures::{channel::mpsc, stream::Fuse, StreamExt};
use log::*;
use nom::lib::std::collections::hash_map::Entry;
use std::{
    cmp,
    collections::HashMap,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_shutdown::ShutdownSignal;
use tokio::{sync::broadcast, task::JoinHandle, time};

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
    pub event_tx: broadcast::Sender<Arc<ConnectivityEvent>>,
    pub connection_manager: ConnectionManagerRequester,
    pub peer_manager: Arc<PeerManager>,
    pub node_identity: Arc<NodeIdentity>,
    pub shutdown_signal: ShutdownSignal,
}

impl ConnectivityManager {
    pub fn create(self) -> ConnectivityManagerActor {
        ConnectivityManagerActor {
            config: self.config,
            status: ConnectivityStatus::Initializing,
            request_rx: self.request_rx.fuse(),
            connection_manager: self.connection_manager,
            peer_manager: self.peer_manager.clone(),
            event_tx: self.event_tx,
            connection_stats: HashMap::new(),
            node_identity: self.node_identity,

            managed_peers: Vec::new(),

            shutdown_signal: Some(self.shutdown_signal),
            pool: ConnectionPool::new(),
        }
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
}

impl fmt::Display for ConnectivityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct ConnectivityManagerActor {
    config: ConnectivityConfig,
    status: ConnectivityStatus,
    request_rx: Fuse<mpsc::Receiver<ConnectivityRequest>>,
    connection_manager: ConnectionManagerRequester,
    node_identity: Arc<NodeIdentity>,
    shutdown_signal: Option<ShutdownSignal>,
    peer_manager: Arc<PeerManager>,
    event_tx: broadcast::Sender<Arc<ConnectivityEvent>>,
    connection_stats: HashMap<NodeId, PeerConnectionStats>,

    managed_peers: Vec<NodeId>,
    pool: ConnectionPool,
}

impl ConnectivityManagerActor {
    pub fn spawn(self) -> JoinHandle<()> {
        task::spawn(Self::run(self))
    }

    pub async fn run(mut self) {
        info!(target: LOG_TARGET, "ConnectivityManager started");
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("ConnectivityManager initialized without a shutdown_signal");

        let mut connection_manager_events = self.connection_manager.get_event_subscription().fuse();

        let interval = self.config.connection_pool_refresh_interval;
        let mut ticker = time::interval_at(
            Instant::now()
                .checked_add(interval)
                .expect("connection_pool_refresh_interval cause overflow")
                .into(),
            interval,
        )
        .fuse();

        self.publish_event(ConnectivityEvent::ConnectivityStateInitialized);

        loop {
            futures::select! {
                req = self.request_rx.select_next_some() => {
                    self.handle_request(req).await;
                },

                event = connection_manager_events.select_next_some() => {
                    if let Ok(event) = event {
                        if let Err(err) = self.handle_connection_manager_event(&event).await {
                            error!(target:LOG_TARGET, "Error handling connection manager event: {:?}", err);
                        }
                    }
                },

                _ = ticker.next() => {
                    if let Err(err) = self.refresh_connection_pool().await {
                        error!(target: LOG_TARGET, "Error when refreshing connection pools: {:?}", err);
                    }
                },

                _ = shutdown_signal => {
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
            GetConnectivityStatus(reply) => {
                let _ = reply.send(self.status);
            },
            DialPeer(node_id, reply) => match self.pool.get(&node_id) {
                Some(state) if state.is_connected() => {
                    debug!(
                        target: LOG_TARGET,
                        "Found existing connection for peer `{}`",
                        node_id.short_str()
                    );
                    let _ = reply.send(Ok(state.connection().cloned().expect("Already checked")));
                },
                _ => {
                    debug!(
                        target: LOG_TARGET,
                        "Existing connection not found for peer `{}`",
                        node_id.short_str()
                    );
                    if let Err(err) = self.connection_manager.send_dial_peer(node_id, reply).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send dial request to connection manager: {:?}", err
                        );
                    }
                },
            },
            AddManagedPeers(node_ids) => {
                self.add_managed_peers(node_ids).await;
            },
            RemovePeer(node_id) => match self.remove_peer(&node_id) {
                Some(node_id) => {
                    debug!(target: LOG_TARGET, "Removed peer {} from managed pool", node_id);
                },
                None => {
                    warn!(
                        target: LOG_TARGET,
                        "Request to remove peer {} that is not managed", node_id
                    );
                },
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
                let _ = reply.send(self.pool.all().into_iter().cloned().collect());
            },
            BanPeer(node_id, duration, reason) => {
                if let Err(err) = self.ban_peer(&node_id, duration, reason).await {
                    error!(target: LOG_TARGET, "Error when banning peer: {:?}", err);
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
        if self.config.is_connection_reaping_enabled {
            self.reap_inactive_connections().await;
        }
        // Attempt to connect all managed peers: Failed, Disconnected or NotConnection will be dialed
        self.try_connect_managed_peers().await?;
        // Remove disconnected/failed peers from the connection pool
        self.clean_connection_pool();
        self.update_connectivity_status();
        Ok(())
    }

    async fn try_connect_managed_peers(&mut self) -> Result<(), ConnectivityError> {
        for node_id in &self.managed_peers {
            match self.pool.get_connection_status(node_id) {
                ConnectionStatus::Failed => {
                    let status = self.pool.set_status(node_id, ConnectionStatus::Retrying);
                    debug!(
                        target: LOG_TARGET,
                        "{} peer '{}' is managed. Retrying.", status, node_id
                    );
                    self.connection_manager.send_dial_peer_no_reply(node_id.clone()).await?;
                },
                ConnectionStatus::Disconnected => {
                    let status = self.pool.set_status(node_id, ConnectionStatus::Retrying);
                    debug!(
                        target: LOG_TARGET,
                        "{} peer '{}' is managed. Retrying.", status, node_id
                    );
                    self.connection_manager.send_dial_peer_no_reply(node_id.clone()).await?;
                    // self.send_dial_request(node_id.clone()).await?;
                },
                ConnectionStatus::NotConnected => {
                    let status = self.pool.set_status(node_id, ConnectionStatus::Connecting);
                    debug!(
                        target: LOG_TARGET,
                        "{} peer '{}' is managed. Connecting.", status, node_id
                    );
                    self.connection_manager.send_dial_peer_no_reply(node_id.clone()).await?;
                },
                _ => {},
            }
        }

        Ok(())
    }

    async fn reap_inactive_connections(&mut self) {
        let connections = self
            .pool
            .get_inactive_connections_mut(self.config.reaper_min_inactive_age);
        for conn in connections {
            // ConnectivityManager MUST NOT disconnect managed peers
            if self.managed_peers.contains(conn.peer_node_id()) {
                continue;
            }

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
        let managed_peers = self.managed_peers.clone();
        let cleared_states = self.pool.filter_drain(|state| {
            (state.status() == ConnectionStatus::Failed || state.status() == ConnectionStatus::Disconnected) &&
                !managed_peers.contains(state.node_id())
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
    ) -> Result<Vec<PeerConnection>, ConnectivityError>
    {
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

    async fn add_managed_peers(&mut self, node_ids: Vec<NodeId>) {
        let pool = &mut self.pool;
        let mut should_update_connectivity = false;
        for node_id in node_ids {
            if !self.managed_peers.contains(&node_id) {
                self.managed_peers.push(node_id.clone());
                should_update_connectivity = true;
            }

            match pool.insert(node_id.clone()) {
                ConnectionStatus::Failed => {
                    debug!(
                        target: LOG_TARGET,
                        "Retrying connection to failed managed peer '{}'", node_id
                    );
                    pool.set_status(&node_id, ConnectionStatus::Retrying);
                    if let Err(err) = self.connection_manager.send_dial_peer_no_reply(node_id.clone()).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send dial request to connection manager: {:?}", err
                        );
                        // Remove from this pool, it may be re-added later by the periodic connection refresh
                        pool.remove(&node_id);
                    }
                },
                ConnectionStatus::NotConnected | ConnectionStatus::Disconnected => {
                    debug!(target: LOG_TARGET, "Dialing offline managed peer '{}'", node_id);
                    pool.set_status(&node_id, ConnectionStatus::Connecting);
                    if let Err(err) = self.connection_manager.send_dial_peer_no_reply(node_id.clone()).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send dial request to connection manager: {:?}", err
                        );
                    }
                },
                status => debug!(
                    target: LOG_TARGET,
                    "Managed peer '{}' added with connection status {}", node_id, status
                ),
            }
        }

        if should_update_connectivity {
            self.update_connectivity_status();
        }
    }

    /// Removes a peer from the managed peers. This does not disconnect the peer, but the peer will be disconnected if
    /// inactive as part of the connection pool refresh procedure
    fn remove_peer(&mut self, node_id: &NodeId) -> Option<NodeId> {
        let pos = self.managed_peers.iter().position(|n| n == node_id)?;
        let removed_peer = self.managed_peers.remove(pos);
        self.update_connectivity_status();
        Some(removed_peer)
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
            if self.peer_manager.set_offline(node_id, true).await? {
                warn!(
                    target: LOG_TARGET,
                    "Peer `{}` was marked as offline but was already offline.", node_id
                );
            } else {
                // Only publish the `PeerOffline` event if we changed the offline state from online to offline
                self.publish_event(ConnectivityEvent::PeerOffline(node_id.clone()));
            }
            self.connection_stats.remove(node_id);
        }

        Ok(())
    }

    async fn handle_connection_manager_event(
        &mut self,
        event: &ConnectionManagerEvent,
    ) -> Result<(), ConnectivityError>
    {
        use ConnectionManagerEvent::*;
        #[allow(clippy::single_match)]
        match event {
            PeerConnected(new_conn) => {
                self.connection_manager
                    .cancel_dial(new_conn.peer_node_id().clone())
                    .await?;
                match self.pool.get_connection(new_conn.peer_node_id()) {
                    Some(existing_conn) => {
                        if !existing_conn.is_connected() {
                            debug!(
                                target: LOG_TARGET,
                                "Tie break: Existing connection was not connected, resolving tie break by using the \
                                 new connection. (New={}, Existing={})",
                                new_conn,
                                existing_conn,
                            );
                        } else if self.tie_break_existing_connection(existing_conn, &new_conn) {
                            debug!(
                                target: LOG_TARGET,
                                "Tie break: (Peer = {}) Keep new {} connection, Disconnect existing {} connection",
                                new_conn.peer_node_id().short_str(),
                                new_conn.direction(),
                                existing_conn.direction()
                            );

                            let node_id = existing_conn.peer_node_id().clone();
                            let direction = existing_conn.direction();
                            delayed_close(existing_conn.clone(), self.config.connection_tie_break_linger);
                            self.publish_event(ConnectivityEvent::PeerConnectionWillClose(node_id, direction));
                        } else {
                            debug!(
                                target: LOG_TARGET,
                                "Tie break: (Peer = {}) Keeping existing {} connection, Disconnecting new {} \
                                 connection",
                                existing_conn.peer_node_id().short_str(),
                                existing_conn.direction(),
                                new_conn.direction(),
                            );

                            delayed_close(new_conn.clone(), self.config.connection_tie_break_linger);
                            // Ignore this event - state can stay as is
                            return Ok(());
                        }
                    },
                    _ => {},
                }
            },
            _ => {},
        }

        let (node_id, mut new_status, connection) = match event {
            PeerDisconnected(node_id) => {
                self.connection_stats.remove(&node_id);
                (&**node_id, ConnectionStatus::Disconnected, None)
            },
            PeerConnected(conn) => (conn.peer_node_id(), ConnectionStatus::Connected, Some(conn.clone())),

            PeerConnectFailed(node_id, ConnectionManagerError::DialCancelled) => {
                debug!(
                    target: LOG_TARGET,
                    "Dial was cancelled before connection completed to peer '{}'", node_id
                );
                (&**node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, err) => {
                debug!(
                    target: LOG_TARGET,
                    "Connection to peer '{}' failed because '{:?}'", node_id, err
                );
                self.handle_peer_connection_failure(node_id).await?;
                (&**node_id, ConnectionStatus::Failed, None)
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

        let is_managed = self.managed_peers.contains(node_id);
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
                if is_managed {
                    self.publish_event(ConnectivityEvent::ManagedPeerDisconnected(node_id));
                } else {
                    self.publish_event(ConnectivityEvent::PeerDisconnected(node_id));
                }
            },
            // Was not connected so don't broadcast event
            (_, Disconnected) => {},
            (_, Failed) => {
                if is_managed {
                    self.publish_event(ConnectivityEvent::ManagedPeerConnectFailed(node_id));
                } else {
                    self.publish_event(ConnectivityEvent::PeerConnectFailed(node_id));
                }
            },
            _ => {
                error!(
                    target: LOG_TARGET,
                    "Unexpected connection status transition ({} to {}) for peer '{}'", old_status, new_status, node_id
                );
            },
        }

        self.update_connectivity_status();
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
            // We connected to them twice for some reason. Drop the newer connection.
            (Outbound, Outbound) => false,
        }
    }

    fn update_connectivity_status(&mut self) {
        // The contract we are making with online/degraded status transitions is as follows:
        // - If no managed peers are set and a single peer is connected we MUST transition to ONLINE
        // - Clients SHOULD tolerate entering a DEGRADED/OFFLINE status
        // - If more managed peers are added, the status MAY transition to DEGRADED
        // - Clients MUST NOT assume that all managed peers are connected when ONLINE
        let min_peers = cmp::max(
            (self.managed_peers.len() as f32 * self.config.min_connectivity).ceil() as usize,
            1,
        );
        let num_connected_nodes = self.pool.count_connected_nodes();
        let num_connected_clients = self.pool.count_connected_clients();
        debug!(
            target: LOG_TARGET,
            "#managed peers = {}, min_peers = {}, #nodes = {}, #clients = {}",
            self.managed_peers.len(),
            min_peers,
            num_connected_nodes,
            num_connected_clients
        );

        match num_connected_nodes {
            n if n >= min_peers => {
                self.transition(ConnectivityStatus::Online(n), min_peers);
            },
            n if n > 0 && n < min_peers => {
                self.transition(ConnectivityStatus::Degraded(n), min_peers);
            },
            n if n == 0 => {
                if num_connected_clients == 0 && self.pool.count_failed() > 0 {
                    self.transition(ConnectivityStatus::Offline, min_peers);
                }
            },
            _ => unreachable!("num_connected is unsigned and only negative pattern covered on this branch"),
        }
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
                self.publish_event(ConnectivityEvent::ConnectivityStateOffline);
            },
            (status, next_status) => unreachable!("Unexpected status transition ({} to {})", status, next_status),
        }
        self.status = next_status;
    }

    fn publish_event(&mut self, event: ConnectivityEvent) {
        // A send operation can only fail if there are no subscribers, so it is safe to ignore the error
        let _ = self.event_tx.send(Arc::new(event));
    }

    async fn ban_peer(
        &mut self,
        node_id: &NodeId,
        duration: Duration,
        reason: String,
    ) -> Result<(), ConnectivityError>
    {
        info!(
            target: LOG_TARGET,
            "Banning peer {} for {}",
            node_id,
            format_duration(duration)
        );

        if let Some(pos) = self.managed_peers.iter().position(|n| n == node_id) {
            let node_id = self.managed_peers.remove(pos);
            debug!(target: LOG_TARGET, "Banned managed peer '{}'", node_id);
        }

        self.peer_manager.ban_peer_by_node_id(node_id, duration, reason).await?;

        self.publish_event(ConnectivityEvent::PeerBanned(node_id.clone()));

        if let Some(conn) = self.pool.get_connection_mut(node_id) {
            conn.disconnect().await?;
            let old_status = self.pool.set_status(node_id, ConnectionStatus::Disconnected);
            debug!(
                target: LOG_TARGET,
                "Disconnecting banned peer {}. The peer connection status was {}", node_id, old_status
            );
        }
        Ok(())
    }
}

fn delayed_close(conn: PeerConnection, delay: Duration) {
    task::spawn(async move {
        time::delay_for(delay).await;
        debug!(
            target: LOG_TARGET,
            "Closing connection from peer `{}` after delay",
            conn.peer_node_id()
        );
        // Can ignore the error here, the error is already logged by peer connection
        let _ = conn.clone().disconnect_silent().await;
    });
}
