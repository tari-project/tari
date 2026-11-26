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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.//! Common MCP (Model Context Protocol)
// infrastructure for Tari applications
//! Network-related MCP tools for base node operations
//!
//! This module provides tools for querying network status, peer information,
//! connectivity, and node identity.

use minotari_app_grpc::tari_rpc::Empty;
use minotari_mcp_common::{McpResult, McpTool, node_rpc};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{Value, json};
use tonic::Request;

/// Tool for getting network status
#[derive(Clone)]
pub struct GetNetworkStatusTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetNetworkStatusTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNetworkStatusTool {
    fn name(&self) -> &str {
        "get_network_status"
    }

    fn description(&self) -> &str {
        "Retrieves base node network connectivity status and connection information"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: Value) -> McpResult<Value> {
        node_rpc::get_network_status(self.grpc_client.clone()).await
    }
}

/// Tool for listing connected peers
#[derive(Clone)]
pub struct ListConnectedPeersTool {
    grpc_client: BaseNodeGrpcClient,
}

impl ListConnectedPeersTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[allow(clippy::indexing_slicing)]
#[async_trait::async_trait]
impl McpTool for ListConnectedPeersTool {
    fn name(&self) -> &str {
        "list_connected_peers"
    }

    fn description(&self) -> &str {
        "Lists all peers currently connected to the base node with detailed connection information"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: Value) -> McpResult<Value> {
        node_rpc::list_connected_peers(self.grpc_client.clone()).await
    }
}

/// Tool for getting all peers (including disconnected)
#[derive(Clone)]
pub struct GetAllPeersTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetAllPeersTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[allow(clippy::indexing_slicing)]
#[async_trait::async_trait]
impl McpTool for GetAllPeersTool {
    fn name(&self) -> &str {
        "get_all_peers"
    }

    fn description(&self) -> &str {
        "Retrieves information about all known peers, including both connected and disconnected peers"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "number",
                    "description": "Maximum number of peers to return (default 50)",
                    "minimum": 1,
                    "maximum": 512
                }
            },
            "required": []
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_peers(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting node identity
#[derive(Clone)]
pub struct GetNodeIdentityTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetNodeIdentityTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNodeIdentityTool {
    fn name(&self) -> &str {
        "get_node_identity"
    }

    fn description(&self) -> &str {
        "Retrieves the base node's network identity including public key and node ID"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: Value) -> McpResult<Value> {
        let request = Request::new(Empty {});

        let response = self.grpc_client.clone().identify(request).await?.into_inner();

        Ok(json!({
            "public_key": hex::encode(&response.public_key),
            "node_id": hex::encode(&response.node_id),
            "public_address": response.public_addresses,
            "identity_info": {
                "public_key_length": response.public_key.len(),
                "node_id_length": response.node_id.len(),
                "has_public_address": !response.public_addresses.is_empty(),
            }
        }))
    }
}

/// Tool for network diagnostics
#[derive(Clone)]
pub struct NetworkDiagnosticsTool {
    grpc_client: BaseNodeGrpcClient,
}

impl NetworkDiagnosticsTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for NetworkDiagnosticsTool {
    fn name(&self) -> &str {
        "network_diagnostics"
    }

    fn description(&self) -> &str {
        "Performs comprehensive network diagnostics including connectivity, performance, and peer analysis"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Privileged
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        // Get network status
        let status_request = Request::new(Empty {});
        let network_status = self
            .grpc_client
            .clone()
            .get_network_status(status_request)
            .await?
            .into_inner();

        // Get connected peers
        let peers_request = Request::new(Empty {});
        let connected_peers = self
            .grpc_client
            .clone()
            .list_connected_peers(peers_request)
            .await?
            .into_inner();

        // Get node identity
        let identity_request = Request::new(Empty {});
        let node_identity = self.grpc_client.clone().identify(identity_request).await?.into_inner();

        // Analyze network health
        let peer_count = connected_peers.connected_peers.len();
        let banned_peers = connected_peers
            .connected_peers
            .iter()
            .filter(|p| p.banned_until > 0)
            .count();

        let health_score = if peer_count >= 8 && network_status.avg_latency_ms <= 200 {
            100
        } else if peer_count >= 4 && network_status.avg_latency_ms <= 500 {
            75
        } else if peer_count >= 1 && network_status.avg_latency_ms <= 1000 {
            50
        } else if peer_count >= 1 {
            25
        } else {
            0
        };

        // Generate recommendations
        let mut recommendations = Vec::new();

        if peer_count < 4 {
            recommendations.push(
                "Consider checking firewall settings or network connectivity - fewer than 4 peers connected"
                    .to_string(),
            );
        }
        if network_status.avg_latency_ms > 1000 {
            recommendations.push("High network latency detected - check internet connection quality".to_string());
        }
        if banned_peers > 0 {
            recommendations.push(format!(
                "{banned_peers} banned peers detected - this may indicate network issues"
            ));
        }
        if peer_count == 0 {
            recommendations
                .push("No peers connected - check network configuration and ensure ports are open".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("Network appears healthy - no issues detected".to_string());
        }

        Ok(json!({
            "network_health": {
                "overall_score": health_score,
                "status": match health_score {
                    90..=100 => "EXCELLENT",
                    70..=89 => "GOOD",
                    40..=69 => "FAIR",
                    1..=39 => "POOR",
                    _ => "CRITICAL",
                },
            },
            "connectivity": {
                "connected_peers": peer_count,
                "banned_peers": banned_peers,
                "healthy_peers": peer_count.saturating_sub(banned_peers),
                "avg_latency_ms": network_status.avg_latency_ms,
                "num_node_connections": network_status.num_node_connections,
            },
            "node_identity": {
                "public_key": hex::encode(&node_identity.public_key),
                "node_id": hex::encode(&node_identity.node_id),
                "public_address": node_identity.public_addresses,
            },
            "peer_diversity": {
                "unique_user_agents": connected_peers.connected_peers.iter()
                    .map(|p| p.user_agent.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                "protocol_versions": connected_peers.connected_peers.iter()
                    .flat_map(|p| p.supported_protocols.iter())
                    .map(|proto| String::from_utf8_lossy(proto).to_string())
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
            },
            "recommendations": recommendations,
            "diagnostic_timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }))
    }
}
