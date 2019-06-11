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

use tari_utilities::message_format::MessageFormat;

use crate::message::{Frame, MessageError};

#[derive(Serialize, Deserialize, Clone)]
pub struct MessageHeader<MType> {
    pub message_type: MType,
}

/// Represents a Message as described in [RFC-0172](https://rfc.tari.com/RFC-0172_PeerToPeerMessagingProtocol.html#messaging-structure).
/// This message has been decrypted but the contents are still serialized
/// as described in [RFC-0171](https://rfc.tari.com/RFC-0171_MessageSerialisation.html)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub header: Frame,
    pub body: Frame,
}

impl Message {
    /// Create a new Message from two MessagFormat types
    pub fn from_message_format<H: MessageFormat, B: MessageFormat>(header: H, msg: B) -> Result<Self, MessageError> {
        let header_frame = header.to_binary()?;
        let body_frame = msg.to_binary()?;
        Ok(Self {
            header: header_frame,
            body: body_frame,
        })
    }

    /// Deserialize and return the header of the message
    pub fn to_header<MType>(&self) -> Result<MessageHeader<MType>, MessageError>
    where
        MessageHeader<MType>: MessageFormat,
        MType: DeserializeOwned,
    {
        MessageHeader::<MType>::from_binary(&self.header).map_err(Into::into)
    }

    pub fn to_message<T>(&self) -> Result<T, MessageError>
    where T: MessageFormat {
        T::from_binary(&self.body).map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tari_utilities::hex::to_hex;

    #[derive(Serialize, Deserialize)]
    struct TestHeader {
        a: u32,
    }

    #[derive(Serialize, Deserialize)]
    struct TestMsg {
        a: u32,
    }

    #[test]
    fn from_message_format() {
        let header = TestHeader { a: 1 };
        let msg = TestMsg { a: 2 };

        let msg = Message::from_message_format(header, msg).unwrap();
        assert_eq!("9101", to_hex(&msg.header));
        assert_eq!("9102", to_hex(&msg.body));
    }
}
