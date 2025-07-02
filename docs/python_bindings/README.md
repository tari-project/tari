# Tari Python Wallet Bindings

Python bindings for the Tari cryptocurrency wallet using PyO3. This package allows Python applications to interact with Tari wallets for creating transactions, managing contacts, and performing cryptographic operations.

## Installation

Build and install the package using the provided build script:

```bash
# Navigate to the wallet FFI directory
cd base_layer/wallet_ffi

# Build Python wheels for all networks
./python_build.sh

# Install the appropriate wheel for your target network
pip install target/wheels/tari_wallet_testnet-*.whl  # For testnet (default)
pip install target/wheels/tari_wallet_mainnet-*.whl  # For mainnet  
pip install target/wheels/tari_wallet_nextnet-*.whl  # For nextnet
```

**Important:** Only install one network version at a time to avoid conflicts.

## Quick Start

```python
import tari_wallet
import tempfile
import os

# Create a temporary directory for wallet data
temp_dir = tempfile.mkdtemp(prefix="tari_wallet_example_")

# Create wallet configuration
config = tari_wallet.PyTariCommsConfig(
    public_address="/ip4/127.0.0.1/tcp/18188",
    database_name="example_wallet",
    datastore_path=temp_dir,
    discovery_timeout=60,
    exclude_dial_test_addresses=True
)

# Define event callbacks (optional)
def on_balance_updated(balance_ptr):
    print(f"[Event] Balance updated: {balance_ptr}")

def on_transaction_mined(tx_ptr):
    print(f"[Event] Transaction mined: {tx_ptr}")

callbacks = {
    "balance_updated": on_balance_updated,
    "transaction_mined": on_transaction_mined,
}

# Create wallet
wallet = tari_wallet.PyTariWallet(
    config=config,
    log_path=os.path.join(temp_dir, "logs"),
    log_verbosity=1,  # Info level
    num_rolling_log_files=5,
    size_per_log_file_bytes=1024*1024,  # 1MB per log file
    network_str="localnet",
    callbacks=callbacks
)
```

## Basic Operations

### Check Balance

```python
try:
    balance = wallet.get_balance()
    print(f"Available balance: {balance.available} microTari")
    print(f"Time-locked balance: {balance.time_locked} microTari")
    print(f"Pending incoming: {balance.pending_incoming} microTari")
    print(f"Pending outgoing: {balance.pending_outgoing} microTari")
except tari_wallet.TariWalletError as e:
    print(f"Error getting balance: {e}")
```

### Sign Messages

```python
message = "Hello from Tari Python bindings!"

try:
    signature = wallet.sign_message(message)
    print(f"Message: {message}")
    print(f"Signature: {signature}")
    print(f"Message signed successfully!")
except tari_wallet.TariWalletError as e:
    print(f"Error signing message: {e}")
```

### Get Transactions

```python
try:
    transactions = wallet.get_completed_transactions(limit=5)
    print(f"Found {len(transactions)} completed transactions")
    for i, tx_id in enumerate(transactions):
        print(f"  Transaction {i+1}: ID {tx_id}")
except tari_wallet.TariWalletError as e:
    print(f"Error getting transactions: {e}")
```

### Get Contacts

```python
try:
    contacts = wallet.get_contacts()
    print(f"Found {len(contacts)} contacts")
    for alias, address in contacts:
        print(f"  Contact: {alias} -> {address[:16]}...")
except tari_wallet.TariWalletError as e:
    print(f"Error getting contacts: {e}")
```

## Error Handling

All wallet operations can raise `TariWalletError` exceptions. Always wrap operations in try-catch blocks:

```python
try:
    balance = test_wallet.get_balance()
    assert isinstance(balance, tari_wallet.PyTariBalance)
except tari_wallet.TariWalletError as e:
    # Balance retrieval might fail in test environment
    print(f"Balance retrieval failed: {e}")
```

## Testing

The package includes comprehensive tests that demonstrate real usage patterns:

```bash
cd base_layer/wallet_ffi
python -m pytest tests/ -v
```

Test files include:
- [`test_wallet.py`](../../base_layer/wallet_ffi/tests/test_wallet.py) - Main wallet functionality
- [`test_balance.py`](../../base_layer/wallet_ffi/tests/test_balance.py) - Balance operations 
- [`test_config.py`](../../base_layer/wallet_ffi/tests/test_config.py) - Configuration testing
- [`test_public_key.py`](../../base_layer/wallet_ffi/tests/test_public_key.py) - Public key operations

## Network Configuration

The wallet supports different networks compiled at build time:

- `"localnet"`: Local development network
- `"nextnet"`: Tari test network for upcoming features
- `"mainnet"`: Tari main network

## Thread Safety

- The Python bindings are **not thread-safe**
- Use wallet instances from a single thread only
- Callback functions are called from the wallet's internal thread context

## Memory Management

- All objects are automatically garbage collected
- No manual cleanup required
- The underlying Rust code uses RAII patterns for resource management

## Requirements

- Python 3.8+
- Compatible with Linux, macOS, and Windows
- Requires network connectivity for wallet operations
- Rust toolchain (for building from source)

## Additional Documentation

- [Build Instructions](build.md) - Detailed build and installation guide
- [API Reference](api.md) - Complete API documentation
- [Examples](examples.md) - Additional usage examples
- [Migration Guide](migration.md) - Guide for migrating from older versions
