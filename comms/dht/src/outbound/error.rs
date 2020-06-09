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

use crate::outbound::message::SendFailure;
use futures::channel::mpsc::SendError;
use tari_comms::message::MessageError;
use tari_crypto::{
    signatures::SchnorrSignatureError,
    tari_utilities::{ciphers::cipher::CipherError, message_format::MessageFormatError},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DhtOutboundError {
    #[error("SendError: {0}")]
    SendError(#[from] SendError),
    #[error("MessageSerializationError: {0}")]
    MessageSerializationError(#[from] MessageError),
    #[error("MessageFormatError: {0}")]
    MessageFormatError(#[from] MessageFormatError),
    #[error("SignatureError: {0}")]
    SignatureError(#[from] SchnorrSignatureError),
    #[error("CipherError: {0}")]
    CipherError(#[from] CipherError),
    #[error("Requester reply channel closed before response was received")]
    RequesterReplyChannelClosed,
    #[error("Peer selection failed")]
    PeerSelectionFailed,
    #[error("Failed to send broadcast message")]
    BroadcastFailed,
    #[error("Reply channel cancelled")]
    ReplyChannelCanceled,
    #[error("Attempted to send a message to ourselves")]
    SendToOurselves,
    #[error("Discovery process failed")]
    DiscoveryFailed,
    #[error("Failed to insert message hash")]
    FailedToInsertMessageHash,
    #[error("Failed to send message: {0}")]
    SendMessageFailed(SendFailure),
    #[error("No messages were queued for sending")]
    NoMessagesQueued,
}

impl From<SendFailure> for DhtOutboundError {
    fn from(err: SendFailure) -> Self {
        match err {
            SendFailure::NoMessagesQueued => DhtOutboundError::NoMessagesQueued,
            err => Self::SendMessageFailed(err),
        }
    }
}
