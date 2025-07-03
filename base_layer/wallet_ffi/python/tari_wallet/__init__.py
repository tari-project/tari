"""
Tari Wallet Python Package

This package provides Python-native utilities for working with Tari wallets,
including base node discovery and management.
"""

from .base_nodes import BaseNode, BaseNodeManager, BaseNodeSelectionStrategy
from .network import TariNetwork, NetworkManager, NetworkConfig
from .discovery import DiscoveryService, SimpleDiscoveryService, DiscoveryConfig
from .config_reader import TariConfigReader, get_config_reader, read_network_config, create_nodes_from_config
from .wallet_helper import (
    create_wallet_with_auto_discovery,
    create_discovery_enabled_wallet,
    create_nextnet_wallet,
    create_mainnet_wallet,
    create_localnet_wallet,
    format_base_node_info,
    get_wallet_seed_peers,
    refresh_base_node_list,
    set_next_base_node,
    sync_base_node
)

__all__ = [
    # Base Node Management
    'BaseNode',
    'BaseNodeManager', 
    'BaseNodeSelectionStrategy',
    
    # Network Configuration
    'TariNetwork',
    'NetworkManager',
    'NetworkConfig',
    
    # Config Reader
    'TariConfigReader',
    'get_config_reader',
    'read_network_config', 
    'create_nodes_from_config',
    
    # Discovery Services
    'DiscoveryService',
    'SimpleDiscoveryService',
    'DiscoveryConfig',
    
    # Wallet Helper Functions
    'create_wallet_with_auto_discovery',
    'create_discovery_enabled_wallet',
    'create_nextnet_wallet',
    'create_mainnet_wallet',
    'create_localnet_wallet',
    'format_base_node_info',
    'get_wallet_seed_peers',
    
    # Explicit Discovery Workflow
    'refresh_base_node_list',
    'set_next_base_node',
    'sync_base_node',
]
