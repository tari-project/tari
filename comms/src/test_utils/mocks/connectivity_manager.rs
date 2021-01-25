// Copyright 2020, The Tari Project
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

use crate::{
    connection_manager::{ConnectionManagerError, PeerConnection},
    connectivity::{ConnectivityEvent, ConnectivityRequest, ConnectivityRequester, ConnectivityStatus},
    peer_manager::NodeId,
    runtime::task,
};
use futures::{channel::mpsc, lock::Mutex, stream::Fuse, StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::broadcast;

pub fn create_connectivity_mock() -> (ConnectivityRequester, ConnectivityManagerMock) {
    let (tx, rx) = mpsc::channel(10);
    let (event_tx, _) = broadcast::channel(10);
    (
        ConnectivityRequester::new(tx, event_tx.clone()),
        ConnectivityManagerMock::new(rx.fuse(), event_tx),
    )
}

#[derive(Debug, Clone)]
pub struct ConnectivityManagerMockState {
    calls: Arc<Mutex<Vec<String>>>,
    active_conns: Arc<Mutex<HashMap<NodeId, PeerConnection>>>,
    selected_connections: Arc<Mutex<Vec<PeerConnection>>>,
    managed_peers: Arc<Mutex<Vec<NodeId>>>,
    event_tx: broadcast::Sender<Arc<ConnectivityEvent>>,
    connectivity_status: Arc<Mutex<ConnectivityStatus>>,
}

impl ConnectivityManagerMockState {
    pub fn new(event_tx: broadcast::Sender<Arc<ConnectivityEvent>>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            event_tx,
            selected_connections: Arc::new(Mutex::new(Vec::new())),
            managed_peers: Arc::new(Mutex::new(Vec::new())),
            active_conns: Arc::new(Mutex::new(HashMap::new())),
            connectivity_status: Arc::new(Mutex::new(ConnectivityStatus::Initializing)),
        }
    }

    async fn add_call(&self, call_str: String) {
        self.calls.lock().await.push(call_str);
    }

    pub async fn take_calls(&self) -> Vec<String> {
        self.calls.lock().await.drain(..).collect()
    }

    pub async fn get_selected_connections(&self) -> Vec<PeerConnection> {
        self.selected_connections.lock().await.clone()
    }

    pub async fn set_selected_connections(&self, conns: Vec<PeerConnection>) {
        *self.selected_connections.lock().await = conns;
    }

    pub async fn set_connectivity_status(&self, status: ConnectivityStatus) {
        *self.connectivity_status.lock().await = status;
    }

    pub async fn get_managed_peers(&self) -> Vec<NodeId> {
        self.managed_peers.lock().await.clone()
    }

    #[allow(dead_code)]
    pub async fn call_count(&self) -> usize {
        self.calls.lock().await.len()
    }

    pub async fn add_active_connection(&self, conn: PeerConnection) {
        self.active_conns.lock().await.insert(conn.peer_node_id().clone(), conn);
    }

    #[allow(dead_code)]
    pub fn publish_event(&self, event: ConnectivityEvent) {
        self.event_tx.send(Arc::new(event)).unwrap();
    }
}

pub struct ConnectivityManagerMock {
    receiver: Fuse<mpsc::Receiver<ConnectivityRequest>>,
    state: ConnectivityManagerMockState,
}

impl ConnectivityManagerMock {
    pub fn new(
        receiver: Fuse<mpsc::Receiver<ConnectivityRequest>>,
        event_tx: broadcast::Sender<Arc<ConnectivityEvent>>,
    ) -> Self
    {
        Self {
            receiver,
            state: ConnectivityManagerMockState::new(event_tx),
        }
    }

    pub fn get_shared_state(&self) -> ConnectivityManagerMockState {
        self.state.clone()
    }

    pub fn spawn(self) {
        task::spawn(Self::run(self));
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: ConnectivityRequest) {
        use ConnectivityRequest::*;
        self.state.add_call(format!("{:?}", req)).await;
        match req {
            DialPeer(node_id, reply) => {
                // Send Ok(conn) if we have an active connection, otherwise Err(DialConnectFailedAllAddresses)
                let _ = reply.send(
                    self.state
                        .active_conns
                        .lock()
                        .await
                        .get(&node_id)
                        .cloned()
                        .ok_or_else(|| ConnectionManagerError::DialConnectFailedAllAddresses)
                        .map_err(Into::into),
                );
            },
            GetConnectivityStatus(reply) => {
                reply.send(*self.state.connectivity_status.lock().await).unwrap();
            },
            AddManagedPeers(peers) => {
                // TODO: we should not have to implement behaviour of the actor in the mock
                //       but should rather have a _good_ way to check the call to the mock
                //       was made with the correct arguments.
                let mut lock = self.state.managed_peers.lock().await;
                for peer in peers {
                    if !lock.contains(&peer) {
                        lock.push(peer.clone());
                    }
                }
            },
            RemovePeer(node_id) => {
                let mut lock = self.state.managed_peers.lock().await;
                if let Some(pos) = lock.iter().position(|n| n == &node_id) {
                    lock.remove(pos);
                }
            },
            SelectConnections(_, reply) => {
                reply.send(Ok(self.state.get_selected_connections().await)).unwrap();
            },
            GetConnection(node_id, reply) => {
                reply
                    .send(self.state.active_conns.lock().await.get(&node_id).cloned())
                    .unwrap();
            },
            GetAllConnectionStates(_) => unimplemented!(),
            BanPeer(_, _, _) => {},
            GetActiveConnections(reply) => {
                reply
                    .send(self.state.active_conns.lock().await.values().cloned().collect())
                    .unwrap();
            },
            WaitStarted(reply) => reply.send(()).unwrap(),
        }
    }
}
