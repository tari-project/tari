//! Wallet network status resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetNetworkStatusRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing wallet network connectivity status
pub struct NetworkStatusResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl NetworkStatusResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
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
        "Current wallet network connectivity and synchronization status"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        let response = client
            .get_network_status(GetNetworkStatusRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get network status: {}", e)))?;

        let status = response.into_inner();

        Ok(serde_json::json!({
            "status": status.status,
            "avg_latency_ms": status.avg_latency_ms,
            "num_node_connections": status.num_node_connections,
            "connection_status": {
                "online": status.num_node_connections > 0,
                "base_node_connected": status.num_node_connections > 0,
                "wallet_connectivity": if status.num_node_connections > 0 { "connected" } else { "disconnected" }
            },
            "network_info": {
                "latency": {
                    "average_ms": status.avg_latency_ms,
                    "status": if status.avg_latency_ms < 100.0 { "good" }
                             else if status.avg_latency_ms < 500.0 { "fair" }
                             else { "poor" }
                },
                "connections": {
                    "node_connections": status.num_node_connections,
                    "connection_quality": if status.num_node_connections >= 3 { "excellent" }
                                          else if status.num_node_connections >= 1 { "good" }
                                          else { "poor" }
                }
            },
            "last_updated": chrono::Utc::now().timestamp()
        }))
    }
}
