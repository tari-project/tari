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
    fmt,
    time::{Duration, Instant},
};

use futures::{Stream, future, stream::FuturesUnordered};
use log::*;
use tokio::{
    sync::{broadcast, broadcast::error::RecvError, mpsc, oneshot},
    time,
};

use super::{
    ConnectivitySelection,
    connection_pool::PeerConnectionState,
    error::ConnectivityError,
    manager::ConnectivityStatus,
};
use crate::{
    Minimized,
    NodeIdentity,
    PeerConnection,
    RefKind,
    connection_manager::ConnectionManagerError,
    peer_manager::{NodeId, Peer},
};

const LOG_TARGET: &str = "comms::connectivity::requester";

/// Maximum time to wait for the ConnectivityManager actor to accept and answer a request that it
/// serves from its own in-memory state (connection pool, status, allow list, ...).
///
/// These requests perform no network I/O, so anything approaching this bound means the actor is not
/// draining its request channel. Returning an error lets the caller log and degrade instead of
/// hanging forever behind a wedged actor — which is how a comms-level stall used to take the whole
/// node down with it (the base node `status` command, DHT peer selection and block sync all wait
/// here).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum time to wait for a `DialPeer` request to resolve.
///
/// Unlike the state queries above this covers a real dial — transport connect, noise handshake and
/// identity exchange, retried across every address the peer advertises — so the bound is generous.
/// It exists to guarantee the caller is eventually released if the actor (or the connection manager
/// behind it) is wedged, not to bound a healthy dial. Callers that need to give up sooner should
/// impose their own, shorter deadline.
const DIAL_PEER_TIMEOUT: Duration = Duration::from_secs(180);

/// Connectivity event broadcast receiver.
pub type ConnectivityEventRx = broadcast::Receiver<ConnectivityEvent>;
/// Connectivity event broadcast sender.
pub type ConnectivityEventTx = broadcast::Sender<ConnectivityEvent>;

/// Node connectivity events emitted by the ConnectivityManager.
#[derive(Debug, Clone)]
pub enum ConnectivityEvent {
    PeerDisconnected(NodeId, Minimized),
    PeerConnected(Box<PeerConnection>),
    PeerConnectFailed(NodeId),
    PeerBanned(NodeId),
    ConnectivityStateInitialized,
    ConnectivityStateOnline(usize),
    ConnectivityStateDegraded(usize),
    ConnectivityStateOffline,
}

impl fmt::Display for ConnectivityEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[allow(clippy::enum_glob_use)]
        use ConnectivityEvent::*;
        match self {
            PeerDisconnected(node_id, minimized) => write!(f, "PeerDisconnected({node_id}, {minimized:?})"),
            PeerConnected(node_id) => write!(f, "PeerConnected({node_id})"),
            PeerConnectFailed(node_id) => write!(f, "PeerConnectFailed({node_id})"),
            PeerBanned(node_id) => write!(f, "PeerBanned({node_id})"),
            ConnectivityStateInitialized => write!(f, "ConnectivityStateInitialized"),
            ConnectivityStateOnline(n) => write!(f, "ConnectivityStateOnline({n})"),
            ConnectivityStateDegraded(n) => write!(f, "ConnectivityStateDegraded({n})"),
            ConnectivityStateOffline => write!(f, "ConnectivityStateOffline"),
        }
    }
}

/// Request types for the ConnectivityManager actor.
#[derive(Debug)]
pub enum ConnectivityRequest {
    WaitStarted(oneshot::Sender<()>),
    /// Dial a peer and return a connection of the requested [`RefKind`].
    ///
    /// `Strong` requests bump the connection's strong counter — reapers and DHT pool pruning will
    /// then skip this peer until every strong handle is dropped. `Weak` requests do not pin the
    /// connection.
    DialPeer {
        node_id: NodeId,
        ref_kind: RefKind,
        reply_tx: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    },
    GetConnectivityStatus(oneshot::Sender<ConnectivityStatus>),
    /// Batch select returns only weak handles. Callers wanting to pin specific peers should call
    /// [`PeerConnection::clone_strong`] on those individually.
    SelectConnections(
        ConnectivitySelection,
        oneshot::Sender<Result<Vec<PeerConnection>, ConnectivityError>>,
    ),
    /// Look up an existing connection; returns a handle of the requested [`RefKind`].
    GetConnection(NodeId, RefKind, oneshot::Sender<Option<PeerConnection>>),
    GetAllConnectionStates(oneshot::Sender<Vec<PeerConnectionState>>),
    GetMinimizeConnectionsThreshold(oneshot::Sender<Option<usize>>),
    /// Batch accessor: returns only weak handles. See [`Self::SelectConnections`].
    GetActiveConnections(oneshot::Sender<Vec<PeerConnection>>),
    BanPeer(NodeId, Duration, String),
    AddPeerToAllowList(NodeId),
    RemovePeerFromAllowList(NodeId),
    GetAllowList(oneshot::Sender<Vec<NodeId>>),
    GetSeeds(oneshot::Sender<Vec<Peer>>),
    GetPeerStats(NodeId, oneshot::Sender<Option<Peer>>),
    GetNodeIdentity(oneshot::Sender<NodeIdentity>),
}

/// Handle to make requests and read events from the ConnectivityManager actor.
#[derive(Debug, Clone)]
pub struct ConnectivityRequester {
    sender: mpsc::Sender<ConnectivityRequest>,
    event_tx: ConnectivityEventTx,
}

impl ConnectivityRequester {
    pub(crate) fn new(sender: mpsc::Sender<ConnectivityRequest>, event_tx: ConnectivityEventTx) -> Self {
        Self { sender, event_tx }
    }

    /// Returns a subscription to [ConnectivityEvent]s.
    ///
    /// [ConnectivityEvent](self::ConnectivityEvent)
    pub fn get_event_subscription(&self) -> ConnectivityEventRx {
        self.event_tx.subscribe()
    }

    pub(crate) fn get_event_publisher(&self) -> ConnectivityEventTx {
        self.event_tx.clone()
    }

    /// Send a fire-and-forget request to the actor, bounded by [`REQUEST_TIMEOUT`].
    async fn send_request(&self, request: ConnectivityRequest) -> Result<(), ConnectivityError> {
        time::timeout(REQUEST_TIMEOUT, self.sender.send(request))
            .await
            .map_err(|_| {
                warn!(target: LOG_TARGET, "ConnectivityManager did not accept a request within {REQUEST_TIMEOUT:.0?}");
                ConnectivityError::RequestTimedOut(REQUEST_TIMEOUT)
            })?
            .map_err(|_| ConnectivityError::ActorDisconnected)
    }

    /// Send a request to the actor and await its reply, bounded by [`REQUEST_TIMEOUT`].
    ///
    /// Only use this for requests the actor answers from its own state — see [`REQUEST_TIMEOUT`].
    async fn request<T>(
        &self,
        make_request: impl FnOnce(oneshot::Sender<T>) -> ConnectivityRequest,
    ) -> Result<T, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = make_request(reply_tx);
        time::timeout(REQUEST_TIMEOUT, async {
            self.sender
                .send(request)
                .await
                .map_err(|_| ConnectivityError::ActorDisconnected)?;
            reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
        })
        .await
        .map_err(|_| {
            warn!(target: LOG_TARGET, "ConnectivityManager did not respond within {REQUEST_TIMEOUT:.0?}");
            ConnectivityError::RequestTimedOut(REQUEST_TIMEOUT)
        })?
    }

    /// Dial a single peer, returning a connection of the requested [`RefKind`].
    ///
    /// Pass [`RefKind::Strong`] when the caller needs the connection pinned (reapers will skip
    /// the peer while any strong handle is alive) — typical for sync. Pass [`RefKind::Weak`]
    /// for opportunistic users (metadata service, gossip, etc.) that can tolerate the
    /// connection being torn down by the reaper.
    pub async fn dial_peer(&self, peer: NodeId, ref_kind: RefKind) -> Result<PeerConnection, ConnectivityError> {
        let mut num_cancels = 0;
        loop {
            let (reply_tx, reply_rx) = oneshot::channel();
            // Bounded by DIAL_PEER_TIMEOUT so a wedged actor cannot park the caller forever.
            let result = time::timeout(DIAL_PEER_TIMEOUT, async {
                self.sender
                    .send(ConnectivityRequest::DialPeer {
                        node_id: peer.clone(),
                        ref_kind,
                        reply_tx: Some(reply_tx),
                    })
                    .await
                    .map_err(|_| ConnectivityError::ActorDisconnected)?;

                reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
            })
            .await
            .map_err(|_| {
                warn!(
                    target: LOG_TARGET,
                    "Dial to peer `{peer}` did not resolve within {DIAL_PEER_TIMEOUT:.0?}"
                );
                ConnectivityError::DialTimedOut(DIAL_PEER_TIMEOUT)
            })??;

            match result {
                Ok(c) => return Ok(c),
                Err(err @ ConnectionManagerError::DialCancelled) => {
                    num_cancels += 1;
                    // Due to simultaneous dialing, it's possible for the dial to be cancelled. However, typically if
                    // dial is called again right after, the resolved connection will be returned.
                    if num_cancels == 1 {
                        continue;
                    }
                    return Err(err.into());
                },
                Err(err) => return Err(err.into()),
            }
        }
    }

    /// Dial many peers, returning a Stream that emits the dial Result as each dial completes.
    /// All returned connections share the same [`RefKind`].
    #[allow(clippy::let_with_type_underscore)]
    pub async fn dial_many_peers<I: IntoIterator<Item = NodeId>>(
        &self,
        peers: I,
        ref_kind: RefKind,
    ) -> impl Stream<Item = Result<PeerConnection, ConnectivityError>> + '_ {
        peers
            .into_iter()
            .map(move |peer| async move { self.dial_peer(peer, ref_kind).await })
            .collect::<FuturesUnordered<_>>()
    }

    /// Send a request to dial many peers without waiting for the response.
    ///
    /// Fire-and-forget dials produce no caller-side handle, so the resulting connection is
    /// stored as weak in the pool — callers wanting to pin the connection must `dial_peer` or
    /// `get_connection` with [`RefKind::Strong`] afterwards.
    pub async fn request_many_dials<I: IntoIterator<Item = NodeId>>(&self, peers: I) -> Result<(), ConnectivityError> {
        future::join_all(peers.into_iter().map(|peer| {
            self.send_request(ConnectivityRequest::DialPeer {
                node_id: peer,
                ref_kind: RefKind::Weak,
                reply_tx: None,
            })
        }))
        .await
        .into_iter()
        .try_for_each(|result| result)
    }

    /// Queries the ConnectivityManager and returns the matching [PeerConnection](crate::PeerConnection)s.
    pub async fn select_connections(
        &mut self,
        selection: ConnectivitySelection,
    ) -> Result<Vec<PeerConnection>, ConnectivityError> {
        self.request(|reply_tx| ConnectivityRequest::SelectConnections(selection, reply_tx))
            .await?
    }

    /// Get an active connection to the given node id if one exists, as a handle of the
    /// requested [`RefKind`]. Returns `Ok(None)` if the peer is not connected.
    pub async fn get_connection(
        &mut self,
        node_id: NodeId,
        ref_kind: RefKind,
    ) -> Result<Option<PeerConnection>, ConnectivityError> {
        self.request(|reply_tx| ConnectivityRequest::GetConnection(node_id, ref_kind, reply_tx))
            .await
    }

    /// Get the peer information from the peer, will return none if the peer is not found
    pub async fn get_peer_info(&self, node_id: NodeId) -> Result<Option<Peer>, ConnectivityError> {
        self.request(|reply_tx| ConnectivityRequest::GetPeerStats(node_id, reply_tx))
            .await
    }

    /// Get the current [ConnectivityStatus](self::ConnectivityStatus).
    pub async fn get_connectivity_status(&mut self) -> Result<ConnectivityStatus, ConnectivityError> {
        self.request(ConnectivityRequest::GetConnectivityStatus).await
    }

    /// Get the full connection state that the connectivity actor.
    pub async fn get_all_connection_states(&mut self) -> Result<Vec<PeerConnectionState>, ConnectivityError> {
        self.request(ConnectivityRequest::GetAllConnectionStates).await
    }

    /// Get the optional minimize connections setting.
    pub async fn get_minimize_connections_threshold(&mut self) -> Result<Option<usize>, ConnectivityError> {
        self.request(ConnectivityRequest::GetMinimizeConnectionsThreshold).await
    }

    /// Get all currently connection [PeerConnection](crate::PeerConnection]s.
    pub async fn get_active_connections(&mut self) -> Result<Vec<PeerConnection>, ConnectivityError> {
        self.request(ConnectivityRequest::GetActiveConnections).await
    }

    /// Ban peer for the given Duration. The ban `reason` is persisted in the peer database for reference.
    pub async fn ban_peer_until<T: Into<String>>(
        &mut self,
        node_id: NodeId,
        duration: Duration,
        reason: T,
    ) -> Result<(), ConnectivityError> {
        self.send_request(ConnectivityRequest::BanPeer(node_id, duration, reason.into()))
            .await
    }

    /// Ban the peer indefinitely.
    pub async fn ban_peer(&mut self, node_id: NodeId, reason: String) -> Result<(), ConnectivityError> {
        self.ban_peer_until(node_id, Duration::from_secs(u64::MAX), reason)
            .await
    }

    /// Adds a peer to an allow list, preventing it from being banned.
    pub async fn add_peer_to_allow_list(&mut self, node_id: NodeId) -> Result<(), ConnectivityError> {
        self.send_request(ConnectivityRequest::AddPeerToAllowList(node_id))
            .await
    }

    /// Retrieve self's allow list.
    pub async fn get_allow_list(&mut self) -> Result<Vec<NodeId>, ConnectivityError> {
        self.request(ConnectivityRequest::GetAllowList).await
    }

    /// Retrieve the list of seeds.
    pub async fn get_seeds(&mut self) -> Result<Vec<Peer>, ConnectivityError> {
        self.request(ConnectivityRequest::GetSeeds).await
    }

    /// Retrieve self's node identity.
    pub async fn get_node_identity(&mut self) -> Result<NodeIdentity, ConnectivityError> {
        self.request(ConnectivityRequest::GetNodeIdentity).await
    }

    /// Removes a peer from an allow list that prevents it from being banned.
    pub async fn remove_peer_from_allow_list(&mut self, node_id: NodeId) -> Result<(), ConnectivityError> {
        self.send_request(ConnectivityRequest::RemovePeerFromAllowList(node_id))
            .await
    }

    /// Returns a Future that resolves when the connectivity actor has started.
    pub async fn wait_started(&mut self) -> Result<(), ConnectivityError> {
        self.request(ConnectivityRequest::WaitStarted).await
    }

    /// Waits for the node to get at least one connection.
    /// This is useful for testing and is not typically be needed in application code.
    pub async fn wait_for_connectivity(&mut self, timeout: Duration) -> Result<(), ConnectivityError> {
        let mut connectivity_events = self.get_event_subscription();
        let status = self.get_connectivity_status().await?;
        if status.is_online() {
            return Ok(());
        }
        let start = Instant::now();
        let mut remaining = timeout;

        let mut last_known_peer_count = status.num_connected_nodes();
        loop {
            debug!(target: LOG_TARGET, "Waiting for connectivity event");
            let recv_result = time::timeout(remaining, connectivity_events.recv())
                .await
                .map_err(|_| ConnectivityError::OnlineWaitTimeout(last_known_peer_count))?;

            remaining = timeout
                .checked_sub(start.elapsed())
                .ok_or(ConnectivityError::OnlineWaitTimeout(last_known_peer_count))?;

            match recv_result {
                Ok(event) => match event {
                    ConnectivityEvent::ConnectivityStateOnline(_) => {
                        info!(target: LOG_TARGET, "Connectivity is ONLINE.");
                        break Ok(());
                    },
                    ConnectivityEvent::ConnectivityStateDegraded(n) => {
                        warn!(target: LOG_TARGET, "Connectivity is DEGRADED ({n} peer(s))");
                        last_known_peer_count = n;
                    },
                    ConnectivityEvent::ConnectivityStateOffline => {
                        warn!(
                            target: LOG_TARGET,
                            "Connectivity is OFFLINE. Waiting for connections..."
                        );
                        last_known_peer_count = 0;
                    },
                    event => {
                        debug!(
                            target: LOG_TARGET,
                            "Received event while waiting for connectivity: {event:?}"
                        );
                    },
                },
                Err(RecvError::Closed) => {
                    error!(
                        target: LOG_TARGET,
                        "Connectivity event stream closed unexpectedly. System may be shutting down."
                    );
                    break Err(ConnectivityError::ConnectivityEventStreamClosed);
                },
                Err(RecvError::Lagged(n)) => {
                    warn!(target: LOG_TARGET, "Lagging behind on {n} connectivity event(s)");
                    // We lagged, so could have missed the state change. Check it explicitly.
                    let status = self.get_connectivity_status().await?;
                    if status.is_online() {
                        break Ok(());
                    }
                },
            }
        }
    }
}
