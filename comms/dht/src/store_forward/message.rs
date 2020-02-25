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

use crate::{
    envelope::DhtMessageHeader,
    proto::store_forward::{StoredMessage, StoredMessagesRequest, StoredMessagesResponse},
};
use chrono::{DateTime, Utc};
use prost_types::Timestamp;

/// Utility function that converts a `chrono::DateTime` to a `prost::Timestamp`
pub(crate) fn datetime_to_timestamp(datetime: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: datetime.timestamp(),
        nanos: datetime.timestamp_subsec_nanos() as i32,
    }
}

impl StoredMessagesRequest {
    pub fn since(since: DateTime<Utc>) -> Self {
        Self {
            since: Some(datetime_to_timestamp(since)),
        }
    }
}

impl StoredMessage {
    pub fn new(version: u32, dht_header: DhtMessageHeader, encrypted_body: Vec<u8>) -> Self {
        Self {
            version,
            dht_header: Some(dht_header.into()),
            encrypted_body,
            stored_at: Some(datetime_to_timestamp(Utc::now())),
        }
    }

    pub fn has_required_fields(&self) -> bool {
        self.dht_header.is_some()
    }
}

impl StoredMessagesResponse {
    pub fn messages(&self) -> &Vec<StoredMessage> {
        &self.messages
    }
}

impl From<Vec<StoredMessage>> for StoredMessagesResponse {
    fn from(messages: Vec<StoredMessage>) -> Self {
        Self { messages }
    }
}
