// Copyright 2020, The Tari Project
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

use crate::{
    connection_manager::PeerConnectionError,
    message::{MessageError, OutboundMessage},
    peer_manager::PeerManagerError,
    protocol::ProtocolError,
};
use derive_error::Error;

#[derive(Debug, Error)]
pub enum InboundMessagingError {
    PeerManagerError(PeerManagerError),
    /// Inbound message signatures are invalid
    InvalidMessageSignature,
    /// The received envelope is invalid
    InvalidEnvelope,
    /// The connected peer sent a public key which did not match the public key of the connected peer
    PeerPublicKeyMismatch,
    /// Failed to decode message
    MessageDecodeError(prost::DecodeError),
    MessageError(MessageError),
}
#[derive(Debug, Error)]
pub enum MessagingProtocolError {
    /// Failed to send message
    #[error(no_from, non_std)]
    MessageSendFailed(OutboundMessage), // Msg returned to sender
    ProtocolError(ProtocolError),
    PeerConnectionError(PeerConnectionError),
    /// Failed to dial peer
    PeerDialFailed,
    /// Failure when sending on an outbound substream
    OutboundSubstreamFailure,
    MessageError(MessageError),
}
