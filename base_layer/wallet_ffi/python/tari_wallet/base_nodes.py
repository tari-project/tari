"""
Base Node Management for Tari Wallet

This module provides base node selection, rotation, and health tracking functionality
that mirrors the pattern used in Android's BaseNodesManager.
"""

import random
import time
from dataclasses import dataclass
from typing import List, Optional, Dict, Any
from enum import Enum

class BaseNodeSelectionStrategy(Enum):
    """Strategy for selecting base nodes"""
    ROUND_ROBIN = "round_robin"
    RANDOM = "random" 
    PRIORITY = "priority"

@dataclass
class BaseNode:
    """Represents a base node that can be connected to"""
    name: str
    public_key: str
    address: str
    is_custom: bool = False
    priority: int = 0
    last_connection_attempt: Optional[float] = None
    last_successful_connection: Optional[float] = None
    consecutive_failures: int = 0
    is_available: bool = True
    is_placeholder_key: bool = False

    def __post_init__(self):
        """Validate the base node configuration"""
        if not self.public_key:
            raise ValueError("Public key cannot be empty")
        if not self.address:
            raise ValueError("Address cannot be empty")
        if not self.name:
            self.name = f"Node-{self.public_key[:8]}"

    def mark_connection_attempt(self):
        """Mark that a connection attempt was made"""
        self.last_connection_attempt = time.time()

    def mark_connection_success(self):
        """Mark a successful connection"""
        self.last_successful_connection = time.time()
        self.consecutive_failures = 0
        self.is_available = True

    def mark_connection_failure(self):
        """Mark a failed connection"""
        self.consecutive_failures += 1
        # Mark as unavailable after 3 consecutive failures
        if self.consecutive_failures >= 3:
            self.is_available = False

    def get_health_score(self) -> float:
        """
        Calculate a health score for this node (0.0 to 1.0)
        Higher score indicates better health
        """
        base_score = 1.0
        
        # Penalty for consecutive failures
        failure_penalty = min(0.8, self.consecutive_failures * 0.2)
        base_score -= failure_penalty
        
        # Bonus for recent successful connections
        if self.last_successful_connection:
            time_since_success = time.time() - self.last_successful_connection
            if time_since_success < 300:  # 5 minutes
                base_score += 0.1
            elif time_since_success > 3600:  # 1 hour
                base_score -= 0.1
        
        # Custom nodes get a small priority boost
        if self.is_custom:
            base_score += 0.05
            
        return max(0.0, min(1.0, base_score))

class BaseNodeManager:
    """
    Manages base node selection, rotation, and health tracking.
    
    This class provides functionality similar to Android's BaseNodesManager,
    allowing automatic selection of healthy base nodes with failover support.
    """
    
    def __init__(self, strategy: BaseNodeSelectionStrategy = BaseNodeSelectionStrategy.ROUND_ROBIN):
        self.strategy = strategy
        self.nodes: List[BaseNode] = []
        self.current_index = 0
        self.current_node: Optional[BaseNode] = None
        
    def add_node(self, node: BaseNode):
        """Add a base node to the managed list"""
        # Check for duplicates by public key
        existing = self.get_node_by_public_key(node.public_key)
        if existing:
            # Update existing node
            existing.address = node.address
            existing.name = node.name
            existing.is_custom = node.is_custom
            existing.priority = node.priority
        else:
            self.nodes.append(node)
            
    def add_nodes_from_seed_peers(self, seed_peer_keys: List[str], address_template: str = "/ip4/127.0.0.1/tcp/18189"):
        """
        Add nodes from seed peer public keys
        
        Args:
            seed_peer_keys: List of public key hex strings
            address_template: Address template (will be updated when peer discovery is implemented)
        """
        for i, public_key in enumerate(seed_peer_keys):
            node = BaseNode(
                name=f"Seed-{i+1}",
                public_key=public_key,
                address=address_template,  # TODO: This should come from actual peer discovery
                is_custom=False,
                priority=i
            )
            self.add_node(node)
    
    def get_node_by_public_key(self, public_key: str) -> Optional[BaseNode]:
        """Find a node by its public key"""
        for node in self.nodes:
            if node.public_key == public_key:
                return node
        return None
        
    def get_available_nodes(self) -> List[BaseNode]:
        """Get all available (healthy) nodes"""
        return [node for node in self.nodes if node.is_available]
        
    def get_current_node(self) -> Optional[BaseNode]:
        """Get the currently selected node"""
        return self.current_node
        
    def select_next_node(self) -> Optional[BaseNode]:
        """
        Select the next node based on the configured strategy
        
        Returns:
            The selected BaseNode or None if no nodes are available
        """
        available_nodes = self.get_available_nodes()
        if not available_nodes:
            return None
            
        if self.strategy == BaseNodeSelectionStrategy.ROUND_ROBIN:
            node = self._select_round_robin(available_nodes)
        elif self.strategy == BaseNodeSelectionStrategy.RANDOM:
            node = self._select_random(available_nodes)
        elif self.strategy == BaseNodeSelectionStrategy.PRIORITY:
            node = self._select_by_priority(available_nodes)
        else:
            node = available_nodes[0]
            
        self.current_node = node
        return node
        
    def switch_to_next_node(self) -> Optional[BaseNode]:
        """
        Switch to the next available node (failover)
        Marks current node connection attempt and selects next
        """
        if self.current_node:
            self.current_node.mark_connection_attempt()
            
        return self.select_next_node()
    
    def mark_current_node_success(self):
        """Mark the current node connection as successful"""
        if self.current_node:
            self.current_node.mark_connection_success()
            
    def mark_current_node_failure(self):
        """Mark the current node connection as failed"""
        if self.current_node:
            self.current_node.mark_connection_failure()
    
    def _select_round_robin(self, available_nodes: List[BaseNode]) -> BaseNode:
        """Select node using round-robin strategy"""
        if self.current_index >= len(available_nodes):
            self.current_index = 0
        node = available_nodes[self.current_index]
        self.current_index = (self.current_index + 1) % len(available_nodes)
        return node
        
    def _select_random(self, available_nodes: List[BaseNode]) -> BaseNode:
        """Select node using random strategy"""
        return random.choice(available_nodes)
        
    def _select_by_priority(self, available_nodes: List[BaseNode]) -> BaseNode:
        """Select node with highest priority (lowest priority number)"""
        return min(available_nodes, key=lambda n: n.priority)
        
    def get_node_statistics(self) -> Dict[str, Any]:
        """Get statistics about managed nodes"""
        total_nodes = len(self.nodes)
        available_nodes = len(self.get_available_nodes())
        
        return {
            "total_nodes": total_nodes,
            "available_nodes": available_nodes,
            "unavailable_nodes": total_nodes - available_nodes,
            "current_node": {
                "name": self.current_node.name if self.current_node else None,
                "public_key": self.current_node.public_key if self.current_node else None,
                "health_score": self.current_node.get_health_score() if self.current_node else None
            },
            "strategy": self.strategy.value
        }
        
    def reset_node_health(self):
        """Reset health status for all nodes (useful for recovery)"""
        for node in self.nodes:
            node.consecutive_failures = 0
            node.is_available = True
