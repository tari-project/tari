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

use std::{
    convert::{TryFrom, TryInto},
    fmt,
    fmt::Display,
};

use bitflags::bitflags;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_comms::{message::MessageTag, peer_manager::NodeId, types::CommsPublicKey, NodeIdentity};
use tari_utilities::{epoch_time::EpochTime, ByteArray, ByteArrayError};
use thiserror::Error;

// Re-export applicable protos
pub use crate::proto::envelope::{dht_header::Destination, DhtEnvelope, DhtHeader, DhtMessageType};
use crate::version::DhtProtocolVersion;

/// Utility function that converts a `chrono::DateTime` to a `EpochTime`
pub(crate) fn datetime_to_epochtime(datetime: DateTime<Utc>) -> EpochTime {
    #[allow(clippy::cast_sign_loss)]
    EpochTime::from_secs_since_epoch(datetime.timestamp() as u64)
}

/// Utility function that converts a `EpochTime` to a `chrono::DateTime`
pub(crate) fn epochtime_to_datetime(datetime: EpochTime) -> DateTime<Utc> {
    let dt = NaiveDateTime::from_timestamp_opt(i64::try_from(datetime.as_u64()).unwrap_or(i64::MAX), 0)
        .unwrap_or(NaiveDateTime::MAX);
    DateTime::from_naive_utc_and_offset(dt, Utc)
}

/// Message errors that should be verified by every node
#[derive(Debug, Error)]
pub enum DhtMessageError {
    #[error("Invalid node destination")]
    InvalidDestination,
    #[error("Invalid origin public key")]
    InvalidOrigin,
    #[error("Invalid or unrecognised DHT message type")]
    InvalidMessageType,
    #[error("Invalid or unsupported DHT protocol version {0}")]
    InvalidProtocolVersion(u32),
    #[error("Invalid or unrecognised network type")]
    InvalidNetwork,
    #[error("Invalid or unrecognised DHT message flags")]
    InvalidMessageFlags,
    #[error("Invalid ephemeral public key")]
    InvalidEphemeralPublicKey,
    #[error("Header is omitted from the message")]
    HeaderOmitted,
    #[error("Message Body is empty")]
    BodyEmpty,
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
    #[derive(Deserialize, Serialize, Default, Copy, Clone, Debug, Eq, PartialEq)]
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
    pub fn is_domain_message(self) -> bool {
        matches!(self, DhtMessageType::None)
    }

    pub fn is_dht_message(self) -> bool {
        self.is_dht_discovery() || self.is_dht_discovery_response() || self.is_dht_join()
    }

    pub fn is_forwardable(self) -> bool {
        self.is_domain_message() || self.is_dht_discovery() || self.is_dht_join()
    }

    pub fn is_dht_discovery(self) -> bool {
        matches!(self, DhtMessageType::Discovery)
    }

    pub fn is_dht_discovery_response(self) -> bool {
        matches!(self, DhtMessageType::DiscoveryResponse)
    }

    pub fn is_dht_join(self) -> bool {
        matches!(self, DhtMessageType::Join)
    }

    pub fn is_saf_message(self) -> bool {
        use DhtMessageType::{SafRequestMessages, SafStoredMessages};
        matches!(self, SafRequestMessages | SafStoredMessages)
    }
}

/// This struct mirrors the protobuf version of DhtHeader but is more ergonomic to work with.
/// It is preferable to not to expose the generated prost structs publicly.
#[derive(Clone, Debug, Eq)]
pub struct DhtMessageHeader {
    pub version: DhtProtocolVersion,
    pub destination: NodeDestination,
    pub message_signature: Vec<u8>,
    pub ephemeral_public_key: Option<CommsPublicKey>,
    pub message_type: DhtMessageType,
    pub flags: DhtMessageFlags,
    pub message_tag: MessageTag,
    pub expires: Option<EpochTime>,
}

impl DhtMessageHeader {
    /// Checks if the DHT header is semantically valid. For example, if the message is flagged as encrypted, but sets a
    /// empty signature or provides no ephemeral public key, this returns false.
    pub fn is_semantically_valid(&self) -> bool {
        // If the message is encrypted:
        // - it needs a destination
        // - it needs an ephemeral public key
        // - it needs a signature
        if self.flags.is_encrypted() {
            // Must have a destination
            if self.destination.is_unknown() {
                return false;
            }

            // Must have an ephemeral public key
            if self.ephemeral_public_key.is_none() {
                return false;
            }

            // Must have a signature
            if self.message_signature.is_empty() {
                return false;
            }
        }

        true
    }
}

impl PartialEq for DhtMessageHeader {
    /// Checks equality between two `DhtMessageHeader`s disregarding the transient message_tag
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version &&
            self.destination == other.destination &&
            self.message_signature == other.message_signature &&
            self.ephemeral_public_key == other.ephemeral_public_key &&
            self.message_type == other.message_type &&
            self.flags == other.flags &&
            self.expires == other.expires
    }
}

impl Display for DhtMessageHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "DhtMessageHeader ({}, Dest:{}, Type:{:?}, Flags:{:?}, Trace:{})",
            self.version, self.destination, self.message_type, self.flags, self.message_tag
        )
    }
}

impl TryFrom<DhtHeader> for DhtMessageHeader {
    type Error = DhtMessageError;

    fn try_from(header: DhtHeader) -> Result<Self, Self::Error> {
        let destination = header
            .destination
            .and_then(|destination| destination.try_into().ok())
            .ok_or(DhtMessageError::InvalidDestination)?;

        let ephemeral_public_key = if header.ephemeral_public_key.is_empty() {
            None
        } else {
            Some(
                CommsPublicKey::from_canonical_bytes(&header.ephemeral_public_key)
                    .map_err(|_| DhtMessageError::InvalidEphemeralPublicKey)?,
            )
        };

        let expires = match header.expires {
            0 => None,
            t => Some(EpochTime::from_secs_since_epoch(t)),
        };

        let version = DhtProtocolVersion::try_from(header.major)?;

        Ok(Self {
            version,
            destination,
            message_signature: header.message_signature,
            ephemeral_public_key,
            message_type: DhtMessageType::from_i32(header.message_type).ok_or(DhtMessageError::InvalidMessageType)?,
            flags: DhtMessageFlags::from_bits(header.flags).ok_or(DhtMessageError::InvalidMessageFlags)?,
            message_tag: MessageTag::from(header.message_tag),
            expires,
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
            major: header.version.as_major(),
            ephemeral_public_key: header
                .ephemeral_public_key
                .as_ref()
                .map(ByteArray::to_vec)
                .unwrap_or_default(),
            message_signature: header.message_signature,
            destination: Some(header.destination.into()),
            message_type: header.message_type as i32,
            flags: header.flags.bits(),
            message_tag: header.message_tag.as_value(),
            expires: header.expires.map(EpochTime::as_u64).unwrap_or_default(),
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
}

/// Represents the ways a destination node can be represented.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub enum NodeDestination {
    /// The sender has chosen not to disclose the message destination, or the destination is
    /// the peer being sent to.
    #[default]
    Unknown,
    /// Destined for a particular public key
    PublicKey(Box<CommsPublicKey>),
}

impl NodeDestination {
    /// Returns the bytes of the `CommsPublicKey` or `NodeId`. Returns an empty slice if the destination is
    /// `Unknown`.
    pub fn to_inner_bytes(&self) -> [u8; 33] {
        // It is important that there is no ambiguity between fields when e.g. using bytes as part of hash pre-image
        // so each type of NodeDestination is assigned a value that differentiates them.
        let mut buf = [0u8; 33];
        match self {
            NodeDestination::Unknown => buf,
            NodeDestination::PublicKey(pk) => {
                buf[0] = 1;
                buf[1..].copy_from_slice(pk.as_bytes());
                buf
            },
        }
    }

    /// Returns a reference to the `CommsPublicKey` if the destination is `CommsPublicKey`.
    pub fn public_key(&self) -> Option<&CommsPublicKey> {
        use NodeDestination::{PublicKey, Unknown};
        match self {
            Unknown => None,
            PublicKey(pk) => Some(pk),
        }
    }

    /// Returns the NodeId for this destination, deriving it from the PublicKey if necessary or returning None if the
    /// destination is `Unknown`.
    pub fn to_derived_node_id(&self) -> Option<NodeId> {
        self.public_key().map(NodeId::from_public_key)
    }

    /// Returns true if the destination is `Unknown`, otherwise false.
    pub fn is_unknown(&self) -> bool {
        matches!(self, NodeDestination::Unknown)
    }

    /// Returns true if the NodeIdentity NodeId or PublicKey is equal to this destination.
    #[inline]
    pub fn equals_node_identity(&self, other: &NodeIdentity) -> bool {
        self == other.public_key()
    }
}

impl PartialEq<CommsPublicKey> for NodeDestination {
    fn eq(&self, other: &CommsPublicKey) -> bool {
        self.public_key().map(|pk| pk == other).unwrap_or(false)
    }
}

impl PartialEq<&CommsPublicKey> for NodeDestination {
    fn eq(&self, other: &&CommsPublicKey) -> bool {
        self.public_key().map(|pk| pk == *other).unwrap_or(false)
    }
}

impl Display for NodeDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            NodeDestination::Unknown => write!(f, "Unknown"),
            NodeDestination::PublicKey(public_key) => write!(f, "PublicKey({})", public_key),
        }
    }
}

impl TryFrom<Destination> for NodeDestination {
    type Error = ByteArrayError;

    fn try_from(destination: Destination) -> Result<Self, Self::Error> {
        match destination {
            Destination::Unknown(_) => Ok(NodeDestination::Unknown),
            Destination::PublicKey(pk) => {
                CommsPublicKey::from_canonical_bytes(&pk).map(|pk| NodeDestination::PublicKey(Box::new(pk)))
            },
        }
    }
}

impl From<CommsPublicKey> for NodeDestination {
    fn from(pk: CommsPublicKey) -> Self {
        NodeDestination::PublicKey(Box::new(pk))
    }
}

impl From<NodeDestination> for Destination {
    fn from(destination: NodeDestination) -> Self {
        use NodeDestination::{PublicKey, Unknown};
        match destination {
            Unknown => Destination::Unknown(true),
            PublicKey(pk) => Destination::PublicKey(pk.to_vec()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod node_destination {
        use rand::rngs::OsRng;
        use tari_crypto::keys::PublicKey;
        use tari_utilities::hex::{to_hex, Hex};

        use super::*;

        #[test]
        fn to_inner_bytes() {
            assert!(NodeDestination::Unknown.to_inner_bytes().iter().all(|b| *b == 0));
            let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng);
            assert!(to_hex(&NodeDestination::PublicKey(Box::new(pk.clone())).to_inner_bytes()).contains(&pk.to_hex()));
        }
    }
}
