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
use futures::{
    channel::{mpsc, oneshot},
    lock::Mutex,
    stream::Fuse,
    StreamExt,
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{sync::broadcast, time};

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
    inner: Arc<Mutex<State>>,
    event_tx: broadcast::Sender<Arc<ConnectivityEvent>>,
}

#[derive(Debug, Default)]
struct State {
    calls: Vec<String>,
    active_conns: HashMap<NodeId, PeerConnection>,
    pending_conns: HashMap<NodeId, Vec<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>>,
    selected_connections: Vec<PeerConnection>,
    managed_peers: Vec<NodeId>,
    connectivity_status: ConnectivityStatus,
}

impl ConnectivityManagerMockState {
    pub fn new(event_tx: broadcast::Sender<Arc<ConnectivityEvent>>) -> Self {
        Self {
            event_tx,
            inner: Default::default(),
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

    pub async fn set_connectivity_status(&self, status: ConnectivityStatus) {
        self.with_state(|state| {
            state.connectivity_status = status;
        })
        .await
    }

    pub async fn get_managed_peers(&self) -> Vec<NodeId> {
        self.with_state(|state| state.managed_peers.clone()).await
    }

    #[allow(dead_code)]
    pub async fn call_count(&self) -> usize {
        self.with_state(|state| state.calls.len()).await
    }

    pub async fn expect_dial_peer(&self, peer: &NodeId) {
        let is_found = self
            .with_state(|state| {
                let peer_str = peer.to_string();
                state
                    .calls
                    .iter()
                    .any(|s| s.contains("DialPeer") && s.contains(&peer_str))
            })
            .await;

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
            time::delay_for(Duration::from_millis(100)).await;
        }
    }

    pub async fn add_active_connection(&self, conn: PeerConnection) {
        self.with_state(|state| {
            let peer = conn.peer_node_id();
            state.active_conns.insert(peer.clone(), conn.clone());
            if let Some(replies) = state.pending_conns.remove(&peer) {
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

    #[allow(dead_code)]
    pub fn publish_event(&self, event: ConnectivityEvent) {
        self.event_tx.send(Arc::new(event)).unwrap();
    }

    pub(self) async fn with_state<F, R>(&self, f: F) -> R
    where F: FnOnce(&mut State) -> R {
        let mut lock = self.inner.lock().await;
        (f)(&mut *lock)
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
    ) -> Self {
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
        task::spawn(Self::run(self));
        state
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
                self.state
                    .with_state(|state| match state.pending_conns.get_mut(&node_id) {
                        Some(replies) => {
                            replies.push(reply);
                        },
                        None => {
                            let _ = reply.send(
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
            AddManagedPeers(peers) => {
                // TODO: we should not have to implement behaviour of the actor in the mock
                //       but should rather have a _good_ way to check the call to the mock
                //       was made with the correct arguments.
                self.state
                    .with_state(|state| {
                        let managed_peers = &mut state.managed_peers;
                        for peer in peers {
                            if !managed_peers.contains(&peer) {
                                managed_peers.push(peer.clone());
                            }
                        }
                    })
                    .await
            },
            RemovePeer(node_id) => {
                self.state
                    .with_state(|state| {
                        if let Some(pos) = state.managed_peers.iter().position(|n| n == &node_id) {
                            state.managed_peers.remove(pos);
                        }
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
            GetAllConnectionStates(_) => unimplemented!(),
            BanPeer(_, _, _) => {},
            GetActiveConnections(reply) => {
                self.state
                    .with_state(|state| reply.send(state.active_conns.values().cloned().collect()).unwrap())
                    .await;
            },
            WaitStarted(reply) => reply.send(()).unwrap(),
        }
    }
}
