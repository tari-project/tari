//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{collections::HashMap, fmt, fmt::Debug};

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

    pub fn clear_stale_nodes(&mut self) {
        for stale in self.stale_nodes.drain(..) {
            self.nodes.remove(stale.as_node_key());
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
        let node = TreeNode::new_latest(node);
        self.nodes.insert(key, node);
        Ok(())
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
            writeln!(f, "    {}: {:?}", key, node)?;
        }
        writeln!(f, "  Stale Nodes:")?;
        for stale in &self.stale_nodes {
            writeln!(f, "    {}", stale.as_node_key())?;
        }
        Ok(())
    }
}
