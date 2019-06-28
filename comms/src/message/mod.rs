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

//! # Message
//!
//! The message module contains the message types which wrap domain-level messages.
//!
//! Described further in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure)
//!
//! - [Frame] and [FrameSet]
//!
//! A [FrameSet] consists of multiple [Frame]s. A [Frame] is the raw byte representation of a message.
//!
//! - [MessageEnvelope]
//!
//! Represents data that is about to go on the wire or has just come off.
//!
//! - [MessageEnvelopeHeader]
//!
//! The header that every message contains.
//!
//! - [Message]
//!
//! This message is deserialized from the body [Frame] of the [MessageEnvelope].
//! It consists of a [MessageHeader] and a domain-level body [Frame].
//! This part of the [MessageEnvelope] can optionally be encrypted for a particular peer.
//!
//! - [MessageHeader]
//!
//! Information about the contained message. Currently, this only contains the
//! domain-level message type.
//!
//! - [MessageData]
//!
//! This message is dispatched by the [InboundMessageBroker] to a [DomainConnector].
//!
//! [Frame]: ./tyoe.Frame.html
//! [FrameSet]: ./tyoe.FrameSet.html
//! [MessageEnvelope]: ./envelope/struct.MessageEnvelope.html
//! [MessageEnvelopeHeader]: ./envelope/struct.MessageEnvelopeHeader.html
//! [Message]: ./message/struct.Message.html
//! [MessageHeader]: ./message/struct.MessageHeader.html
//! [MessageData]: ./message/struct.MessageData.html
//! [InboundMessageBroker]: ../inbound_message_service/inbound_message_broker/struct.InboundMessageBroker.html
//! [DomainConnector]: ../domain_connector/struct.DomainConnector.html
use crate::peer_manager::node_id::NodeId;
use bitflags::*;
use serde::{Deserialize, Serialize};

mod domain_message_context;
mod envelope;
mod error;
mod message;
mod message_context;
mod message_data;
pub mod p2p;

pub use self::{
    domain_message_context::*,
    envelope::{MessageEnvelope, MessageEnvelopeHeader},
    error::MessageError,
    message::{Message, MessageHeader},
    message_context::MessageContext,
    message_data::*,
};

/// Represents a single message frame.
pub type Frame = Vec<u8>;
/// Represents a collection of frames which make up a multipart message.
pub type FrameSet = Vec<Frame>;

bitflags! {
    /// Used to indicate characteristics of the incoming or outgoing message, such
    /// as whether the message is encrypted.
    #[derive(Deserialize, Serialize)]
    pub struct MessageFlags: u8 {
        const NONE = 0b00000000;
        const ENCRYPTED = 0b00000001;
    }
}

/// Represents the ways a destination node can be represented.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum NodeDestination<P> {
    Unknown,
    PublicKey(P),
    NodeId(NodeId),
}
