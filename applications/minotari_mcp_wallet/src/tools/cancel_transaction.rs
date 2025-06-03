//! Cancel transaction MCP tool

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param
};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::CancelTransactionRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for cancelling pending transactions
pub struct CancelTransactionTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl CancelTransactionTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for CancelTransactionTool {
    fn name(&self) -> &str {
        "cancel_transaction"
    }

    fn description(&self) -> &str {
        "Cancel a pending transaction. This is a control operation that can modify transaction state."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "transaction_id" => {
                "type": "string",
                "description": "Transaction ID to cancel (as hex string or number)"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let tx_id = get_required_string_param(params, "transaction_id")?;
        
        if tx_id.is_empty() {
            return Err(McpError::invalid_request("Transaction ID cannot be empty"));
        }

        // Validate transaction ID format (should be numeric or hex)
        if tx_id.parse::<u64>().is_err() && !tx_id.starts_with("0x") {
            return Err(McpError::invalid_request("Transaction ID must be a number or hex string"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let tx_id_str = get_required_string_param(&params, "transaction_id")?;
        
        // Parse transaction ID
        let tx_id = if tx_id_str.starts_with("0x") {
            u64::from_str_radix(&tx_id_str[2..], 16)
                .map_err(|_| McpError::invalid_request("Invalid hex transaction ID"))?
        } else {
            tx_id_str.parse::<u64>()
                .map_err(|_| McpError::invalid_request("Invalid transaction ID format"))?
        };

        // Create cancel request
        let request = CancelTransactionRequest {
            tx_id,
        };

        // Execute cancellation
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .cancel_transaction(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to cancel transaction: {}", e)))?;

        let response = response.into_inner();

        Ok(serde_json::json!({
            "success": response.is_success,
            "transaction_id": tx_id,
            "message": if response.is_success { 
                "Transaction cancelled successfully" 
            } else { 
                "Transaction could not be cancelled (may be already mined or invalid)" 
            }
        }))
    }
}
