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
    cmp,
    cmp::Ordering,
    convert::{TryFrom, TryInto},
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::BitXor,
};

use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use serde::{de, Deserialize, Deserializer, Serialize};
use tari_utilities::{
    hex::{to_hex, Hex},
    ByteArray,
    ByteArrayError,
};
use thiserror::Error;

use crate::{peer_manager::node_distance::NodeDistance, types::CommsPublicKey};

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

    /// Calculate the distance between the current node id and the provided node id using the XOR metric
    pub fn distance(&self, node_id: &NodeId) -> NodeDistance {
        NodeDistance::from_node_ids(self, node_id)
    }

    /// Find and return the indices of the K nearest neighbours from the provided node id list
    pub fn closest_indices(&self, node_ids: &[NodeId], k: usize) -> Vec<usize> {
        let k = cmp::min(k, node_ids.len());
        let mut indices: Vec<usize> = Vec::with_capacity(node_ids.len());
        let mut dists: Vec<NodeDistance> = Vec::with_capacity(node_ids.len());
        for (i, node_id) in node_ids.iter().enumerate() {
            indices.push(i);
            dists.push(self.distance(node_id))
        }
        // Perform partial sort of elements only up to K elements
        let mut nearest_node_indices: Vec<usize> = Vec::with_capacity(k);
        for i in 0..k {
            for j in i + 1..node_ids.len() {
                if dists[i] > dists[j] {
                    dists.swap(i, j);
                    indices.swap(i, j);
                }
            }
            nearest_node_indices.push(indices[i]);
        }
        nearest_node_indices
    }

    /// Find and return the node ids of the K nearest neighbours from the provided node id list
    pub fn closest(&self, node_ids: &[NodeId], k: usize) -> Vec<NodeId> {
        let nearest_node_indices = self.closest_indices(node_ids, k);
        let mut nearest_node_ids: Vec<NodeId> = Vec::with_capacity(nearest_node_indices.len());
        for nearest in nearest_node_indices {
            nearest_node_ids.push(node_ids[nearest].clone());
        }
        nearest_node_ids
    }

    pub fn into_inner(self) -> NodeIdArray {
        self.0
    }

    pub fn short_str(&self) -> String {
        to_hex(&self.0[..8])
    }
}

impl ByteArray for NodeId {
    /// Try and convert the given byte array to a NodeId. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        bytes.try_into().map_err(|err| ByteArrayError::ConversionError {
            reason: format!("{:?}", err),
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
            reason: format!("{:?}", err),
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
            xor[i] = self.0[i] ^ rhs.0[i];
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

    impl<'de> de::Visitor<'de> for KeyStringVisitor<NodeId> {
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
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        tari_utilities::byte_array::ByteArray,
    };

    use super::*;
    use crate::types::{CommsPublicKey, CommsSecretKey};

    #[test]
    fn display() {
        let node_id = NodeId::try_from(&[144u8, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159][..]).unwrap();

        let result = format!("{}", node_id);
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
    fn test_distance_and_ordering() {
        let node_id1 = NodeId::try_from(&[144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159][..]).unwrap();
        let node_id2 = NodeId::try_from(&[186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83][..]).unwrap();
        let node_id3 = NodeId::try_from(&[60, 32, 246, 39, 108, 201, 214, 91, 30, 230, 3, 126, 31][..]).unwrap();
        assert!(node_id1.0 < node_id2.0);
        assert!(node_id1.0 > node_id3.0);
        // XOR metric
        let desired_n1_to_n2_dist =
            NodeDistance::try_from(&[42, 55, 84, 126, 224, 19, 209, 195, 152, 163, 29, 237, 204][..]).unwrap();
        let desired_n1_to_n3_dist =
            NodeDistance::try_from(&[172, 60, 156, 87, 176, 12, 14, 44, 23, 63, 41, 51, 128][..]).unwrap();

        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n1_to_n3_dist = node_id1.distance(&node_id3);
        // Big-endian ordering
        assert!(n1_to_n2_dist < n1_to_n3_dist);
        assert_eq!(n1_to_n2_dist, desired_n1_to_n2_dist);
        assert_eq!(n1_to_n3_dist, desired_n1_to_n3_dist);

        // Commutative
        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n2_to_n1_dist = node_id2.distance(&node_id1);

        assert_eq!(n1_to_n2_dist, n2_to_n1_dist);
    }

    #[test]
    #[allow(clippy::vec_init_then_push)]
    fn test_closest() {
        let mut node_ids: Vec<NodeId> = Vec::new();
        node_ids.push(NodeId::try_from(&[144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159][..]).unwrap());
        node_ids.push(NodeId::try_from(&[75, 249, 102, 1, 2, 166, 155, 37, 22, 54, 84, 98, 56][..]).unwrap());
        node_ids.push(NodeId::try_from(&[60, 32, 246, 39, 108, 201, 214, 91, 30, 230, 3, 126, 31][..]).unwrap());
        node_ids.push(NodeId::try_from(&[134, 116, 78, 53, 246, 206, 200, 147, 126, 96, 54, 113, 67][..]).unwrap());
        node_ids.push(NodeId::try_from(&[75, 146, 162, 130, 22, 63, 247, 182, 156, 103, 174, 32, 134][..]).unwrap());
        node_ids.push(NodeId::try_from(&[186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83][..]).unwrap());
        node_ids.push(NodeId::try_from(&[143, 189, 32, 210, 30, 231, 82, 5, 86, 85, 28, 82, 154][..]).unwrap());
        node_ids.push(NodeId::try_from(&[155, 210, 214, 160, 153, 70, 172, 234, 177, 178, 62, 82, 166][..]).unwrap());
        node_ids.push(NodeId::try_from(&[173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242][..]).unwrap());

        let node_id = NodeId::try_from(&[169, 125, 200, 137, 210, 73, 241, 238, 25, 108, 8, 48, 66][..]).unwrap();

        let k = 3;
        let knn_node_ids = node_id.closest(&node_ids, k);
        assert_eq!(knn_node_ids.len(), k);
        // XOR metric nearest neighbours
        assert_eq!(knn_node_ids[0].0, [
            173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242
        ]);
        assert_eq!(knn_node_ids[1].0, [
            186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83
        ]);
        assert_eq!(knn_node_ids[2].0, [
            143, 189, 32, 210, 30, 231, 82, 5, 86, 85, 28, 82, 154
        ]);

        assert_eq!(node_id.closest(&node_ids, node_ids.len() + 1).len(), node_ids.len());
    }

    #[test]
    fn partial_eq() {
        let bytes = &[173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242][..];
        let nid1 = NodeId::try_from(bytes).unwrap();
        let nid2 = NodeId::try_from(bytes).unwrap();

        assert_eq!(nid1, nid2);
    }

    #[test]
    fn convert_xor_distance_to_u128() {
        let node_id1 = NodeId::try_from(&[128, 28, 106, 112, 220, 197, 216, 119, 9, 128, 42, 77, 55][..]).unwrap();
        let node_id2 = NodeId::try_from(&[160, 28, 106, 112, 220, 197, 216, 119, 9, 128, 42, 77, 54][..]).unwrap();
        let node_id3 = NodeId::try_from(&[64, 28, 106, 112, 220, 197, 216, 119, 9, 128, 42, 77, 54][..]).unwrap();
        let n12_distance = node_id1.distance(&node_id2);
        let n13_distance = node_id1.distance(&node_id3);
        assert_eq!(n12_distance.to_bytes()[..4], [0, 0, 0, 32]);
        assert_eq!(n13_distance.to_bytes()[..4], [0, 0, 0, 192]);
        assert!(n12_distance < n13_distance);
        assert_eq!(n12_distance.as_u128(), ((128 ^ 160) << (12 * 8)) + 1);
        assert_eq!(n13_distance.as_u128(), ((128 ^ 64) << (12 * 8)) + 1);
    }
}
