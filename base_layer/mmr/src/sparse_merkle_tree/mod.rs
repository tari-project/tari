//! Sparse Merkle trees
//! A sparse Merkle tree is a Merkle tree where the non-empty nodes are stored in a map with the key being the hash
//! of the node and the value being the node itself.
//!
//! Merkle trees are a mutable Merkle-ish tree structure, that supports full CRUD operations. Inclusion _and_
//! exclusion proofs are supported, and are succinct.
//!
//! This implementation assumed that there is a key-value store elsewhere. Only the tree hashes themselves are
//! handled in this data structure.
//!
//! When constructing a new tree, a hashing algorithm is specified. This is used to hash the non-leaf nodes. The
//! "values" provided to the tree must already be a hash, and should have been generated from a different hashing
//! algorithm to the one driving the tree, in order to prevent second pre-image attacks.
//!
//! # Example
//!
//! Let's create a SMT with four nodes. We'll use the `Blake256` hash function to hash the nodes, and we'll use
//! the `ValueHash` type to represent the values. The `ValueHash` type is a wrapper around a `[u8; 32]` array.
//!
//! If we insert the nodes at
//!  * A: 01001111 (79 in decimal)
//!  * B: 01011111 (95 in decimal)
//!  * C: 11100000 (224 in decimal)
//!  * D: 11110000 (240 in decimal)
//! you will notice that they the first two diverge at the first bit, while the first and last pairs differ at the
//! fourth bit. This results in a SMT that looks like this:
//!
//! .           ┌──────┐
//!       ┌─────┤ root ├─────┐
//!       │     └──────┘     │
//!      ┌┴┐                ┌┴┐1
//!   ┌──┤ ├──┐          ┌──┤ ├───┐
//!   │  └─┘  │          │  └─┘   │
//!  ┌┴┐     ┌┴┐        ┌┴┐10    ┌┴┐11
//!  │0│  ┌──┤ ├──┐     │0│    ┌─┤ ├─┐
//!  └─┘  │  └─┘  │     └─┘    │ └─┘ │
//!      ┌┴┐     ┌┴┐       110┌┴┐   ┌┴┐111
//!    ┌─┤ ├─┐   │0│          │0│ ┌─┤ ├─┐
//!    │ └─┘ │   └─┘          └─┘ │ └─┘ │
//!   ┌┴┐   ┌┴┐                  ┌┴┐   ┌┴┐
//!   │A│   │B│                  │D│   │C│
//!   └─┘   └─┘                  └─┘   └─┘
//!
//! The merkle root is calculated by hashing nodes in the familiar way.
//! ```rust
//! use tari_crypto::hash::blake2::Blake256;
//! use tari_mmr::sparse_merkle_tree::{NodeKey, SparseMerkleTree, ValueHash};
//!
//! fn new_key(v: u8) -> NodeKey {
//!     let mut key = [0u8; 32];
//!     key[0] = v;
//!     NodeKey::from(key)
//! }
//! let mut tree = SparseMerkleTree::<Blake256>::default();
//! tree.upsert(new_key(79), ValueHash::from([1u8; 32]))
//!     .unwrap();
//! tree.upsert(new_key(95), ValueHash::from([2u8; 32]))
//!     .unwrap();
//! tree.upsert(new_key(240), ValueHash::from([3u8; 32]))
//!     .unwrap();
//! tree.upsert(new_key(224), ValueHash::from([4u8; 32]))
//!     .unwrap();
//! assert_eq!(
//!     tree.hash().to_string(),
//!     "e88862dc2d50248e7830924c1c415e9789069ae451f9eb5e437fdd2d6dffd4dd"
//! );
//!
//! // Deleting nodes is also supported
//! tree.delete(&new_key(224)).unwrap();
//! tree.delete(&new_key(95)).unwrap();
//! tree.delete(&new_key(240)).unwrap();
//! tree.delete(&new_key(79)).unwrap();
//! assert!(tree.is_empty());
//! ```
//! Copyright 2023. The Tari Project
//! SPDX-License-Identifier: BSD-3-Clause

mod bit_utils;
mod error;
mod node;
mod proofs;
mod tree;

pub use error::SMTError;
pub use node::{BranchNode, EmptyNode, LeafNode, Node, NodeHash, NodeKey, ValueHash, EMPTY_NODE_HASH};
pub use proofs::{ExclusionProof, InclusionProof};
pub use tree::{SparseMerkleTree, UpdateResult};
