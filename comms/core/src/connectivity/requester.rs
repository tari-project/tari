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

use futures::{future, stream::FuturesUnordered, Stream};
use log::*;
use tokio::{
    sync::{broadcast, broadcast::error::RecvError, mpsc, oneshot},
    time,
};

use super::{
    connection_pool::PeerConnectionState,
    error::ConnectivityError,
    manager::ConnectivityStatus,
    ConnectivitySelection,
};
use crate::{
    connection_manager::ConnectionManagerError,
    peer_manager::{NodeId, Peer},
    Minimized,
    NodeIdentity,
    PeerConnection,
};

const LOG_TARGET: &str = "comms::connectivity::requester";

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
            PeerDisconnected(node_id, minimized) => write!(f, "PeerDisconnected({}, {:?})", node_id, minimized),
            PeerConnected(node_id) => write!(f, "PeerConnected({})", node_id),
            PeerConnectFailed(node_id) => write!(f, "PeerConnectFailed({})", node_id),
            PeerBanned(node_id) => write!(f, "PeerBanned({})", node_id),
            ConnectivityStateInitialized => write!(f, "ConnectivityStateInitialized"),
            ConnectivityStateOnline(n) => write!(f, "ConnectivityStateOnline({})", n),
            ConnectivityStateDegraded(n) => write!(f, "ConnectivityStateDegraded({})", n),
            ConnectivityStateOffline => write!(f, "ConnectivityStateOffline"),
        }
    }
}

/// Request types for the ConnectivityManager actor.
#[derive(Debug)]
pub enum ConnectivityRequest {
    WaitStarted(oneshot::Sender<()>),
    DialPeer {
        node_id: NodeId,
        reply_tx: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    },
    GetConnectivityStatus(oneshot::Sender<ConnectivityStatus>),
    SelectConnections(
        ConnectivitySelection,
        oneshot::Sender<Result<Vec<PeerConnection>, ConnectivityError>>,
    ),
    GetConnection(NodeId, oneshot::Sender<Option<PeerConnection>>),
    GetAllConnectionStates(oneshot::Sender<Vec<PeerConnectionState>>),
    GetMinimizeConnectionsThreshold(oneshot::Sender<Option<usize>>),
    GetActiveConnections(oneshot::Sender<Vec<PeerConnection>>),
    BanPeer(NodeId, Duration, String),
    AddPeerToAllowList(NodeId),
    RemovePeerFromAllowList(NodeId),
    GetAllowList(oneshot::Sender<Vec<NodeId>>),
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

    /// Dial a single peer
    pub async fn dial_peer(&self, peer: NodeId) -> Result<PeerConnection, ConnectivityError> {
        let mut num_cancels = 0;
        loop {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.sender
                .send(ConnectivityRequest::DialPeer {
                    node_id: peer.clone(),
                    reply_tx: Some(reply_tx),
                })
                .await
                .map_err(|_| ConnectivityError::ActorDisconnected)?;

            match reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)? {
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
    #[allow(clippy::let_with_type_underscore)]
    pub fn dial_many_peers<I: IntoIterator<Item = NodeId>>(
        &self,
        peers: I,
    ) -> impl Stream<Item = Result<PeerConnection, ConnectivityError>> + '_ {
        peers
            .into_iter()
            .map(|peer| self.dial_peer(peer))
            .collect::<FuturesUnordered<_>>()
    }

    /// Send a request to dial many peers without waiting for the response.
    pub async fn request_many_dials<I: IntoIterator<Item = NodeId>>(&self, peers: I) -> Result<(), ConnectivityError> {
        future::join_all(peers.into_iter().map(|peer| {
            self.sender.send(ConnectivityRequest::DialPeer {
                node_id: peer,
                reply_tx: None,
            })
        }))
        .await
        .into_iter()
        .try_for_each(|result| result.map_err(|_| ConnectivityError::ActorDisconnected))
    }

    /// Queries the ConnectivityManager and returns the matching [PeerConnection](crate::PeerConnection)s.
    pub async fn select_connections(
        &mut self,
        selection: ConnectivitySelection,
    ) -> Result<Vec<PeerConnection>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::SelectConnections(selection, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)?
    }

    /// Get an active connection to the given node id if one exists. This will return None if the peer is not connected.
    pub async fn get_connection(&mut self, node_id: NodeId) -> Result<Option<PeerConnection>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetConnection(node_id, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Get the peer information from the peer, will return none if the peer is not found
    pub async fn get_peer_info(&self, node_id: NodeId) -> Result<Option<Peer>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetPeerStats(node_id, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Get the current [ConnectivityStatus](self::ConnectivityStatus).
    pub async fn get_connectivity_status(&mut self) -> Result<ConnectivityStatus, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetConnectivityStatus(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Get the full connection state that the connectivity actor.
    pub async fn get_all_connection_states(&mut self) -> Result<Vec<PeerConnectionState>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetAllConnectionStates(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Get the optional minimize connections setting.
    pub async fn get_minimize_connections_threshold(&mut self) -> Result<Option<usize>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetMinimizeConnectionsThreshold(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Get all currently connection [PeerConnection](crate::PeerConnection]s.
    pub async fn get_active_connections(&mut self) -> Result<Vec<PeerConnection>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetActiveConnections(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Ban peer for the given Duration. The ban `reason` is persisted in the peer database for reference.
    pub async fn ban_peer_until<T: Into<String>>(
        &mut self,
        node_id: NodeId,
        duration: Duration,
        reason: T,
    ) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::BanPeer(node_id, duration, reason.into()))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    /// Ban the peer indefinitely.
    pub async fn ban_peer(&mut self, node_id: NodeId, reason: String) -> Result<(), ConnectivityError> {
        self.ban_peer_until(node_id, Duration::from_secs(u64::MAX), reason)
            .await
    }

    /// Adds a peer to an allow list, preventing it from being banned.
    pub async fn add_peer_to_allow_list(&mut self, node_id: NodeId) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::AddPeerToAllowList(node_id))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    /// Retrieve self's allow list.
    pub async fn get_allow_list(&mut self) -> Result<Vec<NodeId>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetAllowList(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Retrieve self's node identity.
    pub async fn get_node_identity(&mut self) -> Result<NodeIdentity, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetNodeIdentity(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    /// Removes a peer from an allow list that prevents it from being banned.
    pub async fn remove_peer_from_allow_list(&mut self, node_id: NodeId) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::RemovePeerFromAllowList(node_id))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    /// Returns a Future that resolves when the connectivity actor has started.
    pub async fn wait_started(&mut self) -> Result<(), ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::WaitStarted(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
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
                        warn!(target: LOG_TARGET, "Connectivity is DEGRADED ({} peer(s))", n);
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
                            "Received event while waiting for connectivity: {:?}", event
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
                    warn!(target: LOG_TARGET, "Lagging behind on {} connectivity event(s)", n);
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
