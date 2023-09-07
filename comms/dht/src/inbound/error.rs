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

use tari_comms::{connectivity::ConnectivityError, message::MessageError, peer_manager::PeerManagerError};
use thiserror::Error;

use crate::{discovery::DhtDiscoveryError, outbound::DhtOutboundError, peer_validator::DhtPeerValidatorError};

#[derive(Debug, Error)]
pub enum DhtInboundError {
    #[error("MessageError: {0}")]
    MessageError(#[from] MessageError),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("DhtOutboundError: {0}")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Message body invalid")]
    InvalidMessageBody,
    #[error("DhtDiscoveryError: {0}")]
    DhtDiscoveryError(#[from] DhtDiscoveryError),
    #[error("OriginRequired: {0}")]
    OriginRequired(String),
    #[error("Peer validation failed: {0}")]
    PeerValidatorError(#[from] DhtPeerValidatorError),
    #[error("Invalid discovery message {0}")]
    InvalidDiscoveryMessage(#[from] anyhow::Error),
    #[error("ConnectivityError: {0}")]
    ConnectivityError(#[from] ConnectivityError),
}
