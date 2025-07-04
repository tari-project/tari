#!/usr/bin/env python3
"""
Simple Nextnet Test

Test nextnet functionality without full wallet creation
"""

import sys
import os

# Add the Python module path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))

import tari_wallet as tw
from tari_wallet import (
    TariNetwork,
    SimpleDiscoveryService,
    format_base_node_info
)

def test_discovery_only():
    """Test just the discovery service without wallet creation"""
    print("=== Simple Discovery Test ===")
    
    try:
        # Create discovery service for nextnet
        discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
        print("✅ Discovery service created")
        
        # Get available nodes
        available_nodes = discovery.get_available_nodes()
        print(f"✅ Found {len(available_nodes)} available nodes")
        
        # Try to discover and select a node
        print("Discovering nodes...")
        selected_node = discovery.discover_and_select_node(dns_timeout=3.0)
        
        if selected_node:
            print("✅ Node discovery successful!")
            print(f"Selected node: {selected_node.name}")
            print(f"  Public key: {selected_node.public_key[:16]}...")
            print(f"  Address: {selected_node.address}")
            print(f"  Priority: {selected_node.priority}")
            print(f"  Health score: {selected_node.get_health_score():.2f}")
        else:
            print("⚠️  No nodes discovered")
            
        # Show all available nodes
        all_nodes = discovery.get_available_nodes()
        print(f"\nTotal available nodes after discovery: {len(all_nodes)}")
        for i, node in enumerate(all_nodes[:5]):  # Show first 5
            print(f"  {i+1}. {node.name} (priority {node.priority})")
            
        return True
        
    except Exception as e:
        print(f"❌ Discovery test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_ffi_basics():
    """Test FFI class availability"""
    print("\n=== FFI Basics Test ===")
    
    try:
        # Test that FFI classes are available
        if hasattr(tw, 'PyTariWallet'):
            print("✅ PyTariWallet available")
        else:
            print("❌ PyTariWallet not available")
            return False
            
        if hasattr(tw, 'PyTariCommsConfig'):
            print("✅ PyTariCommsConfig available")
        else:
            print("❌ PyTariCommsConfig not available")
            return False
            
        # Test creating transport config (this should be fast)
        transport = tw.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18188")
        print("✅ Transport config created successfully")
        
        return True
        
    except Exception as e:
        print(f"❌ FFI basics test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_network_comparison():
    """Compare discovery across different networks (quick test)"""
    print("\n=== Network Comparison Test ===")
    
    networks = ["nextnet", "mainnet", "testnet"]
    
    for network in networks:
        try:
            print(f"\n--- {network.upper()} ---")
            discovery = SimpleDiscoveryService(TariNetwork(network))
            
            # Get network info without actual discovery
            network_info = discovery.network_manager.get_network_info()
            print(f"DNS seeds: {len(network_info.get('dns_seeds', []))}")
            
            # Get initial available nodes (from config)
            available_nodes = discovery.get_available_nodes()
            print(f"Initial nodes: {len(available_nodes)}")
            
            # Quick discovery test (very short timeout)
            selected_node = discovery.discover_and_select_node(dns_timeout=1.0)
            if selected_node:
                print(f"Quick discovery: {selected_node.name}")
            else:
                print("Quick discovery: No nodes found")
                
        except Exception as e:
            print(f"Error with {network}: {e}")

def main():
    """Run simple tests"""
    print("Simple Nextnet Discovery Tests")
    print("=" * 50)
    
    results = []
    
    # Test FFI basics
    results.append(test_ffi_basics())
    
    # Test discovery only
    results.append(test_discovery_only())
    
    # Test network comparison
    test_network_comparison()
    
    print("\n" + "=" * 50)
    print("Test Summary:")
    print(f"FFI Basics: {'✅' if results[0] else '❌'}")
    print(f"Discovery: {'✅' if results[1] else '❌'}")
    
    if all(results):
        print("\n✅ All core tests passed!")
        print("Next step: Try full wallet creation with longer timeout")
        return 0
    else:
        print("\n❌ Some tests failed")
        return 1

if __name__ == "__main__":
    sys.exit(main())
