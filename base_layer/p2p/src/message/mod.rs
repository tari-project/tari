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

mod tag;
pub use tag::*;
use tari_network::{identity::PeerId, MessageSpec};

use crate::proto;

#[derive(Debug, Clone)]
pub enum TariNodeMessage {
    // Common
    PingPong(proto::liveness::PingPongMessage),
    // Base node
    NewTransaction(proto::types::Transaction),
    NewBlock(proto::core::NewBlock),
    BaseNodeRequest(proto::base_node::BaseNodeServiceRequest),
    BaseNodeResponse(proto::base_node::BaseNodeServiceResponse),
    // Wallet
    SenderPartialTransaction(proto::transaction::TransactionSenderMessage),
    ReceiverPartialTransactionReply(proto::transaction::RecipientSignedMessage),
    TransactionFinalized(proto::transaction::TransactionFinalizedMessage),
    TransactionCancelled(proto::transaction::TransactionCancelledMessage),
    // Chat
    Chat(proto::chat::MessageDispatch),
}

impl TariNodeMessage {
    pub fn into_ping_pong(self) -> Option<proto::liveness::PingPongMessage> {
        match self {
            TariNodeMessage::PingPong(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_new_transaction(self) -> Option<proto::types::Transaction> {
        match self {
            TariNodeMessage::NewTransaction(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_new_block(self) -> Option<proto::core::NewBlock> {
        match self {
            TariNodeMessage::NewBlock(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_base_node_request(self) -> Option<proto::base_node::BaseNodeServiceRequest> {
        match self {
            TariNodeMessage::BaseNodeRequest(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_base_node_response(self) -> Option<proto::base_node::BaseNodeServiceResponse> {
        match self {
            TariNodeMessage::BaseNodeResponse(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_sender_partial_transaction(self) -> Option<proto::transaction::TransactionSenderMessage> {
        match self {
            TariNodeMessage::SenderPartialTransaction(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_receiver_partial_transaction_reply(self) -> Option<proto::transaction::RecipientSignedMessage> {
        match self {
            TariNodeMessage::ReceiverPartialTransactionReply(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_transaction_finalized(self) -> Option<proto::transaction::TransactionFinalizedMessage> {
        match self {
            TariNodeMessage::TransactionFinalized(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_transaction_cancelled(self) -> Option<proto::transaction::TransactionCancelledMessage> {
        match self {
            TariNodeMessage::TransactionCancelled(p) => Some(p),
            _ => None,
        }
    }

    pub fn into_chat(self) -> Option<proto::chat::MessageDispatch> {
        match self {
            TariNodeMessage::Chat(p) => Some(p),
            _ => None,
        }
    }
}

macro_rules! impl_from {
    ($variant:tt, $ty:ty) => {
        impl From<$ty> for TariNodeMessage {
            fn from(value: $ty) -> Self {
                TariNodeMessage::$variant(value)
            }
        }
    };
}

impl_from!(PingPong, proto::liveness::PingPongMessage);
impl_from!(NewTransaction, proto::types::Transaction);
impl_from!(NewBlock, proto::core::NewBlock);
impl_from!(BaseNodeRequest, proto::base_node::BaseNodeServiceRequest);
impl_from!(BaseNodeResponse, proto::base_node::BaseNodeServiceResponse);
impl_from!(SenderPartialTransaction, proto::transaction::TransactionSenderMessage);
impl_from!(
    ReceiverPartialTransactionReply,
    proto::transaction::RecipientSignedMessage
);
impl_from!(TransactionFinalized, proto::transaction::TransactionFinalizedMessage);
impl_from!(TransactionCancelled, proto::transaction::TransactionCancelledMessage);
impl_from!(Chat, proto::chat::MessageDispatch);

pub struct TariNodeMessageSpec;
impl MessageSpec for TariNodeMessageSpec {
    type Message = TariNodeMessage;
}

/// Wrapper around a received message. Provides source peer and origin information
#[derive(Debug, Clone)]
pub struct DomainMessage<T> {
    pub source_peer_id: PeerId,
    /// This DHT header of this message. If `DhtMessageHeader::origin_public_key` is different from the
    /// `source_peer.public_key`, this message was forwarded.
    pub header: DomainMessageHeader,
    /// The domain-level message
    pub payload: T,
}

impl<T> DomainMessage<T> {
    pub fn inner(&self) -> &T {
        &self.payload
    }

    pub fn into_inner(self) -> T {
        self.payload
    }

    /// Consumes this object returning the PeerId of the original sender of this message and the message itself
    pub fn into_origin_and_inner(self) -> (PeerId, T) {
        (self.source_peer_id, self.payload)
    }

    pub fn peer_id(&self) -> PeerId {
        self.source_peer_id
    }
}

// TODO: this is here to avoid having to change a lot of code that references `header.message_tag`
#[derive(Debug, Clone)]
pub struct DomainMessageHeader {
    pub message_tag: MessageTag,
}

// impl From<proto::liveness::PingPongMessage> for TariNodeMessage {
//     fn from(value: proto::liveness::PingPongMessage) -> Self {
//         TariNodeMessage::PingPong(value)
//     }
// }
