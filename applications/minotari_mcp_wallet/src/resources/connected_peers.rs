//! Connected peers resource

use minotari_mcp_common::{McpResource, McpResult, McpError};
use minotari_wallet_grpc_client::{WalletGrpcClient, grpc::GetConnectedPeersRequest};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tonic::transport::Channel;

/// Resource providing information about connected peers
pub struct ConnectedPeersResource {
    grpc_client: Arc<WalletGrpcClient<Channel>>,
}

impl ConnectedPeersResource {
    pub fn new(grpc_client: Arc<WalletGrpcClient<Channel>>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait]
impl McpResource for ConnectedPeersResource {
    fn uri(&self) -> &str {
        "connected_peers"
    }

    fn name(&self) -> &str {
        "Connected Peers"
    }

    fn description(&self) -> &str {
        "Information about peers currently connected to the wallet"
    }

    fn mime_type(&self) -> &str {
        "application/json"
    }

    async fn read(&self) -> McpResult<Value> {
        let mut client = self.grpc_client.as_ref().clone();
        
        let response = client
            .get_connected_peers(GetConnectedPeersRequest {})
            .await
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get connected peers: {}", e)))?;

        let peers_response = response.into_inner();
        
        let mut peers = Vec::new();
        for peer in peers_response.connected_peers {
            peers.push(serde_json::json!({
                "node_id": peer.node_id,
                "public_key": peer.public_key,
                "addresses": peer.addresses,
                "connection_direction": peer.connection_direction,
                "last_seen": peer.last_seen,
                "latency_ms": peer.latency_ms,
                "user_agent": peer.user_agent,
                "features": peer.features
            }));
        }

        Ok(serde_json::json!({
            "connected_peers": peers,
            "peer_count": peers.len(),
            "connection_summary": {
                "total_connections": peers.len(),
                "node_connections": peers.len(),
                "connection_health": if peers.len() >= 3 { "excellent" }
                                    else if peers.len() >= 1 { "good" }
                                    else { "disconnected" }
            },
            "last_updated": chrono::Utc::now().timestamp()
        }))
    }
}
