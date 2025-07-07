//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

// Copyright 2021 Radix Publishing Ltd incorporated in Jersey (Channel Islands).
//
// Licensed under the Radix License, Version 1.0 (the "License"); you may not use this
// file except in compliance with the License. You may obtain a copy of the License at:
//
// radixfoundation.org/licenses/LICENSE-v1
//
// The Licensor hereby grants permission for the Canonical version of the Work to be
// published, distributed and used under or by reference to the Licensor's trademark
// Radix ® and use of any unregistered trade names, logos or get-up.
//
// The Licensor provides the Work (and each Contributor provides its Contributions) on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied,
// including, without limitation, any warranties or conditions of TITLE, NON-INFRINGEMENT,
// MERCHANTABILITY, or FITNESS FOR A PARTICULAR PURPOSE.
//
// Whilst the Work is capable of being deployed, used and adopted (instantiated) to create
// a distributed ledger it is your responsibility to test and validate the code, together
// with all logic and performance of that code under all foreseeable scenarios.
//
// The Licensor does not make or purport to make and hereby excludes liability for all
// and any representation, warranty or undertaking in any form whatsoever, whether express
// or implied, to any entity or person, including any representation, warranty or
// undertaking, as to the functionality security use, value or other characteristics of
// any distributed ledger nor in respect the functioning or value of any tokens which may
// be created stored or transferred using the Work. The Licensor does not warrant that the
// Work or any use of the Work complies with any law or regulation in any territory where
// it may be implemented or used or that it will be appropriate for any specific purpose.
//
// Neither the licensor nor any current or former employees, officers, directors, partners,
// trustees, representatives, agents, advisors, contractors, or volunteers of the Licensor
// shall be liable for any direct or indirect, special, incidental, consequential or other
// losses of any kind, in tort, contract or otherwise (including but not limited to loss
// of revenue, income or profits, or loss of use or data, or loss of reputation, or loss
// of any economic or other opportunity of whatsoever nature or howsoever arising), arising
// out of or in connection with (without limitation of any use, misuse, of any ledger system
// or use made or its functionality or any performance or operation of any code or protocol
// caused by bugs or programming or logic errors or otherwise);
//
// A. any offer, purchase, holding, use, sale, exchange or transmission of any
// cryptographic keys, tokens or assets created, exchanged, stored or arising from any
// interaction with the Work;
//
// B. any failure in a transmission or loss of any token or assets keys or other digital
// artefacts due to errors in transmission;
//
// C. bugs, hacks, logic errors or faults in the Work or any communication;
//
// D. system software or apparatus including but not limited to losses caused by errors
// in holding or transmitting tokens by any third-party;
//
// E. breaches or failure of security including hacker attacks, loss or disclosure of
// password, loss of private key, unauthorised use or misuse of such passwords or keys;
//
// F. any losses including loss of anticipated savings or other benefits resulting from
// use of the Work or any changes to the Work (however implemented).
//
// You are solely responsible for; testing, validating and evaluation of all operation
// logic, functionality, security and appropriateness of using the Work for any commercial
// or non-commercial purpose and for any reproduction or redistribution by You of the
// Work. You assume all risks associated with Your use of the Work and the exercise of
// permissions under this License.

// This file contains code sourced from https://github.com/aptos-labs/aptos-core/tree/1.0.4
// This original source is licensed under https://github.com/aptos-labs/aptos-core/blob/1.0.4/LICENSE
//
// The code in this file has been implemented by Radix® pursuant to an Apache 2 licence and has
// been modified by Radix® and is now licensed pursuant to the Radix® Open-Source Licence.
//
// Each sourced code fragment includes an inline attribution to the original source file in a
// comment starting "SOURCE: ..."
//
// Modifications from the original source are captured in two places:
// * Initial changes to get the code functional/integrated are marked by inline "INITIAL-MODIFICATION: ..." comments
// * Subsequent changes to the code are captured in the git commit history
//
// The following notice is retained from the original source
// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, fmt::Display, ops::Range};

use borsh::{BorshDeserialize, BorshSerialize};
use digest::consts;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tari_crypto::hash_domain;
use tari_hashing::layer2::{tari_hasher32, TariDomainHasher};

use crate::{bit_iter::BitIterator, error::JmtProofVerifyError, store::TreeStoreReader, TreeHash};

hash_domain!(JmtHashDomain, "com.tari.jmt", 0);

pub type JmtHasher = TariDomainHasher<JmtHashDomain, consts::U32>;

fn jmt_node_hasher() -> JmtHasher {
    tari_hasher32::<JmtHashDomain>("Node")
}

pub fn jmt_node_hash<T: BorshSerialize>(data: &T) -> TreeHash {
    jmt_node_hasher().chain(data).finalize_into_array().into()
}

pub fn jmt_node_hash2(d1: &TreeHash, d2: &TreeHash) -> TreeHash {
    jmt_node_hasher().chain(d1).chain(d2).finalize_into_array().into()
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/proof/definition.rs#L182
/// A more detailed version of `SparseMerkleProof` with the only difference that all the leaf
/// siblings are explicitly set as `SparseMerkleLeafNode` instead of its hash value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SparseMerkleProofExt {
    leaf: Option<SparseMerkleLeafNode>,
    /// All siblings in this proof, including the default ones. Siblings are ordered from the bottom
    /// level to the root level.
    siblings: Vec<NodeInProof>,
}

impl SparseMerkleProofExt {
    /// Constructs a new `SparseMerkleProofExt` using leaf and a list of sibling nodes.
    pub(crate) fn new(leaf: Option<SparseMerkleLeafNode>, siblings: Vec<NodeInProof>) -> Self {
        Self { leaf, siblings }
    }

    /// Returns the leaf node in this proof.
    pub fn leaf(&self) -> Option<SparseMerkleLeafNode> {
        self.leaf.clone()
    }

    /// Returns the list of siblings in this proof.
    pub fn siblings(&self) -> &[NodeInProof] {
        &self.siblings
    }

    /// Verifies an element whose key is `element_key` and value is `element_value` exists in the Sparse Merkle Tree
    /// using the provided proof
    pub fn verify_inclusion(
        &self,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
        element_value_hash: &TreeHash,
    ) -> Result<(), JmtProofVerifyError> {
        self.verify(expected_root_hash, element_key, Some(element_value_hash))
    }

    /// Verifies the proof is a valid non-inclusion proof that shows this key doesn't exist in the tree.
    pub fn verify_exclusion(
        &self,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
    ) -> Result<(), JmtProofVerifyError> {
        self.verify(expected_root_hash, element_key, None)
    }

    /// If `element_value` is present, verifies an element whose key is `element_key` and value is
    /// `element_value` exists in the Sparse Merkle Tree using the provided proof. Otherwise,
    /// verifies the proof is a valid non-inclusion proof that shows this key doesn't exist in the
    /// tree.
    fn verify(
        &self,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
        element_value: Option<&TreeHash>,
    ) -> Result<(), JmtProofVerifyError> {
        if self.siblings.len() > 256 {
            return Err(JmtProofVerifyError::TooManySiblings {
                num_siblings: self.siblings.len(),
            });
        }

        match (element_value, &self.leaf) {
            (Some(value_hash), Some(leaf)) => {
                // This is an inclusion proof, so the key and value hash provided in the proof
                // should match element_key and element_value_hash. `siblings` should prove the
                // route from the leaf node to the root.
                if element_key != leaf.key() {
                    return Err(JmtProofVerifyError::KeyMismatch {
                        actual_key: *leaf.key(),
                        expected_key: *element_key,
                    });
                }
                if *value_hash != leaf.value_hash {
                    return Err(JmtProofVerifyError::ValueMismatch {
                        actual: leaf.value_hash,
                        expected: *value_hash,
                    });
                }
            },
            (Some(_), None) => return Err(JmtProofVerifyError::ExpectedInclusionProof),
            (None, Some(leaf)) => {
                // This is a non-inclusion proof. The proof intends to show that if a leaf node
                // representing `element_key` is inserted, it will break a currently existing leaf
                // node represented by `proof_key` into a branch. `siblings` should prove the
                // route from that leaf node to the root.
                if element_key == leaf.key() {
                    return Err(JmtProofVerifyError::ExpectedNonInclusionProof);
                }
                if element_key.common_prefix_bits_len(leaf.key()) < self.siblings.len() {
                    return Err(JmtProofVerifyError::InvalidNonInclusionProof);
                }
            },
            (None, None) => {
                // This is a non-inclusion proof. The proof intends to show that if a leaf node
                // representing `element_key` is inserted, it will show up at a currently empty
                // position. `sibling` should prove the route from this empty position to the root.
            },
        }

        let current_hash = self
            .leaf
            .clone()
            .map_or(SPARSE_MERKLE_PLACEHOLDER_HASH, |leaf| leaf.hash());
        let actual_root_hash = self
            .siblings
            .iter()
            .zip(element_key.iter_bits().rev().skip(256 - self.siblings.len()))
            .fold(current_hash, |hash, (sibling_node, bit)| {
                if bit {
                    SparseMerkleInternalNode::new(sibling_node.hash(), hash).hash()
                } else {
                    SparseMerkleInternalNode::new(hash, sibling_node.hash()).hash()
                }
            });

        if actual_root_hash != *expected_root_hash {
            return Err(JmtProofVerifyError::RootHashMismatch {
                actual_root_hash,
                expected_root_hash: *expected_root_hash,
            });
        }

        Ok(())
    }
}

impl From<SparseMerkleProofExt> for SparseMerkleProof {
    fn from(proof_ext: SparseMerkleProofExt) -> Self {
        Self::new(
            proof_ext.leaf,
            proof_ext.siblings.into_iter().map(|node| node.hash()).collect(),
        )
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/proof/definition.rs#L135
impl SparseMerkleProof {
    /// Constructs a new `SparseMerkleProof` using leaf and a list of siblings.
    pub fn new(leaf: Option<SparseMerkleLeafNode>, siblings: Vec<TreeHash>) -> Self {
        SparseMerkleProof { leaf, siblings }
    }

    /// Returns the leaf node in this proof.
    pub fn leaf(&self) -> Option<SparseMerkleLeafNode> {
        self.leaf.clone()
    }

    /// Returns the list of siblings in this proof.
    pub fn siblings(&self) -> &[TreeHash] {
        &self.siblings
    }
}

/// A proof that can be used to authenticate an element in a Sparse Merkle Tree given trusted root
/// hash. For example, `TransactionInfoToAccountProof` can be constructed on top of this structure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseMerkleProof {
    /// This proof can be used to authenticate whether a given leaf exists in the tree or not.
    ///     - If this is `Some(leaf_node)`
    ///         - If `leaf_node.key` equals requested key, this is an inclusion proof and `leaf_node.value_hash` equals
    ///           the hash of the corresponding account blob.
    ///         - Otherwise this is a non-inclusion proof. `leaf_node.key` is the only key that exists in the subtree
    ///           and `leaf_node.value_hash` equals the hash of the corresponding account blob.
    ///     - If this is `None`, this is also a non-inclusion proof which indicates the subtree is empty.
    leaf: Option<SparseMerkleLeafNode>,

    /// All siblings in this proof, including the default ones. Siblings are ordered from the bottom
    /// level to the root level.
    siblings: Vec<TreeHash>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum NodeInProof {
    Leaf(SparseMerkleLeafNode),
    Other(TreeHash),
}

impl From<TreeHash> for NodeInProof {
    fn from(hash: TreeHash) -> Self {
        Self::Other(hash)
    }
}

impl From<SparseMerkleLeafNode> for NodeInProof {
    fn from(leaf: SparseMerkleLeafNode) -> Self {
        Self::Leaf(leaf)
    }
}

impl NodeInProof {
    pub fn hash(&self) -> TreeHash {
        match self {
            Self::Leaf(leaf) => leaf.hash(),
            Self::Other(hash) => *hash,
        }
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/proof/definition.rs#L681
/// Note: this is not a range proof in the sense that a range of nodes is verified!
/// Instead, it verifies the entire left part of the tree up to a known rightmost node.
/// See the description below.
///
/// A proof that can be used to authenticate a range of consecutive leaves, from the leftmost leaf to
/// the rightmost known one, in a sparse Merkle tree. For example, given the following sparse Merkle tree:
///
/// ```text
///                   root
///                  /     \
///                 /       \
///                /         \
///               o           o
///              / \         / \
///             a   o       o   h
///                / \     / \
///               o   d   e   X
///              / \         / \
///             b   c       f   g
/// ```
///
/// if the proof wants show that `[a, b, c, d, e]` exists in the tree, it would need the siblings
/// `X` and `h` on the right.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparseMerkleRangeProof {
    /// The vector of siblings on the right of the path from root to last leaf. The ones near the
    /// bottom are at the beginning of the vector. In the above example, it's `[X, h]`.
    right_siblings: Vec<TreeHash>,
}

impl SparseMerkleRangeProof {
    /// Constructs a new `SparseMerkleRangeProof`.
    pub fn new(right_siblings: Vec<TreeHash>) -> Self {
        Self { right_siblings }
    }

    /// Returns the right siblings.
    pub fn right_siblings(&self) -> &[TreeHash] {
        &self.right_siblings
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/proof/mod.rs#L97
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SparseMerkleLeafNode {
    key: LeafKey,
    value_hash: TreeHash,
}

impl SparseMerkleLeafNode {
    pub fn new(key: LeafKey, value_hash: TreeHash) -> Self {
        SparseMerkleLeafNode { key, value_hash }
    }

    pub fn key(&self) -> &LeafKey {
        &self.key
    }

    pub fn value_hash(&self) -> &TreeHash {
        &self.value_hash
    }

    pub fn hash(&self) -> TreeHash {
        jmt_node_hash2(&self.key.bytes, &self.value_hash)
    }
}

pub struct SparseMerkleInternalNode {
    left_child: TreeHash,
    right_child: TreeHash,
}

impl SparseMerkleInternalNode {
    pub fn new(left_child: TreeHash, right_child: TreeHash) -> Self {
        Self {
            left_child,
            right_child,
        }
    }

    fn hash(&self) -> TreeHash {
        jmt_node_hash2(&self.left_child, &self.right_child)
    }
}

// INITIAL-MODIFICATION: we propagate usage of our own `Hash` (instead of Aptos' `HashValue`) to avoid
// sourcing the entire https://github.com/aptos-labs/aptos-core/blob/1.0.4/crates/aptos-crypto/src/hash.rs
pub const SPARSE_MERKLE_PLACEHOLDER_HASH: TreeHash = TreeHash::zero();

// CSOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/crates/aptos-crypto/src/hash.rs#L422
/// An iterator over `LeafKey` that generates one bit for each iteration.
pub struct LeafKeyBitIterator<'a> {
    /// The reference to the bytes that represent the `LeafKey`.
    leaf_key_bytes: &'a [u8],
    pos: Range<usize>,
    // invariant pos.end == leaf_key_bytes.len() * 8;
}

impl DoubleEndedIterator for LeafKeyBitIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.pos.next_back().map(|x| self.get_bit(x))
    }
}

impl ExactSizeIterator for LeafKeyBitIterator<'_> {}

impl<'a> LeafKeyBitIterator<'a> {
    /// Constructs a new `LeafKeyBitIterator` using given `leaf_key_bytes`.
    fn new(leaf_key: LeafKeyRef<'a>) -> Self {
        LeafKeyBitIterator {
            leaf_key_bytes: leaf_key.bytes,
            pos: (0..leaf_key.bytes.len() * 8),
        }
    }

    /// Returns the `index`-th bit in the bytes.
    fn get_bit(&self, index: usize) -> bool {
        let pos = index / 8;
        let bit = 7 - index % 8;
        (self.leaf_key_bytes[pos] >> bit) & 1 != 0
    }
}

impl Iterator for LeafKeyBitIterator<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|x| self.get_bit(x))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.pos.size_hint()
    }
}

// INITIAL-MODIFICATION: since we use our own `LeafKey` here, we need it to implement these for it
pub trait IteratedLeafKey {
    fn iter_bits(&self) -> LeafKeyBitIterator<'_>;

    fn get_nibble(&self, index: usize) -> Nibble;
}

impl IteratedLeafKey for LeafKey {
    fn iter_bits(&self) -> LeafKeyBitIterator<'_> {
        LeafKeyBitIterator::new(self.as_ref())
    }

    fn get_nibble(&self, index: usize) -> Nibble {
        Nibble::from(if index % 2 == 0 {
            self.bytes[index / 2] >> 4
        } else {
            self.bytes[index / 2] & 0x0F
        })
    }
}

impl IteratedLeafKey for LeafKeyRef<'_> {
    fn iter_bits(&self) -> LeafKeyBitIterator<'_> {
        LeafKeyBitIterator::new(*self)
    }

    fn get_nibble(&self, index: usize) -> Nibble {
        Nibble::from(if index % 2 == 0 {
            self.bytes[index / 2] >> 4
        } else {
            self.bytes[index / 2] & 0x0F
        })
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/transaction/mod.rs#L57
pub type Version = u64;

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/nibble/mod.rs#L20
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Nibble(u8);

impl From<u8> for Nibble {
    fn from(nibble: u8) -> Self {
        assert!(nibble < 16, "Nibble out of range: {}", nibble);
        Self(nibble)
    }
}

impl From<Nibble> for u8 {
    fn from(nibble: Nibble) -> Self {
        nibble.0
    }
}

impl fmt::LowerHex for Nibble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/nibble/nibble_path/mod.rs#L22
/// NibblePath defines a path in Merkle tree in the unit of nibble (4 bits).
#[derive(Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NibblePath {
    /// Indicates the total number of nibbles in bytes. Either `bytes.len() * 2 - 1` or
    /// `bytes.len() * 2`.
    // Guarantees intended ordering based on the top-to-bottom declaration order of the struct's
    // members.
    num_nibbles: usize,
    /// The underlying bytes that stores the path, 2 nibbles per byte. If the number of nibbles is
    /// odd, the second half of the last byte must be 0.
    bytes: Vec<u8>,
}

/// Supports debug format by concatenating nibbles literally. For example, [0x12, 0xa0] with 3
/// nibbles will be printed as "12a".
impl fmt::Debug for NibblePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.nibbles().try_for_each(|x| write!(f, "{:x}", x))
    }
}

// INITIAL-MODIFICATION: just to show it in errors
impl fmt::Display for NibblePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex_chars = self
            .bytes
            .iter()
            .flat_map(|b| [b >> 4, b & 15])
            .map(|b| char::from_digit(u32::from(b), 16).unwrap())
            .take(self.num_nibbles);

        for ch in hex_chars {
            write!(f, "{}", ch)?;
        }
        Ok(())
    }
}

/// Convert a vector of bytes into `NibblePath` using the lower 4 bits of each byte as nibble.
impl FromIterator<Nibble> for NibblePath {
    fn from_iter<I: IntoIterator<Item = Nibble>>(iter: I) -> Self {
        let mut nibble_path = NibblePath::new_even(vec![]);
        for nibble in iter {
            nibble_path.push(nibble);
        }
        nibble_path
    }
}

impl NibblePath {
    /// Creates a new `NibblePath` from a vector of bytes assuming each byte has 2 nibbles.
    pub fn new_even(bytes: Vec<u8>) -> Self {
        let num_nibbles = bytes.len() * 2;
        NibblePath { num_nibbles, bytes }
    }

    /// Similar to `new()` but asserts that the bytes have one less nibble.
    pub fn new_odd(bytes: Vec<u8>) -> Self {
        assert_eq!(
            bytes.last().expect("Should have odd number of nibbles.") & 0x0F,
            0,
            "Last nibble must be 0."
        );
        let num_nibbles = bytes.len() * 2 - 1;
        NibblePath { num_nibbles, bytes }
    }

    /// Adds a nibble to the end of the nibble path.
    pub fn push(&mut self, nibble: Nibble) {
        if self.num_nibbles % 2 == 0 {
            self.bytes.push(u8::from(nibble) << 4);
        } else {
            self.bytes[self.num_nibbles / 2] |= u8::from(nibble);
        }
        self.num_nibbles += 1;
    }

    /// Pops a nibble from the end of the nibble path.
    pub fn pop(&mut self) -> Option<Nibble> {
        let poped_nibble = if self.num_nibbles % 2 == 0 {
            self.bytes.last_mut().map(|last_byte| {
                let nibble = *last_byte & 0x0F;
                *last_byte &= 0xF0;
                Nibble::from(nibble)
            })
        } else {
            self.bytes.pop().map(|byte| Nibble::from(byte >> 4))
        };
        if poped_nibble.is_some() {
            self.num_nibbles -= 1;
        }
        poped_nibble
    }

    /// Returns the last nibble.
    pub fn last(&self) -> Option<Nibble> {
        let last_byte_option = self.bytes.last();
        if self.num_nibbles % 2 == 0 {
            last_byte_option.map(|last_byte| Nibble::from(*last_byte & 0x0F))
        } else {
            let last_byte = last_byte_option.expect("Last byte must exist if num_nibbles is odd.");
            Some(Nibble::from(*last_byte >> 4))
        }
    }

    /// Get the i-th bit.
    fn get_bit(&self, i: usize) -> bool {
        assert!(i < self.num_nibbles * 4);
        let pos = i / 8;
        let bit = 7 - i % 8;
        ((self.bytes[pos] >> bit) & 1) != 0
    }

    /// Get the i-th nibble.
    pub fn get_nibble(&self, i: usize) -> Nibble {
        assert!(i < self.num_nibbles);
        Nibble::from((self.bytes[i / 2] >> (if i % 2 == 1 { 0 } else { 4 })) & 0xF)
    }

    /// Get a bit iterator iterates over the whole nibble path.
    pub fn bits(&self) -> NibbleBitIterator {
        NibbleBitIterator {
            nibble_path: self,
            pos: (0..self.num_nibbles * 4),
        }
    }

    /// Get a nibble iterator iterates over the whole nibble path.
    pub fn nibbles(&self) -> NibbleIterator {
        NibbleIterator::new(self, 0, self.num_nibbles)
    }

    /// Get the total number of nibbles stored.
    pub fn num_nibbles(&self) -> usize {
        self.num_nibbles
    }

    ///  Returns `true` if the nibbles contains no elements.
    pub fn is_empty(&self) -> bool {
        self.num_nibbles() == 0
    }

    /// Get the underlying bytes storing nibbles.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn truncate(&mut self, len: usize) {
        assert!(len <= self.num_nibbles);
        self.num_nibbles = len;
        self.bytes.truncate(len.div_ceil(2));
        if len % 2 != 0 {
            *self.bytes.last_mut().expect("must exist.") &= 0xF0;
        }
    }
}

pub trait Peekable: Iterator {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item>;
}

/// BitIterator iterates a nibble path by bit.
pub struct NibbleBitIterator<'a> {
    nibble_path: &'a NibblePath,
    pos: Range<usize>,
}

impl Peekable for NibbleBitIterator<'_> {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item> {
        if self.pos.start < self.pos.end {
            Some(self.nibble_path.get_bit(self.pos.start))
        } else {
            None
        }
    }
}

/// BitIterator spits out a boolean each time. True/false denotes 1/0.
impl Iterator for NibbleBitIterator<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|i| self.nibble_path.get_bit(i))
    }
}

/// Support iterating bits in reversed order.
impl DoubleEndedIterator for NibbleBitIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.pos.next_back().map(|i| self.nibble_path.get_bit(i))
    }
}

/// NibbleIterator iterates a nibble path by nibble.
#[derive(Debug)]
pub struct NibbleIterator<'a> {
    /// The underlying nibble path that stores the nibbles
    nibble_path: &'a NibblePath,

    /// The current index, `pos.start`, will bump by 1 after calling `next()` until `pos.start ==
    /// pos.end`.
    pos: Range<usize>,

    /// The start index of the iterator. At the beginning, `pos.start == start`. [start, pos.end)
    /// defines the range of `nibble_path` this iterator iterates over. `nibble_path` refers to
    /// the entire underlying buffer but the range may only be partial.
    start: usize,
    // invariant self.start <= self.pos.start;
    // invariant self.pos.start <= self.pos.end;
}

/// NibbleIterator spits out a byte each time. Each byte must be in range [0, 16).
impl Iterator for NibbleIterator<'_> {
    type Item = Nibble;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().map(|i| self.nibble_path.get_nibble(i))
    }
}

impl Peekable for NibbleIterator<'_> {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item> {
        if self.pos.start < self.pos.end {
            Some(self.nibble_path.get_nibble(self.pos.start))
        } else {
            None
        }
    }
}

impl<'a> NibbleIterator<'a> {
    fn new(nibble_path: &'a NibblePath, start: usize, end: usize) -> Self {
        assert!(start <= end);
        Self {
            nibble_path,
            pos: (start..end),
            start,
        }
    }

    /// Returns a nibble iterator that iterates all visited nibbles.
    pub fn visited_nibbles(&self) -> NibbleIterator<'a> {
        Self::new(self.nibble_path, self.start, self.pos.start)
    }

    /// Returns a nibble iterator that iterates all remaining nibbles.
    pub fn remaining_nibbles(&self) -> NibbleIterator<'a> {
        Self::new(self.nibble_path, self.pos.start, self.pos.end)
    }

    /// Turn it into a `BitIterator`.
    pub fn bits(&self) -> NibbleBitIterator<'a> {
        NibbleBitIterator {
            nibble_path: self.nibble_path,
            pos: (self.pos.start * 4..self.pos.end * 4),
        }
    }

    /// Cut and return the range of the underlying `nibble_path` that this iterator is iterating
    /// over as a new `NibblePath`
    pub fn get_nibble_path(&self) -> NibblePath {
        self.visited_nibbles().chain(self.remaining_nibbles()).collect()
    }

    /// Get the number of nibbles that this iterator covers.
    pub fn num_nibbles(&self) -> usize {
        assert!(self.start <= self.pos.end); // invariant
        self.pos.end - self.start
    }

    /// Return `true` if the iteration is over.
    pub fn is_finished(&self) -> bool {
        self.peek().is_none()
    }
}

// INITIAL-MODIFICATION: We will use this type (instead of `Hash`) to allow for arbitrary key length
/// A leaf key (i.e. a complete nibble path).
#[derive(
    Clone, Debug, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct LeafKey {
    /// The underlying bytes.
    /// All leaf keys of the same tree must be of the same length - otherwise the tree's behavior
    /// becomes unspecified.
    /// All leaf keys must be evenly distributed across their space - otherwise the tree's
    /// performance degrades.
    /// TARI: always a hash, so replaced heap-allocated Vec<u8> with a Hash
    pub bytes: TreeHash,
}

impl LeafKey {
    pub fn new(bytes: TreeHash) -> Self {
        Self { bytes }
    }

    pub fn as_ref(&self) -> LeafKeyRef<'_> {
        LeafKeyRef::new(self.bytes.as_slice())
    }

    pub fn iter_bits(&self) -> BitIterator<'_> {
        BitIterator::new(self.bytes.as_slice())
    }

    pub fn common_prefix_bits_len(&self, other: &LeafKey) -> usize {
        self.iter_bits()
            .zip(other.iter_bits())
            .take_while(|(x, y)| x == y)
            .count()
    }
}

impl Display for LeafKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.bytes.fmt(f)
    }
}

// INITIAL-MODIFICATION: We will use this type (instead of `Hash`) to allow for arbitrary key length
/// A leaf key (i.e. a complete nibble path).
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct LeafKeyRef<'a> {
    /// The underlying bytes.
    /// All leaf keys of the same tree must be of the same length - otherwise the tree's behavior
    /// becomes unspecified.
    /// All leaf keys must be evenly distributed across their space - otherwise the tree's
    /// performance degrades.
    pub bytes: &'a [u8],
}

impl<'a> LeafKeyRef<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl PartialEq<LeafKey> for LeafKeyRef<'_> {
    fn eq(&self, other: &LeafKey) -> bool {
        self.bytes == other.bytes.as_slice()
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/storage/jellyfish-merkle/src/node_type/mod.rs#L48
/// The unique key of each node.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NodeKey {
    /// The version at which the node is created.
    version: Version,
    /// The nibble path this node represents in the tree.
    nibble_path: NibblePath,
}

impl NodeKey {
    /// Creates a new `NodeKey`.
    pub fn new(version: Version, nibble_path: NibblePath) -> Self {
        Self { version, nibble_path }
    }

    /// A shortcut to generate a node key consisting of a version and an empty nibble path.
    pub fn new_empty_path(version: Version) -> Self {
        Self::new(version, NibblePath::new_even(vec![]))
    }

    /// Gets the version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Gets the nibble path.
    pub fn nibble_path(&self) -> &NibblePath {
        &self.nibble_path
    }

    /// Generates a child node key based on this node key.
    pub fn gen_child_node_key(&self, version: Version, n: Nibble) -> Self {
        let mut node_nibble_path = self.nibble_path().clone();
        node_nibble_path.push(n);
        Self::new(version, node_nibble_path)
    }

    /// Generates parent node key at the same version based on this node key.
    pub fn gen_parent_node_key(&self) -> Self {
        let mut node_nibble_path = self.nibble_path().clone();
        assert!(node_nibble_path.pop().is_some(), "Current node key is root.",);
        Self::new(self.version, node_nibble_path)
    }
}

// INITIAL-MODIFICATION: just to show it in errors
impl fmt::Display for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}:{}", self.version, self.nibble_path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NodeType {
    Leaf,
    Null,
    /// A internal node that haven't been finished the leaf count migration, i.e. None or not all
    /// of the children leaf counts are known.
    Internal {
        leaf_count: usize,
    },
}

/// Each child of [`InternalNode`] encapsulates a nibble forking at this node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Child {
    /// The hash value of this child node.
    pub hash: TreeHash,
    /// `version`, the `nibble_path` of the [`NodeKey`] of this [`InternalNode`] the child belongs
    /// to and the child's index constitute the [`NodeKey`] to uniquely identify this child node
    /// from the storage. Used by `[`NodeKey::gen_child_node_key`].
    pub version: Version,
    /// Indicates if the child is a leaf, or if it's an internal node, the total number of leaves
    /// under it (though it can be unknown during migration).
    pub node_type: NodeType,
}

impl Child {
    pub fn new(hash: TreeHash, version: Version, node_type: NodeType) -> Self {
        Self {
            hash,
            version,
            node_type,
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self.node_type, NodeType::Leaf)
    }

    pub fn leaf_count(&self) -> usize {
        match self.node_type {
            NodeType::Leaf => 1,
            NodeType::Internal { leaf_count } => leaf_count,
            NodeType::Null => unreachable!("Child cannot be Null"),
        }
    }
}

/// [`Children`] is just a collection of children belonging to a [`InternalNode`], indexed from 0 to
/// 15, inclusive.
pub(crate) type Children = IndexMap<Nibble, Child>;

/// Represents a 4-level subtree with 16 children at the bottom level. Theoretically, this reduces
/// IOPS to query a tree by 4x since we compress 4 levels in a standard Merkle tree into 1 node.
/// Though we choose the same internal node structure as that of Patricia Merkle tree, the root hash
/// computation logic is similar to a 4-level sparse Merkle tree except for some customizations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InternalNode {
    /// Up to 16 children.
    children: Children,
    /// Total number of leaves under this internal node
    leaf_count: usize,
}

impl InternalNode {
    /// Creates a new Internal node.
    pub fn new(mut children: Children) -> Self {
        children.sort_keys();
        let leaf_count = children.values().map(Child::leaf_count).sum();
        Self { children, leaf_count }
    }

    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    pub fn node_type(&self) -> NodeType {
        NodeType::Internal {
            leaf_count: self.leaf_count,
        }
    }

    pub fn hash(&self) -> TreeHash {
        self.merkle_hash(
            0,  // start index
            16, // the number of leaves in the subtree of which we want the hash of root
            self.generate_bitmaps(),
        )
    }

    pub fn children_sorted(&self) -> impl Iterator<Item = (&Nibble, &Child)> {
        // let mut tmp = self.children.iter().collect::<Vec<_>>();
        // tmp.sort_by_key(|(nibble, _)| **nibble);
        // tmp.into_iter()
        self.children.iter()
    }

    pub fn into_children(self) -> Children {
        self.children
    }

    /// Gets the `n`-th child.
    pub fn child(&self, n: Nibble) -> Option<&Child> {
        self.children.get(&n)
    }

    /// Generates `existence_bitmap` and `leaf_bitmap` as a pair of `u16`s: child at index `i`
    /// exists if `existence_bitmap[i]` is set; child at index `i` is leaf node if
    /// `leaf_bitmap[i]` is set.
    pub fn generate_bitmaps(&self) -> (u16, u16) {
        let mut existence_bitmap = 0;
        let mut leaf_bitmap = 0;
        for (nibble, child) in &self.children {
            let i = u8::from(*nibble);
            existence_bitmap |= 1u16 << i;
            if child.is_leaf() {
                leaf_bitmap |= 1u16 << i;
            }
        }
        // `leaf_bitmap` must be a subset of `existence_bitmap`.
        assert_eq!(existence_bitmap | leaf_bitmap, existence_bitmap);
        (existence_bitmap, leaf_bitmap)
    }

    /// Given a range [start, start + width), returns the sub-bitmap of that range.
    fn range_bitmaps(start: u8, width: u8, bitmaps: (u16, u16)) -> (u16, u16) {
        assert!(start < 16 && width.count_ones() == 1 && start % width == 0);
        assert!(width <= 16 && (start + width) <= 16);
        // A range with `start == 8` and `width == 4` will generate a mask 0b0000111100000000.
        // use as converting to smaller integer types when 'width == 16'
        #[allow(clippy::cast_possible_truncation)]
        let mask = (((1u32 << width) - 1) << start) as u16;
        (bitmaps.0 & mask, bitmaps.1 & mask)
    }

    fn merkle_hash(&self, start: u8, width: u8, (existence_bitmap, leaf_bitmap): (u16, u16)) -> TreeHash {
        // Given a bit [start, 1 << nibble_height], return the value of that range.
        let (range_existence_bitmap, range_leaf_bitmap) =
            Self::range_bitmaps(start, width, (existence_bitmap, leaf_bitmap));
        if range_existence_bitmap == 0 {
            // No child under this subtree
            SPARSE_MERKLE_PLACEHOLDER_HASH
        } else if width == 1 || (range_existence_bitmap.count_ones() == 1 && range_leaf_bitmap != 0) {
            // Only 1 leaf child under this subtree or reach the lowest level
            #[allow(clippy::cast_possible_truncation)]
            let only_child_index = Nibble::from(range_existence_bitmap.trailing_zeros() as u8);
            self.child(only_child_index)
                .expect("Corrupted internal node: existence_bitmap inconsistent")
                .hash
        } else {
            let left_child = self.merkle_hash(start, width / 2, (range_existence_bitmap, range_leaf_bitmap));
            let right_child = self.merkle_hash(
                start + width / 2,
                width / 2,
                (range_existence_bitmap, range_leaf_bitmap),
            );
            SparseMerkleInternalNode::new(left_child, right_child).hash()
        }
    }

    fn gen_node_in_proof<K: Clone, R: TreeStoreReader<K>>(
        &self,
        start: u8,
        width: u8,
        (existence_bitmap, leaf_bitmap): (u16, u16),
        (tree_reader, node_key): (&R, &NodeKey),
    ) -> Result<NodeInProof, JmtStorageError> {
        // Given a bit [start, 1 << nibble_height], return the value of that range.
        let (range_existence_bitmap, range_leaf_bitmap) =
            Self::range_bitmaps(start, width, (existence_bitmap, leaf_bitmap));
        Ok(if range_existence_bitmap == 0 {
            // No child under this subtree
            NodeInProof::Other(SPARSE_MERKLE_PLACEHOLDER_HASH)
        } else if width == 1 || (range_existence_bitmap.count_ones() == 1 && range_leaf_bitmap != 0) {
            // Only 1 leaf child under this subtree or reach the lowest level
            #[allow(clippy::cast_possible_truncation)]
            let only_child_index = Nibble::from(range_existence_bitmap.trailing_zeros() as u8);
            let only_child = self
                .child(only_child_index)
                .expect("Corrupted internal node: existence_bitmap inconsistent");
            if matches!(only_child.node_type, NodeType::Leaf) {
                let only_child_node_key = node_key.gen_child_node_key(only_child.version, only_child_index);
                match tree_reader.get_node(&only_child_node_key)? {
                    Node::Internal(_) => {
                        unreachable!("Corrupted internal node: in-memory leaf child is internal node on disk")
                    },
                    Node::Leaf(leaf_node) => NodeInProof::Leaf(SparseMerkleLeafNode::from(leaf_node)),
                    Node::Null => unreachable!("Child cannot be Null"),
                }
            } else {
                NodeInProof::Other(only_child.hash)
            }
        } else {
            let left_child = self.merkle_hash(start, width / 2, (range_existence_bitmap, range_leaf_bitmap));
            let right_child = self.merkle_hash(
                start + width / 2,
                width / 2,
                (range_existence_bitmap, range_leaf_bitmap),
            );
            NodeInProof::Other(SparseMerkleInternalNode::new(left_child, right_child).hash())
        })
    }

    /// Gets the child and its corresponding siblings that are necessary to generate the proof for
    /// the `n`-th child. If it is an existence proof, the returned child must be the `n`-th
    /// child; otherwise, the returned child may be another child. See inline explanation for
    /// details. When calling this function with n = 11 (node `b` in the following graph), the
    /// range at each level is illustrated as a pair of square brackets:
    ///
    /// ```text
    ///     4      [f   e   d   c   b   a   9   8   7   6   5   4   3   2   1   0] -> root level
    ///            ---------------------------------------------------------------
    ///     3      [f   e   d   c   b   a   9   8] [7   6   5   4   3   2   1   0] width = 8
    ///                                  chs <--┘                        shs <--┘
    ///     2      [f   e   d   c] [b   a   9   8] [7   6   5   4] [3   2   1   0] width = 4
    ///                  shs <--┘               └--> chs
    ///     1      [f   e] [d   c] [b   a] [9   8] [7   6] [5   4] [3   2] [1   0] width = 2
    ///                          chs <--┘       └--> shs
    ///     0      [f] [e] [d] [c] [b] [a] [9] [8] [7] [6] [5] [4] [3] [2] [1] [0] width = 1
    ///     ^                chs <--┘   └--> shs
    ///     |   MSB|<---------------------- uint 16 ---------------------------->|LSB
    ///  height    chs: `child_half_start`         shs: `sibling_half_start`
    /// ```
    pub fn get_child_with_siblings<K: Clone, R: TreeStoreReader<K>>(
        &self,
        node_key: &NodeKey,
        n: Nibble,
        reader: Option<&R>,
    ) -> Result<(Option<NodeKey>, Vec<NodeInProof>), JmtStorageError> {
        let mut siblings = vec![];
        let (existence_bitmap, leaf_bitmap) = self.generate_bitmaps();

        // Nibble height from 3 to 0.
        for h in (0..4).rev() {
            // Get the number of children of the internal node that each subtree at this height
            // covers.
            let width = 1 << h;
            let (child_half_start, sibling_half_start) = get_child_and_sibling_half_start(n, h);
            // Compute the root hash of the subtree rooted at the sibling of `r`.
            if let Some(reader) = reader {
                siblings.push(self.gen_node_in_proof(
                    sibling_half_start,
                    width,
                    (existence_bitmap, leaf_bitmap),
                    (reader, node_key),
                )?);
            } else {
                siblings.push(
                    self.merkle_hash(sibling_half_start, width, (existence_bitmap, leaf_bitmap))
                        .into(),
                );
            }

            let (range_existence_bitmap, range_leaf_bitmap) =
                Self::range_bitmaps(child_half_start, width, (existence_bitmap, leaf_bitmap));

            if range_existence_bitmap == 0 {
                // No child in this range.
                return Ok((None, siblings));
            }

            if width == 1 || (range_existence_bitmap.count_ones() == 1 && range_leaf_bitmap != 0) {
                // Return the only 1 leaf child under this subtree or reach the lowest level
                // Even this leaf child is not the n-th child, it should be returned instead of
                // `None` because it's existence indirectly proves the n-th child doesn't exist.
                // Please read proof format for details.
                #[allow(clippy::cast_possible_truncation)]
                let only_child_index = Nibble::from(range_existence_bitmap.trailing_zeros() as u8);
                return Ok((
                    {
                        let only_child_version = self
                            .child(only_child_index)
                            // Should be guaranteed by the self invariants, but these are not easy to express at the moment
                            .expect("Corrupted internal node: child_bitmap inconsistent")
                            .version;
                        Some(node_key.gen_child_node_key(only_child_version, only_child_index))
                    },
                    siblings,
                ));
            }
        }
        unreachable!("Impossible to get here without returning even at the lowest level.")
    }
}

/// Given a nibble, computes the start position of its `child_half_start` and `sibling_half_start`
/// at `height` level.
pub(crate) fn get_child_and_sibling_half_start(n: Nibble, height: u8) -> (u8, u8) {
    // Get the index of the first child belonging to the same subtree whose root, let's say `r` is
    // at `height` that the n-th child belongs to.
    // Note: `child_half_start` will be always equal to `n` at height 0.
    let child_half_start = (0xFF << height) & u8::from(n);

    // Get the index of the first child belonging to the subtree whose root is the sibling of `r`
    // at `height`.
    let sibling_half_start = child_half_start ^ (1 << height);

    (child_half_start, sibling_half_start)
}

/// Leaf node, capturing the value hash and carrying an arbitrary payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LeafNode<P> {
    // The key of this leaf node (i.e. its full nibble path).
    leaf_key: LeafKey,
    // The hash of an externally-stored value.
    // Note: do not confuse that value with the `payload`.
    value_hash: TreeHash,
    // The client payload.
    // This is not the "value" whose changes are tracked by the tree (in fact, these values are
    // supposed to be stored externally, and the tree only cares about their hashes - see
    // `value_hash`).
    // Rather, the payload is an arbitrary piece of data that the client wishes to store within the
    // tree, in order to facilitate some related processing:
    // - Many clients do not need it and will simply use a no-cost `()`.
    // - A use-case designed by the original authors was to store a non-hashed element key as a payload (while the
    //   `leaf_key` contains that key's hash, to ensure the nibble paths are distributed over their space, for
    //   performance).
    // - Our current use-case (specific to a "two layers" tree) is to store the nested tree's root metadata.
    payload: P,
    // The version at which this leaf was created.
    version: Version,
}

impl<P> LeafNode<P> {
    /// Creates a new leaf node.
    pub fn new(leaf_key: LeafKey, value_hash: TreeHash, payload: P, version: Version) -> Self {
        Self {
            leaf_key,
            value_hash,
            payload,
            version,
        }
    }

    /// Gets the key.
    pub fn leaf_key(&self) -> &LeafKey {
        &self.leaf_key
    }

    /// Gets the associated value hash.
    pub fn value_hash(&self) -> TreeHash {
        self.value_hash
    }

    /// Gets the payload.
    pub fn payload(&self) -> &P {
        &self.payload
    }

    /// Gets the version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Gets the leaf's hash (not to be confused with a `value_hash()`).
    /// This hash incorporates the node's key and the value's hash, in order to capture certain
    /// changes within a sparse merkle tree (consider 2 trees, both containing a single element with
    /// the same value, but stored under different keys - we want their root hashes to differ).
    pub fn leaf_hash(&self) -> TreeHash {
        jmt_node_hash2(&self.leaf_key.bytes, &self.value_hash)
    }
}

impl<K> From<LeafNode<K>> for SparseMerkleLeafNode {
    fn from(leaf_node: LeafNode<K>) -> Self {
        Self::new(leaf_node.leaf_key, leaf_node.value_hash)
    }
}

/// The concrete node type of [`JellyfishMerkleTree`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Node<P> {
    /// A wrapper of [`InternalNode`].
    Internal(InternalNode),
    /// A wrapper of [`LeafNode`].
    Leaf(LeafNode<P>),
    /// Represents empty tree only
    Null,
}

impl<P> From<InternalNode> for Node<P> {
    fn from(node: InternalNode) -> Self {
        Node::Internal(node)
    }
}

impl<P: Clone> From<LeafNode<P>> for Node<P> {
    fn from(node: LeafNode<P>) -> Self {
        Node::Leaf(node)
    }
}

impl<P> Node<P> {
    // /// Creates the [`Internal`](Node::Internal) variant.
    // #[cfg(any(test, feature = "fuzzing"))]
    // pub fn new_internal(children: Children) -> Self {
    //     Node::Internal(InternalNode::new(children))
    // }

    /// Creates the [`Leaf`](Node::Leaf) variant.
    pub fn new_leaf(leaf_key: LeafKey, value_hash: TreeHash, payload: P, version: Version) -> Self {
        Node::Leaf(LeafNode::new(leaf_key, value_hash, payload, version))
    }

    /// Returns `true` if the node is a leaf node.
    pub fn is_leaf(&self) -> bool {
        matches!(self, Node::Leaf(_))
    }

    pub fn leaf(&self) -> Option<&LeafNode<P>> {
        match self {
            Node::Leaf(leaf) => Some(leaf),
            _ => None,
        }
    }

    /// Returns `NodeType`
    pub fn node_type(&self) -> NodeType {
        match self {
            // The returning value will be used to construct a `Child` of a internal node, while an
            // internal node will never have a child of Node::Null.
            Self::Leaf(_) => NodeType::Leaf,
            Self::Internal(n) => n.node_type(),
            Self::Null => NodeType::Null,
        }
    }

    /// Returns leaf count if known
    pub fn leaf_count(&self) -> usize {
        match self {
            Node::Leaf(_) => 1,
            Node::Internal(internal_node) => internal_node.leaf_count,
            Node::Null => 0,
        }
    }

    /// Computes the hash of nodes.
    pub fn hash(&self) -> TreeHash {
        match self {
            Node::Internal(internal_node) => internal_node.hash(),
            Node::Leaf(leaf_node) => leaf_node.leaf_hash(),
            Node::Null => SPARSE_MERKLE_PLACEHOLDER_HASH,
        }
    }
}

// INITIAL-MODIFICATION: we propagate usage of our own error enum (instead of `std::io::ErrorKind`
// used by Aptos) to allow for no-std build.
/// Error originating from underlying storage failure / inconsistency.
#[derive(Debug, thiserror::Error)]
pub enum JmtStorageError {
    #[error("A node {0} expected to exist (according to JMT logic) was not found in the storage")]
    NotFound(NodeKey),

    #[error("Nodes read from the storage are violating some JMT property (e.g. form a cycle).")]
    InconsistentState,

    #[error("Unexpected error: {0}")]
    UnexpectedError(String),

    #[error("Attempted to insert node {0} that already exists")]
    Conflict(NodeKey),
}
