//! Block information resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing block information by height
pub struct BlockInfoResource {
    #[allow(dead_code)]
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl BlockInfoResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for BlockInfoResource {
    fn uri(&self) -> &str {
        "block/{height}"
    }

    fn name(&self) -> &str {
        "Block Information"
    }

    fn description(&self) -> &str {
        "Information about a specific block by height"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    fn supports_templating(&self) -> bool {
        true
    }

    fn resolve_template(&self, params: &HashMap<String, String>) -> McpResult<String> {
        let height = params.get("height")
            .ok_or_else(|| McpError::invalid_request("Missing height parameter"))?;
        
        // Validate height is a number
        height.parse::<u64>()
            .map_err(|_| McpError::invalid_request("Height must be a valid number"))?;

        Ok(format!("block/{}", height))
    }

    async fn read(&self) -> McpResult<Value> {
        // This will be called with the resolved template
        // For now, return placeholder data
        Ok(serde_json::json!({
            "message": "Block information - placeholder implementation"
        }))
    }
}
