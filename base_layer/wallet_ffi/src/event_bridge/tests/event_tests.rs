//! # Event Type Tests
//!
//! Unit tests specifically for event types, serialization, and event data structures

#[cfg(test)]
mod tests {
    use crate::event_bridge::types::{
        WalletEvent, EventType, EventData, EventCategory, EventPriority,
        ConnectivityState, ValidationResults, ContactUpdate,
    };
    use crate::event_bridge::serialization::{
        serialize_event, deserialize_event, SerializationFormat,
        serialize_event_to_json, deserialize_event_from_json,
        EventSerializer, BatchSerializer,
    };
    use std::time::SystemTime;

    fn create_sample_events() -> Vec<WalletEvent> {
        vec![
            WalletEvent::new(
                EventType::TransactionReceived,
                1,
                EventData::TransactionReceived {
                    tx_id: 123,
                    amount: 1000000,
                    sender_address: "sender_123".to_string(),
                    message: Some("Payment for services".to_string()),
                },
            ),
            WalletEvent::new(
                EventType::BalanceUpdated,
                1,
                EventData::BalanceUpdated {
                    available: 5000000,
                    pending_incoming: 1000000,
                    pending_outgoing: 500000,
                    timelocked: Some(2000000),
                },
            ),
            WalletEvent::new(
                EventType::ConnectivityStatus,
                1,
                EventData::ConnectivityStatus {
                    status: ConnectivityState::Connected,
                    peer_count: 8,
                },
            ),
            WalletEvent::new(
                EventType::TransactionMined,
                1,
                EventData::TransactionMined {
                    tx_id: 456,
                    amount: 2000000,
                    block_height: Some(12345),
                },
            ),
            WalletEvent::new(
                EventType::TxoValidationComplete,
                1,
                EventData::TxoValidationComplete {
                    request_key: 789,
                    is_success: true,
                    validation_results: ValidationResults {
                        total_checked: 100,
                        valid_count: 95,
                        invalid_count: 5,
                        errors: vec!["Error 1".to_string(), "Error 2".to_string()],
                    },
                },
            ),
        ]
    }

    #[test]
    fn test_event_creation_and_properties() {
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            42,
            EventData::TransactionReceived {
                tx_id: 100,
                amount: 50000,
                sender_address: "test_sender".to_string(),
                message: None,
            },
        );

        assert_eq!(event.event_type, EventType::TransactionReceived);
        assert_eq!(event.wallet_id, 42);
        assert_eq!(event.event_name(), "transaction_received");
        
        match event.data {
            EventData::TransactionReceived { tx_id, amount, .. } => {
                assert_eq!(tx_id, 100);
                assert_eq!(amount, 50000);
            }
            _ => panic!("Expected TransactionReceived event data"),
        }
    }

    #[test]
    fn test_event_type_categorization() {
        // Test transaction events
        assert_eq!(EventType::TransactionReceived.category(), EventCategory::Transaction);
        assert_eq!(EventType::TransactionBroadcast.category(), EventCategory::Transaction);
        assert_eq!(EventType::TransactionMined.category(), EventCategory::Transaction);

        // Test balance events
        assert_eq!(EventType::BalanceUpdated.category(), EventCategory::Balance);

        // Test connection events
        assert_eq!(EventType::ConnectivityStatus.category(), EventCategory::Connection);
        assert_eq!(EventType::BaseNodeState.category(), EventCategory::Connection);

        // Test validation events
        assert_eq!(EventType::TxoValidationComplete.category(), EventCategory::Validation);
        assert_eq!(EventType::TransactionValidationComplete.category(), EventCategory::Validation);

        // Test communication events
        assert_eq!(EventType::ContactsLivenessUpdated.category(), EventCategory::Communication);
        assert_eq!(EventType::SafMessagesReceived.category(), EventCategory::Communication);

        // Test scanning events
        assert_eq!(EventType::WalletScannedHeight.category(), EventCategory::Scanning);
    }

    #[test]
    fn test_event_type_priorities() {
        // Critical priority events
        assert_eq!(EventType::TransactionReceived.priority(), EventPriority::Critical);
        assert_eq!(EventType::TransactionMined.priority(), EventPriority::Critical);
        assert_eq!(EventType::TransactionCancellation.priority(), EventPriority::Critical);

        // High priority events
        assert_eq!(EventType::TransactionBroadcast.priority(), EventPriority::High);
        assert_eq!(EventType::TransactionFinalized.priority(), EventPriority::High);
        assert_eq!(EventType::BalanceUpdated.priority(), EventPriority::High);

        // Medium priority events
        assert_eq!(EventType::TransactionReply.priority(), EventPriority::Medium);
        assert_eq!(EventType::ConnectivityStatus.priority(), EventPriority::Medium);

        // Low priority events
        assert_eq!(EventType::ContactsLivenessUpdated.priority(), EventPriority::Low);
        assert_eq!(EventType::WalletScannedHeight.priority(), EventPriority::Low);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(EventPriority::Critical > EventPriority::High);
        assert!(EventPriority::High > EventPriority::Medium);
        assert!(EventPriority::Medium > EventPriority::Low);

        let mut priorities = vec![
            EventPriority::Low,
            EventPriority::Critical,
            EventPriority::Medium,
            EventPriority::High,
        ];
        priorities.sort();

        assert_eq!(priorities, vec![
            EventPriority::Low,
            EventPriority::Medium,
            EventPriority::High,
            EventPriority::Critical,
        ]);
    }

    #[test]
    fn test_connectivity_state_conversion() {
        // Test From<u64> implementation
        assert_eq!(ConnectivityState::from(0), ConnectivityState::Disconnected);
        assert_eq!(ConnectivityState::from(1), ConnectivityState::Connecting);
        assert_eq!(ConnectivityState::from(2), ConnectivityState::Connected);
        assert_eq!(ConnectivityState::from(3), ConnectivityState::Synchronizing);
        assert_eq!(ConnectivityState::from(4), ConnectivityState::Synchronized);
        assert_eq!(ConnectivityState::from(999), ConnectivityState::Disconnected); // Unknown values default to Disconnected

        // Test Into<u64> implementation
        let state = ConnectivityState::Connected;
        let status: u64 = state.into();
        assert_eq!(status, 2);

        let state = ConnectivityState::Synchronized;
        let status: u64 = state.into();
        assert_eq!(status, 4);
    }

    #[test]
    fn test_event_serialization_json() {
        let event = WalletEvent::new(
            EventType::BalanceUpdated,
            1,
            EventData::BalanceUpdated {
                available: 1000000,
                pending_incoming: 500000,
                pending_outgoing: 200000,
                timelocked: Some(300000),
            },
        );

        // Test JSON serialization
        let json_bytes = serialize_event(&event, SerializationFormat::Json).unwrap();
        let json_str = String::from_utf8(json_bytes).unwrap();

        assert!(json_str.contains("BalanceUpdated"));
        assert!(json_str.contains("1000000"));
        assert!(json_str.contains("500000"));

        // Test deserialization
        let deserialized = deserialize_event(json_str.as_bytes(), SerializationFormat::Json).unwrap();
        assert_eq!(deserialized.event_type, event.event_type);
        assert_eq!(deserialized.wallet_id, event.wallet_id);

        match (event.data, deserialized.data) {
            (EventData::BalanceUpdated { available: a1, .. }, 
             EventData::BalanceUpdated { available: a2, .. }) => {
                assert_eq!(a1, a2);
            }
            _ => panic!("Event data mismatch after serialization"),
        }
    }

    #[test]
    fn test_event_serialization_pretty_json() {
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived {
                tx_id: 123,
                amount: 1000000,
                sender_address: "test_address".to_string(),
                message: Some("test message".to_string()),
            },
        );

        let pretty_bytes = serialize_event(&event, SerializationFormat::JsonPretty).unwrap();
        let pretty_str = String::from_utf8(pretty_bytes).unwrap();

        // Pretty JSON should have newlines and indentation
        assert!(pretty_str.contains('\n'));
        assert!(pretty_str.contains("  ")); // Indentation
        assert!(pretty_str.contains("TransactionReceived"));
    }

    #[test]
    fn test_convenience_serialization_functions() {
        let event = WalletEvent::new(
            EventType::ConnectivityStatus,
            1,
            EventData::ConnectivityStatus {
                status: ConnectivityState::Connected,
                peer_count: 5,
            },
        );

        // Test convenience functions
        let json = serialize_event_to_json(&event).unwrap();
        let deserialized = deserialize_event_from_json(&json).unwrap();

        assert_eq!(deserialized.event_type, event.event_type);
        assert_eq!(deserialized.wallet_id, event.wallet_id);
    }

    #[test]
    fn test_event_serializer() {
        let event = WalletEvent::new(
            EventType::TransactionMined,
            1,
            EventData::TransactionMined {
                tx_id: 789,
                amount: 2000000,
                block_height: Some(12345),
            },
        );

        let serializer = EventSerializer::new(SerializationFormat::Json);
        let serialized = serializer.serialize(&event).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();

        assert_eq!(deserialized.event_type, event.event_type);
        assert_eq!(deserialized.wallet_id, event.wallet_id);
    }

    #[test]
    fn test_batch_serialization() {
        let events = create_sample_events();
        let batch_serializer = BatchSerializer::new(SerializationFormat::Json);

        let serialized = batch_serializer.serialize_batch(&events).unwrap();
        let deserialized = batch_serializer.deserialize_batch(&serialized).unwrap();

        assert_eq!(deserialized.len(), events.len());
        
        for (original, deserialized) in events.iter().zip(deserialized.iter()) {
            assert_eq!(original.event_type, deserialized.event_type);
            assert_eq!(original.wallet_id, deserialized.wallet_id);
        }
    }

    #[test]
    fn test_line_delimited_batch_serialization() {
        let events = create_sample_events();
        let batch_serializer = BatchSerializer::new(SerializationFormat::Json);

        let mut buffer = Vec::new();
        batch_serializer.serialize_batch_lines(&events, &mut buffer).unwrap();

        let serialized_str = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = serialized_str.lines().collect();
        assert_eq!(lines.len(), events.len());

        // Each line should be valid JSON
        for line in lines {
            let _: WalletEvent = serde_json::from_str(line).unwrap();
        }

        // Test deserialization
        let cursor = std::io::Cursor::new(serialized_str.as_bytes());
        let deserialized = batch_serializer.deserialize_batch_lines(cursor).unwrap();

        assert_eq!(deserialized.len(), events.len());
    }

    #[test]
    fn test_complex_event_data_structures() {
        // Test event with complex nested data
        let event = WalletEvent::new(
            EventType::ContactsLivenessUpdated,
            1,
            EventData::ContactsLivenessUpdated {
                contact_count: 5,
                online_count: 3,
                last_seen_updates: vec![
                    ContactUpdate {
                        public_key: "contact_1".to_string(),
                        last_seen: Some(SystemTime::now()),
                        is_online: true,
                    },
                    ContactUpdate {
                        public_key: "contact_2".to_string(),
                        last_seen: None,
                        is_online: false,
                    },
                ],
            },
        );

        // Test serialization round-trip
        let json = serialize_event_to_json(&event).unwrap();
        let deserialized = deserialize_event_from_json(&json).unwrap();

        assert_eq!(deserialized.event_type, event.event_type);
        
        match deserialized.data {
            EventData::ContactsLivenessUpdated { contact_count, online_count, last_seen_updates } => {
                assert_eq!(contact_count, 5);
                assert_eq!(online_count, 3);
                assert_eq!(last_seen_updates.len(), 2);
                assert_eq!(last_seen_updates[0].public_key, "contact_1");
                assert_eq!(last_seen_updates[1].public_key, "contact_2");
            }
            _ => panic!("Expected ContactsLivenessUpdated event data"),
        }
    }

    #[test]
    fn test_validation_results_structure() {
        let validation_results = ValidationResults {
            total_checked: 1000,
            valid_count: 950,
            invalid_count: 50,
            errors: vec![
                "Invalid signature".to_string(),
                "Missing input".to_string(),
                "Double spend detected".to_string(),
            ],
        };

        let event = WalletEvent::new(
            EventType::TxoValidationComplete,
            1,
            EventData::TxoValidationComplete {
                request_key: 12345,
                is_success: false, // Some errors found
                validation_results,
            },
        );

        // Test serialization
        let json = serialize_event_to_json(&event).unwrap();
        let deserialized = deserialize_event_from_json(&json).unwrap();

        match deserialized.data {
            EventData::TxoValidationComplete { validation_results, is_success, .. } => {
                assert!(!is_success);
                assert_eq!(validation_results.total_checked, 1000);
                assert_eq!(validation_results.valid_count, 950);
                assert_eq!(validation_results.invalid_count, 50);
                assert_eq!(validation_results.errors.len(), 3);
                assert!(validation_results.errors.contains(&"Invalid signature".to_string()));
            }
            _ => panic!("Expected TxoValidationComplete event data"),
        }
    }

    #[test]
    fn test_all_event_types_serialization() {
        // Test that all event types can be serialized and deserialized correctly
        let events = vec![
            // Transaction events
            WalletEvent::new(EventType::TransactionReceived, 1, EventData::TransactionReceived {
                tx_id: 1, amount: 1000000, sender_address: "addr1".to_string(), message: None,
            }),
            WalletEvent::new(EventType::TransactionReply, 1, EventData::TransactionReply {
                tx_id: 2, amount: 2000000, is_success: true,
            }),
            WalletEvent::new(EventType::TransactionFinalized, 1, EventData::TransactionFinalized {
                tx_id: 3, amount: 3000000, fee: 1000,
            }),
            WalletEvent::new(EventType::TransactionBroadcast, 1, EventData::TransactionBroadcast {
                tx_id: 4, amount: 4000000, fee: 1000,
            }),
            WalletEvent::new(EventType::TransactionMined, 1, EventData::TransactionMined {
                tx_id: 5, amount: 5000000, block_height: Some(100),
            }),
            WalletEvent::new(EventType::TransactionMinedUnconfirmed, 1, EventData::TransactionMinedUnconfirmed {
                tx_id: 6, amount: 6000000, confirmations: 1,
            }),
            WalletEvent::new(EventType::FauxTransactionConfirmed, 1, EventData::FauxTransactionConfirmed {
                tx_id: 7, amount: 7000000,
            }),
            WalletEvent::new(EventType::FauxTransactionUnconfirmed, 1, EventData::FauxTransactionUnconfirmed {
                tx_id: 8, amount: 8000000, confirmations: 0,
            }),
            WalletEvent::new(EventType::TransactionSendResult, 1, EventData::TransactionSendResult {
                tx_id: 9, is_success: true, failure_reason: None,
            }),
            WalletEvent::new(EventType::TransactionCancellation, 1, EventData::TransactionCancellation {
                tx_id: 10, reason_code: 1, reason_message: "User cancelled".to_string(),
            }),

            // Other event types
            WalletEvent::new(EventType::BalanceUpdated, 1, EventData::BalanceUpdated {
                available: 1000000, pending_incoming: 0, pending_outgoing: 0, timelocked: None,
            }),
            WalletEvent::new(EventType::ConnectivityStatus, 1, EventData::ConnectivityStatus {
                status: ConnectivityState::Connected, peer_count: 5,
            }),
            WalletEvent::new(EventType::SafMessagesReceived, 1, EventData::SafMessagesReceived {
                message_count: 3,
            }),
            WalletEvent::new(EventType::WalletScannedHeight, 1, EventData::WalletScannedHeight {
                height: 50000, total_height: Some(60000), sync_percentage: Some(83.33),
            }),
            WalletEvent::new(EventType::BaseNodeState, 1, EventData::BaseNodeState {
                node_id: "test_node".to_string(), chain_height: 60000, is_synced: true, sync_percentage: Some(100.0),
            }),
        ];

        // Test each event type
        for event in events {
            let json = serialize_event_to_json(&event).unwrap();
            let deserialized = deserialize_event_from_json(&json).unwrap();

            assert_eq!(deserialized.event_type, event.event_type);
            assert_eq!(deserialized.wallet_id, event.wallet_id);
            
            // Verify the event name matches the type
            assert_eq!(event.event_name(), deserialized.event_name());
        }
    }

    #[test]
    fn test_event_timestamps() {
        let before_creation = SystemTime::now();
        
        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived {
                tx_id: 123,
                amount: 1000000,
                sender_address: "test".to_string(),
                message: None,
            },
        );

        let after_creation = SystemTime::now();

        // Timestamp should be between before and after creation
        assert!(event.timestamp >= before_creation);
        assert!(event.timestamp <= after_creation);

        // Test timestamp serialization
        let json = serialize_event_to_json(&event).unwrap();
        let deserialized = deserialize_event_from_json(&json).unwrap();

        // Timestamps should be preserved (within reasonable precision)
        let original_secs = event.timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        let deserialized_secs = deserialized.timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        
        // Should be within 1 second (accounting for serialization precision)
        assert!((original_secs as i64 - deserialized_secs as i64).abs() <= 1);
    }
}
