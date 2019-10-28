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
    message::MessageError,
    peer_manager::PeerManagerError,
};
use derive_error::Error;
use tari_utilities::{ciphers::cipher::CipherError, thread_join::ThreadError};

#[derive(Debug, Error)]
pub enum ControlServiceError {
    #[error(no_from)]
    BindFailed(ConnectionError),
    MessageError(MessageError),
    /// Failed to send control message to worker
    ControlMessageSendFailed,
    // Failed to join on worker thread
    WorkerThreadJoinFailed(ThreadError),
    PeerManagerError(PeerManagerError),
    ConnectionError(ConnectionError),
    /// The worker thread failed to start
    WorkerThreadFailedToStart,
    /// Received an unencrypted message. Discarding it.
    ReceivedUnencryptedMessage,
    CipherError(CipherError),
    /// Peer is banned, refusing connection request
    PeerBanned,
    /// Received message with an invalid signature
    InvalidMessageSignature,
    /// Received an unexpected reply
    ClientUnexpectedReply,
    NetAddressError(NetAddressError),
    /// The connection address could not be established
    ConnectionAddressNotEstablished,
    #[error(non_std, no_from, msg_embedded)]
    ConnectionProtocolFailed(String),
    /// Failed to decode message
    DecodeError(prost::DecodeError),
    /// Received an invalid or unrecognised message type
    InvalidMessageType,
    /// Received an invalid envelope body,
    InvalidEnvelopeBody,
    /// Received an invalid envelope
    InvalidEnvelope,
    /// Node ID is invalid
    InvalidNodeId,
    /// Peer sent invalid or unrecognised peer features
    InvalidPeerFeatures,
    /// Received an unrecognised message type
    UnrecognisedMessageType,
}
