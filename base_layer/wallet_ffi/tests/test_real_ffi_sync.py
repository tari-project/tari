#!/usr/bin/env python3
"""
Test Real FFI Sync Operations

Test the enhanced sync_base_node function with real FFI operations
"""

import sys
import os

# Add the Python module path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python'))

import tari_wallet as tw
from tari_wallet import create_wallet_with_auto_discovery, format_base_node_info

def test_real_ffi_sync_operations():
    """Test the enhanced sync operations with real FFI calls"""
    print("=== Testing Real FFI Sync Operations ===")
    
    try:
        print("Creating wallet with explicit workflow...")
        
        # Create wallet with explicit workflow to test all steps
        wallet, base_node_info = create_wallet_with_auto_discovery(
            network="nextnet",
            database_name="ffi_sync_test",
            log_verbosity=0,
            dns_timeout=3.0,
            explicit_workflow=True  # This will test the enhanced workflow
        )
        
        print("✅ Wallet created successfully!")
        print(f"Base node info source: {base_node_info.get('source')}")
        
        # Check if we have workflow info
        if "workflow" in base_node_info:
            workflow = base_node_info["workflow"]
            
            # Step 1: Refresh results
            if "step1_refresh" in workflow:
                refresh_info = workflow["step1_refresh"]
                print(f"\n--- Step 1: Refresh ---")
                print(f"Status: {refresh_info.get('status')}")
                print(f"Nodes discovered: {refresh_info.get('nodes_discovered', 0)}")
            
            # Step 2: Selection results
            if "step2_select" in workflow:
                select_info = workflow["step2_select"]
                print(f"\n--- Step 2: Selection ---") 
                selected = select_info.get("selected_node")
                if selected:
                    print(f"Selected node: {selected.get('name')}")
                print(f"Total available: {select_info.get('total_available', 0)}")
            
            # Step 3: Sync results (the enhanced version)
            if "step3_sync" in workflow:
                sync_info = workflow["step3_sync"]
                print(f"\n--- Step 3: Enhanced Sync ---")
                print(f"Status: {sync_info.get('status')}")
                print(f"Summary: {sync_info.get('summary')}")
                
                # Show individual operation results
                if "operations_tested" in sync_info:
                    print("Operations tested:")
                    for op in sync_info["operations_tested"]:
                        status_icon = "✅" if op["status"] == "success" else "❌"
                        print(f"  {status_icon} {op['operation']}: {op.get('result', op.get('error'))}")
            
            # Step 4: FFI validation results
            if "step4_ffi_validation" in workflow:
                ffi_info = workflow["step4_ffi_validation"]
                print(f"\n--- Step 4: FFI Validation ---")
                print(f"FFI seed peers used: {ffi_info.get('ffi_seed_peers_used', False)}")
                if ffi_info.get("ffi_seed_peers_count"):
                    print(f"FFI seed peers count: {ffi_info['ffi_seed_peers_count']}")
                print(f"Discovery method: {ffi_info.get('discovery_method')}")
        
        # Test individual wallet operations
        print(f"\n--- Individual Wallet Tests ---")
        
        # Test get_seed_peers directly
        try:
            seed_peers = wallet.get_seed_peers()
            print(f"✅ Direct get_seed_peers: {len(seed_peers)} peers")
            if seed_peers:
                print(f"   First peer: {seed_peers[0][:16]}...")
        except Exception as e:
            print(f"❌ Direct get_seed_peers failed: {e}")
        
        # Test get_balance directly  
        try:
            balance = wallet.get_balance()
            print(f"✅ Direct get_balance: available={balance.available}")
        except Exception as e:
            print(f"❌ Direct get_balance failed: {e}")
        
        # Test get_contacts directly
        try:
            contacts = wallet.get_contacts()
            print(f"✅ Direct get_contacts: {len(contacts)} contacts")
        except Exception as e:
            print(f"❌ Direct get_contacts failed: {e}")
            
        return True
        
    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    """Run the enhanced sync test"""
    print("Testing Real FFI Sync Operations")
    print("=" * 50)
    
    success = test_real_ffi_sync_operations()
    
    print("\n" + "=" * 50)
    if success:
        print("✅ Enhanced FFI sync operations test completed!")
    else:
        print("❌ Enhanced FFI sync operations test failed!")
        
    return 0 if success else 1

if __name__ == "__main__":
    sys.exit(main())
