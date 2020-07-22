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

use crate::proof_of_work::monero_merkle_tree::monero_merkle_hash_util::{
    create_leaf_hash,
    create_node_hash,
    empty_hash,
};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum MoneroMerkleElement {
    Node {
        left_node: Box<MoneroMerkleElement>,
        right_node: Box<MoneroMerkleElement>,
        hash: Vec<u8>,
    },
    Leaf {
        data: Rc<Vec<u8>>,
        hash: Vec<u8>,
    },
    Empty {
        hash: Vec<u8>,
    },
}

impl MoneroMerkleElement {
    pub fn empty() -> Self {
        MoneroMerkleElement::Empty { hash: empty_hash() }
    }

    pub fn hash(&self) -> Option<&Vec<u8>> {
        match *self {
            MoneroMerkleElement::Node { ref hash, .. } |
            MoneroMerkleElement::Leaf { ref hash, .. } |
            MoneroMerkleElement::Empty { ref hash } => Some(hash),
        }
    }

    pub fn create_leaf(value: Rc<Vec<u8>>) -> MoneroMerkleElement {
        let leaf_hash = create_leaf_hash(value.as_ref());
        MoneroMerkleElement::Leaf {
            data: value,
            hash: leaf_hash,
        }
    }

    pub fn create_node(left: MoneroMerkleElement, right: MoneroMerkleElement) -> MoneroMerkleElement {
        let combined_hash = create_node_hash(left.hash().unwrap(), right.hash().unwrap());
        MoneroMerkleElement::Node {
            hash: combined_hash,
            left_node: Box::new(left),
            right_node: Box::new(right),
        }
    }
}
