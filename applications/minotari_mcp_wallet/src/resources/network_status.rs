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
            .map_err(|e| McpError::resource_access_failed(format!("Failed to get network status: {e}")))?;

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
