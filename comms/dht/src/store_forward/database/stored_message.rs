// Copyright 2020, The Tari Project
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
    inbound::DecryptedDhtMessage,
    proto::envelope::DhtHeader,
    schema::stored_messages,
    store_forward::message::StoredMessagePriority,
};
use chrono::NaiveDateTime;
use digest::Input;
use std::convert::TryInto;
use tari_comms::{message::MessageExt, types::Challenge};
use tari_utilities::hex::Hex;

#[derive(Clone, Debug, Insertable, Default)]
#[table_name = "stored_messages"]
pub struct NewStoredMessage {
    pub version: i32,
    pub origin_pubkey: Option<String>,
    pub message_type: i32,
    pub destination_pubkey: Option<String>,
    pub destination_node_id: Option<String>,
    pub header: Vec<u8>,
    pub body: Vec<u8>,
    pub is_encrypted: bool,
    pub priority: i32,
    pub body_hash: String,
}

impl NewStoredMessage {
    pub fn try_construct(message: DecryptedDhtMessage, priority: StoredMessagePriority) -> Option<Self> {
        let DecryptedDhtMessage {
            version,
            authenticated_origin,
            decryption_result,
            dht_header,
            ..
        } = message;

        let body = match decryption_result {
            Ok(envelope_body) => envelope_body.to_encoded_bytes(),
            Err(encrypted_body) => encrypted_body,
        };

        Some(Self {
            version: version.try_into().ok()?,
            origin_pubkey: authenticated_origin.as_ref().map(|pk| pk.to_hex()),
            message_type: dht_header.message_type as i32,
            destination_pubkey: dht_header.destination.public_key().map(|pk| pk.to_hex()),
            destination_node_id: dht_header.destination.raw_node_id().map(|node_id| node_id.to_hex()),
            is_encrypted: dht_header.flags.is_encrypted(),
            priority: priority as i32,
            header: {
                let dht_header: DhtHeader = dht_header.into();
                dht_header.to_encoded_bytes()
            },
            body_hash: Challenge::new().chain(body.clone()).result().to_vec().to_hex(),
            body,
        })
    }
}

#[derive(Clone, Debug, Queryable, Identifiable)]
pub struct StoredMessage {
    pub id: i32,
    pub version: i32,
    pub origin_pubkey: Option<String>,
    pub message_type: i32,
    pub destination_pubkey: Option<String>,
    pub destination_node_id: Option<String>,
    pub header: Vec<u8>,
    pub body: Vec<u8>,
    pub is_encrypted: bool,
    pub priority: i32,
    pub stored_at: NaiveDateTime,
    pub body_hash: String,
}
