//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::{HashMap, hash_map::Entry},
    fmt,
    fmt::Debug,
};

use crate::{JmtStorageError, Node, NodeKey, StaleTreeNode, TreeNode, TreeStoreReader, TreeStoreWriter};

#[derive(Debug, Default)]
pub struct MemoryTreeStore<P> {
    pub nodes: HashMap<NodeKey, TreeNode<P>>,
    pub stale_nodes: Vec<StaleTreeNode>,
}

impl<P> MemoryTreeStore<P> {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            stale_nodes: Vec::new(),
        }
    }

    /// Removes every recorded stale node. A [`StaleTreeNode::Subtree`] removes the node and all of its descendants.
    pub fn clear_stale_nodes(&mut self) {
        for stale in self.stale_nodes.drain(..) {
            match stale {
                StaleTreeNode::Node(key) => {
                    self.nodes.remove(&key);
                },
                StaleTreeNode::Subtree(key) => {
                    // Iterative so that a deep (or corrupt) tree cannot overflow the stack. Each removed node yields
                    // its children, and a removed key is never visited again, so this terminates.
                    let mut stack = vec![key];
                    while let Some(key) = stack.pop() {
                        if let Some(Node::Internal(internal_node)) = self.nodes.remove(&key).map(TreeNode::into_node) {
                            for (nibble, child) in internal_node.into_children() {
                                stack.push(key.gen_child_node_key(child.version, nibble));
                            }
                        }
                    }
                },
            }
        }
    }
}

impl<P: Clone> TreeStoreReader<P> for MemoryTreeStore<P> {
    fn get_node(&self, key: &NodeKey) -> Result<Node<P>, JmtStorageError> {
        self.nodes
            .get(key)
            .map(|node| node.clone().into_node())
            .ok_or_else(|| JmtStorageError::NotFound(key.clone()))
    }
}

impl<P> TreeStoreWriter<P> for MemoryTreeStore<P> {
    fn insert_node(&mut self, key: NodeKey, node: Node<P>) -> Result<(), JmtStorageError> {
        match self.nodes.entry(key) {
            Entry::Occupied(entry) => Err(JmtStorageError::Conflict(entry.key().clone())),
            Entry::Vacant(entry) => {
                entry.insert(TreeNode::new_latest(node));
                Ok(())
            },
        }
    }

    fn record_stale_tree_node(&mut self, stale: StaleTreeNode) -> Result<(), JmtStorageError> {
        self.stale_nodes.push(stale);
        Ok(())
    }
}

impl<P: Debug> fmt::Display for MemoryTreeStore<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "MemoryTreeStore")?;
        writeln!(f, "  Nodes:")?;
        let mut store = self.nodes.iter().collect::<Vec<_>>();
        store.sort_by_key(|(key, _)| *key);
        for (key, node) in store {
            writeln!(f, "    {key}: {node:?}")?;
        }
        writeln!(f, "  Stale Nodes:")?;
        for stale in &self.stale_nodes {
            writeln!(f, "    {}", stale.as_node_key())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LeafKey, LeafNode, jmt_node_hash};

    #[test]
    fn insert_node_refuses_to_overwrite() {
        let mut mem = MemoryTreeStore::<u64>::new();
        let key = NodeKey::new_empty_path(1);
        let first = Node::Leaf(LeafNode::new(LeafKey::new(jmt_node_hash(&1)), jmt_node_hash(&10), 1, 1));
        mem.insert_node(key.clone(), first.clone()).unwrap();

        let err = mem.insert_node(key.clone(), Node::Null).unwrap_err();
        assert!(matches!(&err, JmtStorageError::Conflict(k) if *k == key), "{err:?}");
        assert_eq!(mem.get_node(&key).unwrap(), first);
    }
}
