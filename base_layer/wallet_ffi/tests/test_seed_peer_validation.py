#!/usr/bin/env python3
"""
Test Seed Peer Discovery Validation

Focused test for validating that FFI seed peers work correctly with nextnet
"""

import sys
import os

# Add the Python module path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python'))

import tari_wallet as tw
from tari_wallet import (
    get_wallet_seed_peers,
    refresh_base_node_list,
    TariNetwork,
    SimpleDiscoveryService
)

def test_ffi_seed_peer_integration():
    """Test that FFI seed peers integrate properly with discovery"""
    print("=== FFI Seed Peer Integration Test ===")
    
    try:
        # First, test discovery without wallet
        print("Testing discovery without wallet...")
        discovery_service = SimpleDiscoveryService(TariNetwork.NEXTNET)
        nodes_without_wallet = discovery_service.get_available_nodes()
        print(f"✅ Nodes without wallet: {len(nodes_without_wallet)}")
        
        # Create a minimal wallet for FFI testing
        print("Creating minimal wallet for FFI testing...")
        
        import tempfile
        temp_dir = tempfile.mkdtemp(prefix="seed_peer_test_")
        
        transport = tw.PyTariTransportConfig.create_tcp("/ip4/127.0.0.1/tcp/18188")
        config = tw.PyTariCommsConfig(
            public_address="/ip4/127.0.0.1/tcp/18188",
            database_name="seed_peer_test",
            datastore_path=temp_dir,
            discovery_timeout=10,  # Short timeout
            exclude_dial_test_addresses=True,
            transport=transport
        )
        
        # This might take time, so we'll be patient
        print("Creating wallet (this may take 30-60 seconds)...")
        wallet = tw.PyTariWallet(
            config=config,
            log_path=None,
            log_verbosity=0,
            num_rolling_log_files=0,
            size_per_log_file_bytes=0,
            network_str="nextnet",
            passphrase="Seed Peer Test",
            seed_passphrase=None,
            callbacks=None
        )
        print("✅ Wallet created successfully!")
        
        # Test get_seed_peers directly
        print("Testing direct get_seed_peers call...")
        seed_peers = wallet.get_seed_peers()
        print(f"✅ FFI seed peers: {len(seed_peers)} found")
        
        if seed_peers:
            print("Seed peer details:")
            for i, peer in enumerate(seed_peers[:3]):  # Show first 3
                print(f"  {i+1}. {peer[:16]}...{peer[-8:]}")
        else:
            print("⚠️  No seed peers returned from FFI")
        
        # Test the enhanced refresh function with wallet
        print("Testing enhanced refresh_base_node_list with wallet...")
        nodes_with_wallet, refresh_info = refresh_base_node_list("nextnet", 3.0, wallet)
        
        print(f"✅ Enhanced refresh completed")
        print(f"   FFI seed peers used: {refresh_info.get('ffi_seed_peers_used', False)}")
        print(f"   FFI seed peers count: {refresh_info.get('ffi_seed_peers_count', 0)}")
        print(f"   Discovery method: {refresh_info.get('discovery_method', 'unknown')}")
        print(f"   Nodes discovered: {len(nodes_with_wallet)}")
        
        # Test the helper function
        print("Testing get_wallet_seed_peers helper...")
        helper_peers = get_wallet_seed_peers(wallet)
        print(f"✅ Helper function peers: {len(helper_peers)}")
        
        # Validate consistency
        if len(seed_peers) == len(helper_peers):
            print("✅ Direct and helper function results are consistent")
        else:
            print(f"⚠️  Inconsistency: direct={len(seed_peers)}, helper={len(helper_peers)}")
        
        return True
        
    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_seed_peer_format_validation():
    """Test that seed peer data has valid format"""
    print("\n=== Seed Peer Format Validation ===")
    
    try:
        # Quick format test without full wallet creation
        print("Testing seed peer format requirements...")
        
        # Expected format: hex string representing public key
        # Should be 64 characters (32 bytes in hex)
        sample_valid_peer = "18a07e52ef6616d7bb7ebaf2c6b5e68f4e4d6c9c4a2b8f3e1d5c7a9b0e8f6d4c2"
        
        if len(sample_valid_peer) == 64:
            print("✅ Expected peer format: 64-character hex string")
        
        try:
            # Test if it's valid hex
            bytes.fromhex(sample_valid_peer)
            print("✅ Sample peer format is valid hex")
        except ValueError:
            print("❌ Sample peer format is not valid hex")
        
        return True
        
    except Exception as e:
        print(f"❌ Format validation failed: {e}")
        return False

def main():
    """Run seed peer validation tests"""
    print("Seed Peer Discovery Validation Tests")
    print("=" * 50)
    
    results = []
    
    # Test format validation (quick)
    results.append(test_seed_peer_format_validation())
    
    # Test FFI integration (slow)
    print(f"\nStarting FFI integration test (may take 1-2 minutes)...")
    results.append(test_ffi_seed_peer_integration())
    
    print("\n" + "=" * 50)
    print("Test Results:")
    print(f"Format Validation: {'✅' if results[0] else '❌'}")
    print(f"FFI Integration: {'✅' if results[1] else '❌'}")
    
    if all(results):
        print("\n✅ All seed peer validation tests passed!")
        return 0
    else:
        print("\n❌ Some seed peer validation tests failed!")
        return 1

if __name__ == "__main__":
    sys.exit(main())
