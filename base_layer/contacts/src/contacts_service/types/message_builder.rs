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

use tari_common_types::tari_address::TariAddress;
use uuid::Uuid;

use crate::contacts_service::types::{message::MessageMetadata, Message};

#[derive(Clone, Debug, Default)]
pub struct MessageBuilder {
    inner: Message,
}

impl MessageBuilder {
    pub fn new() -> Self {
        // We're forcing it to a String before bytes so we can have the same representation used in
        // all places. Otherwise the UUID byte format will differ if displayed somewhere.
        let message_id = Uuid::new_v4().to_string().into_bytes();

        Self {
            inner: Message {
                message_id,
                ..Message::default()
            },
        }
    }

    pub fn address(&self, address: TariAddress) -> Self {
        Self {
            inner: Message {
                address,
                ..self.inner.clone()
            },
        }
    }

    pub fn message(&self, body: String) -> Self {
        let body = body.into_bytes();
        Self {
            inner: Message {
                body,
                ..self.inner.clone()
            },
        }
    }

    pub fn metadata(&self, new_metadata: MessageMetadata) -> Self {
        let mut metadata = self.inner.metadata.clone();
        metadata.push(new_metadata);

        Self {
            inner: Message {
                metadata,
                ..self.inner.clone()
            },
        }
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
