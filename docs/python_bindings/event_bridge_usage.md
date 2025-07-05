# Event Bridge System - Usage Guide

## Quick Start

### Basic Usage

```rust
use minotari_wallet_ffi::event_bridge::{EventBridge, types::*};

// Create an event bridge
let bridge = EventBridge::new(wallet_id);

// Send an event
let event = WalletEvent::new(
    EventType::TransactionReceived,
    wallet_id,
    EventData::TransactionReceived {
        tx_id: 123,
        amount: 1000000,
        sender_address: "sender123".to_string(),
        message: Some("Payment received".to_string()),
    },
);

bridge.send_event(event).await?;
```

### Registering Event Callbacks

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

let bridge = EventBridge::new(wallet_id);
let dispatcher = bridge.dispatcher();

// Register a callback for transaction events
let counter = Arc::new(AtomicU32::new(0));
let counter_clone = Arc::clone(&counter);

dispatcher.register_callback(
    EventType::TransactionReceived,
    "my_transaction_handler".to_string(),
    move |event| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
        println!("Received transaction: {:?}", event);
        Ok(())
    }
).await;
```

## Event Types Reference

### Transaction Events

#### TransactionReceived
Triggered when a transaction is received by the wallet.

```rust
EventData::TransactionReceived {
    tx_id: u64,              // Transaction ID
    amount: u64,             // Amount in microTari
    sender_address: String,  // Sender's address
    message: Option<String>, // Optional message
}
```

**Example:**
```rust
let event = WalletEvent::new(
    EventType::TransactionReceived,
    wallet_id,
    EventData::TransactionReceived {
        tx_id: 456,
        amount: 5000000,  // 5 XTR
        sender_address: "7a1b2c3d4e5f...".to_string(),
        message: Some("Coffee payment".to_string()),
    },
);
```

#### TransactionBroadcast  
Triggered when a transaction is broadcast to the network.

```rust
EventData::TransactionBroadcast {
    tx_id: u64,    // Transaction ID
    amount: u64,   // Amount in microTari
    fee: u64,      // Network fee in microTari
}
```

#### TransactionMined
Triggered when a transaction is confirmed in a block.

```rust
EventData::TransactionMined {
    tx_id: u64,                // Transaction ID
    amount: u64,               // Amount in microTari
    block_height: Option<u64>, // Block height (if known)
}
```

### Balance Events

#### BalanceUpdated
Triggered when the wallet balance changes.

```rust
EventData::BalanceUpdated {
    available: u64,              // Available balance in microTari
    pending_incoming: u64,       // Pending incoming in microTari
    pending_outgoing: u64,       // Pending outgoing in microTari
    timelocked: Option<u64>,     // Timelocked balance in microTari
}
```

**Example:**
```rust
let event = WalletEvent::new(
    EventType::BalanceUpdated,
    wallet_id,
    EventData::BalanceUpdated {
        available: 10000000,      // 10 XTR
        pending_incoming: 500000, // 0.5 XTR
        pending_outgoing: 100000, // 0.1 XTR
        timelocked: None,
    },
);
```

### Connectivity Events

#### ConnectivityStatus
Triggered when wallet connectivity status changes.

```rust
EventData::ConnectivityStatus {
    status: ConnectivityState,  // Connection state
    peer_count: u32,           // Number of connected peers
}
```

**ConnectivityState values:**
- `Disconnected`: No connection to network
- `Connecting`: Attempting to connect
- `Connected`: Connected to network
- `Synchronizing`: Syncing with network
- `Synchronized`: Fully synchronized

## Advanced Usage

### Custom Event Handlers

```rust
use std::error::Error;

// Define a custom handler
async fn custom_transaction_handler(
    event: &WalletEvent
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match &event.data {
        EventData::TransactionReceived { tx_id, amount, .. } => {
            // Custom processing logic
            if *amount > 1000000 {  // If > 1 XTR
                println!("Large transaction received: {} microTari", amount);
                // Send notification, update database, etc.
            }
            Ok(())
        }
        _ => Err("Unexpected event type".into())
    }
}

// Register the handler
dispatcher.register_callback(
    EventType::TransactionReceived,
    "large_transaction_monitor".to_string(),
    custom_transaction_handler
).await;
```

### Event Statistics

```rust
// Get event bridge statistics
let stats = bridge.get_stats().await;

println!("Events processed: {}", stats.events_processed);
println!("Events failed: {}", stats.events_failed);
println!("Average processing time: {:.2}ms", stats.average_processing_time_ms);
println!("Active callbacks: {}", stats.active_callbacks);
```

### Working with Event Channels

```rust
use minotari_wallet_ffi::event_bridge::channel::EventChannelBuilder;

// Create a custom channel
let (channel, mut receiver) = EventChannelBuilder::new(wallet_id)
    .with_capacity(1000)  // Bounded channel with capacity
    .with_metrics(true)   // Enable detailed metrics
    .build();

// Send events through the channel
let event = WalletEvent::new(/* ... */);
channel.send(event).await?;

// Receive events
while let Some(received_event) = receiver.recv().await {
    println!("Received: {:?}", received_event);
}
```

## Testing with MockWallet

### Basic Mock Setup

```rust
use minotari_wallet_ffi::testing::mock_wallet::MockWallet;

#[tokio::test]
async fn test_event_handling() {
    let mut mock_wallet = MockWallet::default();
    mock_wallet.start().unwrap();
    mock_wallet.init_event_bridge().unwrap();
    
    // Simulate events
    mock_wallet.simulate_received_transaction(
        123,                    // tx_id
        1000000,               // amount (1 XTR)
        "test_sender"          // sender
    ).await.unwrap();
    
    // Verify event bridge statistics
    let stats = mock_wallet.get_event_bridge_stats().await;
    assert!(stats.is_some());
}
```

### Advanced Mock Testing

```rust
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_callback_integration() {
    let mut mock_wallet = MockWallet::default();
    mock_wallet.start().unwrap();
    mock_wallet.init_event_bridge().unwrap();
    
    // Track received events
    let received_events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&received_events);
    
    // Register event callback
    mock_wallet.register_event_callback(
        EventType::TransactionReceived,
        "test_callback".to_string(),
        move |event| {
            events_clone.lock().unwrap().push(event.clone());
            Ok(())
        }
    ).await.unwrap();
    
    // Simulate multiple transactions
    for i in 0..5 {
        mock_wallet.simulate_received_transaction(
            i,
            1000000 * (i + 1),
            &format!("sender_{}", i)
        ).await.unwrap();
    }
    
    // Give time for events to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify all events were received
    let events = received_events.lock().unwrap();
    assert_eq!(events.len(), 5);
}
```

## Serialization and Persistence

### JSON Serialization

```rust
use minotari_wallet_ffi::event_bridge::serialization::*;

let event = WalletEvent::new(/* ... */);

// Serialize to JSON
let json = serialize_event_to_json(&event)?;
println!("Event JSON: {}", json);

// Deserialize from JSON
let deserialized_event = deserialize_event_from_json(&json)?;
assert_eq!(event, deserialized_event);
```

### Binary Serialization

```rust
// Serialize to binary format (more efficient)
let binary_data = serialize_event(&event, SerializationFormat::Binary)?;
println!("Binary size: {} bytes", binary_data.len());

// Deserialize from binary
let deserialized_event = deserialize_event(&binary_data, SerializationFormat::Binary)?;
```

### Event Logging

```rust
use std::fs::File;
use std::io::Write;

// Log events to file
async fn log_event_to_file(event: &WalletEvent, filename: &str) -> Result<(), Box<dyn Error>> {
    let json = serialize_event_to_json(event)?;
    let mut file = File::create(filename)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

// Usage in event handler
dispatcher.register_callback(
    EventType::TransactionReceived,
    "event_logger".to_string(),
    |event| {
        // Log event asynchronously
        tokio::spawn(async move {
            log_event_to_file(&event, "transaction_events.json").await
        });
        Ok(())
    }
).await;
```

## Error Handling

### Handling Callback Errors

```rust
dispatcher.register_callback(
    EventType::TransactionReceived,
    "error_prone_handler".to_string(),
    |event| {
        // This handler might fail
        if event.data.is_problematic() {
            return Err("Processing failed".into());
        }
        
        // Normal processing
        process_transaction(event)?;
        Ok(())
    }
).await;
```

### Channel Error Recovery

```rust
use tokio::time::{timeout, Duration};

// Send with timeout and error handling
match timeout(Duration::from_secs(5), bridge.send_event(event)).await {
    Ok(Ok(())) => println!("Event sent successfully"),
    Ok(Err(e)) => eprintln!("Failed to send event: {}", e),
    Err(_) => eprintln!("Timeout sending event"),
}
```

## Performance Optimization

### Batch Event Processing

```rust
use std::collections::VecDeque;

// Batch multiple events for efficiency
async fn batch_process_events(
    events: VecDeque<WalletEvent>,
    bridge: &EventBridge
) -> Result<(), Box<dyn Error>> {
    for event in events {
        bridge.send_event(event).await?;
    }
    Ok(())
}
```

### Memory Management

```rust
// Use Arc for shared event data to reduce memory usage
use std::sync::Arc;

let shared_event_data = Arc::new(EventData::TransactionReceived {
    tx_id: 123,
    amount: 1000000,
    sender_address: "shared_sender".to_string(),
    message: Some("Shared message".to_string()),
});

// Multiple events can share the same data
for wallet_id in wallet_ids {
    let event = WalletEvent {
        event_type: EventType::TransactionReceived,
        wallet_id,
        data: Arc::clone(&shared_event_data),
        timestamp: SystemTime::now(),
    };
    bridge.send_event(event).await?;
}
```

## Integration Examples

### Web API Integration

```rust
use warp::Filter;

// Create web endpoint that triggers events
let event_route = warp::path("trigger_event")
    .and(warp::post())
    .and(warp::body::json())
    .and_then(|body: serde_json::Value| async move {
        let bridge = EventBridge::new(1);
        
        // Parse request and create event
        let event = parse_request_to_event(body)?;
        
        // Send event
        bridge.send_event(event).await?;
        
        Ok::<_, warp::Rejection>(warp::reply::with_status(
            "Event triggered",
            warp::http::StatusCode::OK
        ))
    });
```

### Database Integration

```rust
use sqlx::{PgPool, Row};

// Store events in database
async fn store_event_in_db(
    event: &WalletEvent,
    pool: &PgPool
) -> Result<(), sqlx::Error> {
    let json = serialize_event_to_json(event)?;
    
    sqlx::query!(
        "INSERT INTO wallet_events (wallet_id, event_type, event_data, timestamp) VALUES ($1, $2, $3, $4)",
        event.wallet_id as i64,
        event.event_type.to_string(),
        json,
        event.timestamp
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

// Register database storage handler
dispatcher.register_callback(
    EventType::TransactionReceived,
    "database_storage".to_string(),
    move |event| {
        let pool_clone = pool.clone();
        tokio::spawn(async move {
            store_event_in_db(&event, &pool_clone).await
        });
        Ok(())
    }
).await;
```

## Best Practices

### Event Handler Design

1. **Keep Handlers Fast**: Event handlers should complete quickly
2. **Use Async for I/O**: Use async for database/network operations
3. **Handle Errors Gracefully**: Don't let handler errors crash the system
4. **Avoid Blocking**: Never block in event handlers

### Memory Management

1. **Use Arc for Shared Data**: Reduce memory usage with shared references
2. **Clean Up Resources**: Ensure proper cleanup of resources
3. **Monitor Memory Usage**: Track memory usage in production
4. **Limit Event Retention**: Don't store events indefinitely

### Performance Monitoring

1. **Track Key Metrics**: Monitor latency, throughput, and error rates
2. **Set Alerts**: Alert on performance degradation
3. **Regular Testing**: Regularly test under load
4. **Profile Performance**: Use profiling tools to identify bottlenecks

### Security Considerations

1. **Validate Event Data**: Always validate incoming event data
2. **Sanitize Sensitive Data**: Remove sensitive information from logs
3. **Access Control**: Implement proper access control for events
4. **Audit Events**: Log important events for security auditing
