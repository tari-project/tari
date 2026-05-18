// Copyright 2025, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol) infrastructure for Tari applications
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
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get connected peers: {e}")))?;

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
