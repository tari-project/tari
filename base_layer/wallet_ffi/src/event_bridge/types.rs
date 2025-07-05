//! # Event Types
//!
//! This module defines all event types that correspond to the 18 Tari wallet callbacks.
//! Each event type contains relevant data extracted from callback parameters and includes
//! metadata like timestamps and wallet context.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Transaction data structure for safe representation of C FFI transaction data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionData {
    pub tx_id: u64,
    pub source_address: String,
    pub amount: u64,
    pub message: Option<String>,
    pub timestamp: i64,
    pub status: u8,
}

/// Main wallet event envelope containing type, timestamp, and data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletEvent {
    /// Type of event for categorization and routing
    pub event_type: EventType,
    /// Timestamp when event was created
    pub timestamp: SystemTime,
    /// Wallet instance ID that generated this event
    pub wallet_id: u64,
    /// Event-specific data payload
    pub data: EventData,
}

impl WalletEvent {
    /// Create a new wallet event
    pub fn new(event_type: EventType, wallet_id: u64, data: EventData) -> Self {
        Self {
            event_type,
            timestamp: SystemTime::now(),
            wallet_id,
            data,
        }
    }

    /// Get a string representation of the event type for logging
    pub fn event_name(&self) -> &'static str {
        match self.event_type {
            EventType::TransactionReceived => "transaction_received",
            EventType::TransactionReply => "transaction_reply",
            EventType::TransactionFinalized => "transaction_finalized",
            EventType::TransactionBroadcast => "transaction_broadcast",
            EventType::TransactionMined => "transaction_mined",
            EventType::TransactionMinedUnconfirmed => "transaction_mined_unconfirmed",
            EventType::FauxTransactionConfirmed => "faux_transaction_confirmed",
            EventType::FauxTransactionUnconfirmed => "faux_transaction_unconfirmed",
            EventType::TransactionSendResult => "transaction_send_result",
            EventType::TransactionCancellation => "transaction_cancellation",
            EventType::BalanceUpdated => "balance_updated",
            EventType::TxoValidationComplete => "txo_validation_complete",
            EventType::TransactionValidationComplete => "transaction_validation_complete",
            EventType::ContactsLivenessUpdated => "contacts_liveness_updated",
            EventType::SafMessagesReceived => "saf_messages_received",
            EventType::ConnectivityStatus => "connectivity_status",
            EventType::WalletScannedHeight => "wallet_scanned_height",
            EventType::BaseNodeState => "base_node_state",
        }
    }
}

/// Enumeration of all supported event types
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    // Transaction Events (10 types)
    TransactionReceived,
    TransactionReply,
    TransactionFinalized,
    TransactionBroadcast,
    TransactionMined,
    TransactionMinedUnconfirmed,
    FauxTransactionConfirmed,
    FauxTransactionUnconfirmed,
    TransactionSendResult,
    TransactionCancellation,
    
    // Balance Events (1 type)
    BalanceUpdated,
    
    // Validation Events (2 types)
    TxoValidationComplete,
    TransactionValidationComplete,
    
    // Communication Events (2 types)
    ContactsLivenessUpdated,
    SafMessagesReceived,
    
    // Connection Events (2 types)
    ConnectivityStatus,
    BaseNodeState,
    
    // Scanning Events (1 type)
    WalletScannedHeight,
}

/// Event data variants containing specific information for each event type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventData {
    /// Inbound transaction received from external wallet
    TransactionReceived(TransactionData),
    
    /// Reply received for pending outbound transaction
    TransactionReply {
        tx_id: u64,
        amount: u64,
        is_success: bool,
    },
    
    /// Finalized transaction received from sender
    TransactionFinalized {
        tx_id: u64,
        amount: u64,
        fee: u64,
    },
    
    /// Transaction broadcast to base node mempool
    TransactionBroadcast {
        tx_id: u64,
        amount: u64,
        fee: u64,
    },
    
    /// Transaction detected as mined in a block
    TransactionMined {
        tx_id: u64,
        amount: u64,
        block_height: Option<u64>,
    },
    
    /// Transaction mined but not yet fully confirmed
    TransactionMinedUnconfirmed {
        tx_id: u64,
        amount: u64,
        confirmations: u64,
    },
    
    /// Imported/recovered transaction confirmed as mined
    FauxTransactionConfirmed {
        tx_id: u64,
        amount: u64,
    },
    
    /// Imported/recovered transaction becomes unconfirmed
    FauxTransactionUnconfirmed {
        tx_id: u64,
        amount: u64,
        confirmations: u64,
    },
    
    /// Result of transaction send operation
    TransactionSendResult {
        tx_id: u64,
        is_success: bool,
        failure_reason: Option<String>,
    },
    
    /// Transaction cancelled with reason
    TransactionCancellation {
        tx_id: u64,
        reason_code: u64,
        reason_message: String,
    },
    
    /// Wallet balance updated
    BalanceUpdated {
        available: u64,
        pending_incoming: u64,
        pending_outgoing: u64,
        timelocked: Option<u64>,
    },
    
    /// TXO validation process completed
    TxoValidationComplete {
        request_key: u64,
        is_success: bool,
        validation_results: ValidationResults,
    },
    
    /// Transaction validation completed
    TransactionValidationComplete {
        request_key: u64,
        is_success: bool,
        validation_results: ValidationResults,
    },
    
    /// Contact liveness data updated
    ContactsLivenessUpdated {
        contact_count: u32,
        online_count: u32,
        last_seen_updates: Vec<ContactUpdate>,
    },
    
    /// SAF (Store and Forward) messages received
    SafMessagesReceived {
        message_count: u32,
    },
    
    /// Network connectivity status changed
    ConnectivityStatus {
        status: ConnectivityState,
        peer_count: u32,
    },
    
    /// Wallet scan height updated during sync
    WalletScannedHeight {
        height: u64,
        total_height: Option<u64>,
        sync_percentage: Option<f64>,
    },
    
    /// Base node state changed
    BaseNodeState {
        node_id: String,
        chain_height: u64,
        is_synced: bool,
        sync_percentage: Option<f64>,
    },
}

/// Validation results for TXO and transaction validation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationResults {
    pub total_checked: u64,
    pub valid_count: u64,
    pub invalid_count: u64,
    pub errors: Vec<String>,
}

/// Contact update information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContactUpdate {
    pub public_key: String,
    pub last_seen: Option<SystemTime>,
    pub is_online: bool,
}

/// Network connectivity states
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConnectivityState {
    Disconnected,
    Connecting,
    Connected,
    Synchronizing,
    Synchronized,
}

impl From<u64> for ConnectivityState {
    fn from(status: u64) -> Self {
        match status {
            0 => ConnectivityState::Disconnected,
            1 => ConnectivityState::Connecting,
            2 => ConnectivityState::Connected,
            3 => ConnectivityState::Synchronizing,
            4 => ConnectivityState::Synchronized,
            _ => ConnectivityState::Disconnected,
        }
    }
}

impl Into<u64> for ConnectivityState {
    fn into(self) -> u64 {
        match self {
            ConnectivityState::Disconnected => 0,
            ConnectivityState::Connecting => 1,
            ConnectivityState::Connected => 2,
            ConnectivityState::Synchronizing => 3,
            ConnectivityState::Synchronized => 4,
        }
    }
}

/// Event priority levels for processing and filtering
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl EventType {
    /// Get the priority level for this event type
    pub fn priority(&self) -> EventPriority {
        match self {
            // Critical transaction events
            EventType::TransactionReceived |
            EventType::TransactionMined |
            EventType::TransactionCancellation => EventPriority::Critical,
            
            // High priority transaction events
            EventType::TransactionBroadcast |
            EventType::TransactionFinalized |
            EventType::TransactionSendResult |
            EventType::BalanceUpdated => EventPriority::High,
            
            // Medium priority events
            EventType::TransactionReply |
            EventType::TransactionMinedUnconfirmed |
            EventType::ConnectivityStatus |
            EventType::BaseNodeState => EventPriority::Medium,
            
            // Low priority events
            EventType::FauxTransactionConfirmed |
            EventType::FauxTransactionUnconfirmed |
            EventType::TxoValidationComplete |
            EventType::TransactionValidationComplete |
            EventType::ContactsLivenessUpdated |
            EventType::SafMessagesReceived |
            EventType::WalletScannedHeight => EventPriority::Low,
        }
    }

    /// Get the functional category for this event type
    pub fn category(&self) -> EventCategory {
        match self {
            EventType::TransactionReceived |
            EventType::TransactionReply |
            EventType::TransactionFinalized |
            EventType::TransactionBroadcast |
            EventType::TransactionMined |
            EventType::TransactionMinedUnconfirmed |
            EventType::FauxTransactionConfirmed |
            EventType::FauxTransactionUnconfirmed |
            EventType::TransactionSendResult |
            EventType::TransactionCancellation => EventCategory::Transaction,
            
            EventType::BalanceUpdated => EventCategory::Balance,
            
            EventType::TxoValidationComplete |
            EventType::TransactionValidationComplete => EventCategory::Validation,
            
            EventType::ContactsLivenessUpdated |
            EventType::SafMessagesReceived => EventCategory::Communication,
            
            EventType::ConnectivityStatus |
            EventType::BaseNodeState => EventCategory::Connection,
            
            EventType::WalletScannedHeight => EventCategory::Scanning,
        }
    }
}

/// Event functional categories
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventCategory {
    Transaction,
    Balance,
    Connection,
    Communication,
    Validation,
    Scanning,
}

/// Event metadata for debugging and analysis
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventMetadata {
    pub event_id: uuid::Uuid,
    pub correlation_id: Option<String>,
    pub source_callback: String,
    pub processing_latency_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let transaction_data = TransactionData {
            tx_id: 123,
            source_address: "test_address".to_string(),
            amount: 1000000,
            message: Some("test message".to_string()),
            timestamp: 1640995200,
            status: 1,
        };

        let event = WalletEvent::new(
            EventType::TransactionReceived,
            1,
            EventData::TransactionReceived(transaction_data),
        );

        assert_eq!(event.event_type, EventType::TransactionReceived);
        assert_eq!(event.wallet_id, 1);
        assert_eq!(event.event_name(), "transaction_received");
    }

    #[test]
    fn test_connectivity_state_conversion() {
        assert_eq!(ConnectivityState::from(0), ConnectivityState::Disconnected);
        assert_eq!(ConnectivityState::from(2), ConnectivityState::Connected);
        
        let state = ConnectivityState::Connected;
        let status: u64 = state.into();
        assert_eq!(status, 2);
    }

    #[test]
    fn test_event_priority_ordering() {
        assert!(EventPriority::Critical > EventPriority::High);
        assert!(EventPriority::High > EventPriority::Medium);
        assert!(EventPriority::Medium > EventPriority::Low);
    }

    #[test]
    fn test_event_type_categorization() {
        assert_eq!(EventType::TransactionReceived.category(), EventCategory::Transaction);
        assert_eq!(EventType::BalanceUpdated.category(), EventCategory::Balance);
        assert_eq!(EventType::ConnectivityStatus.category(), EventCategory::Connection);
    }
}
