// Copyright 2023. The Taiji Project
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

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use taiji_common_types::taiji_address::TaijiAddress;
use taiji_comms_dht::domain_message::OutboundDomainMessage;
use taiji_p2p::taiji_message::TaijiMessageType;
use tari_utilities::ByteArray;

use crate::contacts_service::proto;

#[derive(Clone, Debug, Default)]
pub struct Message {
    pub body: Vec<u8>,
    pub address: TaijiAddress,
    pub direction: Direction,
    pub stored_at: u64,
    pub message_id: Vec<u8>,
}

#[repr(u8)]
#[derive(FromPrimitive, Debug, Copy, Clone)]
pub enum Direction {
    Inbound = 0,
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

impl Default for Direction {
    fn default() -> Self {
        Self::Outbound
    }
}

impl From<proto::Message> for Message {
    fn from(message: proto::Message) -> Self {
        Self {
            body: message.body,
            address: TaijiAddress::from_bytes(&message.address).expect("Couldn't parse address"),
            // A Message from a proto::Message will always be an inbound message
            direction: Direction::Inbound,
            stored_at: message.stored_at,
            message_id: message.message_id,
        }
    }
}

impl From<Message> for proto::Message {
    fn from(message: Message) -> Self {
        Self {
            body: message.body,
            address: message.address.to_bytes().to_vec(),
            direction: i32::from(message.direction.as_byte()),
            stored_at: message.stored_at,
            message_id: message.message_id,
        }
    }
}

impl From<Message> for OutboundDomainMessage<proto::Message> {
    fn from(message: Message) -> Self {
        Self::new(&TaijiMessageType::Chat, message.into())
    }
}
