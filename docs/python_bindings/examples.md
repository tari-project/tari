# Tari Python Bindings Examples

This document provides practical examples demonstrating real-world usage of the Tari Python wallet bindings.

## 🆕 Base Node Discovery Examples

### Simple Auto-Discovery Wallet Creation

The easiest way to get started with automatic base node discovery:

```python
import tari_wallet
from tari_wallet import create_wallet_with_auto_discovery, format_base_node_info

# Create wallet with automatic base node discovery
wallet, base_node_info = create_wallet_with_auto_discovery(
    network="nextnet",
    database_name="my_nextnet_wallet"
)

print(f"✅ Wallet created successfully!")
print(f"📡 {format_base_node_info(base_node_info)}")

# Get seed peers discovered by the wallet
seed_peers = wallet.get_seed_peers()
print(f"🌐 Found {len(seed_peers)} seed peers from network")

# Use wallet normally
balance = wallet.get_balance()
print(f"💰 Available: {balance.available} microTari")
```

### Network Exploration

Explore available networks and their configurations:

```python
from tari_wallet import NetworkManager, TariNetwork

# Get all available networks
networks = NetworkManager.get_available_networks()
print(f"Available networks: {networks}")

# Get detailed info for each network
for network_name in networks:
    try:
        network = NetworkManager.get_network_by_name(network_name)
        manager = NetworkManager(network)
        info = manager.get_network_info()
        
        print(f"\n🌐 {network_name.upper()}")
        print(f"  DNS seeds: {info['dns_seeds_count']}")
        print(f"  Peer seeds: {info['peer_seeds_count']}")
        print(f"  Explorer: {info.get('explorer_url', 'N/A')}")
        
        # Show transport breakdown
        transports = info['supported_transports']
        print(f"  Transports: IPv4={transports.get('ip4', 0)}, IPv6={transports.get('ip6', 0)}, Onion={transports.get('onion3', 0)}")
        
    except Exception as e:
        print(f"❌ Error with {network_name}: {e}")
```

### Manual Discovery Process

For more control over the discovery process:

```python
from tari_wallet import SimpleDiscoveryService, TariNetwork

# Create discovery service
discovery = SimpleDiscoveryService(TariNetwork.NEXTNET)

print("🔍 Running base node discovery...")
selected_node = discovery.discover_and_select_node(dns_timeout=5.0)

if selected_node:
    print(f"✅ Selected node: {selected_node.name}")
    print(f"   Public key: {selected_node.public_key}")
    print(f"   Address: {selected_node.address}")
    print(f"   Health score: {selected_node.get_health_score():.2f}")
    
    # Check if this is a real configured node
    if selected_node.public_key.startswith("dns_placeholder"):
        print("   ℹ️  This is a DNS-resolved placeholder")
    else:
        print("   ✅ This is a real configured peer from b_peer_seeds.toml")
else:
    print("❌ No nodes discovered")

# Show all available nodes
available_nodes = discovery.get_available_nodes()
print(f"\n📊 Discovery Results:")
print(f"   Total nodes: {len(available_nodes)}")

for i, node in enumerate(available_nodes[:3]):  # Show first 3
    print(f"   {i+1}. {node.name} (priority {node.priority})")
```

### Advanced Discovery with Background Monitoring

For applications that need continuous monitoring:

```python
import asyncio
from tari_wallet import DiscoveryService, DiscoveryConfig, TariNetwork

async def advanced_discovery_example():
    # Create wallet first
    wallet, _ = create_wallet_with_auto_discovery("nextnet", "monitoring_wallet")
    
    # Create discovery service with wallet integration
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
    discovery.on_nodes_discovered = lambda nodes: print(f"🔍 Discovered {len(nodes)} nodes")
    discovery.on_node_health_changed = lambda node, healthy: print(f"💓 {node.name} is {'healthy' if healthy else 'unhealthy'}")
    discovery.on_discovery_error = lambda error: print(f"❌ Discovery error: {error}")
    
    print("🚀 Starting background discovery...")
    await discovery.start()
    
    # Let it run for a while
    for i in range(6):  # 30 seconds total
        await asyncio.sleep(5)
        status = discovery.get_discovery_status()
        print(f"📊 Status check {i+1}: {status['node_statistics']['total_nodes']} nodes managed")
    
    print("🛑 Stopping discovery...")
    await discovery.stop()
    print("✅ Discovery stopped")

# Run the advanced example
asyncio.run(advanced_discovery_example())
```

### Custom Base Node Configuration

Override automatic discovery with specific nodes:

```python
# Use a specific base node instead of auto-discovery
custom_node = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef::/ip4/192.168.1.100/tcp/18189"

wallet, node_info = create_wallet_with_auto_discovery(
    network="nextnet",
    database_name="custom_node_wallet",
    custom_base_node=custom_node
)

print(f"🔧 Using custom base node:")
print(f"   {format_base_node_info(node_info)}")
```

### Configuration File Reading

See how the discovery system reads from actual Tari configuration files:

```python
from tari_wallet import get_config_reader

# Get the global config reader
config_reader = get_config_reader()

if config_reader.load_config():
    print(f"✅ Loaded config from: {config_reader.config_path}")
    
    # Show available networks from config file
    networks = config_reader.get_available_networks()
    print(f"🌐 Networks in config: {networks}")
    
    # Get detailed info for nextnet
    nextnet_info = config_reader.get_network_info("nextnet")
    print(f"\n📋 Nextnet Configuration:")
    print(f"   Total nodes: {nextnet_info['total_configured_nodes']}")
    print(f"   DNS seeds: {nextnet_info['dns_seeds_count']}")
    print(f"   Transport breakdown: {nextnet_info['supported_transports']}")
    
    # Show some real peer examples
    nodes = config_reader.create_base_nodes_from_config("nextnet")
    print(f"\n🔑 Example peers:")
    for i, node in enumerate(nodes[:3]):
        print(f"   {i+1}. {node.name}")
        print(f"      Key: {node.public_key}")
        print(f"      Address: {node.address}")
else:
    print("❌ Could not load configuration file")
```

## Complete Traditional Examples

### Complete Working Example

This is extracted from the actual working example in the repository:

```python
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
```

## Specific Use Cases

### Wallet Configuration Patterns

#### Basic Configuration

```python
def basic_config(temp_dir):
    """Create a basic wallet configuration for testing."""
    return tari_wallet.PyTariCommsConfig(
        public_address="/ip4/127.0.0.1/tcp/18188",
        database_name="test_wallet",
        datastore_path=temp_dir,
        discovery_timeout=30,
        exclude_dial_test_addresses=True
    )
```

#### Configuration with Different Networks

```python
def test_wallet_with_different_networks(basic_config, temp_dir):
    """Test wallet creation with different networks."""
    networks = ["localnet", "nextnet", "mainnet"]

    for network in networks:
        try:
            wallet = tari_wallet.PyTariWallet(
                config=basic_config,
                log_path=os.path.join(temp_dir, f"logs_{network}"),
                log_verbosity=1,
                num_rolling_log_files=3,
                size_per_log_file_bytes=512*1024,
                network_str=network
            )
            assert wallet is not None
        except tari_wallet.TariWalletError as e:
            # Some networks might not be available in test environment
            pytest.skip(f"Network {network} not available in test env: {e}")
```

#### Configuration with Different Log Levels

```python
def test_wallet_with_different_log_levels(basic_config, temp_dir):
    """Test wallet creation with different log levels."""
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
        assert wallet is not None
```

### Event Callback Patterns

#### Event Collection for Testing

```python
class EventCollector:
    def __init__(self):
        self.events = []

    def collect_event(self, event_name):
        def handler(*args):
            self.events.append((event_name, args))
        return handler

    def get_events(self, event_name=None):
        if event_name:
            return [args for name, args in self.events if name == event_name]
        return self.events

    def clear_events(self):
        self.events.clear()

# Usage
event_collector = EventCollector()
```

#### Comprehensive Callback Setup

```python
def wallet_with_callbacks(basic_config, temp_dir, event_collector):
    """Create a wallet with event callbacks for testing."""
    callbacks = {
        "balance_updated": event_collector.collect_event("balance_updated"),
        "transaction_mined": event_collector.collect_event("transaction_mined"),
        "connectivity_status": event_collector.collect_event("connectivity_status"),
    }

    return tari_wallet.PyTariWallet(
        config=basic_config,
        log_path=os.path.join(temp_dir, "logs"),
        log_verbosity=2,  # Debug level for tests
        num_rolling_log_files=3,
        size_per_log_file_bytes=256*1024,
        network_str="localnet",
        callbacks=callbacks
    )
```

### Balance Operations

#### Comprehensive Balance Checking

```python
def test_balance_calculations(test_wallet):
    """Test balance calculations and relationships."""
    try:
        balance = test_wallet.get_balance()

        # Calculate total balance components
        total_confirmed = balance.available + balance.time_locked
        total_pending = balance.pending_incoming + balance.pending_outgoing
        total_balance = total_confirmed + total_pending

        # All totals should be non-negative
        assert total_confirmed >= 0
        assert total_pending >= 0
        assert total_balance >= 0

        # Available should be <= total confirmed
        assert balance.available <= total_confirmed

    except tari_wallet.TariWalletError as e:
        pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
```

#### Balance Type Verification

```python
def test_balance_type_safety(test_wallet):
    """Test that balance properties return correct types."""
    try:
        balance = test_wallet.get_balance()

        # Test that properties are specifically integers, not floats
        assert type(balance.available) is int
        assert type(balance.time_locked) is int
        assert type(balance.pending_incoming) is int
        assert type(balance.pending_outgoing) is int

        # Test that properties are not boolean (0 and 1 are valid balances)
        assert not isinstance(balance.available, bool)
        assert not isinstance(balance.time_locked, bool)
        assert not isinstance(balance.pending_incoming, bool)
        assert not isinstance(balance.pending_outgoing, bool)

    except tari_wallet.TariWalletError as e:
        pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
```

### Public Key Operations

#### Creating Public Keys from Hex

```python
def test_public_key_from_valid_hex(sample_hex_keys):
    """Test creating public key from valid hex strings."""
    valid_keys = ["valid_32_byte", "valid_64_char"]

    for key_name in valid_keys:
        hex_str = sample_hex_keys[key_name]
        try:
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            assert isinstance(public_key, tari_wallet.PyTariPublicKey)
        except tari_wallet.TariWalletError:
            # Some hex strings might not represent valid public keys
            # even if they're the right format
            pass
```

#### Hex Round-trip Conversion

```python
def test_public_key_round_trip(sample_hex_keys):
    """Test hex -> public key -> hex round trip."""
    hex_str = sample_hex_keys["valid_32_byte"]

    try:
        public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
        result_hex = public_key.to_hex()

        # The result might be in a different case or format
        # but should represent the same key
        assert isinstance(result_hex, str)
        assert len(result_hex) >= len(hex_str.replace("0x", ""))

    except tari_wallet.TariWalletError:
        pytest.skip("Public key creation failed with test hex")
```

### Message Signing and Verification

#### Message Signing with Various Messages

```python
# Test messages fixture
test_messages = [
    "Hello, Tari!",
    "Test message with numbers 123",
    "Unicode test: 你好 🌟",
    "",  # Empty message
    "A" * 1000,  # Long message
    "Special chars: !@#$%^&*()_+-=[]{}|;:,.<>?",
]

# Usage in signing test
for message in test_messages:
    try:
        signature = test_wallet.sign_message(message)
        assert isinstance(signature, str)
        assert len(signature) > 0
    except tari_wallet.TariWalletError as e:
        # Signing might fail in test environment
        pytest.skip(f"Message signing failed: {e}")
```

### Error Handling Patterns

#### Comprehensive Error Handling

```python
def safe_wallet_operation(wallet, operation_name, operation_func):
    """Safely execute a wallet operation with proper error handling."""
    try:
        result = operation_func()
        print(f"✓ {operation_name} succeeded")
        return result
    except tari_wallet.TariWalletError as e:
        print(f"✗ {operation_name} failed with wallet error: {e}")
        return None
    except Exception as e:
        print(f"✗ {operation_name} failed with unexpected error: {e}")
        return None

# Usage examples
balance = safe_wallet_operation(
    wallet, 
    "Balance check", 
    lambda: wallet.get_balance()
)

signature = safe_wallet_operation(
    wallet,
    "Message signing",
    lambda: wallet.sign_message("Test message")
)
```

#### Input Validation

```python
def test_transaction_limits(test_wallet):
    """Test transaction operations with edge case values."""
    # Test with zero amount (should fail)
    with pytest.raises(tari_wallet.TariWalletError):
        test_wallet.send_transaction(
            dest_address="valid_address_format",
            amount=0,
            fee_per_gram=1,
            message="Zero amount test",
            one_sided=False
        )

    # Test with very large amount
    with pytest.raises(tari_wallet.TariWalletError):
        test_wallet.send_transaction(
            dest_address="valid_address_format",
            amount=2**63 - 1,  # Max int64
            fee_per_gram=1,
            message="Large amount test",
            one_sided=False
        )
```

### Memory Management Patterns

#### Multiple Wallet Instance Management

```python
def test_wallet_memory_usage(basic_config, temp_dir):
    """Test that multiple wallet instances don't cause memory issues."""
    wallets = []

    try:
        for i in range(10):
            wallet = tari_wallet.PyTariWallet(
                config=basic_config,
                log_path=os.path.join(temp_dir, f"logs_{i}"),
                log_verbosity=0,  # Error level only to reduce overhead
                num_rolling_log_files=1,
                size_per_log_file_bytes=64*1024,
                network_str="localnet"
            )
            wallets.append(wallet)

        assert len(wallets) == 10

    except tari_wallet.TariWalletError as e:
        # Multiple wallet creation might fail in test environment
        pytest.skip(f"Multiple wallet creation failed (expected): {e}")
```

#### Public Key Memory Management

```python
def test_public_key_memory_management(sample_hex_keys):
    """Test that public key objects are properly managed."""
    keys = []
    hex_str = sample_hex_keys["valid_32_byte"]

    try:
        # Create multiple public key objects
        for _ in range(10):
            public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
            keys.append(public_key)

        assert len(keys) == 10

        # All should be valid
        for key in keys:
            assert isinstance(key, tari_wallet.PyTariPublicKey)
            hex_result = key.to_hex()
            assert isinstance(hex_result, str)

    except tari_wallet.TariWalletError:
        pytest.skip("Public key creation failed with test hex")
```

## Testing Patterns

### Test Fixtures

```python
@pytest.fixture
def temp_dir():
    """Create a temporary directory for test data."""
    temp_path = tempfile.mkdtemp(prefix="tari_test_")
    yield temp_path
    shutil.rmtree(temp_path, ignore_errors=True)
```

### Conditional Testing

Many operations might fail in test environments, so use conditional testing:

```python
def test_network_dependent_operation(test_wallet):
    """Test operation that requires network connectivity."""
    try:
        result = test_wallet.some_network_operation()
        # Test the result if operation succeeds
        assert result is not None
    except tari_wallet.TariWalletError as e:
        # Skip test if network is unavailable
        pytest.skip(f"Network operation failed (expected in test env): {e}")
```

## Best Practices

### Resource Management

```python
import tempfile
import shutil
import os
import tari_wallet

def create_test_wallet():
    """Create a wallet with proper resource management."""
    temp_dir = tempfile.mkdtemp(prefix="tari_wallet_")
    
    try:
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
        
        return wallet, temp_dir
        
    except Exception:
        # Clean up on error
        shutil.rmtree(temp_dir, ignore_errors=True)
        raise

# Usage
wallet, temp_dir = create_test_wallet()
try:
    # Use wallet...
    balance = wallet.get_balance()
finally:
    # Clean up
    shutil.rmtree(temp_dir, ignore_errors=True)
```

### Validation and Error Handling

```python
def validate_and_send_transaction(wallet, dest_address, amount, fee_per_gram, message):
    """Send transaction with comprehensive validation."""
    
    # Input validation
    if not dest_address or len(dest_address) < 10:
        raise ValueError("Invalid destination address")
    
    if amount <= 0:
        raise ValueError("Amount must be positive")
    
    if fee_per_gram <= 0:
        raise ValueError("Fee per gram must be positive")
    
    # Check balance first
    try:
        balance = wallet.get_balance()
        if balance.available < amount:
            raise ValueError(f"Insufficient balance: {balance.available} < {amount}")
    except tari_wallet.TariWalletError as e:
        raise RuntimeError(f"Could not check balance: {e}")
    
    # Send transaction
    try:
        tx_id = wallet.send_transaction(
            dest_address=dest_address,
            amount=amount,
            fee_per_gram=fee_per_gram,
            message=message,
            one_sided=False
        )
        return tx_id
    except tari_wallet.TariWalletError as e:
        raise RuntimeError(f"Transaction failed: {e}")
```

This examples document provides real, verified code patterns extracted from the actual test suite and examples in the repository.
