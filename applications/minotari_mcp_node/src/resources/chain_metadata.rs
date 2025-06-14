//! Chain metadata resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
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
            .get_tip_info(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get chain metadata: {}", e)))?;

        let tip_info = response.into_inner();

        if let Some(metadata) = tip_info.metadata {
            Ok(serde_json::json!({
                "best_block_height": metadata.best_block_height,
                "best_block_hash": hex::encode(&metadata.best_block_hash),
                "accumulated_difficulty": hex::encode(&metadata.accumulated_difficulty),
                "pruned_height": metadata.pruned_height,
                "timestamp": metadata.timestamp,
                "chain_metadata": {
                    "best_block_height": metadata.best_block_height,
                    "accumulated_difficulty": hex::encode(&metadata.accumulated_difficulty),
                    "pruned_height": metadata.pruned_height,
                }
            }))
        } else {
            Ok(serde_json::json!({
                "error": "No metadata available",
                "initial_sync_achieved": tip_info.initial_sync_achieved,
                "base_node_state": tip_info.base_node_state
            }))
        }
    }
}
