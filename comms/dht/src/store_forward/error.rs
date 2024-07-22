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

use std::time::Duration;

use prost::DecodeError;
use tari_comms::{
    connectivity::ConnectivityError,
    message::MessageError,
    peer_manager::{NodeId, PeerManagerError},
};
use thiserror::Error;

use crate::{
    actor::DhtActorError,
    envelope::DhtMessageError,
    error::DhtEncryptError,
    message_signature::MessageSignatureError,
    outbound::DhtOutboundError,
    storage::StorageError,
};

/// Error type for SAF
#[derive(Debug, Error)]
pub enum StoreAndForwardError {
    #[error("DhtMessageError: {0}")]
    DhtMessageError(#[from] DhtMessageError),
    #[error("MessageError: {0}")]
    MessageError(#[from] MessageError),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("DhtOutboundError: {0}")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("DhtEncryptError: {0}")]
    DhtEncryptError(#[from] DhtEncryptError),
    #[error("Received stored message has an invalid destination")]
    InvalidDestination,
    #[error("Received stored message has an invalid origin signature: {0}")]
    InvalidMessageSignature(#[from] MessageSignatureError),
    #[error("Envelope body is missing a required message part")]
    EnvelopeBodyMissingMessagePart,
    #[error("DHT header did not pass semantic validation rules")]
    BadDhtHeaderSemanticallyInvalid,
    #[error("Unable to decrypt received stored message")]
    DecryptionFailed,
    #[error("DhtActorError: {0}")]
    DhtActorError(#[from] DhtActorError),
    #[error("Received duplicate stored message")]
    DuplicateMessage,
    #[error("Unable to decode message: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("The message envelope was malformed: {0}")]
    MalformedEnvelopeBody(DecodeError),
    #[error("StorageError: {0}")]
    StorageError(#[from] StorageError),
    #[error("The store and forward service requester channel closed")]
    RequesterChannelClosed,
    #[error("The request was cancelled by the store and forward service")]
    RequestCancelled,
    #[error("The {field} field was not valid, discarding SAF response: {details}")]
    InvalidSafResponseMessage { field: &'static str, details: String },
    #[error("DHT message type should not have been stored/forwarded")]
    PeerSentDhtMessageViaSaf,
    #[error("SAF message type should not have been stored/forwarded")]
    PeerSentSafMessageViaSaf,
    #[error("Received SAF messages that were not requested")]
    ReceivedUnrequestedSafMessages,
    #[error("SAF messages received from peer {peer} after deadline. Received after {message_age:.2?}")]
    SafMessagesReceivedAfterDeadline { peer: NodeId, message_age: Duration },
    #[error("Invalid SAF request: `stored_at` cannot be in the future")]
    StoredAtWasInFuture,
    #[error("Invariant error (POSSIBLE BUG): {0}")]
    InvariantError(String),
    #[error("ConnectivityError: {0}")]
    ConnectivityError(#[from] ConnectivityError),
}
