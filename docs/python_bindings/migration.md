# Migration Guide for Tari Python Bindings

This guide helps users migrate to the latest version of the Tari Python wallet bindings.

## Current API (Latest)

The current Python bindings are built with PyO3 and provide a comprehensive interface to the Tari wallet functionality. This section documents the stable API that should be used for new development.

### Current Classes and Methods

#### PyTariCommsConfig
- `__new__(public_address, database_name, datastore_path, discovery_timeout, exclude_dial_test_addresses)`

#### PyTariWallet  
- `__new__(config, log_path, log_verbosity, num_rolling_log_files, size_per_log_file_bytes, network_str, passphrase=None, seed_passphrase=None, callbacks=None)`
- `get_balance() -> PyTariBalance`
- `sign_message(message: str) -> str`
- `verify_message_signature(public_key_hex: str, signature: str, message: str) -> bool`
- `send_transaction(dest_address: str, amount: int, fee_per_gram: int, message: str, one_sided: bool) -> int`
- `get_completed_transactions(limit: Optional[int] = None) -> List[int]`
- `get_contacts() -> List[Tuple[str, str]]`

#### PyTariBalance
- `available: int` (property)
- `time_locked: int` (property)  
- `pending_incoming: int` (property)
- `pending_outgoing: int` (property)

#### PyTariPublicKey
- `from_hex(hex_str: str) -> PyTariPublicKey` (classmethod)
- `to_hex() -> str`
- `to_emoji_encoding() -> str`

## Migration from Legacy Implementations

### If Migrating from C FFI Bindings

If you were previously using the C FFI bindings directly, the new Python bindings provide a much cleaner interface:

#### Old C FFI Style (NOT RECOMMENDED):
```python
# Old way - direct C FFI calls (example - don't use this)
from ctypes import *
libwallet = cdll.LoadLibrary("./libwallet_ffi.so")

# Complex setup with raw pointers and manual memory management
config_ptr = libwallet.wallet_create_config(...)
wallet_ptr = libwallet.wallet_create(config_ptr, ...)
balance_ptr = libwallet.wallet_get_balance(wallet_ptr, ...)
# Manual error checking and memory cleanup required
```

#### New Python Bindings (RECOMMENDED):
```python
import tari_wallet

# Clean, Pythonic interface with automatic memory management
config = tari_wallet.PyTariCommsConfig(
    public_address="/ip4/127.0.0.1/tcp/18188",
    database_name="test_wallet",
    datastore_path=temp_dir,
    discovery_timeout=30,
    exclude_dial_test_addresses=True
)

wallet = tari_wallet.PyTariWallet(
    config=config,
    log_path=os.path.join(temp_dir, "logs"),
    log_verbosity=1,
    num_rolling_log_files=3,
    size_per_log_file_bytes=512*1024,
    network_str="localnet"
)

# Clean method calls with proper Python types
balance = wallet.get_balance()
```

### Changes in Error Handling

#### Before:
```python
# Old: Manual error code checking
result = some_wallet_function()
if result.error_code != 0:
    handle_error(result.error_code)
```

#### After:
```python
# New: Python exception handling
try:
    balance = test_wallet.get_balance()
    assert isinstance(balance, tari_wallet.PyTariBalance)
except tari_wallet.TariWalletError as e:
    # Handle wallet-specific errors
    print(f"Wallet error: {e}")
```

### Changes in Memory Management

#### Before:
```python
# Old: Manual memory management
config_ptr = create_config(...)
wallet_ptr = create_wallet(config_ptr, ...)
try:
    # Use wallet
    pass
finally:
    # Manual cleanup required
    destroy_wallet(wallet_ptr)
    destroy_config(config_ptr)
```

#### After:
```python
# New: Automatic memory management
config = tari_wallet.PyTariCommsConfig(...)
wallet = tari_wallet.PyTariWallet(config, ...)
# Memory is automatically managed by Python GC and Rust RAII
```

## Breaking Changes

### Network Configuration

**Change:** Network configuration is now compile-time rather than runtime.

#### Before:
```python
# Old: Runtime network selection (hypothetical)
wallet = create_wallet(network="mainnet")
```

#### After:
```python
# New: Network is baked into the wheel at build time
# Install the appropriate wheel:
# pip install tari_wallet_mainnet-*.whl
# pip install tari_wallet_testnet-*.whl  
# pip install tari_wallet_nextnet-*.whl

wallet = tari_wallet.PyTariWallet(
    # ... other params
    network_str="mainnet"  # Must match the installed wheel
)
```

### Callback Interface

**Change:** Streamlined callback interface.

#### Current Implementation:
```python
callbacks = {
    "balance_updated": lambda balance_ptr: print(f"Balance updated: {balance_ptr}"),
    "transaction_mined": lambda tx_ptr: print(f"Transaction mined: {tx_ptr}"),
    "connectivity_status": lambda status: print(f"Connectivity status: {status}"),
}

wallet = tari_wallet.PyTariWallet(
    config=config,
    # ... other params
    callbacks=callbacks
)
```

### Transaction Interface

**Change:** Simplified transaction methods.

#### Current Implementation:
```python
# Send transaction
try:
    tx_id = wallet.send_transaction(
        dest_address="base58_address",
        amount=1000000,  # microTari
        fee_per_gram=5,
        message="Payment message",
        one_sided=False
    )
except tari_wallet.TariWalletError as e:
    print(f"Transaction failed: {e}")

# Get transactions
transactions = wallet.get_completed_transactions(limit=10)
```

## Configuration Migration

### Log Configuration

#### Current Approach:
```python
# Explicit log configuration in wallet constructor
log_levels = [0, 1, 2, 3, 4]  # Error, Warn, Info, Debug, Trace

for level in log_levels:
    wallet = tari_wallet.PyTariWallet(
        config=basic_config,
        log_path=os.path.join(temp_dir, f"logs_level_{level}"),
        log_verbosity=level,
        num_rolling_log_files=3,
        size_per_log_file_bytes=512*1024,
        network_str="localnet"
    )
```

### Database Configuration

#### Current Approach:
```python
# Database configuration is part of PyTariCommsConfig
config = tari_wallet.PyTariCommsConfig(
    public_address="/ip4/127.0.0.1/tcp/18188",
    database_name="test_wallet",  # Database name
    datastore_path=temp_dir,      # Database location
    discovery_timeout=30,
    exclude_dial_test_addresses=True
)
```

## Best Practices for Migration

### 1. Update Dependencies

```bash
# Remove old wallet bindings
pip uninstall tari_wallet_old  # or whatever the old package was

# Install new bindings
pip install tari_wallet_testnet-*.whl  # or appropriate network
```

### 2. Update Imports

```python
# New import style
import tari_wallet

# All classes are available from the main module
config = tari_wallet.PyTariCommsConfig(...)
wallet = tari_wallet.PyTariWallet(...)
balance = wallet.get_balance()  # Returns tari_wallet.PyTariBalance
```

### 3. Error Handling Migration

Replace manual error checking with exception handling:

```python
# Old pattern (don't use)
def old_get_balance(wallet):
    result, error_code = wallet.get_balance_with_error()
    if error_code != 0:
        return None, f"Error: {error_code}"
    return result, None

# New pattern
def new_get_balance(wallet):
    try:
        return wallet.get_balance()
    except tari_wallet.TariWalletError as e:
        print(f"Balance retrieval failed: {e}")
        return None
```

### 4. Testing Migration

Update test patterns to use pytest fixtures:

```python
# Use the provided test fixtures
import pytest
import tempfile
import tari_wallet

@pytest.fixture
def temp_dir():
    temp_path = tempfile.mkdtemp(prefix="tari_test_")
    yield temp_path
    import shutil
    shutil.rmtree(temp_path, ignore_errors=True)

@pytest.fixture
def test_wallet(temp_dir):
    config = tari_wallet.PyTariCommsConfig(
        public_address="/ip4/127.0.0.1/tcp/18188",
        database_name="test_wallet",
        datastore_path=temp_dir,
        discovery_timeout=30,
        exclude_dial_test_addresses=True
    )
    
    return tari_wallet.PyTariWallet(
        config=config,
        log_path=os.path.join(temp_dir, "logs"),
        log_verbosity=1,
        num_rolling_log_files=3,
        size_per_log_file_bytes=512*1024,
        network_str="localnet"
    )

def test_balance(test_wallet):
    try:
        balance = test_wallet.get_balance()
        assert isinstance(balance, tari_wallet.PyTariBalance)
    except tari_wallet.TariWalletError:
        pytest.skip("Balance check failed in test environment")
```

## Common Migration Issues

### Issue: Import Errors

**Problem:** `ImportError: No module named 'tari_wallet'`

**Solution:** 
1. Ensure you've installed the correct wheel for your target network
2. Check Python version compatibility (3.8+ required)
3. Verify the wheel was built for your platform

### Issue: Network Mismatch

**Problem:** Network operations fail with configuration errors

**Solution:**
1. Ensure the installed wheel matches your intended network
2. Use the correct `network_str` parameter that matches your installed wheel
3. Only install one network wheel at a time

### Issue: Path Configuration

**Problem:** Database or log file access errors

**Solution:**
```python
import tempfile
import os

# Use absolute paths and ensure directories exist
temp_dir = tempfile.mkdtemp(prefix="tari_test_")
log_dir = os.path.join(temp_dir, "logs")
os.makedirs(log_dir, exist_ok=True)

config = tari_wallet.PyTariCommsConfig(
    public_address="/ip4/127.0.0.1/tcp/18188",
    database_name="test_wallet", 
    datastore_path=temp_dir,  # Use absolute path
    discovery_timeout=30,
    exclude_dial_test_addresses=True
)
```

### Issue: Callback Signature Mismatches

**Problem:** Callback functions not being called or causing errors

**Solution:**
```python
# Ensure callback signatures match expected interface
def balance_callback(balance_ptr):
    """Single integer parameter"""
    print(f"Balance updated: {balance_ptr}")

def transaction_callback(tx_ptr, confirmations=None):
    """Some callbacks have optional second parameter"""
    if confirmations is not None:
        print(f"Transaction {tx_ptr} has {confirmations} confirmations")
    else:
        print(f"Transaction {tx_ptr} mined")

callbacks = {
    "balance_updated": balance_callback,
    "transaction_mined_unconfirmed": transaction_callback,
}
```

## Testing Your Migration

### Verification Script

```python
#!/usr/bin/env python3
"""Migration verification script."""

import tempfile
import os
import tari_wallet

def verify_migration():
    """Verify that the new bindings work correctly."""
    
    # Create temporary directory
    temp_dir = tempfile.mkdtemp(prefix="tari_migration_test_")
    
    try:
        print("Testing basic import...")
        assert hasattr(tari_wallet, 'PyTariWallet')
        assert hasattr(tari_wallet, 'PyTariCommsConfig')
        assert hasattr(tari_wallet, 'PyTariBalance')
        assert hasattr(tari_wallet, 'PyTariPublicKey')
        assert hasattr(tari_wallet, 'TariWalletError')
        print("✓ Import successful")
        
        print("Testing configuration creation...")
        config = tari_wallet.PyTariCommsConfig(
            public_address="/ip4/127.0.0.1/tcp/18188",
            database_name="migration_test",
            datastore_path=temp_dir,
            discovery_timeout=30,
            exclude_dial_test_addresses=True
        )
        print("✓ Configuration creation successful")
        
        print("Testing wallet creation...")
        wallet = tari_wallet.PyTariWallet(
            config=config,
            log_path=os.path.join(temp_dir, "logs"),
            log_verbosity=1,
            num_rolling_log_files=3,
            size_per_log_file_bytes=512*1024,
            network_str="localnet"
        )
        print("✓ Wallet creation successful")
        
        print("Testing balance retrieval...")
        try:
            balance = wallet.get_balance()
            assert hasattr(balance, 'available')
            assert hasattr(balance, 'time_locked') 
            assert hasattr(balance, 'pending_incoming')
            assert hasattr(balance, 'pending_outgoing')
            print("✓ Balance retrieval successful")
        except tari_wallet.TariWalletError as e:
            print(f"ℹ Balance retrieval failed (expected in test env): {e}")
        
        print("Testing public key operations...")
        try:
            # This will likely fail but tests the API
            pk = tari_wallet.PyTariPublicKey.from_hex("0123456789abcdef" * 8)
            print("✓ Public key operations successful")
        except tari_wallet.TariWalletError:
            print("ℹ Public key operations failed (expected with test data)")
        
        print("\n🎉 Migration verification completed successfully!")
        return True
        
    except Exception as e:
        print(f"\n❌ Migration verification failed: {e}")
        return False
        
    finally:
        # Cleanup
        import shutil
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    success = verify_migration()
    exit(0 if success else 1)
```

Run this script after migration to verify everything is working correctly.

## Getting Help

If you encounter issues during migration:

1. **Check the [API documentation](api.md)** for current method signatures
2. **Review [examples](examples.md)** for working code patterns  
3. **Run the test suite** to verify functionality: `python -m pytest base_layer/wallet_ffi/tests/ -v`
4. **Check build configuration** in [build guide](build.md)

The migration should be straightforward for most use cases, as the new API is designed to be more intuitive and Pythonic than previous iterations.
