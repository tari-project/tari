//! Mining-related MCP tools

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::{GetNewBlockRequest, PowAlgo}};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Tool for getting new block templates for mining
pub struct GetNewBlockTemplateTool {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl GetNewBlockTemplateTool {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpTool for GetNewBlockTemplateTool {
    fn name(&self) -> &str {
        "get_new_block_template"
    }

    fn description(&self) -> &str {
        "Get a new block template for mining. This is a read-only operation that doesn't modify state."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "pow_algo" => {
                "type": "string",
                "description": "Proof-of-work algorithm (sha3x, randomx)",
                "enum": ["sha3x", "randomx"]
            },
            "max_weight" => {
                "type": "number",
                "description": "Maximum block weight",
                "minimum": 1
            }
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let pow_algo = get_required_string_param(params, "pow_algo")?;
        
        match pow_algo.as_str() {
            "sha3x" | "randomx" => {},
            _ => return Err(McpError::invalid_request("Invalid PoW algorithm. Must be 'sha3x' or 'randomx'")),
        }

        if let Some(max_weight) = params.get("max_weight") {
            if let Some(weight) = max_weight.as_f64() {
                if weight <= 0.0 {
                    return Err(McpError::invalid_request("Max weight must be greater than 0"));
                }
            }
        }

        Ok(())
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let pow_algo_str = get_required_string_param(&params, "pow_algo")?;
        let max_weight = params.get("max_weight")
            .and_then(|v| v.as_f64())
            .unwrap_or(19500.0) as u64; // Default block weight

        // Convert algorithm string to gRPC enum
        let pow_algo = match pow_algo_str.as_str() {
            "sha3x" => PowAlgo::Sha3x,
            "randomx" => PowAlgo::RandomX,
            _ => return Err(McpError::invalid_request("Invalid PoW algorithm")),
        };

        let request = GetNewBlockRequest {
            algo: Some(pow_algo.into()),
            max_weight,
        };

        // Get new block template
        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .get_new_block(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get new block template: {}", e)))?;

        let response = response.into_inner();

        // Extract relevant information from the block template
        let block_template = if let Some(block) = response.block {
            serde_json::json!({
                "header": {
                    "version": block.header.as_ref().map(|h| h.version).unwrap_or(0),
                    "height": block.header.as_ref().map(|h| h.height).unwrap_or(0),
                    "prev_hash": block.header.as_ref()
                        .and_then(|h| h.prev_hash.as_ref())
                        .map(|h| hex::encode(h))
                        .unwrap_or_default(),
                    "timestamp": block.header.as_ref().map(|h| h.timestamp).unwrap_or(0),
                    "pow_algo": pow_algo_str,
                },
                "body": {
                    "kernels": block.body.as_ref().map(|b| b.kernels.len()).unwrap_or(0),
                    "outputs": block.body.as_ref().map(|b| b.outputs.len()).unwrap_or(0),
                    "inputs": block.body.as_ref().map(|b| b.inputs.len()).unwrap_or(0),
                }
            })
        } else {
            serde_json::json!({
                "error": "No block template received"
            })
        };

        // Return the block template information
        Ok(serde_json::json!({
            "success": true,
            "pow_algo": pow_algo_str,
            "max_weight": max_weight,
            "miner_data": response.miner_data,
            "block_template": block_template,
            "message": "Block template generated successfully"
        }))
    }
}
