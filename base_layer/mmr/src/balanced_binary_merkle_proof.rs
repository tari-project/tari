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
    pub node_index: usize,
    _phantom: PhantomData<D>,
}

// Since this is balanced tree, the index `2k+1` is always left child and `2k` is right child

impl<D> BalancedBinaryMerkleProof<D>
where D: Digest + DomainDigest
{
    pub fn verify(&self, root: &Hash, leaf_hash: Hash) -> bool {
        let mut computed_root = leaf_hash;
        let mut node_index = self.node_index;
        for sibling in &self.path {
            if node_index & 1 == 1 {
                computed_root = hash_together::<D>(&computed_root, sibling);
            } else {
                computed_root = hash_together::<D>(sibling, &computed_root);
            }
            node_index = (node_index - 1) >> 1;
        }
        &computed_root == root
    }

    pub fn generate_proof(tree: &BalancedBinaryMerkleTree<D>, leaf_index: usize) -> Self {
        let mut node_index = tree.get_node_index(leaf_index);
        let mut proof = Vec::new();
        while node_index > 0 {
            // Sibling
            let parent = (node_index - 1) >> 1;
            // The children are 2i+1 and 2i+2, so together are 4i+3, we substract one, we get the other.
            let sibling = 4 * parent + 3 - node_index;
            proof.push(tree.get_hash(sibling).clone());
            // Traverse to parent
            node_index = parent;
        }
        Self {
            path: proof,
            node_index: tree.get_node_index(leaf_index),
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
    fn test_generate_and_verify_big_tree() {
        for n in [1usize, 100, 1000, 10000] {
            let leaves = (0..n)
                .map(|i| [i.to_le_bytes().to_vec(), vec![0u8; 24]].concat())
                .collect::<Vec<_>>();
            let hash_0 = leaves[0].clone();
            let hash_n_half = leaves[n / 2].clone();
            let hash_last = leaves[n - 1].clone();
            let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves);
            let root = bmt.get_merkle_root();
            let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0);
            assert!(proof.verify(&root, hash_0));
            let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, n / 2);
            assert!(proof.verify(&root, hash_n_half));
            let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, n - 1);
            assert!(proof.verify(&root, hash_last));
        }
    }
}
