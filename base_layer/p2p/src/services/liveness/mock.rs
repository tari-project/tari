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

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
    RwLock,
};

use futures::StreamExt;
use log::*;
use tari_crypto::tari_utilities::{acquire_read_lock, acquire_write_lock};
use tari_service_framework::{reply_channel, reply_channel::RequestContext};
use tokio::sync::{broadcast, broadcast::error::SendError};

use crate::services::liveness::{
    error::LivenessError,
    handle::LivenessEventSender,
    LivenessEvent,
    LivenessHandle,
    LivenessRequest,
    LivenessResponse,
};

const LOG_TARGET: &str = "p2p::liveness_mock";

pub fn create_p2p_liveness_mock(buf_size: usize) -> (LivenessHandle, LivenessMock, LivenessEventSender) {
    let (sender, receiver) = reply_channel::unbounded();
    let (publisher, _) = broadcast::channel(buf_size);
    (
        LivenessHandle::new(sender, publisher.clone()),
        LivenessMock::new(receiver, LivenessMockState::new(publisher.clone())),
        publisher,
    )
}

#[derive(Debug, Clone)]
pub struct LivenessMockState {
    call_count: Arc<AtomicUsize>,
    event_publisher: Arc<RwLock<LivenessEventSender>>,
    calls: Arc<RwLock<Vec<LivenessRequest>>>,
}

impl LivenessMockState {
    pub fn new(event_publisher: LivenessEventSender) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            event_publisher: Arc::new(RwLock::new(event_publisher)),
            calls: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn publish_event(&self, event: LivenessEvent) -> Result<(), SendError<Arc<LivenessEvent>>> {
        let lock = acquire_read_lock!(self.event_publisher);
        lock.send(Arc::new(event))?;
        Ok(())
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
    receiver: reply_channel::TryReceiver<LivenessRequest, LivenessResponse, LivenessError>,
    mock_state: LivenessMockState,
}

impl LivenessMock {
    pub fn new(
        receiver: reply_channel::TryReceiver<LivenessRequest, LivenessResponse, LivenessError>,
        mock_state: LivenessMockState,
    ) -> Self {
        Self { receiver, mock_state }
    }

    pub fn get_mock_state(&self) -> LivenessMockState {
        self.mock_state.clone()
    }

    pub fn set_mock_state(&mut self, mock_state: LivenessMockState) {
        self.mock_state = mock_state;
    }

    pub async fn run(mut self) {
        debug!(target: LOG_TARGET, "LivenessMock mockin");
        while let Some(req) = self.receiver.next().await {
            self.handle_request(req).await;
        }
    }

    async fn handle_request(&self, req: RequestContext<LivenessRequest, Result<LivenessResponse, LivenessError>>) {
        use LivenessRequest::*;
        let (req, reply) = req.split();
        trace!(target: LOG_TARGET, "LivenessMock received request {:?}", req);
        self.mock_state.add_request_call(req.clone());
        // TODO: Make these responses configurable
        match req {
            SendPing(_) => {
                reply.send(Ok(LivenessResponse::Ok)).unwrap();
            },
            GetPingCount => {
                reply.send(Ok(LivenessResponse::Count(1))).unwrap();
            },
            GetPongCount => {
                reply.send(Ok(LivenessResponse::Count(1))).unwrap();
            },
            GetAvgLatency(_) => {
                reply.send(Ok(LivenessResponse::AvgLatency(None))).unwrap();
            },
            GetNetworkAvgLatency => {
                reply.send(Ok(LivenessResponse::AvgLatency(None))).unwrap();
            },
            SetMetadataEntry(_, _) => {
                reply.send(Ok(LivenessResponse::Ok)).unwrap();
            },
            AddMonitoredPeer(_) => {
                reply.send(Ok(LivenessResponse::Ok)).unwrap();
            },
            RemoveMonitoredPeer(_) => {
                reply.send(Ok(LivenessResponse::Ok)).unwrap();
            },
        }
    }
}
