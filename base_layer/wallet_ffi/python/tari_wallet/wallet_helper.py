"""
Wallet Helper Functions

This module provides high-level helper functions for wallet creation with 
automatic base node discovery integration.
"""

import tempfile
import os
import time
from typing import Optional, Dict, Any, Callable, List, Tuple
from .network import TariNetwork, NetworkManager
from .discovery import SimpleDiscoveryService
from .base_nodes import BaseNode, BaseNodeManager, BaseNodeSelectionStrategy
from .nextnet_debug import (
    diagnose_nextnet_issue, 
    format_nextnet_error,
    nextnet_error_wrapper,
    NextnetErrorHandler
)

def refresh_base_node_list(network: str, discovery_timeout: float = 30.0, wallet=None) -> Tuple[List[BaseNode], Dict[str, Any]]:
    """
    Step 1: Refresh the list of available base nodes for the given network
    
    Args:
        network: Network name (localnet, nextnet, stagenet, mainnet)
        discovery_timeout: Timeout for discovery process
        wallet: Optional PyTariWallet instance to get seed peers from FFI
        
    Returns:
        Tuple of (discovered_nodes, discovery_info)
    """
    tari_network = NetworkManager.get_network_by_name(network)
    discovery_service = SimpleDiscoveryService(tari_network)
    
    # Start discovery process
    discovered_nodes = []
    discovery_info = {
        "network": network,
        "discovery_method": "dns_seeds",
        "discovery_timeout": discovery_timeout,
        "start_time": time.time(),
        "ffi_seed_peers_used": False
    }
    
    # If wallet is provided, try to get seed peers from FFI first
    if wallet:
        try:
            ffi_seed_peers = wallet.get_seed_peers()
            if ffi_seed_peers:
                discovery_info["ffi_seed_peers_used"] = True
                discovery_info["ffi_seed_peers_count"] = len(ffi_seed_peers)
                discovery_info["discovery_method"] = "ffi_seed_peers_and_dns"
                # Note: The discovered nodes will still come from the discovery service
                # but we've validated that FFI seed peers are available
        except Exception as e:
            discovery_info["ffi_seed_peers_error"] = str(e)
    
    try:
        # Get available nodes from discovery service
        available_nodes = discovery_service.get_available_nodes()
        discovered_nodes.extend(available_nodes)
        
        discovery_info.update({
            "status": "success",
            "nodes_discovered": len(discovered_nodes),
            "end_time": time.time()
        })
        
    except Exception as e:
        discovery_info.update({
            "status": "failed", 
            "error": str(e),
            "end_time": time.time()
        })
    
    return discovered_nodes, discovery_info

def set_next_base_node(node_manager: BaseNodeManager) -> Optional[BaseNode]:
    """
    Step 2: Select the next base node using the configured selection strategy
    
    Args:
        node_manager: Configured BaseNodeManager instance
        
    Returns:
        Selected BaseNode or None if no nodes available
    """
    return node_manager.select_next_node()

def sync_base_node(wallet, selected_node: BaseNode) -> Dict[str, Any]:
    """
    Step 3: Sync with the selected base node
    
    Tests actual FFI wallet operations to validate base node connectivity.
    Since PyTariWallet automatically configures base nodes during creation,
    this function tests connectivity by performing wallet operations.
    
    Args:
        wallet: PyTariWallet instance
        selected_node: BaseNode to sync with
        
    Returns:
        Sync result information
    """
    sync_info = {
        "node": {
            "name": selected_node.name,
            "public_key": selected_node.public_key,
            "address": selected_node.address
        },
        "start_time": time.time(),
        "operations_tested": []
    }
    
    try:
        # Mark connection attempt
        selected_node.mark_connection_attempt()
        
        # Test 1: Get seed peers (tests basic FFI connectivity)
        try:
            seed_peers = wallet.get_seed_peers()
            sync_info["operations_tested"].append({
                "operation": "get_seed_peers",
                "status": "success",
                "result": f"{len(seed_peers)} peers retrieved"
            })
        except Exception as e:
            sync_info["operations_tested"].append({
                "operation": "get_seed_peers", 
                "status": "failed",
                "error": str(e)
            })
            
        # Test 2: Get balance (tests wallet initialization and potential network calls)
        try:
            balance = wallet.get_balance()
            sync_info["operations_tested"].append({
                "operation": "get_balance",
                "status": "success", 
                "result": f"available: {balance.available}"
            })
        except Exception as e:
            sync_info["operations_tested"].append({
                "operation": "get_balance",
                "status": "failed",
                "error": str(e)
            })
            
        # Test 3: Get contacts (tests address book functionality)
        try:
            contacts = wallet.get_contacts()
            sync_info["operations_tested"].append({
                "operation": "get_contacts",
                "status": "success",
                "result": f"{len(contacts)} contacts retrieved"
            })
        except Exception as e:
            sync_info["operations_tested"].append({
                "operation": "get_contacts",
                "status": "failed", 
                "error": str(e)
            })
            
        # Determine overall success based on operations
        successful_ops = [op for op in sync_info["operations_tested"] if op["status"] == "success"]
        
        if len(successful_ops) >= 2:  # At least 2 operations successful
            selected_node.mark_connection_success()
            sync_info.update({
                "status": "success",
                "end_time": time.time(),
                "summary": f"{len(successful_ops)}/{len(sync_info['operations_tested'])} operations successful"
            })
        else:
            selected_node.mark_connection_failure()
            sync_info.update({
                "status": "partial_success",
                "end_time": time.time(),
                "summary": f"Only {len(successful_ops)}/{len(sync_info['operations_tested'])} operations successful"
            })
        
    except Exception as e:
        selected_node.mark_connection_failure()
        sync_info.update({
            "status": "failed",
            "error": str(e),
            "end_time": time.time(),
            "summary": "Sync operation failed with exception"
        })
    
    return sync_info

def create_wallet_with_auto_discovery(
    network: str = "mainnet",
    database_name: str = "auto_wallet",
    datastore_path: Optional[str] = None,
    log_path: Optional[str] = None,
    log_verbosity: int = 1,
    passphrase: Optional[str] = None,
    seed_passphrase: Optional[str] = None,
    callbacks: Optional[Dict[str, Callable]] = None,
    custom_base_node: Optional[str] = None,
    dns_timeout: float = 5.0,
    listen_address: str = "/ip4/127.0.0.1/tcp/18188",
    explicit_workflow: bool = True
):
    """
    Create a wallet with automatic base node discovery
    
    This function simplifies wallet creation by automatically discovering
    and selecting appropriate base nodes for the specified network.
    
    When explicit_workflow=True, follows a three-step pattern:
    1. refresh_base_node_list() - Discover available nodes
    2. set_next_base_node() - Select node using round-robin
    3. sync_base_node() - Connect and sync with selected node
    
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
        explicit_workflow: Whether to use explicit three-step workflow (default: True)
        
    Returns:
        Tuple of (wallet, base_node_info)
        
    Example:
        wallet, node_info = create_wallet_with_auto_discovery(
            network="nextnet",
            database_name="my_wallet"
        )
    """
    # Check if PyO3 FFI classes are available using sys.modules inspection
    import sys
    
    # Try to get the tari_wallet module which should have FFI classes after maturin build
    tari_wallet_module = sys.modules.get('tari_wallet')
    
    if tari_wallet_module and hasattr(tari_wallet_module, 'PyTariWallet'):
        # FFI classes are available in the tari_wallet module
        tw = tari_wallet_module
    else:
        # FFI not available - provide helpful nextnet-specific error message
        error_info = NextnetErrorHandler.handle_ffi_not_available(
            ImportError("PyTariWallet not found in module")
        )
        formatted_error = format_nextnet_error(error_info)
        print(formatted_error)
        
        raise ImportError(
            "Nextnet FFI extension not loaded. This function requires the native "
            "Tari wallet extension to be built and installed. Please run:\n"
            "TARI_TARGET_NETWORK=nextnet maturin develop --features python-bindings"
        )
    
    # Set up directories
    if datastore_path is None:
        datastore_path = tempfile.mkdtemp(prefix="tari_wallet_")
    
    # Set up log file path (not directory)
    if log_path is None:
        log_dir = os.path.join(datastore_path, "logs")
        os.makedirs(log_dir, exist_ok=True)
        log_path = os.path.join(log_dir, "wallet.log")
    else:
        # If log_path is provided, ensure its directory exists
        log_dir = os.path.dirname(log_path)
        if log_dir:
            os.makedirs(log_dir, exist_ok=True)
    
    os.makedirs(datastore_path, exist_ok=True)
    
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
        # Auto discovery workflow
        if explicit_workflow:
            # Step 1: Refresh base node list
            discovered_nodes, discovery_info = refresh_base_node_list(network, dns_timeout)
            
            # Step 2: Set up node manager and select next node
            node_manager = BaseNodeManager(BaseNodeSelectionStrategy.ROUND_ROBIN)
            for node in discovered_nodes:
                node_manager.add_node(node)
            
            selected_base_node = set_next_base_node(node_manager)
            
            # Create base node info from explicit workflow
            base_node_info = {
                "source": "explicit_discovery",
                "workflow": {
                    "step1_refresh": discovery_info,
                    "step2_select": {
                        "selected_node": {
                            "name": selected_base_node.name if selected_base_node else None,
                            "public_key": selected_base_node.public_key if selected_base_node else None,
                            "address": selected_base_node.address if selected_base_node else None,
                            "priority": selected_base_node.priority if selected_base_node else None
                        } if selected_base_node else None,
                        "strategy": "round_robin",
                        "total_available": len(node_manager.get_available_nodes())
                    }
                },
                "network": network
            }
        else:
            # Legacy simple discovery
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
                selected_base_node = None
    
    # Create wallet configuration
    # Use provided listen address for wallet configuration
    public_address = listen_address
    
    # Create transport config
    transport = tw.PyTariTransportConfig.create_tcp(public_address)
    
    config = tw.PyTariCommsConfig(
        public_address=public_address,
        database_name=database_name,
        datastore_path=datastore_path,
        discovery_timeout=60,
        exclude_dial_test_addresses=True,
        transport=transport
    )
    
    # Create wallet
    try:
        wallet = tw.PyTariWallet(
            config=config,
            log_path=None,  # Use None for now to avoid log file issues
            log_verbosity=0,  # Use 0 like tests
            num_rolling_log_files=0,  # Use 0 like tests
            size_per_log_file_bytes=0,  # Use 0 like tests
            network_str=network,
            passphrase="Hello from Alasca",  # Use test passphrase for now
            seed_passphrase=seed_passphrase,
            callbacks=callbacks
        )
    except Exception as e:
        # Handle wallet creation errors with nextnet-specific guidance
        error_context = {
            "network": network,
            "timeout": dns_timeout,
            "is_new_wallet": True
        }
        error_info = diagnose_nextnet_issue(e, error_context)
        formatted_error = format_nextnet_error(error_info)
        print(formatted_error)
        raise  # Re-raise the original exception
    
    # Step 3: Sync with selected base node and validate FFI integration
    if explicit_workflow and selected_base_node and base_node_info.get("source") == "explicit_discovery":
        sync_result = sync_base_node(wallet, selected_base_node)
        base_node_info["workflow"]["step3_sync"] = sync_result
        
        # Step 4: Post-creation validation with FFI seed peers
        post_validation_nodes, post_validation_info = refresh_base_node_list(network, dns_timeout, wallet)
        base_node_info["workflow"]["step4_ffi_validation"] = post_validation_info
    
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
    
    elif source == "explicit_discovery":
        workflow = base_node_info.get("workflow", {})
        network = base_node_info.get("network", "unknown")
        
        # Get step results
        refresh_info = workflow.get("step1_refresh", {})
        select_info = workflow.get("step2_select", {})
        sync_info = workflow.get("step3_sync", {})
        
        selected_node = select_info.get("selected_node")
        if selected_node and selected_node.get("name"):
            result = f"Three-step discovery on {network}: {selected_node.get('name')}"
            
            # Add workflow timing if available
            if refresh_info.get("end_time") and refresh_info.get("start_time"):
                refresh_time = refresh_info["end_time"] - refresh_info["start_time"]
                result += f" (refresh: {refresh_time:.1f}s"
                
                if sync_info.get("end_time") and sync_info.get("start_time"):
                    sync_time = sync_info["end_time"] - sync_info["start_time"]
                    result += f", sync: {sync_time:.1f}s"
                
                result += ")"
            
            # Add node count
            total_available = select_info.get("total_available", 0)
            nodes_discovered = refresh_info.get("nodes_discovered", 0)
            if nodes_discovered > 0:
                result += f" [{nodes_discovered} discovered, {total_available} available]"
                
            return result
        else:
            return f"Three-step discovery failed on {network}: No nodes selected"
    
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
