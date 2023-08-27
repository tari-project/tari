// Copyright 2023. The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use std::marker::PhantomData;

use digest::{consts::U32, Digest};

use crate::sparse_merkle_tree::{
    bit_utils::{height_key, path_matches_key, TraverseDirection},
    BranchNode,
    LeafNode,
    NodeHash,
    NodeKey,
    SMTError,
    SparseMerkleTree,
    ValueHash,
    EMPTY_NODE_HASH,
};

pub struct MerkleProof<H> {
    path: Vec<TraverseDirection>,
    siblings: Vec<NodeHash>,
    key: NodeKey,
    value: Option<ValueHash>,
    phantom: std::marker::PhantomData<H>,
}

impl<H: Digest<OutputSize = U32>> MerkleProof<H> {
    pub fn new(path: Vec<TraverseDirection>, siblings: Vec<NodeHash>, key: NodeKey, value: Option<ValueHash>) -> Self {
        Self {
            path,
            siblings,
            key,
            value,
            phantom: PhantomData::<H>,
        }
    }

    pub fn from_tree(tree: &SparseMerkleTree<H>, key: &NodeKey) -> Result<Self, SMTError> {
        tree.build_proof(key)
    }

    fn calculate_root_hash(&self) -> NodeHash {
        let node_hash = match self.value.as_ref() {
            Some(v) => LeafNode::<H>::hash_value(&self.key, v),
            None => EMPTY_NODE_HASH,
        };
        let n = self.siblings.len();
        let hash = self.siblings.iter().zip(self.path.iter()).rev().enumerate().fold(
            node_hash,
            |current, (i, (sibling_hash, direction))| {
                let height = n - i - 1;

                match direction {
                    TraverseDirection::Left => {
                        BranchNode::<H>::branch_hash(height, &height_key(&self.key, height), &current, sibling_hash)
                    },
                    TraverseDirection::Right => {
                        BranchNode::<H>::branch_hash(height, &height_key(&self.key, height), sibling_hash, &current)
                    },
                }
            },
        );
        let mut result = [0; 32];
        result.copy_from_slice(hash.as_slice());
        result.into()
    }

    pub fn validate_inclusion_proof(
        &self,
        expected_key: &NodeKey,
        expected_value: &ValueHash,
        expected_root: &NodeHash,
    ) -> bool {
        expected_key == &self.key &&
            Some(expected_value) == self.value.as_ref() &&
            expected_root == &self.calculate_root_hash()
    }

    pub fn validate_exclusion_proof(&self, expected_key: &NodeKey, expected_root: &NodeHash) -> bool {
        path_matches_key(expected_key, &self.path) &&
            expected_root == &self.calculate_root_hash() &&
            (self.value.is_none() || &self.key != expected_key)
    }

    pub fn key(&self) -> &NodeKey {
        &self.key
    }

    pub fn value_hash(&self) -> Option<&ValueHash> {
        self.value.as_ref()
    }
}

#[cfg(test)]
mod test {
    use blake2::Blake2b;
    use digest::consts::U32;
    use rand::{RngCore, SeedableRng};

    use super::*;

    fn random_arr(n: usize, seed: u64) -> Vec<[u8; 32]> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..n)
            .map(|_| {
                let mut key = [0u8; 32];
                rng.fill_bytes(&mut key);
                key
            })
            .collect()
    }

    fn random_keys(n: usize, seed: u64) -> Vec<NodeKey> {
        random_arr(n, seed).into_iter().map(|k| k.into()).collect()
    }

    fn random_values(n: usize, seed: u64) -> Vec<ValueHash> {
        random_arr(n, seed).into_iter().map(|k| k.into()).collect()
    }

    #[test]
    fn root_proof() {
        let key = NodeKey::from([64u8; 32]);
        let value = ValueHash::from([128u8; 32]);
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let hash = tree.hash().clone();
        let proof = tree.build_proof(&key).unwrap();

        assert!(!proof.validate_inclusion_proof(&key, &value, &hash));
        assert!(proof.validate_exclusion_proof(&key, &hash));

        tree.upsert(key.clone(), value.clone()).unwrap();
        let hash = tree.hash().clone();
        let proof = tree.build_proof(&key).unwrap();

        assert!(proof.validate_inclusion_proof(&key, &value, &hash));
        assert!(!proof.validate_inclusion_proof(&key, &ValueHash::from([1u8; 32]), &hash),);
        assert!(!proof.validate_exclusion_proof(&key, &hash));
    }

    #[test]
    fn merkle_proofs() {
        let n = 15;
        let keys = random_keys(n, 420);
        let values = random_values(n, 1420);
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        (0..n).for_each(|i| {
            let _ = tree.upsert(keys[i].clone(), values[i].clone()).unwrap();
        });
        let root_hash = tree.hash().clone();
        (0..n).for_each(|i| {
            let proof = tree.build_proof(&keys[i]).unwrap();
            // Validate the proof with correct key / value
            assert!(proof.validate_inclusion_proof(&keys[i], &values[i], &root_hash));
            // Show that incorrect value for existing key fails
            assert!(!proof.validate_inclusion_proof(&keys[i], &values[(i + 3) % n], &root_hash),);
            // Exclusion proof fails
            assert!(!proof.validate_exclusion_proof(&keys[i], &root_hash));
        });
        // Test exclusion proof
        let unused_keys = random_keys(n, 72);
        (0..n).for_each(|i| {
            let proof = tree.build_proof(&unused_keys[i]).unwrap();
            assert!(proof.validate_exclusion_proof(&unused_keys[i], &root_hash));
            assert!(!proof.validate_inclusion_proof(&unused_keys[i], &values[i], &root_hash),);
        });
    }
}
