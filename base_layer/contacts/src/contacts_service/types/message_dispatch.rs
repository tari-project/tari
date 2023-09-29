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

use tari_comms_dht::domain_message::OutboundDomainMessage;
use tari_p2p::tari_message::TariMessageType;

use crate::contacts_service::{
    proto,
    types::{Confirmation, Message},
};

pub enum MessageDispatch {
    Message(Message),
    DeliveryConfirmation(Confirmation),
    ReadConfirmation(Confirmation),
}

impl TryFrom<proto::MessageDispatch> for MessageDispatch {
    type Error = String;

    fn try_from(dispatch: proto::MessageDispatch) -> Result<Self, String> {
        Ok(match dispatch.contents {
            Some(proto::message_dispatch::Contents::Message(m)) => MessageDispatch::Message(Message::try_from(m)?),
            Some(proto::message_dispatch::Contents::DeliveryConfirmation(c)) => {
                MessageDispatch::DeliveryConfirmation(Confirmation::from(c))
            },
            Some(proto::message_dispatch::Contents::ReadConfirmation(c)) => {
                MessageDispatch::ReadConfirmation(Confirmation::from(c))
            },
            None => return Err("We didn't get any known type of chat message".to_string()),
        })
    }
}

impl From<MessageDispatch> for proto::MessageDispatch {
    fn from(dispatch: MessageDispatch) -> Self {
        let content = match dispatch {
            MessageDispatch::Message(m) => proto::message_dispatch::Contents::Message(m.into()),
            MessageDispatch::DeliveryConfirmation(c) => {
                proto::message_dispatch::Contents::DeliveryConfirmation(c.into())
            },
            MessageDispatch::ReadConfirmation(c) => proto::message_dispatch::Contents::ReadConfirmation(c.into()),
        };

        Self {
            contents: Some(content),
        }
    }
}

impl From<MessageDispatch> for OutboundDomainMessage<proto::MessageDispatch> {
    fn from(dispatch: MessageDispatch) -> Self {
        Self::new(&TariMessageType::Chat, dispatch.into())
    }
}
