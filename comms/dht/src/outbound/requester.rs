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

use super::{broadcast_strategy::BroadcastStrategy, message::DhtOutboundRequest};
use crate::{
    envelope::{DhtHeader, DhtMessageFlags, DhtMessageType, NodeDestination},
    outbound::{
        message::{ForwardRequest, OutboundEncryption, SendMessageRequest},
        DhtOutboundError,
    },
};
use futures::{channel::mpsc, SinkExt};
use tari_comms::message::{Frame, Message, MessageFlags, MessageHeader};
use tari_utilities::message_format::MessageFormat;

#[derive(Clone)]
pub struct OutboundMessageRequester {
    sender: mpsc::Sender<DhtOutboundRequest>,
}

impl OutboundMessageRequester {
    pub fn new(sender: mpsc::Sender<DhtOutboundRequest>) -> Self {
        Self { sender }
    }

    /// Send a unencrypted message
    pub async fn send_message<T, MType>(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message_type: MType,
        message: T,
    ) -> Result<(), DhtOutboundError>
    where
        MessageHeader<MType>: MessageFormat,
        T: MessageFormat,
    {
        let flags = encryption.flags();
        let body = serialize_message(message_type, message)?;
        self.send(
            broadcast_strategy,
            destination,
            encryption,
            flags,
            DhtMessageType::None,
            body,
        )
        .await
    }

    /// Send a DHT-level message
    pub async fn send_dht_message<T>(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        message_type: DhtMessageType,
        message: T,
    ) -> Result<(), DhtOutboundError>
    where
        T: MessageFormat,
    {
        let flags = encryption.flags();
        // DHT has the message type in the DhtHeader, so no need to duplicate it in the Message wrapper.
        let body = serialize_message((), message)?;
        self.send(broadcast_strategy, destination, encryption, flags, message_type, body)
            .await
    }

    /// Send a raw message
    pub async fn send(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        destination: NodeDestination,
        encryption: OutboundEncryption,
        dht_flags: DhtMessageFlags,
        dht_message_type: DhtMessageType,
        body: Frame,
    ) -> Result<(), DhtOutboundError>
    {
        self.sender
            .send(DhtOutboundRequest::SendMsg(Box::new(SendMessageRequest {
                broadcast_strategy,
                destination,
                encryption,
                // Since NONE is the only option here, hard code to empty() rather than make this part of the public
                // interface. If comms-level message flags become useful, it should be easy to add that to the public
                // API from here up to domain-level
                comms_flags: MessageFlags::empty(),
                dht_flags,
                dht_message_type,
                body,
            })))
            .await
            .map_err(Into::into)
    }

    /// Forward a message
    pub async fn forward_message(
        &mut self,
        broadcast_strategy: BroadcastStrategy,
        dht_header: DhtHeader,
        body: Vec<u8>,
    ) -> Result<(), DhtOutboundError>
    {
        self.sender
            .send(DhtOutboundRequest::Forward(Box::new(ForwardRequest {
                broadcast_strategy,
                comms_flags: MessageFlags::FORWARDED,
                dht_header,
                body,
            })))
            .await
            .map_err(Into::into)
    }
}

fn serialize_message<T, MType>(message_type: MType, message: T) -> Result<Vec<u8>, DhtOutboundError>
where
    T: MessageFormat,
    MessageHeader<MType>: MessageFormat,
{
    let header = MessageHeader::new(message_type)?;
    let msg = Message::from_message_format(header, message)?;

    msg.to_binary().map_err(Into::into)
}
