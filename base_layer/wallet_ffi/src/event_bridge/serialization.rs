//! # Event Serialization
//!
//! This module provides efficient serialization and deserialization for wallet events.
//! It supports both binary (bincode) and JSON formats for different use cases:
//! - Binary for high-performance internal usage
//! - JSON for debugging, logging, and human readability

use super::types::WalletEvent;
use serde_json;
use std::io::{Read, Write};

/// Serialization format options
#[derive(Debug, Clone, Copy)]
pub enum SerializationFormat {
    /// Binary format using bincode (high performance)
    Binary,
    /// JSON format (human readable, debugging)
    Json,
    /// Pretty JSON format (formatted for readability)
    JsonPretty,
}

/// Serialization error types
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Binary serialization error: {0}")]
    Binary(#[from] bincode::Error),
    
    #[error("Unsupported format: {0:?}")]
    UnsupportedFormat(SerializationFormat),
}

/// Serialize a wallet event to bytes using the specified format
pub fn serialize_event(event: &WalletEvent, format: SerializationFormat) -> Result<Vec<u8>, SerializationError> {
    match format {
        SerializationFormat::Binary => {
            let encoded = bincode::serialize(event)?;
            Ok(encoded)
        }
        SerializationFormat::Json => {
            let json = serde_json::to_string(event)?;
            Ok(json.into_bytes())
        }
        SerializationFormat::JsonPretty => {
            let json = serde_json::to_string_pretty(event)?;
            Ok(json.into_bytes())
        }
    }
}

/// Deserialize a wallet event from bytes using the specified format
pub fn deserialize_event(data: &[u8], format: SerializationFormat) -> Result<WalletEvent, SerializationError> {
    match format {
        SerializationFormat::Binary => {
            let event = bincode::deserialize(data)?;
            Ok(event)
        }
        SerializationFormat::Json | SerializationFormat::JsonPretty => {
            let json_str = std::str::from_utf8(data)
                .map_err(|e| SerializationError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
            let event = serde_json::from_str(json_str)?;
            Ok(event)
        }
    }
}

/// Serialize a wallet event to a writer
pub fn serialize_event_to_writer<W: Write>(
    event: &WalletEvent,
    mut writer: W,
    format: SerializationFormat,
) -> Result<(), SerializationError> {
    match format {
        SerializationFormat::Binary => {
            let encoded = bincode::serialize(event)?;
            writer.write_all(&encoded)?;
            Ok(())
        }
        SerializationFormat::Json => {
            serde_json::to_writer(writer, event)?;
            Ok(())
        }
        SerializationFormat::JsonPretty => {
            serde_json::to_writer_pretty(writer, event)?;
            Ok(())
        }
    }
}

/// Deserialize a wallet event from a reader
pub fn deserialize_event_from_reader<R: Read>(
    mut reader: R,
    format: SerializationFormat,
) -> Result<WalletEvent, SerializationError> {
    match format {
        SerializationFormat::Binary => {
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer)?;
            let event = bincode::deserialize(&buffer)?;
            Ok(event)
        }
        SerializationFormat::Json | SerializationFormat::JsonPretty => {
            let event = serde_json::from_reader(reader)?;
            Ok(event)
        }
    }
}

/// Serialize a wallet event to a JSON string (convenience function)
pub fn serialize_event_to_json(event: &WalletEvent) -> Result<String, SerializationError> {
    let json = serde_json::to_string(event)?;
    Ok(json)
}

/// Serialize a wallet event to a pretty JSON string (convenience function)
pub fn serialize_event_to_pretty_json(event: &WalletEvent) -> Result<String, SerializationError> {
    let json = serde_json::to_string_pretty(event)?;
    Ok(json)
}

/// Deserialize a wallet event from a JSON string (convenience function)
pub fn deserialize_event_from_json(json: &str) -> Result<WalletEvent, SerializationError> {
    let event = serde_json::from_str(json)?;
    Ok(event)
}

/// Calculate the serialized size of an event in bytes
pub fn calculate_serialized_size(event: &WalletEvent, format: SerializationFormat) -> Result<usize, SerializationError> {
    let serialized = serialize_event(event, format)?;
    Ok(serialized.len())
}

/// Event serializer with configurable options
pub struct EventSerializer {
    format: SerializationFormat,
    compress: bool,
    include_metadata: bool,
}

impl EventSerializer {
    /// Create a new event serializer with default settings
    pub fn new(format: SerializationFormat) -> Self {
        Self {
            format,
            compress: false,
            include_metadata: true,
        }
    }

    /// Enable or disable compression (not implemented yet)
    pub fn with_compression(mut self, compress: bool) -> Self {
        self.compress = compress;
        self
    }

    /// Include or exclude metadata in serialization
    pub fn with_metadata(mut self, include: bool) -> Self {
        self.include_metadata = include;
        self
    }

    /// Serialize an event using the configured settings
    pub fn serialize(&self, event: &WalletEvent) -> Result<Vec<u8>, SerializationError> {
        // For now, ignore compression and metadata settings
        // TODO: Implement compression and metadata filtering
        serialize_event(event, self.format)
    }

    /// Deserialize an event using the configured settings
    pub fn deserialize(&self, data: &[u8]) -> Result<WalletEvent, SerializationError> {
        deserialize_event(data, self.format)
    }
}

impl Default for EventSerializer {
    fn default() -> Self {
        Self::new(SerializationFormat::Json)
    }
}

/// Batch serialization utilities for handling multiple events
pub struct BatchSerializer {
    format: SerializationFormat,
}

impl BatchSerializer {
    /// Create a new batch serializer
    pub fn new(format: SerializationFormat) -> Self {
        Self { format }
    }

    /// Serialize multiple events to a single byte vector
    pub fn serialize_batch(&self, events: &[WalletEvent]) -> Result<Vec<u8>, SerializationError> {
        match self.format {
            SerializationFormat::Json => {
                let json = serde_json::to_string(events)?;
                Ok(json.into_bytes())
            }
            SerializationFormat::JsonPretty => {
                let json = serde_json::to_string_pretty(events)?;
                Ok(json.into_bytes())
            }
            SerializationFormat::Binary => {
                let encoded = bincode::serialize(events)?;
                Ok(encoded)
            }
        }
    }

    /// Deserialize multiple events from a byte vector
    pub fn deserialize_batch(&self, data: &[u8]) -> Result<Vec<WalletEvent>, SerializationError> {
        match self.format {
            SerializationFormat::Json | SerializationFormat::JsonPretty => {
                let json_str = std::str::from_utf8(data)
                    .map_err(|e| SerializationError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
                let events = serde_json::from_str(json_str)?;
                Ok(events)
            }
            SerializationFormat::Binary => {
                let events = bincode::deserialize(data)?;
                Ok(events)
            }
        }
    }

    /// Serialize events to a writer with line-delimited format
    pub fn serialize_batch_lines<W: Write>(&self, events: &[WalletEvent], mut writer: W) -> Result<(), SerializationError> {
        for event in events {
            let line = serialize_event_to_json(event)?;
            writeln!(writer, "{}", line)?;
        }
        Ok(())
    }

    /// Deserialize events from a reader with line-delimited format
    pub fn deserialize_batch_lines<R: Read>(&self, reader: R) -> Result<Vec<WalletEvent>, SerializationError> {
        let mut events = Vec::new();
        let reader = std::io::BufReader::new(reader);
        
        for line in std::io::BufRead::lines(reader) {
            let line = line?;
            if !line.trim().is_empty() {
                let event = deserialize_event_from_json(&line)?;
                events.push(event);
            }
        }
        
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bridge::types::{EventType, EventData, ConnectivityState, TransactionData};

    fn create_test_event() -> WalletEvent {
        WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived(TransactionData {
                tx_id: 123,
                source_address: "test_address".to_string(),
                amount: 1000000,
                message: Some("test message".to_string()),
                timestamp: 1640995200,
                status: 1,
            }),
        )
    }

    #[test]
    fn test_json_serialization() {
        let event = create_test_event();
        
        let json_bytes = serialize_event(&event, SerializationFormat::Json).unwrap();
        let json_str = String::from_utf8(json_bytes).unwrap();
        
        assert!(json_str.contains("TransactionReceived"));
        assert!(json_str.contains("test_address"));
        assert!(json_str.contains("test message"));
    }

    #[test]
    fn test_json_deserialization() {
        let event = create_test_event();
        
        let json_bytes = serialize_event(&event, SerializationFormat::Json).unwrap();
        let deserialized = deserialize_event(&json_bytes, SerializationFormat::Json).unwrap();
        
        assert_eq!(deserialized.event_type, event.event_type);
        assert_eq!(deserialized.wallet_id, event.wallet_id);
        
        if let (EventData::TransactionReceived(TransactionData { tx_id: orig_tx_id, .. }), 
                EventData::TransactionReceived(TransactionData { tx_id: deser_tx_id, .. })) = 
                (&event.data, &deserialized.data) {
            assert_eq!(orig_tx_id, deser_tx_id);
        } else {
            panic!("Event data mismatch");
        }
    }

    #[test]
    fn test_pretty_json_serialization() {
        let event = create_test_event();
        
        let pretty_bytes = serialize_event(&event, SerializationFormat::JsonPretty).unwrap();
        let pretty_str = String::from_utf8(pretty_bytes).unwrap();
        
        // Pretty JSON should contain newlines and indentation
        assert!(pretty_str.contains('\n'));
        assert!(pretty_str.contains("  ")); // Indentation
    }

    #[test]
    fn test_convenience_functions() {
        let event = create_test_event();
        
        let json = serialize_event_to_json(&event).unwrap();
        let deserialized = deserialize_event_from_json(&json).unwrap();
        
        assert_eq!(deserialized.event_type, event.event_type);
        
        let pretty_json = serialize_event_to_pretty_json(&event).unwrap();
        assert!(pretty_json.contains('\n'));
    }

    #[test]
    fn test_event_serializer() {
        let event = create_test_event();
        let serializer = EventSerializer::new(SerializationFormat::Json);
        
        let serialized = serializer.serialize(&event).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        assert_eq!(deserialized.event_type, event.event_type);
    }

    #[test]
    fn test_batch_serialization() {
        let events = vec![
            create_test_event(),
            WalletEvent::new(
                EventType::BalanceUpdated,
                1,
                EventData::BalanceUpdated {
                    available: 2000000,
                    pending_incoming: 0,
                    pending_outgoing: 0,
                    timelocked: None,
                },
            ),
        ];
        
        let batch_serializer = BatchSerializer::new(SerializationFormat::Json);
        let serialized = batch_serializer.serialize_batch(&events).unwrap();
        let deserialized = batch_serializer.deserialize_batch(&serialized).unwrap();
        
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].event_type, EventType::TransactionReceived);
        assert_eq!(deserialized[1].event_type, EventType::BalanceUpdated);
    }

    #[test]
    fn test_line_delimited_serialization() {
        let events = vec![
            create_test_event(),
            WalletEvent::new(
                EventType::ConnectivityStatus,
                1,
                EventData::ConnectivityStatus {
                    status: ConnectivityState::Connected,
                    peer_count: 5,
                },
            ),
        ];
        
        let batch_serializer = BatchSerializer::new(SerializationFormat::Json);
        let mut buffer = Vec::new();
        
        batch_serializer.serialize_batch_lines(&events, &mut buffer).unwrap();
        
        let serialized_str = String::from_utf8(buffer).unwrap();
        let lines: Vec<&str> = serialized_str.lines().collect();
        assert_eq!(lines.len(), 2);
        
        // Test deserialization
        let cursor = std::io::Cursor::new(serialized_str.as_bytes());
        let deserialized = batch_serializer.deserialize_batch_lines(cursor).unwrap();
        
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].event_type, EventType::TransactionReceived);
        assert_eq!(deserialized[1].event_type, EventType::ConnectivityStatus);
    }

    #[test]
    fn test_serialized_size_calculation() {
        let event = create_test_event();
        
        let json_size = calculate_serialized_size(&event, SerializationFormat::Json).unwrap();
        let pretty_size = calculate_serialized_size(&event, SerializationFormat::JsonPretty).unwrap();
        
        assert!(json_size > 0);
        assert!(pretty_size > json_size); // Pretty JSON should be larger
    }

    #[test]
    fn test_round_trip_all_formats() {
        let event = create_test_event();
        
        for format in [SerializationFormat::Json, SerializationFormat::JsonPretty, SerializationFormat::Binary] {
            let serialized = serialize_event(&event, format).unwrap();
            let deserialized = deserialize_event(&serialized, format).unwrap();
            
            assert_eq!(deserialized.event_type, event.event_type);
            assert_eq!(deserialized.wallet_id, event.wallet_id);
        }
    }

    #[test]
    fn test_binary_serialization() {
        let event = create_test_event();
        
        let binary_bytes = serialize_event(&event, SerializationFormat::Binary).unwrap();
        let deserialized = deserialize_event(&binary_bytes, SerializationFormat::Binary).unwrap();
        
        assert_eq!(deserialized.event_type, event.event_type);
        assert_eq!(deserialized.wallet_id, event.wallet_id);
        
        if let (EventData::TransactionReceived(TransactionData { tx_id: orig_tx_id, .. }), 
                EventData::TransactionReceived(TransactionData { tx_id: deser_tx_id, .. })) = 
                (&event.data, &deserialized.data) {
            assert_eq!(orig_tx_id, deser_tx_id);
        } else {
            panic!("Event data mismatch");
        }
    }

    #[test]
    fn test_binary_vs_json_size() {
        let event = create_test_event();
        
        let binary_size = calculate_serialized_size(&event, SerializationFormat::Binary).unwrap();
        let json_size = calculate_serialized_size(&event, SerializationFormat::Json).unwrap();
        
        // Binary should typically be smaller than JSON
        assert!(binary_size < json_size);
        assert!(binary_size > 0);
    }

    #[test]
    fn test_binary_batch_serialization() {
        let events = vec![
            create_test_event(),
            WalletEvent::new(
                EventType::BalanceUpdated,
                1,
                EventData::BalanceUpdated {
                    available: 2000000,
                    pending_incoming: 0,
                    pending_outgoing: 0,
                    timelocked: None,
                },
            ),
        ];
        
        let batch_serializer = BatchSerializer::new(SerializationFormat::Binary);
        let serialized = batch_serializer.serialize_batch(&events).unwrap();
        let deserialized = batch_serializer.deserialize_batch(&serialized).unwrap();
        
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].event_type, EventType::TransactionReceived);
        assert_eq!(deserialized[1].event_type, EventType::BalanceUpdated);
    }
}
