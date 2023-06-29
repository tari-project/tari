// Copyright 2023. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryFrom,
    fmt::{Debug, Formatter},
    marker::PhantomData,
};

use digest::{consts::U32, Digest};

use crate::sparse_merkle_tree::{
    bit_utils::{count_common_prefix, get_bit, height_key, TraverseDirection},
    Node::*,
    SMTError,
};

macro_rules! hash_type {
    ($name: ident) => {
        /// A wrapper around a 32-byte hash value. Provides convenience functions to display as hex or binary
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; 32]);

        impl $name {
            pub fn as_slice(&self) -> &[u8] {
                &self.0
            }

            pub fn as_mut_slice(&mut self) -> &mut [u8] {
                &mut self.0
            }

            pub fn len(&self) -> usize {
                self.0.len()
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self([0; 32])
            }
        }

        impl std::convert::TryFrom<&[u8]> for $name {
            type Error = SMTError;

            fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
                if value.len() < 32 {
                    return Err(SMTError::ArrayTooShort(value.len()));
                }
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(value);
                Ok(Self(bytes))
            }
        }

        impl From<[u8; 32]> for $name {
            fn from(arr: [u8; 32]) -> Self {
                Self(arr)
            }
        }

        impl From<&[u8; 32]> for $name {
            fn from(arr: &[u8; 32]) -> Self {
                Self(arr.clone())
            }
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }

        impl std::fmt::UpperHex for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.iter().try_for_each(|b| write!(f, "{:02X}", b))
            }
        }

        impl std::fmt::LowerHex for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.iter().try_for_each(|b| write!(f, "{:02x}", b))
            }
        }

        impl std::fmt::Binary for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.iter().try_for_each(|b| write!(f, "{:08b}", b))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                if f.alternate() {
                    write!(f, "{:b}", self)
                } else {
                    write!(f, "{:x}", self)
                }
            }
        }
    };
}

hash_type!(NodeHash);
hash_type!(ValueHash);
hash_type!(NodeKey);

pub const EMPTY_NODE_HASH: NodeHash = NodeHash([0; 32]);

#[derive(Debug)]
pub enum Node<H> {
    Empty(EmptyNode),
    Leaf(LeafNode<H>),
    Branch(BranchNode<H>),
}

impl<H> Clone for Node<H> {
    fn clone(&self) -> Self {
        match self {
            Empty(n) => Empty(n.clone()),
            Leaf(n) => Leaf(n.clone()),
            Branch(_) => panic!("Branch nodes cannot be cloned"),
        }
    }
}

impl<H> Node<H> {
    /// A non-mutable version of [`Node::hash`], which you can use if you _absolutely know_ that the hash is correct.
    /// This would be the case for Empty or Leaf nodes, but you should never call this if the node might be a branch
    /// node.
    pub fn unsafe_hash(&self) -> &NodeHash {
        match self {
            Empty(n) => n.hash(),
            Leaf(n) => n.hash(),
            Branch(n) => n.unsafe_hash(),
        }
    }

    /// Returns true if the node is empty, false otherwise.
    pub fn is_empty(&self) -> bool {
        matches!(self, Node::Empty(_))
    }

    /// Returns true if the node is a leaf, false otherwise.
    pub fn is_leaf(&self) -> bool {
        matches!(self, Node::Leaf(_))
    }

    /// Returns true if the node is a branch, false otherwise.
    pub fn is_branch(&self) -> bool {
        matches!(self, Node::Branch(_))
    }

    /// Casts the node as a branch node, if it is one.
    pub fn as_branch(&self) -> Option<&BranchNode<H>> {
        match self {
            Node::Branch(n) => Some(n),
            _ => None,
        }
    }

    /// Casts the node as a mutable branch node, if it is one.
    pub fn as_branch_mut(&mut self) -> Option<&mut BranchNode<H>> {
        match self {
            Branch(n) => Some(n),
            _ => None,
        }
    }

    /// Casts the node as a leaf node, if it is one.
    pub fn as_leaf(&self) -> Option<&LeafNode<H>> {
        match self {
            Leaf(n) => Some(n),
            _ => None,
        }
    }

    pub fn to_leaf(self) -> Result<LeafNode<H>, SMTError> {
        match self {
            Leaf(n) => Ok(n),
            _ => Err(SMTError::UnexpectedNodeType),
        }
    }

    /// Indicates whether the node is semi-terminal, i.e. whether it is a leaf or empty node, or if a branch, if it is
    /// the last branch in the sub-tree.
    pub fn is_semi_terminal(&self) -> bool {
        match self {
            Leaf(_) | Empty(_) => true,
            Branch(n) => !n.left.is_branch() && !n.right.is_branch(),
        }
    }
}

impl<H: Digest<OutputSize = U32>> Node<H> {
    /// Returns the hash of the node. This is a convenience function that calls the appropriate hash function for the
    /// node type. For empty nodes, this is the empty node hash.
    /// For performance reasons, the function will lazily evaluate the hash of a branch node, which is why it takes
    /// `&mut self`. If you need a read-only version of this function and **know** that the hash is correct, you can
    /// use [`Node::unsafe_hash`] instead.
    pub fn hash(&mut self) -> &NodeHash {
        match self {
            Empty(n) => n.hash(),
            Leaf(n) => n.hash(),
            Branch(n) => n.hash(),
        }
    }
}

//-------------------------------------       Empty Node     -----------------------------------------------------------

/// An empty node. All empty nodes have the same hash, which acts as a marker value for truncated portions of the tree.
#[derive(Clone, Debug)]
pub struct EmptyNode {}

impl EmptyNode {
    pub fn hash(&self) -> &'static NodeHash {
        &EMPTY_NODE_HASH
    }
}

//-------------------------------------       Leaf Node     -----------------------------------------------------------
pub struct LeafNode<H> {
    key: NodeKey,
    hash: NodeHash,
    value: ValueHash,
    hash_type: PhantomData<H>,
}

impl<H> Clone for LeafNode<H> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            hash: self.hash.clone(),
            value: self.value.clone(),
            hash_type: PhantomData,
        }
    }
}

impl<H: Debug> Debug for LeafNode<H> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeafNode")
            .field("key", &self.key.to_string())
            .field("hash", &self.hash.to_string())
            .field("value", &self.value.to_string())
            .finish()
    }
}

impl<H> LeafNode<H> {
    pub fn key(&self) -> &NodeKey {
        &self.key
    }

    pub fn hash(&self) -> &NodeHash {
        &self.hash
    }

    pub fn value(&self) -> &ValueHash {
        &self.value
    }

    pub fn to_value_hash(self) -> ValueHash {
        self.value
    }
}

impl<H: Digest<OutputSize = U32>> LeafNode<H> {
    pub fn new(key: NodeKey, value: ValueHash) -> Self {
        let hash = Self::hash_value(&key, &value);
        Self {
            key,
            hash,
            value,
            hash_type: PhantomData,
        }
    }

    pub fn hash_value(key: &NodeKey, value: &ValueHash) -> NodeHash {
        let hasher = H::new();
        let hash = hasher
            .chain(b"V")
            .chain(key.as_slice())
            .chain(value.as_slice())
            .finalize();
        let mut result = [0; 32];
        result.copy_from_slice(hash.as_slice());
        result.into()
    }

    /// Replaces this leaf node with a new tree that starts at the given height and branches until the given sibling
    /// node is on the opposite side of the branch.
    pub fn build_tree(self, height: usize, sibling: LeafNode<H>) -> Result<BranchNode<H>, SMTError> {
        let diverge_height = count_common_prefix(&self.key, &sibling.key);
        let num_branches = match diverge_height.checked_sub(height) {
            Some(n) => n + 1,
            None => {
                let msg = format!("Diverge height {diverge_height} is less than height {height}");
                return Err(SMTError::InvalidBranch(msg));
            },
        };
        let root_key = height_key(&self.key, height);
        if num_branches == 1 {
            let (left, right) = if self.key < sibling.key {
                (Leaf(self), Leaf(sibling))
            } else {
                (Leaf(sibling), Leaf(self))
            };
            let root = BranchNode::new(height, root_key, left, right)?;
            Ok(root)
        } else {
            let (left, right) = if get_bit(self.key.as_slice(), height) == 0 {
                (Branch(self.build_tree(height + 1, sibling)?), Empty(EmptyNode {}))
            } else {
                (Empty(EmptyNode {}), Branch(self.build_tree(height + 1, sibling)?))
            };
            let root = BranchNode::new(height, root_key, left, right)?;
            Ok(root)
        }
    }
}

//-------------------------------------       Branch Node     ----------------------------------------------------------
pub struct BranchNode<H> {
    // The height of the branch. It is also the number of bits that all keys below this branch share.
    height: usize,
    // Only the first `height` bits of the key are relevant for this branch.
    key: NodeKey,
    hash: NodeHash,
    // Flag to indicate that the tree hash changed somewhere below this branch. and that the hash should be
    // recalculated.
    is_hash_stale: bool,
    left: Box<Node<H>>,
    right: Box<Node<H>>,
    hash_type: PhantomData<H>,
}

impl<H: Debug> Debug for BranchNode<H> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BranchNode")
            .field("height", &self.height)
            .field("key", &self.key.to_string())
            .field("hash", &self.hash.to_string())
            .field("is_hash_stale", &self.is_hash_stale)
            .field("left", &self.left)
            .field("right", &self.right)
            .finish()
    }
}

impl<H> BranchNode<H> {
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn key(&self) -> &NodeKey {
        &self.key
    }

    pub fn child(&self, direction: TraverseDirection) -> &Node<H> {
        match direction {
            TraverseDirection::Left => &self.left,
            TraverseDirection::Right => &self.right,
        }
    }

    pub fn child_mut(&mut self, direction: TraverseDirection) -> &mut Node<H> {
        match direction {
            TraverseDirection::Left => &mut self.left,
            TraverseDirection::Right => &mut self.right,
        }
    }

    pub fn left(&self) -> &Node<H> {
        &self.left
    }

    pub fn right(&self) -> &Node<H> {
        &self.right
    }

    pub(crate) fn left_mut(&mut self) -> &mut Node<H> {
        &mut self.left
    }

    pub(crate) fn right_mut(&mut self) -> &mut Node<H> {
        &mut self.right
    }

    pub fn unsafe_hash(&self) -> &NodeHash {
        &self.hash
    }

    /// Can be used to check if the hash needs to be recalculated.
    pub fn is_stale(&self) -> bool {
        self.is_hash_stale
    }

    pub(crate) fn mark_as_stale(&mut self) {
        self.is_hash_stale = true;
    }
}

impl<H: Digest<OutputSize = U32>> BranchNode<H> {
    pub fn new(height: usize, key: NodeKey, left: Node<H>, right: Node<H>) -> Result<Self, SMTError> {
        match (&left, &right) {
            (Empty(_), Empty(_)) => Err(SMTError::InvalidBranch(
                "Both left and right nodes are empty".to_string(),
            )),
            (Empty(_), Leaf(_)) | (Leaf(_), Empty(_)) => Err(SMTError::InvalidBranch(
                "A branch node cannot an empty node and leaf node as children".into(),
            )),
            (Leaf(_) | Branch(_), Leaf(_) | Branch(_)) | (Empty(_), Branch(_)) | (Branch(_), Empty(_)) => Ok(Self {
                height,
                key,
                hash: NodeHash::default(),
                is_hash_stale: true,
                left: Box::new(left),
                right: Box::new(right),
                hash_type: PhantomData,
            }),
        }
    }

    pub fn hash(&mut self) -> &NodeHash {
        if self.is_hash_stale {
            self.recalculate_hash();
        }
        &self.hash
    }

    pub fn branch_hash(height: usize, key: &NodeKey, left: &NodeHash, right: &NodeHash) -> NodeHash {
        let hasher = H::new();
        let hash = hasher
            .chain(b"B")
            .chain(height.to_le_bytes())
            .chain(key)
            .chain(left)
            .chain(right)
            .finalize();
        // Output is guaranteed to be 32 bytes at compile time due to trait constraint on `H`
        NodeHash::try_from(hash.as_slice()).unwrap()
    }

    fn recalculate_hash(&mut self) {
        let hash = Self::branch_hash(self.height, &self.key, self.left.hash(), self.right.hash());
        self.hash = hash;
        self.is_hash_stale = false;
    }
}

#[cfg(test)]
mod test {
    use rand::{self, RngCore};
    use tari_crypto::hash::blake2::Blake256;

    use super::*;

    fn random_arr() -> [u8; 32] {
        let mut result = [0; 32];
        rand::thread_rng().fill_bytes(&mut result);
        result
    }

    fn random_key() -> NodeKey {
        NodeKey::from(random_arr())
    }

    fn random_value_hash() -> ValueHash {
        ValueHash::from(random_arr())
    }

    #[test]
    fn empty_node() {
        assert_eq!(EmptyNode {}.hash(), &EMPTY_NODE_HASH);
    }

    #[test]
    fn leaf_node() {
        let key = random_key();
        let value = random_value_hash();
        let node = LeafNode::<Blake256>::new(key, value);
        let expect = Blake256::new()
            .chain(b"V")
            .chain(node.key())
            .chain(node.value())
            .finalize();
        let expect = NodeHash::try_from(expect.as_slice()).unwrap();
        assert_eq!(node.hash(), &expect);
    }

    #[test]
    fn branch_empty_leaf() {
        let left = Node::Empty(EmptyNode {});
        let right = Node::Leaf(LeafNode::<Blake256>::new(random_key(), random_value_hash()));
        let branch = BranchNode::<Blake256>::new(0, random_key(), left, right);
        // Should not be allowed - since this can be represented as a leaf node
        assert!(matches!(branch, Err(SMTError::InvalidBranch(_))));

        let left = Node::Leaf(LeafNode::<Blake256>::new(random_key(), random_value_hash()));
        let right = Node::Empty(EmptyNode {});
        let branch = BranchNode::<Blake256>::new(0, random_key(), left, right);
        // Should not be allowed - since this can be represented as a leaf node
        assert!(matches!(branch, Err(SMTError::InvalidBranch(_))));
    }

    #[test]
    fn branch_leaf_leaf() {
        let left = Node::Leaf(LeafNode::<Blake256>::new(random_key(), random_value_hash()));
        let right = Node::Leaf(LeafNode::<Blake256>::new(random_key(), random_value_hash()));
        let l_hash = left.unsafe_hash().clone();
        let r_hash = right.unsafe_hash().clone();
        let mut branch = BranchNode::<Blake256>::new(0, random_key(), left, right).unwrap();
        let expected = Blake256::new()
            .chain(b"B")
            .chain(branch.height().to_le_bytes())
            .chain(branch.key())
            .chain(l_hash)
            .chain(r_hash)
            .finalize();
        assert_eq!(branch.hash().as_slice(), expected.as_slice());
    }
}
