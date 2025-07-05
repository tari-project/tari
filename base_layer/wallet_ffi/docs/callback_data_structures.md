# Tari Wallet Callback Data Structures

## Overview

This document provides comprehensive documentation of all data structures passed to Tari Wallet FFI callbacks, their memory layouts, Python conversion requirements, and usage examples.

## Data Structure Categories

### Transaction Data Structures

#### InboundTransaction
**Used by:** `callback_received_transaction`

```rust
struct InboundTransaction {
    tx_id: TxId,                    // u64 - Unique transaction identifier
    source_address: TariAddress,    // Sender's wallet address
    amount: MicroMinotari,         // Transaction amount in µT
    fee: MicroMinotari,           // Transaction fee in µT
    message: String,              // Optional transaction message
    timestamp: NaiveDateTime,     // When transaction was received
    cancelled: Option<TxCancellationReason>, // Cancellation status
}
```

**Python Conversion:**
- `tx_id: u64` → `int`
- `source_address: TariAddress` → `str` (emoji format)
- `amount: MicroMinotari` → `int` (preserve precision)
- `fee: MicroMinotari` → `int`
- `message: String` → `str`
- `timestamp: NaiveDateTime` → `datetime`
- `cancelled: Option<Enum>` → `Optional[str]`

#### CompletedTransaction
**Used by:** Multiple transaction callbacks

```rust
struct CompletedTransaction {
    tx_id: TxId,                    // Unique transaction identifier
    source_address: TariAddress,    // Sender's address
    destination_address: TariAddress, // Recipient's address
    amount: MicroMinotari,         // Transaction amount
    fee: MicroMinotari,           // Transaction fee
    transaction: Transaction,      // The actual transaction structure
    status: TransactionStatus,     // Current transaction status
    message: String,              // Transaction message
    timestamp: NaiveDateTime,     // Transaction timestamp
    cancelled: Option<TxCancellationReason>, // Cancellation reason
    direction: TransactionDirection, // Inbound/Outbound/Coinbase
    send_count: u32,              // Number of send attempts
    last_send_timestamp: Option<NaiveDateTime>, // Last broadcast attempt
    confirmations: Option<u64>,    // Number of confirmations
}
```

**Transaction Status Values:**
- `Completed` - Transaction negotiated and finalized
- `Broadcast` - Sent to base node mempool
- `Mined` - Included in a block
- `Imported` - Imported from another source
- `Pending` - Waiting for completion

### Balance Data Structures

#### Balance
**Used by:** `callback_balance_updated`

```rust
struct Balance {
    available_balance: MicroMinotari,     // Spendable balance
    time_locked_balance: Option<Vec<...>>, // Time-locked outputs
    pending_incoming_balance: MicroMinotari, // Incoming pending
    pending_outgoing_balance: MicroMinotari, // Outgoing pending
}
```

**Balance States:**
- `available_balance` - Confirmed, spendable funds
- `pending_incoming_balance` - Transactions received but not yet mined
- `pending_outgoing_balance` - Transactions sent but not yet mined
- `time_locked_balance` - Outputs locked until specific time/height

**Precision Notes:**
- 1 Tari (XTR) = 1,000,000 microTari (µT)
- Python `int` can handle full precision without loss
- Display formatting should convert back to XTR for UI

### Status Data Structures

#### TransactionSendStatus
**Used by:** `callback_transaction_send_result`

```rust
enum TransactionSendStatus {
    Queued,                       // Transaction queued for sending
    Sending,                      // Currently being sent
    Sent,                        // Successfully sent
    Failed(String),              // Send failed with reason
    SentDirect,                  // Sent directly to recipient
}
```

**Error Reasons (Failed variant):**
- Network connectivity issues
- Invalid recipient address
- Insufficient funds
- Transaction validation errors

### Communication Data Structures

#### ContactsLivenessData
**Used by:** `callback_contacts_liveness_data_updated`

```rust
struct ContactsLivenessData {
    address: TariAddress,           // Contact's address
    online_status: Option<bool>,    // Whether contact is online
    last_seen: Option<NaiveDateTime>, // When contact was last seen
    metadata: HashMap<String, String>, // Additional contact metadata
}
```

### Network Data Structures

#### TariBaseNodeState
**Used by:** `callback_base_node_state`

```rust
struct TariBaseNodeState {
    node_id: Vec<u8>,              // Base node public key
    best_block_height: u64,        // Current blockchain height
    best_block_hash: BlockHash,    // Hash of current best block
    best_block_timestamp: u64,     // Timestamp of best block
    pruning_horizon: u64,          // Pruning horizon setting
    pruned_height: u64,            // Height up to which chain is pruned
    is_node_synced: bool,          // Whether node is synced
    updated_at: u64,               // Timestamp of last update
    latency: u64,                  // Network latency to node (ms)
}
```

## Python Conversion Complexity

| Type | Complexity | Key Challenges |
|------|------------|----------------|
| InboundTransaction | Complex | Nested structures, enum handling |
| CompletedTransaction | Advanced | Deep nesting, lifetime management |
| Balance | Moderate | Enum/Option handling, precision |
| TransactionSendStatus | Moderate | Enum variants with data |
| ContactsLivenessData | Complex | HashMap conversion, optional fields |
| TariBaseNodeState | Moderate | Multiple numeric fields, bool flags |
| u64 | Simple | Direct mapping to Python int |
| *mut c_void | Simple | Context pointer (opaque) |

## Memory Layout Considerations

### Structure Sizes (Platform-dependent)
- InboundTransaction: ~200-400 bytes
- CompletedTransaction: ~400-800 bytes  
- Balance: ~100-200 bytes
- TransactionSendStatus: ~50-100 bytes
- ContactsLivenessData: ~100-300 bytes
- TariBaseNodeState: ~150-200 bytes

### Memory Safety Notes
- All structures passed as boxed pointers (`*mut T`)
- Python bridge must handle pointer lifetime correctly
- No raw string pointers - all strings are owned
- Reference counting required for shared data

## Python Integration Examples

### Transaction Received Callback
```python
def on_transaction_received(tx_data):
    """
    tx_data: dict with keys:
    - tx_id: int
    - source_address: str 
    - amount: int (in microTari)
    - fee: int (in microTari)
    - message: str
    - timestamp: datetime
    - cancelled: Optional[str]
    """
    print(f"Received {tx_data['amount'] / 1_000_000} XTR from {tx_data['source_address']}")
    if tx_data['message']:
        print(f"Message: {tx_data['message']}")
```

### Balance Updated Callback
```python
def on_balance_updated(balance_data):
    """
    balance_data: dict with keys:
    - available: int (in microTari)
    - pending_incoming: int
    - pending_outgoing: int  
    - time_locked: Optional[List[dict]]
    """
    available_xtr = balance_data['available'] / 1_000_000
    pending_in_xtr = balance_data['pending_incoming'] / 1_000_000
    pending_out_xtr = balance_data['pending_outgoing'] / 1_000_000
    
    print(f"Balance: {available_xtr} XTR")
    print(f"Pending: +{pending_in_xtr} XTR, -{pending_out_xtr} XTR")
```

### Transaction Status Callback
```python
def on_transaction_send_result(tx_id, status_data):
    """
    tx_id: int
    status_data: dict with keys:
    - status: str ("Queued", "Sending", "Sent", "Failed", "SentDirect")
    - error_reason: Optional[str] (for "Failed" status)
    """
    if status_data['status'] == 'Failed':
        print(f"Transaction {tx_id} failed: {status_data.get('error_reason', 'Unknown error')}")
    else:
        print(f"Transaction {tx_id} status: {status_data['status']}")
```

## Implementation Status

### Current Status
✅ All C callback implementations exist and are functional
✅ PyO3 Python bridge functions implemented  
✅ Basic type conversion infrastructure in place
🔧 Python integration testing needed
🔧 Type conversion validation required
🔧 Performance testing needed

### Next Steps
1. Create comprehensive Python integration tests
2. Validate type conversion accuracy
3. Test callback registration and invocation
4. Measure performance characteristics
5. Document best practices for Python callback usage

## Testing Recommendations

### Unit Tests
- Verify structure size calculations
- Test memory layout assumptions
- Validate type conversion functions

### Integration Tests  
- Test callback registration with real wallet
- Verify data structure conversion accuracy
- Test callback invocation under various scenarios

### Performance Tests
- Measure callback latency (target: <10ms)
- Test memory usage with large data structures
- Validate precision preservation in conversions

## Best Practices

### Python Callback Implementation
1. Always check for None/null values in optional fields
2. Handle enum values as strings with proper validation
3. Convert microTari to Tari for display (divide by 1,000,000)
4. Use datetime objects for timestamps
5. Implement proper error handling for invalid data

### Memory Management
1. Never store raw pointers from callbacks
2. Copy data immediately in Python callbacks
3. Handle callback exceptions gracefully
4. Use appropriate Python types for Rust enums

### Performance Optimization
1. Minimize work in callback functions
2. Use async patterns for heavy processing
3. Cache frequently accessed data
4. Batch related operations when possible
