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
use tari_hashing::layer2::{TariDomainHasher, tari_hasher32};

use crate::{
    TreeHash,
    bit_iter::BitIterator,
    error::{InternalNodeError, JmtProofVerifyError, NibbleOutOfRange, NibblePathError},
    store::TreeStoreReader,
};

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

/// The scheme used to hash JMT leaf and internal nodes, and so every root and proof.
///
/// There is deliberately no `Default`: every caller chooses the scheme explicitly. A verifier derives it from its
/// authenticated context (e.g. a protocol version in a signed header), never from the proof, which carries no scheme
/// tag.
///
/// The serde and borsh derives exist for configuration and storage. Never deserialize a scheme from a message supplied
/// by a counterparty or prover and use it to verify that counterparty's proof.
///
/// The enum is `#[non_exhaustive]`, so a `match` on it outside this crate needs a wildcard arm. A new scheme is a new
/// variant with its own domain version; existing variants never change. Every variant must hash leaf and internal
/// nodes under distinct labels: the legacy single-label `"Node"` scheme must never return as a variant, because it
/// lets a leaf be passed off as an internal node. See the crate docs for the scheme table.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum JmtHashScheme {
    /// Domain "com.tari.jmt" version 0; labels "Leaf" and "Internal". The only scheme in use (since tari_jellyfish
    /// 6.0.1-pre.2).
    V1,
}

// Leaf and internal node hashes use distinct labels so that a leaf can never be passed off as an internal node (or
// vice versa) in a proof, since both hash two 32-byte values.
fn jmt_leaf_hash(scheme: JmtHashScheme, key: &LeafKey, value_hash: &TreeHash) -> TreeHash {
    match scheme {
        JmtHashScheme::V1 => tari_hasher32::<JmtHashDomain>("Leaf")
            .chain(&key.bytes)
            .chain(value_hash)
            .finalize_into_array()
            .into(),
    }
}

fn jmt_internal_hash(scheme: JmtHashScheme, left: &TreeHash, right: &TreeHash) -> TreeHash {
    match scheme {
        JmtHashScheme::V1 => tari_hasher32::<JmtHashDomain>("Internal")
            .chain(left)
            .chain(right)
            .finalize_into_array()
            .into(),
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/proof/definition.rs#L182
/// The maximum number of siblings in a proof, i.e. the bit length of a [`LeafKey`].
pub const MAX_PROOF_SIBLINGS: usize = 256;

/// The maximum number of nibbles in a [`NibblePath`], i.e. the nibble length of a [`LeafKey`].
///
/// Enforced when a `NibblePath` is decoded and by [`NibblePath::new_odd`], but not by [`NibblePath::new_even`] or
/// [`NibblePath::push`]. Tree traversals bound their depth independently of this cap.
pub const MAX_NIBBLE_PATH_LEN: usize = 64;

/// A more detailed version of `SparseMerkleProof` with the only difference that all the leaf
/// siblings are explicitly set as `SparseMerkleLeafNode` instead of its hash value.
///
/// # Trust
/// A proof only means something against a root hash that the caller has authenticated independently for the specific
/// tree **and version** being proved (e.g. from a signed header). The proof contents ([`Self::leaf`],
/// [`Self::siblings`]) are untrusted until one of the `verify_*` methods returns `Ok`. An exclusion proof shows that a
/// key is absent at that root; it does not show that the key was destroyed or never existed. Proving destruction also
/// needs an inclusion proof at an earlier authenticated root.
///
/// # Hash scheme
/// The proof carries no [`JmtHashScheme`] tag. A verifier pins the scheme from the same authenticated context as the
/// root (e.g. the protocol version of the signed header), never from the proof or from the prover.
///
/// # Malleability
/// The encoding is not canonical: a sibling [`NodeInProof::Leaf(l)`](NodeInProof::Leaf) verifies identically to
/// [`NodeInProof::Other(l.hash(scheme))`](NodeInProof::Other), so several different byte strings prove the same fact.
/// Proof bytes, and any hash over them, are not an identity. Key dedup, replay protection or caching on
/// `(root, key, value_hash)` instead.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, BorshSerialize)]
pub struct SparseMerkleProofExt {
    leaf: Option<SparseMerkleLeafNode>,
    /// All siblings in this proof, including the default ones. Siblings are ordered from the bottom
    /// level to the root level.
    #[serde(deserialize_with = "deserialize_bounded_siblings")]
    siblings: Vec<NodeInProof>,
}

// Rejects oversized proofs as soon as the length is read, rather than decoding every sibling and failing in `verify`.
impl BorshDeserialize for SparseMerkleProofExt {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let leaf = Option::<SparseMerkleLeafNode>::deserialize_reader(reader)?;
        let len = usize::try_from(u32::deserialize_reader(reader)?)
            .map_err(|_| borsh::io::Error::new(borsh::io::ErrorKind::InvalidData, "sibling count overflow"))?;
        if len > MAX_PROOF_SIBLINGS {
            return Err(borsh::io::Error::new(
                borsh::io::ErrorKind::InvalidData,
                format!("SparseMerkleProofExt has {len} siblings, max is {MAX_PROOF_SIBLINGS}"),
            ));
        }
        let mut siblings = Vec::with_capacity(len);
        for _ in 0..len {
            siblings.push(NodeInProof::deserialize_reader(reader)?);
        }
        Ok(Self { leaf, siblings })
    }
}

fn deserialize_bounded_siblings<'de, D>(deserializer: D) -> Result<Vec<NodeInProof>, D::Error>
where D: serde::Deserializer<'de> {
    struct BoundedVisitor;

    impl<'de> serde::de::Visitor<'de> for BoundedVisitor {
        type Value = Vec<NodeInProof>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "a sequence of at most {MAX_PROOF_SIBLINGS} siblings")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut siblings = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_PROOF_SIBLINGS));
            while let Some(sibling) = seq.next_element()? {
                if siblings.len() == MAX_PROOF_SIBLINGS {
                    return Err(serde::de::Error::invalid_length(
                        MAX_PROOF_SIBLINGS.saturating_add(1),
                        &self,
                    ));
                }
                siblings.push(sibling);
            }
            Ok(siblings)
        }
    }

    deserializer.deserialize_seq(BoundedVisitor)
}

impl SparseMerkleProofExt {
    /// Constructs a new `SparseMerkleProofExt` using leaf and a list of sibling nodes.
    pub(crate) fn new(leaf: Option<SparseMerkleLeafNode>, siblings: Vec<NodeInProof>) -> Self {
        Self { leaf, siblings }
    }

    /// Returns the leaf node in this proof. Untrusted until a `verify_*` method returns `Ok` against an authenticated
    /// root.
    pub fn leaf(&self) -> Option<SparseMerkleLeafNode> {
        self.leaf.clone()
    }

    #[cfg(feature = "minicbor")]
    pub(crate) fn leaf_ref(&self) -> Option<&SparseMerkleLeafNode> {
        self.leaf.as_ref()
    }

    /// Returns the list of siblings in this proof. Untrusted until a `verify_*` method returns `Ok` against an
    /// authenticated root.
    pub fn siblings(&self) -> &[NodeInProof] {
        &self.siblings
    }

    /// Verifies an element whose key is `element_key` and value is `element_value` exists in the Sparse Merkle Tree
    /// using the provided proof.
    ///
    /// `expected_root_hash` must be authenticated independently for the specific tree and version being proved; see
    /// the type-level docs.
    pub fn verify_inclusion(
        &self,
        scheme: JmtHashScheme,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
        element_value_hash: &TreeHash,
    ) -> Result<(), JmtProofVerifyError> {
        self.verify(scheme, expected_root_hash, element_key, Some(element_value_hash))
    }

    /// Verifies the proof is a valid non-inclusion proof that shows this key doesn't exist in the tree.
    ///
    /// `expected_root_hash` must be authenticated independently for the specific tree and version being proved; see
    /// the type-level docs. This proves "absent at this root", not "destroyed" or "never existed".
    ///
    /// Returns [`JmtProofVerifyError::EmptyTreeRoot`] if `expected_root_hash` is
    /// [`SPARSE_MERKLE_PLACEHOLDER_HASH`]: that is the root of an empty tree, so any key is trivially absent, and a
    /// root field that defaults to zero would otherwise accept an empty proof for any key. Use
    /// [`Self::verify_exclusion_or_empty_tree`] only when the root is bound to the specific tree by its own
    /// authenticated context.
    pub fn verify_exclusion(
        &self,
        scheme: JmtHashScheme,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
    ) -> Result<(), JmtProofVerifyError> {
        if *expected_root_hash == SPARSE_MERKLE_PLACEHOLDER_HASH {
            return Err(JmtProofVerifyError::EmptyTreeRoot);
        }
        self.verify(scheme, expected_root_hash, element_key, None)
    }

    /// Like [`Self::verify_exclusion`], but also accepts the empty-tree root
    /// ([`SPARSE_MERKLE_PLACEHOLDER_HASH`]) with an empty proof (no leaf, no siblings). Any other proof against the
    /// empty-tree root is rejected with [`JmtProofVerifyError::RootHashMismatch`].
    ///
    /// Only use this when `expected_root_hash` is bound to the specific tree and version by its own authenticated
    /// context (e.g. a tree whose every key was deleted legitimately has the empty root), never when a root field
    /// could default to zero.
    pub fn verify_exclusion_or_empty_tree(
        &self,
        scheme: JmtHashScheme,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
    ) -> Result<(), JmtProofVerifyError> {
        if *expected_root_hash == SPARSE_MERKLE_PLACEHOLDER_HASH {
            if self.leaf.is_none() && self.siblings.is_empty() {
                return Ok(());
            }
            return Err(JmtProofVerifyError::RootHashMismatch {
                actual_root_hash: self.compute_root_hash(scheme, element_key),
                expected_root_hash: *expected_root_hash,
            });
        }
        self.verify(scheme, expected_root_hash, element_key, None)
    }

    /// If `element_value` is present, verifies an element whose key is `element_key` and value is
    /// `element_value` exists in the Sparse Merkle Tree using the provided proof. Otherwise,
    /// verifies the proof is a valid non-inclusion proof that shows this key doesn't exist in the
    /// tree.
    fn verify(
        &self,
        scheme: JmtHashScheme,
        expected_root_hash: &TreeHash,
        element_key: &LeafKey,
        element_value: Option<&TreeHash>,
    ) -> Result<(), JmtProofVerifyError> {
        if self.siblings.len() > MAX_PROOF_SIBLINGS {
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

        let actual_root_hash = self.compute_root_hash(scheme, element_key);
        if actual_root_hash != *expected_root_hash {
            return Err(JmtProofVerifyError::RootHashMismatch {
                actual_root_hash,
                expected_root_hash: *expected_root_hash,
            });
        }

        Ok(())
    }

    /// Folds the leaf (or the placeholder) up through the siblings along `element_key`'s path.
    fn compute_root_hash(&self, scheme: JmtHashScheme, element_key: &LeafKey) -> TreeHash {
        let current_hash = self
            .leaf
            .as_ref()
            .map_or(SPARSE_MERKLE_PLACEHOLDER_HASH, |leaf| leaf.hash(scheme));
        self.siblings
            .iter()
            .zip(
                element_key
                    .iter_bits()
                    .rev()
                    .skip(MAX_PROOF_SIBLINGS.saturating_sub(self.siblings.len())),
            )
            .fold(current_hash, |hash, (sibling_node, bit)| {
                if bit {
                    SparseMerkleInternalNode::new(sibling_node.hash(scheme), hash).hash(scheme)
                } else {
                    SparseMerkleInternalNode::new(hash, sibling_node.hash(scheme)).hash(scheme)
                }
            })
    }

    /// Converts into a [`SparseMerkleProof`], hashing every [`NodeInProof::Leaf`] sibling under `scheme`. `scheme` must
    /// be the one the tree that generated the proof used.
    pub fn into_compact(self, scheme: JmtHashScheme) -> SparseMerkleProof {
        SparseMerkleProof::new(
            self.leaf,
            self.siblings.into_iter().map(|node| node.hash(scheme)).collect(),
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

/// A sibling in a [`SparseMerkleProofExt`].
///
/// Not canonical: `Leaf(l)` and `Other(l.hash(scheme))` verify identically, so proof bytes are not an identity. See the
/// malleability note on [`SparseMerkleProofExt`].
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
    pub fn hash(&self, scheme: JmtHashScheme) -> TreeHash {
        match self {
            Self::Leaf(leaf) => leaf.hash(scheme),
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

    pub fn hash(&self, scheme: JmtHashScheme) -> TreeHash {
        jmt_leaf_hash(scheme, &self.key, &self.value_hash)
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

    fn hash(&self, scheme: JmtHashScheme) -> TreeHash {
        jmt_internal_hash(scheme, &self.left_child, &self.right_child)
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
        self.pos.next_back().and_then(|x| self.get_bit(x))
    }
}

impl ExactSizeIterator for LeafKeyBitIterator<'_> {}

impl<'a> LeafKeyBitIterator<'a> {
    /// Constructs a new `LeafKeyBitIterator` using given `leaf_key_bytes`.
    fn new(leaf_key: LeafKeyRef<'a>) -> Self {
        LeafKeyBitIterator {
            leaf_key_bytes: leaf_key.bytes,
            pos: (0..leaf_key.bytes.len().saturating_mul(8)),
        }
    }

    /// Returns the `index`-th bit in the bytes.
    fn get_bit(&self, index: usize) -> Option<bool> {
        let pos = index / 8;
        let bit = 7usize.saturating_sub(index % 8);
        Some((self.leaf_key_bytes.get(pos)? >> bit) & 1 != 0)
    }
}

impl Iterator for LeafKeyBitIterator<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().and_then(|x| self.get_bit(x))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.pos.size_hint()
    }
}

// INITIAL-MODIFICATION: since we use our own `LeafKey` here, we need it to implement these for it
pub trait IteratedLeafKey {
    fn iter_bits(&self) -> LeafKeyBitIterator<'_>;

    fn get_nibble(&self, index: usize) -> Option<Nibble>;
}

impl IteratedLeafKey for LeafKey {
    fn iter_bits(&self) -> LeafKeyBitIterator<'_> {
        LeafKeyBitIterator::new(self.as_ref())
    }

    fn get_nibble(&self, index: usize) -> Option<Nibble> {
        Some(Nibble::from_masked(if index.is_multiple_of(2) {
            self.bytes.get(index / 2)? >> 4
        } else {
            self.bytes.get(index / 2)? & 0x0F
        }))
    }
}

impl IteratedLeafKey for LeafKeyRef<'_> {
    fn iter_bits(&self) -> LeafKeyBitIterator<'_> {
        LeafKeyBitIterator::new(*self)
    }

    fn get_nibble(&self, index: usize) -> Option<Nibble> {
        Some(Nibble::from_masked(if index.is_multiple_of(2) {
            self.bytes.get(index / 2)? >> 4
        } else {
            self.bytes.get(index / 2)? & 0x0F
        }))
    }
}

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/transaction/mod.rs#L57
pub type Version = u64;

// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/types/src/nibble/mod.rs#L20
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Nibble(u8);

impl<'de> Deserialize<'de> for Nibble {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let nibble = <u8 as Deserialize>::deserialize(deserializer)?;
        Self::try_from(nibble).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<u8> for Nibble {
    type Error = NibbleOutOfRange;

    fn try_from(nibble: u8) -> Result<Self, Self::Error> {
        if nibble >= 16 {
            return Err(NibbleOutOfRange(nibble));
        }
        Ok(Self(nibble))
    }
}

impl Nibble {
    /// Keeps the low 4 bits of `n`. Only for values that are `< 16` by construction.
    pub(crate) const fn from_masked(n: u8) -> Self {
        Self(n & 0x0F)
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
#[serde(try_from = "NibblePathRaw")]
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

#[derive(Deserialize)]
struct NibblePathRaw {
    num_nibbles: usize,
    bytes: Vec<u8>,
}

impl TryFrom<NibblePathRaw> for NibblePath {
    type Error = NibblePathError;

    fn try_from(raw: NibblePathRaw) -> Result<Self, Self::Error> {
        NibblePath::try_from_parts(raw.num_nibbles, raw.bytes)
    }
}

impl NibblePath {
    /// Builds a path from trusted storage without the checks in [`Self::try_from_parts`].
    #[cfg(feature = "minicbor")]
    pub(crate) fn from_parts_unchecked(num_nibbles: usize, bytes: Vec<u8>) -> Self {
        Self { num_nibbles, bytes }
    }

    /// Builds a decoded path, enforcing [`MAX_NIBBLE_PATH_LEN`], the byte length and a zero padding nibble.
    pub(crate) fn try_from_parts(num_nibbles: usize, bytes: Vec<u8>) -> Result<Self, NibblePathError> {
        if num_nibbles > MAX_NIBBLE_PATH_LEN {
            return Err(NibblePathError::TooLong {
                num_nibbles,
                max: MAX_NIBBLE_PATH_LEN,
            });
        }
        if num_nibbles.div_ceil(2) != bytes.len() {
            return Err(NibblePathError::LengthMismatch {
                num_nibbles,
                num_bytes: bytes.len(),
            });
        }
        if !num_nibbles.is_multiple_of(2) && bytes.last().is_some_and(|b| b & 0x0F != 0) {
            return Err(NibblePathError::NonZeroTrailingNibble);
        }
        Ok(Self { num_nibbles, bytes })
    }
}

/// Supports debug format by concatenating nibbles literally. For example, [0x12, 0xa0] with 3
/// nibbles will be printed as "12a".
impl fmt::Debug for NibblePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.nibbles().try_for_each(|x| write!(f, "{x:x}"))
    }
}

// INITIAL-MODIFICATION: just to show it in errors
impl fmt::Display for NibblePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex_chars = self
            .bytes
            .iter()
            .flat_map(|b| [b >> 4, b & 15])
            .filter_map(|b| char::from_digit(u32::from(b), 16))
            .take(self.num_nibbles);

        for ch in hex_chars {
            write!(f, "{ch}")?;
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
    ///
    /// Does not enforce [`MAX_NIBBLE_PATH_LEN`] (that cap applies on decode and in [`Self::new_odd`]); tree traversals
    /// bound their depth independently.
    pub fn new_even(bytes: Vec<u8>) -> Self {
        let num_nibbles = bytes.len().saturating_mul(2);
        NibblePath { num_nibbles, bytes }
    }

    /// Similar to `new_even()` but the bytes have one less nibble: the low nibble of the last byte is padding and
    /// must be zero.
    pub fn new_odd(bytes: Vec<u8>) -> Result<Self, NibblePathError> {
        let last = bytes.last().ok_or(NibblePathError::EmptyOddPath)?;
        if last & 0x0F != 0 {
            return Err(NibblePathError::NonZeroTrailingNibble);
        }
        let num_nibbles = bytes.len().saturating_mul(2).saturating_sub(1);
        if num_nibbles > MAX_NIBBLE_PATH_LEN {
            return Err(NibblePathError::TooLong {
                num_nibbles,
                max: MAX_NIBBLE_PATH_LEN,
            });
        }
        Ok(NibblePath { num_nibbles, bytes })
    }

    /// Adds a nibble to the end of the nibble path.
    pub fn push(&mut self, nibble: Nibble) {
        if self.num_nibbles.is_multiple_of(2) {
            self.bytes.push(u8::from(nibble) << 4);
        } else if let Some(last_byte) = self.bytes.last_mut() {
            // An odd path always has a last byte with a zero low nibble (enforced by every constructor)
            *last_byte |= u8::from(nibble);
        } else {
            // Not reachable: an odd path has at least one byte
        }
        self.num_nibbles = self.num_nibbles.saturating_add(1);
    }

    /// Pops a nibble from the end of the nibble path.
    pub fn pop(&mut self) -> Option<Nibble> {
        let poped_nibble = if self.num_nibbles.is_multiple_of(2) {
            self.bytes.last_mut().map(|last_byte| {
                let nibble = *last_byte & 0x0F;
                *last_byte &= 0xF0;
                Nibble::from_masked(nibble)
            })
        } else {
            self.bytes.pop().map(|byte| Nibble::from_masked(byte >> 4))
        };
        if poped_nibble.is_some() {
            self.num_nibbles = self.num_nibbles.saturating_sub(1);
        }
        poped_nibble
    }

    /// Returns the last nibble.
    pub fn last(&self) -> Option<Nibble> {
        let last_byte = self.bytes.last()?;
        if self.num_nibbles.is_multiple_of(2) {
            Some(Nibble::from_masked(*last_byte))
        } else {
            Some(Nibble::from_masked(*last_byte >> 4))
        }
    }

    /// Get the i-th bit.
    fn get_bit(&self, i: usize) -> Option<bool> {
        if i >= self.num_nibbles.saturating_mul(4) {
            return None;
        }
        let pos = i / 8;
        let bit = 7usize.saturating_sub(i % 8);
        Some(((self.bytes.get(pos)? >> bit) & 1) != 0)
    }

    /// Get the i-th nibble.
    pub fn get_nibble(&self, i: usize) -> Option<Nibble> {
        Some(Nibble::from_masked(
            self.bytes.get(i / 2)? >> (if i % 2 == 1 { 0 } else { 4 }),
        ))
    }

    /// Get a bit iterator iterates over the whole nibble path.
    pub fn bits(&self) -> NibbleBitIterator<'_> {
        NibbleBitIterator {
            nibble_path: self,
            pos: (0..self.num_nibbles.saturating_mul(4)),
        }
    }

    /// Get a nibble iterator iterates over the whole nibble path.
    pub fn nibbles(&self) -> NibbleIterator<'_> {
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

    /// Shortens the path to its first `len` nibbles.
    pub fn truncate(&mut self, len: usize) -> Result<(), NibblePathError> {
        if len > self.num_nibbles {
            return Err(NibblePathError::TruncateBeyondLength {
                len,
                num_nibbles: self.num_nibbles,
            });
        }
        self.num_nibbles = len;
        self.bytes.truncate(len.div_ceil(2));
        if !len.is_multiple_of(2) &&
            let Some(last_byte) = self.bytes.last_mut()
        {
            *last_byte &= 0xF0;
        }
        Ok(())
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
            Some(self.nibble_path.get_bit(self.pos.start)?)
        } else {
            None
        }
    }
}

/// BitIterator spits out a boolean each time. True/false denotes 1/0.
impl Iterator for NibbleBitIterator<'_> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        self.pos.next().and_then(|i| self.nibble_path.get_bit(i))
    }
}

/// Support iterating bits in reversed order.
impl DoubleEndedIterator for NibbleBitIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.pos.next_back().and_then(|i| self.nibble_path.get_bit(i))
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
        self.pos.next().and_then(|i| self.nibble_path.get_nibble(i))
    }
}

impl Peekable for NibbleIterator<'_> {
    /// Returns the `next()` value without advancing the iterator.
    fn peek(&self) -> Option<Self::Item> {
        if self.pos.start < self.pos.end {
            Some(self.nibble_path.get_nibble(self.pos.start)?)
        } else {
            None
        }
    }
}

impl<'a> NibbleIterator<'a> {
    fn new(nibble_path: &'a NibblePath, start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
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
            pos: (self.pos.start.saturating_mul(4)..self.pos.end.saturating_mul(4)),
        }
    }

    /// Cut and return the range of the underlying `nibble_path` that this iterator is iterating
    /// over as a new `NibblePath`
    pub fn get_nibble_path(&self) -> NibblePath {
        self.visited_nibbles().chain(self.remaining_nibbles()).collect()
    }

    /// Get the number of nibbles that this iterator covers.
    pub fn num_nibbles(&self) -> usize {
        debug_assert!(self.start <= self.pos.end); // invariant
        self.pos.end.saturating_sub(self.start)
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

/// Returns `true` if the first nibbles of `leaf_key` are `prefix`.
pub(crate) fn leaf_key_has_prefix(leaf_key: &LeafKey, prefix: &NibblePath) -> bool {
    prefix
        .nibbles()
        .enumerate()
        .all(|(i, nibble)| leaf_key.get_nibble(i) == Some(nibble))
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

    /// Generates parent node key at the same version based on this node key. Returns `None` for the root.
    pub fn gen_parent_node_key(&self) -> Option<Self> {
        let mut node_nibble_path = self.nibble_path().clone();
        node_nibble_path.pop()?;
        Some(Self::new(self.version, node_nibble_path))
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
    /// Creates a child, rejecting [`NodeType::Null`].
    pub fn try_new(hash: TreeHash, version: Version, node_type: NodeType) -> Result<Self, InternalNodeError> {
        if matches!(node_type, NodeType::Null) {
            return Err(InternalNodeError::NullChild);
        }
        Ok(Self {
            hash,
            version,
            node_type,
        })
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self.node_type, NodeType::Leaf)
    }

    pub fn leaf_count(&self) -> usize {
        match self.node_type {
            NodeType::Leaf => 1,
            NodeType::Internal { leaf_count } => leaf_count,
            // Rejected on deserialization and never constructed by the tree
            NodeType::Null => 0,
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
#[serde(try_from = "InternalNodeRaw")]
pub struct InternalNode {
    /// Up to 16 children.
    children: Children,
    /// Total number of leaves under this internal node
    leaf_count: usize,
}

#[derive(Deserialize)]
struct InternalNodeRaw {
    children: Children,
    leaf_count: usize,
}

impl TryFrom<InternalNodeRaw> for InternalNode {
    type Error = String;

    fn try_from(raw: InternalNodeRaw) -> Result<Self, Self::Error> {
        let InternalNodeRaw { children, leaf_count } = raw;
        let node = InternalNode::try_new(children).map_err(|e| e.to_string())?;
        if leaf_count != node.leaf_count {
            return Err(format!(
                "InternalNode leaf count {leaf_count} does not match children leaf count {}",
                node.leaf_count
            ));
        }
        Ok(node)
    }
}

impl InternalNode {
    /// Creates a new Internal node.
    ///
    /// Rejects a `Null` child, no children, and a single leaf child (which the tree always collapses into the leaf
    /// itself, and which would hash identically to it). A single internal child is allowed.
    pub fn try_new(mut children: Children) -> Result<Self, InternalNodeError> {
        let mut leaf_count = 0usize;
        for child in children.values() {
            if matches!(child.node_type, NodeType::Null) {
                return Err(InternalNodeError::NullChild);
            }
            leaf_count = leaf_count
                .checked_add(child.leaf_count())
                .ok_or(InternalNodeError::LeafCountOverflow)?;
        }
        match children.values().next() {
            None => return Err(InternalNodeError::NoChildren),
            Some(child) if children.len() == 1 && child.is_leaf() => return Err(InternalNodeError::SingleLeafChild),
            Some(_) => {},
        }
        children.sort_keys();
        Ok(Self { children, leaf_count })
    }

    /// Builds a node from trusted storage without the checks in [`Self::try_new`]. `children` must already be in
    /// ascending nibble order.
    #[cfg(feature = "minicbor")]
    pub(crate) fn from_sorted_children_unchecked(children: Children) -> Self {
        let leaf_count = children
            .values()
            .fold(0usize, |acc, child| acc.saturating_add(child.leaf_count()));
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

    pub fn hash(&self, scheme: JmtHashScheme) -> TreeHash {
        self.merkle_hash(
            0,  // start index
            16, // the number of leaves in the subtree of which we want the hash of root
            self.generate_bitmaps(),
            scheme,
        )
    }

    pub fn children_sorted(&self) -> impl Iterator<Item = (&Nibble, &Child)> {
        // let mut tmp = self.children.iter().collect::<Vec<_>>();
        // tmp.sort_by_key(|(nibble, _)| **nibble);
        // tmp.into_iter()
        self.children.iter()
    }

    #[cfg(feature = "minicbor")]
    pub(crate) fn num_children(&self) -> usize {
        self.children.len()
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
        debug_assert_eq!(existence_bitmap | leaf_bitmap, existence_bitmap);
        (existence_bitmap, leaf_bitmap)
    }

    /// Given a range [start, start + width), returns the sub-bitmap of that range.
    fn range_bitmaps(start: u8, width: u8, bitmaps: (u16, u16)) -> (u16, u16) {
        // `start` and `width` only ever come from crate-internal constants
        debug_assert!(start < 16 && width.count_ones() == 1 && start.is_multiple_of(width));
        debug_assert!(width <= 16 && start.saturating_add(width) <= 16);
        // A range with `start == 8` and `width == 4` will generate a mask 0b0000111100000000.
        // use as converting to smaller integer types when 'width == 16'
        #[allow(clippy::cast_possible_truncation)]
        let mask = ((1u32 << width).saturating_sub(1) << start) as u16;
        (bitmaps.0 & mask, bitmaps.1 & mask)
    }

    fn merkle_hash(
        &self,
        start: u8,
        width: u8,
        (existence_bitmap, leaf_bitmap): (u16, u16),
        scheme: JmtHashScheme,
    ) -> TreeHash {
        // Given a bit [start, 1 << nibble_height], return the value of that range.
        let (range_existence_bitmap, range_leaf_bitmap) =
            Self::range_bitmaps(start, width, (existence_bitmap, leaf_bitmap));
        if range_existence_bitmap == 0 {
            // No child under this subtree
            SPARSE_MERKLE_PLACEHOLDER_HASH
        } else if width == 1 || (range_existence_bitmap.count_ones() == 1 && range_leaf_bitmap != 0) {
            // Only 1 leaf child under this subtree or reach the lowest level
            #[allow(clippy::cast_possible_truncation)]
            let only_child_index = Nibble::from_masked(range_existence_bitmap.trailing_zeros() as u8);
            // The bitmaps are generated from `children` itself, so the child always exists
            self.child(only_child_index)
                .map_or(SPARSE_MERKLE_PLACEHOLDER_HASH, |child| child.hash)
        } else {
            let left_child = self.merkle_hash(start, width / 2, (range_existence_bitmap, range_leaf_bitmap), scheme);
            let right_child = self.merkle_hash(
                start.saturating_add(width / 2),
                width / 2,
                (range_existence_bitmap, range_leaf_bitmap),
                scheme,
            );
            SparseMerkleInternalNode::new(left_child, right_child).hash(scheme)
        }
    }

    fn gen_node_in_proof<K: Clone, R: TreeStoreReader<K>>(
        &self,
        start: u8,
        width: u8,
        (existence_bitmap, leaf_bitmap): (u16, u16),
        (tree_reader, node_key): (&R, &NodeKey),
        scheme: JmtHashScheme,
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
            let only_child_index = Nibble::from_masked(range_existence_bitmap.trailing_zeros() as u8);
            let only_child = self.child(only_child_index).ok_or(JmtStorageError::InconsistentState)?;
            if matches!(only_child.node_type, NodeType::Leaf) {
                let only_child_node_key = node_key.gen_child_node_key(only_child.version, only_child_index);
                match tree_reader.get_node(&only_child_node_key)? {
                    Node::Leaf(leaf_node)
                        if leaf_key_has_prefix(leaf_node.leaf_key(), only_child_node_key.nibble_path()) =>
                    {
                        NodeInProof::Leaf(SparseMerkleLeafNode::from(leaf_node))
                    },
                    // Corrupted internal node: in-memory leaf child is not a leaf on disk, or is stored under a path
                    // its key does not start with
                    Node::Leaf(_) | Node::Internal(_) | Node::Null => return Err(JmtStorageError::InconsistentState),
                }
            } else {
                NodeInProof::Other(only_child.hash)
            }
        } else {
            let left_child = self.merkle_hash(start, width / 2, (range_existence_bitmap, range_leaf_bitmap), scheme);
            let right_child = self.merkle_hash(
                start.saturating_add(width / 2),
                width / 2,
                (range_existence_bitmap, range_leaf_bitmap),
                scheme,
            );
            NodeInProof::Other(SparseMerkleInternalNode::new(left_child, right_child).hash(scheme))
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
        scheme: JmtHashScheme,
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
                    scheme,
                )?);
            } else {
                siblings.push(
                    self.merkle_hash(sibling_half_start, width, (existence_bitmap, leaf_bitmap), scheme)
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
                let only_child_index = Nibble::from_masked(range_existence_bitmap.trailing_zeros() as u8);
                let only_child_version = self
                    .child(only_child_index)
                    .ok_or(JmtStorageError::InconsistentState)?
                    .version;
                return Ok((
                    Some(node_key.gen_child_node_key(only_child_version, only_child_index)),
                    siblings,
                ));
            }
        }
        // Not reachable: the lowest level (`width == 1`) always returns above
        Err(JmtStorageError::InconsistentState)
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
    pub fn leaf_hash(&self, scheme: JmtHashScheme) -> TreeHash {
        jmt_leaf_hash(scheme, &self.leaf_key, &self.value_hash)
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
    pub fn hash(&self, scheme: JmtHashScheme) -> TreeHash {
        match self {
            Node::Internal(internal_node) => internal_node.hash(scheme),
            Node::Leaf(leaf_node) => leaf_node.leaf_hash(scheme),
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

    #[error("Attempted to find an index that does not exist in the tree")]
    IndexNotFound,

    #[error("Version {version} must be greater than the persisted version {persisted_version}")]
    NonMonotonicVersion {
        persisted_version: Version,
        version: Version,
    },
}

impl From<InternalNodeError> for JmtStorageError {
    fn from(_: InternalNodeError) -> Self {
        JmtStorageError::InconsistentState
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof_with_siblings(n: usize) -> SparseMerkleProofExt {
        SparseMerkleProofExt::new(None, vec![NodeInProof::Other(TreeHash::zero()); n])
    }

    #[test]
    fn it_rejects_proofs_with_too_many_siblings() {
        let ok = proof_with_siblings(MAX_PROOF_SIBLINGS);
        let bytes = borsh::to_vec(&ok).unwrap();
        assert_eq!(borsh::from_slice::<SparseMerkleProofExt>(&bytes).unwrap(), ok);
        let json = serde_json::to_string(&ok).unwrap();
        assert_eq!(serde_json::from_str::<SparseMerkleProofExt>(&json).unwrap(), ok);

        let too_many = proof_with_siblings(MAX_PROOF_SIBLINGS + 1);
        let bytes = borsh::to_vec(&too_many).unwrap();
        borsh::from_slice::<SparseMerkleProofExt>(&bytes).unwrap_err();
        let json = serde_json::to_string(&too_many).unwrap();
        serde_json::from_str::<SparseMerkleProofExt>(&json).unwrap_err();

        // Huge declared length with no data is rejected without allocating
        let mut bytes = borsh::to_vec(&Option::<SparseMerkleLeafNode>::None).unwrap();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        borsh::from_slice::<SparseMerkleProofExt>(&bytes).unwrap_err();
    }

    fn nibble(n: u8) -> Nibble {
        Nibble::try_from(n).unwrap()
    }

    fn leaf_child() -> Child {
        Child::try_new(TreeHash::zero(), 1, NodeType::Leaf).unwrap()
    }

    fn internal_child(leaf_count: usize) -> Child {
        Child::try_new(TreeHash::zero(), 1, NodeType::Internal { leaf_count }).unwrap()
    }

    fn children(entries: Vec<(u8, Child)>) -> Children {
        entries.into_iter().map(|(n, child)| (nibble(n), child)).collect()
    }

    #[test]
    fn it_rejects_out_of_range_nibbles() {
        assert_eq!(serde_json::from_str::<Nibble>("15").unwrap(), nibble(15));
        serde_json::from_str::<Nibble>("16").unwrap_err();
        assert_eq!(u8::from(Nibble::try_from(15u8).unwrap()), 15);
        assert_eq!(Nibble::try_from(16u8).unwrap_err(), NibbleOutOfRange(16));
    }

    #[test]
    fn it_builds_odd_nibble_paths_fallibly() {
        assert_eq!(NibblePath::new_odd(vec![]).unwrap_err(), NibblePathError::EmptyOddPath);
        assert_eq!(
            NibblePath::new_odd(vec![0x12]).unwrap_err(),
            NibblePathError::NonZeroTrailingNibble
        );
        let path = NibblePath::new_odd(vec![0x10]).unwrap();
        assert_eq!(path.num_nibbles(), 1);
        assert_eq!(path.last(), Some(nibble(1)));

        let path = NibblePath::new_odd([vec![0x11u8; 31], vec![0x10]].concat()).unwrap();
        assert_eq!(path.num_nibbles(), 63);
        assert_eq!(
            NibblePath::new_odd([vec![0x11u8; 32], vec![0x10]].concat()).unwrap_err(),
            NibblePathError::TooLong {
                num_nibbles: 65,
                max: MAX_NIBBLE_PATH_LEN
            }
        );
    }

    #[test]
    fn it_truncates_nibble_paths_fallibly() {
        let mut path = NibblePath::new_even(vec![0x12, 0x34]);
        assert_eq!(path.truncate(5).unwrap_err(), NibblePathError::TruncateBeyondLength {
            len: 5,
            num_nibbles: 4
        });
        path.truncate(4).unwrap();
        assert_eq!(path, NibblePath::new_even(vec![0x12, 0x34]));
        // An odd length zeroes the trailing nibble
        path.truncate(3).unwrap();
        assert_eq!(path.bytes(), &[0x12, 0x30]);
        assert_eq!(path, NibblePath::new_odd(vec![0x12, 0x30]).unwrap());
        path.truncate(0).unwrap();
        assert!(path.is_empty());
    }

    #[test]
    fn it_returns_no_parent_for_the_root() {
        let root = NodeKey::new_empty_path(1);
        assert_eq!(root.gen_parent_node_key(), None);
        let child = NodeKey::new(1, NibblePath::new_odd(vec![0xa0]).unwrap());
        assert_eq!(child.gen_parent_node_key(), Some(root));
    }

    #[test]
    fn it_validates_internal_node_children() {
        assert_eq!(
            Child::try_new(TreeHash::zero(), 1, NodeType::Null).unwrap_err(),
            InternalNodeError::NullChild
        );
        let null_child = Child {
            hash: TreeHash::zero(),
            version: 1,
            node_type: NodeType::Null,
        };
        assert_eq!(
            InternalNode::try_new(children(vec![(0, leaf_child()), (1, null_child)])).unwrap_err(),
            InternalNodeError::NullChild
        );
        assert_eq!(
            InternalNode::try_new(Children::new()).unwrap_err(),
            InternalNodeError::NoChildren
        );
        assert_eq!(
            InternalNode::try_new(children(vec![(0, leaf_child())])).unwrap_err(),
            InternalNodeError::SingleLeafChild
        );
        let node = InternalNode::try_new(children(vec![(4, internal_child(2))])).unwrap();
        assert_eq!(node.leaf_count(), 2);
        let node = InternalNode::try_new(children(vec![(4, leaf_child()), (2, leaf_child())])).unwrap();
        assert_eq!(node.leaf_count(), 2);
        assert_eq!(node.children_sorted().map(|(n, _)| u8::from(*n)).collect::<Vec<_>>(), [
            2, 4
        ]);
    }

    #[test]
    fn it_validates_deserialized_internal_nodes() {
        let node = InternalNode::try_new(children(vec![(3, leaf_child()), (1, internal_child(2))])).unwrap();
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(serde_json::from_value::<InternalNode>(json.clone()).unwrap(), node);

        let mut bad_count = json.clone();
        *bad_count.pointer_mut("/leaf_count").unwrap() = 4.into();
        serde_json::from_value::<InternalNode>(bad_count).unwrap_err();

        let mut null_child = json;
        *null_child.pointer_mut("/children/3/node_type").unwrap() = "Null".into();
        serde_json::from_value::<InternalNode>(null_child).unwrap_err();

        let no_children = serde_json::json!({ "children": {}, "leaf_count": 0 });
        let err = serde_json::from_value::<InternalNode>(no_children).unwrap_err();
        assert!(err.to_string().contains("no children"), "{err}");

        let single_leaf = InternalNode {
            children: children(vec![(3, leaf_child())]),
            leaf_count: 1,
        };
        let err = serde_json::from_value::<InternalNode>(serde_json::to_value(&single_leaf).unwrap()).unwrap_err();
        assert!(err.to_string().contains("single leaf child"), "{err}");
    }

    #[test]
    fn it_validates_deserialized_nibble_paths() {
        let path: NibblePath = [1u8, 2, 3].into_iter().map(nibble).collect();
        let json = serde_json::to_value(&path).unwrap();
        assert_eq!(serde_json::from_value::<NibblePath>(json).unwrap(), path);

        let bad_len = serde_json::json!({ "num_nibbles": 5, "bytes": [0x12] });
        serde_json::from_value::<NibblePath>(bad_len).unwrap_err();
        let bad_tail = serde_json::json!({ "num_nibbles": 3, "bytes": [0x12, 0x34] });
        serde_json::from_value::<NibblePath>(bad_tail).unwrap_err();
    }

    #[test]
    fn it_rejects_nibble_paths_longer_than_a_leaf_key() {
        let max = serde_json::json!({ "num_nibbles": 64, "bytes": vec![0x11u8; 32] });
        let path = serde_json::from_value::<NibblePath>(max).unwrap();
        assert_eq!(path.num_nibbles(), MAX_NIBBLE_PATH_LEN);

        let too_long = NibblePathRaw {
            num_nibbles: 65,
            bytes: [vec![0x11u8; 32], vec![0x10]].concat(),
        };
        assert_eq!(NibblePath::try_from(too_long).unwrap_err(), NibblePathError::TooLong {
            num_nibbles: 65,
            max: MAX_NIBBLE_PATH_LEN
        });
        let mut bytes = vec![0x11u8; 32];
        bytes.push(0x10);
        let too_long = serde_json::json!({ "num_nibbles": 65, "bytes": bytes });
        let err = serde_json::from_value::<NibblePath>(too_long).unwrap_err();
        assert!(err.to_string().contains("max is 64"), "{err}");
    }

    #[test]
    fn it_rejects_exclusion_against_the_empty_tree_root_unless_asked() {
        let key = LeafKey::new(jmt_node_hash(&1u64));
        let empty = SparseMerkleProofExt::new(None, vec![]);
        let zero = TreeHash::zero();
        assert!(matches!(
            empty.verify_exclusion(JmtHashScheme::V1, &zero, &key),
            Err(JmtProofVerifyError::EmptyTreeRoot)
        ));
        empty
            .verify_exclusion_or_empty_tree(JmtHashScheme::V1, &zero, &key)
            .unwrap();

        let with_sibling = SparseMerkleProofExt::new(None, vec![NodeInProof::Other(jmt_node_hash(&2u64))]);
        assert!(matches!(
            with_sibling.verify_exclusion_or_empty_tree(JmtHashScheme::V1, &zero, &key),
            Err(JmtProofVerifyError::RootHashMismatch { .. })
        ));
        let other_key = LeafKey::new(jmt_node_hash(&3u64));
        let with_leaf = SparseMerkleProofExt::new(Some(SparseMerkleLeafNode::new(other_key, zero)), vec![]);
        assert!(matches!(
            with_leaf.verify_exclusion_or_empty_tree(JmtHashScheme::V1, &zero, &key),
            Err(JmtProofVerifyError::RootHashMismatch { .. })
        ));

        // A non-empty root still goes through the full check
        let root = SparseMerkleLeafNode::new(other_key, zero).hash(JmtHashScheme::V1);
        with_leaf.verify_exclusion(JmtHashScheme::V1, &root, &key).unwrap();
        with_leaf
            .verify_exclusion_or_empty_tree(JmtHashScheme::V1, &root, &key)
            .unwrap();
        empty
            .verify_exclusion_or_empty_tree(JmtHashScheme::V1, &root, &key)
            .unwrap_err();
    }

    #[test]
    fn it_iterates_nibble_path_bits() {
        let path: NibblePath = [0xau8, 0x5].into_iter().map(nibble).collect();
        let bits = path.bits().collect::<Vec<_>>();
        assert_eq!(bits, [true, false, true, false, false, true, false, true]);
    }
}
