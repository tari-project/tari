//! Coin split MCP tool for splitting coins

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_u64_param
};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::CoinSplitRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for splitting coins into smaller denominations  
pub struct CoinSplitTool {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl CoinSplitTool {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for CoinSplitTool {
    fn name(&self) -> &str {
        "coin_split"
    }

    fn description(&self) -> &str {
        "Split coins into smaller denominations for more efficient transactions. This is a control operation."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "amount_per_split" => {
                "type": "number",
                "description": "Amount in microTari for each split coin"
            },
            "split_count" => {
                "type": "number", 
                "description": "Number of split coins to create"
            },
            "fee_per_gram" => {
                "type": "number",
                "description": "Fee per gram in microTari (optional)"
            },
            "message" => {
                "type": "string",
                "description": "Optional message for the split transaction"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let amount_per_split = get_required_u64_param(params, "amount_per_split")?;
        let split_count = get_required_u64_param(params, "split_count")?;

        if amount_per_split == 0 {
            return Err(McpError::invalid_request("Amount per split must be greater than 0"));
        }

        if split_count == 0 || split_count > 500 {
            return Err(McpError::invalid_request("Split count must be between 1 and 500"));
        }

        // Check for potential overflow
        if amount_per_split.saturating_mul(split_count) != amount_per_split * split_count {
            return Err(McpError::invalid_request("Total split amount would overflow"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let amount_per_split = get_required_u64_param(&params, "amount_per_split")?;
        let split_count = get_required_u64_param(&params, "split_count")?;
        let fee_per_gram = params.get("fee_per_gram").and_then(|v| v.as_u64()).unwrap_or(5);
        let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Create coin split request
        let request = CoinSplitRequest {
            amount_per_split,
            split_count,
            fee_per_gram,
            message,
        };

        // Execute coin split
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .coin_split(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to split coins: {}", e)))?;

        let response = response.into_inner();

        Ok(serde_json::json!({
            "success": true,
            "transaction_id": response.transaction_id,
            "total_amount": amount_per_split * split_count,
            "amount_per_split": amount_per_split,
            "split_count": split_count,
            "fee": response.fee,
            "message": "Coin split transaction created successfully"
        }))
    }
}
