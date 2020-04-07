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
use derive_error::Error;
use prost::DecodeError;
use std::io;
use tari_comms::{message::MessageError, peer_manager::PeerManagerError};
use tari_crypto::tari_utilities::{byte_array::ByteArrayError, ciphers::cipher::CipherError};

#[derive(Debug, Error)]
pub enum StoreAndForwardError {
    DhtMessageError(DhtMessageError),
    MessageError(MessageError),
    PeerManagerError(PeerManagerError),
    DhtOutboundError(DhtOutboundError),
    /// Received stored message has an invalid destination
    InvalidDestination,
    /// Received stored message has an invalid origin signature
    InvalidOriginMac,
    /// Invalid envelope body
    InvalidEnvelopeBody,
    /// DHT header is invalid
    InvalidDhtHeader,
    /// Received stored message which is not encrypted
    StoredMessageNotEncrypted,
    /// Unable to decrypt received stored message
    DecryptionFailed,
    CipherError(CipherError),
    DhtActorError(DhtActorError),
    /// Received duplicate stored message
    DuplicateMessage,
    CurrentThreadRuntimeInitializeFailed(io::Error),
    /// Unable to decode message
    DecodeError(DecodeError),
    /// Dht header was not provided
    DhtHeaderNotProvided,
    /// Failed to spawn blocking task
    JoinError(tokio::task::JoinError),
    /// Message origin is for all forwarded messages
    MessageOriginRequired,
    /// The message was malformed
    MalformedMessage,

    StorageError(StorageError),
    /// The store and forward service requester channel closed
    RequesterChannelClosed,
    /// The request was cancelled by the store and forward service
    RequestCancelled,
    /// The message was not valid for store and forward
    InvalidStoreMessage,
    /// The envelope version is invalid
    InvalidEnvelopeVersion,
    MalformedNodeId(ByteArrayError),
}
