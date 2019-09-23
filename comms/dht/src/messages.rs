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

use crate::utils::crypto;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use tari_comms::{
    connection::NetAddress,
    message::{MessageEnvelopeHeader, MessageFlags, NodeDestination},
    peer_manager::{NodeId, Peer},
    types::CommsPublicKey,
};

/// The JoinMessage stores the information required for a network join request. It has all the information required to
/// locate and contact the source node, but network behaviour is different compared to DiscoverMessage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct JoinMessage {
    pub node_id: NodeId,
    // TODO: node_type
    pub net_address: Vec<NetAddress>,
}

/// The DiscoverMessage stores the information required for a network discover request. It has all the information
/// required to locate and contact the source node, but network behaviour is different compared to JoinMessage.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiscoverMessage {
    pub node_id: NodeId,
    // TODO: node_type
    pub net_address: Vec<NetAddress>,
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone)]
#[repr(u8)]
pub enum DhtMessageType {
    None = 0,
    Join = 1,
    Discover = 2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DhtHeader {
    pub version: u8,
    pub destination: NodeDestination<CommsPublicKey>,
    pub origin_pubkey: CommsPublicKey,
    pub origin_signature: Vec<u8>,
    pub message_type: DhtMessageType,
    pub flags: MessageFlags,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DhtEnvelope {
    pub header: DhtHeader,
    pub body: Vec<u8>,
}

impl DhtEnvelope {
    pub fn is_signature_valid(&self) -> bool {
        // error here means that the signature could not deserialize, so is invalid
        match crypto::verify(&self.header.origin_pubkey, &self.header.origin_signature, &self.body) {
            Ok(is_valid) => is_valid,
            Err(_) => false,
        }
    }
}

pub struct DhtInboundMessage {
    pub source_peer: Peer,
    pub comms_header: MessageEnvelopeHeader,
    pub dht_header: DhtHeader,
    pub body: Vec<u8>,
}

impl DhtInboundMessage {
    pub fn new(dht_header: DhtHeader, source_peer: Peer, comms_header: MessageEnvelopeHeader, body: Vec<u8>) -> Self {
        Self {
            dht_header,
            source_peer,
            comms_header,
            body,
        }
    }
}
