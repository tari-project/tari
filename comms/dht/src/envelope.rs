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

use crate::{consts::DHT_ENVELOPE_HEADER_VERSION, proto::envelope::DhtOrigin};
use bitflags::bitflags;
use derive_error::Error;
use serde::{Deserialize, Serialize};
use std::{
    convert::{TryFrom, TryInto},
    fmt,
    fmt::Display,
};
use tari_comms::{peer_manager::NodeId, types::CommsPublicKey, utils::signature};
use tari_crypto::tari_utilities::{hex::Hex, ByteArray, ByteArrayError};

// Re-export applicable protos
pub use crate::proto::envelope::{dht_header::Destination, DhtEnvelope, DhtHeader, DhtMessageType, Network};

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
    pub fn is_dht_message(self) -> bool {
        match self {
            DhtMessageType::None => false,
            _ => true,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DhtMessageOrigin {
    pub public_key: CommsPublicKey,
    pub signature: Vec<u8>,
}

impl fmt::Debug for DhtMessageOrigin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DhtMessageOrigin")
            .field("public_key", &self.public_key.to_hex())
            .field("signature", &self.signature.to_hex())
            .finish()
    }
}

impl TryFrom<DhtOrigin> for DhtMessageOrigin {
    type Error = DhtMessageError;

    fn try_from(value: DhtOrigin) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: CommsPublicKey::from_bytes(&value.public_key).map_err(|_| DhtMessageError::InvalidOrigin)?,
            signature: value.signature,
        })
    }
}

impl From<DhtMessageOrigin> for DhtOrigin {
    fn from(value: DhtMessageOrigin) -> Self {
        Self {
            public_key: value.public_key.to_vec(),
            signature: value.signature,
        }
    }
}

/// This struct mirrors the protobuf version of DhtHeader but is more ergonomic to work with.
/// It is preferable to not to expose the generated prost structs publicly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DhtMessageHeader {
    pub version: u32,
    pub destination: NodeDestination,
    /// Origin of the message. This can refer to the same peer that sent the message
    /// or another peer if the message should be forwarded.
    pub origin: Option<DhtMessageOrigin>,
    pub message_type: DhtMessageType,
    pub network: Network,
    pub flags: DhtMessageFlags,
}

impl DhtMessageHeader {
    pub fn new(
        destination: NodeDestination,
        message_type: DhtMessageType,
        origin: Option<DhtMessageOrigin>,
        network: Network,
        flags: DhtMessageFlags,
    ) -> Self
    {
        Self {
            version: DHT_ENVELOPE_HEADER_VERSION,
            destination,
            origin,
            message_type,
            network,
            flags,
        }
    }
}

impl Display for DhtMessageHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "DhtMessageHeader (Dest:{}, Origin:{:?}, Type:{:?}, Network:{:?}, Flags:{:?})",
            self.destination, self.origin, self.message_type, self.network, self.flags
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

        let origin = match header.origin {
            Some(origin) => Some(origin.try_into()?),
            None => None,
        };

        Ok(Self {
            version: header.version,
            destination,
            origin,
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
            origin: header.origin.map(Into::into),
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

    /// Returns true if the header and origin are present, otherwise false
    pub fn has_origin(&self) -> bool {
        self.header.as_ref().map(|h| h.origin.is_some()).unwrap_or(false)
    }

    /// Verifies the origin signature and returns true if it is valid.
    ///
    /// This method panics if called on an envelope without an origin. This should be checked before calling this
    /// function by using the `DhtEnvelope::has_origin` method
    pub fn is_origin_signature_valid(&self) -> bool {
        self.header
            .as_ref()
            .and_then(|header| {
                let origin = header
                    .origin
                    .as_ref()
                    .expect("call is_origin_signature_valid on envelope without origin");

                CommsPublicKey::from_bytes(&origin.public_key)
                    .map(|pk| (pk, &origin.signature))
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
