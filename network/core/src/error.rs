//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::io;

use libp2p::{gossipsub, gossipsub::SubscriptionError, swarm::DialError, Multiaddr, TransportError};
use tari_rpc_framework::RpcError;
use tari_swarm::{messaging, substream, TariSwarmError};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Codec IO error: {0}")]
    CodecError(io::Error),
    #[error("Gossipsub publish error: {0}")]
    GossipPublishError(#[from] gossipsub::PublishError),
    #[error("Failed to send message to peer: {0}")]
    SwarmError(#[from] TariSwarmError),
    #[error("Service has shutdown")]
    NetworkingHandleError(#[from] NetworkingHandleError),
    #[error("Failed to subscribe to topic: {0}")]
    SubscriptionError(#[from] SubscriptionError),
    #[error("Dial failed: {0}")]
    DialError(#[from] DialError),
    #[error("Failed to dial peer: {0}")]
    OutgoingConnectionError(String),
    #[error("Messaging error: {0}")]
    MessagingError(#[from] messaging::Error),
    #[error("Failed to open substream: {0}")]
    FailedToOpenSubstream(#[from] substream::Error),
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
    #[error("Transport error: {0}")]
    TransportError(#[from] TransportError<io::Error>),
    #[error("Peer sync error: {0}")]
    PeerSyncError(#[from] tari_swarm::peersync::Error),
    #[error("Messaging is disabled")]
    MessagingDisabled,
    #[error("Failed to add peer: {details}")]
    FailedToAddPeer { details: String },
}

impl From<oneshot::error::RecvError> for NetworkError {
    fn from(e: oneshot::error::RecvError) -> Self {
        Self::NetworkingHandleError(e.into())
    }
}

impl<T> From<mpsc::error::SendError<T>> for NetworkError {
    fn from(e: mpsc::error::SendError<T>) -> Self {
        Self::NetworkingHandleError(e.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkingHandleError {
    #[error("Service has shutdown")]
    ServiceHasShutdown,
    #[error("Service dropped reply sender without sending a reply")]
    ServiceAbandonedRequest,
}

impl From<oneshot::error::RecvError> for NetworkingHandleError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self::ServiceAbandonedRequest
    }
}

impl<T> From<mpsc::error::SendError<T>> for NetworkingHandleError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        Self::ServiceHasShutdown
    }
}
