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

use std::{convert::TryFrom, marker::PhantomData};

use digest::Digest;
use tari_common::DomainDigest;
use thiserror::Error;

use crate::{common::hash_together, ArrayLike, Hash};

pub(crate) fn cast_to_u32(value: usize) -> Result<u32, BalancedBinaryMerkleTreeError> {
    u32::try_from(value).map_err(|_| BalancedBinaryMerkleTreeError::MathOverFlow)
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum BalancedBinaryMerkleTreeError {
    #[error("There is no leaf with the hash provided.")]
    LeafNotFound,
    #[error("Math overflow")]
    MathOverFlow,
}

// The hashes are perfectly balanced binary tree, so parent at index `i` (0-based) has children at positions `2*i+1` and
// `2*i+1`.
//
// Because this implementation relies on the caller to hash leaf nodes, it is possible to instantiate a tree that is
/// susceptible to second-preimage attacks. The caller _must_ ensure that the hashers used to pre-hash leaf nodes and
/// instantiate the tree cannot produce collisions.
#[derive(Debug)]
pub struct BalancedBinaryMerkleTree<D> {
    hashes: Vec<Hash>,
    _phantom: PhantomData<D>,
}

impl<D> BalancedBinaryMerkleTree<D>
where D: Digest + DomainDigest
{
    // There is no push method for this tree. This tree is created at once and no modifications are allowed.
    pub fn create(leaves: Vec<Hash>) -> Self {
        let leaves_cnt = leaves.len();
        if leaves_cnt == 0 {
            return Self {
                hashes: vec![],
                _phantom: PhantomData,
            };
        }
        // The size of the tree of `n` leaves is `2*n - 1` where the leaves are at the end of the array.
        let mut hashes = Vec::with_capacity(2 * leaves_cnt - 1);
        hashes.extend(vec![vec![0; 32]; leaves_cnt - 1]);
        hashes.extend(leaves);
        // Now we compute the hashes from bottom to up of the tree.
        for i in (0..leaves_cnt - 1).rev() {
            hashes[i] = hash_together::<D>(&hashes[2 * i + 1], &hashes[2 * i + 2]);
        }
        Self {
            hashes,
            _phantom: PhantomData,
        }
    }

    pub fn get_merkle_root(&self) -> Hash {
        if self.hashes.is_empty() {
            D::digest(b"").to_vec()
        } else {
            self.hashes[0].clone()
        }
    }

    pub(crate) fn get_hash(&self, pos: usize) -> &Hash {
        &self.hashes[pos]
    }

    pub fn get_leaf(&self, leaf_index: usize) -> &Hash {
        self.get_hash(self.get_node_index(leaf_index))
    }

    pub(crate) fn get_node_index(&self, leaf_index: usize) -> usize {
        leaf_index + (self.hashes.len() >> 1)
    }

    pub fn find_leaf_index_for_hash(&self, hash: &Hash) -> Result<u32, BalancedBinaryMerkleTreeError> {
        let pos = self
            .hashes
            .position(hash)
            .expect("Unexpected Balanced Binary Merkle Tree error")
            .ok_or(BalancedBinaryMerkleTreeError::LeafNotFound)?;
        if pos < (self.hashes.len() >> 1) {
            // The hash provided was not for leaf, but for node.
            Err(BalancedBinaryMerkleTreeError::LeafNotFound)
        } else {
            Ok(cast_to_u32(pos - (self.hashes.len() >> 1))?)
        }
    }
}

#[cfg(test)]
mod test {
    use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHasher};

    use crate::{balanced_binary_merkle_tree::BalancedBinaryMerkleTreeError, BalancedBinaryMerkleTree};
    hash_domain!(TestDomain, "testing", 0);

    #[test]
    fn test_empty_tree() {
        let leaves = vec![];
        let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves);
        let root = bmt.get_merkle_root();
        assert_eq!(root, vec![
            72, 54, 179, 2, 214, 45, 9, 89, 161, 132, 177, 251, 229, 46, 124, 233, 32, 186, 46, 87, 127, 247, 19, 36,
            225, 191, 167, 189, 58, 58, 39, 74
        ]);
    }

    #[test]
    fn test_single_node_tree() {
        let leaves = vec![vec![0; 32]];
        let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves);
        let root = bmt.get_merkle_root();
        assert_eq!(root, vec![0; 32]);
    }

    #[test]
    fn test_find_leaf() {
        let leaves = (0..100).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<DomainSeparatedHasher<Blake256, TestDomain>>::create(leaves);
        assert_eq!(bmt.find_leaf_index_for_hash(&vec![42; 32]).unwrap(), 42);
        // Non existing hash
        assert_eq!(
            bmt.find_leaf_index_for_hash(&vec![142; 32]),
            Err(BalancedBinaryMerkleTreeError::LeafNotFound)
        );
        // This hash exists but it's not a leaf.
        assert_eq!(
            bmt.find_leaf_index_for_hash(&bmt.get_merkle_root()),
            Err(BalancedBinaryMerkleTreeError::LeafNotFound)
        );
    }
}
