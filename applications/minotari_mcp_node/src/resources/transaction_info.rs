//! Transaction information resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing transaction information by hash
pub struct TransactionInfoResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl TransactionInfoResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for TransactionInfoResource {
    fn uri(&self) -> &str {
        "transaction/{hash}"
    }

    fn name(&self) -> &str {
        "Transaction Information"
    }

    fn description(&self) -> &str {
        "Information about a specific transaction by hash"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    fn supports_templating(&self) -> bool {
        true
    }

    fn resolve_template(&self, params: &HashMap<String, String>) -> McpResult<String> {
        let hash = params.get("hash")
            .ok_or_else(|| McpError::invalid_request("Missing hash parameter"))?;
        
        // Validate hash is hex
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(McpError::invalid_request("Hash must be hexadecimal"));
        }

        if hash.is_empty() {
            return Err(McpError::invalid_request("Hash cannot be empty"));
        }

        Ok(format!("transaction/{}", hash))
    }

    async fn read(&self) -> McpResult<Value> {
        // This will be called with the resolved template
        // For now, return placeholder data
        Ok(serde_json::json!({
            "message": "Transaction information - placeholder implementation"
        }))
    }
}
