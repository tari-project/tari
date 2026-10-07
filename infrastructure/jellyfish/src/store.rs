//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use serde::{Deserialize, Serialize};

use crate::{JmtStorageError, Node, NodeKey};

/// Implementers are able to read nodes from a tree store.
pub trait TreeStoreReader<P> {
    /// Gets node by key, if it exists.
    fn get_node(&self, key: &NodeKey) -> Result<Node<P>, JmtStorageError>;
}

/// Implementers are able to insert nodes to a tree store.
pub trait TreeStoreWriter<P> {
    /// Inserts the node under a new, unique key (i.e. never an update).
    ///
    /// Implementations must return [`JmtStorageError::Conflict`] if a node already exists under `key`. The tree's
    /// stale-node bookkeeping relies on nodes never being overwritten: an overwritten key may also be in the stale
    /// index, and pruning it would delete the live node.
    fn insert_node(&mut self, key: NodeKey, node: Node<P>) -> Result<(), JmtStorageError>;

    /// Marks the given tree part for a (potential) future removal by an arbitrary external pruning
    /// process.
    ///
    /// The tree emits stale nodes as [`StaleNodeIndex`](crate::StaleNodeIndex)`{ node_key, stale_since_version }` in
    /// [`TreeUpdateBatch::stale_node_index_batch`](crate::TreeUpdateBatch::stale_node_index_batch); record each one as
    /// [`StaleTreeNode::Node(node_key)`](StaleTreeNode::Node); that is also how to prune a version range.
    /// [`StaleTreeNode::Subtree`] marks a node and everything reachable through its children's version links,
    /// including nodes that newer roots still share, so it is only safe when no retained root can reach any node in
    /// the subtree (e.g. dropping a whole tree).
    ///
    /// A `StaleNodeIndex` entry may only be deleted once its `stale_since_version` is no newer than the oldest version
    /// the store still serves; until then an older retained root still references the node. This method does not
    /// receive the version, so the store must track it.
    fn record_stale_tree_node(&mut self, part: StaleTreeNode) -> Result<(), JmtStorageError>;
}

/// Implementers are able to read and write nodes to a tree store.
pub trait TreeStore<P>: TreeStoreReader<P> + TreeStoreWriter<P> {}
impl<P, S: TreeStoreReader<P> + TreeStoreWriter<P>> TreeStore<P> for S {}

impl<P, T: TreeStoreReader<P>> TreeStoreReader<P> for &T {
    fn get_node(&self, key: &NodeKey) -> Result<Node<P>, JmtStorageError> {
        (*self).get_node(key)
    }
}

impl<P, T: TreeStoreWriter<P>> TreeStoreWriter<P> for &mut T {
    fn insert_node(&mut self, key: NodeKey, node: Node<P>) -> Result<(), JmtStorageError> {
        (*self).insert_node(key, node)
    }

    fn record_stale_tree_node(&mut self, part: StaleTreeNode) -> Result<(), JmtStorageError> {
        (*self).record_stale_tree_node(part)
    }
}

/// A part of a tree that may become stale (i.e. need eventual pruning).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum StaleTreeNode {
    /// A single node to be removed.
    Node(NodeKey),
    /// An entire subtree of descendants of a specific node (including itself). Only safe when no retained root can
    /// reach any node in the subtree; see [`TreeStoreWriter::record_stale_tree_node`].
    Subtree(NodeKey),
}

impl StaleTreeNode {
    pub fn into_node_key(self) -> NodeKey {
        match self {
            Self::Node(key) | Self::Subtree(key) => key,
        }
    }

    pub fn as_node_key(&self) -> &NodeKey {
        match self {
            Self::Node(key) | Self::Subtree(key) => key,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreeNode<P> {
    V1(Node<P>),
}

impl<P> TreeNode<P> {
    pub fn new_latest(node: Node<P>) -> Self {
        Self::new_v1(node)
    }

    pub fn new_v1(node: Node<P>) -> Self {
        Self::V1(node)
    }

    pub fn as_node(&self) -> &Node<P> {
        match self {
            Self::V1(node) => node,
        }
    }

    pub fn into_node(self) -> Node<P> {
        match self {
            Self::V1(node) => node,
        }
    }
}
