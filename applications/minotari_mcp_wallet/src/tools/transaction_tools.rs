// Copyright 2025, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Transaction-related MCP tools for wallet operations
//!
//! This module provides comprehensive transaction management including
//! transaction history, creation, cancellation, and analysis.

use std::sync::Arc;

use minotari_app_grpc::tari_rpc::GetCompletedTransactionsRequest;
use minotari_mcp_common::{McpResult, McpTool, node_rpc, wallet_rpc};
use minotari_wallet_grpc_client::WalletGrpcClient;
use serde_json::{Value, json};
use tonic::{Request, transport::Channel};

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
        wallet_rpc::get_transaction_info((*self.grpc_client).clone(), &params).await
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
                    "maximum": 512
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        wallet_rpc::get_completed_transactions((*self.grpc_client).clone(), &params).await
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
                                "description": "Payment type: 2 = ONE_SIDED_TO_STEALTH_ADDRESS (default), 1 = ONE_SIDED; 0 is rejected",
                                "minimum": 1,
                                "maximum": 2,
                                "default": 2
                            },
                            "payment_id": {
                                "type": "string",
                                "description": "Payment ID text, sent as user_payment_id"
                            }
                        },
                        "required": ["address", "amount", "fee_per_gram"],
                        "additionalProperties": false
                    },
                    "minItems": 1,
                    "description": "Array of transfer recipients"
                }
            },
            "required": ["recipients"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        wallet_rpc::transfer((*self.grpc_client).clone(), &params).await
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
                }
            },
            "required": ["amount_per_split", "split_count", "fee_per_gram"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        wallet_rpc::coin_split((*self.grpc_client).clone(), &params).await
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
        wallet_rpc::cancel_transaction((*self.grpc_client).clone(), &params).await
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
                    "description": "Maximum number of transactions in the period to analyze (default and maximum: 512)",
                    "minimum": 1,
                    "maximum": 512
                }
            }
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let days_back = params.get("days_back").and_then(|v| v.as_u64()).unwrap_or(30); // Default to 30 days

        let limit = node_rpc::limit_param(
            &params,
            node_rpc::MAX_STREAM_ITEMS as u64,
            node_rpc::MAX_STREAM_ITEMS as u64,
        );

        // Get completed transactions
        let request = Request::new(GetCompletedTransactionsRequest {
            payment_id: None,
            block_hash: None,
            block_height: None,
        });

        let cutoff_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(days_back.saturating_mul(24 * 60 * 60));

        // The wallet returns transactions in no date order, so filter while reading and cap only those in the window
        let mut client = (*self.grpc_client).clone();
        let response_stream = client.get_completed_transactions(request).await?.into_inner();
        let (transactions, truncated) = node_rpc::collect_limited(response_stream, limit, |tx_response| {
            tx_response
                .transaction
                .clone()
                .filter(|transaction| transaction.timestamp >= cutoff_timestamp)
        })
        .await?;

        if transactions.is_empty() {
            return Ok(json!({
                "message": format!("No transactions found in the last {} days", days_back),
                "analysis_period": {
                    "days_analyzed": days_back,
                    "transactions_found": 0,
                },
                "truncated": truncated,
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
        let avg_inbound = if inbound_txs.is_empty() {
            0
        } else {
            total_inbound.checked_div(inbound_txs.len() as u64).unwrap_or(0)
        };
        let avg_outbound = if outbound_txs.is_empty() {
            0
        } else {
            total_outbound.checked_div(outbound_txs.len() as u64).unwrap_or(0)
        };
        let avg_fee = if outbound_txs.is_empty() {
            0
        } else {
            total_fees.checked_div(outbound_txs.len() as u64).unwrap_or(0)
        };

        // Transaction frequency analysis
        let transactions_per_day = total_transactions as f64 / days_back as f64;

        // Fee analysis
        let fee_rates: Vec<u64> = outbound_txs.iter().filter(|tx| tx.fee > 0).map(|tx| tx.fee).collect();

        let (min_fee, max_fee) = if fee_rates.is_empty() {
            (0, 0)
        } else {
            (*fee_rates.iter().min().unwrap(), *fee_rates.iter().max().unwrap())
        };

        #[allow(clippy::cast_possible_wrap)]
        let net_change = (total_inbound as i64)
            .saturating_sub(total_outbound as i64)
            .saturating_sub(total_fees as i64);

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_possible_wrap)]
        Ok(json!({
            "truncated": truncated,
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
                "net_change": net_change,
                "formatted": {
                    "total_inbound_tari": (total_inbound as f64 / 1_000_000.0),
                    "total_outbound_tari": (total_outbound as f64 / 1_000_000.0),
                    "total_fees_tari": (total_fees as f64 / 1_000_000.0),
                    "net_change_tari": (net_change as f64 / 1_000_000.0),
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
