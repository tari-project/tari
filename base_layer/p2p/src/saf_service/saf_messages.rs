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

use crate::tari_message::{NetMessage, TariMessageType};
use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use tari_comms::message::{Message, MessageEnvelope, MessageError};

/// The RetrieveMsgsMessage is used for requesting the set of stored messages from neighbouring peer nodes. If a
/// start_time is provided then only messages after the specified time will be sent, otherwise all applicable messages
/// will be sent.
#[derive(Serialize, Deserialize)]
pub struct RetrieveMsgsMessage {
    start_time: Option<DateTime<Utc>>,
}

impl TryInto<Message> for RetrieveMsgsMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(NetMessage::RetrieveMessages), self).try_into()?)
    }
}

/// The StoredMsgsMessage contains the set of applicable messages retrieved from a neighbouring peer node.
#[derive(Serialize, Deserialize)]
pub struct StoredMsgsMessage {
    pub messages: Vec<MessageEnvelope>,
}

impl TryInto<Message> for StoredMsgsMessage {
    type Error = MessageError;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok((TariMessageType::new(NetMessage::StoredMessages), self).try_into()?)
    }
}
