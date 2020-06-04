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

use crate::{actor::DhtActorError, envelope::DhtMessageError, outbound::DhtOutboundError, storage::StorageError};
use prost::DecodeError;
use tari_comms::{message::MessageError, peer_manager::PeerManagerError};
use tari_utilities::{byte_array::ByteArrayError, ciphers::cipher::CipherError};
use thiserror::Error;

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
    #[error("Received stored message has an invalid destination")]
    InvalidDestination,
    #[error("Received stored message has an invalid origin signature")]
    InvalidOriginMac,
    #[error("Invalid envelope body")]
    InvalidEnvelopeBody,
    #[error("DHT header is invalid")]
    InvalidDhtHeader,
    #[error("Received stored message which is not encrypted")]
    StoredMessageNotEncrypted,
    #[error("Unable to decrypt received stored message")]
    DecryptionFailed,
    #[error("CipherError: {0}")]
    CipherError(#[from] CipherError),
    #[error("DhtActorError: {0}")]
    DhtActorError(#[from] DhtActorError),
    #[error("Received duplicate stored message")]
    DuplicateMessage,
    #[error("Unable to decode message: {0}")]
    DecodeError(#[from] DecodeError),
    #[error("Dht header was not provided")]
    DhtHeaderNotProvided,
    #[error("Message origin is for all forwarded messages")]
    MessageOriginRequired,
    #[error("The message was malformed")]
    MalformedMessage,

    #[error("StorageError: {0}")]
    StorageError(#[from] StorageError),
    #[error("The store and forward service requester channel closed")]
    RequesterChannelClosed,
    #[error("The request was cancelled by the store and forward service")]
    RequestCancelled,
    #[error("The message was not valid for store and forward")]
    InvalidStoreMessage,
    #[error("The envelope version is invalid")]
    InvalidEnvelopeVersion,
    #[error("MalformedNodeId: {0}")]
    MalformedNodeId(#[from] ByteArrayError),
    #[error("NodeDistance threshold was invalid")]
    InvalidNodeDistanceThreshold,
    #[error("DHT message type should not have been forwarded")]
    InvalidDhtMessageType,
    #[error("Failed to send request for store and forward messages: {0}")]
    RequestMessagesFailed(DhtOutboundError),
}
