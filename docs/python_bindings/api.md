# Tari Python Bindings API Reference

Complete API documentation for the Tari Python wallet bindings.

## Module: tari_wallet

The main module providing Python access to Tari wallet functionality.

### Classes

#### PyTariCommsConfig

Configuration class for wallet communication settings.

**Constructor**
```python
PyTariCommsConfig(
    public_address: str,
    database_name: str, 
    datastore_path: str,
    discovery_timeout: int,
    exclude_dial_test_addresses: bool
)
```

**Parameters:**
- `public_address` (str): The network address for the wallet to bind to (e.g., "/ip4/127.0.0.1/tcp/18188")
- `database_name` (str): Name of the database file for wallet storage
- `datastore_path` (str): File system path where wallet data will be stored
- `discovery_timeout` (int): Timeout in seconds for peer discovery operations
- `exclude_dial_test_addresses` (bool): Whether to exclude test network addresses from dialing

**Example:**
```python
config = tari_wallet.PyTariCommsConfig(
    public_address="/ip4/127.0.0.1/tcp/18188",
    database_name="test_wallet",
    datastore_path=temp_dir,
    discovery_timeout=30,
    exclude_dial_test_addresses=True
)
```

---

#### PyTariWallet

Main wallet class providing transaction and cryptographic operations.

**Constructor**
```python
PyTariWallet(
    config: PyTariCommsConfig,
    log_path: str,
    log_verbosity: int,
    num_rolling_log_files: int,
    size_per_log_file_bytes: int,
    network_str: str,
    passphrase: Optional[str] = None,
    seed_passphrase: Optional[str] = None,
    callbacks: Optional[dict] = None
)
```

**Parameters:**
- `config` (PyTariCommsConfig): Communication configuration object
- `log_path` (str): Directory path for wallet log files
- `log_verbosity` (int): Logging level (0=Error, 1=Warn, 2=Info, 3=Debug, 4=Trace)
- `num_rolling_log_files` (int): Number of rotating log files to maintain
- `size_per_log_file_bytes` (int): Maximum size of each log file in bytes
- `network_str` (str): Network identifier ("localnet", "nextnet", or "mainnet")
- `passphrase` (Optional[str]): Optional wallet encryption passphrase
- `seed_passphrase` (Optional[str]): Optional seed phrase passphrase
- `callbacks` (Optional[dict]): Optional dictionary of event callback functions

**Methods:**

##### get_balance() -> PyTariBalance

Returns the current wallet balance information.

**Returns:** PyTariBalance object containing balance details

**Raises:** TariWalletError on wallet operation failure

**Example:**
```python
try:
    balance = test_wallet.get_balance()
    assert isinstance(balance, tari_wallet.PyTariBalance)
except tari_wallet.TariWalletError as e:
    # Balance retrieval might fail in test environment
    pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
```

##### sign_message(message: str) -> str

Signs a message using the wallet's private key.

**Parameters:**
- `message` (str): The message to sign

**Returns:** String containing the signature

**Raises:** TariWalletError on signing failure

**Example:**
```python
try:
    signature = test_wallet.sign_message(message)
    assert isinstance(signature, str)
    assert len(signature) > 0
    # Note: Signatures might not be deterministic due to randomness
except tari_wallet.TariWalletError as e:
    # Signing might fail in test environment
    pytest.skip(f"Message signing failed (expected in test env): {e}")
```

##### verify_message_signature(public_key_hex: str, signature: str, message: str) -> bool

Verifies a message signature against a public key.

**Parameters:**
- `public_key_hex` (str): Hex-encoded public key
- `signature` (str): The signature to verify
- `message` (str): The original message

**Returns:** Boolean indicating if the signature is valid

**Raises:** TariWalletError on verification failure

**Example:**
```python
message = "Test message for verification"

# Test with a sample public key (will likely fail but tests the API)
try:
    is_valid = test_wallet.verify_message_signature(
        sample_hex_keys["valid_32_byte"],
        "sample_signature",
        message
    )
    assert isinstance(is_valid, bool)
except tari_wallet.TariWalletError:
    # Expected to fail with invalid signature/key
    pass
```

##### send_transaction(dest_address: str, amount: int, fee_per_gram: int, message: str, one_sided: bool) -> int

Sends a transaction to the specified address.

**Parameters:**
- `dest_address` (str): Base58-encoded destination address
- `amount` (int): Amount to send in microTari
- `fee_per_gram` (int): Fee per gram for transaction processing
- `message` (str): Message to include with the transaction
- `one_sided` (bool): Whether to send as a one-sided transaction

**Returns:** Transaction ID as integer

**Raises:** TariWalletError on transaction failure

**Example:**
```python
with pytest.raises(tari_wallet.TariWalletError):
    test_wallet.send_transaction(
        dest_address="invalid_address",
        amount=1000,
        fee_per_gram=5,
        message="Test transaction",
        one_sided=False
    )
```

##### get_completed_transactions(limit: Optional[int] = None) -> List[int]

Retrieves completed transaction IDs.

**Parameters:**
- `limit` (Optional[int]): Maximum number of transactions to return (default: 100)

**Returns:** List of transaction IDs

**Raises:** TariWalletError on retrieval failure

**Example:**
```python
try:
    transactions = test_wallet.get_completed_transactions()
    assert isinstance(transactions, list)
    
    # Test with limit
    transactions_limited = test_wallet.get_completed_transactions(limit=5)
    assert isinstance(transactions_limited, list)
    assert len(transactions_limited) <= 5
    
except tari_wallet.TariWalletError as e:
    # Transaction retrieval might fail in test environment
    pytest.skip(f"Transaction retrieval failed (expected in test env): {e}")
```

##### get_contacts() -> List[Tuple[str, str]]

Retrieves wallet contacts.

**Returns:** List of tuples containing (alias, address_hex) pairs

**Raises:** TariWalletError on retrieval failure

**Example:**
```python
try:
    contacts = test_wallet.get_contacts()
    assert isinstance(contacts, list)
    
    for contact in contacts:
        assert isinstance(contact, tuple)
        assert len(contact) == 2
        alias, address = contact
        assert isinstance(alias, str)
        assert isinstance(address, str)

except tari_wallet.TariWalletError as e:
    # Contacts retrieval might fail in test environment
    pytest.skip(f"Contacts retrieval failed (expected in test env): {e}")
```

---

#### PyTariBalance

Balance information container.

**Properties:**
- `available` (int): Available balance in microTari
- `time_locked` (int): Time-locked balance in microTari
- `pending_incoming` (int): Pending incoming balance in microTari
- `pending_outgoing` (int): Pending outgoing balance in microTari

**Example:**
```python
try:
    balance = test_wallet.get_balance()

    # Test that all properties are accessible and return integers
    assert isinstance(balance.available, int)
    assert isinstance(balance.time_locked, int)
    assert isinstance(balance.pending_incoming, int)
    assert isinstance(balance.pending_outgoing, int)

    # Test that balances are non-negative
    assert balance.available >= 0
    assert balance.time_locked >= 0
    assert balance.pending_incoming >= 0
    assert balance.pending_outgoing >= 0

except tari_wallet.TariWalletError as e:
    pytest.skip(f"Balance retrieval failed (expected in test env): {e}")
```

---

#### PyTariPublicKey

Public key wrapper for cryptographic operations.

**Class Methods:**

##### from_hex(hex_str: str) -> PyTariPublicKey

Creates a public key from a hex string.

**Parameters:**
- `hex_str` (str): Hex-encoded public key

**Returns:** PyTariPublicKey instance

**Raises:** TariWalletError on invalid hex string

**Example:**
```python
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

**Methods:**

##### to_hex() -> str

Converts the public key to a hex string.

**Returns:** Hex-encoded public key string

**Raises:** TariWalletError on conversion failure

##### to_emoji_encoding() -> str

Converts the public key to emoji encoding.

**Returns:** Emoji-encoded public key string

**Raises:** TariWalletError on conversion failure

**Example:**
```python
try:
    public_key = tari_wallet.PyTariPublicKey.from_hex(hex_str)
    result_hex = public_key.to_hex()

    assert isinstance(result_hex, str)
    assert len(result_hex) > 0
    # Result should be valid hex
    assert all(c in "0123456789abcdef" for c in result_hex.lower())

except tari_wallet.TariWalletError:
    pytest.skip("Public key creation failed with test hex")
```

---

#### TariWalletError

Exception class for wallet operation errors.

**Inheritance:** Inherits from Python's RuntimeError

**Usage:**
```python
try:
    balance = wallet.get_balance()
except tari_wallet.TariWalletError as e:
    print(f"Wallet operation failed: {e}")
```

## Event Callbacks

The wallet supports real-time event notifications through callback functions.

### Callback Dictionary Format

```python
callbacks = {
    "received_transaction": lambda tx_ptr: print(f"Received transaction: {tx_ptr}"),
    "received_transaction_reply": lambda tx_ptr: print(f"Transaction reply: {tx_ptr}"),
    "received_finalized_transaction": lambda tx_ptr: print(f"Finalized transaction: {tx_ptr}"),
    "transaction_broadcast": lambda tx_ptr: print(f"Transaction broadcast: {tx_ptr}"),
    "transaction_mined": lambda tx_ptr: print(f"Transaction mined: {tx_ptr}"),
    "transaction_mined_unconfirmed": lambda tx_ptr, confirmations: print(f"Transaction mined unconfirmed: {tx_ptr}, confirmations: {confirmations}"),
    "balance_updated": lambda balance_ptr: print(f"Balance updated: {balance_ptr}"),
    "connectivity_status": lambda status: print(f"Connectivity status: {status}"),
}
```

### Available Callbacks

#### Transaction Events

- `"received_transaction"`: `(tx_ptr: int) -> None` - Called when a transaction is received
- `"received_transaction_reply"`: `(tx_ptr: int) -> None` - Called when a transaction reply is received
- `"received_finalized_transaction"`: `(tx_ptr: int) -> None` - Called when a transaction is finalized
- `"transaction_broadcast"`: `(tx_ptr: int) -> None` - Called when a transaction is broadcast to the network
- `"transaction_mined"`: `(tx_ptr: int) -> None` - Called when a transaction is mined
- `"transaction_mined_unconfirmed"`: `(tx_ptr: int, confirmations: int) -> None` - Called when a transaction is mined but unconfirmed
- `"transaction_send_result"`: `(tx_id: int, status: int) -> None` - Called with the result of a transaction send operation
- `"transaction_cancellation"`: `(tx_ptr: int, reason: int) -> None` - Called when a transaction is cancelled

#### Validation Events

- `"txo_validation_complete"`: `(request_key: int, results: int) -> None` - Called when TXO validation completes
- `"transaction_validation_complete"`: `(request_key: int, results: int) -> None` - Called when transaction validation completes

#### Network Events

- `"connectivity_status"`: `(status: int) -> None` - Called when connectivity status changes
- `"contacts_liveness_data_updated"`: `(data_ptr: int) -> None` - Called when contact liveness data updates
- `"saf_messages_received"`: `() -> None` - Called when SAF messages are received

#### Wallet Events

- `"balance_updated"`: `(balance_ptr: int) -> None` - Called when wallet balance updates
- `"wallet_scanned_height"`: `(height: int) -> None` - Called when wallet scanning progresses
- `"base_node_state"`: `(state_ptr: int) -> None` - Called when base node state changes

#### Faux Transaction Events

- `"faux_transaction_confirmed"`: `(tx_ptr: int) -> None` - Called when a faux transaction is confirmed
- `"faux_transaction_unconfirmed"`: `(tx_ptr: int, confirmations: int) -> None` - Called when a faux transaction becomes unconfirmed

## Constants and Types

### Network Identifiers
- `"localnet"`: Local development network
- `"nextnet"`: Tari test network
- `"mainnet"`: Tari main network

### Log Verbosity Levels
- `0`: Error
- `1`: Warning  
- `2`: Info
- `3`: Debug
- `4`: Trace

### Units
- All amounts are in microTari (1 Tari = 1,000,000 microTari)
- All timeouts are in seconds
- All file sizes are in bytes

## Error Handling Best Practices

1. **Always wrap wallet operations in try-catch blocks:**
```python
try:
    result = wallet.some_operation()
except tari_wallet.TariWalletError as e:
    logger.error(f"Wallet operation failed: {e}")
    # Handle the error appropriately
```

2. **Check for null/empty results:**
```python
transactions = wallet.get_completed_transactions()
assert isinstance(transactions, list)

# Test with limit
transactions_limited = wallet.get_completed_transactions(limit=5)
assert isinstance(transactions_limited, list)
assert len(transactions_limited) <= 5
```

3. **Validate inputs before passing to wallet:**
```python
if not dest_address or len(dest_address) < 10:
    raise ValueError("Invalid destination address")
```

## Thread Safety

- The Python bindings are **not thread-safe**
- Use wallet instances from a single thread only
- For multi-threaded applications, create separate wallet instances per thread
- Callback functions are called from the wallet's internal thread context

## Memory Management

- All objects are automatically garbage collected
- No manual cleanup required
- Large transaction lists are automatically freed
- Log files are automatically rotated based on configuration

## Performance Considerations

- Balance queries are fast (cached)
- Transaction queries may be slower for large wallets
- Contact queries are typically fast
- Network operations depend on connectivity
- Callback functions should be lightweight to avoid blocking wallet operations
