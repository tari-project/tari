# Tari Python Wallet Bindings

Python bindings for the Tari cryptocurrency wallet using PyO3. This package allows Python applications to interact with Tari wallets for creating transactions, managing contacts, performing cryptographic operations, and **automatically discovering base nodes**.

## 🆕 New: Automatic Base Node Discovery

The Python bindings now include a comprehensive base node discovery system that automatically finds and manages connections to Tari network nodes. This eliminates the need for manual base node configuration and provides the same discovery capabilities as mobile wallets.

### Key Features

- **🔍 Automatic Discovery**: Combines DNS seed resolution, FFI seed peers, and hardcoded peers
- **🌐 Real Configuration**: Reads from actual `b_peer_seeds.toml` configuration files used by Tari core
- **💪 Health Monitoring**: Tracks node health with automatic failover and rotation
- **📶 Multi-Network Support**: Full support for localnet, nextnet, stagenet, mainnet, esmeralda, and igor
- **🔄 Selection Strategies**: Round-robin, random, and priority-based node selection
- **🏗️ High-Level API**: Simple functions for common use cases

## Installation

Build and install the package using the provided build script:

```bash
# Navigate to the wallet FFI directory
cd base_layer/wallet_ffi

# Build Python wheels (includes discovery framework)
./python_build.sh

# Install the appropriate wheel for your target network
pip install target/wheels/tari_wallet_testnet-*.whl  # For testnet (default)
pip install target/wheels/tari_wallet_mainnet-*.whl  # For mainnet  
pip install target/wheels/tari_wallet_nextnet-*.whl  # For nextnet
```

**Important:** Only install one network version at a time to avoid conflicts.

## Quick Start

### Simple Wallet Creation with Auto-Discovery

The easiest way to create a wallet is using the new auto-discovery functions:

```python
import tari_wallet
from tari_wallet import create_wallet_with_auto_discovery, format_base_node_info

# Create wallet with automatic base node discovery
wallet, base_node_info = create_wallet_with_auto_discovery(
    network="nextnet",
    database_name="my_wallet"
)

print(f"✅ Wallet created!")
print(f"📡 Discovery result: {format_base_node_info(base_node_info)}")

# Get discovered seed peers
seed_peers = wallet.get_seed_peers()
print(f"🌐 Found {len(seed_peers)} seed peers from network")

# Use wallet normally
balance = wallet.get_balance()
print(f"💰 Available: {balance.available} microTari")
```

### Network-Specific Convenience Functions

```python
from tari_wallet import create_nextnet_wallet, create_mainnet_wallet, create_localnet_wallet

# Quick network-specific wallet creation
wallet, node_info = create_nextnet_wallet("my_nextnet_wallet")
wallet, node_info = create_mainnet_wallet("my_mainnet_wallet") 
wallet, node_info = create_localnet_wallet("my_local_wallet")
```

### Manual Discovery and Management

For more control over the discovery process:

```python
from tari_wallet import SimpleDiscoveryService, TariNetwork, NetworkManager

# Explore available networks
available_networks = NetworkManager.get_available_networks()
print(f"Available networks: {available_networks}")

# Manual discovery
discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)
selected_node = discovery.discover_and_select_node()

if selected_node:
    print(f"Selected: {selected_node.name}")
    print(f"Public key: {selected_node.public_key}")
    print(f"Address: {selected_node.address}")
    
# Get all available nodes
available_nodes = discovery.get_available_nodes()
print(f"Total nodes available: {len(available_nodes)}")
```

### Traditional Wallet Creation (Legacy)

You can still create wallets manually if needed:

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

### Get Seed Peers (New)

The wallet can now retrieve seed peers from the network for base node discovery:

```python
try:
    seed_peers = wallet.get_seed_peers()
    print(f"Found {len(seed_peers)} seed peers:")
    for i, peer_key in enumerate(seed_peers[:5]):  # Show first 5
        print(f"  {i+1}. {peer_key}")
except tari_wallet.TariWalletError as e:
    print(f"Error getting seed peers: {e}")
```

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

## Base Node Discovery Framework

The Python bindings include a comprehensive discovery framework for finding and managing base nodes:

### Discovery Components

- **TariConfigReader**: Reads real configuration from `b_peer_seeds.toml`
- **NetworkManager**: Manages network-specific configurations and DNS resolution
- **BaseNodeManager**: Handles node selection, health tracking, and failover
- **DiscoveryService**: Provides automatic background discovery and monitoring
- **SimpleDiscoveryService**: Immediate discovery for simple use cases

### Configuration Sources

The discovery system combines multiple sources for maximum reliability:

1. **Real Peer Seeds**: Read from actual `common/config/presets/b_peer_seeds.toml`
2. **DNS Seeds**: Resolve network-specific DNS names (e.g., `seeds.nextnet.tari.com`)
3. **FFI Seed Peers**: Get peers directly from wallet's internal peer manager
4. **Hardcoded Fallbacks**: Localnet and emergency fallback nodes

### Advanced Discovery Usage

```python
import asyncio
from tari_wallet import DiscoveryService, DiscoveryConfig, TariNetwork

# Create advanced discovery service
config = DiscoveryConfig(
    discovery_interval=300.0,  # 5 minutes
    health_check_interval=60.0,  # 1 minute
    enable_dns_discovery=True,
    enable_ffi_discovery=True
)

discovery = DiscoveryService(
    network=TariNetwork.NEXTNET,
    config=config,
    wallet_get_seed_peers_fn=lambda: wallet.get_seed_peers()
)

# Set up event callbacks
discovery.on_nodes_discovered = lambda nodes: print(f"Discovered {len(nodes)} nodes")
discovery.on_node_health_changed = lambda node, healthy: print(f"Node {node.name} is {'healthy' if healthy else 'unhealthy'}")

# Start background discovery
async def run_discovery():
    await discovery.start()
    
    # Let it run and monitor
    await asyncio.sleep(10)
    
    # Check status
    status = discovery.get_discovery_status()
    print(f"Running: {status['is_running']}")
    print(f"Total nodes: {status['node_statistics']['total_nodes']}")
    
    await discovery.stop()

# Run the discovery service
asyncio.run(run_discovery())
```

### Custom Base Nodes

You can specify custom base nodes instead of using auto-discovery:

```python
# Use a specific base node
custom_node = "pubkey::/ip4/192.168.1.100/tcp/18189"

wallet, node_info = create_wallet_with_auto_discovery(
    network="nextnet",
    custom_base_node=custom_node
)
```

### Network Information

Get detailed information about available networks:

```python
from tari_wallet import NetworkManager

# Get info about all networks
all_networks = NetworkManager.get_all_network_info()
for network_name, info in all_networks.items():
    print(f"{network_name}: {info['peer_seeds_count']} peers, {info['dns_seeds_count']} DNS seeds")

# Get specific network details
manager = NetworkManager(TariNetwork.NEXTNET)
info = manager.get_network_info()
print(f"Nextnet: {info['supported_transports']}")  # Shows IPv4/IPv6/Onion breakdown
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

## Examples

The package includes several example scripts demonstrating different features:

- [`basic_wallet.py`](../../base_layer/wallet_ffi/examples/basic_wallet.py) - Basic wallet operations
- [`base_node_discovery_example.py`](../../base_layer/wallet_ffi/examples/base_node_discovery_example.py) - Complete discovery system demo
- [`config_reader_example.py`](../../base_layer/wallet_ffi/examples/config_reader_example.py) - Configuration file reading

Run examples:
```bash
cd base_layer/wallet_ffi
python examples/base_node_discovery_example.py
python examples/config_reader_example.py
```

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
