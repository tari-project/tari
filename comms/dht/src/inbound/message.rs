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

use crate::{consts::DHT_ENVELOPE_HEADER_VERSION, message::DhtHeader};
use tari_comms::{
    message::{Message, MessageEnvelopeHeader},
    peer_manager::Peer,
};

pub struct DhtInboundMessage {
    pub version: u8,
    pub source_peer: Peer,
    pub comms_header: MessageEnvelopeHeader,
    pub dht_header: DhtHeader,
    pub body: Vec<u8>,
}
impl DhtInboundMessage {
    pub fn new(dht_header: DhtHeader, source_peer: Peer, comms_header: MessageEnvelopeHeader, body: Vec<u8>) -> Self {
        Self {
            version: DHT_ENVELOPE_HEADER_VERSION,
            dht_header,
            source_peer,
            comms_header,
            body,
        }
    }
}

/// Represents a decrypted InboundMessage.
pub struct DecryptedDhtMessage {
    pub version: u8,
    pub source_peer: Peer,
    pub comms_header: MessageEnvelopeHeader,
    pub dht_header: DhtHeader,
    pub decryption_result: Result<Message, Vec<u8>>,
}

impl DecryptedDhtMessage {
    pub fn succeed(decrypted_message: Message, message: DhtInboundMessage) -> Self {
        Self {
            version: message.version,
            source_peer: message.source_peer,
            comms_header: message.comms_header,
            dht_header: message.dht_header,
            decryption_result: Ok(decrypted_message),
        }
    }

    pub fn fail(message: DhtInboundMessage) -> Self {
        Self {
            version: message.version,
            source_peer: message.source_peer,
            comms_header: message.comms_header,
            dht_header: message.dht_header,
            decryption_result: Err(message.body),
        }
    }

    pub fn inner_success(&self) -> &Message {
        // Expect the caller to know that the decryption has succeeded
        self.decryption_result
            .as_ref()
            .expect("called inner_success on failed decryption message")
    }

    pub fn inner_fail(&self) -> &Vec<u8> {
        // Expect the caller to know that the decryption has succeeded
        self.decryption_result
            .as_ref()
            .err()
            .expect("called inner_fail on succesfully decrypted message")
    }

    pub fn failed(&self) -> Option<&Vec<u8>> {
        self.decryption_result.as_ref().err()
    }

    pub fn succeeded(&self) -> Option<&Message> {
        self.decryption_result.as_ref().ok()
    }

    pub fn decryption_succeeded(&self) -> bool {
        self.decryption_result.is_ok()
    }

    pub fn decryption_failed(&self) -> bool {
        self.decryption_result.is_err()
    }
}
