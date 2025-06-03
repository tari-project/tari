//! Chain metadata resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::Empty};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing current blockchain metadata
pub struct ChainMetadataResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl ChainMetadataResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for ChainMetadataResource {
    fn uri(&self) -> &str {
        "chain_metadata"
    }

    fn name(&self) -> &str {
        "Chain Metadata"
    }

    fn description(&self) -> &str {
        "Current blockchain metadata including height, best block hash, and chain tips"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        let response = client
            .get_metadata(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get chain metadata: {}", e)))?;

        let metadata = response.into_inner();

        Ok(serde_json::json!({
            "height_of_longest_chain": metadata.height_of_longest_chain,
            "best_block_hash": hex::encode(&metadata.best_block_hash),
            "best_block_height": metadata.best_block_height,
            "accumulated_difficulty": metadata.accumulated_difficulty.to_string(),
            "pruned_height": metadata.pruned_height,
            "timestamp": metadata.timestamp,
            "chain_metadata": {
                "effective_pruned_height": metadata.effective_pruned_height,
                "accumulated_difficulty": metadata.accumulated_difficulty.to_string(),
                "total_chainwork": metadata.total_chainwork.to_string(),
            }
        }))
    }
}
