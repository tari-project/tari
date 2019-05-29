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

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{connection::Frame, message::MessageError};
use std::convert::TryFrom;
use tari_utilities::message_format::MessageFormat;

#[derive(Serialize, Deserialize, Clone)]
pub struct MessageHeader<MType> {
    pub message_type: MType,
}

/// Represents a Message as described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure).
/// This message has been decrypted but the contents are still serialized
/// as described in [RFC-0171](https://rfc.tari.com/RFC-0171_MessageSerialisation.html)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub header: Frame,
    pub body: Frame,
}

impl Message {
    pub fn to_header<MType>(&self) -> Result<MessageHeader<MType>, MessageError>
    where
        MessageHeader<MType>: MessageFormat,
        MType: DeserializeOwned,
    {
        MessageHeader::<MType>::from_binary(&self.header).map_err(Into::into)
    }

    pub fn to_message<T>(&self) -> Result<T, MessageError>
    where T: MessageFormat {
        // TryFrom<Frame, Error=MessageError> {
        T::from_binary(&self.body).map_err(Into::into)
    }
}

impl<H: MessageFormat, B: MessageFormat> TryFrom<(H, B)> for Message {
    type Error = MessageError;

    /// Create a new Message from two message format types
    fn try_from((header, body): (H, B)) -> Result<Self, Self::Error> {
        let header_frame = header.to_binary()?;
        let body_frame = body.to_binary()?;
        Ok(Self {
            header: header_frame,
            body: body_frame,
        })
    }
}
