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
    proto::{
        envelope::DhtHeader,
        store_forward::{StoredMessage, StoredMessagesRequest, StoredMessagesResponse},
    },
    store_forward::{database, StoreAndForwardError},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use prost::Message;
use prost_types::Timestamp;
use rand::{rngs::OsRng, RngCore};
use std::{
    cmp,
    convert::{TryFrom, TryInto},
};

/// Utility function that converts a `chrono::DateTime<Utc>` to a `prost::Timestamp`
pub(crate) fn datetime_to_timestamp(datetime: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: datetime.timestamp(),
        nanos: datetime.timestamp_subsec_nanos() as i32,
    }
}

/// Utility function that converts a `prost::Timestamp` to a `chrono::DateTime<Utc>`
pub(crate) fn timestamp_to_datetime(timestamp: Timestamp) -> DateTime<Utc> {
    let naive = NaiveDateTime::from_timestamp(timestamp.seconds, cmp::max(0, timestamp.nanos) as u32);
    DateTime::from_utc(naive, Utc)
}

impl StoredMessagesRequest {
    pub fn new() -> Self {
        Self {
            since: None,
            request_id: OsRng.next_u32(),
        }
    }

    pub fn since(since: DateTime<Utc>) -> Self {
        Self {
            since: Some(datetime_to_timestamp(since)),
            request_id: OsRng.next_u32(),
        }
    }
}

#[cfg(test)]
impl StoredMessage {
    pub fn new(version: u32, dht_header: crate::envelope::DhtMessageHeader, encrypted_body: Vec<u8>) -> Self {
        Self {
            version,
            dht_header: Some(dht_header.into()),
            body: encrypted_body,
            stored_at: Some(datetime_to_timestamp(Utc::now())),
        }
    }
}

impl TryFrom<database::StoredMessage> for StoredMessage {
    type Error = StoreAndForwardError;

    fn try_from(message: database::StoredMessage) -> Result<Self, Self::Error> {
        let dht_header = DhtHeader::decode(message.header.as_slice())?;
        Ok(Self {
            stored_at: Some(datetime_to_timestamp(DateTime::from_utc(message.stored_at, Utc))),
            version: message
                .version
                .try_into()
                .map_err(|_| StoreAndForwardError::InvalidEnvelopeVersion)?,
            body: message.body,
            dht_header: Some(dht_header),
        })
    }
}

impl StoredMessagesResponse {
    pub fn messages(&self) -> &Vec<StoredMessage> {
        &self.messages
    }
}

#[derive(Debug, Copy, Clone)]
pub enum StoredMessagePriority {
    Low = 1,
    High = 10,
}
