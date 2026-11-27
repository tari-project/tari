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
//! Blockchain-related MCP tools for base node operations
//!
//! This module provides comprehensive access to blockchain data including headers,
//! blocks, network status, and synchronization information.

use minotari_app_grpc::tari_rpc::GetNetworkStateRequest;
use minotari_mcp_common::{McpResult, McpTool, node_rpc};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{Value, json};
use tonic::Request;

/// Tool for listing blockchain headers
#[derive(Clone)]
pub struct ListHeadersTool {
    grpc_client: BaseNodeGrpcClient,
}

impl ListHeadersTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for ListHeadersTool {
    fn name(&self) -> &str {
        "list_headers"
    }

    fn description(&self) -> &str {
        "Lists headers in the current best blockchain chain with optional pagination and sorting"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "from_height": {
                    "type": "number",
                    "description": "Starting block height",
                    "minimum": 0
                },
                "to_height": {
                    "type": "number",
                    "description": "Ending block height (optional)",
                    "minimum": 0
                }
            },
            "required": ["from_height"]
        })
    }

    #[allow(clippy::cast_possible_truncation)]
    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::list_headers(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting a specific block header by hash
#[derive(Clone)]
pub struct GetHeaderByHashTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetHeaderByHashTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetHeaderByHashTool {
    fn name(&self) -> &str {
        "get_header_by_hash"
    }

    fn description(&self) -> &str {
        "Retrieves a block header by its hash"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "Block hash to query (hex string)"
                }
            },
            "required": ["hash"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_header_by_hash(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting blocks by height
#[derive(Clone)]
pub struct GetBlocksTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetBlocksTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetBlocksTool {
    fn name(&self) -> &str {
        "get_blocks"
    }

    fn description(&self) -> &str {
        "Retrieves blocks by their heights"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "heights": {
                    "type": "array",
                    "items": {
                        "type": "number",
                        "minimum": 0
                    },
                    "description": "Array of block heights to retrieve"
                }
            },
            "required": ["heights"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_blocks(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting tip information
#[derive(Clone)]
pub struct GetTipInfoTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetTipInfoTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetTipInfoTool {
    fn name(&self) -> &str {
        "get_tip_info"
    }

    fn description(&self) -> &str {
        "Retrieves the current blockchain tip information including height, hash, and node state"
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
        node_rpc::get_tip_info(self.grpc_client.clone()).await
    }
}

/// Tool for getting sync information
#[derive(Clone)]
pub struct GetSyncInfoTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetSyncInfoTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetSyncInfoTool {
    fn name(&self) -> &str {
        "get_sync_info"
    }

    fn description(&self) -> &str {
        "Retrieves blockchain synchronization progress and peer information"
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
        node_rpc::get_sync_info(self.grpc_client.clone()).await
    }
}

/// Tool for getting network difficulty over time
#[derive(Clone)]
pub struct GetNetworkDifficultyTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetNetworkDifficultyTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNetworkDifficultyTool {
    fn name(&self) -> &str {
        "get_network_difficulty"
    }

    fn description(&self) -> &str {
        "Retrieves network difficulty and hash rate estimates over a range of blocks"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "from_tip": {
                    "type": "number",
                    "description": "Number of blocks from tip (at most 511, as the tip is included)",
                    "minimum": 0
                },
                "start_height": {
                    "type": "number",
                    "description": "Start height",
                    "minimum": 0
                },
                "end_height": {
                    "type": "number",
                    "description": "End height",
                    "minimum": 0
                }
            },
            "required": [],
            "description": "Either start_height..=end_height, or from_tip; at most 512 heights"
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_network_difficulty(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting tokens in circulation
#[derive(Clone)]
pub struct GetTokensInCirculationTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetTokensInCirculationTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetTokensInCirculationTool {
    fn name(&self) -> &str {
        "get_tokens_in_circulation"
    }

    fn description(&self) -> &str {
        "Retrieves information about tokens in circulation at specific block heights"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::ReadOnly
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "heights": {
                    "type": "array",
                    "items": {
                        "type": "number",
                        "minimum": 0
                    },
                    "description": "Array of block heights to query"
                }
            },
            "required": ["heights"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_tokens_in_circulation(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting network state
#[derive(Clone)]
pub struct GetNetworkStateTool {
    grpc_client: BaseNodeGrpcClient,
}

impl GetNetworkStateTool {
    pub fn new(grpc_client: BaseNodeGrpcClient) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNetworkStateTool {
    fn name(&self) -> &str {
        "get_network_state"
    }

    fn description(&self) -> &str {
        "Retrieves comprehensive network state including metadata, sync status, and hash rates"
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
        let request = Request::new(GetNetworkStateRequest {});

        let response = self.grpc_client.clone().get_network_state(request).await?.into_inner();

        let metadata = response.metadata.as_ref();

        Ok(json!({
            "metadata": {
                "best_block_height": metadata.map(|m| m.best_block_height).unwrap_or(0),
                "best_block_hash": metadata
                    .map(|m| hex::encode(&m.best_block_hash))
                    .unwrap_or_default(),
                "accumulated_difficulty": metadata
                    .map(|m| hex::encode(&m.accumulated_difficulty))
                    .unwrap_or_default(),
                "pruned_height": metadata.map(|m| m.pruned_height).unwrap_or(0),
                "timestamp": metadata.map(|m| m.timestamp).unwrap_or(0),
            },
            "initial_sync_achieved": response.initial_sync_achieved,
            "base_node_state": match response.base_node_state {
                0 => "START_UP",
                1 => "HEADER_SYNC",
                2 => "HORIZON_SYNC",
                3 => "CONNECTING",
                4 => "BLOCK_SYNC",
                5 => "LISTENING",
                6 => "SYNC_FAILED",
                _ => "UNKNOWN",
            },
            "failed_checkpoints": response.failed_checkpoints,
            "reward": response.reward,
            "sha3x_estimated_hash_rate": response.sha3x_estimated_hash_rate,
            "monero_randomx_estimated_hash_rate": response.monero_randomx_estimated_hash_rate,
            "tari_randomx_estimated_hash_rate": response.tari_randomx_estimated_hash_rate,
            "num_connections": response.num_connections,
            "liveness_results": response.liveness_results.iter().map(|lr| json!({
                "peer_node_id": hex::encode(&lr.peer_node_id),
                "discover_latency": lr.discover_latency,
                "ping_latency": lr.ping_latency,
            })).collect::<Vec<_>>(),
        }))
    }
}
