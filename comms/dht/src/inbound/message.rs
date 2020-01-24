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

use crate::{consts::DHT_ENVELOPE_HEADER_VERSION, envelope::DhtMessageHeader};
use tari_comms::{message::EnvelopeBody, peer_manager::Peer, types::CommsPublicKey};

#[derive(Debug, Clone)]
pub struct DhtInboundMessage {
    pub version: u32,
    pub source_peer: Peer,
    pub dht_header: DhtMessageHeader,
    pub body: Vec<u8>,
}
impl DhtInboundMessage {
    pub fn new(dht_header: DhtMessageHeader, source_peer: Peer, body: Vec<u8>) -> Self {
        Self {
            version: DHT_ENVELOPE_HEADER_VERSION,
            dht_header,
            source_peer,
            body,
        }
    }
}

/// Represents a decrypted InboundMessage.
#[derive(Debug, Clone)]
pub struct DecryptedDhtMessage {
    pub version: u32,
    /// The _connected_ peer which sent or forwarded this message. This may not be the peer
    /// which created this message.
    pub source_peer: Peer,
    pub dht_header: DhtMessageHeader,
    pub decryption_result: Result<EnvelopeBody, Vec<u8>>,
}

impl DecryptedDhtMessage {
    pub fn succeeded(decrypted_message: EnvelopeBody, message: DhtInboundMessage) -> Self {
        Self {
            version: message.version,
            source_peer: message.source_peer,
            dht_header: message.dht_header,
            decryption_result: Ok(decrypted_message),
        }
    }

    pub fn failed(message: DhtInboundMessage) -> Self {
        Self {
            version: message.version,
            source_peer: message.source_peer,
            dht_header: message.dht_header,
            decryption_result: Err(message.body),
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

    pub fn origin_public_key(&self) -> &CommsPublicKey {
        self.dht_header
            .origin
            .as_ref()
            .map(|o| &o.public_key)
            .unwrap_or(&self.source_peer.public_key)
    }
}
