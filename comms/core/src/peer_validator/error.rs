// Copyright 2023, The Tari Project
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

use std::time::Duration;

use crate::{bans::BAN_DURATION_LONG, peer_manager::NodeId};

/// Validation errors for peers shared on the network
#[derive(Debug, Clone, thiserror::Error)]
pub enum PeerValidatorError {
    #[error("Peer signature was invalid for peer '{peer}'")]
    InvalidPeerSignature { peer: NodeId },
    #[error("One or more peer addresses were invalid for '{peer}'")]
    InvalidPeerAddresses { peer: NodeId },
    #[error("Peer '{peer}' was banned")]
    PeerHasNoAddresses { peer: NodeId },
    #[error("Invalid multiaddr: {0}")]
    InvalidMultiaddr(String),
    #[error("No public addresses provided")]
    PeerIdentityNoAddresses,
    #[error("Onion v2 is deprecated and not supported")]
    OnionV2NotSupported,
    #[error("Peer provided too many supported protocols: expected max {max} but got {length}")]
    PeerIdentityTooManyProtocols { length: usize, max: usize },
    #[error("Peer provided too many addresses: expected max {max} but got {length}")]
    PeerIdentityTooManyAddresses { length: usize, max: usize },
    #[error("Peer provided a protocol id that exceeds the maximum length: expected max {max} but got {length}")]
    PeerIdentityProtocolIdTooLong { length: usize, max: usize },
    #[error("Peer provided a user agent that exceeds the maximum length: expected max {max} but got {length}")]
    PeerIdentityUserAgentTooLong { length: usize, max: usize },
}

impl PeerValidatorError {
    pub fn as_ban_duration(&self) -> Option<Duration> {
        Some(BAN_DURATION_LONG)
    }
}
