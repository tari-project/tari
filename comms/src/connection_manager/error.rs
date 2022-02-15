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

use thiserror::Error;
use tokio::{sync::mpsc, time::error::Elapsed};

use crate::{
    connection_manager::PeerConnectionRequest,
    noise,
    peer_manager::PeerManagerError,
    protocol::{IdentityProtocolError, ProtocolError},
};

#[derive(Debug, Error, Clone)]
pub enum ConnectionManagerError {
    #[error("Peer manager error: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Peer connection error: {0}")]
    PeerConnectionError(String),
    #[error("Cannot connect to peers which are not persisted in the peer manager database.")]
    PeerNotPersisted,
    #[error("Failed to send request to ConnectionManagerActor. Channel closed.")]
    SendToActorFailed,
    #[error("Request was canceled before the response could be sent")]
    ActorRequestCanceled,
    #[error("The dial reply channel was closed when sending a reply")]
    DialReplyChannelClosed,
    #[error("Failed to connect on all addresses for peer")]
    DialConnectFailedAllAddresses,
    #[error("Failed to connect to peer within the maximum number of attempts")]
    ConnectFailedMaximumAttemptsReached,
    #[error("Yamux connection error: {0}")]
    YamuxConnectionError(String),
    #[error("Failed to perform yamux upgrade on socket: {0}")]
    YamuxUpgradeFailure(String),
    #[error("Establisher channel is closed or full")]
    EstablisherChannelError,
    #[error("Transport error: {0}")]
    TransportError(String),
    #[error("The peer authenticated to a public key which did not match the dialed peer's public key")]
    DialedPublicKeyMismatch,
    #[error("The noise transport failed to provide a valid static public key for the peer")]
    InvalidStaticPublicKey,
    // This is a String because we need this error to be clonable so that we can
    // send the same response to multiple requesters
    #[error("Noise error: {0}")]
    NoiseError(String),
    #[error("Incoming listener stream unexpectedly closed")]
    IncomingListenerStreamClosed,
    #[error("Peer is banned, denying connection")]
    PeerBanned,
    #[error("Unable to parse any of the network addresses offered by the connecting peer")]
    PeerIdentityNoValidAddresses,
    #[error("Identity protocol failed: {0}")]
    IdentityProtocolError(#[from] IdentityProtocolError),
    #[error("The dial was cancelled")]
    DialCancelled,
    #[error("Invalid multiaddr: {0}")]
    InvalidMultiaddr(String),
    #[error("Failed to send wire format byte")]
    WireFormatSendFailed,
    #[error("Noise protocol handshake timed out")]
    NoiseProtocolTimeout,
    #[error("Listener oneshot cancelled")]
    ListenerOneshotCancelled,
    #[error("Peer sent invalid identity signature")]
    PeerIdentityInvalidSignature,
    #[error("Peer did not provide the identity timestamp")]
    PeerIdentityNoUpdatedTimestampProvided,
    #[error("Peer did not provide any public addresses")]
    PeerIdentityNoAddresses,
}

impl From<yamux::ConnectionError> for ConnectionManagerError {
    fn from(err: yamux::ConnectionError) -> Self {
        ConnectionManagerError::YamuxConnectionError(err.to_string())
    }
}

impl From<noise::NoiseError> for ConnectionManagerError {
    fn from(err: noise::NoiseError) -> Self {
        ConnectionManagerError::NoiseError(err.to_string())
    }
}

impl From<PeerConnectionError> for ConnectionManagerError {
    fn from(err: PeerConnectionError) -> Self {
        ConnectionManagerError::PeerConnectionError(err.to_string())
    }
}

#[derive(Debug, Error)]
pub enum PeerConnectionError {
    #[error("Yamux connection error: {0}")]
    YamuxConnectionError(#[from] yamux::ConnectionError),
    #[error("Internal oneshot reply channel was unexpectedly cancelled")]
    InternalReplyCancelled,
    #[error("Failed to send internal request: {0}")]
    InternalRequestSendFailed(#[from] mpsc::error::SendError<PeerConnectionRequest>),
    #[error("Protocol error: {0}")]
    ProtocolError(#[from] ProtocolError),
    #[error("Protocol negotiation timeout")]
    ProtocolNegotiationTimeout,
}

impl From<Elapsed> for PeerConnectionError {
    fn from(_: Elapsed) -> Self {
        PeerConnectionError::ProtocolNegotiationTimeout
    }
}
