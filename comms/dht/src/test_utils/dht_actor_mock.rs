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
#![allow(dead_code)]

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
        RwLock,
    },
};

use tari_comms::peer_manager::Peer;
use tokio::{sync::mpsc, task};

use crate::{
    actor::{DhtRequest, DhtRequester},
    storage::DhtMetadataKey,
};

pub fn create_dht_actor_mock(buf_size: usize) -> (DhtRequester, DhtActorMock) {
    let (tx, rx) = mpsc::channel(buf_size);
    (DhtRequester::new(tx), DhtActorMock::new(rx))
}

#[derive(Default, Debug, Clone)]
pub struct DhtMockState {
    signature_cache_insert: Arc<AtomicUsize>,
    call_count: Arc<AtomicUsize>,
    select_peers: Arc<RwLock<Vec<Peer>>>,
    settings: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl DhtMockState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn set_number_of_message_hits(&self, v: u32) -> &Self {
        self.signature_cache_insert.store(v as usize, Ordering::SeqCst);
        self
    }

    pub fn set_select_peers_response(&self, peers: Vec<Peer>) -> &Self {
        *self.select_peers.write().unwrap() = peers;
        self
    }

    pub fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get_setting(&self, key: &DhtMetadataKey) -> Option<Vec<u8>> {
        self.settings.read().unwrap().get(&key.to_string()).map(Clone::clone)
    }
}

pub struct DhtActorMock {
    receiver: mpsc::Receiver<DhtRequest>,
    state: DhtMockState,
}

impl DhtActorMock {
    pub fn new(receiver: mpsc::Receiver<DhtRequest>) -> Self {
        Self {
            receiver,
            state: DhtMockState::default(),
        }
    }

    pub fn get_shared_state(&self) -> DhtMockState {
        self.state.clone()
    }

    pub fn spawn(self) {
        task::spawn(Self::run(self));
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.recv().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: DhtRequest) {
        use DhtRequest::*;
        self.state.inc_call_count();
        match req {
            SendJoin => {},
            MsgHashCacheInsert { reply_tx, .. } => {
                let v = self.state.signature_cache_insert.load(Ordering::SeqCst);
                reply_tx.send(v as u32).unwrap();
            },
            GetMsgHashHitCount(_, reply_tx) => {
                let v = self.state.signature_cache_insert.load(Ordering::SeqCst);
                reply_tx.send(v as u32).unwrap();
            },
            SelectPeers(_, reply_tx) => {
                let lock = self.state.select_peers.read().unwrap();
                reply_tx
                    .send(lock.iter().cloned().map(|p| p.node_id).collect())
                    .unwrap();
            },
            GetMetadata(key, reply_tx) => {
                let _ = reply_tx.send(Ok(self
                    .state
                    .settings
                    .read()
                    .unwrap()
                    .get(&key.to_string())
                    .map(Clone::clone)));
            },
            SetMetadata(key, value, reply_tx) => {
                self.state.settings.write().unwrap().insert(key.to_string(), value);
                reply_tx.send(Ok(())).unwrap();
            },
            DialDiscoverPeer { .. } => unimplemented!(),
        }
    }
}
