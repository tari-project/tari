// Copyright 2019. The Tari Project
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

use crate::proof_of_work::monero_merkle_tree::{
    monero_merkle_element::MoneroMerkleElement,
    monero_merkle_hash_util::{create_leaf_hash, create_node_hash},
    monero_merkle_proof::MoneroMerkleProof,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{vec_deque::Iter, BTreeMap, VecDeque},
    rc::Rc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoneroMerkleProofNode {
    Left(Vec<u8>),
    Right(Vec<u8>),
}

/// MerkleTree struct represents merkle binary tree with values of type Vec<u8> and map of nodes.
#[derive(Debug, Clone)]
pub struct MoneroMerkleTree {
    root: MoneroMerkleElement,
    height: usize,
    count: usize,
    storage: VecDeque<Rc<Vec<u8>>>,
    nodes: BTreeMap<usize, VecDeque<MoneroMerkleElement>>,
}

impl MoneroMerkleTree {
    /// Creates new, empty `MerkleTree`.
    pub fn new() -> Self {
        MoneroMerkleTree {
            root: MoneroMerkleElement::empty(),
            height: 0,
            count: 0,
            storage: VecDeque::new(),
            nodes: BTreeMap::new(),
        }
    }

    /// Creates `MerkleTree` from `Vec` of u8.
    pub fn from_vec(data: Vec<Vec<u8>>) -> Self {
        if data.is_empty() {
            Self::new()
        } else {
            let elements = data.into_iter().map(|e| Rc::new(e)).collect::<VecDeque<Rc<Vec<u8>>>>();
            let mut result = MoneroMerkleTree {
                root: MoneroMerkleElement::empty(),
                height: 0,
                count: 0,
                storage: elements,
                nodes: BTreeMap::new(),
            };
            result.calculate_tree();
            result
        }
    }

    /// Push element into the end of the tree.
    pub fn push(&mut self, value: Vec<u8>) {
        self.storage.push_back(Rc::new(value));
        self.count = self.storage.len();
        self.calculate_tree();
    }

    /// Removes element from the tree and returns `true` if element was removed
    /// successfully and `false` if `index` out of bounds.
    pub fn remove(&mut self, index: usize) -> bool {
        if let Some(_) = self.storage.remove(index) {
            self.count = self.storage.len();
            self.calculate_tree();
            true
        } else {
            false
        }
    }

    /// Retrieves an element in the `MerkleTree` by index.
    pub fn get(&self, index: usize) -> Option<&Vec<u8>> {
        if let Some(v) = self.storage.get(index) {
            Some(v.as_ref())
        } else {
            None
        }
    }

    /// Retrieves copies of all elements in the `MerkleTree`.
    pub fn get_values(&self) -> Option<Vec<Vec<u8>>> {
        if self.storage.is_empty() {
            None
        } else {
            let values = self
                .storage
                .iter()
                .map(|v| v.as_ref().clone())
                .collect::<Vec<Vec<u8>>>();
            Some(values)
        }
    }

    /// Returns the number of elements in the three
    pub fn len(&self) -> usize {
        self.count
    }

    /// Returns the height of the three
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns `true` if the `MerkleTree` is empty.
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    /// Returns root hash of `MerkleTree`
    pub fn root_hash(&self) -> Option<&Vec<u8>> {
        self.root.hash()
    }

    /// Returns a front-to-back iterator.
    pub fn iter(&self) -> Iter<Rc<Vec<u8>>> {
        self.storage.iter()
    }

    /// Returns the proof for checking if `value` really in tree.
    pub fn get_proof(&self, value: Vec<u8>) -> MoneroMerkleProof {
        let path = self.get_needed_hashes_for_proof(&value);
        MoneroMerkleProof::new(self.root_hash().unwrap().clone(), value.clone(), path)
    }

    fn calculate_tree(&mut self) {
        self.count = self.storage.len();
        self.height = calculate_height(self.count);
        self.root = MoneroMerkleElement::empty();
        self.nodes.clear();
        let mut current_level = self.height;

        if !self.storage.is_empty() {
            let mut leaves = VecDeque::new();
            for value in &self.storage {
                let e = MoneroMerkleElement::create_leaf(value.clone());
                leaves.push_back(e);
            }

            self.nodes.insert(current_level, leaves);

            while current_level > 0 {
                let above_level = current_level - 1;
                let above_row = {
                    let mut row = VecDeque::new();
                    let current_row = self.nodes.get(&current_level).unwrap();
                    for i in (0..current_row.len()).step_by(2) {
                        let left = current_row.get(i).unwrap();
                        let right = current_row.get(i + 1).unwrap_or(left);
                        let node = MoneroMerkleElement::create_node(left.clone(), right.clone());
                        row.push_back(node);
                    }
                    row
                };

                self.nodes.insert(above_level, above_row);
                current_level -= 1;
            }
            assert_eq!(current_level, 0);
            self.root = self.nodes.get(&0).unwrap()[0].clone(); // root_node;
        }
    }

    fn get_needed_hashes_for_proof(&self, value: &Vec<u8>) -> Vec<MoneroMerkleProofNode> {
        let mut level = self.height;
        let mut next_hash = create_leaf_hash(&value);
        let mut needed_hashes = Vec::new();

        while level > 0 {
            if let Some(index) = self.get_element_index(level, &next_hash) {
                let nodes = self.nodes.get(&level).unwrap();
                match nodes.get(index) {
                    Some(&MoneroMerkleElement::Leaf { ref hash, .. }) |
                    Some(&MoneroMerkleElement::Node { ref hash, .. }) => {
                        if index % 2 == 0 {
                            if let Some(sibling_node) = nodes.get(index + 1) {
                                needed_hashes.push(MoneroMerkleProofNode::Right(sibling_node.hash().unwrap().clone()));
                                next_hash = create_node_hash(hash, sibling_node.hash().unwrap());
                            } else {
                                needed_hashes.push(MoneroMerkleProofNode::Right(hash.clone()));
                                next_hash = create_node_hash(hash, hash);
                            }
                        } else {
                            if let Some(sibling_node) = nodes.get(index - 1) {
                                needed_hashes.push(MoneroMerkleProofNode::Left(sibling_node.hash().unwrap().clone()));
                                next_hash = create_node_hash(sibling_node.hash().unwrap(), hash);
                            }
                        }
                    },
                    _ => continue,
                };
            }
            level -= 1;
        }
        needed_hashes
    }

    fn get_element_index(&self, level: usize, hash: &Vec<u8>) -> Option<usize> {
        let row_hashes = self
            .nodes
            .get(&level)
            .unwrap()
            .iter()
            .map(|e| e.hash().unwrap())
            .collect::<Vec<&Vec<u8>>>();
        row_hashes.iter().position(|&s| s == hash)
    }
}

pub fn calculate_height(count: usize) -> usize {
    if count > 0 {
        let height = (count as f64).log2();
        if height - height.floor() > 0.0 {
            (height + 1.0) as usize
        } else {
            height as usize
        }
    } else {
        0
    }
}
