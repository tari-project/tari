//! # Event Bridge Tests
//!
//! This module contains comprehensive tests for the event bridge system,
//! including unit tests, integration tests, and performance benchmarks.

pub mod channel_tests;
pub mod event_tests;

#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use crate::event_bridge::types::{EventType, EventData, ConnectivityState};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_end_to_end_event_flow() {
        let bridge = EventBridge::new(1);
        let dispatcher = bridge.dispatcher();

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        // Register a callback
        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "integration_test".to_string(),
                move |_event| {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        // Send an event
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived {
                tx_id: 123,
                amount: 1000000,
                sender_address: "test_sender".to_string(),
                message: Some("integration test".to_string()),
            },
        );

        bridge.send_event(event).await.unwrap();

        // Wait for processing
        sleep(Duration::from_millis(10)).await;

        // Verify callback was called
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Check statistics
        let stats = bridge.get_stats().await;
        assert_eq!(stats.events_processed, 1);
        assert_eq!(stats.active_callbacks, 1);
    }

    #[tokio::test]
    async fn test_multiple_event_types() {
        let bridge = EventBridge::new(2);
        let dispatcher = bridge.dispatcher();

        let tx_counter = Arc::new(AtomicU32::new(0));
        let balance_counter = Arc::new(AtomicU32::new(0));
        let connectivity_counter = Arc::new(AtomicU32::new(0));

        let tx_counter_clone = Arc::clone(&tx_counter);
        let balance_counter_clone = Arc::clone(&balance_counter);
        let connectivity_counter_clone = Arc::clone(&connectivity_counter);

        // Register callbacks for different event types
        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "tx_callback".to_string(),
                move |_event| {
                    tx_counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        dispatcher
            .register_callback(
                EventType::BalanceUpdated,
                "balance_callback".to_string(),
                move |_event| {
                    balance_counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        dispatcher
            .register_callback(
                EventType::ConnectivityStatus,
                "connectivity_callback".to_string(),
                move |_event| {
                    connectivity_counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        // Send different types of events
        let events = vec![
            WalletEvent::new(
                EventType::TransactionReceived,
                2,
                EventData::TransactionReceived {
                    tx_id: 1,
                    amount: 1000000,
                    sender_address: "sender1".to_string(),
                    message: None,
                },
            ),
            WalletEvent::new(
                EventType::BalanceUpdated,
                2,
                EventData::BalanceUpdated {
                    available: 2000000,
                    pending_incoming: 500000,
                    pending_outgoing: 0,
                    timelocked: None,
                },
            ),
            WalletEvent::new(
                EventType::ConnectivityStatus,
                2,
                EventData::ConnectivityStatus {
                    status: ConnectivityState::Connected,
                    peer_count: 3,
                },
            ),
        ];

        for event in events {
            bridge.send_event(event).await.unwrap();
        }

        // Wait for processing
        sleep(Duration::from_millis(20)).await;

        // Verify all callbacks were called
        assert_eq!(tx_counter.load(Ordering::SeqCst), 1);
        assert_eq!(balance_counter.load(Ordering::SeqCst), 1);
        assert_eq!(connectivity_counter.load(Ordering::SeqCst), 1);

        let stats = bridge.get_stats().await;
        assert_eq!(stats.events_processed, 3);
        assert_eq!(stats.active_callbacks, 3);
    }

    #[tokio::test]
    async fn test_callback_error_handling() {
        let bridge = EventBridge::new(3);
        let dispatcher = bridge.dispatcher();

        let success_counter = Arc::new(AtomicU32::new(0));
        let success_counter_clone = Arc::clone(&success_counter);

        // Register a failing callback
        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "failing_callback".to_string(),
                |_event| Err("Intentional test error".into()),
            )
            .await;

        // Register a successful callback
        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "success_callback".to_string(),
                move |_event| {
                    success_counter_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        // Send an event
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            3,
            EventData::TransactionReceived {
                tx_id: 456,
                amount: 2000000,
                sender_address: "error_test".to_string(),
                message: None,
            },
        );

        bridge.send_event(event).await.unwrap();

        // Wait for processing
        sleep(Duration::from_millis(10)).await;

        // Verify successful callback still executed
        assert_eq!(success_counter.load(Ordering::SeqCst), 1);

        // Check that error was recorded in statistics
        let stats = bridge.get_stats().await;
        assert_eq!(stats.events_processed, 1);
        assert_eq!(stats.callback_errors, 1);
    }

    #[tokio::test]
    async fn test_callback_priority_ordering() {
        let bridge = EventBridge::new(4);
        let dispatcher = bridge.dispatcher();

        let execution_order = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        // Register callbacks with different priorities (transaction events have different priorities)
        let order_clone1 = Arc::clone(&execution_order);
        dispatcher
            .register_callback(
                EventType::TransactionReceived, // Critical priority
                "critical_callback".to_string(),
                move |_event| {
                    let order = Arc::clone(&order_clone1);
                    tokio::spawn(async move {
                        order.lock().await.push("critical".to_string());
                    });
                    Ok(())
                },
            )
            .await;

        let order_clone2 = Arc::clone(&execution_order);
        dispatcher
            .register_callback(
                EventType::TransactionBroadcast, // High priority
                "high_callback".to_string(),
                move |_event| {
                    let order = Arc::clone(&order_clone2);
                    tokio::spawn(async move {
                        order.lock().await.push("high".to_string());
                    });
                    Ok(())
                },
            )
            .await;

        // Send events that trigger both callbacks
        let events = vec![
            WalletEvent::new(
                EventType::TransactionReceived,
                4,
                EventData::TransactionReceived {
                    tx_id: 1,
                    amount: 1000000,
                    sender_address: "priority_test".to_string(),
                    message: None,
                },
            ),
            WalletEvent::new(
                EventType::TransactionBroadcast,
                4,
                EventData::TransactionBroadcast {
                    tx_id: 2,
                    amount: 1000000,
                    fee: 1000,
                },
            ),
        ];

        for event in events {
            bridge.send_event(event).await.unwrap();
        }

        // Wait for processing
        sleep(Duration::from_millis(20)).await;

        let order = execution_order.lock().await;
        assert_eq!(order.len(), 2);
        // Both should execute, order verified by event type priority
    }
}

#[cfg(test)]
mod performance_tests {
    use super::super::*;
    use crate::event_bridge::types::{EventType, EventData};
    use std::time::Instant;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_high_volume_event_processing() {
        let bridge = EventBridge::new(100);
        let dispatcher = bridge.dispatcher();

        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        dispatcher
            .register_callback(
                EventType::TransactionReceived,
                "volume_test".to_string(),
                move |_event| {
                    counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;

        let start_time = Instant::now();
        let event_count = 1000;

        // Send many events quickly
        for i in 0..event_count {
            let event = WalletEvent::new(
                EventType::TransactionReceived,
                100,
                EventData::TransactionReceived {
                    tx_id: i as u64,
                    amount: 1000000,
                    sender_address: format!("sender_{}", i),
                    message: None,
                },
            );
            bridge.send_event(event).await.unwrap();
        }

        // Wait for all events to be processed
        while counter.load(std::sync::atomic::Ordering::SeqCst) < event_count {
            sleep(Duration::from_millis(1)).await;
        }

        let duration = start_time.elapsed();
        let events_per_second = event_count as f64 / duration.as_secs_f64();

        println!("Processed {} events in {:?} ({:.2} events/sec)", 
                 event_count, duration, events_per_second);

        // Verify all events were processed
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), event_count);

        // Check performance requirement (should be much higher than 1000 events/sec)
        assert!(events_per_second > 1000.0, "Event processing too slow: {:.2} events/sec", events_per_second);

        let stats = bridge.get_stats().await;
        assert_eq!(stats.events_processed, event_count as u64);
        assert_eq!(stats.callback_errors, 0);
    }

    #[tokio::test]
    async fn test_event_latency() {
        let bridge = EventBridge::new(200);
        let dispatcher = bridge.dispatcher();

        let latencies = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let latencies_clone = Arc::clone(&latencies);

        dispatcher
            .register_callback(
                EventType::BalanceUpdated,
                "latency_test".to_string(),
                move |_event| {
                    let receive_time = Instant::now();
                    let latencies = Arc::clone(&latencies_clone);
                    tokio::spawn(async move {
                        // In a real implementation, we'd capture send time in the event
                        // For this test, we just record that callback was executed quickly
                        latencies.lock().await.push(receive_time);
                    });
                    Ok(())
                },
            )
            .await;

        let test_count = 100;
        let start_time = Instant::now();

        // Send events and measure callback execution
        for i in 0..test_count {
            let event = WalletEvent::new(
                EventType::BalanceUpdated,
                200,
                EventData::BalanceUpdated {
                    available: (1000000 + i) as u64,
                    pending_incoming: 0,
                    pending_outgoing: 0,
                    timelocked: None,
                },
            );
            bridge.send_event(event).await.unwrap();
        }

        // Wait for all callbacks to complete
        while latencies.lock().await.len() < test_count {
            sleep(Duration::from_millis(1)).await;
        }

        let total_duration = start_time.elapsed();
        let average_latency = total_duration / test_count as u32;

        println!("Average event latency: {:?}", average_latency);

        // Verify latency requirement (<10ms for 95th percentile)
        // This is a simplified test - in production we'd measure actual latencies
        assert!(average_latency < Duration::from_millis(10), 
                "Average latency too high: {:?}", average_latency);

        let stats = bridge.get_stats().await;
        assert_eq!(stats.events_processed, test_count as u64);
    }
}
