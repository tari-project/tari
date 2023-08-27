// Copyright 2019, The Taiji Project
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

use std::{fmt, fmt::Display, sync::Arc};

use taiji_comms::{
    message::{MessageTag, MessagingReplyTx},
    peer_manager::NodeId,
    types::CommsPublicKey,
    Bytes,
    BytesMut,
};
use tari_utilities::hex::Hex;
use thiserror::Error;
use tokio::sync::oneshot;

use crate::{
    envelope::{DhtMessageFlags, DhtMessageHeader, DhtMessageType, NodeDestination},
    outbound::{message_params::FinalSendMessageParams, message_send_state::MessageSendStates},
    version::DhtProtocolVersion,
};

/// Determines if an outbound message should be Encrypted and, if so, for which public key
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum OutboundEncryption {
    /// Message should not be encrypted
    #[default]
    ClearText,
    /// Message should be encrypted using a shared secret derived from the given public key
    EncryptFor(Box<CommsPublicKey>),
}

impl OutboundEncryption {
    pub fn encrypt_for(public_key: CommsPublicKey) -> Self {
        OutboundEncryption::EncryptFor(Box::new(public_key))
    }

    /// Return the correct DHT flags for the encryption setting
    pub fn flags(&self) -> DhtMessageFlags {
        match self {
            OutboundEncryption::EncryptFor(_) => DhtMessageFlags::ENCRYPTED,
            _ => DhtMessageFlags::NONE,
        }
    }

    /// Returns true if encryption is turned on, otherwise false
    pub fn is_encrypt(&self) -> bool {
        use OutboundEncryption::{ClearText, EncryptFor};
        match self {
            ClearText => false,
            EncryptFor(_) => true,
        }
    }
}

impl Display for OutboundEncryption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            OutboundEncryption::ClearText => write!(f, "ClearText"),
            OutboundEncryption::EncryptFor(ref key) => write!(f, "EncryptFor:{}", key.to_hex()),
        }
    }
}

#[derive(Debug, Error)]
pub enum SendFailure {
    #[error("Attempt to send message to ourselves")]
    SendToOurselves,
    #[error("Discovery reply channel cancelled")]
    SenderCancelled,
    #[error("Failure when sending: {0}")]
    General(String),
    #[error("Discovery failed")]
    DiscoveryFailed,
    #[error("Failure when generating messages: {0}")]
    FailedToGenerateMessages(String),
    #[error("No messages were queued for sending")]
    NoMessagesQueued,
}

#[derive(Debug)]
pub enum SendMessageResponse {
    /// Returns the message tags which are queued for sending. These tags will be used in a subsequent OutboundEvent to
    /// indicate if the message succeeded/failed to send
    Queued(MessageSendStates),
    /// A failure occurred when sending
    Failed(SendFailure),
    /// DHT Discovery has been initiated. The caller may wait on the receiver
    /// to find out of the message was sent.
    /// _NOTE: DHT discovery could take minutes (determined by `DhtConfig::discovery_request_timeout)_
    PendingDiscovery(oneshot::Receiver<SendMessageResponse>),
}

impl SendMessageResponse {
    /// Returns the result of a send message request.
    /// A `SendMessageResponse::Queued(n)` will resolve immediately returning `Ok(n)` or
    /// `Err(SendFailure::NoMessagesSent)`. A `SendMessageResponse::Failed` will resolve immediately returning a
    /// `Err(SendFailure)`. If DHT discovery is initiated, this will resolve once discovery has completed, either
    /// succeeding or failing.
    pub async fn resolve(self) -> Result<MessageSendStates, SendFailure> {
        use SendMessageResponse::{Failed, PendingDiscovery, Queued};
        match self {
            Queued(send_states) if !send_states.is_empty() => Ok(send_states),
            Queued(_) => Err(SendFailure::NoMessagesQueued),
            Failed(err) => Err(err),
            PendingDiscovery(rx) => rx.await.map_err(|_| SendFailure::SenderCancelled)?.queued_or_failed(),
        }
    }

    fn queued_or_failed(self) -> Result<MessageSendStates, SendFailure> {
        use SendMessageResponse::{Failed, PendingDiscovery, Queued};
        match self {
            Queued(send_states) if !send_states.is_empty() => Ok(send_states),
            Queued(_) => Err(SendFailure::NoMessagesQueued),
            Failed(err) => Err(err),
            PendingDiscovery(_) => panic!("queued_or_failed() called on PendingDiscovery"),
        }
    }
}

/// Represents a request to the DHT broadcast middleware
#[derive(Debug)]
pub enum DhtOutboundRequest {
    /// Send a message using the given broadcast strategy
    SendMessage(
        Box<FinalSendMessageParams>,
        BytesMut,
        oneshot::Sender<SendMessageResponse>,
    ),
}

impl fmt::Display for DhtOutboundRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            DhtOutboundRequest::SendMessage(params, body, _) => {
                write!(f, "SendMsg({} - <{} bytes>)", params.broadcast_strategy, body.len())
            },
        }
    }
}

/// DhtOutboundMessage consists of the DHT and comms information required to
/// send a message
#[derive(Debug)]
pub struct DhtOutboundMessage {
    pub protocol_version: DhtProtocolVersion,
    pub tag: MessageTag,
    pub destination_node_id: NodeId,
    pub custom_header: Option<DhtMessageHeader>,
    pub body: Bytes,
    pub ephemeral_public_key: Option<Arc<CommsPublicKey>>,
    pub message_signature: Option<Bytes>,
    pub destination: NodeDestination,
    pub dht_message_type: DhtMessageType,
    pub reply: MessagingReplyTx,
    pub dht_flags: DhtMessageFlags,
    pub is_broadcast: bool,
    pub expires: Option<prost_types::Timestamp>,
}

impl fmt::Display for DhtOutboundMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let header_str = self
            .custom_header
            .as_ref()
            .map(|h| format!("{} (Propagated)", h))
            .unwrap_or_else(|| {
                format!(
                    "Flags: {:?}, Destination: {}, Trace: {}",
                    self.dht_flags, self.destination, self.tag,
                )
            });
        write!(
            f,
            "\n---- Outgoing message ---- \nSize: {} byte(s)\nType: {}\nPeer: {}\nHeader: {}\n{}\n----\n{:?}\n",
            self.body.len(),
            self.dht_message_type,
            self.destination,
            header_str,
            self.tag,
            self.body
        )
    }
}
