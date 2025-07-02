"""
Automatic Base Node Discovery Service

This module implements an async discovery service that combines DNS seed resolution
with FFI seed peer retrieval, providing automatic failover and health monitoring.
"""

import asyncio
import time
from typing import List, Optional, Callable, Dict, Any
from dataclasses import dataclass, field
from .base_nodes import BaseNode, BaseNodeManager, BaseNodeSelectionStrategy
from .network import NetworkManager, TariNetwork

@dataclass
class DiscoveryConfig:
    """Configuration for the discovery service"""
    discovery_interval: float = 300.0  # 5 minutes between discoveries
    health_check_interval: float = 60.0  # 1 minute between health checks
    dns_timeout: float = 5.0
    max_discovery_failures: int = 5
    enable_dns_discovery: bool = True
    enable_ffi_discovery: bool = True
    node_selection_strategy: BaseNodeSelectionStrategy = BaseNodeSelectionStrategy.ROUND_ROBIN
    
class DiscoveryService:
    """
    Automatic base node discovery service
    
    Combines DNS seed resolution with FFI seed peer retrieval to maintain a
    healthy list of available base nodes. Provides automatic failover and
    background health monitoring.
    """
    
    def __init__(self, 
                 network: TariNetwork = TariNetwork.NEXTNET,
                 config: Optional[DiscoveryConfig] = None,
                 wallet_get_seed_peers_fn: Optional[Callable[[], List[str]]] = None):
        """
        Initialize the discovery service
        
        Args:
            network: The Tari network to discover nodes for
            config: Discovery configuration 
            wallet_get_seed_peers_fn: Function to get seed peers from wallet FFI
        """
        self.network = network
        self.config = config or DiscoveryConfig()
        self.network_manager = NetworkManager(network)
        self.base_node_manager = BaseNodeManager(self.config.node_selection_strategy)
        self.wallet_get_seed_peers_fn = wallet_get_seed_peers_fn
        
        # State tracking
        self.is_running = False
        self.discovery_task: Optional[asyncio.Task] = None
        self.health_check_task: Optional[asyncio.Task] = None
        self.consecutive_discovery_failures = 0
        self.last_discovery_time: Optional[float] = None
        self.last_health_check_time: Optional[float] = None
        
        # Callbacks
        self.on_nodes_discovered: Optional[Callable[[List[BaseNode]], None]] = None
        self.on_node_health_changed: Optional[Callable[[BaseNode, bool], None]] = None
        self.on_discovery_error: Optional[Callable[[Exception], None]] = None
        
    async def start(self):
        """Start the discovery service with background tasks"""
        if self.is_running:
            return
            
        self.is_running = True
        
        # Perform initial discovery
        await self.discover_nodes()
        
        # Start background tasks
        self.discovery_task = asyncio.create_task(self._discovery_loop())
        self.health_check_task = asyncio.create_task(self._health_check_loop())
        
    async def stop(self):
        """Stop the discovery service and cancel background tasks"""
        self.is_running = False
        
        if self.discovery_task:
            self.discovery_task.cancel()
            try:
                await self.discovery_task
            except asyncio.CancelledError:
                pass
                
        if self.health_check_task:
            self.health_check_task.cancel()
            try:
                await self.health_check_task
            except asyncio.CancelledError:
                pass
                
    async def discover_nodes(self) -> List[BaseNode]:
        """
        Perform node discovery from all available sources
        
        Returns:
            List of discovered BaseNode objects
        """
        discovered_nodes = []
        
        try:
            # 1. Get hardcoded nodes from network configuration
            hardcoded_nodes = self.network_manager.get_hardcoded_base_nodes()
            discovered_nodes.extend(hardcoded_nodes)
            
            # 2. Get nodes from DNS seeds if enabled
            if self.config.enable_dns_discovery:
                dns_nodes = self.network_manager.create_base_nodes_from_dns(
                    timeout=self.config.dns_timeout
                )
                discovered_nodes.extend(dns_nodes)
                
            # 3. Get seed peers from wallet FFI if available
            if self.config.enable_ffi_discovery and self.wallet_get_seed_peers_fn:
                try:
                    seed_peer_keys = self.wallet_get_seed_peers_fn()
                    # Create base nodes from seed peer keys
                    # Note: We use a placeholder address template here
                    # In a real implementation, this would come from peer discovery
                    for i, public_key in enumerate(seed_peer_keys):
                        node = BaseNode(
                            name=f"FFI-Seed-{i+1}",
                            public_key=public_key,
                            address=f"/ip4/127.0.0.1/tcp/{self.network_manager.config.default_port}",  # Placeholder
                            is_custom=False,
                            priority=50 + i  # Medium priority between hardcoded and DNS
                        )
                        discovered_nodes.append(node)
                        
                except Exception as e:
                    if self.on_discovery_error:
                        self.on_discovery_error(e)
                        
            # Add discovered nodes to manager
            for node in discovered_nodes:
                self.base_node_manager.add_node(node)
                
            # Reset failure counter on successful discovery
            self.consecutive_discovery_failures = 0
            self.last_discovery_time = time.time()
            
            # Notify callback
            if self.on_nodes_discovered:
                self.on_nodes_discovered(discovered_nodes)
                
        except Exception as e:
            self.consecutive_discovery_failures += 1
            if self.on_discovery_error:
                self.on_discovery_error(e)
            raise
            
        return discovered_nodes
        
    def get_best_node(self) -> Optional[BaseNode]:
        """Get the best available node for connection"""
        return self.base_node_manager.select_next_node()
        
    def switch_to_next_node(self) -> Optional[BaseNode]:
        """Switch to the next available node (failover)"""
        return self.base_node_manager.switch_to_next_node()
        
    def mark_current_node_success(self):
        """Mark the current node connection as successful"""
        self.base_node_manager.mark_current_node_success()
        current = self.base_node_manager.get_current_node()
        if current and self.on_node_health_changed:
            self.on_node_health_changed(current, True)
            
    def mark_current_node_failure(self):
        """Mark the current node connection as failed"""
        self.base_node_manager.mark_current_node_failure()
        current = self.base_node_manager.get_current_node()
        if current and self.on_node_health_changed:
            self.on_node_health_changed(current, False)
            
    def get_discovery_status(self) -> Dict[str, Any]:
        """Get comprehensive status of the discovery service"""
        stats = self.base_node_manager.get_node_statistics()
        
        return {
            "is_running": self.is_running,
            "network": self.network.value,
            "last_discovery_time": self.last_discovery_time,
            "last_health_check_time": self.last_health_check_time,
            "consecutive_discovery_failures": self.consecutive_discovery_failures,
            "discovery_interval": self.config.discovery_interval,
            "health_check_interval": self.config.health_check_interval,
            "node_statistics": stats,
            "config": {
                "enable_dns_discovery": self.config.enable_dns_discovery,
                "enable_ffi_discovery": self.config.enable_ffi_discovery,
                "dns_timeout": self.config.dns_timeout,
                "max_discovery_failures": self.config.max_discovery_failures,
                "selection_strategy": self.config.node_selection_strategy.value
            }
        }
        
    async def force_rediscovery(self):
        """Force immediate rediscovery of nodes"""
        await self.discover_nodes()
        
    def reset_all_node_health(self):
        """Reset health status for all nodes"""
        self.base_node_manager.reset_node_health()
        
    async def _discovery_loop(self):
        """Background loop for periodic node discovery"""
        while self.is_running:
            try:
                await asyncio.sleep(self.config.discovery_interval)
                
                if not self.is_running:
                    break
                    
                # Check if we should stop discovery due to too many failures
                if self.consecutive_discovery_failures >= self.config.max_discovery_failures:
                    if self.on_discovery_error:
                        self.on_discovery_error(
                            Exception(f"Discovery failed {self.consecutive_discovery_failures} times, stopping")
                        )
                    break
                    
                await self.discover_nodes()
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                if self.on_discovery_error:
                    self.on_discovery_error(e)
                # Continue the loop even if discovery fails
                
    async def _health_check_loop(self):
        """Background loop for node health monitoring"""
        while self.is_running:
            try:
                await asyncio.sleep(self.config.health_check_interval)
                
                if not self.is_running:
                    break
                    
                # Simple health check - in a real implementation this would 
                # ping the nodes or check their responsiveness
                self.last_health_check_time = time.time()
                
                # Check if any nodes have been failing for too long and reset them
                current_time = time.time()
                for node in self.base_node_manager.nodes:
                    if (not node.is_available and 
                        node.last_connection_attempt and 
                        current_time - node.last_connection_attempt > 3600):  # 1 hour
                        # Reset node after 1 hour of being unavailable
                        node.consecutive_failures = 0
                        node.is_available = True
                        if self.on_node_health_changed:
                            self.on_node_health_changed(node, True)
                            
            except asyncio.CancelledError:
                break
            except Exception as e:
                if self.on_discovery_error:
                    self.on_discovery_error(e)

class SimpleDiscoveryService:
    """
    Simplified synchronous discovery service for basic use cases
    
    Provides a simple interface for immediate node discovery without 
    background tasks or async functionality.
    """
    
    def __init__(self, 
                 network: TariNetwork = TariNetwork.NEXTNET,
                 wallet_get_seed_peers_fn: Optional[Callable[[], List[str]]] = None):
        self.network = network
        self.network_manager = NetworkManager(network)
        self.base_node_manager = BaseNodeManager()
        self.wallet_get_seed_peers_fn = wallet_get_seed_peers_fn
        
    def discover_and_select_node(self, dns_timeout: float = 5.0) -> Optional[BaseNode]:
        """
        Perform immediate discovery and select the best node
        
        Args:
            dns_timeout: Timeout for DNS resolution
            
        Returns:
            Selected BaseNode or None if no nodes available
        """
        # Get hardcoded nodes
        hardcoded_nodes = self.network_manager.get_hardcoded_base_nodes()
        for node in hardcoded_nodes:
            self.base_node_manager.add_node(node)
            
        # Get DNS nodes
        try:
            dns_nodes = self.network_manager.create_base_nodes_from_dns(dns_timeout)
            for node in dns_nodes:
                self.base_node_manager.add_node(node)
        except Exception:
            pass  # Continue with hardcoded nodes if DNS fails
            
        # Get FFI seed peers if available
        if self.wallet_get_seed_peers_fn:
            try:
                seed_peer_keys = self.wallet_get_seed_peers_fn()
                for i, public_key in enumerate(seed_peer_keys):
                    node = BaseNode(
                        name=f"FFI-Seed-{i+1}",
                        public_key=public_key,
                        address=f"/ip4/127.0.0.1/tcp/{self.network_manager.config.default_port}",
                        is_custom=False,
                        priority=50 + i
                    )
                    self.base_node_manager.add_node(node)
            except Exception:
                pass  # Continue without FFI peers if unavailable
                
        return self.base_node_manager.select_next_node()
        
    def get_available_nodes(self) -> List[BaseNode]:
        """Get all available base nodes"""
        return self.base_node_manager.get_available_nodes()
