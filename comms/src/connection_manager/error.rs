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

use crate::{
    peer_manager::PeerManagerError,
    protocol::{IdentityProtocolError, ProtocolError},
};
use derive_error::Error;
use futures::channel::mpsc;

#[derive(Debug, Error, Clone)]
pub enum ConnectionManagerError {
    PeerManagerError(PeerManagerError),
    /// Cannot connect to peers which are not persisted in the peer manager database.
    PeerNotPersisted,
    /// Failed to send request to ConnectionManagerActor. Channel closed.
    SendToActorFailed,
    /// Request was canceled before the response could be sent
    ActorRequestCanceled,
    /// The dial reply channel was closed when sending a reply
    DialReplyChannelClosed,
    /// Failed to connect on all addresses for peer
    DialConnectFailedAllAddresses,
    /// Failed to connect to peer within the maximum number of attempts
    ConnectFailedMaximumAttemptsReached,
    #[error(msg_embedded, no_from, non_std)]
    YamuxConnectionError(String),
    /// Establisher channel is closed or full
    /// Failed to perform yamux upgrade on socket
    #[error(msg_embedded, no_from, non_std)]
    YamuxUpgradeFailure(String),
    /// Establisher channel is closed or full
    EstablisherChannelError,
    #[error(msg_embedded, no_from, non_std)]
    TransportError(String),
    /// The peer authenticated to a public key which did not match the dialed peer's public key
    DialedPublicKeyMismatch,
    /// The noise transport failed to provide a valid static public key for the peer
    InvalidStaticPublicKey,
    // This is a String because we need this error to be clonable so that we can
    // send the same response to multiple requesters
    #[error(msg_embedded, no_from, non_std)]
    NoiseError(String),
    /// Incoming listener stream unexpectedly closed
    IncomingListenerStreamClosed,
    /// The peer offered a NodeId that failed to validate against it's public key
    PeerIdentityInvalidNodeId,
    /// Peer is banned, denying connection
    PeerBanned,
    /// Unable to parse any of the network addresses offered by the connecting peer
    PeerIdentityNoValidAddresses,
    IdentityProtocolError(IdentityProtocolError),
    /// The dial was cancelled
    DialCancelled,
    /// The peer is offline and will not be dialed
    PeerOffline,
    #[error(msg_embedded, no_from, non_std)]
    InvalidMultiaddr(String),
}

impl From<yamux::ConnectionError> for ConnectionManagerError {
    fn from(err: yamux::ConnectionError) -> Self {
        ConnectionManagerError::YamuxConnectionError(err.to_string())
    }
}

#[derive(Debug, Error)]
pub enum PeerConnectionError {
    YamuxConnectionError(yamux::ConnectionError),
    /// Internal oneshot reply channel was unexpectedly cancelled
    InternalReplyCancelled,
    /// Failed to send internal request
    InternalRequestSendFailed(mpsc::SendError),
    ProtocolError(ProtocolError),
}
