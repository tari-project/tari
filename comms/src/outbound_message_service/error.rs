// Copyright 2019 The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use crate::{
    connection::{dealer_proxy::DealerProxyError, ConnectionError, NetAddressError, PeerConnectionError},
    connection_manager::ConnectionManagerError,
    message::MessageError,
    peer_manager::PeerManagerError,
};
use derive_error::Error;
use tari_crypto::signatures::SchnorrSignatureError;
use tari_utilities::{ciphers::cipher::CipherError, message_format::MessageFormatError, ByteArrayError};

/// Error type for OutboundMessageService subsystem
#[derive(Debug, Error)]
pub enum OutboundError {
    /// Could not connect to the outbound message pool
    SocketConnectionError(zmq::Error),
    /// Problem communicating to the outbound message pool
    ConnectionError(ConnectionError),
    /// The secret key was not defined in the node identity
    UndefinedSecretKey,
    /// The message signature could not be serialized to a vector of bytes
    SignatureSerializationError,
    /// The generated shared secret could not be serialized to a vector of bytes
    SharedSecretSerializationError(ByteArrayError),
    /// The message could not be serialized
    MessageSerializationError(MessageError),
    /// Could not successfully sign the message
    SignatureError(SchnorrSignatureError),
    /// Error during serialization or deserialization
    MessageFormatError(MessageFormatError),
    /// Problem encountered with Broadcast Strategy and PeerManager
    PeerManagerError(PeerManagerError),
    /// The Thread Safety has been breached and the data access has become poisoned
    PoisonedAccess,
    /// Error requesting or updating a net address
    NetAddressError(NetAddressError),
    /// Error using a Cipher
    CipherError(CipherError),
    /// Error using the ConnectionManager
    ConnectionManagerError(ConnectionManagerError),
    /// Error using a PeerConnection
    PeerConnectionError(PeerConnectionError),
    /// Number of retry attempts exceeded
    RetryAttemptsExceedError,
    #[error(msg_embedded, non_std, no_from)]
    ControlSendError(String),
    DealerProxyError(DealerProxyError),
    /// Could not join the dealer or worker threads
    ThreadJoinError,
}
