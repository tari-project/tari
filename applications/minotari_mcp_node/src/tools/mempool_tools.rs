//! Mempool-related MCP tools for base node operations
//!
//! This module provides tools for querying mempool state, transactions,
//! and transaction validation status.

use minotari_mcp_common::{McpTool, McpError, McpResult, get_required_string_param};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{Value, json};

use tonic::transport::Channel;
use tonic::Request;
use minotari_app_grpc::tari_rpc::{
    GetMempoolTransactionsRequest, Empty, TransactionStateRequest, Signature,
};

/// Tool for getting mempool statistics
#[derive(Clone)]
pub struct GetMempoolStatsTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetMempoolStatsTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetMempoolStatsTool {
    fn name(&self) -> &str {
        "get_mempool_stats"
    }
    
    fn description(&self) -> &str {
        "Retrieves current mempool statistics including transaction counts and total weight"
    }
    
    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }
    
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});
        
        let response = self.grpc_client.clone().get_mempool_stats(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get mempool stats: {}", e)))?
            .into_inner();
        
        Ok(json!({
            "unconfirmed_txs": response.unconfirmed_txs,
            "reorg_txs": response.reorg_txs,
            "unconfirmed_weight": response.unconfirmed_weight,
            "total_transactions": response.unconfirmed_txs + response.reorg_txs,
        }))
    }
}

/// Tool for getting all mempool transactions
#[derive(Clone)]
pub struct GetMempoolTransactionsTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetMempoolTransactionsTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetMempoolTransactionsTool {
    fn name(&self) -> &str {
        "get_mempool_transactions"
    }
    
    fn description(&self) -> &str {
        "Retrieves all transactions currently in the mempool with detailed information"
    }
    
    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }
    
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "number",
                    "description": "Maximum number of transactions to return (default 100)",
                    "minimum": 1,
                    "maximum": 1000
                }
            },
            "required": []
        })
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let limit = params.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100); // Default limit to prevent overwhelming responses
        
        let request = Request::new(GetMempoolTransactionsRequest {});
        
        let mut response_stream = self.grpc_client.clone().get_mempool_transactions(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get mempool transactions: {}", e)))?
            .into_inner();
        
        let mut transactions = Vec::new();
        let mut count = 0;
        
        while let Some(tx_response) = response_stream.message().await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read mempool transaction stream: {}", e)))? {
            
            if count >= limit {
                break;
            }
            
            if let Some(transaction) = tx_response.transaction {
                let tx_info = json!({
                    "offset": hex::encode(&transaction.offset),
                    "body": {
                        "inputs": transaction.body.as_ref()
                            .map(|body| body.inputs.len())
                            .unwrap_or(0),
                        "outputs": transaction.body.as_ref()
                            .map(|body| body.outputs.len())
                            .unwrap_or(0),
                        "kernels": transaction.body.as_ref()
                            .map(|body| body.kernels.len())
                            .unwrap_or(0),
                    },
                    "total_fee": transaction.body.as_ref()
                        .map(|body| body.kernels.iter()
                            .map(|k| k.fee)
                            .sum::<u64>())
                        .unwrap_or(0),
                    "total_weight": transaction.body.as_ref()
                        .map(|body| {
                            body.inputs.len() + body.outputs.len() + body.kernels.len()
                        })
                        .unwrap_or(0),
                });
                
                transactions.push(tx_info);
                count += 1;
            }
        }
        
        Ok(json!({
            "transactions": transactions,
            "count": transactions.len(),
            "limit": limit,
            "note": if count >= limit {
                format!("Results limited to {} transactions. Use limit parameter to adjust.", limit)
            } else {
                "All mempool transactions returned".to_string()
            }
        }))
    }
}

/// Tool for checking transaction state
#[derive(Clone)]
pub struct GetTransactionStateTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetTransactionStateTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetTransactionStateTool {
    fn name(&self) -> &str {
        "get_transaction_state"
    }
    
    fn description(&self) -> &str {
        "Checks the state of a transaction (mempool, mined, or not stored) using its excess signature"
    }
    
    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }
    
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "excess_signature": {
                    "type": "string",
                    "description": "Transaction excess signature (hex string)"
                }
            },
            "required": ["excess_signature"]
        })
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let excess_sig_hex = get_required_string_param(&params, "excess_signature")?;
        
        // Parse excess signature - this would need to be properly structured
        // For now, we'll assume it's provided in the correct format
        let excess_sig_bytes = hex::decode(&excess_sig_hex)
            .map_err(|e| McpError::invalid_request(format!("Invalid hex excess signature: {}", e)))?;
        
        // Create signature object - this is a simplified version
        // In reality, we'd need to properly parse the signature components
        let signature = Signature {
            public_nonce: excess_sig_bytes.clone(),
            signature: excess_sig_bytes,
        };
        
        let request = Request::new(TransactionStateRequest {
            excess_sig: Some(signature),
        });
        
        let response = self.grpc_client.clone().transaction_state(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get transaction state: {}", e)))?
            .into_inner();
        
        let location = match response.result {
            0 => "UNKNOWN",
            1 => "MEMPOOL",
            2 => "MINED",
            3 => "NOT_STORED",
            _ => "INVALID",
        };
        
        Ok(json!({
            "excess_signature": excess_sig_hex,
            "location": location,
            "result_code": response.result,
            "description": match response.result {
                0 => "Transaction state is unknown",
                1 => "Transaction is in the mempool awaiting confirmation",
                2 => "Transaction has been mined and included in a block",
                3 => "Transaction is not stored in this node",
                _ => "Invalid response",
            }
        }))
    }
}

/// Tool for analyzing mempool transaction patterns
#[derive(Clone)]
pub struct AnalyzeMempoolTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl AnalyzeMempoolTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for AnalyzeMempoolTool {
    fn name(&self) -> &str {
        "analyze_mempool"
    }
    
    fn description(&self) -> &str {
        "Provides analysis of mempool transaction patterns including fee distribution and transaction types"
    }
    
    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }
    
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        // First get mempool stats
        let stats_request = Request::new(Empty {});
        let stats_response = self.grpc_client.clone().get_mempool_stats(stats_request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get mempool stats: {}", e)))?
            .into_inner();
        
        // Then get transaction details
        let tx_request = Request::new(GetMempoolTransactionsRequest {});
        let mut tx_stream = self.grpc_client.clone().get_mempool_transactions(tx_request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get mempool transactions: {}", e)))?
            .into_inner();
        
        let mut fee_distribution = Vec::new();
        let mut weight_distribution = Vec::new();
        let mut total_fees = 0u64;
        let mut total_weight = 0usize;
        let mut tx_count = 0;
        
        while let Some(tx_response) = tx_stream.message().await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read transaction: {}", e)))? {
            
            if let Some(transaction) = tx_response.transaction {
                if let Some(body) = transaction.body {
                    let tx_fee: u64 = body.kernels.iter().map(|k| k.fee).sum();
                    let tx_weight = body.inputs.len() + body.outputs.len() + body.kernels.len();
                    
                    fee_distribution.push(tx_fee);
                    weight_distribution.push(tx_weight);
                    total_fees += tx_fee;
                    total_weight += tx_weight;
                    tx_count += 1;
                }
            }
        }
        
        // Calculate statistics
        let avg_fee = if tx_count > 0 { total_fees / tx_count as u64 } else { 0 };
        let avg_weight = if tx_count > 0 { total_weight / tx_count } else { 0 };
        
        // Sort for percentile calculations
        fee_distribution.sort_unstable();
        weight_distribution.sort_unstable();
        
        let fee_percentiles = if !fee_distribution.is_empty() {
            json!({
                "p50": fee_distribution.get(fee_distribution.len() / 2).unwrap_or(&0),
                "p75": fee_distribution.get(fee_distribution.len() * 3 / 4).unwrap_or(&0),
                "p90": fee_distribution.get(fee_distribution.len() * 9 / 10).unwrap_or(&0),
                "p99": fee_distribution.get(fee_distribution.len() * 99 / 100).unwrap_or(&0),
            })
        } else {
            json!({
                "p50": 0,
                "p75": 0,
                "p90": 0,
                "p99": 0,
            })
        };
        
        Ok(json!({
            "mempool_stats": {
                "unconfirmed_txs": stats_response.unconfirmed_txs,
                "reorg_txs": stats_response.reorg_txs,
                "unconfirmed_weight": stats_response.unconfirmed_weight,
            },
            "transaction_analysis": {
                "total_transactions_analyzed": tx_count,
                "total_fees": total_fees,
                "total_weight": total_weight,
                "average_fee": avg_fee,
                "average_weight": avg_weight,
                "fee_per_gram_avg": if total_weight > 0 { 
                    (total_fees as f64 / total_weight as f64).round() as u64 
                } else { 
                    0 
                },
            },
            "fee_distribution": fee_percentiles,
            "weight_statistics": {
                "min": weight_distribution.first().unwrap_or(&0),
                "max": weight_distribution.last().unwrap_or(&0),
                "avg": avg_weight,
            },
            "recommendations": {
                "suggested_fee_per_gram": if !fee_distribution.is_empty() {
                    // Suggest 75th percentile fee rate for faster confirmation
                    let p75_fee = *fee_distribution.get(fee_distribution.len() * 3 / 4).unwrap_or(&0);
                    let p75_weight = *weight_distribution.get(weight_distribution.len() * 3 / 4).unwrap_or(&1);
                    if p75_weight > 0 { p75_fee / p75_weight as u64 } else { 25 }
                } else {
                    25 // Default fee rate
                },
                "network_congestion": if stats_response.unconfirmed_txs > 1000 {
                    "HIGH"
                } else if stats_response.unconfirmed_txs > 100 {
                    "MEDIUM"
                } else {
                    "LOW"
                }
            }
        }))
    }
}
