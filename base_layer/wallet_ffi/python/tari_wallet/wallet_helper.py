"""
Wallet Helper Functions

This module provides high-level helper functions for wallet creation with 
automatic base node discovery integration.
"""

import tempfile
import os
from typing import Optional, Dict, Any, Callable, List
from .network import TariNetwork, NetworkManager
from .discovery import SimpleDiscoveryService
from .base_nodes import BaseNode

def create_wallet_with_auto_discovery(
    network: str = "nextnet",
    database_name: str = "auto_wallet",
    datastore_path: Optional[str] = None,
    log_path: Optional[str] = None,
    log_verbosity: int = 1,
    passphrase: Optional[str] = None,
    seed_passphrase: Optional[str] = None,
    callbacks: Optional[Dict[str, Callable]] = None,
    custom_base_node: Optional[str] = None,
    dns_timeout: float = 5.0,
    listen_address: str = "/ip4/127.0.0.1/tcp/18188"
):
    """
    Create a wallet with automatic base node discovery
    
    This function simplifies wallet creation by automatically discovering
    and selecting appropriate base nodes for the specified network.
    
    Args:
        network: Network name (localnet, nextnet, stagenet, mainnet)
        database_name: Name for the wallet database
        datastore_path: Directory for wallet data (temp dir if None)
        log_path: Directory for log files (same as datastore_path if None)
        log_verbosity: Log verbosity level (0-4)
        passphrase: Optional wallet passphrase
        seed_passphrase: Optional seed passphrase
        callbacks: Optional event callbacks
        custom_base_node: Optional custom base node (format: "pubkey::/ip4/addr/tcp/port")
        dns_timeout: DNS resolution timeout
        listen_address: Wallet listen address (default: localhost)
        
    Returns:
        Tuple of (wallet, base_node_info)
        
    Example:
        wallet, node_info = create_wallet_with_auto_discovery(
            network="nextnet",
            database_name="my_wallet"
        )
    """
    try:
        import tari_wallet as tw
    except ImportError:
        raise ImportError("tari_wallet module not available. Please install the Python bindings.")
    
    # Set up directories
    if datastore_path is None:
        datastore_path = tempfile.mkdtemp(prefix="tari_wallet_")
    if log_path is None:
        log_path = os.path.join(datastore_path, "logs")
    
    os.makedirs(datastore_path, exist_ok=True)
    os.makedirs(log_path, exist_ok=True)
    
    # Parse network
    tari_network = NetworkManager.get_network_by_name(network)
    
    base_node_info = None
    selected_base_node = None
    
    # Handle custom base node
    if custom_base_node:
        try:
            # Parse custom base node format: "pubkey::/ip4/addr/tcp/port"
            parts = custom_base_node.split("::")
            if len(parts) == 2:
                public_key = parts[0]
                address = parts[1]
                selected_base_node = BaseNode(
                    name="Custom",
                    public_key=public_key,
                    address=address,
                    is_custom=True
                )
                base_node_info = {
                    "source": "custom",
                    "node": {
                        "name": selected_base_node.name,
                        "public_key": selected_base_node.public_key,
                        "address": selected_base_node.address
                    }
                }
        except Exception as e:
            raise ValueError(f"Invalid custom base node format: {e}")
    else:
        # Automatic discovery
        discovery_service = SimpleDiscoveryService(tari_network)
        selected_base_node = discovery_service.discover_and_select_node(dns_timeout)
        
        if selected_base_node:
            available_nodes = discovery_service.get_available_nodes()
            base_node_info = {
                "source": "auto_discovery",
                "selected_node": {
                    "name": selected_base_node.name,
                    "public_key": selected_base_node.public_key,
                    "address": selected_base_node.address,
                    "priority": selected_base_node.priority
                },
                "total_discovered": len(available_nodes),
                "network": network
            }
        else:
            # Fallback to default if discovery fails
            base_node_info = {
                "source": "fallback",
                "message": "No nodes discovered, using default configuration"
            }
    
    # Create wallet configuration
    # Use provided listen address for wallet configuration
    public_address = listen_address
    
    config = tw.PyTariCommsConfig(
        public_address=public_address,
        database_name=database_name,
        datastore_path=datastore_path,
        discovery_timeout=60,
        exclude_dial_test_addresses=True
    )
    
    # Create wallet
    wallet = tw.PyTariWallet(
        config=config,
        log_path=log_path,
        log_verbosity=log_verbosity,
        num_rolling_log_files=5,
        size_per_log_file_bytes=1024*1024,
        network_str=network,
        passphrase=passphrase,
        seed_passphrase=seed_passphrase,
        callbacks=callbacks
    )
    
    return wallet, base_node_info

def get_wallet_seed_peers(wallet) -> List[str]:
    """
    Get seed peers from a wallet instance
    
    Args:
        wallet: PyTariWallet instance
        
    Returns:
        List of seed peer public keys as hex strings
    """
    try:
        return wallet.get_seed_peers()
    except Exception as e:
        print(f"Warning: Failed to get seed peers from wallet: {e}")
        return []

def create_discovery_enabled_wallet(
    network: str = "nextnet",
    database_name: str = "discovery_wallet",
    datastore_path: Optional[str] = None,
    enable_background_discovery: bool = True,
    **wallet_kwargs
):
    """
    Create a wallet with full discovery service integration
    
    This is an advanced version that sets up both the wallet and a discovery
    service for continuous base node monitoring.
    
    Args:
        network: Network name
        database_name: Wallet database name
        datastore_path: Data directory
        enable_background_discovery: Whether to start background discovery
        **wallet_kwargs: Additional arguments for wallet creation
        
    Returns:
        Tuple of (wallet, discovery_service, initial_node_info)
    """
    # Create wallet with auto-discovery
    wallet, initial_node_info = create_wallet_with_auto_discovery(
        network=network,
        database_name=database_name,
        datastore_path=datastore_path,
        **wallet_kwargs
    )
    
    # Create discovery service with wallet integration
    tari_network = NetworkManager.get_network_by_name(network)
    
    discovery_service = None
    if enable_background_discovery:
        from .discovery import DiscoveryService, DiscoveryConfig
        
        # Create discovery service with wallet seed peer function
        discovery_service = DiscoveryService(
            network=tari_network,
            config=DiscoveryConfig(),
            wallet_get_seed_peers_fn=lambda: get_wallet_seed_peers(wallet)
        )
    
    return wallet, discovery_service, initial_node_info

def format_base_node_info(base_node_info: Dict[str, Any]) -> str:
    """
    Format base node information for display
    
    Args:
        base_node_info: Base node information dictionary
        
    Returns:
        Formatted string representation
    """
    if not base_node_info:
        return "No base node information available"
    
    source = base_node_info.get("source", "unknown")
    
    if source == "custom":
        node = base_node_info.get("node", {})
        return f"Custom base node: {node.get('name')} ({node.get('public_key', '')[:16]}...)"
    
    elif source == "auto_discovery":
        selected = base_node_info.get("selected_node", {})
        total = base_node_info.get("total_discovered", 0)
        network = base_node_info.get("network", "unknown")
        return (f"Auto-discovered on {network}: {selected.get('name')} "
                f"(priority {selected.get('priority')}, {total} total nodes available)")
    
    elif source == "fallback":
        message = base_node_info.get("message", "Fallback mode")
        return f"Fallback: {message}"
    
    else:
        return f"Unknown source: {source}"

# Convenience functions for common use cases
def create_nextnet_wallet(database_name: str = "nextnet_wallet", **kwargs):
    """Create a wallet configured for Nextnet"""
    return create_wallet_with_auto_discovery(
        network="nextnet",
        database_name=database_name,
        **kwargs
    )

def create_mainnet_wallet(database_name: str = "mainnet_wallet", **kwargs):
    """Create a wallet configured for Mainnet"""
    return create_wallet_with_auto_discovery(
        network="mainnet",
        database_name=database_name,
        **kwargs
    )

def create_localnet_wallet(database_name: str = "localnet_wallet", **kwargs):
    """Create a wallet configured for Localnet (development)"""
    return create_wallet_with_auto_discovery(
        network="localnet",
        database_name=database_name,
        **kwargs
    )
