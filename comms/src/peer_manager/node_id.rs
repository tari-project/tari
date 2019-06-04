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

use std::fmt;

use derive_error::Error;
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    hash::{Hash, Hasher},
};
use tari_utilities::Hashable;

use tari_utilities::hex::to_hex;

const NODE_ID_ARRAY_SIZE: usize = 32;
type NodeIdArray = [u8; NODE_ID_ARRAY_SIZE];

#[derive(Debug, Error)]
pub enum NodeIdError {
    IncorrectByteCount,
    OutOfBounds,
}

#[derive(Clone, Debug, Eq, PartialOrd, Ord)]
pub struct NodeDistance(NodeIdArray);

impl NodeDistance {
    /// Construct a new zero distance
    pub fn new() -> NodeDistance {
        NodeDistance([0; NODE_ID_ARRAY_SIZE])
    }

    /// Calculate the distance between two node ids using the XOR metric
    pub fn from_node_ids(x: &NodeId, y: &NodeId) -> NodeDistance {
        let mut nd = NodeDistance::new();
        for i in 0..nd.0.len() {
            nd.0[i] = x.0[i] ^ y.0[i];
        }
        nd
    }
}

impl PartialEq for NodeDistance {
    fn eq(&self, nd: &NodeDistance) -> bool {
        self.0 == nd.0
    }
}

impl TryFrom<&[u8]> for NodeDistance {
    type Error = NodeIdError;

    /// Construct a node distance from 32 bytes
    fn try_from(elements: &[u8]) -> Result<Self, Self::Error> {
        if elements.len() >= NODE_ID_ARRAY_SIZE {
            let mut bytes = [0; NODE_ID_ARRAY_SIZE];
            bytes.copy_from_slice(&elements[0..NODE_ID_ARRAY_SIZE]);
            Ok(NodeDistance(bytes))
        } else {
            Err(NodeIdError::IncorrectByteCount)
        }
    }
}

#[derive(Clone, Debug, Eq, Deserialize, Serialize)]
pub struct NodeId(NodeIdArray);

impl NodeId {
    /// Construct a new node id on the origin
    pub fn new() -> Self {
        Self([0; NODE_ID_ARRAY_SIZE])
    }

    /// Derive a node id from a public key: node_id=hash(public_key)
    pub fn from_key<K: Hashable>(key: &K) -> Result<Self, NodeIdError> {
        Self::try_from(key.hash().as_slice())
    }

    /// Generate a node id from a base layer registration using the block hash and public key
    // pub fn from_baselayer_registration<?>(......) -> NodeId {
    // TODO: NodeId=hash(blockhash(with block_height),public key?)
    // }

    /// Calculate the distance between the current node id and the provided node id using the XOR metric
    pub fn distance(&self, node_id: &NodeId) -> NodeDistance {
        NodeDistance::from_node_ids(&self, &node_id)
    }

    /// Find and return the indices of the K nearest neighbours from the provided node id list
    pub fn closest_indices(&self, node_ids: &Vec<NodeId>, k: usize) -> Result<Vec<usize>, NodeIdError> {
        if k > node_ids.len() {
            return Err(NodeIdError::OutOfBounds);
        }
        let mut indices: Vec<usize> = Vec::with_capacity(node_ids.len());
        let mut dists: Vec<NodeDistance> = Vec::with_capacity(node_ids.len());
        for i in 0..node_ids.len() {
            indices.push(i);
            dists.push(self.distance(&node_ids[i]));
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
        Ok(nearest_node_indices)
    }

    /// Find and return the node ids of the K nearest neighbours from the provided node id list
    pub fn closest(&self, node_ids: &Vec<NodeId>, k: usize) -> Result<Vec<NodeId>, NodeIdError> {
        let nearest_node_indices = self.closest_indices(&node_ids, k)?;
        let mut nearest_node_ids: Vec<NodeId> = Vec::with_capacity(nearest_node_indices.len());
        for nearest in nearest_node_indices {
            nearest_node_ids.push(node_ids[nearest].clone());
        }
        Ok(nearest_node_ids)
    }
}

impl PartialEq for NodeId {
    fn eq(&self, nid: &NodeId) -> bool {
        self.0 == nid.0
    }
}

impl TryFrom<&[u8]> for NodeId {
    type Error = NodeIdError;

    /// Construct a node id from 32 bytes
    fn try_from(elements: &[u8]) -> Result<Self, Self::Error> {
        if elements.len() >= 32 {
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

#[cfg(test)]
mod test {
    use super::*;
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };
    use tari_utilities::byte_array::ByteArray;

    #[test]
    fn display() {
        let node_id = NodeId::try_from(
            [
                144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 0, 157, 5, 55, 235, 247, 160,
                195, 240, 48, 146, 168, 119, 15, 241, 54,
            ]
            .as_bytes(),
        )
        .unwrap();

        let result = format!("{}", node_id);
        assert_eq!(
            "901c6a70dcc5d87709d92a4d9fd335cf009d0537ebf7a0c3f03092a8770ff136",
            result
        );
    }

    #[test]
    fn test_from_public_key() {
        let mut rng = rand::OsRng::new().unwrap();
        let sk = RistrettoSecretKey::random(&mut rng);
        let pk = RistrettoPublicKey::from_secret_key(&sk);
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
        let n1_to_n2_dist = node_id1.distance(&node_id2);
        let n1_to_n3_dist = node_id1.distance(&node_id3);
        assert!(n1_to_n2_dist < n1_to_n3_dist);
        assert_eq!(n1_to_n2_dist, desired_n1_to_n2_dist);
        assert_eq!(n1_to_n3_dist, desired_n1_to_n3_dist);
    }

    #[test]
    fn test_closest() {
        let mut node_ids: Vec<NodeId> = Vec::new();
        node_ids.push(
            NodeId::try_from(
                [
                    144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 245, 157, 5, 55, 235,
                    247, 160, 195, 240, 48, 146, 168, 119, 15, 241, 54,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    75, 249, 102, 1, 2, 166, 155, 37, 22, 54, 84, 98, 56, 62, 242, 115, 238, 149, 12, 239, 231, 217,
                    35, 168, 106, 203, 199, 168, 147, 32, 234, 38,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    60, 32, 246, 39, 108, 201, 214, 91, 30, 230, 3, 126, 31, 46, 66, 203, 27, 51, 240, 177, 230, 22,
                    118, 102, 201, 55, 211, 147, 229, 26, 116, 103,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    134, 116, 78, 53, 246, 206, 200, 147, 126, 96, 54, 113, 67, 56, 173, 52, 150, 35, 250, 18, 29, 87,
                    231, 228, 125, 49, 95, 53, 103, 250, 54, 214,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    75, 146, 162, 130, 22, 63, 247, 182, 156, 103, 174, 32, 134, 97, 41, 240, 180, 116, 2, 142, 53,
                    197, 209, 113, 191, 205, 45, 151, 93, 167, 43, 72,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83, 83, 45, 185, 219, 206, 226, 128, 5, 26,
                    20, 0, 192, 121, 216, 178, 134, 212, 51, 131,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    143, 189, 32, 210, 30, 231, 82, 5, 86, 85, 28, 82, 154, 127, 90, 98, 108, 106, 186, 179, 36, 194,
                    246, 209, 17, 244, 126, 108, 104, 187, 204, 213,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    155, 210, 214, 160, 153, 70, 172, 234, 177, 178, 62, 82, 166, 202, 71, 205, 139, 247, 170, 91, 234,
                    197, 239, 27, 14, 238, 97, 8, 28, 169, 96, 169,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );
        node_ids.push(
            NodeId::try_from(
                [
                    173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242, 24, 95, 60, 208, 53, 97, 51, 43, 71,
                    149, 89, 123, 150, 162, 67, 240, 208, 67, 56,
                ]
                .as_bytes(),
            )
            .unwrap(),
        );

        let node_id = NodeId::try_from(
            [
                169, 125, 200, 137, 210, 73, 241, 238, 25, 108, 8, 48, 66, 29, 2, 117, 1, 252, 36, 214, 252, 38, 207,
                113, 175, 126, 36, 202, 215, 125, 114, 131,
            ]
            .as_bytes(),
        )
        .unwrap();
        let k = 3;
        match node_id.closest(&node_ids, k) {
            Ok(knn_node_ids) => {
                println!(" KNN = {:?}", knn_node_ids);
                assert_eq!(knn_node_ids.len(), k);
                assert_eq!(knn_node_ids[0].0, [
                    173, 218, 34, 188, 211, 173, 235, 82, 18, 159, 55, 47, 242, 24, 95, 60, 208, 53, 97, 51, 43, 71,
                    149, 89, 123, 150, 162, 67, 240, 208, 67, 56
                ]);
                assert_eq!(knn_node_ids[1].0, [
                    186, 43, 62, 14, 60, 214, 9, 180, 145, 122, 55, 160, 83, 83, 45, 185, 219, 206, 226, 128, 5, 26,
                    20, 0, 192, 121, 216, 178, 134, 212, 51, 131
                ]);
                assert_eq!(knn_node_ids[2].0, [
                    143, 189, 32, 210, 30, 231, 82, 5, 86, 85, 28, 82, 154, 127, 90, 98, 108, 106, 186, 179, 36, 194,
                    246, 209, 17, 244, 126, 108, 104, 187, 204, 213
                ]);
            },
            Err(_e) => assert!(false),
        };
        assert!(node_id.closest(&node_ids, node_ids.len() + 1).is_err());
    }
}
