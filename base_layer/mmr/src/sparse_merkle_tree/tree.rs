// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{fmt::Debug, mem};

use digest::{consts::U32, Digest};
use serde::{Deserialize, Serialize};

use crate::sparse_merkle_tree::{
    bit_utils::{traverse_direction, TraverseDirection},
    EmptyNode,
    ExclusionProof,
    LeafNode,
    Node,
    Node::{Branch, Empty, Leaf},
    NodeHash,
    NodeKey,
    SMTError,
    ValueHash,
};

#[derive(Debug, PartialEq)]
pub enum UpdateResult {
    Updated(ValueHash),
    Inserted,
}

#[derive(Debug, PartialEq)]
pub enum DeleteResult {
    Deleted(ValueHash),
    KeyNotFound,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound(deserialize = "H:"))]
#[serde(bound(serialize = "H:"))]
pub struct SparseMerkleTree<H> {
    size: u64,
    root: Node<H>,
}

impl<H> SparseMerkleTree<H> {
    pub fn new() -> Self {
        Self {
            size: 0,
            root: Node::Empty(EmptyNode {}),
        }
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn root(&self) -> &Node<H> {
        &self.root
    }
}

impl<H> Default for SparseMerkleTree<H> {
    fn default() -> Self {
        Self::new()
    }
}

enum PathClassifier {
    // The desired key is not in the tree
    KeyDoesNotExist,
    // The desired key is in a terminal node
    TerminalBranch,
    // The desired key is in a non-terminal node. The tree continues deeper on the other branch.
    NonTerminalBranch,
}

/// Private struct, representing a terminal node in the tree. A terminal node is on that should house the key we've been
/// searching for in a CRUD operation. The parent must exist (there's special handling for the root elsewhere) and is
/// a branch node by definition.
///
/// There are utility methods for inserting or updating a leaf node, and for deleting a leaf node.
struct TerminalBranch<'a, H> {
    parent: &'a mut Node<H>,
    direction: TraverseDirection,
    empty_siblings: Vec<bool>,
}

impl<'a, H: Digest<OutputSize = U32>> TerminalBranch<'a, H> {
    /// Returns the terminal node of the branch
    pub fn terminal(&self) -> &Node<H> {
        let branch = self.parent.as_branch().unwrap();
        branch.child(self.direction)
    }

    // When inserting a new leaf node, there might be a slew of branch nodes to create depending on where the keys
    // of the existing leaf and new leaf node diverge. E.g. if a leaf node of key `1101` is being inserted into a
    // tree with a single leaf node of key `1100` then we must create branches at `1...`, `11..`, and `110.` with
    // the leaf nodes `1100` and `1101` being the left and right branches at height 4 respectively.
    //
    // This function handles this case, as well the simple update case, and the simple insert case, where the target
    // node is empty.
    fn insert_or_update_leaf(&mut self, leaf: LeafNode<H>) -> Result<UpdateResult, SMTError> {
        let branch = self.parent.as_branch_mut().ok_or(SMTError::UnexpectedNodeType)?;
        let height = branch.height();
        let terminal = branch.child_mut(self.direction);
        match terminal {
            Empty(_) => {
                *terminal = Node::Leaf(leaf);
                Ok(UpdateResult::Inserted)
            },
            Leaf(old_leaf) if old_leaf.key() == leaf.key() => {
                let old = mem::replace(old_leaf, leaf);
                let old_value = old.to_value_hash();
                Ok(UpdateResult::Updated(old_value))
            },
            Leaf(_) => {
                let old_leaf = mem::replace(terminal, Node::Empty(EmptyNode {})).to_leaf()?;
                let branch = old_leaf.build_tree(height + 1, leaf)?;
                *terminal = Node::Branch(branch);
                Ok(UpdateResult::Inserted)
            },
            _ => unreachable!(),
        }
    }

    // Classifies the type of deletion that should be performed on the terminal node. If the key is not in the tree,
    // there is nothing to delete and we return `KeyDoesNotExist`. If the key does exist, we need to know whether the
    // sibling node is a leaf (i.e. the parent is a `TerminalBranch`) or a branch (the parent is a
    // `NonTerminalBranch`). For terminal branches, the deletion logic is more complex as it is a reverse of the
    // `insert_or_update_leaf` logic for insertion.
    fn classify_deletion(&self, key: &NodeKey) -> Result<PathClassifier, SMTError> {
        let branch = self.parent.as_branch().ok_or(SMTError::UnexpectedNodeType)?;
        let terminal = branch.child(self.direction);
        let other_is_branch = branch.child(!self.direction).is_branch();
        if terminal.is_empty() {
            Ok(PathClassifier::KeyDoesNotExist)
        } else if terminal.is_branch() {
            Err(SMTError::InvalidTerminalNode)
        } else {
            let leaf = terminal.as_leaf().ok_or(SMTError::UnexpectedNodeType)?;
            match (leaf.key() == key, other_is_branch) {
                (false, _) => Ok(PathClassifier::KeyDoesNotExist),
                (true, false) => Ok(PathClassifier::TerminalBranch),
                (true, true) => Ok(PathClassifier::NonTerminalBranch),
            }
        }
    }

    // When deleting a node with a non-empty sibling, that node is an orphan and needs to be inserted higher up
    // in the tree at the highest spot where it would have a non-empty sibling.
    //
    // This function prunes that node and returns the height that it needs to be inserted into.
    fn prune(&mut self) -> Result<(Node<H>, usize), SMTError> {
        let branches_to_prune = self.empty_siblings.iter()
            .rev()
            // The last branch has two non-empty nodes by definition, so it's always F.
            .skip(1)
            .take_while(|b| **b)
            .count() +
            1; // Account for the last branch
        let parent = self.parent.as_branch_mut().ok_or(SMTError::UnexpectedNodeType)?;
        let depth = (parent.height() + 1)
            .checked_sub(branches_to_prune)
            .ok_or_else(|| SMTError::InvalidBranch("Logic error: Trying to prune beyond root".into()))?;
        let terminal = parent.child_mut(!self.direction);
        let orphan_node = mem::replace(terminal, Node::Empty(EmptyNode {}));
        Ok((orphan_node, depth))
    }

    // Replaces the terminal node with an Empty node, returning the deleted node
    fn delete(&mut self) -> Result<ValueHash, SMTError> {
        let branch = self.parent.as_branch_mut().ok_or(SMTError::UnexpectedNodeType)?;
        let terminal = branch.child_mut(self.direction);
        let old = mem::replace(terminal, Node::Empty(EmptyNode {}));
        let hash = old.to_leaf()?.to_value_hash();
        Ok(hash)
    }
}

impl<H: Digest<OutputSize = U32>> SparseMerkleTree<H> {
    /// Lazily returns the hash of the Sparse Merkle tree. This function requires a mutable reference to `self` in
    /// case the root node needs to be updated. If you are absolutely sure that the Merkle& root is correct and want a
    /// non-mutable reference, use [`SparseMerkleTree::unsafe_hash()`] instead.
    pub fn hash(&mut self) -> &NodeHash {
        self.root.hash()
    }

    /// Returns the hash of the Sparse Merkle tree. This function does not require a mutable reference to `self` but
    /// should only be used if you are absolutely sure that the Merkle& root is correct. Otherwise, use
    /// [`SparseMerkleTree::hash()`] instead.
    pub fn unsafe_hash(&self) -> &NodeHash {
        self.root.unsafe_hash()
    }

    /// Returns true if the entire Merkle tree is empty.
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    /// Attempts to delete the value at the location `key`. If the tree contains the key, the deleted value hash is
    /// returned. Otherwise, `KeyNotFound` is returned.
    pub fn delete(&mut self, key: &NodeKey) -> Result<DeleteResult, SMTError> {
        if self.is_empty() {
            return Ok(DeleteResult::KeyNotFound);
        }
        if self.root.is_leaf() {
            return self.delete_root(key);
        }
        let mut path = self.find_terminal_branch(key, false)?;
        let result = match path.classify_deletion(key)? {
            PathClassifier::KeyDoesNotExist => DeleteResult::KeyNotFound,
            PathClassifier::TerminalBranch => {
                let deleted = path
                    .terminal()
                    .as_leaf()
                    .ok_or(SMTError::UnexpectedNodeType)?
                    .value()
                    .clone();
                let (orphan, depth) = path.prune()?;
                self.attach_orphan_at_depth(key, depth, orphan)?;
                self.size -= 1;
                DeleteResult::Deleted(deleted)
            },
            PathClassifier::NonTerminalBranch => {
                let deleted_hash = path.delete()?;
                self.size -= 1;
                // Traverse the tree again, marking the path as stale
                let _node = self.find_terminal_branch(key, true)?;
                DeleteResult::Deleted(deleted_hash)
            },
        };
        Ok(result)
    }

    /// Update an existing node at location `key` in the tree, or, if the key does not exist, insert a new node at
    /// location `key` instead. Returns `Ok(UpdateResult::Updated)` if the node was updated, or
    /// `Ok(UpdateResult::Inserted)` if the node was inserted.
    ///
    /// `upsert` takes care of extending the tree if necessary, creating new branches until the new key can be inserted
    /// as a leaf node.
    ///
    /// The hash will be stale after a successful call to `upsert`. Do not call `unsafe_hash` directly after updating
    /// the tree.
    pub fn upsert(&mut self, key: NodeKey, value: ValueHash) -> Result<UpdateResult, SMTError> {
        let new_leaf = LeafNode::new(key, value);
        if self.is_empty() {
            self.root = Node::Leaf(new_leaf);
            self.size += 1;
            return Ok(UpdateResult::Inserted);
        }
        if self.root.is_leaf() {
            return self.upsert_root(new_leaf);
        }
        // Traverse the tree until we find either an empty node or a leaf node.
        let mut terminal_node = self.find_terminal_branch(new_leaf.key(), true)?;
        let result = terminal_node.insert_or_update_leaf(new_leaf)?;
        if let UpdateResult::Inserted = result {
            self.size += 1
        }
        Ok(result)
    }

    /// This will only add new node when it does not exist
    pub fn insert(&mut self, key: NodeKey, value: ValueHash) -> Result<UpdateResult, SMTError> {
        if self.get(&key)?.is_some() {
            return Err(SMTError::KeyExists);
        }
        // So we no know it does not exist, so lets add it.
        self.upsert(key, value)
    }

    /// Returns true if the tree contains the key `key`.
    pub fn contains(&self, key: &NodeKey) -> bool {
        match self.search_node(key) {
            // In the case of a malformed tree where the search fails unexpectedly, play it safe
            Err(_) => false,
            // The node is either empty or is a leaf with an unexpected key
            Ok(None) => false,
            // The node is a leaf with the expected key
            Ok(Some(_)) => true,
        }
    }

    /// Returns the value at location `key` if it exists, or `None` otherwise.
    pub fn get(&self, key: &NodeKey) -> Result<Option<&ValueHash>, SMTError> {
        let node = self.search_node(key)?;
        Ok(node.map(|n| n.as_leaf().unwrap().value()))
    }

    /// Construct the data structures needed to generate the Merkle& proofs. Although this function returns a struct
    /// of type `ExclusionProof` it is not really a valid (exclusion) proof. The constructors do additional
    /// validation before passing the structure on. For this reason, this method is `private` outside of the module.
    pub(crate) fn build_proof_candidate(&self, key: &NodeKey) -> Result<ExclusionProof<H>, SMTError> {
        let mut siblings = Vec::new();
        let mut current_node = &self.root;
        while current_node.is_branch() {
            let branch = current_node.as_branch().unwrap();
            if branch.is_stale() {
                return Err(SMTError::StaleHash);
            }
            let dir = traverse_direction(branch.height(), branch.key(), key)?;
            current_node = match dir {
                TraverseDirection::Left => {
                    siblings.push(branch.right().unsafe_hash().clone());
                    branch.left()
                },
                TraverseDirection::Right => {
                    siblings.push(branch.left().unsafe_hash().clone());
                    branch.right()
                },
            };
        }
        let leaf = current_node.as_leaf().cloned();
        let proof = ExclusionProof::new(siblings, leaf);
        Ok(proof)
    }

    // Finds the branch node above the terminal node. The case of "no parent" is already covered, so there will
    // always be a branch node
    fn find_terminal_branch(
        &mut self,
        child_key: &NodeKey,
        mark_stale: bool,
    ) -> Result<TerminalBranch<'_, H>, SMTError> {
        let mut parent_node = &mut self.root;
        let mut empty_siblings = Vec::new();
        if !parent_node.is_branch() {
            return Err(SMTError::UnexpectedNodeType);
        }
        let mut done = false;
        let mut traverse_dir = TraverseDirection::Left;
        while !done {
            let branch = parent_node.as_branch_mut().unwrap();
            if mark_stale {
                branch.mark_as_stale();
            }
            traverse_dir = traverse_direction(branch.height(), branch.key(), child_key)?;
            let next = match traverse_dir {
                TraverseDirection::Left => {
                    empty_siblings.push(branch.right().is_empty());
                    branch.left()
                },
                TraverseDirection::Right => {
                    empty_siblings.push(branch.left().is_empty());
                    branch.right()
                },
            };
            if next.is_branch() {
                parent_node = match traverse_dir {
                    TraverseDirection::Left => parent_node.as_branch_mut().unwrap().left_mut(),
                    TraverseDirection::Right => parent_node.as_branch_mut().unwrap().right_mut(),
                };
            } else {
                done = true;
            }
        }
        let terminal = TerminalBranch {
            parent: parent_node,
            direction: traverse_dir,
            empty_siblings,
        };
        Ok(terminal)
    }

    // Similar to `find_terminal_branch`, but does not require a mutable reference to self.
    fn search_node(&self, key: &NodeKey) -> Result<Option<&Node<H>>, SMTError> {
        let mut node = &self.root;
        loop {
            match node {
                Branch(branch) => {
                    let traverse_dir = traverse_direction(branch.height(), branch.key(), key)?;
                    node = branch.child(traverse_dir)
                },
                Leaf(leaf) => {
                    return if leaf.key() == key { Ok(Some(node)) } else { Ok(None) };
                },
                Empty(_) => {
                    return Ok(None);
                },
            }
        }
    }

    // Handles the case of deletion when the root is a leaf. If the keys match, the old hash value is returned.
    // Otherwise, the key is not in the tree and `KeyNotFound` is returned.
    fn delete_root(&mut self, key: &NodeKey) -> Result<DeleteResult, SMTError> {
        let leaf = self.root.as_leaf().ok_or(SMTError::UnexpectedNodeType)?;
        if leaf.key() == key {
            let leaf = mem::replace(&mut self.root, Node::Empty(EmptyNode {}));
            let leaf_hash = leaf.to_leaf()?.to_value_hash();
            self.size -= 1;
            Ok(DeleteResult::Deleted(leaf_hash))
        } else {
            Ok(DeleteResult::KeyNotFound)
        }
    }

    // Performs an update or insert if the root is a leaf.
    fn upsert_root(&mut self, new_leaf: LeafNode<H>) -> Result<UpdateResult, SMTError> {
        let leaf = self.root.as_leaf().ok_or(SMTError::UnexpectedNodeType)?;
        if leaf.key() == new_leaf.key() {
            let old_leaf = mem::replace(&mut self.root, Leaf(new_leaf)).to_leaf()?;
            return Ok(UpdateResult::Updated(old_leaf.to_value_hash()));
        }
        let old_root = mem::replace(&mut self.root, Empty(EmptyNode {})).to_leaf()?;
        let root = old_root.build_tree(0, new_leaf)?;
        self.root = Branch(root);
        self.size += 1;
        Ok(UpdateResult::Inserted)
    }

    // This function attaches a node to the branch at the specified height.
    fn attach_orphan_at_depth(&mut self, key: &NodeKey, height: usize, orphan: Node<H>) -> Result<(), SMTError> {
        if height == 0 {
            self.root = orphan;
            return Ok(());
        }
        let mut node = &mut self.root;
        for _ in 0..height {
            let branch = node.as_branch_mut().ok_or(SMTError::UnexpectedNodeType)?;
            branch.mark_as_stale();
            let traverse_dir = traverse_direction(branch.height(), branch.key(), key)?;
            node = branch.child_mut(traverse_dir);
        }
        *node = orphan;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use blake2::Blake2b;
    use digest::{consts::U32, generic_array::GenericArray, Digest};

    use crate::sparse_merkle_tree::{
        tree::{DeleteResult, SparseMerkleTree},
        NodeKey,
        SMTError,
        UpdateResult,
        ValueHash,
        EMPTY_NODE_HASH,
    };

    fn short_key(v: u8) -> NodeKey {
        let mut key = [0u8; 32];
        key[0] = v;
        NodeKey::from(key)
    }

    fn leaf_hash(k: &NodeKey, v: &ValueHash) -> GenericArray<u8, U32> {
        Blake2b::<U32>::new()
            .chain_update(b"V")
            .chain_update(k)
            .chain_update(v)
            .finalize()
    }

    fn branch_hash<B1, B2>(height: usize, key: &NodeKey, left: B1, right: B2) -> GenericArray<u8, U32>
    where
        B1: AsRef<[u8]>,
        B2: AsRef<[u8]>,
    {
        Blake2b::<U32>::default()
            .chain_update(b"B")
            .chain_update(height.to_le_bytes())
            .chain_update(key)
            .chain_update(left)
            .chain_update(right)
            .finalize()
    }

    #[test]
    fn empty_tree() {
        let tree = SparseMerkleTree::<Blake2b<U32>>::default();
        assert_eq!(tree.size(), 0);
        assert!(tree.root().is_empty());
    }

    #[test]
    fn zero_key() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let res = tree.upsert([0u8; 32].into(), [1u8; 32].into());
        assert!(res.is_ok());
    }

    #[test]
    fn single_node() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let key = short_key(1);
        let value = ValueHash::from([1u8; 32]);
        let res = tree.upsert(key.clone(), value.clone()).unwrap();
        assert!(matches!(res, UpdateResult::Inserted));
        assert_eq!(tree.size(), 1);

        assert!(tree.contains(&key));
        assert_eq!(tree.get(&key).unwrap().unwrap(), &value);
        assert_eq!(
            tree.root().unsafe_hash().to_string(),
            "f0ba9d3fa2b32a56d356b851098a2c7cb077f56735371f98eabccd5ffb9da689"
        );
    }

    #[test]
    fn single_node_same_key() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let key = short_key(1);
        let value = ValueHash::from([1u8; 32]);
        let _ = tree.upsert(key.clone(), value).unwrap();
        let value = ValueHash::from([2u8; 32]);
        let _res = tree.upsert(key.clone(), value.clone()).unwrap();
        assert!(tree.contains(&key));
        assert_eq!(tree.get(&key).unwrap().unwrap(), &value);
        assert_eq!(
            tree.root().unsafe_hash().to_string(),
            "ada3a4d0ac92222371c1ec9c3b53acfea4b81c0532d5140469af6867a6610396"
        );
    }

    #[test]
    fn simple_branch() {
        // A simple branch off the root with 2 leaf nodes
        //           +------+
        //     +-----+ root +-----+
        //     |     +------+     |
        //     |                  |
        // +---+----+         +---+----+
        // |0111: v1|         |1111: v2|
        // +--------+         +--------+
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let key1 = short_key(127);
        let value1 = ValueHash::from([1u8; 32]);
        let res = tree.upsert(key1.clone(), value1.clone()).unwrap();
        assert!(matches!(res, UpdateResult::Inserted));
        let key2 = short_key(255);
        let value2 = ValueHash::from([2u8; 32]);
        let res = tree.upsert(key2.clone(), value2.clone()).unwrap();
        assert!(matches!(res, UpdateResult::Inserted));

        assert!(tree.contains(&key1));
        assert!(tree.contains(&key2));
        let _ = tree.hash();
        let left_hash = leaf_hash(&key1, &value1);
        let right_hash = leaf_hash(&key2, &value2);
        let expected = branch_hash(0, &short_key(0), left_hash, right_hash);
        let root = tree.root();
        assert_eq!(root.unsafe_hash().to_string(), format!("{:x}", expected));
        assert!(root.is_branch());
        let left = root.as_branch().unwrap().left().as_leaf().unwrap();
        let right = root.as_branch().unwrap().right().as_leaf().unwrap();

        assert_eq!(tree.size(), 2);
        assert_eq!(left.key(), &key1);
        // Hash is 0d4789822d0861b4d17cad16df7a9c4d0689c0f65ffd7352d576243b38ab6539
        assert_eq!(left.hash().to_string(), format!("{left_hash:x}"));

        assert_eq!(right.key(), &key2);
        // Hash is e3f62f1bfccca2e03e3238cf22748d6a39a7e5eee1dd4b78e2fdd04b5c47d303
        assert_eq!(right.hash().to_string(), format!("{right_hash:x}"));

        // Update a key-value
        let old_hash = tree.unsafe_hash().to_string();
        let res = tree.upsert(key1, value2).unwrap();
        assert_eq!(tree.size(), 2);
        assert!(matches!(res, UpdateResult::Updated(v) if v == value1));
        assert_ne!(tree.hash().to_string(), old_hash);
    }

    #[test]
    fn deep_divergent_nodes() {
        // As with the simple branch test above, but now the keys only diverge several levels down
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        let key1 = short_key(79);
        let value1 = ValueHash::from([1u8; 32]);
        let res = tree.upsert(key1.clone(), value1.clone()).unwrap();
        assert!(matches!(res, UpdateResult::Inserted));
        let key2 = short_key(95);
        let value2 = ValueHash::from([2u8; 32]);
        let res = tree.upsert(key2.clone(), value2.clone()).unwrap();
        assert!(matches!(res, UpdateResult::Inserted));

        assert!(tree.contains(&key1));
        assert!(tree.contains(&key2));
        let _ = tree.hash();
        let left_hash = leaf_hash(&key1, &value1);
        let right_hash = leaf_hash(&key2, &value2);
        // The keys are (95) 0101.1111 and (79) 0100.1111, so the first 3 bits are the same
        // The tree should look like this after 2 inserts:
        //         ┌──────┐
        //      ┌──┤ root ├──┐
        //      │  └──────┘  │
        //     ┌┴┐          ┌┴┐
        //  ┌──┤ ├──┐       │0│
        //  │  └─┘  │       └─┘
        // ┌┴┐     ┌┴┐
        // │0│  ┌──┤ ├──┐
        // └─┘  │  └─┘  │
        //     ┌┴┐     ┌┴┐
        //   ┌─┤ ├─┐   │0│
        //   │ └─┘ │   └─┘
        //  ┌┴┐   ┌┴┐
        //  │A│   │B│
        //  └─┘   └─┘
        assert_eq!(tree.size(), 2);
        let root = tree.root().as_branch().unwrap();
        assert!(root.left().is_branch());
        assert!(root.right().is_empty());
        let level1 = root.left().as_branch().unwrap();
        assert!(level1.left().is_empty());
        let level2 = level1.right().as_branch().unwrap();
        assert!(level2.right().is_empty());
        let level3 = level2.left().as_branch().unwrap();
        assert!(level3.left().is_leaf());
        assert!(level3.right().is_leaf());
        assert_eq!(
            level3.left().as_leaf().unwrap().hash().to_string(),
            format!("{left_hash:x}")
        );
        assert_eq!(
            level3.right().as_leaf().unwrap().hash().to_string(),
            format!("{right_hash:x}")
        );
        // Calculate the root hash, starting from level 3
        let level3_hash = branch_hash(3, &short_key(64), left_hash, right_hash);
        assert_eq!(level3.unsafe_hash().to_string(), format!("{level3_hash:x}"));
        // Level 2
        let level2_hash = branch_hash(2, &short_key(64), level3_hash, &EMPTY_NODE_HASH);
        assert_eq!(level2.unsafe_hash().to_string(), format!("{level2_hash:x}"));
        // Level 1
        let level1_hash = branch_hash(1, &short_key(0), EMPTY_NODE_HASH, level2_hash);
        assert_eq!(level1.unsafe_hash().to_string(), format!("{level1_hash:x}"));
        // Root hash
        let root_hash = branch_hash(0, &short_key(0), level1_hash, EMPTY_NODE_HASH);
        let hash = tree.hash();
        assert_eq!(hash.to_string(), format!("{root_hash:x}"));

        // Now add C to right branch of root at key 11110000....
        // The tree should look like this after 3 inserts:
        //         ┌──────┐
        //      ┌──┤ root ├──┐
        //      │  └──────┘  │
        //     ┌┴┐          ┌┴┐
        //  ┌──┤ ├──┐       │C│
        //  │  └─┘  │       └─┘
        // ┌┴┐     ┌┴┐
        // │0│  ┌──┤ ├──┐
        // └─┘  │  └─┘  │
        //     ┌┴┐     ┌┴┐
        //   ┌─┤ ├─┐   │0│
        //   │ └─┘ │   └─┘
        //  ┌┴┐   ┌┴┐
        //  │A│   │B│
        //  └─┘   └─┘
        let key_c = short_key(240);
        let value_c = ValueHash::from([3u8; 32]);
        let res = tree.upsert(key_c.clone(), value_c.clone()).unwrap();
        assert!(matches!(res, UpdateResult::Inserted));
        assert!(tree.contains(&key_c));
        assert_eq!(tree.size(), 3);

        assert!(tree.root().as_branch().unwrap().is_stale());
        let hash = tree.hash();
        let hash_c = leaf_hash(&key_c, &value_c);
        let expected_hash = branch_hash(0, &short_key(0), level1_hash, hash_c);
        assert_eq!(hash.to_string(), format!("{expected_hash:x}"));
        // Now insert another value causing a cascade of branches down the right:
        //            ┌──────┐
        //      ┌─────┤ root ├─────┐
        //      │     └──────┘     │
        //     ┌┴┐                ┌┴┐1
        //  ┌──┤ ├──┐          ┌──┤ ├───┐
        //  │  └─┘  │          │  └─┘   │
        // ┌┴┐     ┌┴┐        ┌┴┐10    ┌┴┐11
        // │0│  ┌──┤ ├──┐     │0│    ┌─┤ ├─┐
        // └─┘  │  └─┘  │     └─┘    │ └─┘ │
        //     ┌┴┐     ┌┴┐       110┌┴┐   ┌┴┐111
        //   ┌─┤ ├─┐   │0│          │0│ ┌─┤ ├─┐
        //   │ └─┘ │   └─┘          └─┘ │ └─┘ │
        //  ┌┴┐   ┌┴┐                  ┌┴┐   ┌┴┐
        //  │A│   │B│                  │D│   │C│
        //  └─┘   └─┘                  └─┘   └─┘
        // 11100000 in decimal is 224, so the first 3 bits are the same
        let key4 = short_key(224);
        let value4 = ValueHash::from([4u8; 32]);
        let res = tree.upsert(key4.clone(), value4.clone()).unwrap();
        let root = tree.hash().to_string();
        assert!(matches!(res, UpdateResult::Inserted));
        assert!(tree.contains(&key4));
        assert_eq!(tree.size(), 4);

        let hash_d = leaf_hash(&key4, &value4);
        let level3_hash = branch_hash(3, &short_key(224), hash_d, hash_c);
        let level2_hash = branch_hash(2, &short_key(192), &EMPTY_NODE_HASH, level3_hash);
        let level1r_hash = branch_hash(1, &short_key(128), &EMPTY_NODE_HASH, level2_hash);
        let root_hash = branch_hash(0, &short_key(0), level1_hash, level1r_hash);

        let level1 = tree.root().as_branch().unwrap().right().as_branch().unwrap();
        let level2 = level1.right().as_branch().unwrap();
        let level3 = level2.right().as_branch().unwrap();

        assert_eq!(level3.unsafe_hash().to_string(), format!("{level3_hash:x}"));
        assert_eq!(level2.unsafe_hash().to_string(), format!("{level2_hash:x}"));
        assert_eq!(level1.unsafe_hash().to_string(), format!("{level1r_hash:x}"));
        assert_eq!(root, format!("{root_hash:x}"));
    }

    #[test]
    fn order_does_not_matter() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::new();
        tree.upsert(short_key(42), ValueHash::from([4u8; 32])).unwrap();
        tree.upsert(short_key(24), ValueHash::from([2u8; 32])).unwrap();
        let hash1 = tree.hash().clone();

        let mut tree = SparseMerkleTree::<Blake2b<U32>>::new();
        tree.upsert(short_key(24), ValueHash::from([2u8; 32])).unwrap();
        tree.upsert(short_key(42), ValueHash::from([4u8; 32])).unwrap();
        let hash2 = tree.hash().clone();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn delete_empty_tree() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::new();
        let key = short_key(42);
        assert!(matches!(tree.delete(&key), Ok(DeleteResult::KeyNotFound)));
    }

    #[test]
    fn delete_single_node() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::new();
        let key = short_key(42);
        let value = ValueHash::from([1u8; 32]);
        tree.upsert(key.clone(), value.clone()).unwrap();
        assert_eq!(tree.size(), 1);
        assert!(tree.contains(&key));
        let res = tree.delete(&key);
        assert!(matches!(res, Ok(DeleteResult::Deleted(node)) if node == value));
        assert_eq!(tree.size(), 0);
    }

    #[test]
    fn delete_deep_node() {
        // Start with
        //            ┌──────┐
        //      ┌─────┤ root ├─────┐
        //      │     └──────┘     │
        //     ┌┴┐                ┌┴┐1
        //  ┌──┤ ├──┐          ┌──┤ ├───┐
        //  │  └─┘  │          │  └─┘   │
        // ┌┴┐     ┌┴┐        ┌┴┐10    ┌┴┐11
        // │0│  ┌──┤ ├──┐     │0│    ┌─┤ ├─┐
        // └─┘  │  └─┘  │     └─┘    │ └─┘ │
        //     ┌┴┐     ┌┴┐       110┌┴┐   ┌┴┐111
        //   ┌─┤ ├─┐   │0│          │0│ ┌─┤ ├─┐
        //   │ └─┘ │   └─┘          └─┘ │ └─┘ │
        //  ┌┴┐   ┌┴┐                  ┌┴┐   ┌┴┐
        //  │A│   │B│                  │D│   │C│
        //  └─┘   └─┘                  └─┘   └─┘
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        tree.upsert(short_key(79), ValueHash::from([1u8; 32])).unwrap();
        tree.upsert(short_key(95), ValueHash::from([2u8; 32])).unwrap();
        tree.upsert(short_key(240), ValueHash::from([3u8; 32])).unwrap();
        tree.upsert(short_key(224), ValueHash::from([4u8; 32])).unwrap();

        // root hash is e88862dc2d50248e7830924c1c415e9789069ae451f9eb5e437fdd2d6dffd4dd
        assert_eq!(
            tree.hash().to_string(),
            "e88862dc2d50248e7830924c1c415e9789069ae451f9eb5e437fdd2d6dffd4dd"
        );

        // Now deleting D should yield
        //            ┌──────┐
        //      ┌─────┤ root ├─────┐
        //      │     └──────┘     │
        //     ┌┴┐                ┌┴┐
        //  ┌──┤ ├──┐             │C│
        //  │  └─┘  │             └─┘
        // ┌┴┐     ┌┴┐
        // │0│  ┌──┤ ├──┐
        // └─┘  │  └─┘  │
        //     ┌┴┐     ┌┴┐
        //   ┌─┤ ├─┐   │0│
        //   │ └─┘ │   └─┘
        //  ┌┴┐   ┌┴┐
        //  │A│   │B│
        //  └─┘   └─┘
        // Root hash is e693520b5ba4ff8b1e37ae4feabcb54701f32efd6bc4b78db356fa9baa64ca99

        // Deleting a key that does not exist is ok.
        let res = tree.delete(&short_key(5));
        assert!(matches!(res, Ok(DeleteResult::KeyNotFound)));

        // Delete an existing key
        let res = tree.delete(&short_key(224)).unwrap();
        assert_eq!(res, DeleteResult::Deleted(ValueHash::from([4u8; 32])));
        assert_eq!(
            tree.hash().to_string(),
            "e693520b5ba4ff8b1e37ae4feabcb54701f32efd6bc4b78db356fa9baa64ca99"
        );
        assert_eq!(tree.size(), 3);
    }

    #[test]
    fn delete_non_terminal_branch() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        tree.upsert(short_key(127), ValueHash::from([1u8; 32])).unwrap();
        tree.upsert(short_key(128), ValueHash::from([2u8; 32])).unwrap();
        tree.upsert(short_key(192), ValueHash::from([3u8; 32])).unwrap();
        let hash = tree.hash().clone();
        assert_eq!(
            hash.to_string(),
            "ecc68a20f30e6a1d05f75dd9b43504c409d05df5d43ab2e6243b459e2a83524b"
        );
        tree.delete(&short_key(127)).unwrap();
        let hash = tree.hash().clone();
        assert_eq!(
            hash.to_string(),
            "75809eedc0e809d07ddc0a0c44e6c5f50ae0554164d0260708ca668cbc44394d"
        );
    }

    #[test]
    fn delete_highly_branched() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();
        tree.upsert(short_key(65), ValueHash::from([1u8; 32])).unwrap();
        tree.upsert(short_key(79), ValueHash::from([1u8; 32])).unwrap();
        let hash2 = tree.hash().clone();
        tree.upsert(short_key(240), ValueHash::from([3u8; 32])).unwrap();
        tree.upsert(short_key(224), ValueHash::from([4u8; 32])).unwrap();
        let hash4 = tree.hash().clone();
        tree.upsert(short_key(95), ValueHash::from([2u8; 32])).unwrap();
        assert_eq!(tree.size(), 5);

        let res = tree.delete(&short_key(95)).unwrap();
        assert_eq!(res, DeleteResult::Deleted(ValueHash::from([2u8; 32])));
        assert_eq!(tree.hash(), &hash4);

        let _ = tree.delete(&short_key(240)).unwrap();
        let _ = tree.delete(&short_key(224)).unwrap();
        assert_eq!(tree.hash(), &hash2);
        let _ = tree.delete(&short_key(79)).unwrap();
        let _ = tree.delete(&short_key(65)).unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn contains() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();

        // An empty tree contains no keys
        assert!(!tree.contains(&short_key(0)));
        assert!(!tree.contains(&short_key(1)));

        // Add a key, which the tree must then contain
        tree.upsert(short_key(1), ValueHash::from([1u8; 32])).unwrap();
        assert!(!tree.contains(&short_key(0)));
        assert!(tree.contains(&short_key(1)));

        // Delete the key, which the tree must not contain
        tree.delete(&short_key(1)).unwrap();
        assert!(!tree.contains(&short_key(0)));
        assert!(!tree.contains(&short_key(1)));

        // Build a more complex tree with two keys, which the tree must then contain
        tree.upsert(short_key(0), ValueHash::from([0u8; 32])).unwrap();
        tree.upsert(short_key(1), ValueHash::from([1u8; 32])).unwrap();
        assert!(tree.contains(&short_key(0)));
        assert!(tree.contains(&short_key(1)));

        // Delete each key in turn
        tree.delete(&short_key(0)).unwrap();
        assert!(!tree.contains(&short_key(0)));
        tree.delete(&short_key(1)).unwrap();
        assert!(!tree.contains(&short_key(1)));

        // Sanity check that the tree is now empty
        assert!(tree.is_empty());
    }

    #[test]
    fn insert_only_if_exist() {
        let mut tree = SparseMerkleTree::<Blake2b<U32>>::default();

        // An empty tree contains no keys
        assert!(!tree.contains(&short_key(0)));
        assert!(!tree.contains(&short_key(1)));

        // Add a key, which the tree must then contain
        assert_eq!(
            tree.insert(short_key(1), ValueHash::from([1u8; 32])).unwrap(),
            UpdateResult::Inserted
        );
        assert!(!tree.contains(&short_key(0)));
        assert!(tree.contains(&short_key(1)));
        assert_eq!(
            tree.insert(short_key(1), ValueHash::from([2u8; 32])).unwrap_err(),
            SMTError::KeyExists
        );
        assert_eq!(tree.get(&short_key(1)).unwrap().unwrap(), &ValueHash::from([1u8; 32]));

        // Delete the key, which the tree must not contain
        tree.delete(&short_key(1)).unwrap();
        assert!(!tree.contains(&short_key(0)));
        assert!(!tree.contains(&short_key(1)));

        // Build a more complex tree with two keys, which the tree must then contain
        assert_eq!(
            tree.insert(short_key(0), ValueHash::from([0u8; 32])).unwrap(),
            UpdateResult::Inserted
        );
        assert_eq!(
            tree.insert(short_key(1), ValueHash::from([1u8; 32])).unwrap(),
            UpdateResult::Inserted
        );
        assert!(tree.contains(&short_key(0)));
        assert!(tree.contains(&short_key(1)));
        assert_eq!(
            tree.insert(short_key(0), ValueHash::from([1u8; 32])).unwrap_err(),
            SMTError::KeyExists
        );
        assert_eq!(tree.get(&short_key(0)).unwrap().unwrap(), &ValueHash::from([0u8; 32]));
        assert_eq!(
            tree.insert(short_key(1), ValueHash::from([2u8; 32])).unwrap_err(),
            SMTError::KeyExists
        );
        assert_eq!(tree.get(&short_key(1)).unwrap().unwrap(), &ValueHash::from([1u8; 32]));
    }
}
