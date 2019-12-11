//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::connection::ConnectionError;
use derive_error::Error;
use tari_utilities::thread_join::ThreadError;

/// Represents errors which can occur in a PeerConnection.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PeerConnectionError {
    #[error(msg_embedded, non_std, no_from)]
    InitializationError(String),
    #[error(msg_embedded, non_std, no_from)]
    ControlSendError(String),
    /// Peer connection control channel has disconnected
    ControlChannelDisconnected,
    #[error(msg_embedded, non_std, no_from)]
    StateError(String),
    /// Error occurred while shutting down the connection
    ShutdownError,
    /// Failed to establish a connection
    ConnectFailed,
    #[error(msg_embedded, non_std, no_from)]
    UnexpectedConnectionError(String),
    /// Connection attempts exceeded max retries
    ExceededMaxConnectRetryCount,
    /// Peer connection worker thread failed to start
    ThreadInitializationError,
    /// Local identity was not set on outbound connection
    ConnectionIdentityNotSet,
    /// Remote identity was not set on outbound connection
    PeerIdentityNotSet,
    ConnectionError(ConnectionError),
    #[error(no_from)]
    SendFailure(ConnectionError),
    #[error(msg_embedded, no_from, non_std)]
    OperationFailed(String),
    #[error(msg_embedded, no_from, non_std)]
    InvalidOperation(String),
    /// Peer connection channel backlog did not clear after many attempts
    ChannelBacklogError,
    /// Peer connection sink channel disconnected
    ChannelDisconnectedError,
    ThreadJoinError(ThreadError),
    #[error(msg_embedded, no_from, non_std)]
    OperationTimeout(String),
    /// Oneshot sender reply cancelled
    OneshotReplyCancelled,
    #[error(msg_embedded, no_from, non_std)]
    ConnectionTestFailed(String),
    /// Reply channel failed for peer connection control request
    ControlMessageReplyFailed,
    /// Connection to peer was denied
    ConnectionDenied,
    /// Access to this peer connection is denied
    AccessDenied,
}
