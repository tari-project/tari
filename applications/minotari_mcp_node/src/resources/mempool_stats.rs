//! Mempool statistics resource

use std::sync::Arc;

use async_trait::async_trait;
use minotari_mcp_common::{McpError, McpResource, McpResult};
use minotari_node_grpc_client::{grpc::Empty, BaseNodeGrpcClient};
use serde_json::Value;
use tonic::transport::Channel;

/// Resource providing mempool statistics
pub struct MempoolStatsResource {
    grpc_client: Arc<BaseNodeGrpcClient<Channel>>,
}

impl MempoolStatsResource {
    pub fn new(grpc_client: Arc<BaseNodeGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for MempoolStatsResource {
    fn uri(&self) -> &str {
        "mempool_stats"
    }

    fn name(&self) -> &str {
        "Mempool Statistics"
    }

    fn description(&self) -> &str {
        "Current mempool statistics including transaction count and size"
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

        // TODO: Replace with actual mempool stats calls when available
        Ok(serde_json::json!({
            "total_txs": 0,
            "total_weight": 0,
            "message": "Mempool statistics - placeholder implementation"
        }))
    }
}
