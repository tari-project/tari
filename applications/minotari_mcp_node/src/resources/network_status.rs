//! Network status resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing current network status
pub struct NetworkStatusResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl NetworkStatusResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for NetworkStatusResource {
    fn uri(&self) -> &str {
        "network_status"
    }

    fn name(&self) -> &str {
        "Network Status"
    }

    fn description(&self) -> &str {
        "Current network connectivity status including peer connections and sync state"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();

        // Get version info to test connectivity
        let version_response = client
            .get_version(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get version: {}", e)))?;

        let version = version_response.into_inner();

        // TODO: Replace with actual network status calls when gRPC definitions are available
        Ok(serde_json::json!({
            "status": "online",
            "node_version": version.version,
            "network": "unknown",
            "message": "Network status information - placeholder implementation"
        }))
    }
}
