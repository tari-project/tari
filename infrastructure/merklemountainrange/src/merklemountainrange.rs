// Copyright 2019 The Tari Project
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

use crate::merklenode::{Hashable, MerkleNode, ObjectHash};
use std::collections::HashMap;

pub struct MerkleMountainRange<T>
where T: Hashable
{
    // todo convert these to a bitmap
    mmr: Vec<MerkleNode>,
    data: HashMap<ObjectHash, T>,
}

impl<T> MerkleMountainRange<T>
where T: Hashable
{
    /// This function creates a new empty Merkle Mountain Range
    pub fn new() -> MerkleMountainRange<T> {
        MerkleMountainRange { mmr: Vec::new(), data: HashMap::new() }
    }

    /// This function returns a reference to the data stored in the mmr
    /// It will return none if the hash does not exist
    pub fn get_object(&self, hash: ObjectHash) -> Option<&T> {
        self.data.get(&hash)
    }

    /// This function returns a mut reference to the data stored in the mmr
    /// It will return none if the hash does not exist
    pub fn get_mut_object(&mut self, hash: ObjectHash) -> Option<&mut T> {
        self.data.get_mut(&hash)
    }

    /// This function returns the hash proof tree of a given hash.
    /// If the given hash is not in the tree, the vec will be empty.
    /// The Vec will be created in form of the child-child-parent
    pub fn get_hash_proof(&self, hash: ObjectHash) -> Vec<ObjectHash> {
        let mut result = Vec::new();
        let mut i = self.mmr.len();
        for counter in 0..self.mmr.len() {
            if self.mmr[counter].hash == hash {
                result.push(self.mmr[i].hash.clone());
                i = counter;
                break;
            }
        }
        i = peer_index(i as u64) as usize;
        while i < self.mmr.len() {
            result.push(self.mmr[i].hash.clone());
            i += 1;
            result.push(self.mmr[i].hash.clone());

            i = peer_index(i as u64) as usize;
        }
        result
    }

    /// This function returns the peak height of the mmr
    pub fn get_peak(&self) -> u32 {
        let mut height_counter = 0;
        let mmr_len = self.get_last_added_index() as i128;
        while (mmr_len - i128::pow(2, height_counter + 1) - 2) >= 0 {
            height_counter += 1;
        }
        height_counter
    }

    /// This function adds a vec of leaf nodes to the mmr.
    pub fn add_vec(&mut self, objects: Vec<T>) {
        for object in objects {
            self.add_single(object);
        }
    }

    /// This function adds a new leaf node to the mmr.
    pub fn add_single(&mut self, object: T) {
        let node_hash = object.get_hash();
        let node = MerkleNode::new(node_hash);
        self.mmr.push(node);
        if is_node_right(self.get_last_added_index()) {
            self.data.insert(node_hash, object);
            self.add_single_no_leaf(self.get_last_added_index())
        }
    }

    // This function adds non leaf nodes, eg nodes that are not directly a hash of data
    // This is iterative and will continue to up and till it hits the top, will be a future left child
    fn add_single_no_leaf(&mut self, index: u64) {
        let new_hash =
            self.data.get(&self.mmr[index as usize].hash).unwrap().concat(self.mmr[peer_index(index) as usize].hash);

        let new_node = MerkleNode::new(new_hash);
        self.mmr.push(new_node);
        if is_node_right(self.get_last_added_index()) {
            self.add_single_no_leaf(self.get_last_added_index())
        }
    }

    // This function is just a private function to return the index of the last added node
    fn get_last_added_index(&self) -> u64 {
        (self.mmr.len() - 1) as u64
    }
}
/// This function takes in the index and calculates the index of the peer.
pub fn peer_index(index: u64) -> u64 {
    let index_count = (1 << index + 1) - 1;
    if is_node_right(index) {
        index - index_count
    } else {
        index + index_count
    }
}

/// This function takes in the index and calculates if the node is the right child node or not.
/// If the node is the tree root it will still give the answer as if it is a child of a node.
/// This function is an iterative function as we might have to subtract the largest left_most tree.
pub fn is_node_right(index: u64) -> bool {
    let mut height_counter = 0;
    while (index as i128 - i128::pow(2, height_counter + 1) - 2) >= 0 {
        height_counter += 1;
    }
    if (index - u64::pow(2, height_counter + 1) - 2) == 0 {
        return false;
    };
    let cloned_index = index - u64::pow(2, height_counter + 1) - 1;
    if (cloned_index - u64::pow(2, height_counter + 1) - 2) == 0 {
        return true;
    };
    is_node_right(cloned_index)
}

/// This function takes in the index and calculates the height of the node
/// This function is an iterative function as we might have to subtract the largest left_most tree.
pub fn get_node_height(index: u64) -> u32 {
    let mut height_counter = 0;
    while (index as i128 - i128::pow(2, height_counter + 1) - 2) >= 0 {
        height_counter += 1;
    }
    if (index - u64::pow(2, height_counter + 1) - 2) == 0 {
        return height_counter;
    };
    let cloned_index = index - u64::pow(2, height_counter + 1) - 1;
    if (cloned_index - u64::pow(2, height_counter + 1) - 2) == 0 {
        return height_counter;
    };
    get_node_height(cloned_index)
}
