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
    connection::{ConnectionError, PeerConnectionError},
    connection_manager::ConnectionManagerError,
    dispatcher::DispatchError,
    message::MessageError,
    peer_manager::{node_id::NodeIdError, peer_manager::PeerManagerError},
};
use derive_error::Error;
use tari_utilities::{ciphers::cipher::CipherError, message_format::MessageFormatError};

#[derive(Debug, Error)]
pub enum ControlServiceError {
    #[error(no_from)]
    BindFailed(ConnectionError),
    MessageError(MessageError),
    DispatchError(DispatchError),
    MessageFormatError(MessageFormatError),
    /// Failed to send control message to worker
    ControlMessageSendFailed,
    /// Failed to join on worker thread
    WorkerThreadJoinFailed,
    NodeIdError(NodeIdError),
    PeerManagerError(PeerManagerError),
    PeerConnectionError(PeerConnectionError),
    ConnectionError(ConnectionError),
    /// Node identity has not been set
    NodeIdentityNotSet,
    ConnectionManagerError(ConnectionManagerError),
    /// The worker thread failed to start
    WorkerThreadFailedToStart,
    /// Failed to serialize shared secret
    SharedSecretSerializationError,
    /// Received an unencrypted message. Discarding it.
    ReceivedUnencryptedMessage,
    CipherError(CipherError),
    /// Peer is banned, refusing connection request
    PeerBanned,
}
