"""
Node Selection and Management

This module provides advanced node selection strategies and persistent state management
for base node rotation, mirroring proven patterns for optimal wallet connectivity.
"""

import json
import os
import time
from dataclasses import dataclass, asdict
from typing import List, Optional, Dict, Any, Set
from datetime import datetime, timedelta

from .base_nodes import BaseNode, BaseNodeManager, BaseNodeSelectionStrategy


@dataclass
class NodeSelectionState:
    """Persistent state for node selection across wallet sessions"""
    current_index: int = 0
    failed_nodes: Set[str] = None  # Set of failed node public keys
    last_rotation: Optional[float] = None
    session_start: Optional[float] = None
    total_selections: int = 0
    
    def __post_init__(self):
        if self.failed_nodes is None:
            self.failed_nodes = set()
        if self.session_start is None:
            self.session_start = time.time()


class PersistentNodeSelector:
    """
    Node selector with persistent round-robin state and health-based exclusion
    
    Maintains selection state across wallet sessions and automatically excludes
    nodes that consistently fail to connect.
    """
    
    def __init__(self, state_file_path: Optional[str] = None):
        self.state_file_path = state_file_path
        self.state = NodeSelectionState()
        self.node_manager = BaseNodeManager(BaseNodeSelectionStrategy.ROUND_ROBIN)
        self.failed_node_timeout = 1800  # 30 minutes timeout for failed nodes
        
        # Load persistent state if available
        if self.state_file_path and os.path.exists(self.state_file_path):
            self._load_state()
    
    def _load_state(self):
        """Load selection state from file"""
        try:
            with open(self.state_file_path, 'r') as f:
                data = json.load(f)
                self.state.current_index = data.get('current_index', 0)
                self.state.failed_nodes = set(data.get('failed_nodes', []))
                self.state.last_rotation = data.get('last_rotation')
                self.state.total_selections = data.get('total_selections', 0)
                
                # Don't restore session_start - always use current session
                self.state.session_start = time.time()
                
        except (FileNotFoundError, json.JSONDecodeError, KeyError) as e:
            # If state file is corrupted or missing, start fresh
            self.state = NodeSelectionState()
    
    def _save_state(self):
        """Save selection state to file"""
        if not self.state_file_path:
            return
            
        try:
            # Ensure directory exists
            os.makedirs(os.path.dirname(self.state_file_path), exist_ok=True)
            
            data = {
                'current_index': self.state.current_index,
                'failed_nodes': list(self.state.failed_nodes),
                'last_rotation': self.state.last_rotation,
                'total_selections': self.state.total_selections,
                'last_updated': time.time()
            }
            
            with open(self.state_file_path, 'w') as f:
                json.dump(data, f, indent=2)
                
        except Exception as e:
            # Log error but don't fail - state persistence is optional
            print(f"Warning: Could not save node selection state: {e}")
    
    def add_nodes(self, nodes: List[BaseNode]):
        """Add nodes to the managed list"""
        for node in nodes:
            self.node_manager.add_node(node)
    
    def get_available_nodes(self) -> List[BaseNode]:
        """Get nodes that are available and not in failed set"""
        available = self.node_manager.get_available_nodes()
        
        # Filter out nodes that are in failed set (unless timeout expired)
        current_time = time.time()
        filtered_nodes = []
        
        for node in available:
            if node.public_key in self.state.failed_nodes:
                # Check if enough time has passed to retry failed node
                if (node.last_connection_attempt and 
                    current_time - node.last_connection_attempt > self.failed_node_timeout):
                    # Remove from failed set and allow retry
                    self.state.failed_nodes.discard(node.public_key)
                    # Reset node failure count
                    node.consecutive_failures = 0
                    node.is_available = True
                    filtered_nodes.append(node)
                # Otherwise skip this failed node
            else:
                filtered_nodes.append(node)
        
        return filtered_nodes
    
    def select_next_node(self) -> Optional[BaseNode]:
        """
        Select next node using persistent round-robin with health exclusion
        
        Returns:
            Selected BaseNode or None if no nodes available
        """
        available_nodes = self.get_available_nodes()
        if not available_nodes:
            # If no nodes available, reset failed nodes and try again
            self._reset_failed_nodes_if_needed()
            available_nodes = self.get_available_nodes()
            
            if not available_nodes:
                return None
        
        # Ensure current index is within bounds
        if self.state.current_index >= len(available_nodes):
            self.state.current_index = 0
        
        # Select node at current index
        selected_node = available_nodes[self.state.current_index]
        
        # Update state
        self.state.current_index = (self.state.current_index + 1) % len(available_nodes)
        self.state.last_rotation = time.time()
        self.state.total_selections += 1
        
        # Update node manager's current node
        self.node_manager.current_node = selected_node
        
        # Save state
        self._save_state()
        
        return selected_node
    
    def mark_current_node_success(self):
        """Mark current node connection as successful"""
        if self.node_manager.current_node:
            node = self.node_manager.current_node
            node.mark_connection_success()
            
            # Remove from failed set if it was there
            self.state.failed_nodes.discard(node.public_key)
            self._save_state()
    
    def mark_current_node_failure(self):
        """Mark current node connection as failed"""
        if self.node_manager.current_node:
            node = self.node_manager.current_node
            node.mark_connection_failure()
            
            # Add to failed set if it has too many consecutive failures
            if node.consecutive_failures >= 3:
                self.state.failed_nodes.add(node.public_key)
                self._save_state()
    
    def switch_to_next_node(self) -> Optional[BaseNode]:
        """Switch to next available node (for failover scenarios)"""
        if self.node_manager.current_node:
            self.mark_current_node_failure()
        
        return self.select_next_node()
    
    def _reset_failed_nodes_if_needed(self):
        """Reset failed nodes if all nodes are failed or timeout expired"""
        all_nodes = self.node_manager.nodes
        
        # If all nodes are in failed set, reset the failed set
        if len(self.state.failed_nodes) >= len(all_nodes):
            self.state.failed_nodes.clear()
            
            # Reset all node health
            for node in all_nodes:
                node.consecutive_failures = 0
                node.is_available = True
            
            self._save_state()
    
    def get_selection_statistics(self) -> Dict[str, Any]:
        """Get statistics about node selection"""
        available_nodes = self.get_available_nodes()
        all_nodes = self.node_manager.nodes
        
        session_time = time.time() - self.state.session_start if self.state.session_start else 0
        
        return {
            "total_nodes": len(all_nodes),
            "available_nodes": len(available_nodes),
            "failed_nodes": len(self.state.failed_nodes),
            "current_index": self.state.current_index,
            "total_selections": self.state.total_selections,
            "session_time_seconds": session_time,
            "last_rotation": self.state.last_rotation,
            "current_node": {
                "name": self.node_manager.current_node.name if self.node_manager.current_node else None,
                "public_key": self.node_manager.current_node.public_key if self.node_manager.current_node else None,
                "health_score": self.node_manager.current_node.get_health_score() if self.node_manager.current_node else None
            },
            "failed_node_public_keys": list(self.state.failed_nodes)
        }
    
    def reset_selection_state(self):
        """Reset all selection state (useful for testing or recovery)"""
        self.state = NodeSelectionState()
        self.node_manager.reset_node_health()
        self._save_state()


def create_node_selector_for_wallet(
    wallet_datastore_path: str,
    wallet_database_name: str
) -> PersistentNodeSelector:
    """
    Create a node selector with state persistence in wallet directory
    
    Args:
        wallet_datastore_path: Path to wallet data directory
        wallet_database_name: Name of wallet database
        
    Returns:
        Configured PersistentNodeSelector instance
    """
    state_file = os.path.join(
        wallet_datastore_path,
        f"{wallet_database_name}_node_selection.json"
    )
    
    return PersistentNodeSelector(state_file)
