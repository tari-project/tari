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

use crate::{
    connection::{ConnectionError, NetAddressError},
    control_service::{messages::RejectReason, ControlServiceError},
    message::MessageError,
    peer_manager::PeerManagerError,
};
use derive_error::Error;
use tari_utilities::{
    ciphers::cipher::CipherError,
    message_format::MessageFormatError,
    thread_join::ThreadError,
    ByteArrayError,
};

#[derive(Error, Debug)]
pub enum ConnectionManagerError {
    /// There are no available peer connection ports
    NoAvailablePeerConnectionPort,
    /// The peer connection could not be found
    PeerConnectionNotFound,
    /// The peer could not be found
    PeerNotFound,
    // Error establishing connection
    ConnectionError(ConnectionError),
    #[error(no_from)]
    CurveEncryptionGenerateError(ConnectionError),
    MessageFormatError(MessageFormatError),
    MessageError(MessageError),
    /// The global node identity has not been set
    GlobalNodeIdentityNotSet,
    SharedSecretSerializationError(ByteArrayError),
    CipherError(CipherError),
    PeerManagerError(PeerManagerError),
    /// Failed to connect to control service on all addresses
    ControlServiceFailedConnectionAllAddresses,
    /// Problem creating or loading datastore
    DatastoreError,
    /// Connection timed out before it was able to connect
    TimeoutBeforeConnected,
    /// The maximum number of peer connections has been reached
    MaxConnectionsReached,
    /// Failed to shutdown a peer connection
    #[error(no_from)]
    ConnectionShutdownFailed(ConnectionError),
    PeerConnectionThreadError(ThreadError),
    #[error(msg_embedded, non_std, no_from)]
    ControlServicePingPongFailed(String),
    #[error(msg_embedded, non_std, no_from)]
    SendRequestConnectionFailed(String),
    /// Failed to receive a connection request outcome message
    ConnectionRequestOutcomeRecvFail,
    /// The request to establish a peer connection was rejected by the destination peer's control port
    ConnectionRejected(RejectReason),
    /// Failed to receive a connection request outcome before the timeout
    ConnectionRequestOutcomeTimeout,
    ControlServiceError(ControlServiceError),
    //---------------------------------- Async --------------------------------------------//
    /// Failed to send request to ConnectionManagerActor. Channel closed.
    SendToActorFailed,
    /// Request was canceled before the response could be sent
    ActorRequestCanceled,
    /// Curve public key was invalid
    InvalidCurvePublicKey,
    NetAddressError(NetAddressError),
}
