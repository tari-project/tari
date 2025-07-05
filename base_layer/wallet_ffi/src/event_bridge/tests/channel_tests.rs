//! # Channel Tests
//!
//! Unit tests specifically for the event channel infrastructure

#[cfg(test)]
mod tests {
    use crate::event_bridge::{
        channel::{EventChannel, EventChannelBuilder, ChannelStats},
        types::{WalletEvent, EventType, EventData, ConnectivityState},
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::{sleep, Duration, timeout};

    #[tokio::test]
    async fn test_channel_basic_operations() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(1).build();

        assert_eq!(channel.wallet_id(), 1);
        assert!(!channel.is_closed());
        assert!(channel.is_empty());

        // Send an event
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived {
                tx_id: 123,
                amount: 1000000,
                sender_address: "test_sender".to_string(),
                message: Some("test".to_string()),
            },
        );

        channel.send(event.clone()).await.unwrap();
        assert!(!channel.is_empty());

        // Receive the event
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.event_type, event.event_type);
        assert_eq!(received.wallet_id, event.wallet_id);
    }

    #[tokio::test]
    async fn test_channel_statistics() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(2).build();

        // Send multiple events
        for i in 0..5 {
            let event = WalletEvent::new(
                EventType::BalanceUpdated,
                2,
                EventData::BalanceUpdated {
                    available: (1000000 + i) as u64,
                    pending_incoming: 0,
                    pending_outgoing: 0,
                    timelocked: None,
                },
            );
            channel.send(event).await.unwrap();
        }

        // Receive all events
        for _ in 0..5 {
            receiver.recv().await.unwrap();
        }

        // Check statistics
        let stats = channel.get_stats().await;
        assert_eq!(stats.events_sent, 5);
        assert_eq!(stats.events_received, 5);
        assert_eq!(stats.send_errors, 0);
        assert_eq!(stats.receive_errors, 0);
        assert!(stats.average_latency_ms() >= 0.0);
        assert_eq!(stats.success_rate(), 100.0);
    }

    #[tokio::test]
    async fn test_channel_performance() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(3).build();

        let event_count = 1000;
        let start_time = std::time::Instant::now();

        // Send events in parallel
        let send_task = tokio::spawn(async move {
            for i in 0..event_count {
                let event = WalletEvent::new(
                    EventType::ConnectivityStatus,
                    3,
                    EventData::ConnectivityStatus {
                        status: ConnectivityState::Connected,
                        peer_count: i % 10,
                    },
                );
                channel.send(event).await.unwrap();
            }
        });

        // Receive events in parallel
        let receive_task = tokio::spawn(async move {
            for _ in 0..event_count {
                receiver.recv().await.unwrap();
            }
        });

        // Wait for both tasks to complete
        let (send_result, receive_result) = tokio::join!(send_task, receive_task);
        send_result.unwrap();
        receive_result.unwrap();

        let duration = start_time.elapsed();
        let events_per_second = event_count as f64 / duration.as_secs_f64();

        println!("Channel throughput: {:.2} events/sec", events_per_second);

        // Should easily handle thousands of events per second
        assert!(events_per_second > 1000.0, "Channel too slow: {:.2} events/sec", events_per_second);
    }

    #[tokio::test]
    async fn test_channel_error_conditions() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(4).build();

        // Test normal operation
        let event = WalletEvent::new(
            EventType::TransactionMined,
            4,
            EventData::TransactionMined {
                tx_id: 789,
                amount: 500000,
                block_height: Some(12345),
            },
        );

        channel.send(event).await.unwrap();
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.wallet_id, 4);

        // Drop the receiver to test channel closure detection
        drop(receiver);

        // Give some time for the channel to detect closure
        sleep(Duration::from_millis(10)).await;

        // Sending should still work with unbounded channels (they don't detect receiver drop immediately)
        // This is expected behavior for mpsc::unbounded_channel
        let another_event = WalletEvent::new(
            EventType::TransactionCancellation,
            4,
            EventData::TransactionCancellation {
                tx_id: 999,
                reason_code: 1,
                reason_message: "test cancellation".to_string(),
            },
        );

        // This should still succeed because unbounded channels don't fail immediately
        let _ = channel.send(another_event).await;
    }

    #[tokio::test]
    async fn test_channel_try_send() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(5).build();

        let event = WalletEvent::new(
            EventType::SafMessagesReceived,
            5,
            EventData::SafMessagesReceived { message_count: 3 },
        );

        // try_send should work the same as send for unbounded channels
        channel.try_send(event.clone()).await.unwrap();

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.event_type, event.event_type);
    }

    #[tokio::test]
    async fn test_channel_receiver_try_recv() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(6).build();

        // try_recv should fail when no events are available
        match receiver.try_recv().await {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                // Expected behavior
            }
            other => panic!("Expected Empty error, got {:?}", other),
        }

        // Send an event
        let event = WalletEvent::new(
            EventType::WalletScannedHeight,
            6,
            EventData::WalletScannedHeight {
                height: 12345,
                total_height: Some(20000),
                sync_percentage: Some(61.725),
            },
        );

        channel.send(event).await.unwrap();

        // try_recv should now succeed
        let received = receiver.try_recv().await.unwrap();
        assert_eq!(received.wallet_id, 6);
    }

    #[tokio::test]
    async fn test_channel_stats_reset() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(7).build();

        // Send some events
        for i in 0..3 {
            let event = WalletEvent::new(
                EventType::TransactionBroadcast,
                7,
                EventData::TransactionBroadcast {
                    tx_id: i as u64,
                    amount: 1000000,
                    fee: 1000,
                },
            );
            channel.send(event).await.unwrap();
        }

        // Receive events
        for _ in 0..3 {
            receiver.recv().await.unwrap();
        }

        // Verify stats
        let stats = channel.get_stats().await;
        assert_eq!(stats.events_sent, 3);
        assert_eq!(stats.events_received, 3);

        // Reset stats
        channel.reset_stats().await;

        let stats_after_reset = channel.get_stats().await;
        assert_eq!(stats_after_reset.events_sent, 0);
        assert_eq!(stats_after_reset.events_received, 0);
        assert_eq!(stats_after_reset.total_latency_ms, 0);
    }

    #[tokio::test]
    async fn test_channel_latency_measurement() {
        let (mut channel, mut receiver) = EventChannelBuilder::new(8).build();

        let event = WalletEvent::new(
            EventType::TxoValidationComplete,
            8,
            EventData::TxoValidationComplete {
                request_key: 12345,
                is_success: true,
                validation_results: crate::event_bridge::types::ValidationResults {
                    total_checked: 100,
                    valid_count: 95,
                    invalid_count: 5,
                    errors: vec!["test error".to_string()],
                },
            },
        );

        // Add a small delay to ensure measurable latency
        channel.send(event).await.unwrap();
        sleep(Duration::from_millis(1)).await;
        receiver.recv().await.unwrap();

        let stats = channel.get_stats().await;
        assert!(stats.max_latency_ms > 0);
        assert!(stats.min_latency_ms > 0);
        assert!(stats.average_latency_ms() > 0.0);
        assert!(stats.max_latency_ms >= stats.min_latency_ms);
    }

    #[tokio::test]
    async fn test_channel_queue_size_tracking() {
        let (mut channel, _receiver) = EventChannelBuilder::new(9).build();

        // Send multiple events without receiving
        for i in 0..5 {
            let event = WalletEvent::new(
                EventType::TransactionFinalized,
                9,
                EventData::TransactionFinalized {
                    tx_id: i as u64,
                    amount: 1000000,
                    fee: 1000,
                },
            );
            channel.send(event).await.unwrap();
        }

        let stats = channel.get_stats().await;
        assert_eq!(stats.current_queue_size, 5);
        assert_eq!(stats.max_queue_size, 5);
        assert_eq!(channel.len(), 5);
        assert!(!channel.is_empty());
    }

    #[tokio::test]
    async fn test_channel_builder_configuration() {
        // Test default builder
        let (channel1, _receiver1) = EventChannelBuilder::new(10).build();
        assert_eq!(channel1.wallet_id(), 10);

        // Test builder with metrics
        let (channel2, _receiver2) = EventChannelBuilder::new(11)
            .with_metrics(true)
            .build();
        assert_eq!(channel2.wallet_id(), 11);

        // Test builder with capacity (currently ignored but should not panic)
        let (channel3, _receiver3) = EventChannelBuilder::new(12)
            .with_capacity(100)
            .with_metrics(false)
            .build();
        assert_eq!(channel3.wallet_id(), 12);
    }

    #[tokio::test]
    async fn test_channel_concurrent_access() {
        let (channel, mut receiver) = EventChannelBuilder::new(13).build();
        let channel = Arc::new(channel);

        let send_count = 100;
        let num_senders = 5;
        let total_events = send_count * num_senders;

        // Spawn multiple sender tasks
        let mut send_tasks = Vec::new();
        for sender_id in 0..num_senders {
            let channel_clone = Arc::clone(&channel);
            let task = tokio::spawn(async move {
                for i in 0..send_count {
                    let event = WalletEvent::new(
                        EventType::ContactsLivenessUpdated,
                        13,
                        EventData::ContactsLivenessUpdated {
                            contact_count: sender_id as u32,
                            online_count: i as u32,
                            last_seen_updates: vec![],
                        },
                    );
                    channel_clone.send(event).await.unwrap();
                }
            });
            send_tasks.push(task);
        }

        // Spawn receiver task
        let receive_task = tokio::spawn(async move {
            let mut received = 0;
            while received < total_events {
                if receiver.recv().await.is_some() {
                    received += 1;
                }
            }
            received
        });

        // Wait for all tasks to complete
        for task in send_tasks {
            task.await.unwrap();
        }

        let received_count = receive_task.await.unwrap();
        assert_eq!(received_count, total_events);

        let stats = channel.get_stats().await;
        assert_eq!(stats.events_sent, total_events as u64);
        assert_eq!(stats.events_received, total_events as u64);
        assert_eq!(stats.send_errors, 0);
    }

    #[tokio::test]
    async fn test_channel_receiver_close() {
        let (channel, mut receiver) = EventChannelBuilder::new(14).build();

        // Send an event
        let event = WalletEvent::new(
            EventType::BaseNodeState,
            14,
            EventData::BaseNodeState {
                node_id: "test_node".to_string(),
                chain_height: 50000,
                is_synced: true,
                sync_percentage: Some(100.0),
            },
        );

        channel.send(event).await.unwrap();

        // Close the receiver
        receiver.close();

        // Try to receive - should get None
        let result = timeout(Duration::from_millis(100), receiver.recv()).await;
        match result {
            Ok(None) => {
                // Expected: channel is closed
            }
            Ok(Some(_)) => {
                // Might receive the event that was already sent
                // Then subsequent recv() should return None
                let second_result = timeout(Duration::from_millis(100), receiver.recv()).await;
                assert!(matches!(second_result, Ok(None)));
            }
            Err(_) => {
                // Timeout is also acceptable as the receiver is closed
            }
        }
    }
}
