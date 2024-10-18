// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::fmt;

use rand::{rngs::OsRng, RngCore};
use tari_network::{identity::PeerId, MessageSpec};

pub use crate::proto::message::{tari_message, TariMessageType};
use crate::{proto, proto::message::TariMessage};

impl TariMessage {
    pub fn into_ping_pong(self) -> Option<proto::liveness::PingPongMessage> {
        match self.message {
            Some(tari_message::Message::PingPong(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_base_node_request(self) -> Option<proto::base_node::BaseNodeServiceRequest> {
        match self.message {
            Some(tari_message::Message::BaseNodeRequest(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_base_node_response(self) -> Option<proto::base_node::BaseNodeServiceResponse> {
        match self.message {
            Some(tari_message::Message::BaseNodeResponse(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_sender_partial_transaction(self) -> Option<proto::transaction_protocol::TransactionSenderMessage> {
        match self.message {
            Some(tari_message::Message::SenderPartialTransaction(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_receiver_partial_transaction_reply(
        self,
    ) -> Option<proto::transaction_protocol::RecipientSignedMessage> {
        match self.message {
            Some(tari_message::Message::ReceiverPartialTransactionReply(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_transaction_finalized(self) -> Option<proto::transaction_protocol::TransactionFinalizedMessage> {
        match self.message {
            Some(tari_message::Message::TransactionFinalized(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_transaction_cancelled(self) -> Option<proto::transaction_protocol::TransactionCancelledMessage> {
        match self.message {
            Some(tari_message::Message::TransactionCancelled(p)) => Some(p),
            _ => None,
        }
    }

    pub fn into_chat(self) -> Option<proto::chat::MessageDispatch> {
        match self.message {
            Some(tari_message::Message::Chat(p)) => Some(p),
            _ => None,
        }
    }
}

impl tari_message::Message {
    pub fn as_type(&self) -> TariMessageType {
        match self {
            Self::PingPong(_) => TariMessageType::PingPong,
            Self::BaseNodeRequest(_) => TariMessageType::BaseNodeRequest,
            Self::BaseNodeResponse(_) => TariMessageType::BaseNodeResponse,
            Self::SenderPartialTransaction(_) => TariMessageType::SenderPartialTransaction,
            Self::ReceiverPartialTransactionReply(_) => TariMessageType::ReceiverPartialTransactionReply,
            Self::TransactionFinalized(_) => TariMessageType::TransactionFinalized,
            Self::TransactionCancelled(_) => TariMessageType::TransactionCancelled,
            Self::Chat(_) => TariMessageType::Chat,
        }
    }
}

macro_rules! impl_from {
    ($variant:tt, $ty:ty) => {
        impl From<$ty> for tari_message::Message {
            fn from(value: $ty) -> Self {
                tari_message::Message::$variant(value)
            }
        }
    };
}

impl_from!(PingPong, proto::liveness::PingPongMessage);
impl_from!(BaseNodeRequest, proto::base_node::BaseNodeServiceRequest);
impl_from!(BaseNodeResponse, proto::base_node::BaseNodeServiceResponse);
impl_from!(
    SenderPartialTransaction,
    proto::transaction_protocol::TransactionSenderMessage
);
impl_from!(
    ReceiverPartialTransactionReply,
    proto::transaction_protocol::RecipientSignedMessage
);
impl_from!(
    TransactionFinalized,
    proto::transaction_protocol::TransactionFinalizedMessage
);
impl_from!(
    TransactionCancelled,
    proto::transaction_protocol::TransactionCancelledMessage
);
impl_from!(Chat, proto::chat::MessageDispatch);

impl<T: Into<tari_message::Message>> From<T> for TariMessage {
    fn from(value: T) -> Self {
        TariMessage {
            message: Some(value.into()),
        }
    }
}

pub struct TariNodeMessageSpec;
impl MessageSpec for TariNodeMessageSpec {
    type Message = TariMessage;
}

/// Wrapper around a received message. Provides source peer and origin information
#[derive(Debug, Clone)]
pub struct DomainMessage<T> {
    pub source_peer_id: PeerId,
    pub header: DomainMessageHeader,
    /// The domain-level message
    pub payload: T,
}

impl<T> DomainMessage<T> {
    pub fn inner(&self) -> &T {
        &self.payload
    }

    pub fn into_payload(self) -> T {
        self.payload
    }

    /// Consumes this object returning the PeerId of the original sender of this message and the message itself
    pub fn into_origin_and_inner(self) -> (PeerId, T) {
        (self.source_peer_id, self.payload)
    }

    pub fn peer_id(&self) -> PeerId {
        self.source_peer_id
    }

    pub fn map<F: FnMut(T) -> U, U>(self, mut f: F) -> DomainMessage<U> {
        DomainMessage {
            source_peer_id: self.source_peer_id,
            header: self.header,
            payload: f(self.payload),
        }
    }
}

// TODO: this is here to avoid having to change a lot of code that references `header.message_tag`
#[derive(Debug, Clone)]
pub struct DomainMessageHeader {
    pub message_tag: MessageTag,
}

/// Represents a tag for a message
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Default)]
pub struct MessageTag(u64);

impl MessageTag {
    pub fn new() -> Self {
        Self(OsRng.next_u64())
    }

    pub fn as_value(self) -> u64 {
        self.0
    }
}

impl From<u64> for MessageTag {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl fmt::Display for MessageTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "Tag#{}", self.0)
    }
}

/// Trait that exposes conversion to a protobuf i32 enum type.
pub trait ToProtoEnum {
    fn as_i32(&self) -> i32;
}

impl ToProtoEnum for i32 {
    fn as_i32(&self) -> i32 {
        *self
    }
}

impl ToProtoEnum for TariMessageType {
    fn as_i32(&self) -> i32 {
        *self as i32
    }
}
