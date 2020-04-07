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

use crate::store_forward::{StoreAndForwardRequest, StoreAndForwardRequester, StoredMessage};
use chrono::Utc;
use futures::{channel::mpsc, stream::Fuse, StreamExt};
use log::*;
use rand::{rngs::OsRng, RngCore};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{runtime, sync::RwLock};

const LOG_TARGET: &str = "comms::dht::discovery_mock";

pub fn create_store_and_forward_mock() -> (StoreAndForwardRequester, StoreAndForwardMockState) {
    let (tx, rx) = mpsc::channel(10);

    let mock = StoreAndForwardMock::new(rx.fuse());
    let state = mock.get_shared_state();
    runtime::Handle::current().spawn(mock.run());
    (StoreAndForwardRequester::new(tx), state)
}

#[derive(Debug, Clone)]
pub struct StoreAndForwardMockState {
    call_count: Arc<AtomicUsize>,
    stored_messages: Arc<RwLock<Vec<StoredMessage>>>,
    calls: Arc<RwLock<Vec<String>>>,
}

impl StoreAndForwardMockState {
    pub fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            stored_messages: Arc::new(RwLock::new(Vec::new())),
            calls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    pub async fn add_call(&self, call: &StoreAndForwardRequest) {
        self.inc_call_count();
        self.calls.write().await.push(format!("{:?}", call));
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    pub async fn get_messages(&self) -> Vec<StoredMessage> {
        self.stored_messages.read().await.clone()
    }

    pub async fn add_message(&self, message: StoredMessage) {
        self.stored_messages.write().await.push(message)
    }

    pub async fn take_calls(&self) -> Vec<String> {
        self.calls.write().await.drain(..).collect()
    }
}

pub struct StoreAndForwardMock {
    receiver: Fuse<mpsc::Receiver<StoreAndForwardRequest>>,
    state: StoreAndForwardMockState,
}

impl StoreAndForwardMock {
    pub fn new(receiver: Fuse<mpsc::Receiver<StoreAndForwardRequest>>) -> Self {
        Self {
            receiver,
            state: StoreAndForwardMockState::new(),
        }
    }

    pub fn get_shared_state(&self) -> StoreAndForwardMockState {
        self.state.clone()
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: StoreAndForwardRequest) {
        use StoreAndForwardRequest::*;
        trace!(target: LOG_TARGET, "StoreAndForwardMock received request {:?}", req);
        self.state.add_call(&req).await;
        match req {
            FetchMessages(_, reply_tx) => {
                let msgs = self.state.stored_messages.read().await;
                let _ = reply_tx.send(Ok(msgs.clone()));
            },
            InsertMessage(msg) => self.state.stored_messages.write().await.push(StoredMessage {
                id: OsRng.next_u32() as i32,
                version: msg.version,
                origin_pubkey: msg.origin_pubkey,
                message_type: msg.message_type,
                destination_pubkey: msg.destination_pubkey,
                destination_node_id: msg.destination_node_id,
                header: msg.header,
                body: msg.body,
                is_encrypted: msg.is_encrypted,
                priority: msg.priority,
                stored_at: Utc::now().naive_utc(),
            }),
        }
    }
}
