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

use std::marker::PhantomData;

use digest::Digest;
use tari_common::DomainDigest;

use crate::{common::hash_together, BalancedBinaryMerkleTree, Hash};

#[derive(Debug)]
pub struct BalancedBinaryMerkleProof<D> {
    pub path: Vec<Hash>,
    pub leaf_index: usize,
    _phantom: PhantomData<D>,
}

impl<D> BalancedBinaryMerkleProof<D>
where D: Digest + DomainDigest
{
    pub fn verify_consume(mut self, root: &Hash, leaf_hash: Hash) -> bool {
        let mut computed_root = leaf_hash;
        for sibling in self.path.iter() {
            if self.leaf_index & 1 == 1 {
                computed_root = hash_together::<D>(&computed_root, sibling);
            } else {
                computed_root = hash_together::<D>(sibling, &computed_root);
            }
            self.leaf_index = (self.leaf_index - 1) >> 1;
        }
        &computed_root == root
    }

    pub fn generate_proof(tree: &BalancedBinaryMerkleTree<D>, leaf_index: usize) -> Self {
        let mut index = tree.get_leaf_index(leaf_index);
        let mut proof = Vec::new();
        while index > 0 {
            // Sibling
            let parent = (index - 1) >> 1;
            // The children are 2i+1 and 2i+2, so together are 4i+3, we substract one, we get the other.
            let sibling = 4 * parent + 3 - index;
            proof.push(tree.get_hash(sibling).clone());
            // Parent
            index = parent;
        }
        Self {
            path: proof,
            leaf_index: tree.get_leaf_index(leaf_index),
            _phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};

    use crate::{BalancedBinaryMerkleProof, BalancedBinaryMerkleTree};
    hash_domain!(TestDomain, "testing", 0);

    #[test]
    fn test_generate_and_verify() {
        let leaves = vec![vec![0; 32]; 3000];
        let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves);
        let root = bmt.get_merkle_root();
        let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0);
        assert!(proof.verify_consume(&root, vec![0; 32]));
    }
}
