//! Peer list resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing list of known peers
pub struct PeerListResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl PeerListResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for PeerListResource {
    fn uri(&self) -> &str {
        "peer_list"
    }

    fn name(&self) -> &str {
        "Peer List"
    }

    fn description(&self) -> &str {
        "List of known peers and their connection status"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();

        // Test connectivity
        let version_response = client
            .get_version(Empty {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to connect to node: {}", e)))?;

        let _version = version_response.into_inner();

        // TODO: Replace with actual peer list calls when available
        Ok(serde_json::json!({
            "peers": [],
            "message": "Peer list information - placeholder implementation"
        }))
    }
}
