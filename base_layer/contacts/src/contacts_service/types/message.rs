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

use std::convert::TryFrom;

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};
use tari_common_types::tari_address::TariAddress;
use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_p2p::tari_message::TariMessageType;
use tari_utilities::ByteArray;

use crate::contacts_service::proto;

#[derive(Clone, Debug, Default)]
pub struct Message {
    pub body: Vec<u8>,
    pub metadata: Vec<MessageMetadata>,
    pub address: TariAddress,
    pub direction: Direction,
    pub stored_at: u64,
    pub delivery_confirmation_at: Option<u64>,
    pub read_confirmation_at: Option<u64>,
    pub message_id: Vec<u8>,
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MessageMetadata {
    pub metadata_type: MessageMetadataType,
    pub data: Vec<u8>,
}

#[repr(u8)]
#[derive(FromPrimitive, Debug, Copy, Clone, Default, Deserialize, Serialize, PartialEq)]
pub enum MessageMetadataType {
    Reply = 0,
    #[default]
    TokenRequest = 1,
}

impl MessageMetadataType {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }
}

impl TryFrom<proto::Message> for Message {
    type Error = String;

    fn try_from(message: proto::Message) -> Result<Self, Self::Error> {
        let mut metadata = vec![];
        for m in message.metadata {
            match MessageMetadata::try_from(m) {
                Ok(md) => metadata.push(md),
                Err(e) => return Err(e),
            }
        }

        Ok(Self {
            body: message.body,
            metadata,
            address: TariAddress::from_bytes(&message.address).map_err(|e| e.to_string())?,
            // A Message from a proto::Message will always be an inbound message
            direction: Direction::Inbound,
            stored_at: message.stored_at,
            delivery_confirmation_at: message.delivery_confirmation_at,
            read_confirmation_at: message.read_confirmation_at,
            message_id: message.message_id,
        })
    }
}

impl From<Message> for proto::Message {
    fn from(message: Message) -> Self {
        Self {
            body: message.body,
            metadata: message
                .metadata
                .iter()
                .map(|m| proto::MessageMetadata::from(m.clone()))
                .collect(),
            address: message.address.to_bytes().to_vec(),
            direction: i32::from(message.direction.as_byte()),
            stored_at: message.stored_at,
            delivery_confirmation_at: message.delivery_confirmation_at,
            read_confirmation_at: message.read_confirmation_at,
            message_id: message.message_id,
        }
    }
}

impl From<Message> for OutboundDomainMessage<proto::Message> {
    fn from(message: Message) -> Self {
        Self::new(&TariMessageType::Chat, message.into())
    }
}

impl TryFrom<proto::MessageMetadata> for MessageMetadata {
    type Error = String;

    fn try_from(md: proto::MessageMetadata) -> Result<Self, Self::Error> {
        if let Some(md_type) =
            MessageMetadataType::from_byte(u8::try_from(md.metadata_type).map_err(|e| e.to_string())?)
        {
            Ok(Self {
                data: md.data,
                metadata_type: md_type,
            })
        } else {
            Err("Not a valid metadata type".into())
        }
    }
}

impl From<MessageMetadata> for proto::MessageMetadata {
    fn from(md: MessageMetadata) -> Self {
        Self {
            data: md.data,
            metadata_type: i32::from(md.metadata_type.as_byte()),
        }
    }
}
