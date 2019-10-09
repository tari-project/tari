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

use crate::envelope::DhtHeader;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_comms::message::MessageEnvelopeHeader;

/// The RetrieveMessageRequest is used for requesting the set of stored messages from neighbouring peer nodes. If a
/// start_time is provided then only messages after the specified time will be sent, otherwise all applicable messages
/// will be sent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct StoredMessagesRequest {
    pub since: Option<DateTime<Utc>>,
}

impl StoredMessagesRequest {
    pub fn new() -> Self {
        Self { since: None }
    }

    pub fn since(since: DateTime<Utc>) -> Self {
        Self { since: Some(since) }
    }
}

/// Storage for a single message envelope, including the date and time when the element was stored
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredMessage {
    pub stored_at: DateTime<Utc>,
    pub version: u8,
    pub comms_header: MessageEnvelopeHeader,
    pub dht_header: DhtHeader,
    pub encrypted_body: Vec<u8>,
}

impl StoredMessage {
    pub fn new(
        version: u8,
        comms_header: MessageEnvelopeHeader,
        dht_header: DhtHeader,
        encrypted_body: Vec<u8>,
    ) -> Self
    {
        Self {
            version,
            comms_header,
            dht_header,
            encrypted_body,
            stored_at: Utc::now(),
        }
    }
}

/// The StoredMessages contains the set of applicable messages retrieved from a neighbouring peer node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredMessagesResponse {
    pub messages: Vec<StoredMessage>,
}

impl StoredMessagesResponse {
    pub fn len(&self) -> usize {
        self.messages.len()
    }
}

impl From<Vec<StoredMessage>> for StoredMessagesResponse {
    fn from(messages: Vec<StoredMessage>) -> Self {
        Self { messages }
    }
}
