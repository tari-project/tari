// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::{error::ConnectionManagerError, peer_connection::PeerConnection};
use crate::{connection_manager::manager::ConnectionManagerEvent, multiaddr::Multiaddr, peer_manager::NodeId};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Requests which are handled by the ConnectionManagerService
#[derive(Debug)]
pub enum ConnectionManagerRequest {
    /// Dial a given peer by node id.
    /// Parameters:
    /// 1. Node Id to dial
    DialPeer(NodeId, oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>),
    /// Register a oneshot to get triggered when the node is listening, or has failed to listen
    NotifyListening(oneshot::Sender<Multiaddr>),
    /// Retrieve an active connection for a given node id if one exists.
    GetActiveConnection(NodeId, oneshot::Sender<Option<PeerConnection>>),
    /// Retrieve all active connections
    GetActiveConnections(oneshot::Sender<Vec<PeerConnection>>),
    /// Retrieve the number of active connections
    GetNumActiveConnections(oneshot::Sender<usize>),
    /// Disconnect a peer
    DisconnectPeer(NodeId, oneshot::Sender<Result<(), ConnectionManagerError>>),
}

/// Responsible for constructing requests to the ConnectionManagerService
#[derive(Clone)]
pub struct ConnectionManagerRequester {
    sender: mpsc::Sender<ConnectionManagerRequest>,
    event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
}

impl ConnectionManagerRequester {
    /// Create a new ConnectionManagerRequester
    pub fn new(
        sender: mpsc::Sender<ConnectionManagerRequest>,
        event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    ) -> Self
    {
        Self { sender, event_tx }
    }
}

macro_rules! request_fn {
   ($name:ident($($param:ident:$param_ty:ty),+) -> $ret:ty, request = $($request:ident)::+ $(,)?) => {
        pub async fn $name(
            &mut self,
            $($param: $param_ty),+
        ) -> Result<$ret, ConnectionManagerError>
        {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.sender
                .send($($request)::+($($param),+, reply_tx))
                .await
                .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
            reply_rx.await.map_err(|_| ConnectionManagerError::ActorRequestCanceled)
        }
   };
   ($name:ident() -> $ret:ty, request = $($request:ident)::+ $(,)?) => {
        pub async fn $name(&mut self) -> Result<$ret, ConnectionManagerError>
        {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.sender
                .send($($request)::+(reply_tx))
                .await
                .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
            reply_rx.await.map_err(|_| ConnectionManagerError::ActorRequestCanceled)
        }
   };
}

impl ConnectionManagerRequester {
    request_fn!(get_active_connections() -> Vec<PeerConnection>, request = ConnectionManagerRequest::GetActiveConnections);

    request_fn!(get_num_active_connections() -> usize, request = ConnectionManagerRequest::GetNumActiveConnections);

    request_fn!(get_active_connection(node_id: NodeId) -> Option<PeerConnection>, request = ConnectionManagerRequest::GetActiveConnection);

    request_fn!(disconnect_peer(node_id: NodeId) -> Result<(), ConnectionManagerError>, request = ConnectionManagerRequest::DisconnectPeer);

    /// Returns a ConnectionManagerEvent stream
    pub fn get_event_subscription(&self) -> broadcast::Receiver<Arc<ConnectionManagerEvent>> {
        self.event_tx.subscribe()
    }

    /// Attempt to connect to a remote peer
    pub async fn dial_peer(&mut self, node_id: NodeId) -> Result<PeerConnection, ConnectionManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectionManagerRequest::DialPeer(node_id, reply_tx))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
        reply_rx
            .await
            .map_err(|_| ConnectionManagerError::ActorRequestCanceled)?
    }

    /// Return the listening address of this node's listener. This will asynchronously block until the listener has
    /// initialized and a listening address has been established.
    ///
    /// This is useful when using "assigned port" addresses, such as /ip4/0.0.0.0/tcp/0 or /memory/0 for listening and
    /// you wish to know the final assigned port.
    pub async fn wait_until_listening(&mut self) -> Result<Multiaddr, ConnectionManagerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectionManagerRequest::NotifyListening(reply_tx))
            .await
            .map_err(|_| ConnectionManagerError::SendToActorFailed)?;
        reply_rx.await.map_err(|_| ConnectionManagerError::ActorRequestCanceled)
    }
}
