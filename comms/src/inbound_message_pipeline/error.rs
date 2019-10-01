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

use crate::{message::MessageError, outbound_message_service::OutboundServiceError, peer_manager::PeerManagerError};
use derive_error::Error;
use futures::channel::mpsc;

#[derive(Debug, Error)]
pub enum InboundMessagePipelineError {
    /// The incoming message already existed in the message cache and was thus discarded
    DuplicateMessageDiscarded,
    /// Could not deserialize incoming message data
    DeserializationError,
    /// Inbound message signatures are invalid
    InvalidMessageSignature,
    /// Invalid Destination that cannot be routed
    InvalidDestination,
    /// Message destined for this node cannot be decrypted
    DecryptionFailure,
    /// The source peer did not exist in the peer manager
    CannotFindSourcePeer,
    /// Error when sending inbound message
    SendError(mpsc::SendError),
    MessageError(MessageError),
    PeerManagerError(PeerManagerError),
    OutboundServiceError(OutboundServiceError),
}
