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

use super::{error::LivenessError, state::Metadata};
use crate::proto::liveness::MetadataKey;
use futures::{stream::Fuse, StreamExt};
use tari_broadcast_channel::Subscriber;
use tari_comms::peer_manager::NodeId;
use tari_service_framework::reply_channel::SenderService;
use tower::Service;

/// Request types made through the `LivenessHandle` and are handled by the `LivenessService`
#[derive(Debug)]
pub enum LivenessRequest {
    /// Send a ping to the given node ID
    SendPing(NodeId),
    /// Retrieve the total number of pings received
    GetPingCount,
    /// Retrieve the total number of pongs received
    GetPongCount,
    /// Get average latency for node ID
    GetAvgLatency(NodeId),
    /// Set the metadata attached to each pong message
    SetPongMetadata(MetadataKey, Vec<u8>),
}

/// Response type for `LivenessService`
#[derive(Debug)]
pub enum LivenessResponse {
    /// Indicates that the request succeeded
    Ok,
    /// Used to return a counter value from `GetPingCount` and `GetPongCount`
    Count(usize),
    /// Response for GetAvgLatency
    AvgLatency(Option<u32>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LivenessEvent {
    /// A ping was received
    ReceivedPing,
    /// A pong was received. The latency to the peer (if available) and the metadata contained
    /// within the received pong message are included as part of the event
    ReceivedPong(Box<PongEvent>),
}

/// Repressents a pong event
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PongEvent {
    /// The node id of the node which sent this pong
    pub node_id: NodeId,
    /// Latency if available (i.e. a corresponding ping was sent within the Liveness state inflight ping TTL)
    pub latency: Option<u32>,
    /// Pong metadata
    pub metadata: Metadata,
}

impl PongEvent {
    pub(super) fn new(node_id: NodeId, latency: Option<u32>, metadata: Metadata) -> Self {
        Self {
            node_id,
            latency,
            metadata,
        }
    }
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

    /// Returns a fused event stream for the liveness service
    pub fn get_event_stream_fused(&self) -> Fuse<Subscriber<LivenessEvent>> {
        self.event_stream.clone().fuse()
    }

    /// Send a ping to a given node ID
    pub async fn send_ping(&mut self, node_id: NodeId) -> Result<(), LivenessError> {
        match self.handle.call(LivenessRequest::SendPing(node_id)).await?? {
            LivenessResponse::Ok => Ok(()),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }

    /// Retrieve the global ping count
    pub async fn get_ping_count(&mut self) -> Result<usize, LivenessError> {
        match self.handle.call(LivenessRequest::GetPingCount).await?? {
            LivenessResponse::Count(c) => Ok(c),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }

    /// Retrieve the global pong count
    pub async fn get_pong_count(&mut self) -> Result<usize, LivenessError> {
        match self.handle.call(LivenessRequest::GetPongCount).await?? {
            LivenessResponse::Count(c) => Ok(c),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }

    /// Set metadata entry for the pong message
    pub async fn set_pong_metadata_entry(&mut self, key: MetadataKey, value: Vec<u8>) -> Result<(), LivenessError> {
        match self.handle.call(LivenessRequest::SetPongMetadata(key, value)).await?? {
            LivenessResponse::Ok => Ok(()),
            _ => Err(LivenessError::UnexpectedApiResponse),
        }
    }
}
