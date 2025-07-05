// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! # Callback Signatures
//! 
//! This module contains comprehensive documentation and type definitions for all
//! Tari Wallet FFI callback functions. Each callback is documented with its
//! signature, parameters, and expected behavior.

use std::ffi::c_void;
use minotari_wallet::{
    output_manager_service::service::Balance,
    transaction_service::{
        handle::TransactionSendStatus,
        storage::models::{CompletedTransaction, InboundTransaction},
    },
};
use tari_contacts::contacts_service::handle::ContactsLivenessData;
use crate::ffi_basenode_state::TariBaseNodeState;

/// Comprehensive callback signature definitions extracted from the Tari C FFI library.
/// These signatures match the exact C function pointers used in the callback handler.
pub mod signatures {
    use super::*;

    /// Called when an inbound transaction is received from an external wallet
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer (typically Arc<Mutex<PythonCallbacks>>)
    /// * `tx` - Pointer to the received inbound transaction
    /// 
    /// # Thread Safety
    /// This callback may be called from any thread and must be thread-safe
    pub type CallbackReceivedTransaction = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut InboundTransaction);

    /// Called when a reply is received for a pending outbound transaction
    /// 
    /// # Parameters  
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the completed transaction with reply
    pub type CallbackReceivedTransactionReply = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction);

    /// Called when a finalized transaction is received from the sender
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer  
    /// * `tx` - Pointer to the finalized completed transaction
    pub type CallbackReceivedFinalizedTransaction = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction);

    /// Called when a finalized transaction is broadcast to a base node mempool
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the broadcast transaction
    pub type CallbackTransactionBroadcast = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction);

    /// Called when a broadcast transaction is detected as mined
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the mined transaction
    pub type CallbackTransactionMined = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction);

    /// Called when a transaction is mined but not yet fully confirmed
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the mined transaction
    /// * `confirmations` - Number of confirmations received
    pub type CallbackTransactionMinedUnconfirmed = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction, confirmations: u64);

    /// Called when an imported/recovered transaction is confirmed as mined
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the confirmed transaction
    pub type CallbackFauxTransactionConfirmed = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction);

    /// Called when an imported/recovered transaction becomes unconfirmed
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the unconfirmed transaction  
    /// * `confirmations` - Number of confirmations
    pub type CallbackFauxTransactionUnconfirmed = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction, confirmations: u64);

    /// Called with the result of a transaction send operation
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `tx_id` - Transaction ID as u64
    /// * `status` - Pointer to transaction send status
    pub type CallbackTransactionSendResult = 
        unsafe extern "C" fn(context: *mut c_void, tx_id: u64, status: *mut TransactionSendStatus);

    /// Called when a transaction is cancelled
    /// 
    /// # Parameters  
    /// * `context` - Callback context pointer
    /// * `tx` - Pointer to the cancelled transaction
    /// * `reason` - Cancellation reason code
    pub type CallbackTransactionCancellation = 
        unsafe extern "C" fn(context: *mut c_void, tx: *mut CompletedTransaction, reason: u64);

    /// Called when TXO validation process completes
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `request_key` - Request identifier
    /// * `result` - Validation result (0 = success, non-zero = error code)
    pub type CallbackTxoValidationComplete = 
        unsafe extern "C" fn(context: *mut c_void, request_key: u64, result: u64);

    /// Called when contact liveness data is updated
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `data` - Pointer to updated liveness data
    pub type CallbackContactsLivenessDataUpdated = 
        unsafe extern "C" fn(context: *mut c_void, data: *mut ContactsLivenessData);

    /// Called when wallet balance is updated
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `balance` - Pointer to updated balance information
    pub type CallbackBalanceUpdated = 
        unsafe extern "C" fn(context: *mut c_void, balance: *mut Balance);

    /// Called when transaction validation completes
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `request_key` - Request identifier
    /// * `result` - Validation result (0 = success, non-zero = error code)  
    pub type CallbackTransactionValidationComplete = 
        unsafe extern "C" fn(context: *mut c_void, request_key: u64, result: u64);

    /// Called when SAF (Store and Forward) messages are received
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    pub type CallbackSafMessagesReceived = 
        unsafe extern "C" fn(context: *mut c_void);

    /// Called when connectivity status changes
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `status` - New connectivity status (0 = offline, 1 = online)
    pub type CallbackConnectivityStatus = 
        unsafe extern "C" fn(context: *mut c_void, status: u64);

    /// Called when wallet scan height is updated
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `height` - New scanned block height
    pub type CallbackWalletScannedHeight = 
        unsafe extern "C" fn(context: *mut c_void, height: u64);

    /// Called when base node state changes
    /// 
    /// # Parameters
    /// * `context` - Callback context pointer
    /// * `state` - Pointer to new base node state
    pub type CallbackBaseNodeState = 
        unsafe extern "C" fn(context: *mut c_void, state: *mut TariBaseNodeState);
}

/// Callback signature information for analysis and documentation
#[derive(Debug, Clone)]
pub struct CallbackSignature {
    pub name: &'static str,
    pub parameters: Vec<CallbackParameter>,
    pub purpose: &'static str,
    pub category: CallbackCategory,
}

/// Parameter information for callback signatures
#[derive(Debug, Clone)]
pub struct CallbackParameter {
    pub name: &'static str,
    pub param_type: &'static str,
    pub description: &'static str,
}

/// Callback categories for functional grouping
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackCategory {
    Transaction,
    Balance,
    Connection,
    Communication,
    Scanning,
    Validation,
}

/// Comprehensive list of all callback signatures with metadata
pub fn get_all_callback_signatures() -> Vec<CallbackSignature> {
    vec![
        CallbackSignature {
            name: "callback_received_transaction",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut InboundTransaction", 
                    description: "Received inbound transaction"
                }
            ],
            purpose: "Called when inbound transaction received from external wallet",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_received_transaction_reply",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction",
                    description: "Transaction with received reply"
                }
            ],
            purpose: "Called when reply received for pending outbound transaction",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_received_finalized_transaction",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx", 
                    param_type: "*mut CompletedTransaction",
                    description: "Finalized transaction"
                }
            ],
            purpose: "Called when finalized transaction received from sender",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_transaction_broadcast",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void", 
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction",
                    description: "Broadcast transaction"
                }
            ],
            purpose: "Called when transaction broadcast to base node mempool",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_transaction_mined",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction", 
                    description: "Mined transaction"
                }
            ],
            purpose: "Called when transaction detected as mined",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_transaction_mined_unconfirmed",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction",
                    description: "Mined but unconfirmed transaction"
                },
                CallbackParameter {
                    name: "confirmations",
                    param_type: "u64",
                    description: "Number of confirmations received"
                }
            ],
            purpose: "Called when transaction mined but not fully confirmed",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_faux_transaction_confirmed", 
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction",
                    description: "Confirmed imported/recovered transaction"
                }
            ],
            purpose: "Called when imported/recovered transaction confirmed",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_faux_transaction_unconfirmed",
            parameters: vec![
                CallbackParameter {
                    name: "context", 
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction", 
                    description: "Unconfirmed imported/recovered transaction"
                },
                CallbackParameter {
                    name: "confirmations",
                    param_type: "u64",
                    description: "Number of confirmations"
                }
            ],
            purpose: "Called when imported/recovered transaction becomes unconfirmed",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_transaction_send_result",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx_id",
                    param_type: "u64",
                    description: "Transaction ID"
                },
                CallbackParameter {
                    name: "status",
                    param_type: "*mut TransactionSendStatus",
                    description: "Transaction send result status"
                }
            ],
            purpose: "Called with result of transaction send operation",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_transaction_cancellation",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void", 
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "tx",
                    param_type: "*mut CompletedTransaction",
                    description: "Cancelled transaction"
                },
                CallbackParameter {
                    name: "reason",
                    param_type: "u64",
                    description: "Cancellation reason code"
                }
            ],
            purpose: "Called when transaction is cancelled",
            category: CallbackCategory::Transaction,
        },
        CallbackSignature {
            name: "callback_transaction_validation_complete",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "request_key", 
                    param_type: "u64",
                    description: "Validation request identifier"
                },
                CallbackParameter {
                    name: "result",
                    param_type: "u64", 
                    description: "Validation result (0=success)"
                }
            ],
            purpose: "Called when transaction validation completes",
            category: CallbackCategory::Validation,
        },
        CallbackSignature {
            name: "callback_balance_updated",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "balance",
                    param_type: "*mut Balance",
                    description: "Updated balance information"
                }
            ],
            purpose: "Called when wallet balance changes",
            category: CallbackCategory::Balance,
        },
        CallbackSignature {
            name: "callback_txo_validation_complete",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "request_key",
                    param_type: "u64",
                    description: "Validation request identifier"
                },
                CallbackParameter {
                    name: "result",
                    param_type: "u64",
                    description: "Validation result (0=success)"
                }
            ],
            purpose: "Called when TXO validation completes",
            category: CallbackCategory::Validation,
        },
        CallbackSignature {
            name: "callback_contacts_liveness_data_updated",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "data",
                    param_type: "*mut ContactsLivenessData",
                    description: "Updated contact liveness data"
                }
            ],
            purpose: "Called when contact liveness data updated",
            category: CallbackCategory::Communication,
        },
        CallbackSignature {
            name: "callback_saf_messages_received",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                }
            ],
            purpose: "Called when SAF messages received",
            category: CallbackCategory::Communication,
        },
        CallbackSignature {
            name: "callback_connectivity_status",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "status",
                    param_type: "u64",
                    description: "Connectivity status (0=offline, 1=online)"
                }
            ],
            purpose: "Called when connectivity status changes",
            category: CallbackCategory::Connection,
        },
        CallbackSignature {
            name: "callback_wallet_scanned_height",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "height",
                    param_type: "u64",
                    description: "New scanned block height"
                }
            ],
            purpose: "Called when wallet scan height updated",
            category: CallbackCategory::Scanning,
        },
        CallbackSignature {
            name: "callback_base_node_state",
            parameters: vec![
                CallbackParameter {
                    name: "context",
                    param_type: "*mut c_void",
                    description: "Callback context pointer"
                },
                CallbackParameter {
                    name: "state",
                    param_type: "*mut TariBaseNodeState",
                    description: "Updated base node state"
                }
            ],
            purpose: "Called when base node state changes",
            category: CallbackCategory::Connection,
        },
    ]
}

/// Get callbacks by category
pub fn get_callbacks_by_category(category: CallbackCategory) -> Vec<CallbackSignature> {
    get_all_callback_signatures()
        .into_iter()
        .filter(|sig| sig.category == category)
        .collect()
}

/// Get callback signature by name
pub fn get_callback_signature(name: &str) -> Option<CallbackSignature> {
    get_all_callback_signatures()
        .into_iter()
        .find(|sig| sig.name == name)
}
