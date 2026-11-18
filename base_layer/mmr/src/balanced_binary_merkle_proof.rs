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

use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    marker::PhantomData,
};

use borsh::{BorshDeserialize, BorshSerialize};
use digest::Digest;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BalancedBinaryMerkleTree, Hash, common::hash_together};

fn cast_to_u32(value: usize) -> Result<u32, BalancedBinaryMerkleProofError> {
    u32::try_from(value).map_err(|_| BalancedBinaryMerkleProofError::MathOverflow)
}

/// Inclusion proof for a single node of a [`BalancedBinaryMerkleTree`].
///
/// Leaves and internal nodes are hashed identically (no domain separation), so callers must domain-separate leaf
/// hashes if a leaf hash could also be an internal node value. `node_index` is bound to the path: the proof only
/// verifies if the walk from `node_index` ends exactly at the root.
#[derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BalancedBinaryMerkleProof<D> {
    /// Since this is balanced tree, the index `2k+1` is always left child and `2k` is right child
    path: Vec<Hash>,
    node_index: u32,
    _phantom: PhantomData<D>,
}

impl<D> BalancedBinaryMerkleProof<D>
where D: Digest
{
    /// Returns true if `leaf_hash` at `node_index` hashes up `path` to `root`. The proof verifies only if `node_index`
    /// and `path.len()` describe a walk that ends exactly at the root (node index 0), so an internal node cannot be
    /// passed off as a leaf with a shorter path. Leaves and internal nodes are hashed identically, so callers must
    /// domain-separate leaf hashes if a hash could also be an internal node value.
    #[must_use = "Must use the result of the proof verification"]
    pub fn verify(&self, root: &Hash, leaf_hash: Hash) -> bool {
        let mut computed_root = leaf_hash;
        let mut node_index = self.node_index;
        for sibling in &self.path {
            if node_index & 1 == 1 {
                computed_root = hash_together::<D>(&computed_root, sibling);
            } else {
                computed_root = hash_together::<D>(sibling, &computed_root);
            }

            match node_index.checked_sub(1).and_then(|i| i.checked_shr(1)) {
                Some(i) => {
                    node_index = i;
                },
                None => return false,
            }
        }
        computed_root == *root && node_index == 0
    }

    pub fn generate_proof(
        tree: &BalancedBinaryMerkleTree<D>,
        leaf_index: usize,
    ) -> Result<Self, BalancedBinaryMerkleProofError> {
        let node_index = tree.get_node_index(leaf_index);
        let mut index = node_index;
        let mut path = Vec::new();
        while index > 0 {
            // Parent at (i - 1) / 2
            let parent = index.saturating_sub(1) >> 1;
            // The children are 2i + 1 and 2i + 2, so together are 4i + 3. We subtract one, we get the other.
            let sibling = parent.saturating_mul(4).saturating_add(3).saturating_sub(index);
            let hash = tree
                .get_hash(sibling)
                .cloned()
                .ok_or(BalancedBinaryMerkleProofError::TreeDoesNotContainLeafIndex { leaf_index })?;
            path.push(hash);
            // Traverse to parent
            index = parent;
        }
        Ok(Self {
            path,
            node_index: cast_to_u32(node_index)?,
            _phantom: PhantomData,
        })
    }

    pub fn path(&self) -> &[Hash] {
        &self.path
    }

    pub fn node_index(&self) -> u32 {
        self.node_index
    }
}

#[derive(Debug, Error)]
pub enum BalancedBinaryMerkleProofError {
    #[error("Can't merge zero proofs.")]
    CantMergeZeroProofs,
    #[error("Bad proof semantics")]
    BadProofSemantics,
    #[error("Math overflow")]
    MathOverflow,
    #[error("Tree does not contain leaf index {leaf_index}")]
    TreeDoesNotContainLeafIndex { leaf_index: usize },
    #[error("Index {index} is out of range. The len is {len}")]
    IndexOutOfRange { index: usize, len: usize },
    #[error("The computed root does not match the expected root")]
    RootMismatch,
}

/// Flag to indicate if proof data represents an index or a node hash
/// This reduces the need for checking lengths instead
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MergedBalancedBinaryMerkleIndexOrHash {
    Index(u64),
    Hash(Hash),
}

/// Several [`BalancedBinaryMerkleProof`]s for the same tree merged into one, sharing the common parts of their paths.
///
/// Leaves and internal nodes are hashed identically (no domain separation), so callers must domain-separate leaf
/// hashes. Each proof's node index is bound to its path, and proofs with heights above 32 are rejected.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MergedBalancedBinaryMerkleProof<D> {
    paths: Vec<Vec<MergedBalancedBinaryMerkleIndexOrHash>>,
    node_indices: Vec<u32>,
    heights: Vec<u32>,
    _phantom: PhantomData<D>,
}

impl<D> MergedBalancedBinaryMerkleProof<D>
where D: Digest
{
    pub fn create_from_proofs(proofs: &[BalancedBinaryMerkleProof<D>]) -> Result<Self, BalancedBinaryMerkleProofError> {
        let heights = proofs
            .iter()
            .map(|proof| cast_to_u32(proof.path.len()))
            .collect::<Result<Vec<_>, _>>()?;
        let max_height = *heights
            .iter()
            .max()
            .ok_or(BalancedBinaryMerkleProofError::CantMergeZeroProofs)?;

        let mut indices = proofs.iter().map(|proof| proof.node_index).collect::<Vec<_>>();
        let mut paths = vec![Vec::new(); proofs.len()];
        let mut join_indices = vec![false; proofs.len()];
        let mut hash_map = HashMap::new();
        for height in (0..max_height).rev() {
            hash_map.clear();
            for (index, proof) in proofs.iter().enumerate() {
                // If this path was already joined ignore it.
                if !join_indices.get(index).expect("Index should expect") && proof.path.len() > height as usize {
                    let parent = indices.get(index).expect("Index should expect").saturating_sub(1) >> 1;
                    if let Some(other_proof_idx) = hash_map.insert(parent, index) {
                        *join_indices.get_mut(index).expect("Index should expect") = true;
                        // The other proof doesn't need a hash, it needs an index to this hash. It must have
                        // contributed a hash at this height, otherwise the proofs we were given cannot be merged
                        // (e.g. more than two proofs sharing the same parent).
                        *paths
                            .get_mut(other_proof_idx)
                            .ok_or(BalancedBinaryMerkleProofError::BadProofSemantics)?
                            .first_mut()
                            .ok_or(BalancedBinaryMerkleProofError::BadProofSemantics)? =
                            MergedBalancedBinaryMerkleIndexOrHash::Index(
                                u64::try_from(index).map_err(|_| BalancedBinaryMerkleProofError::MathOverflow)?,
                            );
                    } else {
                        paths.get_mut(index).expect("should exist").insert(
                            0,
                            MergedBalancedBinaryMerkleIndexOrHash::Hash(
                                proof
                                    .path
                                    .get(proof.path.len().saturating_sub(1).saturating_sub(height as usize))
                                    .expect("Should exist")
                                    .clone(),
                            ),
                        );
                    }
                    *indices.get_mut(index).expect("Should exits") = parent;
                }
            }
        }

        Ok(Self {
            paths,
            node_indices: proofs.iter().map(|proof| proof.node_index).collect::<Vec<_>>(),
            heights,
            _phantom: PhantomData,
        })
    }

    /// Verify the merged proof for the given leaf hashes against the root, consuming the proof. Returns
    /// `Err(BalancedBinaryMerkleProofError::RootMismatch)` if the computed root does not match `root`.
    ///
    /// Node indices are `u32`, so no balanced tree has more than 32 levels; proofs with a larger height are rejected.
    pub fn verify_consume(mut self, root: &Hash, leaf_hashes: Vec<Hash>) -> Result<(), BalancedBinaryMerkleProofError> {
        // Check that the proof and verifier data match
        let n = self.node_indices.len(); // number of merged proofs
        if self.paths.len() != n || self.heights.len() != n || leaf_hashes.len() != n {
            return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
        }

        let mut computed_hashes = leaf_hashes;
        let max_height = *self
            .heights
            .iter()
            .max()
            .ok_or(BalancedBinaryMerkleProofError::CantMergeZeroProofs)?;
        if max_height > 32 {
            return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
        }
        let mut consumed = HashSet::new();
        // We need to compute the hashes row by row to be sure they are processed correctly.
        for height in (0..max_height).rev() {
            // Snapshot only the hashes referenced by an `Index` this round, rather than cloning every hash each round
            let referenced = self
                .paths
                .iter()
                .zip(&self.heights)
                .filter(|(_, h)| **h > height)
                .filter_map(|(path, _)| match path.last() {
                    Some(MergedBalancedBinaryMerkleIndexOrHash::Index(i)) => usize::try_from(*i).ok(),
                    _ => None,
                })
                .filter_map(|i| computed_hashes.get(i).map(|hash| (i, hash.clone())))
                .collect::<HashMap<_, _>>();
            let mut dangling_paths = HashSet::new();
            for (index, leaf) in computed_hashes.iter_mut().enumerate().rev() {
                if *self.heights.get(index).expect("Should exist") <= height {
                    continue;
                }

                let Some(hash_or_index) = self.paths.get_mut(index).expect("Should exist").pop() else {
                    // Check if we already joined with other path.
                    if !consumed.contains(&index) {
                        // If the path ended, it's going to be merged to some other path.
                        if !dangling_paths.insert(index) {
                            return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
                        }
                    }
                    // Path at this index already completely processed
                    continue;
                };

                let hash = match hash_or_index {
                    MergedBalancedBinaryMerkleIndexOrHash::Index(index) => {
                        if !dangling_paths
                            .remove(&usize::try_from(index).map_err(|_| BalancedBinaryMerkleProofError::MathOverflow)?)
                        {
                            // If some path is joining our path, that path should have ended.
                            return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
                        }
                        consumed
                            .insert(usize::try_from(index).map_err(|_| BalancedBinaryMerkleProofError::MathOverflow)?);
                        let index = usize::try_from(index).map_err(|_| BalancedBinaryMerkleProofError::MathOverflow)?;

                        // The index must also point to one of the proofs
                        referenced
                            .get(&index)
                            .ok_or(BalancedBinaryMerkleProofError::IndexOutOfRange { index, len: n })?
                    },
                    MergedBalancedBinaryMerkleIndexOrHash::Hash(ref hash) => hash,
                };
                // Left (2k + 1) or right (2k) sibling?
                if self.node_indices.get(index).expect("Should exist") & 1 == 1 {
                    *leaf = hash_together::<D>(leaf, hash);
                } else {
                    *leaf = hash_together::<D>(hash, leaf);
                }
                // Parent
                *self.node_indices.get_mut(index).expect("Should exist") =
                    self.node_indices.get(index).expect("Should exist").saturating_sub(1) >> 1;
            }
            if !dangling_paths.is_empty() {
                // Something path ended, but it's not joined with any other path.
                return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
            }
        }
        if consumed.len().saturating_add(1) < self.paths.len() {
            // If the proof is valid then all but one paths will be consumed by other paths.
            return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
        }
        // Every proof that was not merged into another must have walked up to the root
        if self
            .node_indices
            .iter()
            .enumerate()
            .any(|(i, node_index)| *node_index != 0 && !consumed.contains(&i))
        {
            return Err(BalancedBinaryMerkleProofError::BadProofSemantics);
        }
        let computed_root = computed_hashes
            .first()
            .ok_or(BalancedBinaryMerkleProofError::BadProofSemantics)?;
        if computed_root == root {
            Ok(())
        } else {
            Err(BalancedBinaryMerkleProofError::RootMismatch)
        }
    }
}

#[cfg(test)]
mod test {

    #![allow(clippy::indexing_slicing)]
    use blake2::Blake2b;
    use digest::consts::U32;
    use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};

    use super::*;

    hash_domain!(TestDomain, "com.tari.test.testing", 0);

    type TestHasher = DomainSeparatedHasher<Blake2b<U32>, TestDomain>;

    #[test]
    fn test_small_tree() {
        let leaves = (0..4usize)
            .map(|i| vec![u8::try_from(i).unwrap(); 32])
            .collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());

        assert_eq!(bmt.num_nodes(), (4 << 1) - 1);
        assert_eq!(bmt.num_leaf_nodes(), 4);
        let root = bmt.get_merkle_root();
        let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
        assert!(proof.verify(&root, leaves[0].clone()));
        assert!(!proof.verify(&root, leaves[1].clone()));
        assert!(!proof.verify(&root, leaves[2].clone()));
        assert!(!proof.verify(&root, leaves[3].clone()));

        let proof1 = BalancedBinaryMerkleProof::generate_proof(&bmt, 1).unwrap();

        let merged = MergedBalancedBinaryMerkleProof::create_from_proofs(&[proof, proof1]).unwrap();
        merged
            .verify_consume(&root, vec![leaves[0].clone(), leaves[1].clone()])
            .unwrap();
    }

    #[test]
    fn test_merged_proof_with_short_heights_is_rejected() {
        // `heights` is deserialised along with the rest of the proof, so it may not line up with the other fields
        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![vec![], vec![]],
            node_indices: vec![0, 1],
            heights: vec![1],
            _phantom: PhantomData,
        };
        assert!(matches!(
            proof.verify_consume(&vec![0u8; 32], vec![vec![], vec![]]).unwrap_err(),
            BalancedBinaryMerkleProofError::BadProofSemantics
        ));
    }

    #[test]
    fn test_merging_proofs_that_share_a_parent_is_rejected() {
        // Three proofs for the same leaf all share a parent, so they cannot be merged into a single proof
        let leaves = (0..4usize)
            .map(|i| vec![u8::try_from(i).unwrap(); 32])
            .collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves);
        let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
        let err =
            MergedBalancedBinaryMerkleProof::create_from_proofs(&[proof.clone(), proof.clone(), proof]).unwrap_err();
        assert!(matches!(err, BalancedBinaryMerkleProofError::BadProofSemantics));
    }

    #[test]
    fn test_zero_height_proof_should_be_invalid() {
        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![vec![]],
            node_indices: vec![0],
            heights: vec![0],
            _phantom: PhantomData,
        };
        assert!(matches!(
            proof.verify_consume(&vec![0u8; 32], vec![vec![]]).unwrap_err(),
            BalancedBinaryMerkleProofError::RootMismatch
        ));

        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![vec![]],
            node_indices: vec![0],
            heights: vec![1],
            _phantom: PhantomData,
        };
        // This will fail because the node height is 1 and it's empty, so it's not going to compute the root hash.
        proof.verify_consume(&vec![0u8; 32], vec![vec![]]).unwrap_err();
    }

    #[test]
    fn test_generate_and_verify_big_tree() {
        for n in [1usize, 100, 1000, 10_000] {
            let leaves = (0..n)
                .map(|i| [i.to_le_bytes().to_vec(), vec![0u8; 24]].concat())
                .collect::<Vec<_>>();
            let hash_0 = leaves[0].clone();
            let hash_n_half = leaves[n / 2].clone();
            let hash_last = leaves[n - 1].clone();
            let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves);
            let root = bmt.get_merkle_root();
            let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
            assert!(proof.verify(&root, hash_0));
            let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, n / 2).unwrap();
            assert!(proof.verify(&root, hash_n_half));
            let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, n - 1).unwrap();
            assert!(proof.verify(&root, hash_last));
        }
    }

    #[test]
    fn test_merge_proof() {
        let leaves = (0..255).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());
        let indices = [50, 0, 200, 150, 100];
        let root = bmt.get_merkle_root();
        let proofs = indices
            .iter()
            .map(|i| BalancedBinaryMerkleProof::generate_proof(&bmt, *i))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let merged_proof = MergedBalancedBinaryMerkleProof::create_from_proofs(&proofs).unwrap();
        merged_proof
            .verify_consume(&root, indices.iter().map(|i| leaves[*i].clone()).collect::<Vec<_>>())
            .unwrap();
    }

    #[test]
    fn test_merge_proof_full_tree() {
        let leaves = (0..=255).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());
        let root = bmt.get_merkle_root();
        let proofs = (0..=255)
            .map(|i| BalancedBinaryMerkleProof::generate_proof(&bmt, i))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let merged_proof = MergedBalancedBinaryMerkleProof::create_from_proofs(&proofs).unwrap();
        merged_proof.verify_consume(&root, leaves).unwrap();
    }

    #[test]
    fn test_verify_faulty_proof() {
        let faulty_proof = BalancedBinaryMerkleProof::<TestHasher> {
            path: vec![vec![1u8; 32], vec![1u8; 32]],
            node_index: 2,
            _phantom: Default::default(),
        };

        // This used to panic since this proof is not possible by using generate_proof
        assert!(!faulty_proof.verify(&vec![0u8; 32], vec![0u8; 32]));

        let faulty_proof = BalancedBinaryMerkleProof::<TestHasher> {
            path: vec![vec![1u8; 32], vec![1u8; 32], vec![0u8; 32], vec![0u8; 32]],
            node_index: 3,
            _phantom: Default::default(),
        };
        assert!(!faulty_proof.verify(&vec![0u8; 32], vec![0u8; 32]));

        // Merged proof - no panic
        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![],
            node_indices: vec![],
            heights: vec![],
            _phantom: PhantomData,
        };
        proof.verify_consume(&vec![0u8; 32], vec![]).unwrap_err();

        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![vec![MergedBalancedBinaryMerkleIndexOrHash::Hash(vec![1u8; 32])], vec![
                MergedBalancedBinaryMerkleIndexOrHash::Hash(vec![2u8; 32]),
            ]],
            node_indices: vec![1, 1],
            // max_height == 0 which equates to leaf_hash[0] == root, even though this proof is invalid.
            // This assumes an attacker can control the first leaf hash.
            heights: vec![0, 0],
            _phantom: PhantomData,
        };
        // This will fail because there are more hashes on the same level as there can be.
        proof
            .verify_consume(&vec![5u8; 32], vec![vec![5u8; 32], vec![2u8; 32]])
            .unwrap_err();

        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![vec![MergedBalancedBinaryMerkleIndexOrHash::Hash(vec![5u8; 32])], vec![
                MergedBalancedBinaryMerkleIndexOrHash::Index(1),
            ]],
            node_indices: vec![1, 1],
            heights: vec![0, 1],
            _phantom: PhantomData,
        };
        // This will fail because we can't have any more nodes if we have leaf at the root.
        proof
            .verify_consume(&vec![5u8; 32], vec![vec![5u8; 32], vec![2u8; 32]])
            .unwrap_err();
    }

    #[test]
    fn test_generate_faulty_proof() {
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(vec![]);
        let err = BalancedBinaryMerkleProof::<TestHasher>::generate_proof(&bmt, 1).unwrap_err();
        assert!(matches!(
            err,
            BalancedBinaryMerkleProofError::TreeDoesNotContainLeafIndex { leaf_index: 1 }
        ));
    }

    #[test]
    fn verify_rejects_mutated_node_index() {
        let leaves = (0..4u8).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());
        let root = bmt.get_merkle_root();
        let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
        assert_eq!(proof.node_index, 3);
        assert!(proof.verify(&root, leaves[0].clone()));
        let mutated = BalancedBinaryMerkleProof::<TestHasher> { node_index: 7, ..proof };
        assert!(!mutated.verify(&root, leaves[0].clone()));
    }

    #[test]
    fn verify_rejects_internal_node_as_leaf() {
        let leaves = (0..4u8).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves);
        let root = bmt.get_merkle_root();
        let mut proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
        // Keep only the path above internal node 1, still claiming the leaf's node index
        proof.path.remove(0);
        assert!(!proof.verify(&root, bmt.get_hash(1).unwrap().clone()));
    }

    #[test]
    fn verify_rejects_truncated_path() {
        let leaves = (0..4u8).map(|i| vec![i; 32]).collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());
        let root = bmt.get_merkle_root();
        let mut proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
        proof.path.pop();
        assert!(!proof.verify(&root, leaves[0].clone()));
    }

    #[test]
    fn merged_rejects_height_above_32() {
        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths: vec![vec![]],
            node_indices: vec![0],
            heights: vec![33],
            _phantom: PhantomData,
        };
        assert!(matches!(
            proof.verify_consume(&vec![0u8; 32], vec![vec![0u8; 32]]).unwrap_err(),
            BalancedBinaryMerkleProofError::BadProofSemantics
        ));
    }

    #[test]
    fn merged_verify_is_linear() {
        let n = 4096usize;
        // A 4096-long path is rejected up front by the height bound
        let mut paths = vec![Vec::new(); n];
        paths[0] = vec![MergedBalancedBinaryMerkleIndexOrHash::Hash(vec![0u8; 32]); n];
        let proof = MergedBalancedBinaryMerkleProof::<TestHasher> {
            paths,
            node_indices: vec![0; n],
            heights: vec![u32::try_from(n).unwrap(); n],
            _phantom: PhantomData,
        };
        let start = std::time::Instant::now();
        let err = proof
            .verify_consume(&vec![0u8; 32], vec![vec![0u8; 32]; n])
            .unwrap_err();
        assert!(matches!(err, BalancedBinaryMerkleProofError::BadProofSemantics));
        assert!(start.elapsed() < std::time::Duration::from_millis(100));

        // Honest merge of every leaf, with all heights raised to 32 and proof 0 padded so every round runs over n
        // proofs. Not timed: with heights capped at 32 the cost is dominated by the n honest hashes, and a wall-clock
        // bound on that in a debug build is flaky on loaded CI machines.
        let leaves = (0..n)
            .map(|i| [i.to_le_bytes().to_vec(), vec![0u8; 24]].concat())
            .collect::<Vec<_>>();
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());
        let proofs = (0..n)
            .map(|i| BalancedBinaryMerkleProof::generate_proof(&bmt, i))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut merged = MergedBalancedBinaryMerkleProof::create_from_proofs(&proofs).unwrap();
        let padding = 32 - merged.heights[0] as usize;
        merged.paths[0].splice(0..0, vec![
            MergedBalancedBinaryMerkleIndexOrHash::Hash(vec![1u8; 32]);
            padding
        ]);
        merged.heights = vec![32; n];
        let err = merged.verify_consume(&bmt.get_merkle_root(), leaves).unwrap_err();
        assert!(matches!(err, BalancedBinaryMerkleProofError::RootMismatch));
    }

    #[test]
    fn test_single_node_proof() {
        let leaves = vec![vec![1u8; 32]];
        let bmt = BalancedBinaryMerkleTree::<TestHasher>::create(leaves.clone());

        assert_eq!(bmt.num_nodes(), 1);
        assert_eq!(bmt.num_leaf_nodes(), 1);
        let root = bmt.get_merkle_root();
        assert_eq!(root, leaves[0]);
        let proof = BalancedBinaryMerkleProof::generate_proof(&bmt, 0).unwrap();
        assert!(proof.verify(&root, leaves[0].clone()));
        assert!(proof.path.is_empty());

        let merged = MergedBalancedBinaryMerkleProof::create_from_proofs(&[proof]).unwrap();
        merged.verify_consume(&root, vec![leaves[0].clone()]).unwrap();
    }
}
