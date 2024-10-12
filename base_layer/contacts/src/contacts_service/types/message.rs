// Copyright 2023. The Tari Project
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

use std::{convert::TryFrom, fmt::Display};

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use tari_common_types::tari_address::TariAddress;
use tari_max_size::MaxSizeBytes;
use tari_p2p::proto::chat as proto;
use tari_utilities::ByteArray;

pub(crate) const MAX_MESSAGE_ID_SIZE: usize = 36;
pub type MessageId = MaxSizeBytes<MAX_MESSAGE_ID_SIZE>;
pub(crate) const MAX_BODY_SIZE: usize = 2 * 1024 * 1024;
pub type ChatBody = MaxSizeBytes<MAX_BODY_SIZE>;
pub(crate) const MAX_MESSAGE_SIZE: usize = MAX_BODY_SIZE + 512 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Message {
    pub body: ChatBody,
    pub metadata: Vec<MessageMetadata>,
    pub receiver_address: TariAddress,
    pub sender_address: TariAddress,
    pub direction: Direction,
    pub sent_at: u64,
    pub stored_at: u64,
    pub delivery_confirmation_at: Option<u64>,
    pub read_confirmation_at: Option<u64>,
    pub message_id: MessageId,
}

impl Message {
    pub fn push(&mut self, metadata: MessageMetadata) {
        self.metadata.push(metadata)
    }
}

#[repr(u8)]
#[derive(FromPrimitive, Debug, Copy, Clone, Default, PartialEq)]
pub enum Direction {
    Inbound = 0,
    #[default]
    Outbound = 1,
}

impl Direction {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }
}

pub(crate) const MAX_KEY_SIZE: usize = 256;
pub type MetadataKey = MaxSizeBytes<MAX_KEY_SIZE>;
pub(crate) const MAX_DATA_SIZE: usize = 2 * 1024 * 1024;
pub type MetadataData = MaxSizeBytes<MAX_DATA_SIZE>;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MessageMetadata {
    #[serde(with = "tari_utilities::serde::hex")]
    pub key: MetadataKey,
    #[serde(with = "tari_utilities::serde::hex")]
    pub data: MetadataData,
}

impl Message {
    pub fn data_byte_size(&self) -> usize {
        self.body.len() +
            self.metadata.iter().map(|m| m.data.len()).sum::<usize>() +
            self.metadata.iter().map(|m| m.key.len()).sum::<usize>()
    }
}

impl TryFrom<proto::Message> for Message {
    type Error = String;

    fn try_from(message: proto::Message) -> Result<Self, Self::Error> {
        let mut metadata = vec![];
        for m in message.metadata {
            metadata.push(MessageMetadata::try_from(m)?);
        }

        Ok(Self {
            body: ChatBody::try_from(message.body).map_err(|e| format!("body: ({})", e))?,
            metadata,
            receiver_address: TariAddress::from_bytes(&message.receiver_address)
                .map_err(|e| format!("receiver_address: ({})", e))?,
            sender_address: TariAddress::from_bytes(&message.sender_address)
                .map_err(|e| format!("sender_address: ({})", e))?,
            // A Message from a proto::Message will always be an inbound message
            direction: Direction::Inbound,
            message_id: MessageId::try_from(message.message_id).map_err(|e| format!("message_id: ({})", e))?,
            ..Message::default()
        })
    }
}

impl From<Message> for proto::Message {
    fn from(message: Message) -> Self {
        Self {
            body: message.body.to_vec(),
            metadata: message
                .metadata
                .iter()
                .map(|m| proto::MessageMetadata::from(m.clone()))
                .collect(),
            receiver_address: message.receiver_address.to_vec(),
            sender_address: message.sender_address.to_vec(),
            direction: i32::from(message.direction.as_byte()),
            message_id: message.message_id.to_vec(),
        }
    }
}

impl TryFrom<proto::MessageMetadata> for MessageMetadata {
    type Error = String;

    fn try_from(md: proto::MessageMetadata) -> Result<Self, Self::Error> {
        Ok(Self {
            data: MetadataData::try_from(md.data).map_err(|e| format!("metadata data: ({})", e))?,
            key: MetadataKey::try_from(md.key).map_err(|e| format!("metadata key: ({})", e))?,
        })
    }
}

impl From<MessageMetadata> for proto::MessageMetadata {
    fn from(md: MessageMetadata) -> Self {
        Self {
            data: md.data.to_vec(),
            key: md.key.to_vec(),
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Message {{ message_id: {}, receiver_address: {}, sender_address: {}, direction: {:?}, body: {}, \
             metadata: {:?}, sent_at: {}, stored_at: {}, delivery_confirmation_at: {:?}, read_confirmation_at: {:?} }}",
            self.message_id,
            self.receiver_address,
            self.sender_address,
            self.direction,
            self.body,
            format!(
                "{:?}",
                self.metadata
                    .iter()
                    .map(|m| format!("({}, {})", m.key, m.data))
                    .collect::<Vec<String>>()
            ),
            self.sent_at,
            self.stored_at,
            self.delivery_confirmation_at,
            self.read_confirmation_at,
        )
    }
}
