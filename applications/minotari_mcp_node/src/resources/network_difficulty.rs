//! Network difficulty resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_node_grpc_client::{BaseNodeGrpcClient, grpc::Empty};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing network difficulty information
pub struct NetworkDifficultyResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl NetworkDifficultyResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for NetworkDifficultyResource {
    fn uri(&self) -> &str {
        "network_difficulty"
    }

    fn name(&self) -> &str {
        "Network Difficulty"
    }

    fn description(&self) -> &str {
        "Current network mining difficulty information"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        // Get chain metadata for difficulty info
        let metadata_response = client
            .get_tip_info(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get metadata: {}", e)))?;

        let tip_info = metadata_response.into_inner();

        if let Some(metadata) = tip_info.metadata {
            Ok(serde_json::json!({
                "accumulated_difficulty": hex::encode(&metadata.accumulated_difficulty),
                "height": metadata.best_block_height,
                "timestamp": metadata.timestamp,
                "message": "Network difficulty information"
            }))
        } else {
            Ok(serde_json::json!({
                "error": "No metadata available",
                "initial_sync_achieved": tip_info.initial_sync_achieved,
                "message": "Network difficulty information unavailable"
            }))
        }
    }
}
