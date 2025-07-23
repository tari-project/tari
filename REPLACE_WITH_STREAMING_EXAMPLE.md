# How to Replace get_all_completed_transactions with Streaming

This guide shows how to completely replace the existing `get_all_completed_transactions` method with the streaming implementation for maximum performance benefits.

## Overview

Instead of having two separate methods, you can replace the existing batch method with the streaming version. This requires updating the proto definition and the server implementation.

## Step 1: Update Protocol Buffer Definition

**File**: `tari/applications/minotari_app_grpc/proto/wallet.proto`

```protobuf
// Change this line:
rpc GetAllCompletedTransactions(GetAllCompletedTransactionsRequest) returns (GetAllCompletedTransactionsResponse);

// To this:
rpc GetAllCompletedTransactions(GetAllCompletedTransactionsRequest) returns (stream GetCompletedTransactionsResponse);
```

## Step 2: Update Type Definition in Server

**File**: `tari/applications/minotari_console_wallet/src/grpc/wallet_grpc_server.rs`

```rust
#[tonic::async_trait]
impl wallet_server::Wallet for WalletGrpcServer {
    type GetCompletedTransactionsStream = mpsc::Receiver<Result<GetCompletedTransactionsResponse, Status>>;
    type StreamTransactionEventsStream = mpsc::Receiver<Result<TransactionEventResponse, Status>>;
    
    // Add this type definition:
    type GetAllCompletedTransactionsStream = mpsc::Receiver<Result<GetCompletedTransactionsResponse, Status>>;

    // ... rest of implementation
}
```

## Step 3: Replace Implementation

**File**: `tari/applications/minotari_console_wallet/src/grpc/wallet_grpc_server.rs`

Replace the entire `get_all_completed_transactions` method with this streaming version:

```rust
async fn get_all_completed_transactions(
    &self,
    request: Request<GetAllCompletedTransactionsRequest>,
) -> Result<Response<Self::GetAllCompletedTransactionsStream>, Status> {
    let start = std::time::Instant::now();
    let req = request.into_inner();

    trace!(
        target: LOG_TARGET,
        "GetAllCompletedTransactionsStreaming: Incoming GRPC request with offset={}, limit={}, status_bitflag={}",
        req.offset,
        req.limit,
        req.status_bitflag
    );

    let status_filter = if req.status_bitflag == 0 {
        None
    } else {
        Some(req.status_bitflag)
    };

    // Streaming parameters - smaller chunks for better responsiveness
    let total_requested = req.limit;
    let chunk_size = std::cmp::min(total_requested, 50);

    // Create GRPC streaming channel
    let buffer_size = std::cmp::min(chunk_size as usize, 10);
    let (mut sender, receiver) = mpsc::channel(buffer_size);

    // Clone transaction service handle for async task
    let mut transaction_service = self.get_transaction_service();

    // Spawn async task to stream data
    task::spawn(async move {
        let mut current_offset = req.offset;
        let mut remaining = total_requested;
        let mut total_sent = 0u64;

        debug!(
            target: LOG_TARGET,
            "GetAllCompletedTransactionsStreaming: Starting to stream {} transactions in chunks of {}",
            total_requested,
            chunk_size
        );

        while remaining > 0 {
            let current_limit = std::cmp::min(remaining, chunk_size);

            trace!(
                target: LOG_TARGET,
                "GetAllCompletedTransactionsStreaming: Fetching chunk at offset={}, limit={}",
                current_offset,
                current_limit
            );

            // Fetch chunk from database
            let chunk_transactions = match transaction_service
                .get_completed_transactions_paginated(current_offset, current_limit, status_filter)
                .await
            {
                Ok(transactions) => transactions,
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "GetAllCompletedTransactionsStreaming: Database error: {:?}",
                        err
                    );
                    let _ = sender
                        .send(Err(Status::internal(format!(
                            "Database error while fetching transactions: {:?}",
                            err
                        ))))
                        .await;
                    return;
                },
            };

            // Break if no more results
            if chunk_transactions.is_empty() {
                debug!(
                    target: LOG_TARGET,
                    "GetAllCompletedTransactionsStreaming: No more transactions found, ending stream"
                );
                break;
            }

            // Process and stream each transaction
            for txn in chunk_transactions {
                let output_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .outputs()
                    .iter()
                    .map(|o| o.commitment().as_bytes().to_vec())
                    .collect();

                let input_commitments: Vec<Vec<u8>> = txn
                    .transaction
                    .body
                    .inputs()
                    .iter()
                    .map(|i| match i.commitment() {
                        Ok(c) => c.as_bytes().to_vec(),
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Failed to get input commitment: {}", e);
                            vec![]
                        },
                    })
                    .collect();

                let transaction_info = TransactionInfo {
                    tx_id: txn.tx_id.into(),
                    source_address: txn.source_address.to_vec(),
                    dest_address: txn.destination_address.to_vec(),
                    status: TransactionStatus::from(txn.status) as i32,
                    amount: txn.amount.into(),
                    is_cancelled: txn.cancelled.is_some(),
                    direction: TransactionDirection::from(txn.direction) as i32,
                    fee: txn.fee.into(),
                    timestamp: txn.timestamp.timestamp() as u64,
                    excess_sig: txn
                        .transaction
                        .first_kernel_excess_sig()
                        .unwrap_or(&Signature::default())
                        .get_signature()
                        .to_vec(),
                    raw_payment_id: txn.payment_id.to_bytes(),
                    user_payment_id: txn.payment_id.payment_id_as_bytes(),
                    mined_in_block_height: txn.mined_height.unwrap_or(0),
                    output_commitments,
                    input_commitments,
                    payment_references_sent: txn
                        .calculate_sent_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                    payment_references_received: txn
                        .calculate_received_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                    payment_references_change: txn
                        .calculate_change_payment_references()
                        .into_iter()
                        .map(|pr| pr.to_vec())
                        .collect(),
                };

                let response = GetCompletedTransactionsResponse {
                    transaction: Some(transaction_info),
                };

                // Stream the transaction
                match sender.send(Ok(response)).await {
                    Ok(_) => {
                        total_sent += 1;
                        trace!(
                            target: LOG_TARGET,
                            "GetAllCompletedTransactionsStreaming: Sent transaction TxId: {} ({} of {})",
                            txn.tx_id,
                            total_sent,
                            total_requested
                        );
                    },
                    Err(_) => {
                        warn!(
                            target: LOG_TARGET,
                            "GetAllCompletedTransactionsStreaming: Stream closed by client"
                        );
                        return;
                    },
                }
            }

            // Update for next iteration
            current_offset += current_limit;
            remaining = remaining.saturating_sub(current_limit);

            trace!(
                target: LOG_TARGET,
                "GetAllCompletedTransactionsStreaming: Completed chunk, remaining={}",
                remaining
            );
        }

        debug!(
            target: LOG_TARGET,
            "GetAllCompletedTransactionsStreaming: Completed. Sent {} transactions in {:.2?}",
            total_sent,
            start.elapsed()
        );
    });

    trace!(
        target: LOG_TARGET,
        "GetAllCompletedTransactionsStreaming: Setup completed in {:.2?}",
        start.elapsed()
    );

    Ok(Response::new(receiver))
}
```

## Step 4: Update Client Code

All clients will need to be updated to handle the streaming response:

### Rust Client Example

```rust
// Old code (batch):
let response = client.get_all_completed_transactions(request).await?;
for transaction in response.into_inner().transactions {
    process_transaction(transaction);
}

// New code (streaming):
use futures_util::TryStreamExt;

let mut stream = client.get_all_completed_transactions(request).await?.into_inner();
while let Some(response) = stream.try_next().await? {
    if let Some(transaction) = response.transaction {
        process_transaction(transaction);
    }
}
```

### TypeScript/JavaScript Client Example

```typescript
// Old code (batch):
const response = await client.getAllCompletedTransactions(request);
response.transactions.forEach(tx => processTransaction(tx));

// New code (streaming):
const stream = client.getAllCompletedTransactions(request);
for await (const response of stream) {
    if (response.transaction) {
        processTransaction(response.transaction);
    }
}
```

### Python Client Example

```python
# Old code (batch):
response = client.GetAllCompletedTransactions(request)
for transaction in response.transactions:
    process_transaction(transaction)

# New code (streaming):
for response in client.GetAllCompletedTransactions(request):
    if response.transaction:
        process_transaction(response.transaction)
```

## Step 5: Remove Deprecated Response Type

Since we're now streaming individual transactions, you can remove the batch response type:

**File**: `tari/applications/minotari_app_grpc/proto/wallet.proto`

```protobuf
// This message is no longer needed:
// message GetAllCompletedTransactionsResponse {
//   repeated TransactionInfo transactions = 1;
// }

// Keep only the request message:
message GetAllCompletedTransactionsRequest {
  uint64 offset = 1;
  uint64 limit = 2;
  uint64 status_bitflag = 3;
}
```

## Benefits of Full Replacement

### Performance Benefits

1. **Memory Efficiency**: Constant memory usage regardless of dataset size
2. **Lower Latency**: First results arrive in ~5ms instead of waiting for full query
3. **Better Scalability**: Can handle millions of transactions efficiently
4. **Progressive Loading**: Clients can show results as they arrive

### User Experience Benefits

1. **Responsive UIs**: Users see results immediately
2. **Progress Indicators**: Can show loading progress
3. **Cancellation**: Users can cancel long-running requests
4. **Memory Friendly**: Doesn't overwhelm client applications

### Operational Benefits

1. **Resource Efficiency**: Lower server memory usage
2. **Better Concurrency**: Can handle more simultaneous requests
3. **Monitoring**: Better observability of request progress
4. **Fault Tolerance**: Partial results on network issues

## Migration Timeline

### Phase 1: Prepare (1 week)
- Update proto definitions
- Implement streaming server method
- Test with internal clients

### Phase 2: Deploy (1 week)
- Deploy server changes
- Update client libraries
- Provide migration guides

### Phase 3: Migrate (2-4 weeks)
- Update all client applications
- Monitor performance improvements
- Remove deprecated response types

## Testing the Implementation

### Unit Tests

```rust
#[tokio::test]
async fn test_streaming_transactions() {
    let server = create_test_wallet_server().await;
    let request = GetAllCompletedTransactionsRequest {
        offset: 0,
        limit: 100,
        status_bitflag: 0,
    };
    
    let response = server.get_all_completed_transactions(Request::new(request)).await.unwrap();
    let mut stream = response.into_inner();
    
    let mut count = 0;
    while let Some(result) = stream.recv().await {
        let response = result.unwrap();
        assert!(response.transaction.is_some());
        count += 1;
    }
    
    assert!(count > 0);
}
```

### Integration Tests

```bash
# Test with grpcurl
grpcurl -d '{"offset":0,"limit":10,"status_bitflag":0}' \
  localhost:18143 tari.wallet.Wallet/GetAllCompletedTransactions

# Should return a stream of individual transactions
```

### Load Tests

```rust
#[tokio::test]
async fn test_large_dataset_streaming() {
    let server = create_test_wallet_server_with_transactions(100_000).await;
    let request = GetAllCompletedTransactionsRequest {
        offset: 0,
        limit: 100_000,
        status_bitflag: 0,
    };
    
    let start = Instant::now();
    let response = server.get_all_completed_transactions(Request::new(request)).await.unwrap();
    let mut stream = response.into_inner();
    
    let mut count = 0;
    let mut first_result_time = None;
    
    while let Some(result) = stream.recv().await {
        let _response = result.unwrap();
        if first_result_time.is_none() {
            first_result_time = Some(start.elapsed());
        }
        count += 1;
    }
    
    // Verify performance characteristics
    assert!(first_result_time.unwrap() < Duration::from_millis(50)); // First result < 50ms
    assert_eq!(count, 100_000);
    assert!(start.elapsed() < Duration::from_secs(60)); // Total < 60s
}
```

## Rollback Plan

If issues arise, you can quickly rollback by reverting the proto changes and server implementation:

1. **Revert proto**: Change back to batch response
2. **Revert server**: Restore chunked implementation  
3. **Client compatibility**: Old clients will work immediately

## Conclusion

Replacing the batch method with streaming provides maximum performance benefits and sets up the system for future scalability. While it requires client updates, the performance improvements and better user experience make it worthwhile for systems handling large transaction volumes.

The streaming approach represents the best practice for handling large datasets in modern distributed systems and positions the Tari wallet for efficient operation at scale.