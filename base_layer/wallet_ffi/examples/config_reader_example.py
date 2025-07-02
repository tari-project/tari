#!/usr/bin/env python3
"""
Config Reader Example

This example demonstrates how the discovery system now reads from the actual
Tari configuration files instead of using hardcoded values.
"""

import sys
import os

# Add the Python module path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))

import tari_wallet as tw
from tari_wallet import (
    TariConfigReader,
    get_config_reader,
    read_network_config,
    create_nodes_from_config,
    NetworkManager,
    TariNetwork
)


def config_reader_example():
    """Demonstrate reading from actual Tari config files"""
    print("=== Config Reader Example ===")
    
    # Get the global config reader
    config_reader = get_config_reader()
    
    print(f"Config file path: {config_reader.config_path}")
    
    # Load configuration
    if config_reader.load_config():
        print("✅ Successfully loaded configuration from file")
        
        # Show available networks
        available_networks = config_reader.get_available_networks()
        print(f"Available networks: {available_networks}")
        
        # Show detailed info for each network
        for network in available_networks:
            print(f"\n--- {network.upper()} ---")
            
            # Get raw config
            network_config = config_reader.get_network_config(network)
            print(f"DNS seeds: {network_config.get('dns_seeds', [])}")
            print(f"Peer seeds: {len(network_config.get('peer_seeds', []))} configured")
            
            # Get comprehensive info
            network_info = config_reader.get_network_info(network)
            print(f"Total nodes: {network_info['total_configured_nodes']}")
            
            # Show transport breakdown
            transports = network_info['supported_transports']
            print(f"Transports: IPv4={transports.get('ip4', 0)}, IPv6={transports.get('ip6', 0)}, Onion={transports.get('onion3', 0)}")
            
            # Show first few peer seeds as examples
            nodes = config_reader.create_base_nodes_from_config(network)
            for i, node in enumerate(nodes[:3]):  # Show first 3
                print(f"  Example peer {i+1}: {node.name}")
                print(f"    Public key: {node.public_key[:16]}...")
                print(f"    Address: {node.address}")
                
    else:
        print("❌ Failed to load configuration file")
        print("Using fallback hardcoded configurations")


def network_manager_example():
    """Demonstrate NetworkManager using config files"""
    print("\n=== Network Manager with Config Files ===")
    
    # Test different networks
    networks_to_test = ["nextnet", "mainnet", "stagenet", "localnet"]
    
    for network_name in networks_to_test:
        print(f"\n--- Testing {network_name.upper()} ---")
        
        try:
            network = NetworkManager.get_network_by_name(network_name)
            manager = NetworkManager(network)
            
            # Get network info
            info = manager.get_network_info()
            print(f"Network: {info['name']}")
            print(f"DNS seeds: {info['dns_seeds_count']}")
            print(f"Peer seeds: {info['peer_seeds_count']}")
            print(f"Config loaded: {info['config_file_loaded']}")
            print(f"Explorer: {info.get('explorer_url', 'N/A')}")
            
            # Get actual nodes
            nodes = manager.get_hardcoded_base_nodes()
            print(f"Configured nodes: {len(nodes)}")
            
            if nodes:
                example_node = nodes[0]
                print(f"Example node: {example_node.name}")
                print(f"  Key: {example_node.public_key[:16]}...")
                print(f"  Address: {example_node.address}")
                
        except Exception as e:
            print(f"Error testing {network_name}: {e}")


def comparison_example():
    """Compare old hardcoded vs new config-based approach"""
    print("\n=== Comparison: Config vs Hardcoded ===")
    
    config_reader = get_config_reader()
    
    if config_reader.load_config():
        print("📁 Reading from config file:")
        
        # Nextnet example
        nextnet_config = config_reader.get_network_config("nextnet")
        nextnet_nodes = config_reader.create_base_nodes_from_config("nextnet")
        
        print(f"Nextnet from config:")
        print(f"  DNS seeds: {nextnet_config.get('dns_seeds', [])}")
        print(f"  Peer seeds: {len(nextnet_config.get('peer_seeds', []))}")
        print(f"  Total configured nodes: {len(nextnet_nodes)}")
        
        # Show some real public keys
        for i, node in enumerate(nextnet_nodes[:3]):
            print(f"  Real peer {i+1}: {node.public_key}")
            
    else:
        print("❌ Could not read config file - this would fall back to hardcoded values")


def discovery_integration_example():
    """Show how discovery integrates with config reading"""
    print("\n=== Discovery Integration Example ===")
    
    from tari_wallet import SimpleDiscoveryService
    
    # Create discovery service for nextnet
    discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
    
    print("🔍 Running discovery with config-based peers...")
    
    # This will now use the actual config file
    selected_node = discovery.discover_and_select_node(dns_timeout=3.0)
    
    if selected_node:
        print(f"✅ Selected node: {selected_node.name}")
        print(f"   Public key: {selected_node.public_key}")
        print(f"   Address: {selected_node.address}")
        print(f"   Priority: {selected_node.priority}")
        
        # Check if this is a real key from config vs placeholder
        if selected_node.public_key.startswith("dns_placeholder"):
            print("   ℹ️  This is a DNS-resolved placeholder node")
        else:
            print("   ✅ This is a real configured peer from b_peer_seeds.toml")
    else:
        print("❌ No nodes discovered")
        
    # Show all available nodes
    available_nodes = discovery.get_available_nodes()
    print(f"\nTotal available nodes: {len(available_nodes)}")
    
    # Count real vs placeholder nodes
    real_nodes = [n for n in available_nodes if not n.public_key.startswith("dns_placeholder")]
    placeholder_nodes = [n for n in available_nodes if n.public_key.startswith("dns_placeholder")]
    
    print(f"Real configured nodes: {len(real_nodes)}")
    print(f"DNS placeholder nodes: {len(placeholder_nodes)}")


def wallet_creation_example():
    """Show wallet creation using config-based discovery"""
    print("\n=== Wallet Creation with Config-Based Discovery ===")
    
    try:
        from tari_wallet import create_wallet_with_auto_discovery, format_base_node_info
        
        print("Creating wallet with auto-discovery (using real config)...")
        
        wallet, base_node_info = create_wallet_with_auto_discovery(
            network="nextnet",
            database_name="config_example_wallet",
            log_verbosity=0  # Reduce noise
        )
        
        print("✅ Wallet created successfully!")
        print(f"Discovery result: {format_base_node_info(base_node_info)}")
        
        # Show that we can get seed peers
        try:
            seed_peers = wallet.get_seed_peers()
            print(f"FFI returned {len(seed_peers)} seed peers from wallet")
        except Exception as e:
            print(f"Note: Could not get seed peers from wallet (expected): {e}")
            
    except Exception as e:
        print(f"Wallet creation error: {e}")


def main():
    """Run all examples"""
    print("Config Reader Examples")
    print("=" * 50)
    
    # Basic config reading
    config_reader_example()
    
    # Network manager integration
    network_manager_example()
    
    # Comparison with old approach
    comparison_example()
    
    # Discovery integration
    discovery_integration_example()
    
    # Wallet creation
    wallet_creation_example()
    
    print("\n" + "=" * 50)
    print("Config reader examples completed!")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nExamples interrupted by user")
    except Exception as e:
        print(f"Unexpected error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
