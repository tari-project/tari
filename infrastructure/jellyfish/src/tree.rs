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

use std::{collections::BTreeMap, marker::PhantomData};

use super::{
    LeafKeyRef,
    TreeHash,
    store::TreeStoreReader,
    types::{
        Child,
        InternalNode,
        IteratedLeafKey,
        JmtStorageError,
        LeafKey,
        LeafNode,
        MAX_NIBBLE_PATH_LEN,
        Nibble,
        NibblePath,
        Node,
        NodeKey,
        SPARSE_MERKLE_PLACEHOLDER_HASH,
        SparseMerkleProof,
        SparseMerkleProofExt,
        SparseMerkleRangeProof,
        Version,
        leaf_key_has_prefix,
    },
};

// INITIAL-MODIFICATION: the original used a known key size (32) as a limit
const SANITY_NIBBLE_LIMIT: usize = 1000;

pub type ProofValue<P> = (TreeHash, P, Version);

// SOURCE: https://github.com/radixdlt/radixdlt-scrypto/blob/ca8e553c31a956c0851c1855291efe4a47fb5c97/radix-engine-stores/src/hash_tree/jellyfish.rs
// SOURCE: https://github.com/aptos-labs/aptos-core/blob/1.0.4/storage/jellyfish-merkle/src/lib.rs#L329
/// The Jellyfish Merkle tree data structure. See [`crate`] for description.
pub struct JellyfishMerkleTree<'a, R, P> {
    reader: &'a R,
    _payload: PhantomData<P>,
}

impl<'a, R: 'a + TreeStoreReader<P>, P: Clone> JellyfishMerkleTree<'a, R, P> {
    /// Creates a `JellyfishMerkleTree` backed by the given [`TreeReader`](trait.TreeReader.html).
    pub fn new(reader: &'a R) -> Self {
        Self {
            reader,
            _payload: PhantomData,
        }
    }

    /// For each value set:
    /// Returns the new nodes and values in a batch after applying `value_set`. For
    /// example, if after transaction `T_i` the committed state of tree in the persistent storage
    /// looks like the following structure:
    ///
    /// ```text
    ///              S_i
    ///             /   \
    ///            .     .
    ///           .       .
    ///          /         \
    ///         o           x
    ///        / \
    ///       A   B
    ///        storage (disk)
    /// ```
    ///
    /// where `A` and `B` denote the states of two adjacent accounts, and `x` is a sibling subtree
    /// of the path from root to A and B in the tree. Then a `value_set` produced by the next
    /// transaction `T_{i+1}` modifies other accounts `C` and `D` exist in the subtree under `x`, a
    /// new partial tree will be constructed in memory and the structure will be:
    ///
    /// ```text
    ///                 S_i      |      S_{i+1}
    ///                /   \     |     /       \
    ///               .     .    |    .         .
    ///              .       .   |   .           .
    ///             /         \  |  /             \
    ///            /           x | /               x'
    ///           o<-------------+-               / \
    ///          / \             |               C   D
    ///         A   B            |
    ///           storage (disk) |    cache (memory)
    /// ```
    ///
    /// With this design, we are able to query the global state in persistent storage and
    /// generate the proposed tree delta based on a specific root hash and `value_set`. For
    /// example, if we want to execute another transaction `T_{i+1}'`, we can use the tree `S_i` in
    /// storage and apply the `value_set` of transaction `T_{i+1}`. Then if the storage commits
    /// the returned batch, the state `S_{i+1}` is ready to be read from the tree by calling
    /// [`get_with_proof`](struct.JellyfishMerkleTree.html#method.get_with_proof). Anything inside
    /// the batch is not reachable from public interfaces before being committed.
    pub fn batch_put_value_set<I: IntoIterator<Item = (LeafKey, Option<(TreeHash, P)>)>>(
        &self,
        value_set: I,
        persisted_version: Option<Version>,
        version: Version,
    ) -> Result<(TreeHash, TreeUpdateBatch<P>), JmtStorageError> {
        // Writing at or below the persisted version would record live nodes as stale and write their replacements
        // under the same keys, so pruning the stale index would delete the live tree.
        if let Some(persisted_version) = persisted_version.filter(|p| version <= *p) {
            return Err(JmtStorageError::NonMonotonicVersion {
                persisted_version,
                version,
            });
        }
        let value_set = value_set.into_iter().collect::<BTreeMap<_, _>>();
        let deduped_and_sorted_kvs = value_set.iter().map(|(k, v)| (k, v.as_ref())).collect::<Vec<_>>();

        let mut batch = TreeUpdateBatch::new();
        let root_node_opt = if let Some(persisted_version) = persisted_version {
            self.batch_insert_at(
                &NodeKey::new_empty_path(persisted_version),
                version,
                deduped_and_sorted_kvs.as_slice(),
                0,
                None,
                &mut batch,
            )?
        } else {
            Self::batch_update_subtree(
                &NodeKey::new_empty_path(version),
                version,
                deduped_and_sorted_kvs.as_slice(),
                0,
                &mut batch,
            )?
        };

        let node_key = NodeKey::new_empty_path(version);
        let root_hash = if let Some(root_node) = root_node_opt {
            let hash = root_node.hash();
            batch.put_node(node_key, root_node);
            hash
        } else {
            batch.put_node(node_key, Node::Null);
            SPARSE_MERKLE_PLACEHOLDER_HASH
        };

        Ok((root_hash, batch))
    }

    fn batch_insert_at(
        &self,
        node_key: &NodeKey,
        version: Version,
        kvs: &[(&LeafKey, Option<&(TreeHash, P)>)],
        depth: usize,
        expected_leaf: Option<bool>,
        batch: &mut TreeUpdateBatch<P>,
    ) -> Result<Option<Node<P>>, JmtStorageError> {
        let node = self.reader.get_node(node_key)?;
        // `expected_leaf` is the kind the parent's `Child` entry records (`None` at the root)
        if expected_leaf.is_some_and(|expected_leaf| expected_leaf != node.is_leaf()) {
            return Err(JmtStorageError::InconsistentState);
        }
        batch.put_stale_node(node_key.clone(), version, &node);

        match node {
            Node::Internal(internal_node) => {
                // There is a small possibility that the old internal node is intact.
                // Traverse all the path touched by `kvs` from this internal node.
                let range_iter = NibbleRangeIterator::new(kvs, depth);
                // INITIAL-MODIFICATION: there was a par_iter (conditionally) used here
                let new_children = range_iter
                    .map(|(left, right)| {
                        self.insert_at_child(node_key, &internal_node, version, kvs, left, right, depth, batch)
                    })
                    .collect::<Result<Vec<_>, JmtStorageError>>()?;

                // Reuse the current `InternalNode` in memory to create a new internal node.
                let mut old_children = internal_node.into_children();
                let mut new_created_children: Vec<(Nibble, Node<P>)> = Vec::new();
                for (child_nibble, child_option) in new_children {
                    if let Some(child) = child_option {
                        new_created_children.push((child_nibble, child));
                    } else {
                        old_children.swap_remove(&child_nibble);
                    }
                }

                if old_children.is_empty() && new_created_children.is_empty() {
                    return Ok(None);
                }
                if old_children.len() <= 1 && new_created_children.len() <= 1 {
                    if let Some((new_nibble, new_child)) = new_created_children.first() {
                        if let Some((old_nibble, _old_child)) = old_children.iter().next() {
                            if old_nibble == new_nibble && new_child.is_leaf() {
                                return Ok(Some(new_child.clone()));
                            }
                        } else if new_child.is_leaf() {
                            return Ok(Some(new_child.clone()));
                        } else {
                            // Nothing to do
                        }
                    } else if let Some((old_child_nibble, old_child)) = old_children.iter().next() {
                        if old_child.is_leaf() {
                            let old_child_node_key = node_key.gen_child_node_key(old_child.version, *old_child_nibble);
                            // The parent records a leaf here, so anything else (or a leaf stored under a path its key
                            // does not start with) is a corrupt store and must not be lifted into the new tree
                            let old_child_node = match self.reader.get_node(&old_child_node_key)? {
                                Node::Leaf(leaf)
                                    if leaf_key_has_prefix(leaf.leaf_key(), old_child_node_key.nibble_path()) =>
                                {
                                    Node::Leaf(leaf)
                                },
                                Node::Leaf(_) | Node::Internal(_) | Node::Null => {
                                    return Err(JmtStorageError::InconsistentState);
                                },
                            };
                            batch.put_stale_node(old_child_node_key, version, &old_child_node);
                            return Ok(Some(old_child_node));
                        }
                    } else {
                        // Not reachable: both empty returned above
                    }
                }

                let mut new_children = old_children;
                for (child_index, new_child_node) in new_created_children {
                    let new_child_node_key = node_key.gen_child_node_key(version, child_index);
                    new_children.insert(
                        child_index,
                        Child::try_new(new_child_node.hash(), version, new_child_node.node_type())?,
                    );
                    batch.put_node(new_child_node_key, new_child_node);
                }
                let new_internal_node = InternalNode::try_new(new_children)?;
                Ok(Some(new_internal_node.into()))
            },
            Node::Leaf(leaf_node) => {
                Self::batch_update_subtree_with_existing_leaf(node_key, version, leaf_node, kvs, depth, batch)
            },
            Node::Null => {
                // Null node can only exist at depth 0
                if depth != 0 {
                    return Err(JmtStorageError::InconsistentState);
                }
                Self::batch_update_subtree(node_key, version, kvs, 0, batch)
            },
        }
    }

    fn insert_at_child(
        &self,
        node_key: &NodeKey,
        internal_node: &InternalNode,
        version: Version,
        kvs: &[(&LeafKey, Option<&(TreeHash, P)>)],
        left: usize,
        right: usize,
        depth: usize,
        batch: &mut TreeUpdateBatch<P>,
    ) -> Result<(Nibble, Option<Node<P>>), JmtStorageError> {
        let child_index = kvs
            .get(left)
            .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?
            .0
            .get_nibble(depth)
            .ok_or(JmtStorageError::IndexNotFound)?;
        let child = internal_node.child(child_index);

        let new_child_node_option = match child {
            Some(child) => self.batch_insert_at(
                &node_key.gen_child_node_key(child.version, child_index),
                version,
                kvs.get(left..=right)
                    .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?,
                depth.saturating_add(1),
                Some(child.is_leaf()),
                batch,
            )?,
            None => Self::batch_update_subtree(
                &node_key.gen_child_node_key(version, child_index),
                version,
                kvs.get(left..=right)
                    .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?,
                depth.saturating_add(1),
                batch,
            )?,
        };

        Ok((child_index, new_child_node_option))
    }

    fn batch_update_subtree_with_existing_leaf(
        node_key: &NodeKey,
        version: Version,
        existing_leaf_node: LeafNode<P>,
        kvs: &[(&LeafKey, Option<&(TreeHash, P)>)],
        depth: usize,
        batch: &mut TreeUpdateBatch<P>,
    ) -> Result<Option<Node<P>>, JmtStorageError> {
        let existing_leaf_key = existing_leaf_node.leaf_key();
        // A leaf stored under a path its key does not start with would be moved by its own key
        if !leaf_key_has_prefix(existing_leaf_key, node_key.nibble_path()) {
            return Err(JmtStorageError::InconsistentState);
        }

        if kvs.len() == 1 &&
            kvs.first()
                .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?
                .0 ==
                existing_leaf_key
        {
            if let (key, Some((value_hash, payload))) = *kvs
                .first()
                .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?
            {
                let new_leaf_node = Node::new_leaf(*key, *value_hash, payload.clone(), version);
                Ok(Some(new_leaf_node))
            } else {
                Ok(None)
            }
        } else {
            let existing_leaf_bucket = existing_leaf_key
                .get_nibble(depth)
                .ok_or(JmtStorageError::IndexNotFound)?;
            let mut isolated_existing_leaf = true;
            let mut children = vec![];
            for (left, right) in NibbleRangeIterator::new(kvs, depth) {
                let child_index = kvs
                    .get(left)
                    .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?
                    .0
                    .get_nibble(depth)
                    .ok_or(JmtStorageError::IndexNotFound)?;
                let child_node_key = node_key.gen_child_node_key(version, child_index);
                if let Some(new_child_node) = if existing_leaf_bucket == child_index {
                    isolated_existing_leaf = false;
                    Self::batch_update_subtree_with_existing_leaf(
                        &child_node_key,
                        version,
                        existing_leaf_node.clone(),
                        kvs.get(left..=right)
                            .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?,
                        depth.saturating_add(1),
                        batch,
                    )?
                } else {
                    Self::batch_update_subtree(
                        &child_node_key,
                        version,
                        kvs.get(left..=right)
                            .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?,
                        depth.saturating_add(1),
                        batch,
                    )?
                } {
                    children.push((child_index, new_child_node));
                }
            }
            if isolated_existing_leaf {
                children.push((existing_leaf_bucket, existing_leaf_node.into()));
            }

            if children.is_empty() {
                Ok(None)
            } else if children.len() == 1 && children.first().is_some_and(|(_, child)| child.is_leaf()) {
                Ok(children.pop().map(|(_, child)| child))
            } else {
                let new_internal_node = InternalNode::try_new(
                    children
                        .into_iter()
                        .map(|(child_index, new_child_node)| {
                            let new_child_node_key = node_key.gen_child_node_key(version, child_index);
                            let result = (
                                child_index,
                                Child::try_new(new_child_node.hash(), version, new_child_node.node_type())?,
                            );
                            batch.put_node(new_child_node_key, new_child_node);
                            Ok(result)
                        })
                        .collect::<Result<_, JmtStorageError>>()?,
                )?;
                Ok(Some(new_internal_node.into()))
            }
        }
    }

    fn batch_update_subtree(
        node_key: &NodeKey,
        version: Version,
        kvs: &[(&LeafKey, Option<&(TreeHash, P)>)],
        depth: usize,
        batch: &mut TreeUpdateBatch<P>,
    ) -> Result<Option<Node<P>>, JmtStorageError> {
        if kvs.len() == 1 {
            if let (&key, Some((value_hash, payload))) = *kvs
                .first()
                .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?
            {
                let new_leaf_node = Node::new_leaf(key, *value_hash, payload.clone(), version);
                Ok(Some(new_leaf_node))
            } else {
                Ok(None)
            }
        } else {
            let mut children = vec![];
            for (left, right) in NibbleRangeIterator::new(kvs, depth) {
                let child_index = kvs
                    .get(left)
                    .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?
                    .0
                    .get_nibble(depth)
                    .ok_or(JmtStorageError::IndexNotFound)?;
                let child_node_key = node_key.gen_child_node_key(version, child_index);
                if let Some(new_child_node) = Self::batch_update_subtree(
                    &child_node_key,
                    version,
                    kvs.get(left..=right)
                        .ok_or(JmtStorageError::UnexpectedError("Out of bounds".to_string()))?,
                    depth.saturating_add(1),
                    batch,
                )? {
                    children.push((child_index, new_child_node))
                }
            }
            if children.is_empty() {
                Ok(None)
            } else if children.len() == 1 && children.first().is_some_and(|(_, child)| child.is_leaf()) {
                Ok(children.pop().map(|(_, child)| child))
            } else {
                let new_internal_node = InternalNode::try_new(
                    children
                        .into_iter()
                        .map(|(child_index, new_child_node)| {
                            let new_child_node_key = node_key.gen_child_node_key(version, child_index);
                            let result = (
                                child_index,
                                Child::try_new(new_child_node.hash(), version, new_child_node.node_type())?,
                            );
                            batch.put_node(new_child_node_key, new_child_node);
                            Ok(result)
                        })
                        .collect::<Result<_, JmtStorageError>>()?,
                )?;
                Ok(Some(new_internal_node.into()))
            }
        }
    }

    /// Returns the value (if applicable) and the corresponding merkle proof.
    pub fn get_with_proof(
        &self,
        key: LeafKeyRef<'_>,
        version: Version,
    ) -> Result<(Option<ProofValue<P>>, SparseMerkleProof), JmtStorageError> {
        self.get_with_proof_ext(key, version)
            .map(|(value, proof_ext)| (value, proof_ext.into()))
    }

    pub fn get_with_proof_ext(
        &self,
        key: LeafKeyRef<'_>,
        version: Version,
    ) -> Result<(Option<ProofValue<P>>, SparseMerkleProofExt), JmtStorageError> {
        // Empty tree just returns proof with no sibling hash.
        let mut next_node_key = NodeKey::new_empty_path(version);
        let mut siblings = vec![];
        let nibble_path = NibblePath::new_even(key.bytes.to_vec());
        let mut nibble_iter = nibble_path.nibbles();
        // The kind the parent's `Child` entry records for `next_node_key` (`None` at the root)
        let mut expected_leaf = None;

        for _nibble_depth in 0..SANITY_NIBBLE_LIMIT {
            let next_node = self.reader.get_node(&next_node_key)?;
            if expected_leaf.is_some_and(|expected_leaf| expected_leaf != next_node.is_leaf()) {
                return Err(JmtStorageError::InconsistentState);
            }
            match next_node {
                Node::Internal(internal_node) => {
                    let queried_child_index = nibble_iter.next().ok_or(JmtStorageError::InconsistentState)?;
                    let (child_node_key, mut siblings_in_internal) = internal_node.get_child_with_siblings(
                        &next_node_key,
                        queried_child_index,
                        Some(self.reader),
                    )?;
                    siblings.append(&mut siblings_in_internal);
                    next_node_key = match child_node_key {
                        Some(node_key) => {
                            let child = node_key
                                .nibble_path()
                                .last()
                                .and_then(|nibble| internal_node.child(nibble))
                                .ok_or(JmtStorageError::InconsistentState)?;
                            expected_leaf = Some(child.is_leaf());
                            node_key
                        },
                        None => {
                            return Ok((
                                None,
                                SparseMerkleProofExt::new(None, {
                                    siblings.reverse();
                                    siblings
                                }),
                            ));
                        },
                    };
                },
                Node::Leaf(leaf_node) => {
                    if !leaf_key_has_prefix(leaf_node.leaf_key(), next_node_key.nibble_path()) {
                        return Err(JmtStorageError::InconsistentState);
                    }
                    return Ok((
                        if leaf_node.leaf_key().as_ref() == key {
                            Some((leaf_node.value_hash(), leaf_node.payload().clone(), leaf_node.version()))
                        } else {
                            None
                        },
                        SparseMerkleProofExt::new(Some(leaf_node.into()), {
                            siblings.reverse();
                            siblings
                        }),
                    ));
                },
                Node::Null => {
                    // Null only represents the empty tree, so it can only be the root
                    if !next_node_key.nibble_path().is_empty() {
                        return Err(JmtStorageError::InconsistentState);
                    }
                    return Ok((None, SparseMerkleProofExt::new(None, vec![])));
                },
            }
        }
        Err(JmtStorageError::InconsistentState)
    }

    /// Gets the proof that shows a list of keys up to `rightmost_key_to_prove` exist at `version`.
    pub fn get_range_proof(
        &self,
        rightmost_key_to_prove: LeafKeyRef<'_>,
        version: Version,
    ) -> Result<SparseMerkleRangeProof, JmtStorageError> {
        let (leaf, proof) = self.get_with_proof(rightmost_key_to_prove, version)?;
        // rightmost_key_to_prove must exist
        if leaf.is_none() {
            return Err(JmtStorageError::IndexNotFound);
        }

        let siblings = proof
            .siblings()
            .iter()
            .rev()
            .zip(rightmost_key_to_prove.iter_bits())
            .filter_map(|(sibling, bit)| {
                // We only need to keep the siblings on the right.
                if bit { None } else { Some(*sibling) }
            })
            .rev()
            .collect();
        Ok(SparseMerkleRangeProof::new(siblings))
    }

    fn get_root_node(&self, version: Version) -> Result<Node<P>, JmtStorageError> {
        let root_node_key = NodeKey::new_empty_path(version);
        self.reader.get_node(&root_node_key)
    }

    pub fn get_root_hash(&self, version: Version) -> Result<TreeHash, JmtStorageError> {
        self.get_root_node(version).map(|n| n.hash())
    }

    pub fn get_leaf_count(&self, version: Version) -> Result<usize, JmtStorageError> {
        self.get_root_node(version).map(|n| n.leaf_count())
    }

    /// Returns the keys of `key` and every node below it, in post-order (children in nibble order before their
    /// parent). Returns [`JmtStorageError::InconsistentState`] if the store links nodes deeper than a leaf key allows.
    pub fn get_all_nodes_referenced(&self, key: NodeKey) -> Result<Vec<NodeKey>, JmtStorageError> {
        // Visit each node before its children, pushing the children so that the last one is visited first. The
        // reverse of that order is the post-order with children in nibble order.
        // Each key carries the kind its parent's `Child` entry records (`None` for the starting key).
        let mut out_keys = vec![];
        let mut stack = vec![(key, None)];
        while let Some((key, expected_leaf)) = stack.pop() {
            if key.nibble_path().num_nibbles() > MAX_NIBBLE_PATH_LEN {
                return Err(JmtStorageError::InconsistentState);
            }
            let node = self.reader.get_node(&key)?;
            if expected_leaf.is_some_and(|expected_leaf: bool| expected_leaf != node.is_leaf()) {
                return Err(JmtStorageError::InconsistentState);
            }
            match node {
                Node::Internal(internal_node) => {
                    for (child_nibble, child) in internal_node.children_sorted() {
                        stack.push((
                            key.gen_child_node_key(child.version, *child_nibble),
                            Some(child.is_leaf()),
                        ));
                    }
                },
                Node::Leaf(_) => {},
                // Null only represents the empty tree, so it can only be the root
                Node::Null => {
                    if !key.nibble_path().is_empty() {
                        return Err(JmtStorageError::InconsistentState);
                    }
                },
            }
            out_keys.push(key);
        }
        out_keys.reverse();
        Ok(out_keys)
    }
}

/// An iterator that iterates the index range (inclusive) of each different nibble at given
/// `nibble_idx` of all the keys in a sorted key-value pairs which have the identical Hash
/// prefix (up to nibble_idx).
struct NibbleRangeIterator<'a, P> {
    sorted_kvs: &'a [(&'a LeafKey, P)],
    nibble_idx: usize,
    pos: usize,
}

impl<'a, P> NibbleRangeIterator<'a, P> {
    fn new(sorted_kvs: &'a [(&'a LeafKey, P)], nibble_idx: usize) -> Self {
        NibbleRangeIterator {
            sorted_kvs,
            nibble_idx,
            pos: 0,
        }
    }
}

impl<P> Iterator for NibbleRangeIterator<'_, P> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let left = self.pos;
        if self.pos < self.sorted_kvs.len() {
            let cur_nibble = self.sorted_kvs.get(left)?.0.get_nibble(self.nibble_idx);
            let (mut i, mut j) = (left, self.sorted_kvs.len().saturating_sub(1));
            // Find the last index of the cur_nibble.
            while i < j {
                let mid = j.saturating_sub(j.saturating_sub(i) / 2);
                if self.sorted_kvs.get(mid)?.0.get_nibble(self.nibble_idx) > cur_nibble {
                    j = mid.saturating_sub(1);
                } else {
                    i = mid;
                }
            }
            self.pos = i.saturating_add(1);
            Some((left, i))
        } else {
            None
        }
    }
}

/// The nodes to write and the nodes made stale by one `batch_put_value_set` call.
///
/// The tree emits each stale node as a [`StaleNodeIndex`]. A store records each one as
/// [`StaleTreeNode::Node(node_key)`](crate::StaleTreeNode::Node), never as a subtree: only the listed node is stale,
/// its descendants may still be referenced by newer versions. Pruning a version range means recording each of its
/// `StaleNodeIndex` entries as `Node`. [`StaleTreeNode::Subtree`](crate::StaleTreeNode::Subtree) deletes everything
/// reachable through the children's version links, including nodes that newer roots still share, so it is only safe
/// when no retained root can reach any node in the subtree (e.g. dropping a whole tree).
///
/// A `StaleNodeIndex` entry may only be deleted once its `stale_since_version` is no newer than the oldest version the
/// store still serves; until then an older retained root still references the node. `record_stale_tree_node` does not
/// receive the version, so the store must track it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TreeUpdateBatch<P> {
    pub node_batch: Vec<(NodeKey, Node<P>)>,
    pub stale_node_index_batch: Vec<StaleNodeIndex>,
    pub num_new_leaves: usize,
    pub num_stale_leaves: usize,
}

impl<P: Clone> TreeUpdateBatch<P> {
    pub fn new() -> Self {
        Self {
            node_batch: vec![],
            stale_node_index_batch: vec![],
            num_new_leaves: 0,
            num_stale_leaves: 0,
        }
    }

    fn inc_num_new_leaves(&mut self) {
        self.num_new_leaves = self.num_new_leaves.saturating_add(1);
    }

    fn inc_num_stale_leaves(&mut self) {
        self.num_stale_leaves = self.num_stale_leaves.saturating_add(1);
    }

    pub fn put_node(&mut self, node_key: NodeKey, node: Node<P>) {
        if node.is_leaf() {
            self.inc_num_new_leaves();
        }
        self.node_batch.push((node_key, node))
    }

    pub fn put_stale_node(&mut self, node_key: NodeKey, stale_since_version: Version, node: &Node<P>) {
        if node.is_leaf() {
            self.inc_num_stale_leaves();
        }
        self.stale_node_index_batch.push(StaleNodeIndex {
            node_key,
            stale_since_version,
        });
    }
}

/// Indicates a node becomes stale since `stale_since_version`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StaleNodeIndex {
    /// The version since when the node is overwritten and becomes stale.
    pub stale_since_version: Version,
    /// The [`NodeKey`](node_type/struct.NodeKey.html) identifying the node associated with this
    /// record.
    pub node_key: NodeKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        NodeInProof,
        NodeType,
        SparseMerkleLeafNode,
        StaleTreeNode,
        TreeNode,
        TreeStoreWriter,
        jmt_node_hash,
        memory_store::MemoryTreeStore,
    };

    fn leaf_key(seed: u64) -> LeafKey {
        LeafKey::new(jmt_node_hash(&seed))
    }

    #[test]
    fn check_merkle_proof() {
        // Evaluating the functionality of the JMT Merkle proof.
        let mut mem = MemoryTreeStore::new();
        let jmt = JellyfishMerkleTree::new(&mem);

        let values = [
            (leaf_key(1), Some((jmt_node_hash(&10), Some(1u64)))),
            (leaf_key(2), Some((jmt_node_hash(&11), Some(2)))),
            (leaf_key(3), Some((jmt_node_hash(&12), Some(3)))),
        ];
        let (_, diff) = jmt.batch_put_value_set(values, None, 1).unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }

        for a in diff.stale_node_index_batch {
            mem.record_stale_tree_node(StaleTreeNode::Node(a.node_key)).unwrap();
        }
        mem.clear_stale_nodes();

        let jmt = JellyfishMerkleTree::new(&mem);

        // This causes get_with_proof to fail with node NotFound.
        let values = [
            (leaf_key(4), Some((jmt_node_hash(&13), Some(4u64)))),
            (leaf_key(5), Some((jmt_node_hash(&14), Some(5)))),
            (leaf_key(6), Some((jmt_node_hash(&15), Some(6)))),
        ];
        let (_mr, diff) = jmt.batch_put_value_set(values, Some(1), 2).unwrap();

        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        for a in diff.stale_node_index_batch {
            mem.record_stale_tree_node(StaleTreeNode::Node(a.node_key)).unwrap();
        }
        mem.clear_stale_nodes();
        let jmt = JellyfishMerkleTree::new(&mem);

        let k = leaf_key(3);
        let (_value, sparse) = jmt.get_with_proof(k.as_ref(), 2).unwrap();

        let leaf = sparse.leaf().unwrap();
        assert_eq!(*leaf.key(), k);
        assert_eq!(*leaf.value_hash(), jmt_node_hash(&12));
        // Unanswered: How do we verify the proof root matches a Merkle root?
        // assert!(sparse.siblings().iter().any(|h| *h == mr));
    }

    /// Commits two leaves at version 1 whose keys differ in the first nibble, so the root is an internal node with two
    /// leaf children.
    fn two_leaf_tree() -> (MemoryTreeStore<()>, LeafKey, LeafKey) {
        let (k1, k2) = (leaf_key(1), leaf_key(2));
        assert_ne!(k1.get_nibble(0), k2.get_nibble(0));
        let mut mem = MemoryTreeStore::new();
        let values = [
            (k1, Some((jmt_node_hash(&10), ()))),
            (k2, Some((jmt_node_hash(&11), ()))),
        ];
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set(values, None, 1)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        (mem, k1, k2)
    }

    fn leaf_node_key(key: &LeafKey) -> NodeKey {
        NodeKey::new_empty_path(1).gen_child_node_key(1, key.get_nibble(0).unwrap())
    }

    #[test]
    fn it_errors_on_range_proof_for_missing_key() {
        let (mem, _, _) = two_leaf_tree();
        let missing = leaf_key(3);
        let err = JellyfishMerkleTree::new(&mem)
            .get_range_proof(missing.as_ref(), 1)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::IndexNotFound), "{err:?}");
    }

    #[test]
    fn it_errors_on_null_node_below_root() {
        let (mut mem, _, k2) = two_leaf_tree();
        // Corrupt the store directly: `insert_node` refuses to overwrite.
        mem.nodes.insert(leaf_node_key(&k2), TreeNode::new_latest(Node::Null));
        let err = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(k2, Some((jmt_node_hash(&12), ())))], Some(1), 2)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
    }

    #[test]
    fn it_errors_on_proof_when_leaf_child_is_not_a_leaf_in_storage() {
        let (mut mem, k1, k2) = two_leaf_tree();
        let internal = InternalNode::try_new(
            [(
                Nibble::from_masked(0),
                Child::try_new(TreeHash::zero(), 1, NodeType::Internal { leaf_count: 1 }).unwrap(),
            )]
            .into_iter()
            .collect(),
        )
        .unwrap();
        for corrupt in [Node::Null, internal.into()] {
            // Corrupt the store directly: `insert_node` refuses to overwrite.
            mem.nodes.insert(leaf_node_key(&k2), TreeNode::new_latest(corrupt));
            let err = JellyfishMerkleTree::new(&mem)
                .get_with_proof(k1.as_ref(), 1)
                .unwrap_err();
            assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
        }
    }

    #[test]
    fn it_rejects_an_internal_node_presented_as_a_leaf() {
        // Two leaves whose keys differ in the first bit, so the root is the internal node of those two leaves
        let first_bit = |k: &LeafKey| k.iter_bits().next();
        let a = (leaf_key(1), jmt_node_hash(&10));
        let b = (2..)
            .map(|seed| (leaf_key(seed), jmt_node_hash(&11)))
            .find(|(k, _)| first_bit(k) != first_bit(&a.0))
            .unwrap();
        let mut mem = MemoryTreeStore::new();
        let values = [a, b].map(|(k, v)| (k, Some((v, ()))));
        let (root, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set(values, None, 1)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        let (_, proof) = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(a.0.as_ref(), 1)
            .unwrap();
        proof.verify_inclusion(&root, &a.0, &a.1).unwrap();

        // Present the root as a leaf with key = left child hash and value = right child hash
        let (left, right) = if first_bit(&a.0) == Some(false) { (a, b) } else { (b, a) };
        let left_hash = SparseMerkleLeafNode::new(left.0, left.1).hash();
        let right_hash = SparseMerkleLeafNode::new(right.0, right.1).hash();
        let fake_key = LeafKey::new(left_hash);
        let forged = SparseMerkleProofExt::new(Some(SparseMerkleLeafNode::new(fake_key, right_hash)), vec![]);

        // Forged inclusion of a key that is not in the tree
        forged.verify_inclusion(&root, &fake_key, &right_hash).unwrap_err();
        // Forged non-inclusion of keys that are in the tree
        forged.verify_exclusion(&root, &a.0).unwrap_err();
        forged.verify_exclusion(&root, &b.0).unwrap_err();
    }

    #[test]
    fn it_rejects_a_version_equal_to_the_persisted_version() {
        // The store is empty, so any read would fail with `NotFound` instead.
        let mem = MemoryTreeStore::<()>::new();
        let err = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(leaf_key(1), Some((jmt_node_hash(&10), ())))], Some(5), 5)
            .unwrap_err();
        assert!(
            matches!(err, JmtStorageError::NonMonotonicVersion {
                persisted_version: 5,
                version: 5
            }),
            "{err:?}"
        );
        assert!(mem.nodes.is_empty());
        assert!(mem.stale_nodes.is_empty());
    }

    #[test]
    fn it_rejects_a_version_below_the_persisted_version() {
        let mem = MemoryTreeStore::<()>::new();
        let err = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(leaf_key(1), Some((jmt_node_hash(&10), ())))], Some(5), 4)
            .unwrap_err();
        assert!(
            matches!(err, JmtStorageError::NonMonotonicVersion {
                persisted_version: 5,
                version: 4
            }),
            "{err:?}"
        );
        assert!(mem.nodes.is_empty());
    }

    #[test]
    fn it_accepts_the_next_version() {
        let mut mem = MemoryTreeStore::<()>::new();
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(leaf_key(1), Some((jmt_node_hash(&10), ())))], None, 5)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(leaf_key(2), Some((jmt_node_hash(&11), ())))], Some(5), 6)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        let (value, _) = JellyfishMerkleTree::new(&mem)
            .get_with_proof(leaf_key(2).as_ref(), 6)
            .unwrap();
        assert!(value.is_some());
    }

    #[test]
    fn it_errors_on_proof_for_null_node_below_root() {
        let (mut mem, _, k2) = two_leaf_tree();
        // Corrupt the store directly: `insert_node` refuses to overwrite.
        mem.nodes.insert(leaf_node_key(&k2), TreeNode::new_latest(Node::Null));
        let err = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(k2.as_ref(), 1)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
    }

    #[test]
    fn it_returns_an_empty_proof_for_a_null_root() {
        let mut mem = MemoryTreeStore::<()>::new();
        let (root, diff) = JellyfishMerkleTree::new(&mem).batch_put_value_set([], None, 1).unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        let key = leaf_key(1);
        let (value, proof) = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(key.as_ref(), 1)
            .unwrap();
        assert!(value.is_none());
        assert!(proof.leaf().is_none() && proof.siblings().is_empty());
        proof.verify_exclusion_or_empty_tree(&root, &key).unwrap();
    }

    #[test]
    fn it_errors_on_a_leaf_stored_under_the_wrong_path() {
        let (mut mem, k1, k2) = two_leaf_tree();
        let wrong_key = (3..)
            .map(leaf_key)
            .find(|k| k.get_nibble(0) != k2.get_nibble(0))
            .unwrap();
        // Corrupt the store directly: `insert_node` refuses to overwrite.
        mem.nodes.insert(
            leaf_node_key(&k2),
            TreeNode::new_latest(Node::new_leaf(wrong_key, jmt_node_hash(&11), (), 1)),
        );
        let err = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(k2.as_ref(), 1)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");

        // The mis-keyed leaf is also rejected when it is read as a sibling for another key's proof
        let err = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(k1.as_ref(), 1)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");

        // The update path refuses to move the mis-keyed leaf by its own key
        let err = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(k2, Some((jmt_node_hash(&12), ())))], Some(1), 2)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
    }

    /// A key whose first nibble differs from `key`'s.
    fn key_in_other_bucket(key: &LeafKey) -> LeafKey {
        (3..)
            .map(leaf_key)
            .find(|k| k.get_nibble(0) != key.get_nibble(0))
            .unwrap()
    }

    fn single_internal_child_node() -> Node<()> {
        let child = Child::try_new(TreeHash::zero(), 1, NodeType::Internal { leaf_count: 1 }).unwrap();
        InternalNode::try_new([(Nibble::from_masked(0), child)].into_iter().collect())
            .unwrap()
            .into()
    }

    #[test]
    fn it_errors_when_collapsing_onto_a_corrupt_leaf() {
        let (_, k1, _) = two_leaf_tree();
        let corrupt_nodes = [
            Node::Null,
            single_internal_child_node(),
            Node::new_leaf(key_in_other_bucket(&k1), jmt_node_hash(&10), (), 1),
        ];
        for corrupt in corrupt_nodes {
            let (mut mem, k1, k2) = two_leaf_tree();
            // Corrupt the store directly: `insert_node` refuses to overwrite.
            mem.nodes
                .insert(leaf_node_key(&k1), TreeNode::new_latest(corrupt.clone()));
            // Deleting k2 leaves the root with only k1's leaf, which is lifted up to become the new root
            let err = JellyfishMerkleTree::new(&mem)
                .batch_put_value_set([(k2, None)], Some(1), 2)
                .unwrap_err();
            assert!(
                matches!(err, JmtStorageError::InconsistentState),
                "{corrupt:?}: {err:?}"
            );
        }
    }

    #[test]
    fn it_collapses_onto_the_remaining_leaf() {
        let (mut mem, k1, k2) = two_leaf_tree();
        let (root, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(k2, None)], Some(1), 2)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        assert_eq!(root, SparseMerkleLeafNode::new(k1, jmt_node_hash(&10)).hash());
        let (value, proof) = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(k1.as_ref(), 2)
            .unwrap();
        assert!(value.is_some());
        proof.verify_inclusion(&root, &k1, &jmt_node_hash(&10)).unwrap();
    }

    #[test]
    fn it_errors_when_a_leaf_child_is_stored_as_an_internal_node() {
        let (mut mem, _, k2) = two_leaf_tree();
        // Corrupt the store directly: `insert_node` refuses to overwrite.
        mem.nodes
            .insert(leaf_node_key(&k2), TreeNode::new_latest(single_internal_child_node()));
        let err = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(k2.as_ref(), 1)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");

        let err = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(k2, Some((jmt_node_hash(&12), ())))], Some(1), 2)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
    }

    #[test]
    fn it_errors_when_an_internal_child_is_stored_as_a_leaf() {
        let mut mem = MemoryTreeStore::<()>::new();
        let [a, b, c] = three_level_keys();
        let values = [a, b, c].map(|k| (k, Some((jmt_node_hash(&10), ()))));
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set(values, None, 1)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        // `a` and `b` share their first nibble, so the root records an internal child there
        let internal_key = leaf_node_key(&a);
        mem.nodes.insert(
            internal_key,
            TreeNode::new_latest(Node::new_leaf(a, jmt_node_hash(&10), (), 1)),
        );
        let err = JellyfishMerkleTree::new(&mem)
            .get_with_proof_ext(a.as_ref(), 1)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");

        let err = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set([(a, Some((jmt_node_hash(&12), ())))], Some(1), 2)
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
    }

    /// Answers every key with an internal node that has one internal child, i.e. an endless chain.
    struct EndlessStore;

    impl TreeStoreReader<()> for EndlessStore {
        fn get_node(&self, _key: &NodeKey) -> Result<Node<()>, JmtStorageError> {
            let child = Child::try_new(TreeHash::zero(), 1, NodeType::Internal { leaf_count: 1 })?;
            let node = InternalNode::try_new([(Nibble::from_masked(0), child)].into_iter().collect())?;
            Ok(Node::Internal(node))
        }
    }

    #[test]
    fn it_bounds_get_all_nodes_referenced() {
        let result = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| JellyfishMerkleTree::new(&EndlessStore).get_all_nodes_referenced(NodeKey::new_empty_path(1)))
            .unwrap()
            .join()
            .unwrap();
        assert!(matches!(result, Err(JmtStorageError::InconsistentState)), "{result:?}");
    }

    #[test]
    fn it_errors_on_corrupt_children_in_get_all_nodes_referenced() {
        let (mut mem, _, k2) = two_leaf_tree();
        // Corrupt the store directly: `insert_node` refuses to overwrite.
        mem.nodes
            .insert(leaf_node_key(&k2), TreeNode::new_latest(single_internal_child_node()));
        let err = JellyfishMerkleTree::new(&mem)
            .get_all_nodes_referenced(NodeKey::new_empty_path(1))
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");

        // An internal child stored as `Null`
        let mut mem = MemoryTreeStore::<()>::new();
        let [a, b, c] = three_level_keys();
        let values = [a, b, c].map(|k| (k, Some((jmt_node_hash(&10), ()))));
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set(values, None, 1)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        mem.nodes.insert(leaf_node_key(&a), TreeNode::new_latest(Node::Null));
        let err = JellyfishMerkleTree::new(&mem)
            .get_all_nodes_referenced(NodeKey::new_empty_path(1))
            .unwrap_err();
        assert!(matches!(err, JmtStorageError::InconsistentState), "{err:?}");
    }

    #[test]
    fn it_accepts_a_null_root_in_get_all_nodes_referenced() {
        let mut mem = MemoryTreeStore::<()>::new();
        let (_, diff) = JellyfishMerkleTree::new(&mem).batch_put_value_set([], None, 1).unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        let keys = JellyfishMerkleTree::new(&mem)
            .get_all_nodes_referenced(NodeKey::new_empty_path(1))
            .unwrap();
        assert_eq!(keys, [NodeKey::new_empty_path(1)]);
    }

    #[test]
    fn it_lists_all_nodes_referenced_in_post_order() {
        let (mem, k1, k2) = two_leaf_tree();
        let root = NodeKey::new_empty_path(1);
        let keys = JellyfishMerkleTree::new(&mem)
            .get_all_nodes_referenced(root.clone())
            .unwrap();
        let mut leaves = [leaf_node_key(&k1), leaf_node_key(&k2)];
        leaves.sort();
        assert_eq!(keys, [leaves[0].clone(), leaves[1].clone(), root]);
    }

    /// Keys whose first nibbles are `[a, b]`, `[a, c]` and `[d, ..]`, so the tree is root -> internal -> two leaves,
    /// plus a third leaf under the root.
    fn three_level_keys() -> [LeafKey; 3] {
        let a = leaf_key(1);
        let b = (2..)
            .map(leaf_key)
            .find(|k| k.get_nibble(0) == a.get_nibble(0) && k.get_nibble(1) != a.get_nibble(1))
            .unwrap();
        let c = (2..)
            .map(leaf_key)
            .find(|k| k.get_nibble(0) != a.get_nibble(0))
            .unwrap();
        [a, b, c]
    }

    #[test]
    fn it_clears_a_stale_subtree_from_the_memory_store() {
        let mut mem = MemoryTreeStore::<()>::new();
        let values = three_level_keys().map(|k| (k, Some((jmt_node_hash(&10), ()))));
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set(values, None, 1)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        assert_eq!(mem.nodes.len(), 5);

        mem.record_stale_tree_node(StaleTreeNode::Subtree(NodeKey::new_empty_path(1)))
            .unwrap();
        mem.clear_stale_nodes();
        assert!(mem.nodes.is_empty(), "{mem}");
    }

    #[test]
    fn it_clears_only_the_stale_node_from_the_memory_store() {
        let mut mem = MemoryTreeStore::<()>::new();
        let values = three_level_keys().map(|k| (k, Some((jmt_node_hash(&10), ()))));
        let (_, diff) = JellyfishMerkleTree::new(&mem)
            .batch_put_value_set(values, None, 1)
            .unwrap();
        for (k, v) in diff.node_batch {
            mem.insert_node(k, v).unwrap();
        }
        mem.record_stale_tree_node(StaleTreeNode::Node(NodeKey::new_empty_path(1)))
            .unwrap();
        mem.clear_stale_nodes();
        assert_eq!(mem.nodes.len(), 4);
    }

    /// Documents that proof bytes are not canonical: a `Leaf` sibling and its `Other(hash)` form verify identically.
    #[test]
    fn proof_encoding_is_malleable() {
        let (mem, k1, _) = two_leaf_tree();
        let jmt = JellyfishMerkleTree::new(&mem);
        let root = jmt.get_root_hash(1).unwrap();
        let (_, proof) = jmt.get_with_proof_ext(k1.as_ref(), 1).unwrap();
        proof.verify_inclusion(&root, &k1, &jmt_node_hash(&10)).unwrap();

        let mut siblings = proof.siblings().to_vec();
        let leaf_sibling = siblings.iter_mut().find(|s| matches!(s, NodeInProof::Leaf(_))).unwrap();
        *leaf_sibling = NodeInProof::Other(leaf_sibling.hash());
        let other = SparseMerkleProofExt::new(proof.leaf(), siblings);
        other.verify_inclusion(&root, &k1, &jmt_node_hash(&10)).unwrap();

        assert_ne!(borsh::to_vec(&proof).unwrap(), borsh::to_vec(&other).unwrap());
    }
}
