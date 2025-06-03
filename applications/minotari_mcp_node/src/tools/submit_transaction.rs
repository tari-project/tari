//! Submit transaction MCP tool

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::SubmitTransactionRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for submitting transactions to the mempool
pub struct SubmitTransactionTool {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl SubmitTransactionTool {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for SubmitTransactionTool {
    fn name(&self) -> &str {
        "submit_transaction"
    }

    fn description(&self) -> &str {
        "Submit a transaction to the mempool. This is a control operation that can modify blockchain state."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "transaction_hex" => {
                "type": "string",
                "description": "Hexadecimal representation of the transaction to submit"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let tx_hex = get_required_string_param(params, "transaction_hex")?;
        
        if tx_hex.is_empty() {
            return Err(McpError::invalid_request("Transaction hex cannot be empty"));
        }

        // Basic hex validation
        if !tx_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Transaction hex contains invalid characters"));
        }

        if tx_hex.len() % 2 != 0 {
            return Err(McpError::invalid_request("Transaction hex must have even length"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let tx_hex = get_required_string_param(&params, "transaction_hex")?;
        
        // Convert hex to bytes
        let tx_bytes = hex::decode(&tx_hex)
            .map_err(|e| McpError::tool_execution_failed(format!("Invalid hex encoding: {}", e)))?;

        // Create submit request with transaction bytes
        let request = SubmitTransactionRequest {
            transaction: tx_bytes,
        };

        // Submit transaction to mempool
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .submit_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to submit transaction: {}", e)))?;

        let response = response.into_inner();

        // Return result
        Ok(serde_json::json!({
            "success": true,
            "transaction_id": response.transaction_id,
            "message": "Transaction submitted successfully to mempool"
        }))
    }
}
