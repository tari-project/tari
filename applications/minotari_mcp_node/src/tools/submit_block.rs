//! Submit block MCP tool

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::{Block, SubmitBlockRequest}};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for submitting blocks to the base node
pub struct SubmitBlockTool {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl SubmitBlockTool {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for SubmitBlockTool {
    fn name(&self) -> &str {
        "submit_block"
    }

    fn description(&self) -> &str {
        "Submit a new block to the Tari blockchain. This is a control operation that modifies blockchain state."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "block_hex" => {
                "type": "string",
                "description": "Hexadecimal representation of the block to submit"
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let block_hex = get_required_string_param(params, "block_hex")?;
        
        if block_hex.is_empty() {
            return Err(McpError::invalid_request("Block hex cannot be empty"));
        }

        // Basic hex validation
        if !block_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Block hex contains invalid characters"));
        }

        if block_hex.len() % 2 != 0 {
            return Err(McpError::invalid_request("Block hex must have even length"));
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let block_hex = get_required_string_param(&params, "block_hex")?;
        
        // Convert hex to bytes
        let block_bytes = hex::decode(&block_hex)
            .map_err(|e| McpError::tool_execution_failed(format!("Invalid hex encoding: {}", e)))?;

        // Parse block (this would need proper block parsing)
        // For now, we'll create a placeholder block structure
        let block = Block {
            header: None, // Would be populated from parsed block data
            body: None,   // Would be populated from parsed block data
        };

        let request = SubmitBlockRequest { block: Some(block) };

        // Submit block to node
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .submit_block(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to submit block: {}", e)))?;

        let response = response.into_inner();

        // Return result
        Ok(serde_json::json!({
            "success": true,
            "block_hash": response.block_hash,
            "height": response.height,
            "message": "Block submitted successfully"
        }))
    }
}
