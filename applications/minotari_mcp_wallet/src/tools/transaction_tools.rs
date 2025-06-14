//! Transaction-related MCP tools for wallet operations
//!
//! This module provides comprehensive transaction management including
//! transaction history, creation, cancellation, and analysis.

use std::sync::Arc;

use minotari_app_grpc::tari_rpc::{
    BlockHashHex,
    BlockHeight,
    CancelTransactionRequest,
    CoinSplitRequest,
    GetCompletedTransactionsRequest,
    GetTransactionInfoRequest,
    PaymentRecipient,
    TransferRequest,
    UserPaymentId,
};
use minotari_mcp_common::{get_optional_string_param, get_required_u64_param, McpError, McpResult, McpTool};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{json, Value};
use tonic::{transport::Channel, Request};

/// Tool for getting transaction information by ID
#[derive(Clone)]
pub struct GetTransactionInfoTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetTransactionInfoTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetTransactionInfoTool {
    fn name(&self) -> &str {
        "get_transaction_info"
    }

    fn description(&self) -> &str {
        "Retrieves detailed information for specific transactions by their IDs"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "transaction_ids": {
                    "type": "array",
                    "items": {"type": "number"},
                    "description": "Array of transaction IDs to query",
                    "minItems": 1
                }
            },
            "required": ["transaction_ids"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let transaction_ids: Vec<u64> = params
            .get("transaction_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::invalid_request("transaction_ids array is required"))?
            .iter()
            .filter_map(|v| v.as_u64())
            .collect();

        if transaction_ids.is_empty() {
            return Err(McpError::invalid_request("At least one transaction ID is required"));
        }

        let request = Request::new(GetTransactionInfoRequest { transaction_ids });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .get_transaction_info(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get transaction info: {}", e)))?
            .into_inner();

        let transactions: Vec<Value> = response
            .transactions
            .iter()
            .map(|tx| {
                json!({
                    "tx_id": tx.tx_id,
                    "source_address": hex::encode(&tx.source_address),
                    "dest_address": hex::encode(&tx.dest_address),
                    "status": match tx.status {
                        0 => "COMPLETED",
                        1 => "BROADCAST",
                        2 => "MINED_UNCONFIRMED",
                        3 => "IMPORTED",
                        4 => "PENDING",
                        5 => "COINBASE",
                        6 => "MINED_CONFIRMED",
                        7 => "REJECTED",
                        8 => "CANCELLED",
                        9 => "NOT_FOUND",
                        _ => "UNKNOWN",
                    },
                    "direction": match tx.direction {
                        0 => "UNKNOWN",
                        1 => "INBOUND",
                        2 => "OUTBOUND",
                        _ => "UNKNOWN",
                    },
                    "amount": tx.amount,
                    "fee": tx.fee,
                    "is_cancelled": tx.is_cancelled,
                    "excess_sig": hex::encode(&tx.excess_sig),
                    "timestamp": tx.timestamp,
                    "payment_id": hex::encode(&tx.user_payment_id),
                    "mined_in_block_height": tx.mined_in_block_height,
                    "formatted": {
                        "amount_tari": (tx.amount as f64 / 1_000_000.0),
                        "fee_tari": (tx.fee as f64 / 1_000_000.0),
                        "timestamp_readable": if tx.timestamp > 0 {
                            chrono::DateTime::from_timestamp(tx.timestamp as i64, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                .unwrap_or_else(|| "Invalid timestamp".to_string())
                        } else {
                            "No timestamp".to_string()
                        },
                    },
                    "confirmations": if tx.mined_in_block_height > 0 && tx.status == 6 {
                        "CONFIRMED"
                    } else if tx.mined_in_block_height > 0 {
                        "MINED_UNCONFIRMED"
                    } else {
                        "UNCONFIRMED"
                    },
                })
            })
            .collect();

        // Calculate summary statistics
        let total_amount: u64 = transactions.iter().filter_map(|tx| tx["amount"].as_u64()).sum();
        let total_fees: u64 = transactions.iter().filter_map(|tx| tx["fee"].as_u64()).sum();
        let confirmed_count = transactions
            .iter()
            .filter(|tx| tx["status"].as_str() == Some("MINED_CONFIRMED"))
            .count();

        Ok(json!({
            "transactions": transactions,
            "summary": {
                "total_transactions": transactions.len(),
                "confirmed_transactions": confirmed_count,
                "pending_transactions": transactions.len() - confirmed_count,
                "total_amount": total_amount,
                "total_fees": total_fees,
                "total_amount_tari": (total_amount as f64 / 1_000_000.0),
                "total_fees_tari": (total_fees as f64 / 1_000_000.0),
            }
        }))
    }
}

/// Tool for getting completed transactions with optional filtering
#[derive(Clone)]
pub struct GetCompletedTransactionsTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl GetCompletedTransactionsTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetCompletedTransactionsTool {
    fn name(&self) -> &str {
        "get_completed_transactions"
    }

    fn description(&self) -> &str {
        "Retrieves completed transactions with optional filtering by payment ID or block"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "payment_id": {
                    "type": "string",
                    "description": "Optional payment ID to filter by"
                },
                "block_hash": {
                    "type": "string",
                    "description": "Optional block hash to filter by"
                },
                "block_height": {
                    "type": "number",
                    "description": "Optional block height to filter by",
                    "minimum": 0
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of transactions to return (default: 100)",
                    "minimum": 1,
                    "maximum": 1000
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let payment_id = get_optional_string_param(&params, "payment_id").map(|payment_id_str| UserPaymentId {
            utf8_string: payment_id_str,
            u256: vec![],
            user_bytes: vec![],
        });

        let block_hash = get_optional_string_param(&params, "block_hash").map(|hash| BlockHashHex { hash });
        let block_height = params
            .get("block_height")
            .and_then(|v| v.as_u64())
            .map(|h| BlockHeight { block_height: h });

        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(100);

        // Capture the filter states before moving the values
        let has_payment_id = payment_id.is_some();
        let has_block_hash = block_hash.is_some();
        let has_block_height = block_height.is_some();

        let request = Request::new(GetCompletedTransactionsRequest {
            payment_id,
            block_hash,
            block_height,
        });

        let mut client = (*self.grpc_client).clone();
        let mut response_stream = client
            .get_completed_transactions(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get completed transactions: {}", e)))?
            .into_inner();

        let mut transactions = Vec::new();
        let mut count = 0;

        while let Some(tx_response) = response_stream
            .message()
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read transaction stream: {}", e)))?
        {
            if count >= limit {
                break;
            }

            if let Some(transaction) = tx_response.transaction {
                transactions.push(json!({
                    "tx_id": transaction.tx_id,
                    "source_address": hex::encode(&transaction.source_address),
                    "dest_address": hex::encode(&transaction.dest_address),
                    "status": match transaction.status {
                        0 => "COMPLETED",
                        1 => "BROADCAST",
                        2 => "MINED_UNCONFIRMED",
                        3 => "IMPORTED",
                        4 => "PENDING",
                        5 => "COINBASE",
                        6 => "MINED_CONFIRMED",
                        7 => "REJECTED",
                        8 => "CANCELLED",
                        9 => "NOT_FOUND",
                        _ => "UNKNOWN",
                    },
                    "direction": match transaction.direction {
                        0 => "UNKNOWN",
                        1 => "INBOUND",
                        2 => "OUTBOUND",
                        _ => "UNKNOWN",
                    },
                    "amount": transaction.amount,
                    "fee": transaction.fee,
                    "timestamp": transaction.timestamp,
                    "payment_id": hex::encode(&transaction.user_payment_id),
                    "mined_in_block_height": transaction.mined_in_block_height,
                    "formatted": {
                        "amount_tari": (transaction.amount as f64 / 1_000_000.0),
                        "fee_tari": (transaction.fee as f64 / 1_000_000.0),
                        "date": if transaction.timestamp > 0 {
                            chrono::DateTime::from_timestamp(transaction.timestamp as i64, 0)
                                .map(|dt| dt.format("%Y-%m-%d").to_string())
                                .unwrap_or_else(|| "Invalid date".to_string())
                        } else {
                            "No date".to_string()
                        },
                        "time": if transaction.timestamp > 0 {
                            chrono::DateTime::from_timestamp(transaction.timestamp as i64, 0)
                                .map(|dt| dt.format("%H:%M:%S").to_string())
                                .unwrap_or_else(|| "Invalid time".to_string())
                        } else {
                            "No time".to_string()
                        },
                    },
                }));
                count += 1;
            }
        }

        // Analyze transaction patterns
        let inbound_count = transactions
            .iter()
            .filter(|tx| tx["direction"].as_str() == Some("INBOUND"))
            .count();
        let outbound_count = transactions
            .iter()
            .filter(|tx| tx["direction"].as_str() == Some("OUTBOUND"))
            .count();

        let total_inbound: u64 = transactions
            .iter()
            .filter(|tx| tx["direction"].as_str() == Some("INBOUND"))
            .filter_map(|tx| tx["amount"].as_u64())
            .sum();
        let total_outbound: u64 = transactions
            .iter()
            .filter(|tx| tx["direction"].as_str() == Some("OUTBOUND"))
            .filter_map(|tx| tx["amount"].as_u64())
            .sum();
        let total_fees: u64 = transactions.iter().filter_map(|tx| tx["fee"].as_u64()).sum();

        Ok(json!({
            "transactions": transactions,
            "summary": {
                "total_transactions": transactions.len(),
                "inbound_transactions": inbound_count,
                "outbound_transactions": outbound_count,
                "total_inbound_amount": total_inbound,
                "total_outbound_amount": total_outbound,
                "total_fees_paid": total_fees,
                "net_balance_change": total_inbound as i64 - total_outbound as i64 - total_fees as i64,
            },
            "formatted_summary": {
                "total_inbound_tari": (total_inbound as f64 / 1_000_000.0),
                "total_outbound_tari": (total_outbound as f64 / 1_000_000.0),
                "total_fees_tari": (total_fees as f64 / 1_000_000.0),
                "net_change_tari": ((total_inbound as i64 - total_outbound as i64 - total_fees as i64) as f64 / 1_000_000.0),
            },
            "metadata": {
                "limit_applied": limit,
                "results_truncated": count >= limit,
                "filters": {
                    "payment_id": has_payment_id,
                    "block_hash": has_block_hash,
                    "block_height": has_block_height,
                }
            }
        }))
    }
}

/// Enhanced transfer tool with improved validation and features
#[derive(Clone)]
pub struct TransferTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl TransferTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for TransferTool {
    fn name(&self) -> &str {
        "transfer"
    }

    fn description(&self) -> &str {
        "Execute transfers to one or more recipients with comprehensive validation and options"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
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
                                "type": "number",
                                "description": "Amount to transfer (in microTari)",
                                "minimum": 1
                            },
                            "fee_per_gram": {
                                "type": "number",
                                "description": "Fee per gram for this transfer",
                                "minimum": 1
                            },
                            "payment_type": {
                                "type": "number",
                                "description": "Payment type (default: 0 for standard)",
                                "minimum": 0
                            },
                            "message": {
                                "type": "string",
                                "description": "Optional message for this transfer"
                            }
                        },
                        "required": ["address", "amount", "fee_per_gram"]
                    },
                    "minItems": 1,
                    "description": "Array of transfer recipients"
                }
            },
            "required": ["recipients"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let recipients_array = params
            .get("recipients")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::invalid_request("recipients array is required"))?;

        if recipients_array.is_empty() {
            return Err(McpError::invalid_request("At least one recipient is required"));
        }

        let mut recipients = Vec::new();
        let mut total_amount = 0u64;

        for (i, recipient_data) in recipients_array.iter().enumerate() {
            let address = recipient_data
                .get("address")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_request(format!("address is required for recipient {}", i)))?;

            let amount = recipient_data
                .get("amount")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| McpError::invalid_request(format!("amount is required for recipient {}", i)))?;

            if amount == 0 {
                return Err(McpError::invalid_request(format!(
                    "amount must be greater than 0 for recipient {}",
                    i
                )));
            }

            let fee_per_gram = recipient_data
                .get("fee_per_gram")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| McpError::invalid_request(format!("fee_per_gram is required for recipient {}", i)))?;

            let payment_type = recipient_data.get("payment_type").and_then(|v| v.as_u64()).unwrap_or(0); // Default to standard payment

            let raw_payment_id = recipient_data
                .get("payment_id")
                .and_then(|v| v.as_str())
                .map(|s| s.as_bytes().to_vec())
                .unwrap_or_default();

            let user_payment_id = recipient_data
                .get("payment_id")
                .and_then(|v| v.as_str())
                .map(|payment_id_str| UserPaymentId {
                    utf8_string: payment_id_str.to_string(),
                    u256: vec![],
                    user_bytes: vec![],
                });

            recipients.push(PaymentRecipient {
                address: address.to_string(),
                amount,
                fee_per_gram,
                payment_type: payment_type as i32,
                raw_payment_id,
                user_payment_id,
            });

            total_amount += amount;
        }

        // Validate total amount
        if total_amount > 1_000_000_000_000 {
            // 1 million Tari
            return Err(McpError::invalid_request("Total transfer amount exceeds safety limit"));
        }

        let request = Request::new(TransferRequest { recipients });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .transfer(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to execute transfer: {}", e)))?
            .into_inner();

        let results: Vec<Value> = response
            .results
            .iter()
            .map(|result| {
                json!({
                    "address": result.address,
                    "transaction_id": result.transaction_id,
                    "is_success": result.is_success,
                    "failure_message": result.failure_message,
                })
            })
            .collect();

        let successful_transfers = results
            .iter()
            .filter(|r| r["is_success"].as_bool().unwrap_or(false))
            .count();

        let failed_transfers = results.len() - successful_transfers;

        Ok(json!({
            "results": results,
            "summary": {
                "total_recipients": results.len(),
                "successful_transfers": successful_transfers,
                "failed_transfers": failed_transfers,
                "total_amount": total_amount,
                "total_amount_tari": (total_amount as f64 / 1_000_000.0),
                "success_rate": if !results.is_empty() {
                    (successful_transfers as f64 / results.len() as f64 * 100.0).round()
                } else {
                    0.0
                },
            },
            "transaction_ids": results.iter()
                .filter(|r| r["is_success"].as_bool().unwrap_or(false))
                .filter_map(|r| r["transaction_id"].as_u64())
                .collect::<Vec<_>>(),
        }))
    }
}

/// Tool for coin splitting operations
#[derive(Clone)]
pub struct CoinSplitTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl CoinSplitTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for CoinSplitTool {
    fn name(&self) -> &str {
        "coin_split"
    }

    fn description(&self) -> &str {
        "Splits wallet funds into multiple smaller outputs for improved transaction flexibility"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "amount_per_split": {
                    "type": "number",
                    "description": "Amount per split output (in microTari)",
                    "minimum": 1
                },
                "split_count": {
                    "type": "number",
                    "description": "Number of outputs to create",
                    "minimum": 1,
                    "maximum": 100
                },
                "fee_per_gram": {
                    "type": "number",
                    "description": "Fee per gram for the transaction",
                    "minimum": 1
                },
                "lock_height": {
                    "type": "number",
                    "description": "Optional lock height for outputs",
                    "minimum": 0
                }
            },
            "required": ["amount_per_split", "split_count", "fee_per_gram"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let amount_per_split = get_required_u64_param(&params, "amount_per_split")?;
        let split_count = get_required_u64_param(&params, "split_count")?;
        let fee_per_gram = get_required_u64_param(&params, "fee_per_gram")?;

        if amount_per_split == 0 {
            return Err(McpError::invalid_request("amount_per_split must be greater than 0"));
        }

        if split_count == 0 || split_count > 100 {
            return Err(McpError::invalid_request("split_count must be between 1 and 100"));
        }

        if fee_per_gram == 0 {
            return Err(McpError::invalid_request("fee_per_gram must be greater than 0"));
        }

        let lock_height = params.get("lock_height").and_then(|v| v.as_u64()).unwrap_or(0);

        let payment_id = if let Some(payment_id_str) = params.get("payment_id").and_then(|v| v.as_str()) {
            payment_id_str.as_bytes().to_vec()
        } else {
            vec![]
        };

        let total_amount = amount_per_split * split_count;

        let request = Request::new(CoinSplitRequest {
            amount_per_split,
            split_count,
            fee_per_gram,
            lock_height,
            payment_id,
        });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .coin_split(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to execute coin split: {}", e)))?
            .into_inner();

        Ok(json!({
            "transaction_id": response.tx_id,
            "split_details": {
                "amount_per_split": amount_per_split,
                "split_count": split_count,
                "total_amount": total_amount,
                "fee_per_gram": fee_per_gram,
                "lock_height": lock_height,
            },
            "formatted": {
                "amount_per_split_tari": (amount_per_split as f64 / 1_000_000.0),
                "total_amount_tari": (total_amount as f64 / 1_000_000.0),
            },
            "benefits": [
                "Improves transaction flexibility by creating multiple smaller UTXOs",
                "Enables parallel spending for faster transaction processing",
                "Reduces likelihood of change outputs in future transactions"
            ],
            "estimated_completion": "Transaction will be confirmed within the next few blocks"
        }))
    }
}

/// Tool for cancelling transactions
#[derive(Clone)]
pub struct CancelTransactionTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl CancelTransactionTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for CancelTransactionTool {
    fn name(&self) -> &str {
        "cancel_transaction"
    }

    fn description(&self) -> &str {
        "Cancels a pending transaction by its transaction ID"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tx_id": {
                    "type": "number",
                    "description": "Transaction ID to cancel",
                    "minimum": 1
                }
            },
            "required": ["tx_id"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let tx_id = get_required_u64_param(&params, "tx_id")?;

        let request = Request::new(CancelTransactionRequest { tx_id });

        let mut client = (*self.grpc_client).clone();
        let response = client
            .cancel_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to cancel transaction: {}", e)))?
            .into_inner();

        let message = if response.is_success {
            "Transaction has been successfully cancelled".to_string()
        } else {
            format!("Cancellation failed: {}", response.failure_message)
        };

        Ok(json!({
            "tx_id": tx_id,
            "is_success": response.is_success,
            "failure_message": response.failure_message,
            "status": if response.is_success {
                "CANCELLED"
            } else {
                "CANCELLATION_FAILED"
            },
            "message": message,
            "note": if response.is_success {
                "Funds will be available for spending again shortly"
            } else {
                "Transaction may have already been mined or does not exist"
            }
        }))
    }
}

/// Tool for transaction analysis and insights
#[derive(Clone)]
pub struct TransactionAnalysisTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl TransactionAnalysisTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for TransactionAnalysisTool {
    fn name(&self) -> &str {
        "transaction_analysis"
    }

    fn description(&self) -> &str {
        "Provides comprehensive analysis of wallet transaction patterns and insights"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "days_back": {
                    "type": "number",
                    "description": "Number of days to analyze (default: 30)",
                    "minimum": 1,
                    "maximum": 365
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum number of transactions to analyze (default: 1000)",
                    "minimum": 1,
                    "maximum": 10000
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let days_back = params.get("days_back").and_then(|v| v.as_u64()).unwrap_or(30); // Default to 30 days

        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(1000); // Analyze up to 1000 transactions

        // Get completed transactions
        let request = Request::new(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        });

        let mut client = (*self.grpc_client).clone();
        let mut response_stream = client
            .get_completed_transactions(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get transactions: {}", e)))?
            .into_inner();

        let mut transactions = Vec::new();
        let mut count = 0;
        let cutoff_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() -
            (days_back * 24 * 60 * 60);

        while let Some(tx_response) = response_stream
            .message()
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read transaction: {}", e)))?
        {
            if count >= limit {
                break;
            }

            if let Some(transaction) = tx_response.transaction {
                if transaction.timestamp >= cutoff_timestamp {
                    transactions.push(transaction);
                    count += 1;
                }
            }
        }

        if transactions.is_empty() {
            return Ok(json!({
                "message": format!("No transactions found in the last {} days", days_back),
                "analysis_period": {
                    "days_analyzed": days_back,
                    "transactions_found": 0,
                }
            }));
        }

        // Analyze transaction patterns
        let total_transactions = transactions.len();
        let inbound_txs: Vec<_> = transactions.iter().filter(|tx| tx.direction == 1).collect();
        let outbound_txs: Vec<_> = transactions.iter().filter(|tx| tx.direction == 2).collect();

        let total_inbound: u64 = inbound_txs.iter().map(|tx| tx.amount).sum();
        let total_outbound: u64 = outbound_txs.iter().map(|tx| tx.amount).sum();
        let total_fees: u64 = outbound_txs.iter().map(|tx| tx.fee).sum();

        // Calculate averages
        let avg_inbound = if !inbound_txs.is_empty() {
            total_inbound / inbound_txs.len() as u64
        } else {
            0
        };
        let avg_outbound = if !outbound_txs.is_empty() {
            total_outbound / outbound_txs.len() as u64
        } else {
            0
        };
        let avg_fee = if !outbound_txs.is_empty() {
            total_fees / outbound_txs.len() as u64
        } else {
            0
        };

        // Transaction frequency analysis
        let transactions_per_day = total_transactions as f64 / days_back as f64;

        // Fee analysis
        let fee_rates: Vec<u64> = outbound_txs.iter().filter(|tx| tx.fee > 0).map(|tx| tx.fee).collect();

        let (min_fee, max_fee) = if !fee_rates.is_empty() {
            (*fee_rates.iter().min().unwrap(), *fee_rates.iter().max().unwrap())
        } else {
            (0, 0)
        };

        Ok(json!({
            "analysis_summary": {
                "period_days": days_back,
                "total_transactions": total_transactions,
                "inbound_transactions": inbound_txs.len(),
                "outbound_transactions": outbound_txs.len(),
                "transaction_frequency": {
                    "per_day": (transactions_per_day * 100.0).round() / 100.0,
                    "per_week": (transactions_per_day * 7.0 * 100.0).round() / 100.0,
                }
            },
            "financial_summary": {
                "total_inbound": total_inbound,
                "total_outbound": total_outbound,
                "total_fees_paid": total_fees,
                "net_change": total_inbound as i64 - total_outbound as i64 - total_fees as i64,
                "formatted": {
                    "total_inbound_tari": (total_inbound as f64 / 1_000_000.0),
                    "total_outbound_tari": (total_outbound as f64 / 1_000_000.0),
                    "total_fees_tari": (total_fees as f64 / 1_000_000.0),
                    "net_change_tari": ((total_inbound as i64 - total_outbound as i64 - total_fees as i64) as f64 / 1_000_000.0),
                }
            },
            "transaction_patterns": {
                "average_inbound": avg_inbound,
                "average_outbound": avg_outbound,
                "average_fee": avg_fee,
                "fee_range": {
                    "min_fee": min_fee,
                    "max_fee": max_fee,
                    "avg_fee": avg_fee,
                },
                "spending_ratio": if total_inbound > 0 {
                    (total_outbound as f64 / total_inbound as f64 * 100.0).round()
                } else {
                    0.0
                },
            },
            "insights": {
                "activity_level": match transactions_per_day {
                    f if f >= 5.0 => "VERY_HIGH",
                    f if f >= 2.0 => "HIGH",
                    f if f >= 0.5 => "MODERATE",
                    f if f >= 0.1 => "LOW",
                    _ => "VERY_LOW",
                },
                "spending_behavior": match total_outbound.cmp(&total_inbound) {
                    std::cmp::Ordering::Greater => "NET_SPENDER",
                    std::cmp::Ordering::Less => "NET_ACCUMULATOR",
                    std::cmp::Ordering::Equal => "BALANCED",
                },
                "fee_efficiency": if avg_fee <= 25 {
                    "EFFICIENT"
                } else if avg_fee <= 100 {
                    "MODERATE"
                } else {
                    "HIGH_FEES"
                },
            },
            "recommendations": {
                "fee_optimization": if avg_fee > 50 {
                    "Consider using lower fee rates for non-urgent transactions"
                } else {
                    "Fee usage appears optimal"
                },
                "transaction_consolidation": if transactions_per_day > 3.0 {
                    "Consider batching smaller transactions to reduce fees"
                } else {
                    "Transaction frequency appears reasonable"
                },
            }
        }))
    }
}
