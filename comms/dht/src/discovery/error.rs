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

use tari_comms::peer_manager::PeerManagerError;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::{
    outbound::{message::SendFailure, DhtOutboundError},
    peer_validator::DhtPeerValidatorError,
};

#[derive(Debug, Error)]
pub enum DhtDiscoveryError {
    #[error("The reply channel was canceled")]
    ReplyCanceled,
    #[error("DhtOutboundError: {0}")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Received an invalid `NodeId`")]
    InvalidNodeId,
    #[error("MPSC channel is disconnected")]
    ChannelDisconnected,
    #[error("The discovery request timed out")]
    DiscoveryTimeout,
    #[error("Failed to send discovery message: {0}")]
    DiscoverySendFailed(SendFailure),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("InvalidPeerMultiaddr: {0}")]
    InvalidPeerMultiaddr(String),
    #[error("No signature provided")]
    NoSignatureProvided,
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Invalid discovery response: {details}")]
    InvalidDiscoveryResponse { details: anyhow::Error },
    #[error("DHT peer validator error: {0}")]
    PeerValidatorError(#[from] DhtPeerValidatorError),
}

impl DhtDiscoveryError {
    /// Returns true if this error is a `DiscoveryTimeout`, otherwise false
    pub fn is_timeout(&self) -> bool {
        matches!(self, DhtDiscoveryError::DiscoveryTimeout)
    }
}

impl<T> From<SendError<T>> for DhtDiscoveryError {
    fn from(_: SendError<T>) -> Self {
        DhtDiscoveryError::ChannelDisconnected
    }
}
