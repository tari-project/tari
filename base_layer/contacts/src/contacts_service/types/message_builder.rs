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

use tari_common_types::tari_address::TariAddress;
use uuid::Uuid;

use crate::contacts_service::{
    error::ContactsServiceError,
    types::{
        message::{MessageMetadata, MAX_MESSAGE_SIZE},
        ChatBody,
        Message,
        MessageId,
    },
};

#[derive(Clone, Debug, Default)]
pub struct MessageBuilder {
    inner: Message,
}

impl MessageBuilder {
    pub fn new() -> Self {
        // We're forcing it to a String before bytes so we can have the same representation used in
        // all places, otherwise the UUID byte format will differ if displayed somewhere.
        let message_id = Uuid::new_v4().to_string().into_bytes();

        Self {
            inner: Message {
                message_id: MessageId::from_bytes_truncate(message_id),
                ..Message::default()
            },
        }
    }

    pub fn receiver_address(&self, receiver_address: TariAddress) -> Self {
        Self {
            inner: Message {
                receiver_address,
                ..self.inner.clone()
            },
        }
    }

    pub fn sender_address(&self, sender_address: TariAddress) -> Self {
        Self {
            inner: Message {
                sender_address,
                ..self.inner.clone()
            },
        }
    }

    pub fn message(&self, body: String) -> Result<Self, ContactsServiceError> {
        let message = Message {
            body: ChatBody::try_from(body.into_bytes())?,
            ..self.inner.clone()
        };
        self.finalize(message)
    }

    pub fn metadata(&self, new_metadata: MessageMetadata) -> Result<Self, ContactsServiceError> {
        let mut metadata = self.inner.metadata.clone();
        metadata.push(new_metadata);
        let message = Message {
            metadata,
            ..self.inner.clone()
        };
        self.finalize(message)
    }

    fn finalize(&self, message: Message) -> Result<MessageBuilder, ContactsServiceError> {
        if message.data_byte_size() > MAX_MESSAGE_SIZE {
            return Err(ContactsServiceError::MessageSizeExceeded(format!(
                "current: {}, limit: {}",
                message.data_byte_size(),
                MAX_MESSAGE_SIZE
            )));
        }
        Ok(Self { inner: message })
    }

    pub fn build(&self) -> Message {
        self.inner.clone()
    }
}

impl From<Message> for MessageBuilder {
    fn from(message: Message) -> Self {
        Self {
            inner: Message { ..message },
        }
    }
}

#[cfg(test)]
mod test {
    use std::{convert::TryFrom, str::from_utf8_mut};

    use uuid::Uuid;

    use crate::contacts_service::types::{
        message::{MAX_BODY_SIZE, MAX_DATA_SIZE, MAX_KEY_SIZE, MAX_MESSAGE_ID_SIZE},
        ChatBody,
        MessageBuilder,
        MessageId,
        MessageMetadata,
        MetadataData,
        MetadataKey,
    };

    #[test]
    fn test_message_id_size() {
        for _ in 0..10 {
            let message_id = Uuid::new_v4().to_string().into_bytes();
            assert!(
                MessageId::try_from(message_id.clone()).is_ok(),
                "Invalid size - MAX_MESSAGE_ID_SIZE length: {}, message_id length: {}",
                MessageId::default().len(),
                message_id.len()
            );
        }
    }

    #[test]
    fn test_message_size() {
        assert!(MetadataKey::try_from(vec![0u8; MAX_KEY_SIZE]).is_ok());
        assert!(MetadataKey::try_from(vec![0u8; MAX_KEY_SIZE + 1]).is_err());
        assert!(MetadataData::try_from(vec![0u8; MAX_DATA_SIZE]).is_ok());
        assert!(MetadataData::try_from(vec![0u8; MAX_DATA_SIZE + 1]).is_err());

        assert!(ChatBody::try_from(vec![0u8; MAX_BODY_SIZE]).is_ok());
        assert!(ChatBody::try_from(vec![0u8; MAX_BODY_SIZE + 1]).is_err());

        assert!(MessageId::try_from(vec![0u8; MAX_MESSAGE_ID_SIZE]).is_ok());
        assert!(MessageId::try_from(vec![0u8; MAX_MESSAGE_ID_SIZE + 1]).is_err());

        let mut builder = MessageBuilder::new();
        builder = builder
            .metadata(MessageMetadata {
                key: Default::default(),
                data: Default::default(),
            })
            .unwrap();
        let message = builder.build();
        assert_eq!(message.metadata.len(), 1);
        builder = builder
            .metadata(MessageMetadata {
                key: MetadataKey::try_from(vec![0u8; MAX_KEY_SIZE]).unwrap(),
                data: MetadataData::try_from(vec![0u8; MAX_DATA_SIZE]).unwrap(),
            })
            .unwrap();
        let message = builder.build();
        assert_eq!(message.metadata.len(), 2);
        assert!(builder
            .metadata(MessageMetadata {
                key: MetadataKey::try_from(vec![0u8; MAX_KEY_SIZE]).unwrap(),
                data: MetadataData::try_from(vec![0u8; MAX_DATA_SIZE]).unwrap()
            })
            .is_err());
        let message = builder.build();
        assert_eq!(message.metadata.len(), 2);

        let builder = MessageBuilder::new();
        let mut body = vec![0u8; MAX_BODY_SIZE];
        let body_str = from_utf8_mut(&mut body).unwrap().to_string();
        builder.message(body_str.clone()).unwrap();
        assert!(builder.message(body_str).is_ok());
        let mut body = vec![0u8; MAX_BODY_SIZE + 1];
        let body_str = from_utf8_mut(&mut body).unwrap().to_string();
        assert!(builder.message(body_str).is_err());
    }
}
