//! Sync progress resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing blockchain synchronization progress
pub struct SyncProgressResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl SyncProgressResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for SyncProgressResource {
    fn uri(&self) -> &str {
        "sync_progress"
    }

    fn name(&self) -> &str {
        "Sync Progress"
    }

    fn description(&self) -> &str {
        "Blockchain synchronization progress and status"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();

        // Get basic version info to test connectivity
        let version_response = client
            .get_version(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to connect to node: {}", e)))?;

        let _version = version_response.into_inner();

        // TODO: Replace with actual sync progress calls when available
        Ok(serde_json::json!({
            "status": "synced",
            "message": "Sync progress information - placeholder implementation"
        }))
    }
}
