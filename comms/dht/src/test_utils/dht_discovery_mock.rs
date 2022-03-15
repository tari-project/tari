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

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
        RwLock,
    },
    time::Duration,
};

use log::*;
use tari_comms::peer_manager::Peer;
use tokio::{sync::mpsc, task};

use crate::{
    discovery::{DhtDiscoveryRequest, DhtDiscoveryRequester},
    DhtDiscoveryError,
};

const LOG_TARGET: &str = "comms::dht::discovery_mock";

pub fn create_dht_discovery_mock(timeout: Duration) -> (DhtDiscoveryRequester, DhtDiscoveryMock) {
    let (tx, rx) = mpsc::channel(10);
    (DhtDiscoveryRequester::new(tx, timeout), DhtDiscoveryMock::new(rx))
}

#[derive(Debug, Clone)]
pub struct DhtDiscoveryMockState {
    call_count: Arc<AtomicUsize>,
    discover_peer: Arc<RwLock<Option<Peer>>>,
}

impl DhtDiscoveryMockState {
    pub fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            discover_peer: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_discover_peer_response(&self, peer: Peer) -> &Self {
        *self.discover_peer.write().unwrap() = Some(peer);
        self
    }

    pub fn inc_call_count(&self) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

pub struct DhtDiscoveryMock {
    receiver: mpsc::Receiver<DhtDiscoveryRequest>,
    state: DhtDiscoveryMockState,
}

impl DhtDiscoveryMock {
    pub fn new(receiver: mpsc::Receiver<DhtDiscoveryRequest>) -> Self {
        Self {
            receiver,
            state: DhtDiscoveryMockState::new(),
        }
    }

    pub fn get_shared_state(&self) -> DhtDiscoveryMockState {
        self.state.clone()
    }

    pub fn spawn(mut self) {
        task::spawn(async move {
            while let Some(req) = self.receiver.recv().await {
                self.handle_request(req).await;
            }
        });
    }

    async fn handle_request(&self, req: DhtDiscoveryRequest) {
        use DhtDiscoveryRequest::*;
        trace!(target: LOG_TARGET, "DhtDiscoveryMock received request {:?}", req);
        self.state.inc_call_count();
        match req {
            DiscoverPeer(_, _, reply_tx) => {
                let lock = self.state.discover_peer.read().unwrap();
                reply_tx
                    .send(lock.clone().ok_or(DhtDiscoveryError::DiscoveryTimeout))
                    .unwrap();
            },
            NotifyDiscoveryResponseReceived(_) => {},
        }
    }
}
