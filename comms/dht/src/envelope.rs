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
use derive_error::Error;
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
    fmt::Display,
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, utils::signature};
use tari_utilities::{hex::Hex, ByteArray, ByteArrayError};

// Re-export applicable protos
pub use crate::proto::envelope::{dht_header::Destination, DhtEnvelope, DhtHeader, DhtMessageType, Network};

#[derive(Debug, Error)]
pub enum DhtMessageError {
    /// Invalid node destination
    InvalidDestination,
    /// Invalid origin public key
    InvalidOriginPublicKey,
    /// Invalid or unrecognised DHT message type
    InvalidMessageType,
    /// Invalid or unrecognised network type
    InvalidNetwork,
    /// Invalid or unrecognised DHT message flags
    InvalidMessageFlags,
    /// Header was omitted from the message
    HeaderOmitted,
}

bitflags! {
    /// Used to indicate characteristics of the incoming or outgoing message, such
    /// as whether the message is encrypted.
    #[derive(Deserialize, Serialize, Default)]
    pub struct DhtMessageFlags: u32 {
        const NONE = 0b0000_0000;
        const ENCRYPTED = 0b0000_0001;
    }
}

impl DhtMessageType {
    pub fn is_dht_message(&self) -> bool {
        match self {
            DhtMessageType::None => false,
            _ => true,
        }
    }
}

/// This struct mirrors the protobuf version of DhtHeader but is more ergonomic to work with.
/// It is preferable to not to expose the generated prost structs publicly.
#[derive(Clone, PartialEq, Eq)]
pub struct DhtMessageHeader {
    pub version: u32,
    pub destination: NodeDestination,
    /// Origin public key of the message. This can be the same peer that sent the message
    /// or another peer if the message should be forwarded.
    pub origin_public_key: CommsPublicKey,
    pub origin_signature: Vec<u8>,
    pub message_type: DhtMessageType,
    pub network: Network,
    pub flags: DhtMessageFlags,
}

impl DhtMessageHeader {
    pub fn new(
        destination: NodeDestination,
        origin_pubkey: CommsPublicKey,
        origin_signature: Vec<u8>,
        message_type: DhtMessageType,
        network: Network,
        flags: DhtMessageFlags,
    ) -> Self
    {
        Self {
            version: DHT_ENVELOPE_HEADER_VERSION,
            destination: destination.into(),
            origin_public_key: origin_pubkey,
            origin_signature,
            message_type,
            network,
            flags,
        }
    }
}

impl fmt::Debug for DhtMessageHeader {
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

impl TryFrom<DhtHeader> for DhtMessageHeader {
    type Error = DhtMessageError;

    fn try_from(header: DhtHeader) -> Result<Self, Self::Error> {
        Ok(Self::new(
            header
                .destination
                .map(|destination| destination.try_into().ok())
                .filter(Option::is_some)
                .map(Option::unwrap)
                .ok_or(DhtMessageError::InvalidDestination)?,
            CommsPublicKey::from_bytes(&header.origin_public_key)
                .map_err(|_| DhtMessageError::InvalidOriginPublicKey)?,
            header.origin_signature,
            DhtMessageType::from_i32(header.message_type).ok_or(DhtMessageError::InvalidMessageType)?,
            Network::from_i32(header.network).ok_or(DhtMessageError::InvalidNetwork)?,
            DhtMessageFlags::from_bits(header.flags).ok_or(DhtMessageError::InvalidMessageFlags)?,
        ))
    }
}

impl TryFrom<Option<DhtHeader>> for DhtMessageHeader {
    type Error = DhtMessageError;

    fn try_from(header: Option<DhtHeader>) -> Result<Self, Self::Error> {
        match header {
            Some(header) => header.try_into(),
            None => Err(DhtMessageError::HeaderOmitted),
        }
    }
}

impl From<DhtMessageHeader> for DhtHeader {
    fn from(header: DhtMessageHeader) -> Self {
        Self {
            version: header.version,
            origin_public_key: header.origin_public_key.to_vec(),
            origin_signature: header.origin_signature,
            destination: Some(header.destination.into()),
            message_type: header.message_type as i32,
            network: header.network as i32,
            flags: header.flags.bits(),
        }
    }
}

impl DhtEnvelope {
    pub fn new(header: DhtHeader, body: Vec<u8>) -> Self {
        Self {
            header: Some(header),
            body,
        }
    }

    pub fn is_signature_valid(&self) -> bool {
        self.header
            .as_ref()
            .and_then(|header| {
                CommsPublicKey::from_bytes(&header.origin_public_key)
                    .map(|pk| (pk, &header.origin_signature))
                    .ok()
            })
            .map(|(origin_public_key, origin_signature)| {
                match signature::verify(&origin_public_key, origin_signature, &self.body) {
                    Ok(is_valid) => is_valid,
                    // error means that the signature could not deserialize, so is invalid
                    Err(_) => false,
                }
            })
            .unwrap_or(false)
    }
}

/// Represents the ways a destination node can be represented.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum NodeDestination {
    /// The sender has chosen not to disclose the message destination, or the destination is
    /// the peer being sent to.
    Unknown,
    /// Destined for a particular public key
    PublicKey(CommsPublicKey),
    /// Destined for a particular node id, or network region
    NodeId(NodeId),
}

impl NodeDestination {
    pub fn to_inner_bytes(&self) -> Vec<u8> {
        match self {
            NodeDestination::Unknown => Vec::default(),
            NodeDestination::PublicKey(pk) => pk.to_vec(),
            NodeDestination::NodeId(node_id) => node_id.to_vec(),
        }
    }
}

impl Display for NodeDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            NodeDestination::Unknown => write!(f, "Unknown"),
            NodeDestination::NodeId(node_id) => write!(f, "NodeId({})", node_id),
            NodeDestination::PublicKey(public_key) => write!(f, "PublicKey({})", public_key),
        }
    }
}

impl Default for NodeDestination {
    fn default() -> Self {
        NodeDestination::Unknown
    }
}

impl TryFrom<Destination> for NodeDestination {
    type Error = ByteArrayError;

    fn try_from(destination: Destination) -> Result<Self, Self::Error> {
        match destination {
            Destination::Unknown(_) => Ok(NodeDestination::Unknown),
            Destination::PublicKey(pk) => {
                CommsPublicKey::from_bytes(&pk).and_then(|pk| Ok(NodeDestination::PublicKey(pk)))
            },
            Destination::NodeId(node_id) => {
                NodeId::from_bytes(&node_id).and_then(|node_id| Ok(NodeDestination::NodeId(node_id)))
            },
        }
    }
}

impl From<NodeDestination> for Destination {
    fn from(destination: NodeDestination) -> Self {
        use NodeDestination::*;
        match destination {
            Unknown => Destination::Unknown(true),
            PublicKey(pk) => Destination::PublicKey(pk.to_vec()),
            NodeId(node_id) => Destination::NodeId(node_id.to_vec()),
        }
    }
}
