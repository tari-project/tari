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
    sync::{Semaphore, mpsc, oneshot},
    task::JoinHandle,
    time,
    time::MissedTickBehavior,
};
use tracing::{Instrument, Level, span};

use super::{
    ConnectivityEventTx,
    config::ConnectivityConfig,
    connection_pool::{ConnectionPool, ConnectionStatus},
    connection_stats::PeerConnectionStats,
    error::ConnectivityError,
    proactive_dialer::ProactiveDialer,
    requester::{ConnectivityEvent, ConnectivityRequest},
    selection::ConnectivitySelection,
};
use crate::{
    Minimized,
    NodeIdentity,
    PeerConnection,
    PeerConnectionError,
    PeerManager,
    RefKind,
    connection_manager::{
        ConnectionDirection,
        ConnectionManagerError,
        ConnectionManagerEvent,
        ConnectionManagerRequester,
    },
    peer_manager::{NodeId, PEER_LOOKUP_TIMEOUT, PeerManagerError},
    utils::datetime::format_duration,
};

const LOG_TARGET: &str = "comms::connectivity::manager";

// Maximum time allowed for refreshing the connection pool
const POOL_REFRESH_TIMEOUT: Duration = Duration::from_millis(2500);

/// How many times a failed ban-persistence write (see `ConnectivityManagerActor::ban_peer`) is retried in the
/// background before giving up and logging loudly. Bounded so a peer database that stays unavailable does not
/// accumulate retry tasks forever - three attempts, doubling from `BAN_PERSIST_RETRY_INITIAL_DELAY`, is enough
/// to ride out a transient contention window (this is exactly what `PEER_DATABASE_BUSY_TIMEOUT` bounds a single
/// attempt to) without meaningfully delaying when we give up and say so.
const BAN_PERSIST_RETRY_ATTEMPTS: usize = 3;
const BAN_PERSIST_RETRY_INITIAL_DELAY: Duration = Duration::from_secs(2);
/// Upper bound for the doubling delay between retries - matches `BAN_PERSIST_RETRY_ATTEMPTS` (2s, 4s, 8s all
/// fall under this; the cap only matters if either constant above is changed later).
const BAN_PERSIST_RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
/// Caps how many ban-persistence retries (see `ConnectivityManagerActor::retry_ban_persistence`) may be
/// in flight at once. A burst of independent bans during sustained peer-database unavailability would otherwise
/// spawn one retry task per ban, each issuing up to `BAN_PERSIST_RETRY_ATTEMPTS` more writes against the very
/// database that is already struggling - a feedback direction opposite to what the retry exists for. Generous
/// rather than tight: this is a backstop against a burst, not an expected steady-state load (tokio's default
/// blocking pool already absorbs far more than this without the retries needing their own bound at all).
const MAX_CONCURRENT_BAN_PERSIST_RETRIES: usize = 16;
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
        let proactive_dialer =
            ProactiveDialer::new(self.config, self.connection_manager.clone(), self.peer_manager.clone());

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
            seeds: vec![],
            ban_persist_retry_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_BAN_PERSIST_RETRIES)),
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
        write!(f, "{self:?}")
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
    seeds: Vec<NodeId>,
    /// Bounds concurrent `retry_ban_persistence` tasks - see `MAX_CONCURRENT_BAN_PERSIST_RETRIES`.
    ban_persist_retry_permits: Arc<Semaphore>,
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
                    trace!(target: LOG_TARGET, "Request ({task_id}): {req:?}");
                    self.handle_request(req).await;
                    if timer.elapsed() > ACCEPTABLE_CONNECTIVITY_REQUEST_PROCESSING_TIME {
                        warn!(
                            target: LOG_TARGET,
                            "Request ({}) took too long to process: {:.2?}",
                            task_id,
                            format_duration(timer.elapsed())
                        );
                    }
                    trace!(target: LOG_TARGET, "Request ({task_id}) done");
                },

                Ok(event) = connection_manager_events.recv() => {
                    let timer = Instant::now();
                    let task_id = rand::random::<u64>();
                    trace!(target: LOG_TARGET, "Event ({task_id}): {event:?}");
                    if let Err(err) = self.handle_connection_manager_event(&event).await {
                        error!(target:LOG_TARGET, "Error handling connection manager event ({task_id}): {err:?}");
                    }
                    if timer.elapsed() > ACCEPTABLE_EVENT_PROCESSING_TIME {
                        warn!(
                            target: LOG_TARGET,
                            "Event ({}) took too long to process: {:.2?}",
                            task_id,
                            format_duration(timer.elapsed())
                        );
                    }
                    trace!(target: LOG_TARGET, "Event ({task_id}) done");
                },

                _ = connection_pool_timer.tick() => {
                    let task_id = rand::random::<u64>();
                    trace!(target: LOG_TARGET, "Pool refresh peers task ({task_id})");
                    self.cleanup_connection_stats();
                    match tokio::time::timeout(POOL_REFRESH_TIMEOUT, self.refresh_connection_pool(task_id)).await {
                        Ok(res) => {
                            if let Err(err) = res {
                                error!(target: LOG_TARGET, "Error refreshing connection pools ({task_id}): {err:?}");
                            }
                        },
                        Err(_) => {
                            warn!(
                                target: LOG_TARGET,
                                "Pool refresh task ({task_id}) timeout",
                            );
                        },
                    }
                    trace!(target: LOG_TARGET, "Pool refresh task ({task_id}) done" );
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

    #[allow(clippy::too_many_lines)]
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
            DialPeer {
                node_id,
                ref_kind,
                reply_tx,
            } => {
                let tracing_id = tracing::Span::current().id();
                let span = span!(Level::TRACE, "handle_dial_peer");
                span.follows_from(tracing_id);
                self.handle_dial_peer(node_id.clone(), ref_kind, reply_tx)
                    .instrument(span)
                    .await;
            },
            SelectConnections(selection, reply) => {
                // Batch accessor — always returns weak handles. Callers wanting to pin a
                // specific connection should call `clone_strong()` on it.
                let _result = reply.send(self.select_connections(selection));
            },
            GetConnection(node_id, ref_kind, reply) => {
                let _result = reply.send(
                    self.pool
                        .get(&node_id)
                        .filter(|c| c.status() == ConnectionStatus::Connected)
                        .and_then(|c| c.connection())
                        .filter(|conn| conn.is_connected())
                        .map(|conn| conn.clone_with(ref_kind)),
                );
            },
            GetPeerStats(node_id, reply) => {
                let peer = match self.peer_manager.find_by_node_id(&node_id).await {
                    Ok(v) => v,
                    Err(e) => {
                        error!(target: LOG_TARGET, "Error when retrieving peer: {e:?}");
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
                        "Peer is excluded from being banned as it was found in the AllowList, NodeId: {node_id:?}"
                    );
                } else if let Err(err) = self.ban_peer(&node_id, duration, reason).await {
                    error!(target: LOG_TARGET, "Error when banning peer: {err:?}");
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
                    error!(target: LOG_TARGET, "Error when retrieving seed peers: {e:?}");
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
        ref_kind: RefKind,
        reply_tx: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    ) {
        // Shed rather than block. `handle_dial_peer` runs on the ConnectivityManager's single request
        // loop and this ban check precedes every other statement in it, so a slow peer database used
        // to wedge the actor here — and with it `select_connections`, which the DHT needs to
        // propagate anything at all. The query still completes on the blocking pool; we just stop
        // waiting on it.
        let ban_check = time::timeout(PEER_LOOKUP_TIMEOUT, self.peer_manager.is_peer_banned(&node_id)).await;
        let Ok(ban_check) = ban_check else {
            warn!(
                target: LOG_TARGET,
                "Ban check for dial to peer '{}' exceeded {PEER_LOOKUP_TIMEOUT:.0?}. Shedding the dial rather than \
                 stalling the connectivity manager.",
                node_id.short_str()
            );
            if let Some(reply) = reply_tx {
                let _result = reply.send(Err(ConnectionManagerError::PeerLookupTimeout));
            }
            return;
        };
        match ban_check {
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

        // Per-peer circuit breaker. `handle_dial_peer` is the single chokepoint every dial passes
        // through, so this is the only place the breaker can cover DHT-originated dials as well as
        // the proactive dialer's own - previously it was consulted only inside the dialer's
        // candidate filter, which is why the DHT could re-dial a peer that had just failed
        // `circuit_breaker_failure_threshold` times in a row.
        //
        // It applies to *speculative* dials only: `reply_tx: None` together with `RefKind::Weak` is
        // exactly what `ConnectivityRequester::request_many_dials` sends (the pool refresh) and is
        // never what an explicit `dial_peer` from sync or RPC sends. An explicit dial names a peer
        // some other subsystem specifically needs, and must never be circuit-broken.
        let is_speculative_dial = reply_tx.is_none() && matches!(ref_kind, RefKind::Weak);
        if is_speculative_dial &&
            let Some(stats) = self.connection_stats.get(&node_id) &&
            !stats.should_allow_connection(self.config.circuit_breaker_retry_interval)
        {
            debug!(
                target: LOG_TARGET,
                "Skipping speculative dial to peer {} - its circuit breaker is open ({})",
                node_id.short_str(),
                stats
            );
            return;
        }

        match self.pool.get(&node_id) {
            // The connection pool may temporarily contain a connection that is not connected so we need to check this.
            Some(state) if state.is_connected() => {
                if let Some(reply_tx) = reply_tx {
                    let _result = reply_tx.send(Ok(state.connection().expect("Already checked").clone_with(ref_kind)));
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

                // When the caller wants a Strong handle but the dial is happening asynchronously
                // (no existing pooled connection), wrap their reply_tx so the connection returned
                // by the lower-level ConnectionManager — which is always Weak — is upgraded to a
                // Strong clone before delivery. For Weak (or fire-and-forget None) we pass the
                // reply through unchanged.
                let wrapped_reply_tx = match (reply_tx, ref_kind) {
                    (Some(outer_tx), RefKind::Strong) => {
                        let (inner_tx, inner_rx) = oneshot::channel::<Result<PeerConnection, ConnectionManagerError>>();
                        tokio::spawn(async move {
                            match inner_rx.await {
                                Ok(Ok(conn)) => {
                                    let _result = outer_tx.send(Ok(conn.clone_strong()));
                                },
                                Ok(Err(err)) => {
                                    let _result = outer_tx.send(Err(err));
                                },
                                Err(_) => {
                                    // dial actor dropped reply — propagate by dropping outer_tx
                                },
                            }
                        });
                        Some(inner_tx)
                    },
                    (other, _) => other,
                };

                // Non-blocking: this runs inside the actor's `select!` handler, so awaiting space on
                // the connection manager's request channel would park the whole ConnectivityManager
                // and every caller queued behind it. See `try_send_dial_peer`.
                if let Err(err) = self.connection_manager.try_send_dial_peer(node_id, wrapped_reply_tx) {
                    warn!(
                        target: LOG_TARGET,
                        "Shed dial request to connection manager: {err}"
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
                match disconnect_silent_with_timeout(
                    conn,
                    Minimized::No,
                    None,
                    "ConnectivityManagerActor disconnect all",
                )
                .await
                {
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
        // The DHT pool size this node was wired for. Both comms-side ceilings are set from
        // `num_neighbouring_nodes + num_random_nodes` by `P2pInitializer`, so either one reports the number the
        // DHT's own status line calls the "peer pool" - printing it here is what lets the two log targets be
        // read against each other instead of against each other's absence.
        let dht_pool_size = self
            .config
            .maintain_n_closest_connections_only
            .unwrap_or(self.config.reaper_min_connection_threshold);
        let num_connected_nodes = self.pool.count_connected_nodes();
        let floor = self.config.proactive_dialing_floor;
        // Decided once, here, so the single status line below and the branch it describes cannot disagree.
        let dialer_state = if !self.config.proactive_dialing_enabled {
            "disabled"
        } else if num_connected_nodes < floor {
            "ARMED"
        } else {
            "idle"
        };

        info!(
            target: LOG_TARGET,
            "CONNECTIVITY_REFRESH: Performing connection pool cleanup/refresh ({}). (#Peers = {}, \
             #ConnectedNodes(incl. inbound)={}, #Failed={}, #Disconnected={}, #Clients={}, DHT pool size={}, \
             proactive dialer={} (floor {}))",
            task_id,
            self.pool.count_entries(),
            num_connected_nodes,
            self.pool.count_failed(),
            self.pool.count_disconnected(),
            self.pool.count_connected_clients(),
            dht_pool_size,
            dialer_state,
            floor,
        );

        self.clean_connection_pool();
        // Deliberately outside the proactive-dialing gate below: aging seed connections out is unconditional,
        // only the seed *re-dial* path (inside `execute_proactive_dialing`) is gated on the floor.
        self.disconnect_seed_peers(task_id).await;

        if self.config.is_connection_reaping_enabled {
            self.reap_inactive_connections(task_id).await;
        }
        if let Some(threshold) = self.config.maintain_n_closest_connections_only {
            self.maintain_n_closest_peer_connections_only(threshold, task_id).await;
        }

        // Proactive dialing is a *recovery* mechanism, not a steady-state connection-count controller - the DHT
        // peer pool owns that. Gate it here, at the call site, rather than relying on the dialer's own early
        // return: the constraint that keeps the two loops from fighting is "this floor stays below the DHT pool
        // size", and it should be legible where the decision is made.
        //
        // Re-read the connection count rather than reusing the one above: the reaping and minimize-connections
        // passes in between may have dropped connections, and a node that has just been pushed below the floor
        // should recover on this tick, not the next one.
        if self.config.proactive_dialing_enabled &&
            self.pool.count_connected_nodes() < floor &&
            let Err(err) = self.execute_proactive_dialing(task_id).await
        {
            warn!(
                target: LOG_TARGET,
                "({task_id}) Proactive dialing failed: {err:?}"
            );
        }

        self.update_connectivity_status();
        self.update_connectivity_metrics();
        Ok(())
    }

    async fn maintain_n_closest_peer_connections_only(&mut self, threshold: usize, task_id: u64) {
        let start = Instant::now();
        // Select all active peer connections (that are communication nodes) with health-aware selection
        let selection = ConnectivitySelection::random_nodes(self.pool.count_connected_nodes(), vec![]);
        let mut connections = match self.select_connections_with_health(selection) {
            Ok(peers) => peers,
            Err(e) => {
                warn!(
                    target: LOG_TARGET,
                    "Connectivity error trying to maintain {threshold} peer connections ({task_id}) ({e:?})",
                );
                return;
            },
        };
        let num_connections = connections.len();

        // Remove peers that are on the allow list or are currently pinned by a strong
        // reference. A non-zero strong-count is the source of truth for "in use by sync (or any
        // other strong holder)" — this replaces the previous out-of-band sync_peers list.
        connections.retain(|conn| !self.allow_list.contains(conn.peer_node_id()) && !conn.is_strongly_held());
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
                "minimize_connections: ({}) Disconnecting '{}' because the node exceeds the {} connection threshold",
                task_id,
                conn.peer_node_id(),
                threshold
            );
            match disconnect_if_unused_with_timeout(
                conn,
                Minimized::Yes,
                Some(task_id),
                "ConnectivityManagerActor maintain connections",
            )
            .await
            {
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
        // Strong handles pin a connection — skip them even when they appear idle.
        connections.retain(|conn| !conn.is_strongly_held());
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
            match disconnect_with_timeout(
                conn,
                Minimized::Yes,
                Some(task_id),
                "ConnectivityManagerActor reap inactive",
            )
            .await
            {
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

    async fn refresh_seeds_list(&mut self) {
        match self.peer_manager.get_seed_peers().await {
            Ok(seeds) => {
                self.seeds = seeds.into_iter().map(|p| p.node_id).collect();
            },
            Err(err) => {
                error!(target: LOG_TARGET, "Failed to fetch seed peers: {}", err);
            },
        }
    }

    async fn disconnect_seed_peers(&mut self, task_id: u64) {
        self.refresh_seeds_list().await;

        if self.seeds.is_empty() {
            return;
        }

        // Identify seeds that are too old
        let mut seeds_to_disconnect = Vec::new();
        for seed_node_id in &self.seeds {
            if let Some(conn) = self.pool.get_connection(seed_node_id) &&
                conn.is_connected() &&
                conn.age() > self.config.max_seed_peer_age
            {
                seeds_to_disconnect.push(conn.clone());
            }
        }

        if seeds_to_disconnect.is_empty() {
            return;
        }

        debug!(
            target: LOG_TARGET,
            "({}) Found {} seed peer(s) eligible for cleanup", task_id, seeds_to_disconnect.len()
        );

        for mut conn in seeds_to_disconnect {
            if self.pool.count_connected_nodes() <= self.config.min_connectivity {
                debug!(
                    target: LOG_TARGET,
                    "({}) SKIPPING seed disconnect for '{}'. Connected Nodes ({}) <= Min ({})",
                    task_id,
                    conn.peer_node_id().short_str(),
                    self.pool.count_connected_nodes(),
                    self.config.min_connectivity
                );
                break;
            }

            debug!(
                target: LOG_TARGET,
                "({}) Disconnecting seed peer '{}' ...",
                task_id,
                conn.peer_node_id().short_str()
            );

            match disconnect_with_timeout(
                &mut conn,
                Minimized::Yes,
                Some(task_id),
                "ConnectivityManagerActor disconnect seed",
            )
            .await
            {
                Ok(_) => {
                    self.pool.remove(conn.peer_node_id());
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Seed peer '{}' already disconnected ({:?}). Error: {:?}",
                        conn.peer_node_id().short_str(),
                        task_id,
                        err
                    );
                },
            }
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
        trace!(target: LOG_TARGET, "Selection query: {selection:?}");
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
        trace!(target: LOG_TARGET, "Health-aware selection query: {selection:?}");
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
                "Node is offline. Ignoring connection failure event for peer '{node_id}'."
            );
            self.publish_event(ConnectivityEvent::ConnectivityStateOffline);
            return Ok(());
        }

        let _num_failed = self.mark_peer_failed(node_id.clone());

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
                if let Some(conn) = self.pool.get_connection(node_id) &&
                    conn.id() != *id
                {
                    debug!(
                        target: LOG_TARGET,
                        "Ignoring peer disconnected event for stale peer connection (id: {id}) for peer '{node_id}'"

                    );
                    return Ok(());
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
                    "Peer '{node_id}' contains only excluded addresses ({msg})"

                );
                (node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, ConnectionManagerError::NoiseHandshakeError(msg)) => {
                if let Some(conn) = self.pool.get_connection(node_id) {
                    debug!(
                        target: LOG_TARGET,
                        "Handshake error to peer '{node_id}', disconnecting for a fresh retry ({msg})"
                    );
                    let mut conn = conn.clone();
                    disconnect_with_timeout(
                        &mut conn,
                        Minimized::No,
                        None,
                        "ConnectivityManagerActor peer connect failed",
                    )
                    .await?;
                }
                (node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, ConnectionManagerError::DialCancelled) => {
                if let Some(conn) = self.pool.get_connection(node_id) &&
                    conn.is_connected() &&
                    conn.direction().is_inbound()
                {
                    debug!(
                        target: LOG_TARGET,
                        "Ignoring DialCancelled({node_id}) event because an inbound connection already exists"
                    );

                    return Ok(());
                }
                debug!(
                    target: LOG_TARGET,
                    "Dial was cancelled before connection completed to peer '{node_id}'"
                );
                (node_id, ConnectionStatus::Failed, None)
            },
            PeerConnectFailed(node_id, err) => {
                debug!(
                    target: LOG_TARGET,
                    "Connection to peer '{node_id}' failed because '{err:?}'"
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
                "Peer connection for node '{node_id}' transitioned from {old_status} to {new_status}"
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
                    "Unexpected connection status transition ({old_status} to {new_status}) for peer '{node_id}'"
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

                    let _result = disconnect_silent_with_timeout(
                        existing_conn,
                        Minimized::Yes,
                        None,
                        "ConnectivityManagerActor tie break",
                    )
                    .await;
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

                    let _result = disconnect_silent_with_timeout(
                        &mut new_conn.clone(),
                        Minimized::Yes,
                        None,
                        "ConnectivityManagerActor tie break",
                    )
                    .await;
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
            "#min_peers = {min_peers}, #nodes = {num_connected_nodes}, #clients = {num_connected_clients}"
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
        metrics::connections(ConnectionDirection::Outbound).set(total.saturating_sub(num_inbound));

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
                    "Connectivity is ONLINE ({n}/{required_num_peers} connections)"
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
                    "Connectivity is DEGRADED ({n}/{required_num_peers} connections)"
                );
                if m != n {
                    self.publish_event(ConnectivityEvent::ConnectivityStateDegraded(n));
                }
            },
            (_, Degraded(n)) => {
                info!(
                    target: LOG_TARGET,
                    "Connectivity is DEGRADED ({n}/{required_num_peers} connections)"
                );
                self.publish_event(ConnectivityEvent::ConnectivityStateDegraded(n));
            },
            (Offline, Offline) => {},
            (_, Offline) => {
                warn!(
                    target: LOG_TARGET,
                    "Connectivity is OFFLINE (0/{required_num_peers} connections)"
                );
                #[cfg(feature = "metrics")]
                {
                    self.uptime = None;
                }
                self.publish_event(ConnectivityEvent::ConnectivityStateOffline);
            },
            (status, next_status) => unreachable!("Unexpected status transition ({status} to {next_status})"),
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
        let ban_result = self
            .peer_manager
            .ban_peer_by_node_id(node_id, duration, reason.clone())
            .await;

        #[cfg(feature = "metrics")]
        super::metrics::banned_peers_counter().inc();

        if let Err(ref err) = ban_result {
            // Deliberately done *before* publishing `PeerBanned` and disconnecting below, not after: an
            // operator who reacts to that event (or to the disconnect) by calling `unban-peer`/
            // `unban-all-peers` must find this already queued, or the retry has no way to know the ban was
            // lifted out from under it - see the residual-race note on `retry_ban_persistence`. Everything from
            // here down to the retry being spawned is synchronous, non-blocking Rust with no `.await` in
            // between, so the window this leaves is a handful of CPU instructions, not the up-to-
            // `PEER_DISCONNECT_TIMEOUT` (250ms) the disconnect below can otherwise take - nowhere close to
            // reachable by a human reacting to an event, only by another task racing the exact same instant,
            // which is what the residual-race note already accounts for.
            //
            // Skip the retry entirely for `PeerNotFound`: that means this node has no record of the peer at all
            // (e.g. banning on a connection that was never added via `add_or_update_peer`), which retrying
            // cannot fix.
            if !matches!(err, PeerManagerError::PeerNotFound(_)) {
                // Semaphore acquired *before* `ban_generation` reads/creates a tracked entry: a retry shed here
                // for being over `MAX_CONCURRENT_BAN_PERSIST_RETRIES` is never spawned and never checks its
                // generation, so it must not leave an entry behind either - see the doc comment on
                // `PeerManager::ban_generations` for why that map needs to stay bounded.
                match Arc::clone(&self.ban_persist_retry_permits).try_acquire_owned() {
                    Ok(permit) => {
                        // This is the generation the retry must still match right before it writes.
                        let expected_generation = self.peer_manager.ban_generation(node_id);
                        // `retry_ban_persistence` is a plain `async fn`, spawned here rather than spawning
                        // internally, so a test can `.await` it directly and observe whether it wrote - see its
                        // doc comment and the `retry_ban_persistence_tests` module for why that matters.
                        tokio::spawn(Self::retry_ban_persistence(
                            self.peer_manager.clone(),
                            node_id.clone(),
                            duration,
                            reason,
                            expected_generation,
                            permit,
                        ));
                    },
                    Err(_) => {
                        error!(
                            target: LOG_TARGET,
                            "Peer {node_id} is being banned but the ban could not be persisted, and \
                             {MAX_CONCURRENT_BAN_PERSIST_RETRIES} ban-persistence retries are already in flight \
                             so this one was not queued. It is only banned for this session - a restart or a \
                             fresh `is_peer_banned` lookup will not see it as banned."
                        );
                    },
                }
            }
        }

        self.publish_event(ConnectivityEvent::PeerBanned(node_id.clone()));

        if let Some(conn) = self.pool.get_connection_mut(node_id) {
            // The ban decision has already been made and published above, whether or not it landed in the peer
            // database (see above); closing the connection is best-effort regardless. The connection may
            // already have been torn down (frequently the very reason we are banning the peer, e.g. it dropped
            // mid-sync), in which case the disconnect request cannot be sent. That is an expected race, not an
            // error, so we log it quietly and still consider the ban successful.
            match disconnect_with_timeout(conn, Minimized::Yes, None, "ConnectivityManagerActor ban peer").await {
                Ok(_) => {
                    let status = self.pool.get_connection_status(node_id);
                    debug!(
                        target: LOG_TARGET,
                        "Disconnected banned peer {node_id}. The peer connection status is {status}"
                    );
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Banned peer {node_id} but its connection was already closing ({err}); nothing to disconnect"
                    );
                },
            }
        }

        ban_result?;
        Ok(())
    }

    /// Retries a ban write that failed to persist, in the background, without blocking the calling actor. See
    /// `BAN_PERSIST_RETRY_ATTEMPTS` for why this is bounded rather than retried forever, and
    /// `MAX_CONCURRENT_BAN_PERSIST_RETRIES` for why `permit` must be held for the task's lifetime.
    ///
    /// Before every write attempt (including the first), re-checks `expected_generation` against the peer's
    /// *current* ban generation - via `PeerManager::ban_generation_if_tracked`, which never inserts - and
    /// abandons without writing the moment they no longer match, *including* when the entry is missing
    /// entirely (see that method's doc comment for why a missing entry must never be read as "unchanged"; it
    /// can go missing not only via an unban but also via `PeerManager::maybe_prune_ban_generations`'s
    /// oldest-eviction stage, which is a relative ranking with no guarantee it will never pick a live retry's
    /// entry). This is what stops the retry from resurrecting a ban an operator deliberately lifted with
    /// `unban-peer`/`unban-all-peers` while this task was sleeping: those calls bump the generation immediately
    /// (see `PeerManager::unban_peer`), independent of whether their own database write succeeds. Residual
    /// race: the check and the write below are not atomic with each other, so a generation bump landing in the
    /// gap between them - i.e. an unban arriving after this task has just confirmed the generation still
    /// matches but before its own write commits - is not caught, and the write would still land. Closing that
    /// fully needs the unban itself to take precedence at the database layer (e.g. a compare-and-swap in
    /// `set_banned` keyed to the same generation, which `set_banned` does not have), not just a check at this
    /// call site. What this does close is the much wider window this finding was actually about: the
    /// multi-second gap between a failed write and the retry that follows it, which is exactly what an operator
    /// reacting to a `PeerBanned` event needs.
    ///
    /// Deliberately a plain `async fn`, not a `fn` that spawns internally: the caller (`ban_peer`) is the one
    /// that spawns it. That lets `retry_ban_persistence_tests` drive this directly with `.await` and inspect
    /// the peer database afterward to see whether the write actually happened - i.e. it makes *the decision to
    /// write or abandon* the thing under test, not just the accessor (`PeerManager::ban_generation_if_tracked`)
    /// the decision happens to be built on. A test that only covered the accessor in isolation would keep
    /// passing even if this function's call site were reverted back to the insert-on-read `ban_generation`,
    /// silently reintroducing the exact resurrection bug this exists to prevent - see that test module's doc
    /// comment for how it was verified to actually catch that reversion.
    async fn retry_ban_persistence(
        peer_manager: Arc<PeerManager>,
        node_id: NodeId,
        duration: Duration,
        reason: String,
        expected_generation: u64,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) {
        let _permit = permit;
        // Snapshots for the final log lines below: both `node_id` and `reason` are moved into the retry
        // closure and would otherwise no longer be available once `retry_with_backoff` returns - the
        // closure (and its captures) is dropped along with it.
        let node_id_for_log = node_id.clone();
        let reason_for_log = reason.clone();
        let result = retry_with_backoff(
            BAN_PERSIST_RETRY_ATTEMPTS,
            BAN_PERSIST_RETRY_INITIAL_DELAY,
            BAN_PERSIST_RETRY_MAX_DELAY,
            move |attempt| {
                let peer_manager = peer_manager.clone();
                let node_id = node_id.clone();
                let reason = reason.clone();
                async move {
                    // `ban_generation_if_tracked`, not `ban_generation`: this is a re-check, not the
                    // baseline capture, and must never insert. A missing entry here means "we no longer
                    // know whether this ban is still wanted" - either an unban, or
                    // `maybe_prune_ban_generations`'s oldest-eviction stage, which is a *relative* ranking an
                    // unban's re-touch does not defend against the way it does against the TTL stage (see that
                    // method's doc comment). Treating `None` the same as "the generation changed" - abandon,
                    // do not write - rather than as "unchanged" is the fix: comparing against a
                    // freshly-reinserted `0` would make an evicted entry indistinguishable from a legitimate
                    // baseline of `0` and let a stale retry resurrect a lifted ban.
                    if peer_manager.ban_generation_if_tracked(&node_id) != Some(expected_generation) {
                        return Ok(BanPersistOutcome::Superseded);
                    }
                    peer_manager
                        .ban_peer_by_node_id(&node_id, duration, reason)
                        .await
                        .map(|_| BanPersistOutcome::Persisted(attempt))
                }
            },
        )
        .await;

        match result {
            Ok(BanPersistOutcome::Persisted(attempt)) => {
                info!(
                    target: LOG_TARGET,
                    "Ban for peer {node_id_for_log} persisted on retry {attempt}/{BAN_PERSIST_RETRY_ATTEMPTS}"
                );
            },
            Ok(BanPersistOutcome::Superseded) => {
                debug!(
                    target: LOG_TARGET,
                    "Abandoned a ban-persistence retry for peer {node_id_for_log}: either its ban state \
                     changed (e.g. an operator unban) or its bookkeeping entry was pruned while this retry was \
                     queued - both read the same way and are handled the same way, by not writing"
                );
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "Peer {node_id_for_log} was disconnected as banned ({reason_for_log}) but the ban could \
                     not be persisted after {BAN_PERSIST_RETRY_ATTEMPTS} retries: {err}. It is only banned \
                     for this session - a restart or a fresh `is_peer_banned` lookup will not see it as \
                     banned. The peer database may be persistently unavailable."
                );
            },
        }
    }

    async fn execute_proactive_dialing(&mut self, task_id: u64) -> Result<(), ConnectivityError> {
        debug!(
            target: LOG_TARGET,
            "({}) Starting proactive dialing execution - current connections: {}, floor: {}",
            task_id,
            self.pool.count_connected_nodes(),
            self.config.proactive_dialing_floor
        );

        // First, clean up old health data to keep metrics accurate
        for stats in self.connection_stats.values_mut() {
            stats.cleanup_old_health_data(self.config.success_rate_tracking_window);
        }

        // Update circuit breaker metrics
        self.update_circuit_breaker_metrics();

        self.refresh_seeds_list().await;

        // Determine if we should exclude seeds.
        let excluded_peers = if self.pool.count_connected_nodes() < self.config.min_connectivity {
            debug!(target: LOG_TARGET, "({}) Critical connectivity level ({} < {}). Allowing proactive dialer to retry Seed Nodes.",
                task_id,
                self.pool.count_connected_nodes(),
                self.config.min_connectivity
            );
            vec![]
        } else {
            self.seeds.clone()
        };

        // Execute proactive dialing logic
        match self
            .proactive_dialer
            .execute_proactive_dialing(&self.pool, &self.connection_stats, &excluded_peers, task_id)
            .await
        {
            Ok(dialed_count) => {
                if dialed_count > 0 {
                    debug!(
                        target: LOG_TARGET,
                        "({task_id}) Proactive dialing initiated {dialed_count} peer connections"
                    );
                }
                Ok(())
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "({task_id}) Proactive dialing failed: {err:?}"

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
    requester: &str,
) -> Result<(), PeerConnectionError> {
    match tokio::time::timeout(PEER_DISCONNECT_TIMEOUT, connection.disconnect(minimized, requester)).await {
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

async fn disconnect_if_unused_with_timeout(
    connection: &mut PeerConnection,
    minimized: Minimized,
    task_id: Option<u64>,
    requester: &str,
) -> Result<(), PeerConnectionError> {
    match tokio::time::timeout(
        PEER_DISCONNECT_TIMEOUT,
        connection.disconnect_if_unused(minimized, 0, 0, requester),
    )
    .await
    {
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
    requester: &str,
) -> Result<(), PeerConnectionError> {
    match tokio::time::timeout(
        PEER_DISCONNECT_TIMEOUT,
        connection.disconnect_silent(minimized, requester),
    )
    .await
    {
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

/// Outcome of one attempt inside `retry_ban_persistence`'s call to `retry_with_backoff`. Both variants stop the
/// retry loop (it is only `Err` that keeps it going) - `Superseded` is not a failure, just a different reason to
/// stop, which is why it is carried as `Ok` rather than folded into the error type.
enum BanPersistOutcome {
    /// The write succeeded, on the given attempt number (1-based).
    Persisted(usize),
    /// Not attempted: the peer's ban generation had already moved on from what this retry was queued for (see
    /// `ConnectivityManagerActor::retry_ban_persistence`).
    Superseded,
}

/// Retries `f` until it returns `Ok`, sleeping before each attempt (including the first) for a delay that starts
/// at `initial_delay` and doubles - capped at `max_delay` - after each `Err`. Returns the first `Ok`, or the last
/// `Err` once `attempts` have all been made.
///
/// Generic and free of anything but the backoff policy itself - no I/O, no domain knowledge of what `f` does -
/// specifically so the attempt-counting, doubling and give-up behaviour can be unit-tested on their own (see the
/// `tests` module below) without needing a database or any other real failure to inject.
async fn retry_with_backoff<F, Fut, T, E>(
    attempts: usize,
    initial_delay: Duration,
    max_delay: Duration,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut delay = initial_delay;
    let mut last_err = None;
    for attempt in 1..=attempts.max(1) {
        time::sleep(delay).await;
        match f(attempt).await {
            Ok(v) => return Ok(v),
            Err(err) => {
                delay = delay.checked_mul(2).unwrap_or(max_delay).min(max_delay);
                last_err = Some(err);
            },
        }
    }
    // `attempts.max(1)` above guarantees the loop ran at least once, so it always either returned `Ok` or set
    // `last_err` - this is unreachable, not a real fallback.
    #[allow(clippy::expect_used)]
    Err(last_err.expect("retry_with_backoff: loop ran at least once, so an Err was always recorded on exit"))
}

#[cfg(test)]
mod retry_with_backoff_tests {
    #![allow(clippy::indexing_slicing)]
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Succeeds on the Nth call; every call before that returns `Err`. Also records how many times it was
    /// invoked and the wall-clock (virtual, under `start_paused`) instant of each call, so tests can assert on
    /// both attempt counting and the backoff delay between attempts.
    struct CountingFailThenSucceed {
        calls: AtomicUsize,
        succeed_on_attempt: usize,
        call_instants: std::sync::Mutex<Vec<time::Instant>>,
    }

    impl CountingFailThenSucceed {
        fn new(succeed_on_attempt: usize) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                succeed_on_attempt,
                call_instants: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn call(&self, attempt: usize) -> Result<usize, &'static str> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.call_instants.lock().unwrap().push(time::Instant::now());
            if attempt >= self.succeed_on_attempt {
                Ok(attempt)
            } else {
                Err("not yet")
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_on_first_attempt_calls_f_exactly_once() {
        let target = CountingFailThenSucceed::new(1);
        let result = retry_with_backoff(3, Duration::from_secs(2), Duration::from_secs(30), |attempt| {
            std::future::ready(target.call(attempt))
        })
        .await;
        assert_eq!(result, Ok(1));
        assert_eq!(target.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retries_with_doubling_backoff_until_success() {
        let target = CountingFailThenSucceed::new(3);
        let start = time::Instant::now();
        let result = retry_with_backoff(5, Duration::from_secs(2), Duration::from_secs(30), |attempt| {
            std::future::ready(target.call(attempt))
        })
        .await;
        assert_eq!(result, Ok(3));
        assert_eq!(target.calls.load(Ordering::SeqCst), 3);

        // Attempt 1 after 2s, attempt 2 after a further 4s, attempt 3 (the success) after a further 8s - 14s
        // total from start, matching the doubling policy (2s, 4s, 8s).
        let instants = target.call_instants.lock().unwrap();
        assert_eq!(instants.len(), 3);
        assert_eq!(instants[0].saturating_duration_since(start), Duration::from_secs(2));
        assert_eq!(instants[1].saturating_duration_since(start), Duration::from_secs(6));
        assert_eq!(instants[2].saturating_duration_since(start), Duration::from_secs(14));
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_exhausting_every_attempt() {
        let target = CountingFailThenSucceed::new(usize::MAX);
        let result: Result<usize, &'static str> =
            retry_with_backoff(3, Duration::from_secs(2), Duration::from_secs(30), |attempt| {
                std::future::ready(target.call(attempt))
            })
            .await;
        assert_eq!(result, Err("not yet"));
        assert_eq!(target.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn delay_is_capped_at_max_delay() {
        let target = CountingFailThenSucceed::new(usize::MAX);
        let start = time::Instant::now();
        let _ = retry_with_backoff(4, Duration::from_secs(5), Duration::from_secs(8), |attempt| {
            std::future::ready(target.call(attempt))
        })
        .await;

        // Per-step delay uncapped would double as 5s, 10s, 20s, 40s. Capped at 8s it is 5s, 8s, 8s, 8s -
        // cumulative instants 5s, 13s, 21s, 29s.
        let instants = target.call_instants.lock().unwrap();
        assert_eq!(instants.len(), 4);
        assert_eq!(instants[0].saturating_duration_since(start), Duration::from_secs(5));
        assert_eq!(instants[1].saturating_duration_since(start), Duration::from_secs(13));
        assert_eq!(instants[2].saturating_duration_since(start), Duration::from_secs(21));
        assert_eq!(instants[3].saturating_duration_since(start), Duration::from_secs(29));
    }
}

#[cfg(test)]
mod retry_ban_persistence_tests {
    use tokio::sync::Semaphore;

    use super::*;
    use crate::{
        peer_manager::{PeerFeatures, create_test_peer},
        test_utils::peer_manager::build_peer_manager,
    };

    /// End-to-end regression test for the resurrection bug fixed by moving `retry_ban_persistence`'s re-check
    /// off insert-on-read `PeerManager::ban_generation` onto the never-inserting `ban_generation_if_tracked` -
    /// see that function's doc comment for the full account of why. This drives `retry_ban_persistence` itself,
    /// not just the accessor it is built on, so that reverting its call site back to
    /// `peer_manager.ban_generation(&node_id) != expected_generation` is actually caught here rather than only
    /// by a test of the accessor in isolation, which would keep passing regardless of what the call site does.
    ///
    /// Verified this actually catches that reversion: temporarily changed the re-check inside
    /// `retry_ban_persistence` back to the pre-fix `peer_manager.ban_generation(&node_id) !=
    /// expected_generation`, ran this test, and confirmed it failed (the peer ended up banned - the exact
    /// resurrection this exists to prevent); reverted, and confirmed it passes again.
    #[tokio::test(start_paused = true)]
    async fn retry_abandons_without_writing_when_its_tracked_entry_has_gone_missing() {
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peer_manager = build_peer_manager(&this_peer).unwrap();
        let target_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let node_id = target_peer.node_id.clone();
        peer_manager.add_or_update_peer(target_peer).await.unwrap();

        // The baseline `ban_peer` would have captured when it scheduled this retry.
        let expected_generation = peer_manager.ban_generation(&node_id);

        // Simulate the entry going missing mid-retry - e.g. `maybe_prune_ban_generations`'s oldest-eviction
        // stage - without needing to provoke a real, threshold-sized sweep.
        peer_manager.forget_ban_generation_for_test(&node_id);

        let permits = Arc::new(Semaphore::new(1));
        let permit = permits.try_acquire_owned().unwrap();

        ConnectivityManagerActor::retry_ban_persistence(
            peer_manager.clone(),
            node_id.clone(),
            Duration::from_secs(3600),
            "test".to_string(),
            expected_generation,
            permit,
        )
        .await;

        assert!(
            !peer_manager.is_peer_banned(&node_id).await.unwrap(),
            "a retry whose tracked entry has gone missing at re-check time must abandon without writing - ending up \
             banned here means it silently resurrected/persisted a ban that had already been superseded"
        );
    }

    /// Sibling of the above along the "unchanged" path: with the entry left alone and matching, the retry must
    /// still actually persist the ban - confirming the abandon path above is a real decision, not a bug that
    /// happens to make ever writing look like abandoning.
    #[tokio::test(start_paused = true)]
    async fn retry_persists_the_ban_when_its_tracked_entry_is_unchanged() {
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peer_manager = build_peer_manager(&this_peer).unwrap();
        let target_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let node_id = target_peer.node_id.clone();
        peer_manager.add_or_update_peer(target_peer).await.unwrap();

        let expected_generation = peer_manager.ban_generation(&node_id);

        let permits = Arc::new(Semaphore::new(1));
        let permit = permits.try_acquire_owned().unwrap();

        ConnectivityManagerActor::retry_ban_persistence(
            peer_manager.clone(),
            node_id.clone(),
            Duration::from_secs(3600),
            "test".to_string(),
            expected_generation,
            permit,
        )
        .await;

        assert!(
            peer_manager.is_peer_banned(&node_id).await.unwrap(),
            "a retry whose tracked entry is unchanged must actually persist the ban"
        );
    }
}

#[cfg(test)]
mod speculative_dial_circuit_breaker_tests {
    use tari_shutdown::Shutdown;
    use tokio::sync::broadcast;

    use super::*;
    use crate::{
        connection_manager::ConnectionManagerRequest,
        peer_manager::{PeerFeatures, create_test_peer},
        test_utils::{node_identity::build_node_identity, peer_manager::build_peer_manager},
    };

    /// Builds a bare `ConnectivityManagerActor` plus the receiving end of its connection manager request
    /// channel. The receiver is held directly rather than going through `ConnectionManagerMock` so the
    /// assertions below are synchronous - "was a `DialPeer` request produced by this call?" is answered by
    /// `try_recv`, with no sleeping or racing against a spawned mock task.
    fn setup(
        peer_manager: Arc<PeerManager>,
    ) -> (
        ConnectivityManagerActor,
        mpsc::Receiver<ConnectionManagerRequest>,
        Shutdown,
    ) {
        let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);
        let (cm_tx, cm_rx) = mpsc::channel(10);
        let (cm_event_tx, _) = broadcast::channel(10);
        let connection_manager = ConnectionManagerRequester::new(cm_tx, cm_event_tx);
        let shutdown = Shutdown::new();
        let (_request_tx, request_rx) = mpsc::channel(1);
        let (event_tx, _event_rx) = broadcast::channel(10);
        let config = ConnectivityConfig::default();
        let proactive_dialer = ProactiveDialer::new(config, connection_manager.clone(), peer_manager.clone());

        let actor = ConnectivityManagerActor {
            config,
            status: ConnectivityStatus::Initializing,
            request_rx,
            connection_manager,
            node_identity,
            peer_manager,
            event_tx,
            connection_stats: HashMap::new(),
            pool: ConnectionPool::new(),
            shutdown_signal: shutdown.to_signal(),
            #[cfg(feature = "metrics")]
            uptime: Some(Instant::now()),
            allow_list: vec![],
            proactive_dialer,
            seeds: vec![],
            ban_persist_retry_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_BAN_PERSIST_RETRIES)),
        };

        (actor, cm_rx, shutdown)
    }

    /// Drives a peer's stats to a tripped (open) circuit breaker the same way the actor itself does, via
    /// consecutive recorded failures at the configured threshold.
    fn trip_circuit_breaker(actor: &mut ConnectivityManagerActor, node_id: &NodeId) {
        let threshold = actor.config.circuit_breaker_failure_threshold;
        let stats = actor.connection_stats.entry(node_id.clone()).or_default();
        for _ in 0..threshold {
            stats.set_connection_failed_with_threshold(threshold);
        }
        assert!(
            !stats.should_allow_connection(actor.config.circuit_breaker_retry_interval),
            "test setup is wrong: the circuit breaker did not open after {threshold} consecutive failures"
        );
    }

    /// `reply_tx: None` + `RefKind::Weak` is exactly and only what `ConnectivityRequester::request_many_dials`
    /// sends, whose sole caller in the workspace is the DHT peer pool refresh. Those dials are speculative -
    /// nobody is waiting on them and no subsystem named the peer - so a peer whose breaker is open must be
    /// skipped rather than re-dialed on every refresh tick.
    #[tokio::test]
    async fn speculative_dial_to_a_circuit_broken_peer_is_skipped() {
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peer_manager = build_peer_manager(&this_peer).unwrap();
        let target_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let node_id = target_peer.node_id.clone();
        peer_manager.add_or_update_peer(target_peer).await.unwrap();

        let (mut actor, mut cm_rx, _shutdown) = setup(peer_manager);
        trip_circuit_breaker(&mut actor, &node_id);

        actor.handle_dial_peer(node_id.clone(), RefKind::Weak, None).await;

        assert!(
            cm_rx.try_recv().is_err(),
            "a speculative dial to a circuit-broken peer must not reach the connection manager"
        );
    }

    /// The other half of the discriminator, and the one with the blast radius: an explicit dial (block sync, an
    /// RPC client, anything holding a `reply_tx`) names a peer some subsystem specifically needs, and must go
    /// through regardless of the breaker. Getting this wrong silently breaks sync.
    #[tokio::test]
    async fn explicit_dial_to_a_circuit_broken_peer_still_goes_through() {
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peer_manager = build_peer_manager(&this_peer).unwrap();
        let target_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let node_id = target_peer.node_id.clone();
        peer_manager.add_or_update_peer(target_peer).await.unwrap();

        let (mut actor, mut cm_rx, _shutdown) = setup(peer_manager);
        trip_circuit_breaker(&mut actor, &node_id);

        let (reply_tx, _reply_rx) = oneshot::channel();
        actor
            .handle_dial_peer(node_id.clone(), RefKind::Strong, Some(reply_tx))
            .await;

        match cm_rx.try_recv() {
            Ok(ConnectionManagerRequest::DialPeer { node_id: dialed, .. }) => assert_eq!(dialed, node_id),
            other => panic!("an explicit dial must never be circuit-broken, got {other:?}"),
        }
    }

    /// A speculative dial to a peer with no failure history - the overwhelmingly common case for the pool
    /// refresh - must still be dialed. Without this, the test above would also pass if the new check simply
    /// dropped every speculative dial.
    #[tokio::test]
    async fn speculative_dial_to_a_healthy_peer_still_goes_through() {
        let this_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let peer_manager = build_peer_manager(&this_peer).unwrap();
        let target_peer = create_test_peer(false, PeerFeatures::COMMUNICATION_NODE);
        let node_id = target_peer.node_id.clone();
        peer_manager.add_or_update_peer(target_peer).await.unwrap();

        let (mut actor, mut cm_rx, _shutdown) = setup(peer_manager);

        actor.handle_dial_peer(node_id.clone(), RefKind::Weak, None).await;

        match cm_rx.try_recv() {
            Ok(ConnectionManagerRequest::DialPeer { node_id: dialed, .. }) => assert_eq!(dialed, node_id),
            other => panic!("a speculative dial to a healthy peer must go through, got {other:?}"),
        }
    }
}
