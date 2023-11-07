// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::marker::PhantomData;

use digest::{consts::U32, Digest};

use crate::sparse_merkle_tree::{
    bit_utils::{height_key, TraverseDirection},
    BranchNode,
    EmptyNode,
    LeafNode,
    NodeHash,
    NodeKey,
    SMTError,
    SparseMerkleTree,
    ValueHash,
};

/// An inclusion proof for a key-value pair in a sparse Merkle& tree.
///
/// Given a sparse Merkle& tree, `tree`, you can create a proof that a certain key-value pair exists by calling
/// [`InclusionProof::from_tree`], for example:
///
/// ```
/// # use blake2::Blake2b;
/// # use digest::consts::U32;
/// # use tari_mmr::sparse_merkle_tree::{ExclusionProof, InclusionProof, NodeKey, SparseMerkleTree, ValueHash};
///  let key = NodeKey::from([64u8; 32]);
///  let value = ValueHash::from([128u8; 32]);
///
/// let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
///  tree.upsert(key.clone(), value.clone()).unwrap();
///  let hash = tree.hash().clone();
///
///  let in_proof = InclusionProof::from_tree(&tree, &key, &value).unwrap();
///  assert!(in_proof.validate(&key, &value, &hash));
/// ```
///
/// If you try to create an inclusion proof that is invalid, such as using the wrong value, or a key that is not in
/// the tree, `from_tree` will return a `NonViableProof` error.
///
/// ```
/// # use blake2::Blake2b;
/// # use digest::consts::U32;
/// # use tari_mmr::sparse_merkle_tree::{ExclusionProof, InclusionProof, NodeKey, SparseMerkleTree, ValueHash, SMTError};
///  let key = NodeKey::from([64u8; 32]);
///  let non_existent_key = NodeKey::from([65u8; 32]);
///  let value = ValueHash::from([128u8; 32]);
///  let wrong_value = ValueHash::from([127u8; 32]);
///
///  let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
///  tree.upsert(key.clone(), value.clone()).unwrap();
///  let root = tree.hash().clone();
///  let in_proof = InclusionProof::from_tree(&tree, &non_existent_key, &value);
///  assert!(matches!(in_proof, Err(SMTError::NonViableProof)));
///  let in_proof = InclusionProof::from_tree(&tree, &key, &wrong_value);
///  assert!(matches!(in_proof, Err(SMTError::NonViableProof)));
/// ```
pub struct InclusionProof<H> {
    siblings: Vec<NodeHash>,
    phantom: std::marker::PhantomData<H>,
}

/// An exclusion proof for a key in a sparse Merkle& tree.
///
/// Given a sparse Merkle& tree, `tree`, you can create a proof that a certain key does *not exist* in the tree by
/// calling [`ExclusionProof::from_tree`]. For example:
///
/// ```
/// # use blake2::Blake2b;
/// # use digest::consts::U32;
/// # use tari_mmr::sparse_merkle_tree::{ExclusionProof, InclusionProof, NodeKey, SparseMerkleTree, ValueHash};
///  let key = NodeKey::from([64u8; 32]);
///  let value = ValueHash::from([128u8; 32]);
///  let non_existent_key = NodeKey::from([65u8; 32]);
///  let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
///  tree.upsert(key, value).unwrap();
/// let hash = tree.hash().clone();
/// let ex_proof = ExclusionProof::from_tree(&tree, &non_existent_key).unwrap();
/// assert!(ex_proof.validate(&non_existent_key, &hash));
/// ```
///
/// As with [`InclusionProof`], if you try to create an exclusion proof that is invalid, such as using a key that is
/// in the tree, `from_tree` will return a `NonViableProof` error. For example, using the same tree from the last
/// example,
/// ```
/// # use blake2::Blake2b;
/// # use digest::consts::U32;
/// # use tari_mmr::sparse_merkle_tree::{ExclusionProof, InclusionProof, NodeKey, SparseMerkleTree, ValueHash, SMTError};
/// # let key = NodeKey::from([64u8; 32]);
/// # let value = ValueHash::from([128u8; 32]);
/// # let non_existent_key = NodeKey::from([65u8; 32]);
/// # let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
/// # tree.upsert(key.clone(), value).unwrap();
/// let ex_proof = ExclusionProof::from_tree(&tree, &key);
/// assert!(matches!(ex_proof, Err(SMTError::NonViableProof)));
/// ```
pub struct ExclusionProof<H> {
    siblings: Vec<NodeHash>,
    // The terminal node of the tree proof, or `None` if the the node is `Empty`.
    leaf: Option<LeafNode<H>>,
    phantom: std::marker::PhantomData<H>,
}

trait MerkleProofDigest<H: Digest<OutputSize = U32>> {
    /// Returns an array to the vector of sibling hashes along the path to the key's leaf node for this proof.
    fn siblings(&self) -> &[NodeHash];

    /// Calculate the Merkle& tree root for this proof, given the key and value hash.
    fn calculate_root_hash(&self, key: &NodeKey, leaf_hash: NodeHash) -> NodeHash {
        let n = self.siblings().len();
        let dirs = key.as_directions().take(n);
        let hash = self.siblings().iter().zip(dirs).rev().enumerate().fold(
            leaf_hash,
            |current, (i, (sibling_hash, direction))| {
                let height = n - i - 1;
                match direction {
                    TraverseDirection::Left => {
                        BranchNode::<H>::branch_hash(height, &height_key(key, height), &current, sibling_hash)
                    },
                    TraverseDirection::Right => {
                        BranchNode::<H>::branch_hash(height, &height_key(key, height), sibling_hash, &current)
                    },
                }
            },
        );
        let mut result = [0; 32];
        result.copy_from_slice(hash.as_slice());
        result.into()
    }
}

impl<H: Digest<OutputSize = U32>> InclusionProof<H> {
    /// Construct an inclusion proof using the vector of siblings provided. Usually you will not use this method, but
    /// will generate the proof using [`InclusionProof::from_tree`] instead.
    pub fn new(siblings: Vec<NodeHash>) -> Self {
        Self {
            siblings,
            phantom: PhantomData::<H>,
        }
    }

    /// Generates an inclusion proof for the given key and value hash from the given tree. If the key does not exist in
    /// tree, or the key does exist, but the value hash does not match, then `from_tree` will return a
    /// `NonViableProof` error.
    pub fn from_tree(tree: &SparseMerkleTree<H>, key: &NodeKey, value_hash: &ValueHash) -> Result<Self, SMTError> {
        let proof = tree.build_proof_candidate(key)?;
        match proof.leaf {
            Some(leaf) => {
                let node_hash = LeafNode::<H>::hash_value(key, value_hash);
                if leaf.hash() != &node_hash {
                    return Err(SMTError::NonViableProof);
                }
            },
            None => return Err(SMTError::NonViableProof),
        }
        Ok(Self::new(proof.siblings))
    }

    /// Validates the inclusion proof against the given key, value hash and root hash.
    /// The function reconstructs the tree using the expected key and value hash, and then calculates the root hash.
    /// Validation succeeds if the calculated root hash matches the given root hash.
    pub fn validate(&self, expected_key: &NodeKey, expected_value: &ValueHash, expected_root: &NodeHash) -> bool {
        // calculate expected leaf node hash
        let leaf_hash = LeafNode::<H>::hash_value(expected_key, expected_value);
        let calculated_root = self.calculate_root_hash(expected_key, leaf_hash);
        calculated_root == *expected_root
    }
}

impl<H: Digest<OutputSize = U32>> MerkleProofDigest<H> for InclusionProof<H> {
    fn siblings(&self) -> &[NodeHash] {
        &self.siblings
    }
}

impl<H: Digest<OutputSize = U32>> ExclusionProof<H> {
    /// Construct an exclusion proof using the vector of siblings and the existing leaf node provided. Usually you will
    /// not use this method, but will generate the proof using [`ExclusionProof::from_tree`] instead.
    pub fn new(siblings: Vec<NodeHash>, leaf: Option<LeafNode<H>>) -> Self {
        Self {
            siblings,
            leaf,
            phantom: PhantomData::<H>,
        }
    }

    /// Generates an exclusion proof for the given key from the given tree. If the key exists in the tree then
    /// `from_tree` will return a `NonViableProof` error.
    pub fn from_tree(tree: &SparseMerkleTree<H>, key: &NodeKey) -> Result<Self, SMTError> {
        let proof = tree.build_proof_candidate(key)?;
        // If the keys match, then we cannot provide an exclusion proof, since the key *is* in the tree
        if let Some(leaf) = &proof.leaf {
            if leaf.key() == key {
                return Err(SMTError::NonViableProof);
            }
        }
        Ok(proof)
    }

    /// Validates the exclusion proof against the given key and root hash. The function reconstructs the tree using the
    /// expected key and places the leaf node provided in the proof at the terminal position. It then calculates the
    /// root hash. Validation succeeds if the calculated root hash matches the given root hash, and the leaf node is
    /// empty, or the existing leaf node has a different key to the expected key.
    pub fn validate(&self, expected_key: &NodeKey, expected_root: &NodeHash) -> bool {
        let leaf_hash = match &self.leaf {
            Some(leaf) => leaf.hash().clone(),
            None => (EmptyNode {}).hash().clone(),
        };
        let root = self.calculate_root_hash(expected_key, leaf_hash);
        // For exclusion proof, roots must match AND existing leaf must be empty, or keys must not match
        root == *expected_root &&
            match &self.leaf {
                Some(leaf) => leaf.key() != expected_key,
                None => true,
            }
    }
}

impl<H: Digest<OutputSize = U32>> MerkleProofDigest<H> for ExclusionProof<H> {
    fn siblings(&self) -> &[NodeHash] {
        &self.siblings
    }
}

#[cfg(test)]
mod test {
    use blake2::Blake2b;
    use digest::consts::U32;

    use super::*;

    #[test]
    fn root_proof() {
        let key = NodeKey::from([64u8; 32]);
        let key2 = NodeKey::from([65u8; 32]);
        let value = ValueHash::from([128u8; 32]);
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let hash = tree.hash().clone();
        let in_proof = InclusionProof::from_tree(&tree, &key, &value);
        assert!(matches!(in_proof, Err(SMTError::NonViableProof)));
        let ex_proof = ExclusionProof::from_tree(&tree, &key).unwrap();
        assert!(ex_proof.validate(&key, &hash));

        tree.upsert(key.clone(), value.clone()).unwrap();
        let hash2 = tree.hash().clone();

        let in_proof = InclusionProof::from_tree(&tree, &key, &value).unwrap();
        let ex_proof = ExclusionProof::from_tree(&tree, &key);
        assert!(matches!(ex_proof, Err(SMTError::NonViableProof)));

        assert!(in_proof.validate(&key, &value, &hash2));
        // correct key, wrong value
        assert!(!in_proof.validate(&key, &ValueHash::from([1u8; 32]), &hash2),);
        // incorrect key, correct value
        assert!(!in_proof.validate(&key2, &value, &hash2));
        // correct key, wrong hash
        assert!(!in_proof.validate(&key, &value, &hash));

        // exclusion proof assertions
        let ex_proof = ExclusionProof::from_tree(&tree, &key2).unwrap();
        assert!(!ex_proof.validate(&key, &hash2));
        assert!(!ex_proof.validate(&key, &hash));
        assert!(!ex_proof.validate(&key2, &hash));
        assert!(ex_proof.validate(&key2, &hash2));
    }

    #[test]
    fn non_viable_inclusion_proof() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let key = NodeKey::from([64u8; 32]);
        let value = ValueHash::from([128u8; 32]);
        // Inclusion proof on empty tree
        let proof = InclusionProof::from_tree(&tree, &key, &value);
        assert!(matches!(proof, Err(SMTError::NonViableProof)));
        tree.upsert(key, value.clone()).unwrap();
        // Inclusion proof on non-existent key
        let proof = InclusionProof::from_tree(&tree, &NodeKey::from([65u8; 32]), &value);
        assert!(matches!(proof, Err(SMTError::NonViableProof)));
        // Existing key, wrong value
        let proof = InclusionProof::from_tree(&tree, &NodeKey::from([64u8; 32]), &ValueHash::from([0u8; 32]));
        assert!(matches!(proof, Err(SMTError::NonViableProof)));
    }

    #[test]
    fn proof_with_stale_hash() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        tree.upsert(NodeKey::from([64u8; 32]), ValueHash::from([128u8; 32]))
            .unwrap();
        tree.upsert(NodeKey::from([155u8; 32]), ValueHash::from([128u8; 32]))
            .unwrap();
        let key = NodeKey::from([65u8; 32]);
        let value = ValueHash::from([65u8; 32]);
        tree.upsert(key.clone(), value.clone()).unwrap();
        let proof = InclusionProof::from_tree(&tree, &key, &value);
        assert!(matches!(proof, Err(SMTError::StaleHash)));
    }

    #[test]
    fn deep_inclusion_proof() {
        let key1 = NodeKey::from([64u8; 32]);
        let mut key2 = key1.clone();
        key2.as_slice_mut()[31] = 65;
        let value = ValueHash::from([128u8; 32]);
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        tree.upsert(key1, ValueHash::from([42u8; 32])).unwrap();
        tree.upsert(key2.clone(), value.clone()).unwrap();
        tree.hash();
        let proof = InclusionProof::from_tree(&tree, &key2, &value).unwrap();
        assert!(proof.validate(&key2, &value, tree.hash()));
    }

    #[test]
    fn exclusion_proofs() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let proof = ExclusionProof::from_tree(&tree, &NodeKey::from([64; 32])).unwrap();
        // Assert that key does not exist
        assert!(proof.validate(&NodeKey::from([64u8; 32]), tree.hash()));
        // Validation with a non-existent key, but different root should fail
        assert!(!proof.validate(&NodeKey::from([64u8; 32]), &NodeHash::from([1; 32])));

        // Tree with single node
        tree.upsert(NodeKey::from([64u8; 32]), ValueHash::from([42u8; 32]))
            .unwrap();
        // Cannot create an exclusion proof for a key that exists
        let proof = ExclusionProof::from_tree(&tree, &NodeKey::from([64u8; 32]));
        assert!(matches!(proof, Err(SMTError::NonViableProof)));
        // A valid exclusion proof
        let proof = ExclusionProof::from_tree(&tree, &NodeKey::from([65u8; 32])).unwrap();
        assert!(proof.validate(&NodeKey::from([65u8; 32]), tree.hash()));
        // Does not validate against invalid root hash
        assert!(!proof.validate(&NodeKey::from([65u8; 32]), &NodeHash::default()));
        // All key paths for exclusion proofs are identical right now, so any non-existent key will validate. (bug or
        // feature?)
        assert!(proof.validate(&NodeKey::from([67u8; 32]), tree.hash()));
        // Use an exclusion proof to try and give an invalid validation for a key that does exist
        assert!(!proof.validate(&NodeKey::from([64u8; 32]), tree.hash()));

        // A tree with branches
        tree.upsert(NodeKey::from([96u8; 32]), ValueHash::from([1u8; 32]))
            .unwrap();
        tree.upsert(NodeKey::from([222u8; 32]), ValueHash::from([2u8; 32]))
            .unwrap();
        tree.hash();
        // Cannot create an exclusion proof for a key that exists
        let proof = ExclusionProof::from_tree(&tree, &NodeKey::from([222u8; 32]));
        assert!(matches!(proof, Err(SMTError::NonViableProof)));
        // A valid exclusion proof
        let proof = ExclusionProof::from_tree(&tree, &NodeKey::from([99u8; 32])).unwrap();
        assert!(proof.validate(&NodeKey::from([99u8; 32]), tree.hash()));
        // Does not validate against invalid root hash
        assert!(!proof.validate(&NodeKey::from([99u8; 32]), &NodeHash::default()));
        // Not all non-existent keys will validate. (bug or feature?)
        assert!(!proof.validate(&NodeKey::from([65; 32]), tree.hash()));
        // But any keys with prefix 011 (that is not 96) will validate
        assert!(proof.validate(&NodeKey::from([0b0110_0011; 32]), tree.hash()));
        assert!(proof.validate(&NodeKey::from([0b0111_0001; 32]), tree.hash()));
        // The key 96 does exist..
        assert!(!proof.validate(&NodeKey::from([0b0110_0000; 32]), tree.hash()));
    }
}
