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

use crate::services::liveness::{
    error::LivenessError,
    LivenessEvent,
    LivenessHandle,
    LivenessRequest,
    LivenessResponse,
};
use futures::{SinkExt, StreamExt};
use log::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
    RwLock,
};
use tari_broadcast_channel as broadcast_channel;
use tari_broadcast_channel::{Publisher, SendError};
use tari_service_framework::{reply_channel, RequestContext};
use tari_utilities::acquire_write_lock;

const LOG_TARGET: &str = "base_layer::p2p::liveness_mock";

pub fn create_p2p_liveness_mock(buf_size: usize) -> (LivenessHandle, LivenessMock) {
    let (sender, receiver) = reply_channel::unbounded();
    let (publisher, subscriber) = broadcast_channel::bounded(buf_size);
    (
        LivenessHandle::new(sender, subscriber),
        LivenessMock::new(receiver, LivenessMockState::new(publisher)),
    )
}

#[derive(Debug, Clone)]
pub struct LivenessMockState {
    call_count: Arc<AtomicUsize>,
    event_publisher: Arc<RwLock<Publisher<LivenessEvent>>>,
    calls: Arc<RwLock<Vec<LivenessRequest>>>,
}

impl LivenessMockState {
    pub fn new(event_publisher: Publisher<LivenessEvent>) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            event_publisher: Arc::new(RwLock::new(event_publisher)),
            calls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn publish_event(&self, event: LivenessEvent) -> Result<(), SendError<LivenessEvent>> {
        acquire_write_lock!(self.event_publisher).send(event).await
    }

    pub fn add_request_call(&self, req: LivenessRequest) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        acquire_write_lock!(self.calls).push(req);
    }

    pub fn take_calls(&self) -> Vec<LivenessRequest> {
        acquire_write_lock!(self.calls).drain(..).collect()
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

pub struct LivenessMock {
    receiver: reply_channel::Receiver<LivenessRequest, Result<LivenessResponse, LivenessError>>,
    mock_state: LivenessMockState,
}

impl LivenessMock {
    pub fn new(
        receiver: reply_channel::Receiver<LivenessRequest, Result<LivenessResponse, LivenessError>>,
        mock_state: LivenessMockState,
    ) -> Self
    {
        Self { receiver, mock_state }
    }

    pub fn get_mock_state(&self) -> LivenessMockState {
        self.mock_state.clone()
    }

    pub fn set_mock_state(&mut self, mock_state: LivenessMockState) {
        self.mock_state = mock_state;
    }

    pub async fn run(mut self) {
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>) {
        use LivenessRequest::*;
        let (req, reply_tx) = req.split();
        trace!(target: LOG_TARGET, "LivenessMock received request {:?}", req);
        self.mock_state.add_request_call(req.clone());
        // TODO: Make these responses configurable
        match req {
            SendPing(_) => {
                reply_tx.send(Ok(LivenessResponse::Ok)).unwrap();
            },
            GetPingCount => {
                reply_tx.send(Ok(LivenessResponse::Count(1))).unwrap();
            },
            GetPongCount => {
                reply_tx.send(Ok(LivenessResponse::Count(1))).unwrap();
            },
            GetAvgLatency(_) => {
                reply_tx.send(Ok(LivenessResponse::AvgLatency(None))).unwrap();
            },
            SetPongMetadata(_, _) => {
                reply_tx.send(Ok(LivenessResponse::Ok)).unwrap();
            },
            GetNumActiveNeighbours => {
                reply_tx.send(Ok(LivenessResponse::NumActiveNeighbours(8))).unwrap();
            },
        }
    }
}
