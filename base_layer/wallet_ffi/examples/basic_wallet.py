#!/usr/bin/env python3
"""
Basic Tari Wallet Example

This example demonstrates the basic usage of the Tari Python wallet bindings,
including wallet creation, balance checking, and message signing.
"""

import sys
import os
import tempfile
import tari_wallet


def main():
    """Main example function."""
    
    # Create a temporary directory for wallet data
    temp_dir = tempfile.mkdtemp(prefix="tari_wallet_example_")
    print(f"Using temporary directory: {temp_dir}")
    
    try:
        # Step 1: Create wallet configuration
        print("Creating wallet configuration...")
        config = tari_wallet.PyTariCommsConfig(
            public_address="/ip4/127.0.0.1/tcp/18188",
            database_name="example_wallet",
            datastore_path=temp_dir,
            discovery_timeout=60,
            exclude_dial_test_addresses=True
        )
        
        # Step 2: Define event callbacks (optional)
        def on_balance_updated(balance_ptr):
            print(f"[Event] Balance updated: {balance_ptr}")
        
        def on_transaction_mined(tx_ptr):
            print(f"[Event] Transaction mined: {tx_ptr}")
        
        def on_connectivity_status(status):
            print(f"[Event] Connectivity status: {status}")
        
        callbacks = {
            "balance_updated": on_balance_updated,
            "transaction_mined": on_transaction_mined,
            "connectivity_status": on_connectivity_status,
        }
        
        # Step 3: Create wallet
        print("Creating wallet...")
        wallet = tari_wallet.PyTariWallet(
            config=config,
            log_path=os.path.join(temp_dir, "logs"),
            log_verbosity=1,  # Info level
            num_rolling_log_files=5,
            size_per_log_file_bytes=1024*1024,  # 1MB per log file
            network_str="localnet",
            callbacks=callbacks
        )
        
        print("Wallet created successfully!")
        
        # Step 4: Get wallet balance
        print("\nChecking wallet balance...")
        try:
            balance = wallet.get_balance()
            print(f"Available balance: {balance.available} microTari")
            print(f"Time-locked balance: {balance.time_locked} microTari")
            print(f"Pending incoming: {balance.pending_incoming} microTari")
            print(f"Pending outgoing: {balance.pending_outgoing} microTari")
        except tari_wallet.TariWalletError as e:
            print(f"Error getting balance: {e}")
        
        # Step 5: Sign and verify a message
        print("\nSigning a message...")
        message = "Hello from Tari Python bindings!"
        
        try:
            signature = wallet.sign_message(message)
            print(f"Message: {message}")
            print(f"Signature: {signature}")
            
            # For verification, we would need the wallet's public key
            # This is just an example of the API
            print(f"Message signed successfully!")
            
        except tari_wallet.TariWalletError as e:
            print(f"Error signing message: {e}")
        
        # Step 6: Demonstrate public key operations
        print("\nDemonstrating public key operations...")
        try:
            # Create a public key from hex (example key)
            example_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            
            try:
                public_key = tari_wallet.PyTariPublicKey.from_hex(example_hex)
                print(f"Created public key from hex")
                print(f"Public key hex: {public_key.to_hex()}")
                print(f"Public key emoji: {public_key.to_emoji_encoding()}")
            except tari_wallet.TariWalletError as e:
                print(f"Note: Example public key creation failed (expected): {e}")
            
        except Exception as e:
            print(f"Error with public key operations: {e}")
        
        # Step 7: Get completed transactions
        print("\nGetting completed transactions...")
        try:
            transactions = wallet.get_completed_transactions(limit=5)
            print(f"Found {len(transactions)} completed transactions")
            for i, tx_id in enumerate(transactions):
                print(f"  Transaction {i+1}: ID {tx_id}")
        except tari_wallet.TariWalletError as e:
            print(f"Error getting transactions: {e}")
        
        # Step 8: Get contacts
        print("\nGetting contacts...")
        try:
            contacts = wallet.get_contacts()
            print(f"Found {len(contacts)} contacts")
            for alias, address in contacts:
                print(f"  Contact: {alias} -> {address[:16]}...")
        except tari_wallet.TariWalletError as e:
            print(f"Error getting contacts: {e}")
        
        print("\nExample completed successfully!")
        
    except tari_wallet.TariWalletError as e:
        print(f"Wallet error: {e}")
        return 1
    except Exception as e:
        print(f"Unexpected error: {e}")
        return 1
    
    finally:
        # Cleanup (optional - Python will handle it automatically)
        try:
            import shutil
            shutil.rmtree(temp_dir)
            print(f"Cleaned up temporary directory: {temp_dir}")
        except Exception as e:
            print(f"Warning: Could not clean up temporary directory: {e}")
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
