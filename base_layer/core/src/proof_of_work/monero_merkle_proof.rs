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

use crate::proof_of_work::{
    monero_merkle_hash_util::{create_leaf_hash, create_node_hash},
    monero_merkle_tree::MoneroMerkleProofNode,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoneroMerkleProof {
    root_hash: Vec<u8>,
    value: Vec<u8>,
    path: Vec<MoneroMerkleProofNode>,
}

impl MoneroMerkleProof {
    pub fn new(root_hash: Vec<u8>, value: Vec<u8>, path: Vec<MoneroMerkleProofNode>) -> Self {
        MoneroMerkleProof { root_hash, value, path }
    }

    pub fn validate(&self, root_hash: &Vec<u8>) -> bool {
        let mut hash = create_leaf_hash(&self.value);

        for node in &self.path {
            hash = match node {
                &MoneroMerkleProofNode::Left(ref proof_hash) => create_node_hash(proof_hash, &hash),
                &MoneroMerkleProofNode::Right(ref proof_hash) => create_node_hash(&hash, proof_hash),
            };
        }

        &hash == root_hash
    }

    pub fn validate_value(&self, tx_hash: &Vec<u8>) -> bool {
        if &self.value == tx_hash {
            return true;
        }
        false
    }
}
