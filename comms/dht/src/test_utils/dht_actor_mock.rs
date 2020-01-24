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

use crate::actor::{DhtRequest, DhtRequester};
use futures::{channel::mpsc, stream::Fuse, StreamExt};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
    RwLock,
};
use tari_comms::peer_manager::Peer;

pub fn create_dht_actor_mock(buf_size: usize) -> (DhtRequester, DhtActorMock) {
    let (tx, rx) = mpsc::channel(buf_size);
    (DhtRequester::new(tx), DhtActorMock::new(rx.fuse()))
}

#[derive(Default, Debug, Clone)]
pub struct DhtMockState {
    signature_cache_insert: Arc<AtomicBool>,
    call_count: Arc<AtomicUsize>,
    select_peers: Arc<RwLock<Vec<Peer>>>,
}

impl DhtMockState {
    pub fn new() -> Self {
        Self {
            signature_cache_insert: Arc::new(AtomicBool::new(false)),
            call_count: Arc::new(AtomicUsize::new(0)),
            select_peers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn set_signature_cache_insert(&self, v: bool) -> &Self {
        self.signature_cache_insert.store(v, Ordering::SeqCst);
        self
    }

    pub fn set_select_peers_response(&self, peers: Vec<Peer>) -> &Self {
        *acquire_write_lock!(self.select_peers) = peers;
        self
    }

    pub fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

pub struct DhtActorMock {
    receiver: Fuse<mpsc::Receiver<DhtRequest>>,
    state: DhtMockState,
}

impl DhtActorMock {
    pub fn new(receiver: Fuse<mpsc::Receiver<DhtRequest>>) -> Self {
        Self {
            receiver,
            state: DhtMockState::default(),
        }
    }

    pub fn set_shared_state(&mut self, state: DhtMockState) {
        self.state = state;
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: DhtRequest) {
        use DhtRequest::*;
        self.state.inc_call_count();
        match req {
            SendJoin => {},
            MsgHashCacheInsert(_, reply_tx) => {
                let v = self.state.signature_cache_insert.load(Ordering::SeqCst);
                reply_tx.send(v).unwrap();
            },
            SelectPeers(_, reply_tx) => {
                let lock = self.state.select_peers.read().unwrap();
                reply_tx.send(lock.clone()).unwrap();
            },
            SendRequestStoredMessages(_) => {},
        }
    }
}
