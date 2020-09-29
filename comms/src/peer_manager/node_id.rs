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

use crate::types::CommsPublicKey;
use blake2::{
    digest::{Input, VariableOutput},
    VarBlake2b,
};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{
    cmp,
    cmp::Ordering,
    convert::{TryFrom, TryInto},
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
};
use tari_crypto::tari_utilities::{
    hex::{to_hex, Hex},
    ByteArray,
    ByteArrayError,
};
use thiserror::Error;

const NODE_ID_ARRAY_SIZE: usize = 13; // 104-bit as per RFC-0151
type NodeIdArray = [u8; NODE_ID_ARRAY_SIZE];

pub type NodeDistance = XorDistance; // or HammingDistance

#[derive(Debug, Error, Clone)]
pub enum NodeIdError {
    #[error("Incorrect byte count (expected {} bytes)", NODE_ID_ARRAY_SIZE)]
    IncorrectByteCount,
    #[error("Invalid digest output size")]
    InvalidDigestOutputSize,
}

//------------------------------------- XOR Metric -----------------------------------------------//
const NODE_XOR_DISTANCE_ARRAY_SIZE: usize = NODE_ID_ARRAY_SIZE;
type NodeXorDistanceArray = [u8; NODE_XOR_DISTANCE_ARRAY_SIZE];

#[derive(Clone, Debug, Eq, PartialOrd, Ord, Default)]
pub struct XorDistance(NodeXorDistanceArray);

impl XorDistance {
    /// Construct a new zero distance
    pub fn new() -> Self {
        Self([0; NODE_XOR_DISTANCE_ARRAY_SIZE])
    }

    /// Calculate the distance between two node ids using the Hamming distance.
    pub fn from_node_ids(x: &NodeId, y: &NodeId) -> Self {
        Self(xor(&x.0, &y.0))
    }

    /// Returns the maximum distance.
    pub const fn max_distance() -> Self {
        Self([255; NODE_XOR_DISTANCE_ARRAY_SIZE])
    }

    /// Returns a zero distance.
    pub const fn zero() -> Self {
        Self([0; NODE_XOR_DISTANCE_ARRAY_SIZE])
    }

    /// Returns the number of bytes required to represent the `XorDistance`
    pub const fn byte_length() -> usize {
        NODE_XOR_DISTANCE_ARRAY_SIZE
    }
}

impl PartialEq for XorDistance {
    fn eq(&self, nd: &XorDistance) -> bool {
        self.0 == nd.0
    }
}

impl TryFrom<&[u8]> for XorDistance {
    type Error = NodeIdError;

    /// Construct a node distance from a set of bytes
    fn try_from(elements: &[u8]) -> Result<Self, Self::Error> {
        if elements.len() >= NODE_XOR_DISTANCE_ARRAY_SIZE {
            let mut bytes = [0; NODE_XOR_DISTANCE_ARRAY_SIZE];
            bytes.copy_from_slice(&elements[0..NODE_XOR_DISTANCE_ARRAY_SIZE]);
            Ok(XorDistance(bytes))
        } else {
            Err(NodeIdError::IncorrectByteCount)
        }
    }
}

impl TryFrom<XorDistance> for u128 {
    type Error = String;

    fn try_from(value: XorDistance) -> Result<Self, Self::Error> {
        if XorDistance::byte_length() > 16 {
            return Err("XorDistance has too many bytes to be converted to U128".to_string());
        }
        let slice = value.as_bytes();
        let mut bytes: [u8; 16] = [0u8; 16];
        bytes[..XorDistance::byte_length()].copy_from_slice(&slice[..XorDistance::byte_length()]);
        Ok(u128::from_be_bytes(bytes))
    }
}

//---------------------------------- Hamming Distance --------------------------------------------//
const NODE_HAMMING_DISTANCE_ARRAY_SIZE: usize = 1;
type NodeHammingDistanceArray = [u8; NODE_HAMMING_DISTANCE_ARRAY_SIZE];

/// Hold the distance calculated between two NodeId's. This is used for DHT-style routing.
#[derive(Clone, Debug, Eq, PartialOrd, Ord, Default)]
pub struct HammingDistance(NodeHammingDistanceArray);

impl HammingDistance {
    /// Construct a new zero distance
    pub fn new() -> Self {
        Self([0; NODE_HAMMING_DISTANCE_ARRAY_SIZE])
    }

    /// Calculate the distance between two node ids using the Hamming distance.
    pub fn from_node_ids(x: &NodeId, y: &NodeId) -> Self {
        let xor_bytes = xor(&x.0, &y.0);
        Self([hamming_distance(xor_bytes)])
    }

    /// Returns the maximum distance.
    pub const fn max_distance() -> Self {
        Self([NODE_ID_ARRAY_SIZE as u8 * 8; NODE_HAMMING_DISTANCE_ARRAY_SIZE])
    }
}

impl TryFrom<&[u8]> for HammingDistance {
    type Error = NodeIdError;

    /// Construct a node distance from a set of bytes
    fn try_from(elements: &[u8]) -> Result<Self, Self::Error> {
        if elements.len() >= NODE_HAMMING_DISTANCE_ARRAY_SIZE {
            let mut bytes = [0; NODE_HAMMING_DISTANCE_ARRAY_SIZE];
            bytes.copy_from_slice(&elements[0..NODE_HAMMING_DISTANCE_ARRAY_SIZE]);
            Ok(HammingDistance(bytes))
        } else {
            Err(NodeIdError::IncorrectByteCount)
        }
    }
}

impl PartialEq for HammingDistance {
    fn eq(&self, nd: &HammingDistance) -> bool {
        self.0 == nd.0
    }
}

/// Calculate the Exclusive OR between the node_id x and y.
fn xor(x: &NodeIdArray, y: &NodeIdArray) -> NodeIdArray {
    let mut nd = [0u8; NODE_ID_ARRAY_SIZE];
    for i in 0..nd.len() {
        nd[i] = x[i] ^ y[i];
    }
    nd
}

/// Calculate the hamming distance (the number of set (1) bits of the XOR metric)
fn hamming_distance(nd: NodeIdArray) -> u8 {
    let xor_bytes = &nd;
    let mut set_bit_count = 0u8;
    for b in xor_bytes {
        let mut mask = 0b1u8;
        for _ in 0..8 {
            if b & mask > 0 {
                set_bit_count += 1;
            }
            mask <<= 1;
        }
    }

    set_bit_count
}

impl ByteArray for NodeDistance {
    /// Try and convert the given byte array to a NodeDistance. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        bytes
            .try_into()
            .map_err(|err| ByteArrayError::ConversionError(format!("{:?}", err)))
    }

    /// Return the NodeDistance as a byte array
    fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl fmt::Display for NodeDistance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", to_hex(&self.0))
    }
}

//--------------------------------------- NodeId -------------------------------------------------//

/// A Node Identity is used as a unique identifier for a node in the Tari communications network.
#[derive(Clone, Eq, Deserialize, Serialize, Default)]
pub struct NodeId(NodeIdArray);

impl NodeId {
    /// Construct a new node id on the origin
    pub fn new() -> Self {
        Self([0; NODE_ID_ARRAY_SIZE])
    }

    /// Derive a node id from a public key: node_id=hash(public_key)
    pub fn from_key<K: ByteArray>(key: &K) -> Result<Self, NodeIdError> {
        let bytes = key.as_bytes();
        let mut hasher = VarBlake2b::new(NODE_ID_ARRAY_SIZE).map_err(|_| NodeIdError::InvalidDigestOutputSize)?;
        hasher.input(bytes);
        let v = hasher.vec_result();
        Self::try_from(v.as_slice())
    }

    /// Derive a node id from a public key: node_id=hash(public_key)
    /// This function uses `NodeId::from_key` internally but is infallible because `NodeId::from_key` cannot fail when
    /// used with a `CommsPublicKey`.
    pub fn from_public_key(key: &CommsPublicKey) -> Self {
        Self::from_key(key).expect("NodeId::from_key is implemented incorrectly for CommsPublicKey")
    }

    /// Calculate the distance between the current node id and the provided node id using the XOR metric
    pub fn distance(&self, node_id: &NodeId) -> NodeDistance {
        NodeDistance::from_node_ids(&self, &node_id)
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
        let nearest_node_indices = self.closest_indices(&node_ids.to_vec(), k);
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
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        bytes
            .try_into()
            .map_err(|err| ByteArrayError::ConversionError(format!("{:?}", err)))
    }

    /// Return the NodeId as a byte array
    fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl ByteArray for Box<NodeId> {
    /// Try and convert the given byte array to a NodeId. Any failures (incorrect array length,
    /// implementation-specific checks, etc) return a [ByteArrayError](enum.ByteArrayError.html).
    fn from_bytes(bytes: &[u8]) -> Result<Self, ByteArrayError> {
        let node_id = NodeId::try_from(bytes).map_err(|err| ByteArrayError::ConversionError(format!("{:?}", err)))?;
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
        self.0.partial_cmp(&other.0)
    }
}

impl Ord for NodeId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl TryFrom<&[u8]> for NodeId {
    type Error = NodeIdError;

    /// Construct a node id from 32 bytes
    fn try_from(elements: &[u8]) -> Result<Self, Self::Error> {
        if elements.len() >= NODE_ID_ARRAY_SIZE {
            let mut bytes = [0; NODE_ID_ARRAY_SIZE];
            bytes.copy_from_slice(&elements[0..NODE_ID_ARRAY_SIZE]);
            Ok(NodeId(bytes))
        } else {
            Err(NodeIdError::IncorrectByteCount)
        }
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
    };

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
    use super::*;
    use crate::types::{CommsPublicKey, CommsSecretKey};
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        tari_utilities::byte_array::ByteArray,
    };

    #[test]
    fn display() {
        let node_id =
            NodeId::try_from(&[144u8, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53][..]).unwrap();

        let result = format!("{}", node_id);
        assert_eq!("901c6a70dcc5d87709d92a4d9f", result);
    }

    #[test]
    fn test_from_public_key() {
        let mut rng = rand::rngs::OsRng;
        let sk = CommsSecretKey::random(&mut rng);
        let pk = CommsPublicKey::from_secret_key(&sk);
        let node_id = NodeId::from_key(&pk).unwrap();
        assert_ne!(node_id.0.to_vec(), NodeId::new().0.to_vec());
        // Ensure node id is different to original public key
        let mut pk_array: [u8; 32] = [0; 32];
        pk_array.copy_from_slice(&pk.as_bytes());
        assert_ne!(node_id.0.to_vec(), pk_array.to_vec());
    }

    #[test]
    fn test_distance_and_ordering() {
        let node_id1 = NodeId::try_from(
            [
                144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 0, 157, 5, 55, 235, 247, 160,
                195, 240, 48, 146, 168, 119, 15, 241, 54,
            ]
            .as_bytes(),
        )
        .unwrap();
        let node_id2 = NodeId::try_from(
            [
                186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83, 83, 45, 185, 219, 206, 226, 128, 5, 26, 20, 0,
                192, 121, 216, 178, 134, 212, 51, 131,
            ]
            .as_bytes(),
        )
        .unwrap();
        let node_id3 = NodeId::try_from(
            [
                60, 32, 246, 39, 108, 201, 214, 91, 30, 230, 3, 126, 31, 46, 66, 203, 27, 51, 240, 177, 230, 22, 118,
                102, 201, 55, 211, 147, 229, 26, 116, 103,
            ]
            .as_bytes(),
        )
        .unwrap();
        assert!(node_id1.0 < node_id2.0);
        assert!(node_id1.0 > node_id3.0);
        // XOR metric
        let desired_n1_to_n2_dist = NodeDistance::try_from(
            [
                42, 55, 84, 126, 224, 19, 209, 195, 152, 163, 29, 237, 204, 128, 24, 118, 219, 83, 231, 183, 238, 237,
                180, 195, 48, 73, 74, 26, 241, 219, 194, 181,
            ]
            .as_bytes(),
        )
        .unwrap();
        let desired_n1_to_n3_dist = NodeDistance::try_from(
            [
                172, 60, 156, 87, 176, 12, 14, 44, 23, 63, 41, 51, 128, 253, 119, 4, 27, 174, 245, 134, 13, 225, 214,
                165, 57, 7, 65, 59, 146, 21, 133, 81,
            ]
            .as_bytes(),
        )
        .unwrap();
        // Hamming distance
        // let desired_n1_to_n2_dist_bytes: &[u8] = &vec![52u8];
        // let desired_n1_to_n2_dist = NodeDistance::try_from(desired_n1_to_n2_dist_bytes).unwrap();
        // let desired_n1_to_n3_dist = NodeDistance::try_from(
        // [
        // 46, 60, 156, 87, 176, 12, 14, 44, 23, 63, 41, 51, 128, 253, 119, 4, 27, 174, 245, 134, 13, 225, 214,
        // 165, 57, 7, 65, 59, 146, 21, 133, 81,
        // ]
        // .as_bytes(),
        // )
        // .unwrap(); // Unused bytes will be discarded
        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n1_to_n3_dist = node_id1.distance(&node_id3);
        assert!(n1_to_n2_dist < n1_to_n3_dist); // XOR metric
                                                // assert!(n1_to_n2_dist > n1_to_n3_dist); // Hamming Distance
        assert_eq!(n1_to_n2_dist, desired_n1_to_n2_dist);
        assert_eq!(n1_to_n3_dist, desired_n1_to_n3_dist);

        // Commutative
        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n2_to_n1_dist = node_id2.distance(&node_id1);

        assert_eq!(n1_to_n2_dist, n2_to_n1_dist);
    }

    #[test]
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
        // Hamming distance nearest neighbours
        // assert_eq!(knn_node_ids[0].0, [
        // 75, 146, 162, 130, 22, 63, 247, 182, 156, 103, 174, 32, 134
        // ]);
        // assert_eq!(knn_node_ids[1].0, [
        // 134, 116, 78, 53, 246, 206, 200, 147, 126, 96, 54, 113, 67
        // ]);
        // assert_eq!(knn_node_ids[2].0, [
        // 144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159
        // ]);
        assert_eq!(node_id.closest(&node_ids, node_ids.len() + 1).len(), node_ids.len());
    }

    #[test]
    fn partial_eq() {
        let bytes = [
            173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242, 24, 95, 60, 208, 53, 97, 51, 43, 71, 149, 89,
            123, 150, 162, 67, 240, 208, 67, 56,
        ]
        .as_bytes();
        let nid1 = NodeId::try_from(bytes.clone()).unwrap();
        let nid2 = NodeId::try_from(bytes.clone()).unwrap();

        assert_eq!(nid1, nid2);
    }

    #[test]
    fn hamming_distance() {
        let mut node_id1 = NodeId::default().into_inner().to_vec();
        let mut node_id2 = NodeId::default().into_inner().to_vec();
        // Same bits
        node_id1[0] = 0b00010100;
        node_id2[0] = 0b00010100;
        // Different bits
        node_id1[1] = 0b11010100;
        node_id1[12] = 0b01000011;
        node_id2[10] = 0b01000011;
        node_id2[9] = 0b11111111;
        let node_id1 = NodeId::from_bytes(node_id1.as_slice()).unwrap();
        let node_id2 = NodeId::from_bytes(node_id2.as_slice()).unwrap();

        let hamming_dist = HammingDistance::from_node_ids(&node_id1, &node_id2);
        assert_eq!(hamming_dist, HammingDistance([18]));

        let node_max = NodeId::from_bytes(&[255; NODE_ID_ARRAY_SIZE]).unwrap();
        let node_min = NodeId::default();

        let hamming_dist = HammingDistance::from_node_ids(&node_max, &node_min);
        assert_eq!(hamming_dist, HammingDistance::max_distance());
    }

    #[test]
    fn convert_xor_distance_to_u128() {
        let node_id1 = NodeId::try_from(
            [
                144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 0, 157, 5, 55, 235, 247, 160,
                195, 240, 48, 146, 168, 119, 15, 241, 54,
            ]
            .as_bytes(),
        )
        .unwrap();
        let node_id2 = NodeId::try_from(
            [
                186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83, 83, 45, 185, 219, 206, 226, 128, 5, 26, 20, 0,
                192, 121, 216, 178, 134, 212, 51, 131,
            ]
            .as_bytes(),
        )
        .unwrap();
        let node_id3 = NodeId::try_from(
            [
                60, 32, 246, 39, 108, 201, 214, 91, 30, 230, 3, 126, 31, 46, 66, 203, 27, 51, 240, 177, 230, 22, 118,
                102, 201, 55, 211, 147, 229, 26, 116, 103,
            ]
            .as_bytes(),
        )
        .unwrap();
        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n1_to_n3_dist = node_id1.distance(&node_id3);
        assert!(n1_to_n2_dist < n1_to_n3_dist);
        let n12_distance = u128::try_from(n1_to_n2_dist).unwrap();
        let n13_distance = u128::try_from(n1_to_n3_dist).unwrap();
        assert!(n12_distance < n13_distance);
        assert_eq!(n12_distance, 56114865924689668092413877285545836544);
        assert_eq!(n13_distance, 228941924089749863963604860508980641792);
    }
}
