#!/usr/bin/env python3
"""
Explicit Discovery Workflow Example

This example demonstrates the three-step base node discovery workflow:
1. refresh_base_node_list() - Discover available nodes
2. set_next_base_node() - Select node using round-robin  
3. sync_base_node() - Connect and sync with selected node

Shows both high-level auto-discovery and step-by-step control.
"""

import sys
import os
import tempfile
import time

# Add the Python module path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'python'))

import tari_wallet as tw
from tari_wallet import (
    create_wallet_with_auto_discovery,
    refresh_base_node_list,
    set_next_base_node, 
    sync_base_node,
    format_base_node_info,
    TariNetwork,
    NetworkManager,
    BaseNodeSelectionStrategy
)
from tari_wallet.node_selection import PersistentNodeSelector, create_node_selector_for_wallet
from tari_wallet.sync_manager import WalletSyncManager, create_sync_manager_for_wallet


def step_by_step_discovery_example():
    """Demonstrate explicit step-by-step discovery workflow"""
    print("=== Step-by-Step Discovery Workflow ===")
    
    network = "nextnet"
    
    # Step 1: Refresh base node list
    print("\n1. Refreshing base node list...")
    start_time = time.time()
    
    discovered_nodes, discovery_info = refresh_base_node_list(network, discovery_timeout=30.0)
    
    refresh_time = time.time() - start_time
    print(f"   ✓ Discovered {len(discovered_nodes)} nodes in {refresh_time:.2f}s")
    print(f"   Status: {discovery_info.get('status')}")
    if discovery_info.get('error'):
        print(f"   Error: {discovery_info.get('error')}")
    
    # Show discovered nodes
    if discovered_nodes:
        print(f"   Nodes found:")
        for i, node in enumerate(discovered_nodes[:3]):  # Show first 3
            print(f"     {i+1}. {node.name} ({node.public_key[:16]}...)")
    
    if not discovered_nodes:
        print("   No nodes discovered, cannot continue with example")
        return
    
    # Step 2: Set up node manager and select next node
    print("\n2. Setting up round-robin node selection...")
    
    # Create temporary directory for this example
    temp_dir = tempfile.mkdtemp(prefix="explicit_discovery_")
    print(f"   Using temp directory: {temp_dir}")
    
    # Create persistent node selector
    node_selector = create_node_selector_for_wallet(temp_dir, "explicit_example")
    node_selector.add_nodes(discovered_nodes)
    
    # Select first node
    selected_node = set_next_base_node(node_selector.node_manager)
    
    if selected_node:
        print(f"   ✓ Selected: {selected_node.name} ({selected_node.public_key[:16]}...)")
        print(f"   Selection strategy: {node_selector.node_manager.strategy.value}")
        
        # Show selection statistics
        stats = node_selector.get_selection_statistics()
        print(f"   Available nodes: {stats['available_nodes']}/{stats['total_nodes']}")
    else:
        print("   ✗ No node could be selected")
        return
    
    # Step 3: Create wallet and sync
    print("\n3. Creating wallet and syncing with selected node...")
    
    try:
        # Create wallet with explicit workflow disabled to avoid double-sync
        wallet, base_node_info = create_wallet_with_auto_discovery(
            network=network,
            database_name="explicit_workflow_example",
            datastore_path=temp_dir,
            explicit_workflow=False  # We're doing the workflow manually
        )
        
        print("   ✓ Wallet created successfully")
        
        # Now do explicit sync with our selected node
        sync_start = time.time()
        sync_result = sync_base_node(wallet, selected_node)
        sync_time = time.time() - sync_start
        
        print(f"   ✓ Sync completed in {sync_time:.2f}s")
        print(f"   Sync status: {sync_result.get('status')}")
        
        if sync_result.get('error'):
            print(f"   Sync error: {sync_result.get('error')}")
        
    except Exception as e:
        print(f"   ✗ Wallet creation/sync failed: {e}")
        return
    
    # Demonstrate multiple node rotations
    print("\n4. Demonstrating node rotation...")
    
    for i in range(3):
        print(f"   Rotation {i+1}:")
        next_node = set_next_base_node(node_selector.node_manager)
        if next_node:
            print(f"     Selected: {next_node.name} ({next_node.public_key[:16]}...)")
        else:
            print("     No more nodes available")
            break
    
    # Show final statistics
    print("\n5. Final Statistics:")
    stats = node_selector.get_selection_statistics()
    print(f"   Total selections: {stats['total_selections']}")
    print(f"   Session time: {stats['session_time_seconds']:.1f}s")
    print(f"   Current index: {stats['current_index']}")
    
    # Cleanup
    import shutil
    try:
        shutil.rmtree(temp_dir)
        print(f"\n   Cleaned up: {temp_dir}")
    except Exception as e:
        print(f"\n   Warning: Could not clean up {temp_dir}: {e}")


def auto_discovery_with_timing_example():
    """Demonstrate high-level auto-discovery with timing measurements"""
    print("\n=== Auto-Discovery with Performance Timing ===")
    
    # Test multiple networks for comparison
    networks = ["nextnet", "localnet"]
    
    for network in networks:
        print(f"\nTesting {network} network:")
        
        # Create wallet with explicit workflow (default)
        start_time = time.time()
        
        try:
            wallet, base_node_info = create_wallet_with_auto_discovery(
                network=network,
                database_name=f"timing_test_{network}",
                log_verbosity=0  # Minimal logging for timing
            )
            
            total_time = time.time() - start_time
            
            print(f"   ✓ Total time: {total_time:.2f}s")
            print(f"   {format_base_node_info(base_node_info)}")
            
            # Show workflow timing breakdown if available
            if base_node_info.get("source") == "explicit_discovery":
                workflow = base_node_info.get("workflow", {})
                
                refresh_info = workflow.get("step1_refresh", {})
                if refresh_info.get("refresh_time_seconds"):
                    print(f"     - Refresh: {refresh_info['refresh_time_seconds']:.2f}s")
                
                sync_info = workflow.get("step3_sync", {})
                if sync_info.get("connection_time_seconds"):
                    print(f"     - Sync: {sync_info['connection_time_seconds']:.2f}s")
            
        except Exception as e:
            print(f"   ✗ Failed: {e}")


def caching_demonstration():
    """Demonstrate peer caching for instant subsequent connections"""
    print("\n=== Peer Caching Demonstration ===")
    
    network_obj = NetworkManager.get_network_by_name("nextnet")
    temp_dir = tempfile.mkdtemp(prefix="caching_demo_")
    
    # Create sync manager with caching
    sync_manager = create_sync_manager_for_wallet(network_obj, temp_dir, "cache_demo")
    
    print(f"Using temp directory: {temp_dir}")
    
    # Check initial cache state
    cache_stats = sync_manager.get_cache_statistics()
    print(f"Initial cache: {cache_stats['total_cached_peers']} peers")
    
    # Check if refresh is needed
    refresh_needed = sync_manager.is_refresh_needed(max_age_minutes=30)
    print(f"Refresh needed: {refresh_needed}")
    
    if refresh_needed:
        print("\nRefreshing node list with caching...")
        start_time = time.time()
        
        nodes, refresh_info = sync_manager.refresh_base_node_list(discovery_timeout=30.0)
        refresh_time = time.time() - start_time
        
        print(f"   ✓ Refresh completed in {refresh_time:.2f}s")
        print(f"   Cache hits: {refresh_info.get('cache_hits', 0)}")
        print(f"   New discoveries: {refresh_info.get('new_discoveries', 0)}")
        print(f"   Total nodes: {len(nodes)}")
        
        # Show updated cache stats
        cache_stats = sync_manager.get_cache_statistics()
        print(f"   Updated cache: {cache_stats['total_cached_peers']} peers")
    
    # Show instant connection candidates
    instant_candidates = sync_manager.get_instant_connection_candidates()
    print(f"\nInstant connection candidates: {len(instant_candidates)}")
    for i, node in enumerate(instant_candidates[:3]):
        print(f"   {i+1}. {node.name} ({node.public_key[:16]}...)")
    
    # Cleanup
    import shutil
    try:
        shutil.rmtree(temp_dir)
        print(f"\nCleaned up: {temp_dir}")
    except Exception as e:
        print(f"Warning: Could not clean up {temp_dir}: {e}")


def main():
    """Run all discovery workflow examples"""
    print("Tari Wallet - Explicit Discovery Workflow Examples")
    print("=" * 55)
    
    try:
        # Step-by-step workflow
        step_by_step_discovery_example()
        
        # Auto-discovery with timing
        auto_discovery_with_timing_example()
        
        # Caching demonstration
        caching_demonstration()
        
        print("\n" + "=" * 55)
        print("All examples completed successfully!")
        
    except KeyboardInterrupt:
        print("\n\nExample interrupted by user")
        return 1
    except Exception as e:
        print(f"\n\nExample failed with error: {e}")
        import traceback
        traceback.print_exc()
        return 1
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
