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

use crate::{
    envelope::{DhtMessageFlags, DhtMessageHeader},
    outbound::message_params::FinalSendMessageParams,
};
use futures::channel::oneshot;
use std::{fmt, fmt::Display};
use tari_comms::{
    message::{MessageFlags, MessageTag},
    peer_manager::Peer,
    types::CommsPublicKey,
};
use tari_crypto::tari_utilities::hex::Hex;

/// Determines if an outbound message should be Encrypted and, if so, for which public key
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundEncryption {
    /// Message should not be encrypted
    None,
    /// Message should be encrypted using a shared secret derived from the given public key
    EncryptFor(Box<CommsPublicKey>),
}

impl OutboundEncryption {
    /// Return the correct DHT flags for the encryption setting
    pub fn flags(&self) -> DhtMessageFlags {
        match self {
            OutboundEncryption::EncryptFor(_) => DhtMessageFlags::ENCRYPTED,
            _ => DhtMessageFlags::NONE,
        }
    }

    /// Returns true if encryption is turned on, otherwise false
    pub fn is_encrypt(&self) -> bool {
        use OutboundEncryption::*;
        match self {
            None => false,
            EncryptFor(_) => true,
        }
    }
}

impl Display for OutboundEncryption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            OutboundEncryption::None => write!(f, "None"),
            OutboundEncryption::EncryptFor(ref key) => write!(f, "EncryptFor:{}", key.to_hex()),
        }
    }
}

impl Default for OutboundEncryption {
    fn default() -> Self {
        OutboundEncryption::None
    }
}

#[derive(Debug)]
pub enum SendMessageResponse {
    /// Returns the message tags which are queued for sending. These tags will be used in a subsequent OutboundEvent to
    /// indicate if the message succeeded/failed to send
    Queued(Vec<MessageTag>),
    /// A failure occurred when sending
    Failed,
    /// DHT Discovery has been initiated. The caller may wait on the receiver
    /// to find out of the message was sent.
    /// _NOTE: DHT discovery could take minutes (determined by `DhtConfig::discovery_request_timeout)_
    PendingDiscovery(oneshot::Receiver<SendMessageResponse>),
}

impl SendMessageResponse {
    /// Returns the result of a send message request.
    /// A `SendMessageResponse::Queued(n)` will resolve immediately returning `Some(n)`.
    /// A `SendMessageResponse::Failed` will resolve immediately returning a `None`.
    /// If DHT discovery is initiated, this will resolve once discovery has completed, either
    /// succeeding (`Some(n)`) or failing (`None`).
    pub async fn resolve_ok(self) -> Option<Vec<MessageTag>> {
        use SendMessageResponse::*;
        match self {
            Queued(tags) => Some(tags),
            Failed => None,
            PendingDiscovery(rx) => rx.await.ok()?.queued_or_failed(),
        }
    }

    fn queued_or_failed(self) -> Option<Vec<MessageTag>> {
        use SendMessageResponse::*;
        match self {
            Queued(tags) => Some(tags),
            Failed => None,
            PendingDiscovery(_) => panic!("ok_or_failed() called on PendingDiscovery"),
        }
    }
}

/// Represents a request to the DHT broadcast middleware
#[derive(Debug)]
pub enum DhtOutboundRequest {
    /// Send a message using the given broadcast strategy
    SendMessage(
        Box<FinalSendMessageParams>,
        Vec<u8>,
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
#[derive(Clone, Debug)]
pub struct DhtOutboundMessage {
    pub tag: MessageTag,
    pub destination_peer: Peer,
    pub dht_header: DhtMessageHeader,
    pub comms_flags: MessageFlags,
    pub encryption: OutboundEncryption,
    pub body: Vec<u8>,
}

impl DhtOutboundMessage {
    /// Create a new DhtOutboundMessage
    pub fn new(
        destination_peer: Peer,
        dht_header: DhtMessageHeader,
        encryption: OutboundEncryption,
        comms_flags: MessageFlags,
        body: Vec<u8>,
    ) -> Self
    {
        Self {
            tag: MessageTag::new(),
            destination_peer,
            dht_header,
            encryption,
            comms_flags,
            body,
        }
    }
}

impl fmt::Display for DhtOutboundMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "\n---- DhtOutboundMessage ---- \nSize: {} byte(s)\nType: {}\nPeer: {}\nHeader: {} \nFlags: \
             {:?}\nEncryption: {}\n{}\n----",
            self.body.len(),
            self.dht_header.message_type,
            self.destination_peer,
            self.dht_header,
            self.dht_header.flags,
            self.encryption,
            self.tag
        )
    }
}
