// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Callback Data Types
//!
//! This module provides comprehensive analysis and documentation of all data
//! structures passed to wallet callbacks, including their memory layout,
//! Python conversion requirements, and field descriptions.

use minotari_wallet::{
    output_manager_service::service::Balance,
    transaction_service::{
        handle::TransactionSendStatus,
        storage::models::{CompletedTransaction, InboundTransaction},
    },
};
use tari_contacts::contacts_service::handle::ContactsLivenessData;
use crate::ffi_basenode_state::TariBaseNodeState;

/// Comprehensive documentation of callback data structures
pub mod data_structures {

    /// Analysis of TariPendingInboundTransaction structure
    /// Used by: callback_received_transaction
    pub mod pending_inbound_transaction {
        
        /// Memory layout documentation for InboundTransaction
        pub fn document_structure() -> String {
            format!(r#"
InboundTransaction Structure:
├── tx_id: TxId (u64)                    - Unique transaction identifier
├── source_address: TariAddress          - Sender's wallet address  
├── amount: MicroMinotari               - Transaction amount in µT
├── fee: MicroMinotari                  - Transaction fee in µT
├── message: String                     - Optional transaction message
├── timestamp: NaiveDateTime            - When transaction was received
└── cancelled: Option<TxCancellationReason> - Cancellation status

Python Conversion Requirements:
- tx_id: u64 → Python int
- source_address: TariAddress → Python string (emoji format)
- amount: MicroMinotari → Python int (preserve precision)
- fee: MicroMinotari → Python int
- message: String → Python str
- timestamp: NaiveDateTime → Python datetime
- cancelled: Option<Enum> → Python Optional[str]

Memory Safety:
- Structure passed as boxed pointer (*mut InboundTransaction)
- Python bridge must handle pointer lifetime correctly
- No raw string pointers - all strings are owned
            "#)
        }

        /// Get field descriptions for documentation
        pub fn get_field_descriptions() -> Vec<(&'static str, &'static str, &'static str)> {
            vec![
                ("tx_id", "TxId (u64)", "Unique identifier for the transaction"),
                ("source_address", "TariAddress", "Address of the wallet sending the transaction"),
                ("amount", "MicroMinotari", "Amount being sent in microTari (1 XTR = 1,000,000 µT)"),
                ("fee", "MicroMinotari", "Transaction fee in microTari"),
                ("message", "String", "Optional message attached to transaction"),
                ("timestamp", "NaiveDateTime", "When the transaction was first received"),
                ("cancelled", "Option<TxCancellationReason>", "Cancellation status if cancelled"),
            ]
        }
    }

    /// Analysis of TariCompletedTransaction structure  
    /// Used by: Multiple transaction callbacks
    pub mod completed_transaction {
        
        pub fn document_structure() -> String {
            format!(r#"
CompletedTransaction Structure:
├── tx_id: TxId (u64)                    - Unique transaction identifier
├── source_address: TariAddress          - Sender's address
├── destination_address: TariAddress     - Recipient's address
├── amount: MicroMinotari               - Transaction amount
├── fee: MicroMinotari                  - Transaction fee
├── transaction: Transaction            - The actual transaction structure
├── status: TransactionStatus           - Current transaction status
├── message: String                     - Transaction message
├── timestamp: NaiveDateTime            - Transaction timestamp
├── cancelled: Option<TxCancellationReason> - Cancellation reason
├── direction: TransactionDirection     - Inbound/Outbound/Coinbase
├── send_count: u32                     - Number of send attempts
├── last_send_timestamp: Option<NaiveDateTime> - Last broadcast attempt
└── confirmations: Option<u64>          - Number of confirmations

Status Values:
- Completed: Transaction negotiated and finalized
- Broadcast: Sent to base node mempool  
- Mined: Included in a block
- Imported: Imported from another source
- Pending: Waiting for completion

Python Conversion Requirements:
- Complex nested structure requiring careful conversion
- Enums need string representation in Python
- Optional fields become Python Optional types
- Timestamps need datetime conversion
            "#)
        }

        pub fn get_status_descriptions() -> Vec<(&'static str, &'static str)> {
            vec![
                ("Completed", "Transaction negotiated between parties, ready for broadcast"),
                ("Broadcast", "Transaction sent to base node mempool"),
                ("Mined", "Transaction included in a mined block"), 
                ("Imported", "Transaction imported from external source"),
                ("Pending", "Transaction still being negotiated"),
            ]
        }
    }

    /// Analysis of Balance structure
    /// Used by: callback_balance_updated
    pub mod balance {
        
        pub fn document_structure() -> String {
            format!(r#"
Balance Structure:
├── available_balance: MicroMinotari     - Spendable balance
├── time_locked_balance: Option<Vec<...>> - Time-locked outputs (complex)
├── pending_incoming_balance: MicroMinotari - Incoming pending amount
└── pending_outgoing_balance: MicroMinotari - Outgoing pending amount

Balance States:
- available_balance: Confirmed, spendable funds
- pending_incoming_balance: Transactions received but not yet mined
- pending_outgoing_balance: Transactions sent but not yet mined  
- time_locked_balance: Outputs locked until specific time/height

Python Conversion:
- All MicroMinotari values → Python int (no precision loss)
- time_locked_balance → Optional[List[Dict]] (complex structure)
- Balance operations in Python should preserve precision

Precision Notes:
- 1 Tari (XTR) = 1,000,000 microTari (µT)
- Python int can handle full precision without loss
- Display formatting should convert back to XTR for UI
            "#)
        }

        pub fn get_precision_info() -> (&'static str, u64, &'static str) {
            ("microTari", 1_000_000, "1 XTR = 1,000,000 µT")
        }
    }

    /// Analysis of TransactionSendStatus
    /// Used by: callback_transaction_send_result  
    pub mod transaction_send_status {
        
        pub fn document_structure() -> String {
            format!(r#"
TransactionSendStatus Enum:
├── Queued                              - Transaction queued for sending
├── Sending                             - Currently being sent
├── Sent                               - Successfully sent
├── Failed(reason: String)             - Send failed with reason
└── SentDirect                         - Sent directly to recipient

Python Conversion:
- Enum variants → Python strings
- Failed variant includes reason string
- Simple string representation sufficient for most use cases

Error Reasons (Failed variant):
- Network connectivity issues
- Invalid recipient address  
- Insufficient funds
- Transaction validation errors
            "#)
        }

        pub fn get_status_variants() -> Vec<(&'static str, &'static str)> {
            vec![
                ("Queued", "Transaction is queued for transmission"),
                ("Sending", "Transaction is currently being sent"),
                ("Sent", "Transaction successfully transmitted"),
                ("Failed", "Transmission failed (includes error reason)"),
                ("SentDirect", "Transaction sent directly to recipient"),
            ]
        }
    }

    /// Analysis of ContactsLivenessData
    /// Used by: callback_contacts_liveness_data_updated
    pub mod contacts_liveness_data {
        
        pub fn document_structure() -> String {
            format!(r#"
ContactsLivenessData Structure:
├── address: TariAddress                 - Contact's address
├── online_status: Option<bool>          - Whether contact is online
├── last_seen: Option<NaiveDateTime>     - When contact was last seen
└── metadata: HashMap<String, String>    - Additional contact metadata

Python Conversion:
- address → Python string (emoji format)
- online_status → Python Optional[bool]
- last_seen → Python Optional[datetime]
- metadata → Python Dict[str, str]

Use Cases:
- Contact availability indication
- Last-seen timestamps for UI
- Contact status updates
            "#)
        }
    }

    /// Analysis of TariBaseNodeState
    /// Used by: callback_base_node_state
    pub mod base_node_state {
        
        pub fn document_structure() -> String {
            format!(r#"
TariBaseNodeState Structure:
├── node_id: Vec<u8>                     - Base node public key
├── best_block_height: u64               - Current blockchain height
├── best_block_hash: BlockHash           - Hash of current best block
├── best_block_timestamp: u64            - Timestamp of best block
├── pruning_horizon: u64                 - Pruning horizon setting
├── pruned_height: u64                   - Height up to which chain is pruned
├── is_node_synced: bool                 - Whether node is synced
├── updated_at: u64                      - Timestamp of last update
└── latency: u64                         - Network latency to node (ms)

Python Conversion:
- node_id → Python bytes
- Heights and timestamps → Python int
- best_block_hash → Python bytes (32 bytes)
- Booleans → Python bool
- Latency in milliseconds for performance monitoring

Sync Status:
- is_node_synced indicates if node is caught up with network
- Latency provides connection quality information
- Heights track blockchain synchronization progress
            "#)
        }
    }
}

/// Memory layout information for all callback data types
#[derive(Debug, Clone)]
pub struct CallbackDataTypeInfo {
    pub type_name: &'static str,
    pub memory_size: usize,
    pub alignment: usize,
    pub is_pod: bool, // Plain Old Data - safe for memcpy
    pub has_pointers: bool,
    pub python_conversion_complexity: ConversionComplexity,
}

#[derive(Debug, Clone)]
pub enum ConversionComplexity {
    Simple,    // Direct type mapping
    Moderate,  // Some enum/option handling
    Complex,   // Nested structures, custom conversion
    Advanced,  // Deep nesting, lifetime management
}

/// Get data type information for all callback parameters
pub fn get_all_callback_data_types() -> Vec<CallbackDataTypeInfo> {
    vec![
        CallbackDataTypeInfo {
            type_name: "InboundTransaction",
            memory_size: std::mem::size_of::<InboundTransaction>(),
            alignment: std::mem::align_of::<InboundTransaction>(),
            is_pod: false,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Complex,
        },
        CallbackDataTypeInfo {
            type_name: "CompletedTransaction", 
            memory_size: std::mem::size_of::<CompletedTransaction>(),
            alignment: std::mem::align_of::<CompletedTransaction>(),
            is_pod: false,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Advanced,
        },
        CallbackDataTypeInfo {
            type_name: "Balance",
            memory_size: std::mem::size_of::<Balance>(),
            alignment: std::mem::align_of::<Balance>(),
            is_pod: false,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Moderate,
        },
        CallbackDataTypeInfo {
            type_name: "TransactionSendStatus",
            memory_size: std::mem::size_of::<TransactionSendStatus>(),
            alignment: std::mem::align_of::<TransactionSendStatus>(),
            is_pod: false,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Moderate,
        },
        CallbackDataTypeInfo {
            type_name: "ContactsLivenessData",
            memory_size: std::mem::size_of::<ContactsLivenessData>(),
            alignment: std::mem::align_of::<ContactsLivenessData>(),
            is_pod: false,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Complex,
        },
        CallbackDataTypeInfo {
            type_name: "TariBaseNodeState",
            memory_size: std::mem::size_of::<TariBaseNodeState>(),
            alignment: std::mem::align_of::<TariBaseNodeState>(),
            is_pod: false,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Moderate,
        },
        CallbackDataTypeInfo {
            type_name: "u64",
            memory_size: 8,
            alignment: 8,
            is_pod: true,
            has_pointers: false,
            python_conversion_complexity: ConversionComplexity::Simple,
        },
        CallbackDataTypeInfo {
            type_name: "*mut c_void",
            memory_size: 8,
            alignment: 8,
            is_pod: true,
            has_pointers: true,
            python_conversion_complexity: ConversionComplexity::Simple,
        },
    ]
}

/// Generate comprehensive report of all callback data structures
pub fn generate_data_structure_report() -> String {
    let mut report = String::new();
    
    report.push_str("# Tari Wallet Callback Data Structures Report\n\n");
    
    // Memory layout analysis
    report.push_str("## Memory Layout Analysis\n\n");
    let data_types = get_all_callback_data_types();
    
    for data_type in &data_types {
        report.push_str(&format!(
            "### {}\n- Size: {} bytes\n- Alignment: {} bytes\n- POD: {}\n- Has Pointers: {}\n- Conversion Complexity: {:?}\n\n",
            data_type.type_name,
            data_type.memory_size,
            data_type.alignment,
            data_type.is_pod,
            data_type.has_pointers,
            data_type.python_conversion_complexity
        ));
    }
    
    // Detailed structure documentation
    report.push_str("## Detailed Structure Documentation\n\n");
    
    report.push_str("### InboundTransaction\n");
    report.push_str(&data_structures::pending_inbound_transaction::document_structure());
    report.push_str("\n\n");
    
    report.push_str("### CompletedTransaction\n");
    report.push_str(&data_structures::completed_transaction::document_structure());
    report.push_str("\n\n");
    
    report.push_str("### Balance\n");
    report.push_str(&data_structures::balance::document_structure());
    report.push_str("\n\n");
    
    report.push_str("### TransactionSendStatus\n");
    report.push_str(&data_structures::transaction_send_status::document_structure());
    report.push_str("\n\n");
    
    report.push_str("### ContactsLivenessData\n");
    report.push_str(&data_structures::contacts_liveness_data::document_structure());
    report.push_str("\n\n");
    
    report.push_str("### TariBaseNodeState\n");
    report.push_str(&data_structures::base_node_state::document_structure());
    report.push_str("\n\n");
    
    // Python conversion summary
    report.push_str("## Python Conversion Summary\n\n");
    report.push_str("| Type | Complexity | Key Challenges |\n");
    report.push_str("|------|------------|----------------|\n");
    
    for data_type in &data_types {
        let challenges = match data_type.python_conversion_complexity {
            ConversionComplexity::Simple => "Direct mapping",
            ConversionComplexity::Moderate => "Enum/Option handling", 
            ConversionComplexity::Complex => "Nested structures",
            ConversionComplexity::Advanced => "Deep nesting, lifetime management",
        };
        
        report.push_str(&format!(
            "| {} | {:?} | {} |\n",
            data_type.type_name,
            data_type.python_conversion_complexity,
            challenges
        ));
    }
    
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_type_info_completeness() {
        let data_types = get_all_callback_data_types();
        
        // Should have info for all major data types
        assert!(data_types.len() >= 6);
        
        // Verify key types are included
        let type_names: Vec<&str> = data_types.iter().map(|dt| dt.type_name).collect();
        assert!(type_names.contains(&"InboundTransaction"));
        assert!(type_names.contains(&"CompletedTransaction"));
        assert!(type_names.contains(&"Balance"));
        assert!(type_names.contains(&"TransactionSendStatus"));
    }
    
    #[test]
    fn test_memory_layout_info() {
        let data_types = get_all_callback_data_types();
        
        for data_type in data_types {
            // Memory size should be reasonable
            assert!(data_type.memory_size > 0);
            assert!(data_type.memory_size < 10000); // Sanity check
            
            // Alignment should be power of 2
            assert!(data_type.alignment > 0);
            assert!(data_type.alignment.is_power_of_two());
        }
    }
    
    #[test]
    fn test_structure_documentation() {
        // Test that documentation functions don't panic
        let _ = data_structures::pending_inbound_transaction::document_structure();
        let _ = data_structures::completed_transaction::document_structure();
        let _ = data_structures::balance::document_structure();
        let _ = data_structures::transaction_send_status::document_structure();
        let _ = data_structures::contacts_liveness_data::document_structure();
        let _ = data_structures::base_node_state::document_structure();
    }
    
    #[test]
    fn test_field_descriptions() {
        let fields = data_structures::pending_inbound_transaction::get_field_descriptions();
        assert!(!fields.is_empty());
        
        // Each field should have name, type, and description
        for (name, type_str, desc) in fields {
            assert!(!name.is_empty());
            assert!(!type_str.is_empty());
            assert!(!desc.is_empty());
        }
    }
    
    #[test]
    fn test_report_generation() {
        let report = generate_data_structure_report();
        
        // Report should be substantial
        assert!(report.len() > 1000);
        
        // Should contain key sections
        assert!(report.contains("Memory Layout Analysis"));
        assert!(report.contains("Detailed Structure Documentation"));
        assert!(report.contains("Python Conversion Summary"));
        
        // Should mention all major types
        assert!(report.contains("InboundTransaction"));
        assert!(report.contains("CompletedTransaction"));
        assert!(report.contains("Balance"));
    }
    
    #[test]
    fn test_precision_info() {
        let (unit, factor, description) = data_structures::balance::get_precision_info();
        assert_eq!(unit, "microTari");
        assert_eq!(factor, 1_000_000);
        assert!(description.contains("XTR"));
    }
}
