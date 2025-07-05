# Tari API Documentation

This directory contains comprehensive API reference documentation for Tari wallet components.

## API Reference

### Transaction Events API
- **[Transaction Events](transaction_events.md)** - Complete reference for transaction callback events, data structures, and usage patterns

### Python Bindings API  
- **[Python Bindings](../python_bindings/api.md)** - Complete Python wallet API reference
- **[Python Examples](../python_bindings/examples.md)** - Practical usage examples

### C FFI API
- **[C Headers](../../base_layer/wallet_ffi/wallet.h)** - C function declarations and structures
- **[Callback Reference](../../base_layer/wallet_ffi/docs/callback_analysis.md)** - All 18 callback functions documented

## Event System APIs

### Event Bridge
- **Architecture**: [Event Bridge Design](../../base_layer/wallet_ffi/docs/event_bridge_design.md)
- **Usage Guide**: [Event Bridge Usage](../../base_layer/wallet_ffi/docs/event_bridge_usage.md)
- **Performance**: Sub-millisecond latency, >10,000 events/second throughput

### Traditional Callbacks
- **Transaction Callbacks**: [Implementation Guide](../../base_layer/wallet_ffi/docs/transaction_callbacks.md)
- **Data Structures**: [Callback Data Reference](../../base_layer/wallet_ffi/docs/callback_data_structures.md)
- **Memory Safety**: [Valgrind Integration](../../scripts/run_valgrind_tests.sh)

## Quick Reference

### Event Types
```rust
// Transaction Events (10 types)
TransactionReceived, TransactionReply, TransactionFinalized,
TransactionBroadcast, TransactionMined, TransactionMinedUnconfirmed,
TransactionCancellation, TransactionSendResult, 
FauxTransactionConfirmed, FauxTransactionUnconfirmed

// Balance Events (1 type)
BalanceUpdated

// Network Events (2 types)  
ConnectivityStatus, BaseNodeState

// Communication Events (2 types)
ContactsLivenessDataUpdated, SAFMessagesReceived

// Validation Events (2 types)
TransactionValidationComplete, TXOValidationComplete

// Scanning Events (1 type)
WalletScannedHeight
```

### Performance Characteristics
- **Latency**: <1ms average event routing
- **Throughput**: >10,000 events/second sustained
- **Memory**: ~500 bytes overhead per event
- **Concurrency**: Full thread-safe operation

### Integration Patterns
- **Python**: Event bridge with async handlers (recommended)
- **C/C++**: Traditional callback registration
- **Memory Safety**: Valgrind integration and automated testing
- **Testing**: Comprehensive mock infrastructure and performance validation

## Development Resources

### Testing Tools
- **Memory Testing**: [Valgrind Scripts](../../scripts/run_valgrind_tests.sh)
- **Performance**: [Benchmark Suite](../../base_layer/wallet_ffi/benches/event_bridge_bench.rs)
- **Integration**: [Test Harness](../../base_layer/wallet_ffi/tests/)

### Example Applications
- **Basic Usage**: [Python Examples](../../base_layer/wallet_ffi/examples/)
- **Transaction Monitoring**: [Monitor Example](../../examples/transaction_monitor.py)
- **Mock Testing**: [Mock Callbacks](../../base_layer/wallet_ffi/tests/fixtures/mock_callbacks.py)

For detailed implementation guidance, refer to the specific API documentation linked above.
