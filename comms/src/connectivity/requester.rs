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
    connection_pool::PeerConnectionState,
    error::ConnectivityError,
    manager::ConnectivityStatus,
    ConnectivitySelection,
};
use crate::{connection_manager::ConnectionManagerError, peer_manager::NodeId, PeerConnection};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
    StreamExt,
};
use log::*;
use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::broadcast, time};

const LOG_TARGET: &str = "comms::connectivity::requester";

pub type ConnectivityEventRx = broadcast::Receiver<Arc<ConnectivityEvent>>;
pub type ConnectivityEventTx = broadcast::Sender<Arc<ConnectivityEvent>>;

#[derive(Debug, Clone)]
pub enum ConnectivityEvent {
    PeerDisconnected(NodeId),
    ManagedPeerDisconnected(NodeId),
    PeerConnected(PeerConnection),
    PeerConnectFailed(NodeId),
    ManagedPeerConnectFailed(NodeId),
    PeerBanned(NodeId),
    PeerOffline(NodeId),

    ConnectivityStateInitialized,
    ConnectivityStateOnline(usize),
    ConnectivityStateDegraded(usize),
    ConnectivityStateOffline,
}

impl fmt::Display for ConnectivityEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ConnectivityEvent::*;
        match self {
            PeerDisconnected(node_id) => write!(f, "PeerDisconnected({})", node_id),
            ManagedPeerDisconnected(node_id) => write!(f, "ManagedPeerDisconnected({})", node_id),
            PeerConnected(node_id) => write!(f, "PeerConnected({})", node_id),
            PeerConnectFailed(node_id) => write!(f, "PeerConnectFailed({})", node_id),
            ManagedPeerConnectFailed(node_id) => write!(f, "ManagedPeerConnectFailed({})", node_id),
            PeerBanned(node_id) => write!(f, "PeerBanned({})", node_id),
            PeerOffline(node_id) => write!(f, "PeerOffline({})", node_id),
            ConnectivityStateInitialized => write!(f, "ConnectivityStateInitialized"),
            ConnectivityStateOnline(n) => write!(f, "ConnectivityStateOnline({})", n),
            ConnectivityStateDegraded(n) => write!(f, "ConnectivityStateDegraded({})", n),
            ConnectivityStateOffline => write!(f, "ConnectivityStateOffline"),
        }
    }
}

#[derive(Debug)]
pub enum ConnectivityRequest {
    DialPeer(NodeId, oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>),
    GetConnectivityStatus(oneshot::Sender<ConnectivityStatus>),
    AddManagedPeers(Vec<NodeId>),
    RemovePeer(NodeId),
    SelectConnections(
        ConnectivitySelection,
        oneshot::Sender<Result<Vec<PeerConnection>, ConnectivityError>>,
    ),
    GetConnection(NodeId, oneshot::Sender<Option<PeerConnection>>),
    GetAllConnectionStates(oneshot::Sender<Vec<PeerConnectionState>>),
    BanPeer(NodeId, Duration),
}

#[derive(Debug, Clone)]
pub struct ConnectivityRequester {
    sender: mpsc::Sender<ConnectivityRequest>,
    event_tx: ConnectivityEventTx,
}

impl ConnectivityRequester {
    pub fn new(sender: mpsc::Sender<ConnectivityRequest>, event_tx: ConnectivityEventTx) -> Self {
        Self { sender, event_tx }
    }

    pub fn subscribe_event_stream(&self) -> ConnectivityEventRx {
        self.event_tx.subscribe()
    }

    pub async fn dial_peer(&mut self, peer: NodeId) -> Result<PeerConnection, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::DialPeer(peer, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx
            .await
            .map_err(|_| ConnectivityError::ActorResponseCancelled)?
            .map_err(Into::into)
    }

    pub async fn add_managed_peers(&mut self, peers: Vec<NodeId>) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::AddManagedPeers(peers))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    pub async fn remove_peer(&mut self, peer: NodeId) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::RemovePeer(peer))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    pub async fn select_connections(
        &mut self,
        selection: ConnectivitySelection,
    ) -> Result<Vec<PeerConnection>, ConnectivityError>
    {
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

    pub async fn get_connectivity_status(&mut self) -> Result<ConnectivityStatus, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetConnectivityStatus(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    pub async fn get_all_connection_states(&mut self) -> Result<Vec<PeerConnectionState>, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetAllConnectionStates(reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)
    }

    pub async fn ban_peer(&mut self, node_id: NodeId, duration: Duration) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::BanPeer(node_id, duration))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    /// Waits for the node to get at least one connection.
    /// This is useful for testing and is not typically be needed in application code.
    pub async fn wait_for_connectivity(&mut self, timeout: Duration) -> Result<(), ConnectivityError> {
        let mut connectivity_events = self.subscribe_event_stream();
        let status = self.get_connectivity_status().await?;
        if status.is_online() {
            return Ok(());
        }
        let start = Instant::now();
        let mut remaining = timeout;

        loop {
            debug!(target: LOG_TARGET, "Waiting for connectivity event");
            let recv_result = time::timeout(remaining, connectivity_events.next())
                .await
                .map_err(|_| ConnectivityError::OnlineWaitTimeout)?
                .ok_or_else(|| ConnectivityError::ConnectivityEventStreamClosed)?;

            remaining = timeout
                .checked_sub(start.elapsed())
                .ok_or_else(|| ConnectivityError::OnlineWaitTimeout)?;

            match recv_result {
                Ok(event) => match &*event {
                    ConnectivityEvent::ConnectivityStateOnline(_) => {
                        info!(target: LOG_TARGET, "Connectivity is ONLINE.");
                        break Ok(());
                    },
                    ConnectivityEvent::ConnectivityStateDegraded(_) => {
                        warn!(target: LOG_TARGET, "Connectivity is DEGRADED.");
                    },
                    ConnectivityEvent::ConnectivityStateOffline => {
                        warn!(
                            target: LOG_TARGET,
                            "Connectivity is OFFLINE. Waiting for connections..."
                        );
                    },
                    event => {
                        debug!(
                            target: LOG_TARGET,
                            "Received event while waiting for connectivity: {:?}", event
                        );
                    },
                },
                Err(broadcast::RecvError::Closed) => {
                    error!(
                        target: LOG_TARGET,
                        "Connectivity event stream closed unexpectedly. System may be shutting down."
                    );
                    break Err(ConnectivityError::ConnectivityEventStreamClosed);
                },
                Err(broadcast::RecvError::Lagged(n)) => {
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
