# Transaction Events API Reference

## Overview

This document provides a comprehensive API reference for transaction events in the Tari Wallet system. The transaction event system provides real-time notifications for all transaction lifecycle events through both traditional C FFI callbacks and the modern event bridge system.

## Event Types

### TransactionReceived
Triggered when a new inbound transaction is detected.

#### Event Data Structure
```rust
pub struct TransactionData {
    pub tx_id: u64,                    // Unique transaction identifier
    pub source_address: String,        // Sender's wallet address
    pub amount: u64,                   // Transaction amount in µT (microTari)
    pub message: Option<String>,       // Optional transaction message
    pub timestamp: i64,                // Unix timestamp when received
    pub status: u8,                    // Transaction status code
}
```

#### C FFI Structure
```c
typedef struct TariPendingInboundTransaction {
    unsigned long long tx_id;
    const TariWalletAddress* source_pk;
    unsigned long long amount;
    const char* message;
    long long timestamp;
    unsigned long long status;
} TariPendingInboundTransaction;
```

#### Usage Examples

**Python Event Bridge:**
```python
@bridge.on_event(EventType.TransactionReceived)
async def handle_received_transaction(event):
    print(f"Received {event.amount} µT from {event.source_address}")
    if event.message:
        print(f"Message: {event.message}")
```

**C Callback:**
```c
void callback_received_transaction(TariPendingInboundTransaction* tx) {
    printf("Received transaction ID: %llu\n", tx->tx_id);
    printf("Amount: %llu µT\n", tx->amount);
    if (tx->message) {
        printf("Message: %s\n", tx->message);
    }
}
```

### TransactionReply
Triggered when a reply is received for a sent transaction.

#### Event Data Structure
```rust
pub struct TransactionData {
    pub tx_id: u64,                    // Original transaction ID
    pub source_address: String,        // Recipient's address (reply sender)
    pub amount: u64,                   // Original transaction amount
    pub message: Option<String>,       // Reply message from recipient
    pub timestamp: i64,                // When reply was received
    pub status: u8,                    // Reply status
}
```

#### Usage Example
```python
@bridge.on_event(EventType.TransactionReply)
async def handle_transaction_reply(event):
    print(f"Reply received for transaction {event.tx_id}")
    if event.message:
        print(f"Reply message: {event.message}")
```

### TransactionFinalized
Triggered when a transaction is finalized and ready for broadcast.

#### Event Data Structure
Same as `TransactionData` with `status` indicating finalization state.

#### Usage Example
```python
@bridge.on_event(EventType.TransactionFinalized)
async def handle_transaction_finalized(event):
    print(f"Transaction {event.tx_id} finalized, ready for broadcast")
```

### TransactionBroadcast
Triggered when a transaction is successfully broadcast to the network.

#### Event Data Structure
Same as `TransactionData` with `status` indicating broadcast success.

#### Usage Example
```python
@bridge.on_event(EventType.TransactionBroadcast)
async def handle_transaction_broadcast(event):
    print(f"Transaction {event.tx_id} broadcast to network")
    # Transaction is now pending in mempool
```

### TransactionMined
Triggered when a transaction is included in a mined block.

#### Event Data Structure
```rust
pub struct TransactionData {
    pub tx_id: u64,                    // Transaction ID
    pub source_address: String,        // Counterparty address
    pub amount: u64,                   // Transaction amount
    pub message: Option<String>,       // Transaction message
    pub timestamp: i64,                // Mining timestamp
    pub status: u8,                    // Mining status
}
```

#### Usage Example
```python
@bridge.on_event(EventType.TransactionMined)
async def handle_transaction_mined(event):
    print(f"Transaction {event.tx_id} mined successfully!")
    # Transaction is now confirmed
```

### TransactionMinedUnconfirmed  
Triggered when a transaction is mined but not yet confirmed.

#### Usage Example
```python
@bridge.on_event(EventType.TransactionMinedUnconfirmed)
async def handle_transaction_unconfirmed(event):
    print(f"Transaction {event.tx_id} mined but awaiting confirmation")
```

### TransactionCancellation
Triggered when a transaction is cancelled or rejected.

#### Event Data Structure
```rust
pub struct TransactionData {
    pub tx_id: u64,                    // Cancelled transaction ID
    pub source_address: String,        // Counterparty address
    pub amount: u64,                   // Transaction amount
    pub message: Option<String>,       // Cancellation reason
    pub timestamp: i64,                // Cancellation timestamp
    pub status: u8,                    // Cancellation status code
}
```

#### Usage Example
```python
@bridge.on_event(EventType.TransactionCancellation)
async def handle_transaction_cancelled(event):
    print(f"Transaction {event.tx_id} cancelled")
    if event.message:
        print(f"Reason: {event.message}")
```

### TransactionSendResult
Triggered with the result of a transaction send operation.

#### Event Data Structure
```rust
pub struct TransactionSendResult {
    pub tx_id: u64,                    // Transaction ID
    pub success: bool,                 // Whether send was successful
    pub failure_message: Option<String>, // Error message if failed
    pub timestamp: i64,                // Result timestamp
}
```

#### Usage Example
```python
@bridge.on_event(EventType.TransactionSendResult)
async def handle_send_result(event):
    if event.success:
        print(f"Transaction {event.tx_id} sent successfully")
    else:
        print(f"Failed to send transaction {event.tx_id}: {event.failure_message}")
```

### FauxTransactionConfirmed
Triggered when an imported transaction is confirmed.

#### Usage Example
```python
@bridge.on_event(EventType.FauxTransactionConfirmed)
async def handle_faux_confirmed(event):
    print(f"Imported transaction {event.tx_id} confirmed")
```

### FauxTransactionUnconfirmed
Triggered when an imported transaction becomes unconfirmed.

#### Usage Example
```python
@bridge.on_event(EventType.FauxTransactionUnconfirmed)
async def handle_faux_unconfirmed(event):
    print(f"Imported transaction {event.tx_id} unconfirmed")
```

## Status Codes

Transaction status codes provide detailed information about transaction state:

```rust
pub const TRANSACTION_STATUS_PENDING: u8 = 0;
pub const TRANSACTION_STATUS_COMPLETED: u8 = 1;
pub const TRANSACTION_STATUS_BROADCAST: u8 = 2;
pub const TRANSACTION_STATUS_MINED: u8 = 3;
pub const TRANSACTION_STATUS_CANCELLED: u8 = 4;
pub const TRANSACTION_STATUS_REJECTED: u8 = 5;
```

## Error Handling

### Event Bridge Error Handling
```python
@bridge.on_event(EventType.TransactionReceived)
async def handle_transaction_with_error_handling(event):
    try:
        # Process transaction
        await process_transaction(event)
    except Exception as e:
        logger.error(f"Error processing transaction {event.tx_id}: {e}")
        # Implement retry logic or error reporting
```

### C Callback Error Handling
```c
void callback_received_transaction(TariPendingInboundTransaction* tx) {
    if (!tx) {
        fprintf(stderr, "Null transaction pointer received\n");
        return;
    }
    
    if (tx->amount == 0) {
        fprintf(stderr, "Invalid transaction amount: %llu\n", tx->amount);
        return;
    }
    
    // Process valid transaction
    process_transaction(tx);
}
```

## Performance Considerations

### Event Bridge Performance
- **Latency**: <1ms average event delivery
- **Throughput**: >10,000 events/second sustained
- **Memory**: ~500 bytes overhead per event
- **Backpressure**: Automatic handling for high-frequency events

### Optimization Tips

1. **Use Async Handlers**: For non-blocking event processing
```python
@bridge.on_event(EventType.TransactionReceived)
async def handle_async(event):
    await expensive_operation(event)
```

2. **Batch Processing**: For high-volume scenarios
```python
transaction_queue = []

@bridge.on_event(EventType.TransactionReceived)
async def batch_handler(event):
    transaction_queue.append(event)
    if len(transaction_queue) >= 100:
        await process_batch(transaction_queue)
        transaction_queue.clear()
```

3. **Memory Management**: Proper cleanup for long-running handlers
```python
@bridge.on_event(EventType.TransactionReceived)
async def memory_efficient_handler(event):
    # Process immediately and don't hold references
    result = await process_transaction(event)
    # Let event be garbage collected
    return result
```

## Testing

### Mock Event Generation
```python
from tari_wallet.testing import MockEventBridge

# Create mock for testing
mock_bridge = MockEventBridge()

# Generate test events
test_event = TransactionData(
    tx_id=12345,
    source_address="test_address",
    amount=1000000,
    message=Some("Test transaction"),
    timestamp=1640995200,
    status=1
)

await mock_bridge.emit_event(EventType.TransactionReceived, test_event)
```

### Performance Testing
```python
import time
from tari_wallet import EventBridge

bridge = EventBridge.new()
event_count = 0
start_time = time.time()

@bridge.on_event(EventType.TransactionReceived)
async def benchmark_handler(event):
    global event_count
    event_count += 1
    
    if event_count % 1000 == 0:
        elapsed = time.time() - start_time
        rate = event_count / elapsed
        print(f"Processing rate: {rate:.0f} events/second")
```

## Memory Safety

The transaction event system includes comprehensive memory safety measures:

### Valgrind Integration
Use the provided valgrind scripts for memory leak detection:
```bash
# Run memory leak detection
./scripts/run_valgrind_tests.sh
```

### Memory Usage Monitoring
```python
import psutil
import os

@bridge.on_event(EventType.TransactionReceived)
async def memory_monitoring_handler(event):
    process = psutil.Process(os.getpid())
    memory_mb = process.memory_info().rss / 1024 / 1024
    
    if memory_mb > 100:  # Alert if memory usage exceeds 100MB
        print(f"High memory usage detected: {memory_mb:.1f}MB")
```

## Integration Examples

### Complete Transaction Monitor
```python
from tari_wallet import PyTariWallet, EventBridge, EventType
import asyncio
import logging

class TransactionMonitor:
    def __init__(self, wallet_passphrase):
        self.wallet = PyTariWallet.create_with_passphrase(wallet_passphrase)
        self.bridge = EventBridge.new()
        self.setup_handlers()
    
    def setup_handlers(self):
        @self.bridge.on_event(EventType.TransactionReceived)
        async def on_received(event):
            logging.info(f"📥 Received {event.amount} µT from {event.source_address}")
        
        @self.bridge.on_event(EventType.TransactionBroadcast)
        async def on_broadcast(event):
            logging.info(f"📡 Transaction {event.tx_id} broadcast to network")
        
        @self.bridge.on_event(EventType.TransactionMined)
        async def on_mined(event):
            logging.info(f"⛏️ Transaction {event.tx_id} mined successfully!")
    
    async def start(self):
        await self.bridge.start()
        logging.info("Transaction monitor started")
    
    async def stop(self):
        await self.bridge.stop()
        logging.info("Transaction monitor stopped")

# Usage
async def main():
    monitor = TransactionMonitor("my_wallet_passphrase")
    await monitor.start()
    
    try:
        # Keep monitoring until interrupted
        await asyncio.sleep(float('inf'))
    except KeyboardInterrupt:
        await monitor.stop()

if __name__ == "__main__":
    asyncio.run(main())
```

For more examples and detailed implementation guidance, see the [main callback documentation](../callbacks.md) and [wallet FFI documentation](../../base_layer/wallet_ffi/docs/).
