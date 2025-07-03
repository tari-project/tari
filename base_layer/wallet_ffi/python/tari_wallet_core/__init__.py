"""
Tari Wallet Core

This is the core module containing shared functionality for network-specific
Tari wallet packages. It provides the base implementation that can be configured
for different networks (mainnet, testnet, nextnet).
"""

from ..tari_wallet.base_nodes import BaseNode, BaseNodeManager, BaseNodeSelectionStrategy
from ..tari_wallet.network import TariNetwork, NetworkManager, NetworkConfig
from ..tari_wallet.discovery import DiscoveryService, SimpleDiscoveryService, DiscoveryConfig
from ..tari_wallet.config_reader import TariConfigReader, get_config_reader, read_network_config, create_nodes_from_config
from ..tari_wallet.wallet_helper import (
    create_wallet_with_auto_discovery,
    create_discovery_enabled_wallet,
    format_base_node_info,
    get_wallet_seed_peers,
    refresh_base_node_list,
    set_next_base_node,
    sync_base_node
)
from ..tari_wallet.node_selection import PersistentNodeSelector, create_node_selector_for_wallet
from ..tari_wallet.sync_manager import WalletSyncManager, create_sync_manager_for_wallet


def create_network_wallet(
    network_name: str,
    database_name: str = None,
    **kwargs
):
    """
    Create a wallet for a specific network with auto-discovery
    
    This is the core wallet creation function that can be configured
    for any network. Network-specific packages will wrap this function
    with their default network configuration.
    
    Args:
        network_name: Name of the network (mainnet, testnet, nextnet, localnet)
        database_name: Name for wallet database (auto-generated if None)
        **kwargs: Additional arguments passed to create_wallet_with_auto_discovery
        
    Returns:
        Tuple of (wallet, base_node_info)
    """
    if database_name is None:
        database_name = f"{network_name}_wallet"
    
    return create_wallet_with_auto_discovery(
        network=network_name,
        database_name=database_name,
        **kwargs
    )


def get_network_defaults(network_name: str):
    """
    Get default configuration for a specific network
    
    Args:
        network_name: Name of the network
        
    Returns:
        Dictionary of default configuration values
    """
    network = NetworkManager.get_network_by_name(network_name)
    
    return {
        "network": network_name,
        "network_obj": network,
        "discovery_timeout": 30.0,
        "explicit_workflow": True,
        "listen_address": "/ip4/127.0.0.1/tcp/18188"
    }


__all__ = [
    # Core functionality
    'create_network_wallet',
    'get_network_defaults',
    
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
    'format_base_node_info',
    'get_wallet_seed_peers',
    
    # Explicit Discovery Workflow
    'refresh_base_node_list',
    'set_next_base_node',
    'sync_base_node',
    
    # Advanced Components
    'PersistentNodeSelector',
    'create_node_selector_for_wallet',
    'WalletSyncManager',
    'create_sync_manager_for_wallet',
]
