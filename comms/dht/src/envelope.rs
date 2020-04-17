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

use bitflags::bitflags;
use derive_error::Error;
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
    fmt::Display,
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
use tari_crypto::tari_utilities::{ByteArray, ByteArrayError};

// Re-export applicable protos
pub use crate::proto::envelope::{dht_header::Destination, DhtEnvelope, DhtHeader, DhtMessageType, Network};
use bytes::Bytes;

#[derive(Debug, Error)]
pub enum DhtMessageError {
    /// Invalid node destination
    InvalidDestination,
    /// Invalid origin public key
    InvalidOrigin,
    /// Invalid or unrecognised DHT message type
    InvalidMessageType,
    /// Invalid or unrecognised network type
    InvalidNetwork,
    /// Invalid or unrecognised DHT message flags
    InvalidMessageFlags,
    /// Invalid ephemeral public key
    InvalidEphemeralPublicKey,
    /// Header was omitted from the message
    HeaderOmitted,
}

impl fmt::Display for DhtMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Debug output works well for simple enums
        fmt::Debug::fmt(self, f)
    }
}

bitflags! {
    /// Used to indicate characteristics of the incoming or outgoing message, such
    /// as whether the message is encrypted.
    #[derive(Deserialize, Serialize, Default)]
    pub struct DhtMessageFlags: u32 {
        const NONE = 0x00;
        /// Set if the message is encrypted
        const ENCRYPTED = 0x01;
    }
}

impl DhtMessageFlags {
    pub fn is_encrypted(self) -> bool {
        self.contains(Self::ENCRYPTED)
    }
}

impl DhtMessageType {
    pub fn is_dht_message(self) -> bool {
        self.is_dht_discovery() || self.is_dht_join()
    }

    pub fn is_dht_discovery(self) -> bool {
        match self {
            DhtMessageType::Discovery => true,
            _ => false,
        }
    }

    pub fn is_dht_join(self) -> bool {
        match self {
            DhtMessageType::Join => true,
            _ => false,
        }
    }

    pub fn is_saf_message(self) -> bool {
        use DhtMessageType::*;
        match self {
            SafRequestMessages | SafStoredMessages => true,
            _ => false,
        }
    }
}

/// This struct mirrors the protobuf version of DhtHeader but is more ergonomic to work with.
/// It is preferable to not to expose the generated prost structs publicly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DhtMessageHeader {
    pub version: u32,
    pub destination: NodeDestination,
    /// Encoded DhtOrigin. This can refer to the same peer that sent the message
    /// or another peer if the message is being propagated.
    pub origin_mac: Vec<u8>,
    pub ephemeral_public_key: Option<CommsPublicKey>,
    pub message_type: DhtMessageType,
    pub network: Network,
    pub flags: DhtMessageFlags,
}

impl DhtMessageHeader {
    pub fn is_valid(&self) -> bool {
        if self.flags.contains(DhtMessageFlags::ENCRYPTED) {
            !self.origin_mac.is_empty() && self.ephemeral_public_key.is_some()
        } else {
            true
        }
    }
}

impl Display for DhtMessageHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "DhtMessageHeader (Dest:{}, Type:{:?}, Network:{:?}, Flags:{:?})",
            self.destination, self.message_type, self.network, self.flags
        )
    }
}

impl TryFrom<DhtHeader> for DhtMessageHeader {
    type Error = DhtMessageError;

    fn try_from(header: DhtHeader) -> Result<Self, Self::Error> {
        let destination = header
            .destination
            .map(|destination| destination.try_into().ok())
            .filter(Option::is_some)
            .map(Option::unwrap)
            .ok_or_else(|| DhtMessageError::InvalidDestination)?;

        let ephemeral_public_key = if header.ephemeral_public_key.is_empty() {
            None
        } else {
            Some(
                CommsPublicKey::from_bytes(&header.ephemeral_public_key)
                    .map_err(|_| DhtMessageError::InvalidEphemeralPublicKey)?,
            )
        };

        Ok(Self {
            version: header.version,
            destination,
            origin_mac: header.origin_mac,
            ephemeral_public_key,
            message_type: DhtMessageType::from_i32(header.message_type)
                .ok_or_else(|| DhtMessageError::InvalidMessageType)?,
            network: Network::from_i32(header.network).ok_or_else(|| DhtMessageError::InvalidNetwork)?,
            flags: DhtMessageFlags::from_bits(header.flags).ok_or_else(|| DhtMessageError::InvalidMessageFlags)?,
        })
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
            ephemeral_public_key: header
                .ephemeral_public_key
                .as_ref()
                .map(ByteArray::to_vec)
                .unwrap_or_else(Vec::new),
            origin_mac: header.origin_mac,
            destination: Some(header.destination.into()),
            message_type: header.message_type as i32,
            network: header.network as i32,
            flags: header.flags.bits(),
        }
    }
}

impl DhtEnvelope {
    pub fn new(header: DhtHeader, body: Bytes) -> Self {
        Self {
            header: Some(header),
            body: body.to_vec(),
        }
    }
}

/// Represents the ways a destination node can be represented.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum NodeDestination {
    /// The sender has chosen not to disclose the message destination, or the destination is
    /// the peer being sent to.
    Unknown,
    /// Destined for a particular public key
    PublicKey(Box<CommsPublicKey>),
    /// Destined for a particular node id, or network region
    NodeId(Box<NodeId>),
}

impl NodeDestination {
    pub fn to_inner_bytes(&self) -> Vec<u8> {
        match self {
            NodeDestination::Unknown => Vec::default(),
            NodeDestination::PublicKey(pk) => pk.to_vec(),
            NodeDestination::NodeId(node_id) => node_id.to_vec(),
        }
    }

    pub fn public_key(&self) -> Option<&CommsPublicKey> {
        match self {
            NodeDestination::Unknown => None,
            NodeDestination::PublicKey(pk) => Some(pk),
            NodeDestination::NodeId(_) => None,
        }
    }

    pub fn node_id(&self) -> Option<&NodeId> {
        match self {
            NodeDestination::Unknown => None,
            NodeDestination::PublicKey(_) => None,
            NodeDestination::NodeId(node_id) => Some(node_id),
        }
    }

    pub fn is_unknown(&self) -> bool {
        match self {
            NodeDestination::Unknown => true,
            _ => false,
        }
    }
}

impl PartialEq<&CommsPublicKey> for NodeDestination {
    fn eq(&self, other: &&CommsPublicKey) -> bool {
        self.public_key().map(|pk| pk == *other).unwrap_or(false)
    }
}

impl PartialEq<&NodeId> for NodeDestination {
    fn eq(&self, other: &&NodeId) -> bool {
        self.node_id().map(|node_id| node_id == *other).unwrap_or(false)
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
                CommsPublicKey::from_bytes(&pk).and_then(|pk| Ok(NodeDestination::PublicKey(Box::new(pk))))
            },
            Destination::NodeId(node_id) => {
                NodeId::from_bytes(&node_id).and_then(|node_id| Ok(NodeDestination::NodeId(Box::new(node_id))))
            },
        }
    }
}

impl From<CommsPublicKey> for NodeDestination {
    fn from(pk: CommsPublicKey) -> Self {
        NodeDestination::PublicKey(Box::new(pk))
    }
}

impl From<NodeId> for NodeDestination {
    fn from(node_id: NodeId) -> Self {
        NodeDestination::NodeId(Box::new(node_id))
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
