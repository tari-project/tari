#!/usr/bin/env python3
"""
Base Node Discovery Example

This example demonstrates the new base node discovery functionality,
showing how to create wallets with automatic base node selection.
"""

import sys
import os
import tempfile
import asyncio

# Add the Python module path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))

import tari_wallet as tw
from tari_wallet import (
    create_wallet_with_auto_discovery,
    create_discovery_enabled_wallet,
    format_base_node_info,
    TariNetwork,
    SimpleDiscoveryService,
    BaseNodeSelectionStrategy
)


def basic_discovery_example():
    """Demonstrate basic wallet creation with auto-discovery"""
    print("=== Basic Discovery Example ===")
    
    try:
        # Create wallet with automatic base node discovery
        wallet, base_node_info = create_wallet_with_auto_discovery(
            network="nextnet",
            database_name="discovery_example",
            log_verbosity=1
        )
        
        print("Wallet created successfully!")
        print(f"Base node info: {format_base_node_info(base_node_info)}")
        
        # Get seed peers from the wallet
        print("\nGetting seed peers from wallet FFI...")
        try:
            seed_peers = wallet.get_seed_peers()
            print(f"Found {len(seed_peers)} seed peers:")
            for i, peer_key in enumerate(seed_peers[:3]):  # Show first 3
                print(f"  {i+1}. {peer_key[:16]}...")
        except Exception as e:
            print(f"Could not get seed peers: {e}")
        
        # Get wallet balance (should work if discovery was successful)
        print("\nChecking wallet balance...")
        try:
            balance = wallet.get_balance()
            print(f"Balance check successful - Available: {balance.available} microTari")
        except Exception as e:
            print(f"Balance check failed (expected for new wallet): {e}")
            
        return wallet, base_node_info
        
    except Exception as e:
        print(f"Error in basic discovery: {e}")
        return None, None


def discovery_service_example():
    """Demonstrate discovery service with network exploration"""
    print("\n=== Discovery Service Example ===")
    
    try:
        # Create a simple discovery service
        discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
        
        print("Discovering base nodes...")
        selected_node = discovery.discover_and_select_node(dns_timeout=3.0)
        
        if selected_node:
            print(f"Selected node: {selected_node.name}")
            print(f"  Public key: {selected_node.public_key[:16]}...")
            print(f"  Address: {selected_node.address}")
            print(f"  Priority: {selected_node.priority}")
            print(f"  Health score: {selected_node.get_health_score():.2f}")
        else:
            print("No nodes discovered")
            
        # Show all available nodes
        available_nodes = discovery.get_available_nodes()
        print(f"\nTotal available nodes: {len(available_nodes)}")
        for i, node in enumerate(available_nodes[:5]):  # Show first 5
            print(f"  {i+1}. {node.name} (priority {node.priority})")
            
        return discovery
        
    except Exception as e:
        print(f"Error in discovery service: {e}")
        return None


async def advanced_discovery_example():
    """Demonstrate advanced discovery with background monitoring"""
    print("\n=== Advanced Discovery Example ===")
    
    try:
        # Create wallet with full discovery service
        wallet, discovery_service, initial_info = create_discovery_enabled_wallet(
            network="nextnet",
            database_name="advanced_discovery",
            enable_background_discovery=True
        )
        
        print("Advanced wallet created!")
        print(f"Initial discovery: {format_base_node_info(initial_info)}")
        
        if discovery_service:
            # Set up event callbacks
            def on_nodes_discovered(nodes):
                print(f"[Event] Discovered {len(nodes)} nodes")
                
            def on_node_health_changed(node, is_healthy):
                status = "healthy" if is_healthy else "unhealthy"
                print(f"[Event] Node {node.name} is now {status}")
                
            def on_discovery_error(error):
                print(f"[Event] Discovery error: {error}")
            
            discovery_service.on_nodes_discovered = on_nodes_discovered
            discovery_service.on_node_health_changed = on_node_health_changed
            discovery_service.on_discovery_error = on_discovery_error
            
            # Start discovery service
            print("Starting background discovery...")
            await discovery_service.start()
            
            # Let it run for a few seconds
            await asyncio.sleep(5)
            
            # Check status
            status = discovery_service.get_discovery_status()
            print(f"Discovery status: {status['node_statistics']['total_nodes']} nodes managed")
            print(f"Current node: {status['node_statistics']['current_node']['name']}")
            
            # Stop the service
            await discovery_service.stop()
            print("Discovery service stopped")
            
        return wallet, discovery_service
        
    except Exception as e:
        print(f"Error in advanced discovery: {e}")
        return None, None


def network_comparison_example():
    """Compare discovery across different networks"""
    print("\n=== Network Comparison Example ===")
    
    networks = ["localnet", "nextnet", "stagenet", "mainnet"]
    
    for network in networks:
        try:
            print(f"\n--- {network.upper()} ---")
            discovery = SimpleDiscoveryService(TariNetwork(network))
            
            # Get network info
            network_info = discovery.network_manager.get_network_info()
            print(f"DNS seeds: {len(network_info['dns_seeds'])}")
            print(f"Hardcoded peers: {network_info['hardcoded_peers_count']}")
            
            # Try discovery
            selected_node = discovery.discover_and_select_node(dns_timeout=2.0)
            available_count = len(discovery.get_available_nodes())
            
            if selected_node:
                print(f"Discovery successful: {available_count} nodes found")
                print(f"Selected: {selected_node.name}")
            else:
                print("No nodes discovered")
                
        except Exception as e:
            print(f"Error with {network}: {e}")


def custom_base_node_example():
    """Demonstrate custom base node specification"""
    print("\n=== Custom Base Node Example ===")
    
    try:
        # Use a custom base node (example format)
        custom_node = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef::/ip4/192.168.1.100/tcp/18189"
        
        wallet, base_node_info = create_wallet_with_auto_discovery(
            network="nextnet",
            database_name="custom_node_example",
            custom_base_node=custom_node
        )
        
        print("Wallet with custom base node created!")
        print(f"Base node info: {format_base_node_info(base_node_info)}")
        
        return wallet, base_node_info
        
    except Exception as e:
        print(f"Error with custom base node: {e}")
        return None, None


async def main():
    """Run all examples"""
    print("Base Node Discovery Examples")
    print("=" * 50)
    
    # Basic discovery
    wallet1, info1 = basic_discovery_example()
    
    # Discovery service  
    discovery1 = discovery_service_example()
    
    # Network comparison
    network_comparison_example()
    
    # Custom base node
    wallet2, info2 = custom_base_node_example()
    
    # Advanced discovery (async)
    wallet3, discovery2 = await advanced_discovery_example()
    
    print("\n" + "=" * 50)
    print("All examples completed!")
    
    # Summary
    successful_wallets = sum(1 for w in [wallet1, wallet2, wallet3] if w is not None)
    print(f"Successfully created {successful_wallets} wallets with discovery")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nExamples interrupted by user")
    except Exception as e:
        print(f"Unexpected error: {e}")
        sys.exit(1)
