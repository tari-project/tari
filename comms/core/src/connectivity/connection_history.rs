use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use crate::peer_manager::NodeId;

/// Tracks connection history to nodes to enforce cooldown periods
pub struct ConnectionHistory {
    /// Maps node IDs to the last time we disconnected from them
    last_disconnected: HashMap<NodeId, Instant>,
}

impl ConnectionHistory {
    pub fn new() -> Self {
        Self {
            last_disconnected: HashMap::new(),
        }
    }
    
    /// Record that we disconnected from a node
    pub fn record_disconnection(&mut self, node_id: &NodeId) {
        self.last_disconnected.insert(node_id.clone(), Instant::now());
    }
    
    /// Check if a node is in cooldown period
    pub fn is_in_cooldown(&self, node_id: &NodeId, cooldown: Duration) -> bool {
        if let Some(last_time) = self.last_disconnected.get(node_id) {
            last_time.elapsed() < cooldown
        } else {
            false
        }
    }

    /// Get the time elapsed since disconnection for a node
    pub fn time_since_disconnection(&self, node_id: &NodeId) -> Option<Duration> {
        self.last_disconnected.get(node_id).map(|time| time.elapsed())
    }

    /// Clean up old history entries
    pub fn cleanup(&mut self, max_age: Duration) {
        self.last_disconnected.retain(|_, time| time.elapsed() < max_age);
    }
    
    /// Get nodes that are not in cooldown
    pub fn get_available_nodes<'a>(&self, nodes: impl Iterator<Item = &'a NodeId>, cooldown: Duration) -> Vec<NodeId> {
        nodes
            .filter(|node_id| !self.is_in_cooldown(node_id, cooldown))
            .cloned()
            .collect()
    }
}