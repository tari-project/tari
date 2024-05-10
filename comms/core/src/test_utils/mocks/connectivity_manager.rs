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

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::lock::Mutex;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time,
};

use crate::{
    connection_manager::{ConnectionManagerError, PeerConnection},
    connectivity::{
        ConnectivityEvent,
        ConnectivityEventTx,
        ConnectivityRequest,
        ConnectivityRequester,
        ConnectivityStatus,
    },
    peer_manager::NodeId,
};

pub fn create_connectivity_mock() -> (ConnectivityRequester, ConnectivityManagerMock) {
    let (tx, rx) = mpsc::channel(10);
    let (event_tx, _) = broadcast::channel(10);
    (
        ConnectivityRequester::new(tx, event_tx.clone()),
        ConnectivityManagerMock::new(rx, event_tx),
    )
}

#[derive(Debug, Clone)]
pub struct ConnectivityManagerMockState {
    inner: Arc<Mutex<State>>,
    event_tx: ConnectivityEventTx,
}

#[derive(Debug, Default)]
struct State {
    calls: Vec<String>,
    dialed_peers: Vec<NodeId>,
    active_conns: HashMap<NodeId, PeerConnection>,
    pending_conns: HashMap<NodeId, Vec<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>>,
    selected_connections: Vec<PeerConnection>,
    banned_peers: Vec<(NodeId, Duration, String)>,
    connectivity_status: ConnectivityStatus,
}

impl ConnectivityManagerMockState {
    pub fn new(event_tx: ConnectivityEventTx) -> Self {
        Self {
            event_tx,
            inner: Default::default(),
        }
    }

    pub async fn wait_until_event_receivers_ready(&self) {
        let mut timeout = 0;
        while self.event_tx.receiver_count() == 0 {
            time::sleep(Duration::from_millis(10)).await;
            timeout += 10;
            if timeout > 5000 {
                panic!("Event receiver not ready after 5 secs");
            }
        }
    }

    async fn add_call(&self, call_str: String) {
        self.with_state(|state| state.calls.push(call_str)).await
    }

    pub async fn take_calls(&self) -> Vec<String> {
        self.with_state(|state| state.calls.drain(..).collect()).await
    }

    pub async fn count_calls_containing(&self, pat: &str) -> usize {
        self.with_state(|state| state.calls.iter().filter(|s| s.contains(pat)).count())
            .await
    }

    pub async fn get_selected_connections(&self) -> Vec<PeerConnection> {
        self.with_state(|state| state.selected_connections.clone()).await
    }

    pub async fn set_selected_connections(&self, conns: Vec<PeerConnection>) {
        self.with_state(|state| {
            state.selected_connections = conns;
        })
        .await
    }

    pub async fn get_dialed_peers(&self) -> Vec<NodeId> {
        self.with_state(|state| state.dialed_peers.clone()).await
    }

    pub async fn take_dialed_peers(&self) -> Vec<NodeId> {
        self.with_state(|state| state.dialed_peers.drain(..).collect()).await
    }

    pub async fn clear_dialed_peers(&self) {
        self.with_state(|state| {
            state.dialed_peers.clear();
        })
        .await
    }

    pub async fn add_dialed_peer(&self, node_id: NodeId) {
        self.with_state(|state| {
            state.dialed_peers.push(node_id);
        })
        .await
    }

    pub async fn set_connectivity_status(&self, status: ConnectivityStatus) {
        self.with_state(|state| {
            state.connectivity_status = status;
        })
        .await
    }

    #[allow(dead_code)]
    pub async fn call_count(&self) -> usize {
        self.with_state(|state| state.calls.len()).await
    }

    pub async fn expect_dial_peer(&self, peer: &NodeId) {
        let is_found = self.with_state(|state| state.dialed_peers.contains(peer)).await;
        assert!(is_found, "expected call to dial peer {} but no dial was found", peer);
    }

    pub async fn await_call_count(&self, count: usize) {
        let mut attempts = 0;
        while self.call_count().await < count {
            attempts += 1;
            assert!(
                attempts <= 10,
                "expected call count to equal {} within 1 second but it was {}",
                count,
                self.call_count().await
            );
            time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn add_active_connection(&self, conn: PeerConnection) {
        self.with_state(|state| {
            let peer = conn.peer_node_id();
            state.active_conns.insert(peer.clone(), conn.clone());
            if let Some(replies) = state.pending_conns.remove(peer) {
                replies.into_iter().for_each(|reply| {
                    reply.send(Ok(conn.clone())).unwrap();
                });
            }
        })
        .await
    }

    pub async fn set_pending_connection(&self, peer: &NodeId) {
        self.with_state(|state| {
            state.pending_conns.entry(peer.clone()).or_default();
        })
        .await
    }

    pub fn publish_event(&self, event: ConnectivityEvent) {
        self.event_tx.send(event).unwrap();
    }

    pub async fn take_banned_peers(&self) -> Vec<(NodeId, Duration, String)> {
        self.with_state(|state| state.banned_peers.drain(..).collect()).await
    }

    pub(self) async fn with_state<F, R>(&self, f: F) -> R
    where F: FnOnce(&mut State) -> R {
        let mut lock = self.inner.lock().await;
        (f)(&mut lock)
    }
}

pub struct ConnectivityManagerMock {
    receiver: mpsc::Receiver<ConnectivityRequest>,
    state: ConnectivityManagerMockState,
}

impl ConnectivityManagerMock {
    pub fn new(receiver: mpsc::Receiver<ConnectivityRequest>, event_tx: ConnectivityEventTx) -> Self {
        Self {
            receiver,
            state: ConnectivityManagerMockState::new(event_tx),
        }
    }

    pub fn get_shared_state(&self) -> ConnectivityManagerMockState {
        self.state.clone()
    }

    pub fn spawn(self) -> ConnectivityManagerMockState {
        let state = self.get_shared_state();
        tokio::spawn(Self::run(self));
        state
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.recv().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: ConnectivityRequest) {
        #[allow(clippy::enum_glob_use)]
        use ConnectivityRequest::*;
        self.state.add_call(format!("{:?}", req)).await;
        match req {
            DialPeer { node_id, reply_tx } => {
                self.state.add_dialed_peer(node_id.clone()).await;
                // No reply, no reason to do anything in the mock
                if reply_tx.is_none() {
                    return;
                }
                let reply_tx = reply_tx.unwrap();
                // Send Ok(&mut conn) if we have an active connection, otherwise Err(DialConnectFailedAllAddresses)
                self.state
                    .with_state(|state| match state.pending_conns.get_mut(&node_id) {
                        Some(replies) => {
                            replies.push(reply_tx);
                        },
                        None => {
                            let _result = reply_tx.send(
                                state
                                    .active_conns
                                    .get(&node_id)
                                    .cloned()
                                    .ok_or(ConnectionManagerError::DialConnectFailedAllAddresses)
                                    .map_err(Into::into),
                            );
                        },
                    })
                    .await;
            },
            GetConnectivityStatus(reply) => {
                self.state
                    .with_state(|state| {
                        reply.send(state.connectivity_status).unwrap();
                    })
                    .await;
            },
            SelectConnections(_, reply) => {
                reply.send(Ok(self.state.get_selected_connections().await)).unwrap();
            },
            GetConnection(node_id, reply) => {
                self.state
                    .with_state(|state| {
                        reply.send(state.active_conns.get(&node_id).cloned()).unwrap();
                    })
                    .await
            },
            GetPeerStats(_, _) => {
                unimplemented!()
            },
            GetAllConnectionStates(_) => unimplemented!(),
            BanPeer(node_id, duration, reason) => {
                self.state
                    .with_state(|state| {
                        state.banned_peers.push((node_id, duration, reason));
                    })
                    .await
            },
            AddPeerToAllowList(_) => {},
            RemovePeerFromAllowList(_) => {},
            GetActiveConnections(reply) => {
                self.state
                    .with_state(|state| reply.send(state.active_conns.values().cloned().collect()).unwrap())
                    .await;
            },
            WaitStarted(reply) => reply.send(()).unwrap(),
            GetNodeIdentity(_) => unimplemented!(),
            GetAllowList(reply) => {
                let _result = reply.send(vec![]);
            },
            GetMinimizeConnectionsThreshold(_) => unimplemented!(),
        }
    }
}
