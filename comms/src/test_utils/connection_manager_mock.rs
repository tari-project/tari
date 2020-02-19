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
    connection_manager::{
        next::{ConnectionManagerError, ConnectionManagerEvent, ConnectionManagerRequest, ConnectionManagerRequester},
        PeerConnection,
    },
    peer_manager::NodeId,
};
use futures::{channel::mpsc, lock::Mutex, stream::Fuse, StreamExt};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::broadcast;

pub fn create_connection_manager_mock(buf_size: usize) -> (ConnectionManagerRequester, ConnectionManagerMock) {
    let (tx, rx) = mpsc::channel(buf_size);
    let (event_tx, _) = broadcast::channel(buf_size);
    (
        ConnectionManagerRequester::new(tx, event_tx.clone()),
        ConnectionManagerMock::new(rx.fuse(), event_tx),
    )
}

#[derive(Debug, Clone)]
pub struct ConnectionManagerMockState {
    call_count: Arc<AtomicUsize>,
    active_conns: Arc<Mutex<HashMap<NodeId, PeerConnection>>>,
    event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
}

impl ConnectionManagerMockState {
    pub fn new(event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            event_tx,
            active_conns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub async fn add_active_connection(&self, node_id: NodeId, conn: PeerConnection) {
        self.active_conns.lock().await.insert(node_id, conn);
    }

    #[allow(dead_code)]
    pub fn publish_event(&mut self, event: ConnectionManagerEvent) {
        self.event_tx.send(Arc::new(event)).unwrap();
    }
}

pub struct ConnectionManagerMock {
    receiver: Fuse<mpsc::Receiver<ConnectionManagerRequest>>,
    state: ConnectionManagerMockState,
}

impl ConnectionManagerMock {
    pub fn new(
        receiver: Fuse<mpsc::Receiver<ConnectionManagerRequest>>,
        event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    ) -> Self
    {
        Self {
            receiver,
            state: ConnectionManagerMockState::new(event_tx),
        }
    }

    pub fn get_shared_state(&self) -> ConnectionManagerMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: ConnectionManagerRequest) {
        use ConnectionManagerRequest::*;
        self.state.inc_call_count();
        match req {
            DialPeer(node_id, reply_tx) => {
                // Send Ok(conn) if we have an active connection, otherwise Err(DialConnectFailedAllAddresses)
                reply_tx
                    .send(
                        self.state
                            .active_conns
                            .lock()
                            .await
                            .get(&node_id)
                            .map(Clone::clone)
                            .ok_or_else(|| ConnectionManagerError::DialConnectFailedAllAddresses),
                    )
                    .unwrap();
            },
            NotifyListening(_reply_tx) => {},
            GetActiveConnection(node_id, reply_tx) => {
                reply_tx
                    .send(self.state.active_conns.lock().await.get(&node_id).map(Clone::clone))
                    .unwrap();
            },
        }
    }
}
