//! # Event Bridge Performance Benchmarks
//!
//! This module contains comprehensive performance benchmarks for the event bridge system.
//! The benchmarks test latency, throughput, and scalability under various conditions.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::runtime::Runtime;
use std::time::Duration;

// Import the modules we need to benchmark from the current crate
use minotari_wallet_ffi::event_bridge::{
    types::{WalletEvent, EventType, EventData, ConnectivityState},
    channel::EventChannelBuilder,
    EventBridge,
};

/// Create a sample transaction received event for benchmarking
fn create_sample_transaction_event(tx_id: u64) -> WalletEvent {
    WalletEvent::new(
        EventType::TransactionReceived,
        1,
        EventData::TransactionReceived {
            tx_id,
            amount: 1000000,
            sender_address: format!("sender_{}", tx_id),
            message: Some("benchmark event".to_string()),
        },
    )
}

/// Create a sample balance updated event for benchmarking
fn create_sample_balance_event(amount: u64) -> WalletEvent {
    WalletEvent::new(
        EventType::BalanceUpdated,
        1,
        EventData::BalanceUpdated {
            available: amount,
            pending_incoming: 0,
            pending_outgoing: 0,
            timelocked: None,
        },
    )
}

/// Benchmark event channel send/receive latency
fn bench_channel_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("event_channel_send_receive", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (channel, mut receiver) = EventChannelBuilder::new(1).build();
                let event = create_sample_transaction_event(1);
                
                channel.send(event).await.unwrap();
                receiver.recv().await.expect("Failed to receive event");
            });
        });
    });
}

/// Benchmark event throughput with varying numbers of events
fn bench_event_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("event_throughput");
    
    for event_count in [100, 1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("events", event_count),
            event_count,
            |b, &event_count| {
                b.iter(|| {
                    rt.block_on(async move {
                        let (channel, mut receiver) = EventChannelBuilder::new(1).build();
                        
                        // Spawn receiver task
                        let receive_task = tokio::spawn(async move {
                            for _ in 0..event_count {
                                receiver.recv().await.expect("Failed to receive event");
                            }
                        });
                        
                        // Send events
                        for i in 0..event_count {
                            let event = create_sample_transaction_event(i as u64);
                            channel.send(event).await.unwrap();
                        }
                        
                        receive_task.await.unwrap();
                    });
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark dispatcher with multiple callbacks
fn bench_dispatcher_callbacks(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("dispatcher_callbacks");
    
    for callback_count in [1, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("callbacks", callback_count),
            callback_count,
            |b, &callback_count| {
                b.iter(|| {
                    rt.block_on(async move {
                        let bridge = EventBridge::new(1);
                        let dispatcher = bridge.dispatcher();
                        
                        // Register multiple callbacks
                        let counter = Arc::new(AtomicU32::new(0));
                        for i in 0..callback_count {
                            let counter_clone = Arc::clone(&counter);
                            dispatcher
                                .register_callback(
                                    EventType::TransactionReceived,
                                    format!("callback_{}", i),
                                    move |_event| {
                                        counter_clone.fetch_add(1, Ordering::SeqCst);
                                        Ok(())
                                    },
                                )
                                .await;
                        }
                        
                        // Send event
                        let event = create_sample_transaction_event(1);
                        bridge.send_event(event).await.unwrap();
                        
                        // Wait for callbacks to complete
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        
                        // Verify all callbacks were called
                        assert_eq!(counter.load(Ordering::SeqCst), callback_count);
                    });
                });
            },
        );
    }
    
    group.finish();
}

/// Benchmark event serialization performance
fn bench_event_serialization(c: &mut Criterion) {
    use minotari_wallet_ffi::event_bridge::serialization::{
        serialize_event_to_json, deserialize_event_from_json,
        SerializationFormat, serialize_event, deserialize_event,
    };
    
    let event = create_sample_transaction_event(12345);
    
    c.bench_function("serialize_json", |b| {
        b.iter(|| {
            serialize_event_to_json(&event).unwrap()
        });
    });
    
    c.bench_function("deserialize_json", |b| {
        let json = serialize_event_to_json(&event).unwrap();
        b.iter(|| {
            deserialize_event_from_json(&json).unwrap()
        });
    });
    
    c.bench_function("serialize_binary", |b| {
        b.iter(|| {
            serialize_event(&event, SerializationFormat::Binary).unwrap()
        });
    });
    
    c.bench_function("deserialize_binary", |b| {
        let binary_data = serialize_event(&event, SerializationFormat::Binary).unwrap();
        b.iter(|| {
            deserialize_event(&binary_data, SerializationFormat::Binary).unwrap()
        });
    });
}

/// Benchmark queue backpressure handling
fn bench_queue_backpressure(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("queue_backpressure", |b| {
        b.iter(|| {
            rt.block_on(async {
            let (channel, mut receiver) = EventChannelBuilder::new(1).build();
            
            // Send many events quickly (faster than receiver can process)
            let send_count = 10000;
            for i in 0..send_count {
                let event = create_sample_balance_event(i as u64);
                channel.send(event).await.unwrap();
            }
            
            // Now receive all events
            for _ in 0..send_count {
                receiver.recv().await.expect("Failed to receive event");
            }
            });
        });
    });
}

/// Benchmark memory usage patterns
fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("memory_usage_stress", |b| {
        b.iter(|| {
            rt.block_on(async {
            let bridge = EventBridge::new(1);
            
            // Create and process many events to test memory management
            let event_count = 1000;
            for i in 0..event_count {
                let event = create_sample_transaction_event(i as u64);
                bridge.send_event(event).await.unwrap();
            }
            
            // Wait for processing
            tokio::time::sleep(Duration::from_millis(5)).await;
            
            // Check statistics
            let stats = bridge.get_stats().await;
            assert_eq!(stats.events_processed, event_count);
            });
        });
    });
}

/// Benchmark different event types processing
fn bench_event_types(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("event_types");
    
    let event_types = vec![
        ("transaction_received", EventType::TransactionReceived),
        ("balance_updated", EventType::BalanceUpdated),
        ("connectivity_status", EventType::ConnectivityStatus),
        ("transaction_mined", EventType::TransactionMined),
    ];
    
    for (name, event_type) in event_types {
        group.bench_function(name, |b| {
            b.iter(|| {
                rt.block_on(async {
                let bridge = EventBridge::new(1);
                let dispatcher = bridge.dispatcher();
                
                let counter = Arc::new(AtomicU32::new(0));
                let counter_clone = Arc::clone(&counter);
                
                dispatcher
                    .register_callback(
                        event_type.clone(),
                        "type_test_callback".to_string(),
                        move |_event| {
                            counter_clone.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                    )
                    .await;
                
                let event = match event_type {
                    EventType::TransactionReceived => create_sample_transaction_event(1),
                    EventType::BalanceUpdated => create_sample_balance_event(1000000),
                    EventType::ConnectivityStatus => WalletEvent::new(
                        EventType::ConnectivityStatus,
                        1,
                        EventData::ConnectivityStatus {
                            status: ConnectivityState::Connected,
                            peer_count: 5,
                        },
                    ),
                    EventType::TransactionMined => WalletEvent::new(
                        EventType::TransactionMined,
                        1,
                        EventData::TransactionMined {
                            tx_id: 1,
                            amount: 1000000,
                            block_height: Some(12345),
                        },
                    ),
                    _ => create_sample_transaction_event(1),
                };
                
                bridge.send_event(event).await.unwrap();
                
                // Wait for processing
                tokio::time::sleep(Duration::from_millis(1)).await;
                
                assert_eq!(counter.load(Ordering::SeqCst), 1);
                });
            });
        });
    }
    
    group.finish();
}

// Configure benchmark groups
criterion_group!(
    event_bridge_benches,
    bench_channel_latency,
    bench_event_throughput,
    bench_dispatcher_callbacks,
    bench_event_serialization,
    bench_queue_backpressure,
    bench_memory_usage,
    bench_event_types,
);

criterion_main!(event_bridge_benches);
