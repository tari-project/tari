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

use crate::consts::DHT_ENVELOPE_HEADER_VERSION;
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::fmt;
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, utils::signature};
use tari_utilities::hex::Hex;

bitflags! {
    /// Used to indicate characteristics of the incoming or outgoing message, such
    /// as whether the message is encrypted.
    #[derive(Deserialize, Serialize)]
    pub struct DhtMessageFlags: u8 {
        const NONE = 0b0000_0000;
        const ENCRYPTED = 0b0000_0001;
    }
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum DhtMessageType {
    /// Not a DHT message
    None = 0,
    // DHT
    /// Join Request
    Join = 1,
    /// Discover Request
    Discover = 2,
    // Store and Forward
    /// Request messages from a node
    SAFRequestMessages = 3,
    /// Message contains the stored messages
    SAFStoredMessages = 4,
}

impl DhtMessageType {
    pub fn is_dht_message(&self) -> bool {
        match self {
            DhtMessageType::None => false,
            _ => true,
        }
    }
}

impl Default for DhtMessageType {
    fn default() -> Self {
        DhtMessageType::None
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DhtHeader {
    pub version: u8,
    pub destination: NodeDestination,
    /// Origin public key of the message. This can be the same peer that sent the message
    /// or another peer if the message should be forwarded.
    pub origin_public_key: CommsPublicKey,
    pub origin_signature: Vec<u8>,
    pub message_type: DhtMessageType,
    pub flags: DhtMessageFlags,
}

impl DhtHeader {
    pub fn new(
        destination: NodeDestination,
        origin_pubkey: CommsPublicKey,
        origin_signature: Vec<u8>,
        message_type: DhtMessageType,
        flags: DhtMessageFlags,
    ) -> Self
    {
        Self {
            version: DHT_ENVELOPE_HEADER_VERSION,
            destination,
            origin_public_key: origin_pubkey,
            origin_signature,
            message_type,
            flags,
        }
    }
}

impl fmt::Debug for DhtHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DhtHeader")
            .field("version", &self.version)
            .field("destination", &self.destination)
            .field("origin_public_key", &self.origin_public_key.to_hex())
            .field("origin_signature", &self.origin_signature.to_hex())
            .field("message_type", &self.message_type)
            .field("flags", &self.flags)
            .finish()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DhtEnvelope {
    pub header: DhtHeader,
    pub body: Vec<u8>,
}

impl DhtEnvelope {
    pub fn new(header: DhtHeader, body: Vec<u8>) -> Self {
        Self { header, body }
    }

    pub fn is_signature_valid(&self) -> bool {
        match signature::verify(
            &self.header.origin_public_key,
            &self.header.origin_signature,
            &self.body,
        ) {
            Ok(is_valid) => is_valid,
            // error means that the signature could not deserialize, so is invalid
            Err(_) => false,
        }
    }
}

/// Represents the ways a destination node can be represented.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum NodeDestination {
    /// The sender has chosen not to disclose the message destination, or the destination is
    /// the peer being sent to.
    Unspecified,
    /// Destined for a particular public key
    PublicKey(CommsPublicKey),
    /// Destined for a particular node id, or network region
    NodeId(NodeId),
}

impl Default for NodeDestination {
    fn default() -> Self {
        NodeDestination::Unspecified
    }
}
