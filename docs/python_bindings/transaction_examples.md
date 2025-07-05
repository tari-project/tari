# Tari Wallet Transaction Examples

This document provides comprehensive examples for working with Tari wallet transactions using Python bindings and the event bridge system.

## Overview

The Tari wallet Python bindings provide multiple approaches for transaction monitoring and handling:

1. **Event Bridge System (Recommended)** - Modern async event handling with high performance
2. **Traditional Callbacks** - Direct callback registration for immediate event handling
3. **Polling API** - Manual transaction status checking for simple integrations

## Complete Transaction Monitor Example

The [`transaction_monitor.py`](../../examples/transaction_monitor.py) example demonstrates a production-ready transaction monitoring application with:

### Features
- **Real-time Event Handling**: All 18 transaction and wallet event types
- **Performance Monitoring**: Sub-millisecond latency tracking and throughput statistics
- **Memory Safety**: Built-in memory usage monitoring and leak detection
- **Error Recovery**: Comprehensive error handling and retry logic
- **Graceful Shutdown**: Proper cleanup and final statistics reporting
- **Testing Support**: Mock event generation for development and testing

### Running the Example

```bash
# Basic usage with real wallet
python examples/transaction_monitor.py --passphrase "my_secure_passphrase"

# Testing mode with mock events
python examples/transaction_monitor.py --mock --log-level DEBUG

# Custom configuration
python examples/transaction_monitor.py \
    --passphrase "production_wallet" \
    --log-level INFO
```

### Expected Output

```
2024-01-15 10:30:00,123 - INFO - Initializing Tari wallet transaction monitor...
2024-01-15 10:30:01,456 - INFO - Wallet created successfully
2024-01-15 10:30:01,789 - INFO - Transaction monitor initialized successfully
2024-01-15 10:30:02,012 - INFO - Event bridge started successfully
2024-01-15 10:30:02,034 - INFO - 🚀 Transaction monitor is now running...

2024-01-15 10:30:15,234 - INFO - 📥 RECEIVED: Transaction 12345 | Amount: 1,500,000 µT | From: 7a2b8c4d1e... | Message: Payment for services
2024-01-15 10:30:16,567 - INFO - 📡 BROADCAST: Transaction 12346 | Amount: 2,000,000 µT | To: 9f3e7b2a8c...
2024-01-15 10:30:45,890 - INFO - ⛏️ MINED: Transaction 12345 | Amount: 1,500,000 µT | Status: Confirmed ✅
2024-01-15 10:30:46,123 - INFO - 💰 BALANCE: Available: 15,750,000 µT | Pending In: 0 µT | Pending Out: 500,000 µT

2024-01-15 10:31:00,456 - INFO - 📊 STATS: Events: 25 | Rate: 0.8/sec | Received: 5 | Mined: 3
```

## Quick Start Examples

### Basic Event Bridge Setup

```python
import asyncio
from tari_wallet import PyTariWallet, EventBridge, EventType

async def basic_monitor():
    # Create wallet and event bridge
    wallet = PyTariWallet.create_with_passphrase("my_passphrase")
    bridge = EventBridge.new()
    
    # Handle transaction events
    @bridge.on_event(EventType.TransactionReceived)
    async def on_received(event):
        print(f"Received {event.amount} µT from {event.source_address}")
    
    @bridge.on_event(EventType.TransactionMined)
    async def on_mined(event):
        print(f"Transaction {event.tx_id} confirmed!")
    
    # Start monitoring
    await bridge.start()
    
    try:
        # Monitor until interrupted
        await asyncio.sleep(float('inf'))
    except KeyboardInterrupt:
        await bridge.stop()

# Run the monitor
asyncio.run(basic_monitor())
```

### Transaction Sending with Event Monitoring

```python
import asyncio
from tari_wallet import PyTariWallet, EventBridge, EventType

class TransactionSender:
    def __init__(self, passphrase):
        self.wallet = PyTariWallet.create_with_passphrase(passphrase)
        self.bridge = EventBridge.new()
        self.pending_transactions = {}
        
    async def setup(self):
        # Monitor send results
        @self.bridge.on_event(EventType.TransactionSendResult)
        async def on_send_result(event):
            tx_id = event.tx_id
            if event.success:
                print(f"✅ Transaction {tx_id} sent successfully")
                self.pending_transactions[tx_id] = 'sent'
            else:
                print(f"❌ Failed to send transaction {tx_id}: {event.failure_message}")
                self.pending_transactions.pop(tx_id, None)
        
        # Monitor broadcast confirmation
        @self.bridge.on_event(EventType.TransactionBroadcast)
        async def on_broadcast(event):
            tx_id = event.tx_id
            print(f"📡 Transaction {tx_id} broadcast to network")
            self.pending_transactions[tx_id] = 'broadcast'
        
        # Monitor mining confirmation
        @self.bridge.on_event(EventType.TransactionMined)
        async def on_mined(event):
            tx_id = event.tx_id
            print(f"⛏️ Transaction {tx_id} mined and confirmed!")
            self.pending_transactions[tx_id] = 'confirmed'
        
        await self.bridge.start()
    
    async def send_transaction(self, address: str, amount: int, message: str = ""):
        """Send a transaction and track its progress"""
        try:
            # Send transaction through wallet
            tx_id = await self.wallet.send_transaction(address, amount, message)
            self.pending_transactions[tx_id] = 'pending'
            
            print(f"🚀 Initiated transaction {tx_id} for {amount:,} µT")
            return tx_id
            
        except Exception as e:
            print(f"Failed to initiate transaction: {e}")
            return None
    
    async def wait_for_confirmation(self, tx_id: int, timeout: int = 300):
        """Wait for transaction confirmation with timeout"""
        start_time = asyncio.get_event_loop().time()
        
        while asyncio.get_event_loop().time() - start_time < timeout:
            status = self.pending_transactions.get(tx_id)
            
            if status == 'confirmed':
                return True
            elif status is None:
                return False  # Transaction failed or cancelled
            
            await asyncio.sleep(1)
        
        print(f"⏰ Timeout waiting for transaction {tx_id} confirmation")
        return False

# Usage example
async def send_and_wait():
    sender = TransactionSender("sender_wallet_passphrase")
    await sender.setup()
    
    # Send a transaction
    tx_id = await sender.send_transaction(
        address="recipient_address_here",
        amount=1000000,  # 1 XTR
        message="Payment for goods"
    )
    
    if tx_id:
        # Wait for confirmation
        confirmed = await sender.wait_for_confirmation(tx_id)
        if confirmed:
            print("✅ Transaction completed successfully!")
        else:
            print("❌ Transaction failed or timed out")

asyncio.run(send_and_wait())
```

### Balance Monitoring with Alerts

```python
import asyncio
from tari_wallet import PyTariWallet, EventBridge, EventType

class BalanceMonitor:
    def __init__(self, passphrase, min_balance_alert=1000000):  # 1 XTR
        self.wallet = PyTariWallet.create_with_passphrase(passphrase)
        self.bridge = EventBridge.new()
        self.min_balance_alert = min_balance_alert
        self.current_balance = 0
        
    async def setup(self):
        @self.bridge.on_event(EventType.BalanceUpdated)
        async def on_balance_update(event):
            old_balance = self.current_balance
            self.current_balance = event.available_balance
            
            # Log balance change
            change = self.current_balance - old_balance
            if change > 0:
                print(f"💰⬆️ Balance increased by {change:,} µT (Total: {self.current_balance:,} µT)")
            elif change < 0:
                print(f"💰⬇️ Balance decreased by {abs(change):,} µT (Total: {self.current_balance:,} µT)")
            
            # Low balance alert
            if self.current_balance < self.min_balance_alert:
                print(f"🚨 LOW BALANCE ALERT: {self.current_balance:,} µT (minimum: {self.min_balance_alert:,} µT)")
                await self.handle_low_balance()
            
            # Show pending balances
            if event.pending_incoming_balance > 0:
                print(f"⏳ Pending incoming: {event.pending_incoming_balance:,} µT")
            if event.pending_outgoing_balance > 0:
                print(f"⏳ Pending outgoing: {event.pending_outgoing_balance:,} µT")
        
        await self.bridge.start()
    
    async def handle_low_balance(self):
        """Handle low balance condition"""
        print("💡 Consider adding funds to your wallet")
        # Could implement automatic funding logic here
        
    async def get_current_balance(self):
        """Get current balance from wallet"""
        try:
            balance = await self.wallet.get_balance()
            return balance.available_balance
        except Exception as e:
            print(f"Error getting balance: {e}")
            return 0

# Usage
async def monitor_balance():
    monitor = BalanceMonitor("balance_monitor_wallet", min_balance_alert=5000000)  # 5 XTR alert
    await monitor.setup()
    
    # Get initial balance
    initial_balance = await monitor.get_current_balance()
    print(f"Initial balance: {initial_balance:,} µT")
    
    try:
        await asyncio.sleep(float('inf'))
    except KeyboardInterrupt:
        print("Balance monitoring stopped")

asyncio.run(monitor_balance())
```

### High-Frequency Transaction Processing

```python
import asyncio
import time
from collections import deque
from tari_wallet import PyTariWallet, EventBridge, EventType

class HighVolumeProcessor:
    """Example for processing high-frequency transaction events"""
    
    def __init__(self, passphrase, batch_size=100, batch_timeout=5.0):
        self.wallet = PyTariWallet.create_with_passphrase(passphrase)
        self.bridge = EventBridge.new()
        self.batch_size = batch_size
        self.batch_timeout = batch_timeout
        
        # Batching infrastructure
        self.transaction_queue = deque()
        self.last_batch_time = time.time()
        self.processing_stats = {
            'total_processed': 0,
            'batches_processed': 0,
            'average_batch_size': 0
        }
        
    async def setup(self):
        @self.bridge.on_event(EventType.TransactionReceived)
        async def on_transaction(event):
            # Add to queue for batch processing
            self.transaction_queue.append({
                'event': event,
                'timestamp': time.time()
            })
            
            # Process batch if size threshold reached or timeout exceeded
            current_time = time.time()
            time_since_last_batch = current_time - self.last_batch_time
            
            if (len(self.transaction_queue) >= self.batch_size or 
                time_since_last_batch >= self.batch_timeout):
                await self.process_batch()
        
        await self.bridge.start()
        
        # Start periodic batch processor for timeout handling
        asyncio.create_task(self.periodic_batch_processor())
    
    async def process_batch(self):
        """Process a batch of transactions"""
        if not self.transaction_queue:
            return
        
        # Extract batch
        batch = []
        batch_size = min(len(self.transaction_queue), self.batch_size)
        
        for _ in range(batch_size):
            if self.transaction_queue:
                batch.append(self.transaction_queue.popleft())
        
        if not batch:
            return
        
        print(f"📦 Processing batch of {len(batch)} transactions...")
        
        try:
            # Process transactions in batch
            start_time = time.time()
            
            # Example: Database insertion, analysis, etc.
            await self.process_transaction_batch(batch)
            
            processing_time = time.time() - start_time
            
            # Update statistics
            self.processing_stats['total_processed'] += len(batch)
            self.processing_stats['batches_processed'] += 1
            self.processing_stats['average_batch_size'] = (
                self.processing_stats['total_processed'] / 
                self.processing_stats['batches_processed']
            )
            
            print(f"✅ Batch processed in {processing_time*1000:.1f}ms")
            print(f"📊 Stats: {self.processing_stats['total_processed']} total, "
                  f"avg batch: {self.processing_stats['average_batch_size']:.1f}")
            
            self.last_batch_time = time.time()
            
        except Exception as e:
            print(f"❌ Error processing batch: {e}")
            # Could implement retry logic here
    
    async def process_transaction_batch(self, batch):
        """Process a batch of transactions (implement your logic here)"""
        # Example processing: analyze transaction patterns
        total_amount = sum(item['event'].amount for item in batch)
        unique_addresses = set(item['event'].source_address for item in batch)
        
        print(f"   📈 Batch analysis: Total amount: {total_amount:,} µT, "
              f"Unique addresses: {len(unique_addresses)}")
        
        # Simulate processing time
        await asyncio.sleep(0.01)  # 10ms processing time
        
        # Example: Store in database, send notifications, etc.
        for item in batch:
            event = item['event']
            # Process individual transaction
            await self.process_single_transaction(event)
    
    async def process_single_transaction(self, event):
        """Process a single transaction (implement your logic)"""
        # Example: fraud detection, categorization, etc.
        if event.amount > 10000000:  # > 10 XTR
            print(f"🚨 Large transaction detected: {event.tx_id} for {event.amount:,} µT")
        
        # Additional processing logic here
        pass
    
    async def periodic_batch_processor(self):
        """Periodic processor for timeout-based batching"""
        while True:
            try:
                await asyncio.sleep(1)  # Check every second
                
                current_time = time.time()
                time_since_last_batch = current_time - self.last_batch_time
                
                # Process batch if timeout exceeded and queue not empty
                if (time_since_last_batch >= self.batch_timeout and 
                    len(self.transaction_queue) > 0):
                    await self.process_batch()
                    
            except Exception as e:
                print(f"Error in periodic batch processor: {e}")

# Usage
async def high_volume_processing():
    processor = HighVolumeProcessor(
        "high_volume_wallet",
        batch_size=50,      # Process 50 transactions at once
        batch_timeout=3.0   # Or every 3 seconds
    )
    
    await processor.setup()
    
    print("🚀 High-volume transaction processor started")
    print("Processing transactions in batches for optimal performance...")
    
    try:
        await asyncio.sleep(float('inf'))
    except KeyboardInterrupt:
        print("High-volume processor stopped")

asyncio.run(high_volume_processing())
```

## Error Handling Patterns

### Robust Event Handler with Retry Logic

```python
import asyncio
import random
from tari_wallet import EventBridge, EventType

class RobustEventHandler:
    def __init__(self):
        self.bridge = EventBridge.new()
        self.retry_queue = []
        self.max_retries = 3
        
    async def setup(self):
        @self.bridge.on_event(EventType.TransactionReceived)
        async def robust_handler(event):
            await self.handle_with_retry(self.process_transaction, event)
        
        await self.bridge.start()
        
        # Start retry processor
        asyncio.create_task(self.retry_processor())
    
    async def handle_with_retry(self, handler, event, attempt=1):
        """Handle event with automatic retry on failure"""
        try:
            await handler(event)
        except Exception as e:
            print(f"❌ Error in handler (attempt {attempt}): {e}")
            
            if attempt < self.max_retries:
                # Add to retry queue with exponential backoff
                retry_delay = 2 ** attempt + random.uniform(0, 1)
                retry_item = {
                    'handler': handler,
                    'event': event,
                    'attempt': attempt + 1,
                    'retry_time': time.time() + retry_delay
                }
                self.retry_queue.append(retry_item)
                print(f"⏳ Scheduled retry in {retry_delay:.1f}s")
            else:
                print(f"💀 Max retries exceeded for event {event.tx_id}")
    
    async def process_transaction(self, event):
        """Example transaction processor that might fail"""
        # Simulate random failures for testing
        if random.random() < 0.3:  # 30% failure rate
            raise Exception("Simulated processing failure")
        
        print(f"✅ Successfully processed transaction {event.tx_id}")
    
    async def retry_processor(self):
        """Process retry queue"""
        while True:
            try:
                current_time = time.time()
                
                # Find items ready for retry
                ready_items = [
                    item for item in self.retry_queue 
                    if item['retry_time'] <= current_time
                ]
                
                # Remove ready items from queue
                self.retry_queue = [
                    item for item in self.retry_queue 
                    if item['retry_time'] > current_time
                ]
                
                # Process retries
                for item in ready_items:
                    await self.handle_with_retry(
                        item['handler'], 
                        item['event'], 
                        item['attempt']
                    )
                
                await asyncio.sleep(1)  # Check every second
                
            except Exception as e:
                print(f"Error in retry processor: {e}")

# Usage
handler = RobustEventHandler()
asyncio.run(handler.setup())
```

## Performance Optimization

### Event Handler Performance Tips

1. **Use Async Handlers**: Always use async functions for event handlers
2. **Avoid Blocking Operations**: Use `asyncio.sleep()` instead of `time.sleep()`
3. **Batch Processing**: Group related operations for better throughput
4. **Memory Management**: Don't hold references to event data longer than necessary
5. **Error Isolation**: Handle errors locally to prevent handler failures

### Memory Optimization Example

```python
import asyncio
import weakref
from tari_wallet import EventBridge, EventType

class MemoryOptimizedHandler:
    def __init__(self):
        self.bridge = EventBridge.new()
        # Use weak references for caches to prevent memory leaks
        self.address_cache = weakref.WeakValueDictionary()
        
    async def setup(self):
        @self.bridge.on_event(EventType.TransactionReceived)
        async def memory_efficient_handler(event):
            # Process immediately, don't store references
            await self.process_immediately(event)
            
            # If caching is needed, use weak references
            if event.source_address not in self.address_cache:
                # Create lightweight cache entry
                cache_entry = {'count': 1, 'last_seen': time.time()}
                self.address_cache[event.source_address] = cache_entry
            else:
                self.address_cache[event.source_address]['count'] += 1
            
            # Clear event reference immediately
            del event
        
        await self.bridge.start()
    
    async def process_immediately(self, event):
        """Process event data immediately without storing"""
        # Extract needed data immediately
        tx_data = {
            'id': event.tx_id,
            'amount': event.amount,
            'address_hash': hash(event.source_address)  # Don't store full address
        }
        
        # Process the extracted data
        await self.handle_transaction_data(tx_data)
    
    async def handle_transaction_data(self, tx_data):
        """Handle extracted transaction data"""
        print(f"Processed transaction {tx_data['id']} for {tx_data['amount']} µT")
```

## Testing and Development

### Mock Event Generation for Testing

```python
import asyncio
import random
from tari_wallet.testing import MockEventBridge
from tari_wallet import EventType, TransactionData

async def test_event_handlers():
    """Test event handlers with mock events"""
    bridge = MockEventBridge()
    
    # Set up test handlers
    events_received = []
    
    @bridge.on_event(EventType.TransactionReceived)
    async def test_handler(event):
        events_received.append(event)
        print(f"Test: Received transaction {event.tx_id}")
    
    await bridge.start()
    
    # Generate test events
    for i in range(10):
        test_event = TransactionData(
            tx_id=i,
            source_address=f"test_address_{i}",
            amount=random.randint(100000, 10000000),
            message=f"Test transaction {i}",
            timestamp=int(time.time()),
            status=1
        )
        
        await bridge.emit_event(EventType.TransactionReceived, test_event)
        await asyncio.sleep(0.1)  # Small delay between events
    
    # Verify results
    assert len(events_received) == 10
    print(f"✅ Test passed: {len(events_received)} events processed")

# Run test
asyncio.run(test_event_handlers())
```

For more advanced examples and production deployment patterns, see the [complete transaction monitor example](../../examples/transaction_monitor.py) and the [main callback documentation](../callbacks.md).
