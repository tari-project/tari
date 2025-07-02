#!/usr/bin/env python3
"""
Tari Transaction Example

This example demonstrates transaction operations with the Tari Python wallet bindings,
including sending transactions and monitoring transaction events.
"""

import sys
import os
import tempfile
import time
import threading
import tari_wallet


class TransactionMonitor:
    """Monitor for transaction events."""
    
    def __init__(self):
        self.received_transactions = []
        self.mined_transactions = []
        self.transaction_events = []
        self.lock = threading.Lock()
    
    def on_received_transaction(self, tx_ptr):
        with self.lock:
            print(f"[EVENT] Received transaction: {tx_ptr}")
            self.received_transactions.append(tx_ptr)
    
    def on_transaction_mined(self, tx_ptr):
        with self.lock:
            print(f"[EVENT] Transaction mined: {tx_ptr}")
            self.mined_transactions.append(tx_ptr)
    
    def on_transaction_broadcast(self, tx_ptr):
        with self.lock:
            print(f"[EVENT] Transaction broadcast: {tx_ptr}")
            self.transaction_events.append(('broadcast', tx_ptr))
    
    def on_transaction_send_result(self, tx_id, status):
        with self.lock:
            print(f"[EVENT] Transaction send result: ID={tx_id}, Status={status}")
            self.transaction_events.append(('send_result', tx_id, status))
    
    def on_balance_updated(self, balance_ptr):
        with self.lock:
            print(f"[EVENT] Balance updated: {balance_ptr}")
    
    def get_callbacks(self):
        """Get callback dictionary for wallet."""
        return {
            "received_transaction": self.on_received_transaction,
            "transaction_mined": self.on_transaction_mined,
            "transaction_broadcast": self.on_transaction_broadcast,
            "transaction_send_result": self.on_transaction_send_result,
            "balance_updated": self.on_balance_updated,
        }


def create_test_wallet(temp_dir, name, port):
    """Create a test wallet with the given parameters."""
    
    config = tari_wallet.PyTariCommsConfig(
        public_address=f"/ip4/127.0.0.1/tcp/{port}",
        database_name=f"{name}_wallet",
        datastore_path=os.path.join(temp_dir, name),
        discovery_timeout=30,
        exclude_dial_test_addresses=True
    )
    
    monitor = TransactionMonitor()
    
    wallet = tari_wallet.PyTariWallet(
        config=config,
        log_path=os.path.join(temp_dir, name, "logs"),
        log_verbosity=2,  # Debug level for more detailed logs
        num_rolling_log_files=3,
        size_per_log_file_bytes=512*1024,  # 512KB per log file
        network_str="localnet",
        callbacks=monitor.get_callbacks()
    )
    
    return wallet, monitor


def main():
    """Main transaction example function."""
    
    # Create temporary directory for wallet data
    temp_dir = tempfile.mkdtemp(prefix="tari_transaction_example_")
    print(f"Using temporary directory: {temp_dir}")
    
    try:
        # Create two test wallets for transaction testing
        print("Creating sender wallet...")
        sender_wallet, sender_monitor = create_test_wallet(temp_dir, "sender", 18188)
        
        print("Creating receiver wallet...")
        receiver_wallet, receiver_monitor = create_test_wallet(temp_dir, "receiver", 18189)
        
        print("Wallets created successfully!")
        
        # Check initial balances
        print("\n=== Initial Balances ===")
        try:
            sender_balance = sender_wallet.get_balance()
            print(f"Sender balance: {sender_balance.available} microTari available")
            
            receiver_balance = receiver_wallet.get_balance()
            print(f"Receiver balance: {receiver_balance.available} microTari available")
        except tari_wallet.TariWalletError as e:
            print(f"Error getting initial balances: {e}")
        
        # Demonstrate transaction creation (will likely fail in test environment)
        print("\n=== Transaction Example ===")
        
        # Example recipient address (this would be a real Base58 address in practice)
        recipient_address = "example_base58_address_would_go_here"
        amount = 1000000  # 1 Tari in microTari
        fee_per_gram = 5
        message = "Test transaction from Python bindings"
        
        print(f"Attempting to send {amount} microTari...")
        print(f"Recipient: {recipient_address}")
        print(f"Message: {message}")
        print(f"Fee per gram: {fee_per_gram}")
        
        try:
            tx_id = sender_wallet.send_transaction(
                dest_address=recipient_address,
                amount=amount,
                fee_per_gram=fee_per_gram,
                message=message,
                one_sided=False
            )
            print(f"Transaction created successfully! ID: {tx_id}")
            
            # Wait a bit for events
            print("Waiting for transaction events...")
            time.sleep(2)
            
        except tari_wallet.TariWalletError as e:
            print(f"Transaction failed (expected in test environment): {e}")
        
        # Get transaction history
        print("\n=== Transaction History ===")
        try:
            sender_transactions = sender_wallet.get_completed_transactions(limit=10)
            print(f"Sender has {len(sender_transactions)} completed transactions:")
            for i, tx_id in enumerate(sender_transactions):
                print(f"  {i+1}. Transaction ID: {tx_id}")
            
            receiver_transactions = receiver_wallet.get_completed_transactions(limit=10)
            print(f"Receiver has {len(receiver_transactions)} completed transactions:")
            for i, tx_id in enumerate(receiver_transactions):
                print(f"  {i+1}. Transaction ID: {tx_id}")
                
        except tari_wallet.TariWalletError as e:
            print(f"Error getting transaction history: {e}")
        
        # Display contacts
        print("\n=== Contacts ===")
        try:
            sender_contacts = sender_wallet.get_contacts()
            print(f"Sender has {len(sender_contacts)} contacts:")
            for alias, address in sender_contacts:
                print(f"  {alias}: {address[:32]}...")
            
            receiver_contacts = receiver_wallet.get_contacts()
            print(f"Receiver has {len(receiver_contacts)} contacts:")
            for alias, address in receiver_contacts:
                print(f"  {alias}: {address[:32]}...")
                
        except tari_wallet.TariWalletError as e:
            print(f"Error getting contacts: {e}")
        
        # Display event summaries
        print("\n=== Event Summary ===")
        with sender_monitor.lock:
            print(f"Sender events: {len(sender_monitor.transaction_events)} transaction events, "
                  f"{len(sender_monitor.received_transactions)} received, "
                  f"{len(sender_monitor.mined_transactions)} mined")
        
        with receiver_monitor.lock:
            print(f"Receiver events: {len(receiver_monitor.transaction_events)} transaction events, "
                  f"{len(receiver_monitor.received_transactions)} received, "
                  f"{len(receiver_monitor.mined_transactions)} mined")
        
        # Demonstrate message signing between wallets
        print("\n=== Message Signing Example ===")
        message_to_sign = "Cross-wallet message verification test"
        
        try:
            signature = sender_wallet.sign_message(message_to_sign)
            print(f"Sender signed message: '{message_to_sign}'")
            print(f"Signature: {signature}")
            
            # In a real scenario, you would get the sender's public key
            # and use it to verify the signature on the receiver side
            print("Note: Message verification requires the sender's public key")
            
        except tari_wallet.TariWalletError as e:
            print(f"Error signing message: {e}")
        
        print("\nTransaction example completed!")
        
    except tari_wallet.TariWalletError as e:
        print(f"Wallet error: {e}")
        return 1
    except Exception as e:
        print(f"Unexpected error: {e}")
        import traceback
        traceback.print_exc()
        return 1
    
    finally:
        # Cleanup
        try:
            import shutil
            shutil.rmtree(temp_dir)
            print(f"Cleaned up temporary directory: {temp_dir}")
        except Exception as e:
            print(f"Warning: Could not clean up temporary directory: {e}")
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
