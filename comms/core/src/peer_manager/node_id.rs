//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    cmp::Ordering,
    convert::{TryFrom, TryInto},
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::BitXor,
};

use blake2::{
    Blake2bVar,
    digest::{Update, VariableOutput},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use tari_utilities::{
    ByteArray,
    ByteArrayError,
    hex::{Hex, to_hex},
};
use thiserror::Error;

use crate::types::CommsPublicKey;

pub(super) type NodeIdArray = [u8; NodeId::byte_size()];

/// Error type for NodeId
#[derive(Debug, Error, Clone)]
pub enum NodeIdError {
    #[error("Incorrect byte count (expected {} bytes)", NodeId::byte_size())]
    IncorrectByteCount,
    #[error("Invalid digest output size")]
    InvalidDigestOutputSize,
}

/// A Node Identity is used as a unique identifier for a node in the Tari communications network.
#[derive(Clone, Eq, Deserialize, Serialize, Default)]
pub struct NodeId(NodeIdArray);

impl NodeId {
    /// Construct a new node id on the origin
    pub fn new() -> Self {
        Default::default()
    }

    /// 104-bit/13 byte as per RFC-0151
    pub const fn byte_size() -> usize {
        13
    }

    /// Derive a node id from a public key: node_id=hash(public_key)
    pub fn from_key<K: ByteArray>(key: &K) -> Self {
        let bytes = key.as_bytes();
        let mut buf = [0u8; NodeId::byte_size()];
        Blake2bVar::new(NodeId::byte_size())
            .expect("NodeId::byte_size() is invalid")
            .chain(bytes)
            // Safety: output size and buf size are equal
            .finalize_variable(&mut buf).unwrap();
        NodeId(buf)
    }

    /// Derive a node id from a public key: node_id = hash(public_key)
    pub fn from_public_key(key: &CommsPublicKey) -> Self {
        Self::from_key(key)
    }

    pub fn into_inner(self) -> NodeIdArray {
        self.0
    }

    pub fn short_str(&self) -> String {
        to_hex(self.0.get(..8).expect("Index should exist"))
    }
}

impl ByteArray for NodeId {
    /// Try and convert the given byte array to a NodeId. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        bytes.try_into().map_err(|err| ByteArrayError::ConversionError {
            reason: format!("{err:?}"),
        })
    }

    /// Return the NodeId as a byte array
    fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl ByteArray for Box<NodeId> {
    /// Try and convert the given byte array to a NodeId. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        let node_id = NodeId::try_from(bytes).map_err(|err| ByteArrayError::ConversionError {
            reason: format!("{err:?}"),
        })?;
        Ok(Box::new(node_id))
    }

    /// Return the NodeId as a byte array
    fn as_bytes(&self) -> &[u8] {
        &self.as_ref().0
    }
}

impl PartialEq for NodeId {
    fn eq(&self, nid: &NodeId) -> bool {
        self.0 == nid.0
    }
}

impl PartialOrd<NodeId> for NodeId {
    fn partial_cmp(&self, other: &NodeId) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NodeId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl BitXor for &NodeId {
    type Output = NodeIdArray;

    fn bitxor(self, rhs: Self) -> Self::Output {
        let mut xor = [0u8; NodeId::byte_size()];
        #[allow(clippy::needless_range_loop)]
        for i in 0..NodeId::byte_size() {
            *xor.get_mut(i).expect("Index should exist") =
                self.0.get(i).expect("Index should exist") ^ rhs.0.get(i).expect("Index should exist");
        }
        xor
    }
}

impl TryFrom<&[u8]> for NodeId {
    type Error = NodeIdError;

    /// Construct a node id from 32 bytes
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != NodeId::byte_size() {
            return Err(NodeIdError::IncorrectByteCount);
        }

        let mut buf = [0; NodeId::byte_size()];
        buf.copy_from_slice(bytes);
        Ok(NodeId(buf))
    }
}

impl From<CommsPublicKey> for NodeId {
    fn from(pk: CommsPublicKey) -> Self {
        NodeId::from_public_key(&pk)
    }
}

impl Hash for NodeId {
    /// Require the implementation of the Hash trait for Hashmaps
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl AsRef<[u8]> for NodeId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", to_hex(&self.0))
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NodeId({})", to_hex(&self.0))
    }
}

pub fn deserialize_node_id_from_hex<'de, D>(des: D) -> Result<NodeId, D::Error>
where D: Deserializer<'de> {
    struct KeyStringVisitor<K> {
        marker: PhantomData<K>,
    }

    impl de::Visitor<'_> for KeyStringVisitor<NodeId> {
        type Value = NodeId;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a node id in hex format")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where E: de::Error {
            NodeId::from_hex(v).map_err(E::custom)
        }
    }
    des.deserialize_str(KeyStringVisitor { marker: PhantomData })
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tari_crypto::keys::SecretKey;

    use super::*;
    use crate::types::CommsSecretKey;

    #[test]
    fn display() {
        let node_id = NodeId::try_from(&[144u8, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159][..]).unwrap();

        let result = format!("{node_id}");
        assert_eq!("901c6a70dcc5d87709d92a4d9f", result);
    }

    #[test]
    fn test_from_public_key() {
        let mut rng = rand::rngs::OsRng;
        let sk = CommsSecretKey::random(&mut rng);
        let pk = CommsPublicKey::from_secret_key(&sk);
        let node_id = NodeId::from_key(&pk);
        assert_ne!(node_id.0.to_vec(), NodeId::new().0.to_vec());
        // Ensure node id is different to original public key
        let mut pk_array: [u8; 32] = [0; 32];
        pk_array.copy_from_slice(pk.as_bytes());
        assert_ne!(node_id.0.to_vec(), pk_array.to_vec());
    }

    #[test]
    fn partial_eq() {
        let bytes = &[173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242][..];
        let nid1 = NodeId::try_from(bytes).unwrap();
        let nid2 = NodeId::try_from(bytes).unwrap();

        assert_eq!(nid1, nid2);
    }
}
