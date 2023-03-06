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

use std::{collections::HashMap, convert::TryInto, marker::PhantomData};

use digest::Digest;
use tari_common::DomainDigest;
use tari_utilities::ByteArray;
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum MergedBalancedBinaryMerkleProofError {
    #[error("Can't merge zero proofs.")]
    CantMergeZeroProofs,
}

#[derive(Debug)]
pub struct MergedBalancedBinaryMerkleProof<D> {
    pub paths: Vec<Vec<Vec<u8>>>,
    pub leaves_indices: Vec<usize>,
    pub join_indices: Vec<Option<usize>>,
    pub heights: Vec<usize>,
    _phantom: PhantomData<D>,
}

impl<D> MergedBalancedBinaryMerkleProof<D>
where D: Digest + DomainDigest
{
    pub fn create_from_proofs(
        proofs: Vec<BalancedBinaryMerkleProof<D>>,
    ) -> Result<Self, MergedBalancedBinaryMerkleProofError> {
        let heights = proofs.iter().map(|proof| proof.path.len()).collect::<Vec<_>>();
        let max_height = heights
            .iter()
            .max()
            .ok_or(MergedBalancedBinaryMerkleProofError::CantMergeZeroProofs)?;
        let mut indices = proofs.iter().map(|proof| proof.node_index).collect::<Vec<_>>();
        let mut paths = vec![Vec::new(); proofs.len()];
        let mut join_indices = vec![None; proofs.len()];
        for height in (0..*max_height).rev() {
            let mut hash_map = HashMap::new();
            for (index, proof) in proofs.iter().enumerate() {
                // If this path was already joined ignore it.
                if join_indices[index].is_none() && proof.path.len() > height {
                    let parent = (indices[index] - 1) >> 1;
                    if let Some(sibling) = hash_map.insert(parent, index) {
                        join_indices[index] = Some(sibling);
                        // The sibling doesn't need a hash, it needs an index to this.
                        *paths[sibling].first_mut().unwrap() = index.to_le_bytes().to_vec();
                    } else {
                        paths[index].insert(0, proof.path[proof.path.len() - 1 - height].clone());
                    }
                    indices[index] = parent;
                }
            }
        }
        Ok(Self {
            paths,
            leaves_indices: proofs.iter().map(|proof| proof.node_index).collect::<Vec<_>>(),
            heights,
            join_indices,
            _phantom: PhantomData,
        })
    }

    pub fn verify_consume(mut self, root: &Hash, leaves_hashes: Vec<Hash>) -> bool {
        let mut computed_hashes = leaves_hashes;
        let max_height = self.heights.iter().max().unwrap();
        let length = computed_hashes.len();
        // We need to compute the hashes row by row to be sure they are processed correctly.
        for height in (0..*max_height).rev() {
            let hashes = computed_hashes.clone();
            for (leaf, index) in computed_hashes.iter_mut().zip(0..length) {
                if self.heights[index] > height {
                    if let Some(hash_or_index) = self.paths[index].pop() {
                        let hash = match hash_or_index.len() {
                            8 => {
                                let index = usize::from_le_bytes(hash_or_index.as_bytes().try_into().unwrap());
                                &hashes[index]
                            },
                            32 => &hash_or_index,
                            _ => panic!("It should be either index (size 8) or hash (size 32)"),
                        };
                        let parent = (self.leaves_indices[index] - 1) >> 1;
                        if self.leaves_indices[index] & 1 == 1 {
                            *leaf = hash_together::<D>(leaf, hash);
                        } else {
                            *leaf = hash_together::<D>(hash, leaf);
                        }
                        self.leaves_indices[index] = parent;
                    }
                }
            }
        }
        &computed_hashes[0] == root
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};

    use super::MergedBalancedBinaryMerkleProof;
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

    #[test]
    fn test_merge_proof() {
        let leaves = (0..255).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves.clone());
        let indices = [50, 0, 200, 150, 100];
        let root = bmt.get_merkle_root();
        let proofs = indices
            .iter()
            .map(|i| BalancedBinaryMerkleProof::generate_proof(&bmt, *i))
            .collect::<Vec<_>>();
        let merged_proof = MergedBalancedBinaryMerkleProof::create_from_proofs(proofs).unwrap();
        assert!(merged_proof.verify_consume(&root, indices.iter().map(|i| leaves[*i].clone()).collect::<Vec<_>>()));
    }

    #[test]
    fn test_merge_proof_full_tree() {
        let leaves = (0..255).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves.clone());
        let root = bmt.get_merkle_root();
        let proofs = (0..255)
            .map(|i| BalancedBinaryMerkleProof::generate_proof(&bmt, i))
            .collect::<Vec<_>>();
        let merged_proof = MergedBalancedBinaryMerkleProof::create_from_proofs(proofs).unwrap();
        assert!(merged_proof.verify_consume(&root, leaves));
    }
}
