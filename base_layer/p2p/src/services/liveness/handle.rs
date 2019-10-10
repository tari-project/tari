// Copyright 2019 The Tari Project
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

use crate::services::liveness::error::LivenessError;
use futures::{stream::Fuse, StreamExt};
use serde::{Deserialize, Serialize};
use tari_broadcast_channel::Subscriber;
use tari_comms::types::CommsPublicKey;
use tari_service_framework::reply_channel::SenderService;
use tower::Service;

/// Request types made through the `LivenessHandle` and are handled by the `LivenessService`
#[derive(Debug)]
pub enum LivenessRequest {
    /// Send a ping to the given public key
    SendPing(CommsPublicKey),
    /// Retrieve the total number of pings received
    GetPingCount,
    /// Retrieve the total number of pongs received
    GetPongCount,
}

/// Response type for `LivenessService`
#[derive(Debug)]
pub enum LivenessResponse {
    PingSent,
    Count(usize),
}

/// The PingPong comms-level message
#[derive(Debug, Serialize, Deserialize)]
pub enum PingPong {
    Ping,
    Pong,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum LivenessEvent {
    ReceivedPing,
    ReceivedPong,
}

#[derive(Clone)]
pub struct LivenessHandle {
    handle: SenderService<LivenessRequest, Result<LivenessResponse, LivenessError>>,
    event_stream: Subscriber<LivenessEvent>,
}

impl LivenessHandle {
    pub fn new(
        handle: SenderService<LivenessRequest, Result<LivenessResponse, LivenessError>>,
        event_stream: Subscriber<LivenessEvent>,
    ) -> Self
    {
        Self { handle, event_stream }
    }

    pub fn get_event_stream_fused(&self) -> Fuse<Subscriber<LivenessEvent>> {
        self.event_stream.clone().fuse()
    }

    pub async fn send_ping(&mut self, pub_key: CommsPublicKey) -> Result<(), LivenessError> {
        match self.handle.call(LivenessRequest::SendPing(pub_key)).await?? {
            LivenessResponse::PingSent => Ok(()),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }

    pub async fn get_ping_count(&mut self) -> Result<usize, LivenessError> {
        match self.handle.call(LivenessRequest::GetPingCount).await?? {
            LivenessResponse::Count(c) => Ok(c),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }

    pub async fn get_pong_count(&mut self) -> Result<usize, LivenessError> {
        match self.handle.call(LivenessRequest::GetPongCount).await?? {
            LivenessResponse::Count(c) => Ok(c),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }
}
