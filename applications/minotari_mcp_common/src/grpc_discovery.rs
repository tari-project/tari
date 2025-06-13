//! gRPC Service Discovery and Schema Generation
//!
//! This module provides functionality to automatically discover all available gRPC methods
//! from .proto files and generate JSON Schema definitions for MCP tool parameter validation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

/// Information about a gRPC method including its input/output schemas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcMethodInfo {
    /// The method name (e.g., "GetBalance")
    pub name: String,
    /// The service name (e.g., "Wallet")
    pub service: String,
    /// Full method path (e.g., "tari.rpc.Wallet/GetBalance")
    pub full_name: String,
    /// JSON Schema for input parameters
    pub input_schema: Value,
    /// JSON Schema for output response
    pub output_schema: Value,
    /// Human-readable description of the method
    pub description: String,
    /// Whether this is a control operation (requires explicit consent)
    pub is_control_operation: bool,
    /// Method category for organization
    pub category: GrpcMethodCategory,
    /// Whether this method uses streaming
    pub is_streaming: bool,
}

/// Categories for organizing gRPC methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrpcMethodCategory {
    // Base Node categories
    Blockchain,
    Mining,
    Network,
    Mempool,
    Validation,
    
    // Wallet categories
    Balance,
    Transaction,
    Address,
    AtomicSwap,
    Recovery,
    
    // Common categories
    System,
    Status,
}

impl fmt::Display for GrpcMethodCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blockchain => write!(f, "blockchain"),
            Self::Mining => write!(f, "mining"),
            Self::Network => write!(f, "network"),
            Self::Mempool => write!(f, "mempool"),
            Self::Validation => write!(f, "validation"),
            Self::Balance => write!(f, "balance"),
            Self::Transaction => write!(f, "transaction"),
            Self::Address => write!(f, "address"),
            Self::AtomicSwap => write!(f, "atomic_swap"),
            Self::Recovery => write!(f, "recovery"),
            Self::System => write!(f, "system"),
            Self::Status => write!(f, "status"),
        }
    }
}

/// Service discovery for gRPC methods
#[derive(Debug, Clone)]
pub struct ServiceDiscovery {
    /// All available gRPC methods
    pub methods: Vec<GrpcMethodInfo>,
    /// Methods that are restricted by configuration
    pub restricted_methods: HashSet<String>,
}

impl ServiceDiscovery {
    /// Create a new service discovery instance
    pub fn new() -> Self {
        Self {
            methods: Vec::new(),
            restricted_methods: HashSet::new(),
        }
    }

    /// Add a gRPC method to the discovery registry
    pub fn add_method(&mut self, method: GrpcMethodInfo) {
        self.methods.push(method);
    }

    /// Get all methods in a specific category
    pub fn methods_by_category(&self, category: GrpcMethodCategory) -> Vec<&GrpcMethodInfo> {
        self.methods.iter()
            .filter(|m| m.category == category)
            .collect()
    }

    /// Get all available (non-restricted) methods
    pub fn available_methods(&self) -> Vec<&GrpcMethodInfo> {
        self.methods.iter()
            .filter(|m| !self.restricted_methods.contains(&m.full_name))
            .collect()
    }

    /// Get all control operations
    pub fn control_methods(&self) -> Vec<&GrpcMethodInfo> {
        self.methods.iter()
            .filter(|m| m.is_control_operation && !self.restricted_methods.contains(&m.full_name))
            .collect()
    }

    /// Get all read-only operations
    pub fn readonly_methods(&self) -> Vec<&GrpcMethodInfo> {
        self.methods.iter()
            .filter(|m| !m.is_control_operation && !self.restricted_methods.contains(&m.full_name))
            .collect()
    }

    /// Restrict specific methods based on configuration
    pub fn restrict_methods(&mut self, method_names: &[String]) {
        for name in method_names {
            self.restricted_methods.insert(name.clone());
        }
    }

    /// Get method by full name
    pub fn get_method(&self, full_name: &str) -> Option<&GrpcMethodInfo> {
        self.methods.iter().find(|m| m.full_name == full_name)
    }
}

impl Default for ServiceDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Base Node gRPC method definitions
pub fn base_node_methods() -> Vec<GrpcMethodInfo> {
    vec![
        // Blockchain methods
        GrpcMethodInfo {
            name: "ListHeaders".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/ListHeaders".to_string(),
            input_schema: list_headers_input_schema(),
            output_schema: block_header_response_schema(),
            description: "Lists headers in the current best chain".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Blockchain,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "GetHeaderByHash".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetHeaderByHash".to_string(),
            input_schema: get_header_by_hash_input_schema(),
            output_schema: block_header_response_schema(),
            description: "Get header by hash".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Blockchain,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetBlocks".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetBlocks".to_string(),
            input_schema: get_blocks_input_schema(),
            output_schema: historical_block_schema(),
            description: "Returns blocks in the current best chain".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Blockchain,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "GetTipInfo".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetTipInfo".to_string(),
            input_schema: empty_schema(),
            output_schema: tip_info_response_schema(),
            description: "Get the base node tip information".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Status,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetSyncInfo".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetSyncInfo".to_string(),
            input_schema: empty_schema(),
            output_schema: sync_info_response_schema(),
            description: "Get the base node sync information".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Status,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetNetworkStatus".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetNetworkStatus".to_string(),
            input_schema: empty_schema(),
            output_schema: network_status_response_schema(),
            description: "Get Base Node network connectivity status".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Network,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetPeers".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetPeers".to_string(),
            input_schema: get_peers_request_schema(),
            output_schema: get_peers_response_schema(),
            description: "Get all peers from the base node".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Network,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "ListConnectedPeers".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/ListConnectedPeers".to_string(),
            input_schema: empty_schema(),
            output_schema: list_connected_peers_response_schema(),
            description: "List currently connected peers".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Network,
            is_streaming: false,
        },

        // Mining methods
        GrpcMethodInfo {
            name: "GetNewBlockTemplate".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetNewBlockTemplate".to_string(),
            input_schema: new_block_template_request_schema(),
            output_schema: new_block_template_response_schema(),
            description: "Get the block template".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Mining,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetNewBlock".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetNewBlock".to_string(),
            input_schema: new_block_template_schema(),
            output_schema: get_new_block_result_schema(),
            description: "Construct a new block from a provided template".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Mining,
            is_streaming: false,
        },

        // Mempool methods
        GrpcMethodInfo {
            name: "GetMempoolTransactions".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetMempoolTransactions".to_string(),
            input_schema: get_mempool_transactions_request_schema(),
            output_schema: get_mempool_transactions_response_schema(),
            description: "Get transactions from the mempool".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Mempool,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "GetMempoolStats".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetMempoolStats".to_string(),
            input_schema: empty_schema(),
            output_schema: mempool_stats_response_schema(),
            description: "Get mempool stats".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Mempool,
            is_streaming: false,
        },

        // Control operations (require explicit consent)
        GrpcMethodInfo {
            name: "SubmitBlock".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/SubmitBlock".to_string(),
            input_schema: block_schema(),
            output_schema: submit_block_response_schema(),
            description: "Submit a new block for propagation".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Mining,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "SubmitTransaction".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/SubmitTransaction".to_string(),
            input_schema: submit_transaction_request_schema(),
            output_schema: submit_transaction_response_schema(),
            description: "Submit a transaction for propagation".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Mempool,
            is_streaming: false,
        },

        // Search and validation methods
        GrpcMethodInfo {
            name: "SearchKernels".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/SearchKernels".to_string(),
            input_schema: search_kernels_request_schema(),
            output_schema: historical_block_schema(),
            description: "Search for blocks containing the specified kernels".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Validation,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "SearchUtxos".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/SearchUtxos".to_string(),
            input_schema: search_utxos_request_schema(),
            output_schema: historical_block_schema(),
            description: "Search for blocks containing the specified commitments".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Validation,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "FetchMatchingUtxos".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/FetchMatchingUtxos".to_string(),
            input_schema: fetch_matching_utxos_request_schema(),
            output_schema: fetch_matching_utxos_response_schema(),
            description: "Fetch any utxos that exist in the main chain".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Validation,
            is_streaming: true,
        },

        // Network difficulty and tokens
        GrpcMethodInfo {
            name: "GetNetworkDifficulty".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetNetworkDifficulty".to_string(),
            input_schema: height_request_schema(),
            output_schema: network_difficulty_response_schema(),
            description: "Get network difficulties".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Status,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "GetTokensInCirculation".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetTokensInCirculation".to_string(),
            input_schema: get_blocks_input_schema(),
            output_schema: value_at_height_response_schema(),
            description: "Get coins in circulation".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Status,
            is_streaming: true,
        },

        // System info
        GrpcMethodInfo {
            name: "GetVersion".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/GetVersion".to_string(),
            input_schema: empty_schema(),
            output_schema: string_value_schema(),
            description: "Get Version".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::System,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "Identify".to_string(),
            service: "BaseNode".to_string(),
            full_name: "tari.rpc.BaseNode/Identify".to_string(),
            input_schema: empty_schema(),
            output_schema: node_identity_schema(),
            description: "This returns the node's network identity".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::System,
            is_streaming: false,
        },
    ]
}

/// Wallet gRPC method definitions
pub fn wallet_methods() -> Vec<GrpcMethodInfo> {
    vec![
        // Balance methods
        GrpcMethodInfo {
            name: "GetBalance".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetBalance".to_string(),
            input_schema: get_balance_request_schema(),
            output_schema: get_balance_response_schema(),
            description: "Returns the wallet balance details".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Balance,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetUnspentAmounts".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetUnspentAmounts".to_string(),
            input_schema: empty_schema(),
            output_schema: get_unspent_amounts_response_schema(),
            description: "Returns the total value of unspent outputs in the wallet".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Balance,
            is_streaming: false,
        },

        // Address methods
        GrpcMethodInfo {
            name: "GetAddress".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetAddress".to_string(),
            input_schema: empty_schema(),
            output_schema: get_address_response_schema(),
            description: "Returns wallet addresses (interactive and one-sided)".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Address,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetCompleteAddress".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetCompleteAddress".to_string(),
            input_schema: empty_schema(),
            output_schema: get_complete_address_response_schema(),
            description: "Get complete address information in multiple formats".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Address,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetPaymentIdAddress".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetPaymentIdAddress".to_string(),
            input_schema: get_payment_id_address_request_schema(),
            output_schema: get_complete_address_response_schema(),
            description: "Returns addresses generated for a specific payment ID".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Address,
            is_streaming: false,
        },

        // Transaction methods (read-only)
        GrpcMethodInfo {
            name: "GetTransactionInfo".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetTransactionInfo".to_string(),
            input_schema: get_transaction_info_request_schema(),
            output_schema: get_transaction_info_response_schema(),
            description: "Returns the transaction details for the given transaction IDs".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetCompletedTransactions".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetCompletedTransactions".to_string(),
            input_schema: get_completed_transactions_request_schema(),
            output_schema: get_completed_transactions_response_schema(),
            description: "Streams completed transactions for a given user payment ID".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Transaction,
            is_streaming: true,
        },
        GrpcMethodInfo {
            name: "GetBlockHeightTransactions".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetBlockHeightTransactions".to_string(),
            input_schema: get_block_height_transactions_request_schema(),
            output_schema: get_block_height_transactions_response_schema(),
            description: "Returns all transactions that were mined at a specific block height".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetTransactionPayRefs".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetTransactionPayRefs".to_string(),
            input_schema: get_transaction_pay_refs_request_schema(),
            output_schema: get_transaction_pay_refs_response_schema(),
            description: "Returns all PayRefs (payment references) for a specific transaction".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },

        // Network and status methods
        GrpcMethodInfo {
            name: "GetNetworkStatus".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetNetworkStatus".to_string(),
            input_schema: empty_schema(),
            output_schema: wallet_network_status_response_schema(),
            description: "Returns the wallet's current network connectivity status".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Network,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "ListConnectedPeers".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/ListConnectedPeers".to_string(),
            input_schema: empty_schema(),
            output_schema: wallet_list_connected_peers_response_schema(),
            description: "Returns a list of peers currently connected to the wallet".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Network,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "GetState".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetState".to_string(),
            input_schema: get_state_request_schema(),
            output_schema: get_state_response_schema(),
            description: "Returns the current operational state of the wallet".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Status,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "CheckConnectivity".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/CheckConnectivity".to_string(),
            input_schema: get_connectivity_request_schema(),
            output_schema: check_connectivity_response_schema(),
            description: "Returns lightweight response indicating network connectivity status".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::Status,
            is_streaming: false,
        },

        // System methods
        GrpcMethodInfo {
            name: "GetVersion".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/GetVersion".to_string(),
            input_schema: get_version_request_schema(),
            output_schema: get_version_response_schema(),
            description: "Returns the current version of the running wallet service".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::System,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "Identify".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/Identify".to_string(),
            input_schema: get_identity_request_schema(),
            output_schema: get_identity_response_schema(),
            description: "Returns the identity information of the wallet node".to_string(),
            is_control_operation: false,
            category: GrpcMethodCategory::System,
            is_streaming: false,
        },

        // Control operations (require explicit consent)
        GrpcMethodInfo {
            name: "Transfer".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/Transfer".to_string(),
            input_schema: transfer_request_schema(),
            output_schema: transfer_response_schema(),
            description: "Execute transfers to one or more recipients".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "CoinSplit".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/CoinSplit".to_string(),
            input_schema: coin_split_request_schema(),
            output_schema: coin_split_response_schema(),
            description: "Creates a transaction that splits funds into multiple smaller outputs".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "CreateBurnTransaction".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/CreateBurnTransaction".to_string(),
            input_schema: create_burn_transaction_request_schema(),
            output_schema: create_burn_transaction_response_schema(),
            description: "Creates a burn transaction for burning a specified amount of Tari currency".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "CancelTransaction".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/CancelTransaction".to_string(),
            input_schema: cancel_transaction_request_schema(),
            output_schema: cancel_transaction_response_schema(),
            description: "Cancels a specific transaction by its ID".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Transaction,
            is_streaming: false,
        },

        // Atomic swap methods (control operations)
        GrpcMethodInfo {
            name: "SendShaAtomicSwapTransaction".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/SendShaAtomicSwapTransaction".to_string(),
            input_schema: send_sha_atomic_swap_request_schema(),
            output_schema: send_sha_atomic_swap_response_schema(),
            description: "Sends a XTR SHA Atomic Swap transaction".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::AtomicSwap,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "ClaimShaAtomicSwapTransaction".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/ClaimShaAtomicSwapTransaction".to_string(),
            input_schema: claim_sha_atomic_swap_request_schema(),
            output_schema: claim_sha_atomic_swap_response_schema(),
            description: "Claims a SHA Atomic Swap transaction using a pre-image and output hash".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::AtomicSwap,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "ClaimHtlcRefundTransaction".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/ClaimHtlcRefundTransaction".to_string(),
            input_schema: claim_htlc_refund_request_schema(),
            output_schema: claim_htlc_refund_response_schema(),
            description: "Claims an HTLC refund transaction after the timelock period has passed".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::AtomicSwap,
            is_streaming: false,
        },

        // Import and validation methods (control operations)
        GrpcMethodInfo {
            name: "ImportUtxos".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/ImportUtxos".to_string(),
            input_schema: import_utxos_request_schema(),
            output_schema: import_utxos_response_schema(),
            description: "Imports UTXOs into the wallet as spendable outputs".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Recovery,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "RevalidateAllTransactions".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/RevalidateAllTransactions".to_string(),
            input_schema: revalidate_request_schema(),
            output_schema: revalidate_response_schema(),
            description: "Will trigger a complete revalidation of all wallet outputs".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Recovery,
            is_streaming: false,
        },
        GrpcMethodInfo {
            name: "ValidateAllTransactions".to_string(),
            service: "Wallet".to_string(),
            full_name: "tari.rpc.Wallet/ValidateAllTransactions".to_string(),
            input_schema: validate_request_schema(),
            output_schema: validate_response_schema(),
            description: "Will trigger a validation of all wallet outputs".to_string(),
            is_control_operation: true,
            category: GrpcMethodCategory::Recovery,
            is_streaming: false,
        },
    ]
}

// JSON Schema generation functions - simplified for this implementation
// In a full implementation, these would be generated from the .proto files

fn empty_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn list_headers_input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "from_height": {
                "type": "integer",
                "format": "uint64",
                "description": "The height to start at"
            },
            "num_headers": {
                "type": "integer", 
                "format": "uint64",
                "description": "The number of headers to return"
            },
            "sorting": {
                "type": "integer",
                "description": "The ordering to return the headers in"
            }
        },
        "additionalProperties": false
    })
}

fn block_header_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "header": {
                "type": "object",
                "description": "The block header"
            },
            "confirmations": {
                "type": "integer",
                "format": "uint64",
                "description": "The number of blocks from the tip"
            },
            "reward": {
                "type": "integer",
                "format": "uint64", 
                "description": "The block reward"
            },
            "difficulty": {
                "type": "integer",
                "format": "uint64",
                "description": "Achieved difficulty"
            },
            "num_transactions": {
                "type": "integer",
                "format": "uint32",
                "description": "Number of transactions in the block"
            }
        },
        "required": ["header"],
        "additionalProperties": false
    })
}

fn get_header_by_hash_input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "hash": {
                "type": "string",
                "format": "byte",
                "description": "The hash of the block header"
            }
        },
        "required": ["hash"],
        "additionalProperties": false
    })
}

fn get_blocks_input_schema() -> Value {
    serde_json::json!({
        "type": "object", 
        "properties": {
            "heights": {
                "type": "array",
                "items": {
                    "type": "integer",
                    "format": "uint64"
                },
                "description": "Block heights to retrieve"
            }
        },
        "required": ["heights"],
        "additionalProperties": false
    })
}

fn historical_block_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "block": {
                "type": "object",
                "description": "The historical block"
            },
            "spent_commitments": {
                "type": "array",
                "items": {
                    "type": "string",
                    "format": "byte"
                },
                "description": "Spent commitments"
            }
        },
        "additionalProperties": false
    })
}

fn tip_info_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "metadata": {
                "type": "object",
                "description": "Chain metadata"
            },
            "initial_sync_achieved": {
                "type": "boolean",
                "description": "Whether initial sync is achieved"
            },
            "base_node_state": {
                "type": "integer", 
                "description": "Current base node state"
            },
            "failed_checkpoints": {
                "type": "boolean",
                "description": "Whether there are failed checkpoints"
            }
        },
        "additionalProperties": false
    })
}

fn sync_info_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "tip_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Tip height"
            },
            "local_height": {
                "type": "integer",
                "format": "uint64", 
                "description": "Local height"
            },
            "peer_node_id": {
                "type": "array",
                "items": {
                    "type": "string",
                    "format": "byte"
                },
                "description": "Peer node IDs"
            }
        },
        "additionalProperties": false
    })
}

fn network_status_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "description": "Network connectivity status"
            },
            "avg_latency_ms": {
                "type": "integer",
                "format": "uint64",
                "description": "Average latency in milliseconds"
            },
            "num_node_connections": {
                "type": "integer",
                "format": "uint64",
                "description": "Number of node connections"
            }
        },
        "additionalProperties": false
    })
}

fn get_peers_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn get_peers_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "peer": {
                "type": "object",
                "description": "Peer information"
            }
        },
        "additionalProperties": false
    })
}

fn list_connected_peers_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "connected_peers": {
                "type": "array",
                "items": {
                    "type": "object"
                },
                "description": "List of connected peers"
            }
        },
        "additionalProperties": false
    })
}

fn new_block_template_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "algo": {
                "type": "integer",
                "description": "PoW algorithm"
            },
            "max_weight": {
                "type": "integer",
                "format": "uint64", 
                "description": "Maximum block weight"
            }
        },
        "required": ["algo"],
        "additionalProperties": false
    })
}

fn new_block_template_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "new_block_template": {
                "type": "object",
                "description": "New block template"
            },
            "initial_sync_achieved": {
                "type": "boolean",
                "description": "Whether initial sync is achieved"
            },
            "miner_data": {
                "type": "object",
                "description": "Miner data"
            }
        },
        "additionalProperties": false
    })
}

fn new_block_template_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "header": {
                "type": "object",
                "description": "Block header template"
            },
            "body": {
                "type": "object", 
                "description": "Block body template"
            }
        },
        "additionalProperties": false
    })
}

fn get_new_block_result_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "block_hash": {
                "type": "string",
                "format": "byte",
                "description": "Header hash of the completed block"
            },
            "block": {
                "type": "object",
                "description": "The completed block"
            },
            "merge_mining_hash": {
                "type": "string",
                "format": "byte",
                "description": "Merge mining hash"
            },
            "tari_unique_id": {
                "type": "string",
                "format": "byte",
                "description": "Tari unique ID"
            },
            "miner_data": {
                "type": "object",
                "description": "Miner data"
            }
        },
        "additionalProperties": false
    })
}

fn get_mempool_transactions_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn get_mempool_transactions_response_schema() -> Value {
    serde_json::json!({
        "type": "object", 
        "properties": {
            "transaction": {
                "type": "object",
                "description": "Mempool transaction"
            }
        },
        "additionalProperties": false
    })
}

fn mempool_stats_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "unconfirmed_txs": {
                "type": "integer",
                "format": "uint64",
                "description": "Number of unconfirmed transactions"
            },
            "reorg_txs": {
                "type": "integer",
                "format": "uint64",
                "description": "Number of reorg transactions"
            },
            "unconfirmed_weight": {
                "type": "integer", 
                "format": "uint64",
                "description": "Total unconfirmed weight"
            }
        },
        "additionalProperties": false
    })
}

fn block_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "header": {
                "type": "object",
                "description": "Block header"
            },
            "body": {
                "type": "object",
                "description": "Block body"
            }
        },
        "required": ["header", "body"],
        "additionalProperties": false
    })
}

fn submit_block_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "block_hash": {
                "type": "string",
                "format": "byte",
                "description": "Hash of the submitted block"
            }
        },
        "additionalProperties": false
    })
}

fn submit_transaction_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transaction": {
                "type": "object",
                "description": "Transaction to submit"
            }
        },
        "required": ["transaction"],
        "additionalProperties": false
    })
}

fn submit_transaction_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "result": {
                "type": "integer",
                "description": "Submit transaction result"
            }
        },
        "additionalProperties": false
    })
}

fn search_kernels_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "signatures": {
                "type": "array",
                "items": {
                    "type": "object"
                },
                "description": "Kernel signatures to search for"
            }
        },
        "required": ["signatures"],
        "additionalProperties": false
    })
}

fn search_utxos_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "commitments": {
                "type": "array",
                "items": {
                    "type": "string",
                    "format": "byte"
                },
                "description": "UTXO commitments to search for"
            }
        },
        "required": ["commitments"],
        "additionalProperties": false
    })
}

fn fetch_matching_utxos_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "hashes": {
                "type": "array",
                "items": {
                    "type": "string",
                    "format": "byte"
                },
                "description": "UTXO hashes to fetch"
            }
        },
        "required": ["hashes"],
        "additionalProperties": false
    })
}

fn fetch_matching_utxos_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "output": {
                "type": "object",
                "description": "Transaction output"
            }
        },
        "additionalProperties": false
    })
}

fn height_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "from_tip": {
                "type": "integer",
                "format": "uint64",
                "description": "Height from the chain tip"
            },
            "start_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Starting height"
            },
            "end_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Ending height"
            }
        },
        "additionalProperties": false
    })
}

fn network_difficulty_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "difficulty": {
                "type": "integer",
                "format": "uint64",
                "description": "Network difficulty"
            },
            "estimated_hash_rate": {
                "type": "integer",
                "format": "uint64",
                "description": "Estimated hash rate"
            },
            "height": {
                "type": "integer",
                "format": "uint64",
                "description": "Block height"
            },
            "timestamp": {
                "type": "integer",
                "format": "uint64",
                "description": "Timestamp"
            },
            "pow_algo": {
                "type": "integer",
                "format": "uint64",
                "description": "PoW algorithm"
            }
        },
        "additionalProperties": false
    })
}

fn value_at_height_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "height": {
                "type": "integer",
                "format": "uint64",
                "description": "Block height"
            },
            "mined_rewards": {
                "type": "integer",
                "format": "uint64",
                "description": "Mined rewards"
            },
            "spendable_rewards": {
                "type": "integer",
                "format": "uint64",
                "description": "Spendable rewards"
            },
            "spendable_pre_mine": {
                "type": "integer",
                "format": "uint64",
                "description": "Spendable pre-mine"
            },
            "total_spendable": {
                "type": "integer",
                "format": "uint64",
                "description": "Total spendable amount"
            }
        },
        "additionalProperties": false
    })
}

fn string_value_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "value": {
                "type": "string",
                "description": "String value"
            }
        },
        "additionalProperties": false
    })
}

fn node_identity_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "public_key": {
                "type": "string",
                "format": "byte",
                "description": "Node's public key"
            },
            "public_address": {
                "type": "string",
                "description": "Node's public address"
            },
            "node_id": {
                "type": "string", 
                "format": "byte",
                "description": "Node ID"
            }
        },
        "additionalProperties": false
    })
}

// Wallet schema functions

fn get_balance_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "payment_id": {
                "type": "object",
                "description": "Optional payment ID filter"
            }
        },
        "additionalProperties": false
    })
}

fn get_balance_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "available_balance": {
                "type": "integer",
                "format": "uint64",
                "description": "Available balance"
            },
            "pending_incoming_balance": {
                "type": "integer",
                "format": "uint64",
                "description": "Pending incoming balance"
            },
            "pending_outgoing_balance": {
                "type": "integer",
                "format": "uint64",
                "description": "Pending outgoing balance"
            },
            "timelocked_balance": {
                "type": "integer",
                "format": "uint64",
                "description": "Timelocked balance"
            }
        },
        "additionalProperties": false
    })
}

fn get_unspent_amounts_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "amount": {
                "type": "integer",
                "format": "uint64",
                "description": "Total unspent amount"
            }
        },
        "additionalProperties": false
    })
}

fn get_address_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "interactive_address": {
                "type": "string",
                "format": "byte",
                "description": "Interactive address"
            },
            "one_sided_address": {
                "type": "string",
                "format": "byte",
                "description": "One-sided address"
            }
        },
        "additionalProperties": false
    })
}

fn get_complete_address_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "interactive_address": {
                "type": "string",
                "format": "byte",
                "description": "Interactive address (bytes)"
            },
            "one_sided_address": {
                "type": "string",
                "format": "byte",
                "description": "One-sided address (bytes)"
            },
            "interactive_address_base58": {
                "type": "string",
                "description": "Interactive address (base58)"
            },
            "one_sided_address_base58": {
                "type": "string",
                "description": "One-sided address (base58)"
            },
            "interactive_address_emoji": {
                "type": "string",
                "description": "Interactive address (emoji)"
            },
            "one_sided_address_emoji": {
                "type": "string",
                "description": "One-sided address (emoji)"
            }
        },
        "additionalProperties": false
    })
}

fn get_payment_id_address_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "payment_id": {
                "type": "string",
                "format": "byte",
                "description": "Payment ID"
            }
        },
        "required": ["payment_id"],
        "additionalProperties": false
    })
}

fn get_transaction_info_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transaction_ids": {
                "type": "array",
                "items": {
                    "type": "integer",
                    "format": "uint64"
                },
                "description": "Transaction IDs to query",
                "minItems": 1
            }
        },
        "required": ["transaction_ids"],
        "additionalProperties": false
    })
}

fn get_transaction_info_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transactions": {
                "type": "array",
                "items": {
                    "type": "object"
                },
                "description": "Transaction information"
            }
        },
        "additionalProperties": false
    })
}

fn get_completed_transactions_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "payment_id": {
                "type": "object",
                "description": "Optional payment ID filter"
            },
            "block_hash": {
                "type": "string",
                "description": "Optional block hash filter"
            },
            "block_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Optional block height filter"
            }
        },
        "additionalProperties": false
    })
}

fn get_completed_transactions_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transaction": {
                "type": "object",
                "description": "Completed transaction"
            }
        },
        "additionalProperties": false
    })
}

fn get_block_height_transactions_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "block_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Block height to fetch transactions for"
            }
        },
        "required": ["block_height"],
        "additionalProperties": false
    })
}

fn get_block_height_transactions_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transactions": {
                "type": "array",
                "items": {
                    "type": "object"
                },
                "description": "Transactions at block height"
            }
        },
        "additionalProperties": false
    })
}

fn get_transaction_pay_refs_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transaction_id": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction ID"
            }
        },
        "required": ["transaction_id"],
        "additionalProperties": false
    })
}

fn get_transaction_pay_refs_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "payment_references": {
                "type": "array",
                "items": {
                    "type": "string",
                    "format": "byte"
                },
                "description": "Payment references"
            }
        },
        "additionalProperties": false
    })
}

fn wallet_network_status_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "enum": ["ONLINE", "DEGRADED", "OFFLINE"],
                "description": "Network connectivity status"
            },
            "avg_latency_ms": {
                "type": "integer",
                "format": "uint64",
                "description": "Average latency in milliseconds"
            },
            "num_node_connections": {
                "type": "integer",
                "format": "uint64",
                "description": "Number of node connections"
            }
        },
        "additionalProperties": false
    })
}

fn wallet_list_connected_peers_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "connected_peers": {
                "type": "array",
                "items": {
                    "type": "object"
                },
                "description": "List of connected peers"
            }
        },
        "additionalProperties": false
    })
}

fn get_state_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn get_state_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "scanned_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Latest blockchain height scanned"
            },
            "balance": {
                "type": "object",
                "description": "Current balance information"
            },
            "network": {
                "type": "object",
                "description": "Network connectivity status"
            }
        },
        "additionalProperties": false
    })
}

fn get_connectivity_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn check_connectivity_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "description": "Connectivity status"
            }
        },
        "additionalProperties": false
    })
}

fn get_version_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn get_version_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "version": {
                "type": "string",
                "description": "Wallet service version"
            }
        },
        "additionalProperties": false
    })
}

fn get_identity_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn get_identity_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "public_key": {
                "type": "string",
                "format": "byte",
                "description": "Wallet's public key"
            },
            "public_address": {
                "type": "string",
                "description": "Wallet's public address"
            },
            "node_id": {
                "type": "string",
                "format": "byte",
                "description": "Wallet node ID"
            }
        },
        "additionalProperties": false
    })
}

fn transfer_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "recipients": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Recipient address"
                        },
                        "amount": {
                            "type": "integer",
                            "format": "uint64",
                            "description": "Amount to transfer"
                        },
                        "fee_per_gram": {
                            "type": "integer",
                            "format": "uint64",
                            "description": "Fee per gram"
                        },
                        "payment_type": {
                            "type": "integer",
                            "description": "Payment type"
                        },
                        "payment_id": {
                            "type": "string",
                            "format": "byte",
                            "description": "Payment ID"
                        }
                    },
                    "required": ["address", "amount", "fee_per_gram"]
                },
                "minItems": 1
            }
        },
        "required": ["recipients"],
        "additionalProperties": false
    })
}

fn transfer_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Recipient address"
                        },
                        "transaction_id": {
                            "type": "integer",
                            "format": "uint64",
                            "description": "Transaction ID"
                        },
                        "is_success": {
                            "type": "boolean",
                            "description": "Whether transfer was successful"
                        },
                        "failure_message": {
                            "type": "string",
                            "description": "Failure message if unsuccessful"
                        }
                    }
                }
            }
        },
        "additionalProperties": false
    })
}

fn coin_split_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "amount_per_split": {
                "type": "integer",
                "format": "uint64",
                "description": "Value of each individual output",
                "minimum": 1
            },
            "split_count": {
                "type": "integer",
                "format": "uint64",
                "description": "Number of outputs to create",
                "minimum": 1
            },
            "fee_per_gram": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction fee rate per gram",
                "minimum": 1
            },
            "lock_height": {
                "type": "integer",
                "format": "uint64",
                "description": "Earliest block height for validity"
            },
            "payment_id": {
                "type": "string",
                "format": "byte",
                "description": "Optional payment ID"
            }
        },
        "required": ["amount_per_split", "split_count", "fee_per_gram"],
        "additionalProperties": false
    })
}

fn coin_split_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "tx_id": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction ID of the coin split"
            }
        },
        "additionalProperties": false
    })
}

fn create_burn_transaction_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "amount": {
                "type": "integer",
                "format": "uint64",
                "description": "Amount of Tari to burn",
                "minimum": 1
            },
            "fee_per_gram": {
                "type": "integer",
                "format": "uint64",
                "description": "Fee per gram for the transaction",
                "minimum": 1
            },
            "claim_public_key": {
                "type": "string",
                "format": "byte",
                "description": "Public key to claim ownership of burned coins"
            },
            "payment_id": {
                "type": "string",
                "format": "byte",
                "description": "Optional payment ID"
            }
        },
        "required": ["amount", "fee_per_gram"],
        "additionalProperties": false
    })
}

fn create_burn_transaction_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transaction_id": {
                "type": "integer",
                "format": "uint64",
                "description": "ID of the burn transaction"
            },
            "is_success": {
                "type": "boolean",
                "description": "Whether burn transaction was successfully created"
            },
            "failure_message": {
                "type": "string",
                "description": "Error message if creation failed"
            },
            "commitment": {
                "type": "string",
                "format": "byte",
                "description": "Commitment associated with the burn"
            },
            "ownership_proof": {
                "type": "string",
                "format": "byte",
                "description": "Proof of ownership for burned coins"
            },
            "range_proof": {
                "type": "string",
                "format": "byte",
                "description": "Range proof for burned coins"
            },
            "reciprocal_claim_public_key": {
                "type": "string",
                "format": "byte",
                "description": "Reciprocal claim public key"
            }
        },
        "additionalProperties": false
    })
}

fn cancel_transaction_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "tx_id": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction ID to cancel"
            }
        },
        "required": ["tx_id"],
        "additionalProperties": false
    })
}

fn cancel_transaction_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "is_success": {
                "type": "boolean",
                "description": "Whether cancellation was successful"
            },
            "failure_message": {
                "type": "string",
                "description": "Reason for failure if unsuccessful"
            }
        },
        "additionalProperties": false
    })
}

fn send_sha_atomic_swap_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "recipient": {
                "type": "object",
                "properties": {
                    "address": {
                        "type": "string",
                        "description": "Recipient address"
                    },
                    "amount": {
                        "type": "integer",
                        "format": "uint64",
                        "description": "Amount to swap"
                    },
                    "fee_per_gram": {
                        "type": "integer",
                        "format": "uint64",
                        "description": "Fee per gram"
                    },
                    "payment_id": {
                        "type": "string",
                        "format": "byte",
                        "description": "Payment ID"
                    }
                },
                "required": ["address", "amount", "fee_per_gram"]
            }
        },
        "required": ["recipient"],
        "additionalProperties": false
    })
}

fn send_sha_atomic_swap_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "transaction_id": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction ID"
            },
            "pre_image": {
                "type": "string",
                "format": "byte",
                "description": "SHA pre-image of the atomic swap"
            },
            "output_hash": {
                "type": "string",
                "format": "byte",
                "description": "Hash of the output"
            },
            "is_success": {
                "type": "boolean",
                "description": "Whether transaction was successful"
            },
            "failure_message": {
                "type": "string",
                "description": "Error message if failed"
            }
        },
        "additionalProperties": false
    })
}

fn claim_sha_atomic_swap_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "output": {
                "type": "string",
                "pattern": "^[0-9a-fA-F]{64}$",
                "description": "Hex-encoded output hash (SHA-256 digest)"
            },
            "pre_image": {
                "type": "string",
                "pattern": "^[0-9a-fA-F]+$",
                "description": "Hex-encoded original pre-image"
            },
            "fee_per_gram": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction fee per gram",
                "minimum": 1
            }
        },
        "required": ["output", "pre_image", "fee_per_gram"],
        "additionalProperties": false
    })
}

fn claim_sha_atomic_swap_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "object",
                "properties": {
                    "address": {
                        "type": "string",
                        "description": "Address"
                    },
                    "transaction_id": {
                        "type": "integer",
                        "format": "uint64",
                        "description": "Transaction ID"
                    },
                    "is_success": {
                        "type": "boolean",
                        "description": "Whether claim was successful"
                    },
                    "failure_message": {
                        "type": "string",
                        "description": "Error message if failed"
                    }
                }
            }
        },
        "additionalProperties": false
    })
}

fn claim_htlc_refund_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "output_hash": {
                "type": "string",
                "pattern": "^[0-9a-fA-F]{64}$",
                "description": "Hex-encoded SHA-256 hash of HTLC output"
            },
            "fee_per_gram": {
                "type": "integer",
                "format": "uint64",
                "description": "Transaction fee per gram",
                "minimum": 1
            }
        },
        "required": ["output_hash", "fee_per_gram"],
        "additionalProperties": false
    })
}

fn claim_htlc_refund_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "object",
                "properties": {
                    "address": {
                        "type": "string",
                        "description": "Address"
                    },
                    "transaction_id": {
                        "type": "integer",
                        "format": "uint64",
                        "description": "Transaction ID"
                    },
                    "is_success": {
                        "type": "boolean",
                        "description": "Whether refund was successful"
                    },
                    "failure_message": {
                        "type": "string",
                        "description": "Error message if failed"
                    }
                }
            }
        },
        "additionalProperties": false
    })
}

fn import_utxos_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "outputs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "description": "Unblinded output"
                },
                "description": "List of unblinded outputs to import",
                "minItems": 1
            },
            "payment_id": {
                "type": "string",
                "format": "byte",
                "description": "Optional payment ID"
            }
        },
        "required": ["outputs"],
        "additionalProperties": false
    })
}

fn import_utxos_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "tx_ids": {
                "type": "array",
                "items": {
                    "type": "integer",
                    "format": "uint64"
                },
                "description": "Transaction IDs for imported UTXOs"
            }
        },
        "additionalProperties": false
    })
}

fn revalidate_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn revalidate_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "success": {
                "type": "boolean",
                "description": "Whether revalidation was successful"
            }
        },
        "additionalProperties": false
    })
}

fn validate_request_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn validate_response_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "success": {
                "type": "boolean",
                "description": "Whether validation was successful"
            }
        },
        "additionalProperties": false
    })
}
