//! Mining-related MCP tools

use minotari_mcp_common::{
    McpTool, McpResult, McpError, PermissionLevel,
    json_schema, get_required_string_param
};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::{NewBlockTemplateRequest, PowAlgo, pow_algo::PowAlgos}};
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
        "Get a new block template for mining. This is a control operation that prepares mining work."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Control
    }

    fn input_schema(&self) -> Value {
        json_schema! {
            "pow_algo" => serde_json::json!({
                "type": "string",
                "description": "Proof-of-work algorithm to use",
                "enum": ["sha3x", "randomxm", "randomxt"]
            }),
            "max_weight" => serde_json::json!({
                "type": "number",
                "description": "Maximum block weight (optional, default: 19500)",
                "default": 19500
            })
        }
    }

    fn validate_params(&self, params: &Value) -> McpResult<()> {
        let pow_algo = get_required_string_param(params, "pow_algo")?;
        
        match pow_algo.as_str() {
            "sha3x" | "randomxm" | "randomxt" => Ok(()),
            _ => Err(McpError::invalid_request("Invalid PoW algorithm. Must be one of: sha3x, randomxm, randomxt")),
        }
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let pow_algo_str = get_required_string_param(&params, "pow_algo")?;
        let max_weight = params.get("max_weight")
            .and_then(|v| v.as_f64())
            .unwrap_or(19500.0) as u64;

        let pow_algos = match pow_algo_str.as_str() {
            "sha3x" => PowAlgos::Sha3x,
            "randomxm" => PowAlgos::Randomxm,
            "randomxt" => PowAlgos::Randomxt,
            _ => return Err(McpError::invalid_request("Invalid PoW algorithm")),
        };

        let pow_algo = PowAlgo {
            pow_algo: pow_algos as i32,
        };

        let request = NewBlockTemplateRequest {
            algo: Some(pow_algo),
            max_weight,
        };

        let mut client = self.grpc_client.as_ref().clone();
        let response = client
            .get_new_block_template(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get new block template: {}", e)))?;

        let response = response.into_inner();

        let block_template = if let Some(block) = response.new_block_template {
            serde_json::json!({
                "header": {
                    "version": block.header.as_ref().map(|h| h.version).unwrap_or(0),
                    "height": block.header.as_ref().map(|h| h.height).unwrap_or(0),
                    "prev_hash": block.header.as_ref()
                        .map(|h| hex::encode(&h.prev_hash))
                        .unwrap_or_default(),
                    "pow_data": block.header.as_ref().and_then(|h| h.pow.as_ref()).map(|p| hex::encode(&p.pow_data)).unwrap_or_default(),
                    "pow_algo": pow_algo_str,
                },
                "body": {
                    "kernels": block.body.as_ref().map(|b| b.kernels.len()).unwrap_or(0),
                    "outputs": block.body.as_ref().map(|b| b.outputs.len()).unwrap_or(0),
                    "inputs": block.body.as_ref().map(|b| b.inputs.len()).unwrap_or(0),
                },
                "mempool_in_sync": block.is_mempool_in_sync
            })
        } else {
            serde_json::json!({
                "error": "No block template received"
            })
        };

        Ok(serde_json::json!({
            "success": true,
            "pow_algo": pow_algo_str,
            "max_weight": max_weight,
            "has_miner_data": response.miner_data.is_some(),
            "initial_sync_achieved": response.initial_sync_achieved,
            "block_template": block_template,
            "message": "Block template generated successfully"
        }))
    }
}
