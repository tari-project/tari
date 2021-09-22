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

use crate::envelope::{DhtMessageFlags, DhtMessageHeader};
use std::{
    fmt,
    fmt::{Display, Formatter},
    sync::Arc,
};
use tari_comms::{
    message::{EnvelopeBody, MessageTag},
    peer_manager::Peer,
    types::CommsPublicKey,
};

#[derive(Debug, Clone)]
pub struct DhtInboundMessage {
    pub tag: MessageTag,
    pub source_peer: Arc<Peer>,
    pub dht_header: DhtMessageHeader,
    /// True if forwarded via store and forward, otherwise false
    pub is_saf_message: bool,
    pub dedup_hit_count: u32,
    pub body: Vec<u8>,
}
impl DhtInboundMessage {
    pub fn new(tag: MessageTag, dht_header: DhtMessageHeader, source_peer: Arc<Peer>, body: Vec<u8>) -> Self {
        Self {
            tag,
            dht_header,
            source_peer,
            is_saf_message: false,
            dedup_hit_count: 0,
            body,
        }
    }
}

impl Display for DhtInboundMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "\n---- Inbound Message ---- \nSize: {} byte(s)\nType: {}\nPeer: {}\nHit Count: {}\nHeader: {}\n{}\n----",
            self.body.len(),
            self.dht_header.message_type,
            self.source_peer,
            self.dedup_hit_count,
            self.dht_header,
            self.tag,
        )
    }
}

/// Represents a decrypted InboundMessage.
#[derive(Debug, Clone)]
pub struct DecryptedDhtMessage {
    pub tag: MessageTag,
    /// The _connected_ peer which sent or forwarded this message. This may not be the peer
    /// which created this message.
    pub source_peer: Arc<Peer>,
    pub authenticated_origin: Option<CommsPublicKey>,
    pub dht_header: DhtMessageHeader,
    pub is_saf_message: bool,
    pub is_saf_stored: Option<bool>,
    pub is_already_forwarded: bool,
    pub decryption_result: Result<EnvelopeBody, Vec<u8>>,
    pub dedup_hit_count: u32,
}

impl DecryptedDhtMessage {
    /// Returns true if this message has been received before, otherwise false if this is the first time
    pub fn is_duplicate(&self) -> bool {
        self.dedup_hit_count > 1
    }

    pub fn major_version(&self) -> u32 {
        self.dht_header.version.as_major()
    }
}

impl DecryptedDhtMessage {
    pub fn succeeded(
        message_body: EnvelopeBody,
        authenticated_origin: Option<CommsPublicKey>,
        message: DhtInboundMessage,
    ) -> Self {
        Self {
            tag: message.tag,
            source_peer: message.source_peer,
            authenticated_origin,
            dht_header: message.dht_header,
            is_saf_message: message.is_saf_message,
            is_saf_stored: None,
            is_already_forwarded: false,
            decryption_result: Ok(message_body),
            dedup_hit_count: message.dedup_hit_count,
        }
    }

    pub fn failed(message: DhtInboundMessage) -> Self {
        Self {
            tag: message.tag,
            source_peer: message.source_peer,
            authenticated_origin: None,
            dht_header: message.dht_header,
            is_saf_message: message.is_saf_message,
            is_saf_stored: None,
            is_already_forwarded: false,
            decryption_result: Err(message.body),
            dedup_hit_count: message.dedup_hit_count,
        }
    }

    pub fn fail(&self) -> Option<&Vec<u8>> {
        self.decryption_result.as_ref().err()
    }

    pub fn fail_mut(&mut self) -> Option<&mut Vec<u8>> {
        self.decryption_result.as_mut().err()
    }

    pub fn success(&self) -> Option<&EnvelopeBody> {
        self.decryption_result.as_ref().ok()
    }

    pub fn success_mut(&mut self) -> Option<&mut EnvelopeBody> {
        self.decryption_result.as_mut().ok()
    }

    pub fn decryption_succeeded(&self) -> bool {
        self.decryption_result.is_ok()
    }

    pub fn decryption_failed(&self) -> bool {
        self.decryption_result.is_err()
    }

    pub fn authenticated_origin(&self) -> Option<&CommsPublicKey> {
        self.authenticated_origin.as_ref()
    }

    /// Returns true if the message is or was encrypted by
    pub fn is_encrypted(&self) -> bool {
        self.dht_header.flags.contains(DhtMessageFlags::ENCRYPTED)
    }

    pub fn has_origin_mac(&self) -> bool {
        !self.dht_header.origin_mac.is_empty()
    }

    pub fn body_len(&self) -> usize {
        match self.decryption_result.as_ref() {
            Ok(b) => b.total_size(),
            Err(b) => b.len(),
        }
    }

    pub fn set_saf_stored(&mut self, is_stored: bool) {
        self.is_saf_stored = Some(is_stored);
    }

    pub fn set_already_forwarded(&mut self, is_already_forwarded: bool) {
        self.is_already_forwarded = is_already_forwarded;
    }
}

impl Display for DecryptedDhtMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "version = {}, origin = {}, decryption_result = {}, header = ({}), is_saf_message = {}, is_saf_stored = \
             {:?}, source_peer = {}, tag = {}",
            self.major_version(),
            self.authenticated_origin
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "None".to_string()),
            self.decryption_result
                .as_ref()
                .map(|envelope| format!("Success({})", envelope))
                .unwrap_or_else(|_| "Failed".to_string()),
            self.dht_header,
            self.is_saf_message,
            self.is_saf_stored,
            self.source_peer.node_id,
            self.tag
        )
    }
}
