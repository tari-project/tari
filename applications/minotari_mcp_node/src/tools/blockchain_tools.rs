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

use minotari_app_grpc::tari_rpc::{
    Empty,
    GetBlocksRequest,
    GetHeaderByHashRequest,
    GetNetworkStateRequest,
    HeightRequest,
    ListHeadersRequest,
};
use minotari_mcp_common::{get_required_string_param, McpError, McpResult, McpTool};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{json, Value};
use tonic::{transport::Channel, Request};

/// Tool for listing blockchain headers
#[derive(Clone)]
pub struct ListHeadersTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl ListHeadersTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
        let from_height = params.get("from_height").and_then(|v| v.as_u64()).unwrap_or(0);

        let num_headers = params.get("num_headers").and_then(|v| v.as_u64()).unwrap_or(10);

        let sorting = params.get("sorting").and_then(|v| v.as_i64()).unwrap_or(0) as i32; // Default to SORTING_DESC

        let request = Request::new(ListHeadersRequest {
            from_height,
            num_headers,
            sorting,
        });

        let mut response_stream = self
            .grpc_client
            .clone()
            .list_headers(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to list headers: {e}")))?
            .into_inner();

        let mut headers = Vec::new();
        while let Some(header_response) = response_stream
            .message()
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read header stream: {e}")))?
        {
            headers.push(json!({
                "height": header_response.header.as_ref().map(|h| h.height).unwrap_or(0),
                "hash": header_response.header.as_ref()
                    .map(|h| hex::encode(&h.hash))
                    .unwrap_or_default(),
                "confirmations": header_response.confirmations,
                "reward": header_response.reward,
                "difficulty": header_response.difficulty,
                "num_transactions": header_response.num_transactions,
                "timestamp": header_response.header.as_ref()
                    .map(|h| h.timestamp)
                    .unwrap_or(0),
                "version": header_response.header.as_ref()
                    .map(|h| h.version)
                    .unwrap_or(0),
            }));
        }

        Ok(json!({
            "headers": headers,
            "count": headers.len(),
            "from_height": from_height,
            "num_headers": num_headers,
            "sorting": if sorting == 0 { "desc" } else { "asc" }
        }))
    }
}

/// Tool for getting a specific block header by hash
#[derive(Clone)]
pub struct GetHeaderByHashTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetHeaderByHashTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
        let hash_hex = get_required_string_param(&params, "hash")?;

        let hash_bytes =
            hex::decode(&hash_hex).map_err(|e| McpError::invalid_request(format!("Invalid hex hash: {e}")))?;

        let request = Request::new(GetHeaderByHashRequest { hash: hash_bytes });

        let response = self
            .grpc_client
            .clone()
            .get_header_by_hash(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get header: {e}")))?
            .into_inner();

        Ok(json!({
            "header": {
                "height": response.header.as_ref().map(|h| h.height).unwrap_or(0),
                "hash": response.header.as_ref()
                    .map(|h| hex::encode(&h.hash))
                    .unwrap_or_default(),
                "prev_hash": response.header.as_ref()
                    .map(|h| hex::encode(&h.prev_hash))
                    .unwrap_or_default(),
                "timestamp": response.header.as_ref().map(|h| h.timestamp).unwrap_or(0),
                "version": response.header.as_ref().map(|h| h.version).unwrap_or(0),
            },
            "confirmations": response.confirmations,
            "reward": response.reward,
            "difficulty": response.difficulty,
            "num_transactions": response.num_transactions,
        }))
    }
}

/// Tool for getting blocks by height
#[derive(Clone)]
pub struct GetBlocksTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetBlocksTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
        let heights: Vec<u64> = params
            .get("heights")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::invalid_request("heights array is required".to_string()))?
            .iter()
            .filter_map(|v| v.as_u64())
            .collect();

        if heights.is_empty() {
            return Err(McpError::invalid_request("At least one height is required".to_string()));
        }

        let request = Request::new(GetBlocksRequest {
            heights: heights.clone(),
        });

        let mut response_stream = self
            .grpc_client
            .clone()
            .get_blocks(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get blocks: {e}")))?
            .into_inner();

        let mut blocks = Vec::new();
        while let Some(historical_block) = response_stream
            .message()
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read block stream: {e}")))?
        {
            if let Some(block) = historical_block.block {
                blocks.push(json!({
                    "height": block.header.as_ref().map(|h| h.height).unwrap_or(0),
                    "hash": block.header.as_ref()
                        .map(|h| hex::encode(&h.hash))
                        .unwrap_or_default(),
                    "prev_hash": block.header.as_ref()
                        .map(|h| hex::encode(&h.prev_hash))
                        .unwrap_or_default(),
                    "timestamp": block.header.as_ref().map(|h| h.timestamp).unwrap_or(0),
                    "num_inputs": block.body.as_ref().map(|b| b.inputs.len()).unwrap_or(0),
                    "num_outputs": block.body.as_ref().map(|b| b.outputs.len()).unwrap_or(0),
                    "num_kernels": block.body.as_ref().map(|b| b.kernels.len()).unwrap_or(0),
                }));
            }
        }

        Ok(json!({
            "blocks": blocks,
            "count": blocks.len(),
            "requested_heights": heights,
        }))
    }
}

/// Tool for getting tip information
#[derive(Clone)]
pub struct GetTipInfoTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetTipInfoTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
        let request = Request::new(Empty {});

        let response = self
            .grpc_client
            .clone()
            .get_tip_info(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get tip info: {e}")))?
            .into_inner();

        let metadata = response.metadata.as_ref();

        Ok(json!({
            "tip_height": metadata.map(|m| m.best_block_height).unwrap_or(0),
            "tip_hash": metadata
                .map(|m| hex::encode(&m.best_block_hash))
                .unwrap_or_default(),
            "accumulated_difficulty": metadata
                .map(|m| hex::encode(&m.accumulated_difficulty))
                .unwrap_or_default(),
            "pruned_height": metadata.map(|m| m.pruned_height).unwrap_or(0),
            "timestamp": metadata.map(|m| m.timestamp).unwrap_or(0),
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
        }))
    }
}

/// Tool for getting sync information
#[derive(Clone)]
pub struct GetSyncInfoTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetSyncInfoTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
        let request = Request::new(Empty {});

        let response = self
            .grpc_client
            .clone()
            .get_sync_info(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get sync info: {e}")))?
            .into_inner();

        Ok(json!({
            "tip_height": response.tip_height,
            "local_height": response.local_height,
            "sync_progress": if response.tip_height > 0 {
                (response.local_height as f64 / response.tip_height as f64 * 100.0).round()
            } else {
                100.0
            },
            "blocks_behind": response.tip_height.saturating_sub(response.local_height),
            "peer_node_ids": response.peer_node_id.iter()
                .map(hex::encode)
                .collect::<Vec<_>>(),
            "is_synced": response.tip_height == response.local_height,
        }))
    }
}

/// Tool for getting network difficulty over time
#[derive(Clone)]
pub struct GetNetworkDifficultyTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNetworkDifficultyTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
                    "description": "Number of blocks from tip",
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
            "required": []
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let from_tip = params.get("from_tip").and_then(|v| v.as_u64()).unwrap_or(0);
        let start_height = params.get("start_height").and_then(|v| v.as_u64()).unwrap_or(0);
        let end_height = params.get("end_height").and_then(|v| v.as_u64()).unwrap_or(0);

        let request = Request::new(HeightRequest {
            from_tip,
            start_height,
            end_height,
        });

        let mut response_stream = self
            .grpc_client
            .clone()
            .get_network_difficulty(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get network difficulty: {e}")))?
            .into_inner();

        let mut difficulties = Vec::new();
        while let Some(difficulty_response) = response_stream
            .message()
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read difficulty stream: {e}")))?
        {
            difficulties.push(json!({
                "height": difficulty_response.height,
                "difficulty": difficulty_response.difficulty,
                "estimated_hash_rate": difficulty_response.estimated_hash_rate,
                "timestamp": difficulty_response.timestamp,
                "pow_algo": difficulty_response.pow_algo,
                "sha3x_estimated_hash_rate": difficulty_response.sha3x_estimated_hash_rate,
                "monero_randomx_estimated_hash_rate": difficulty_response.monero_randomx_estimated_hash_rate,
                "tari_randomx_estimated_hash_rate": difficulty_response.tari_randomx_estimated_hash_rate,
            }));
        }

        Ok(json!({
            "difficulties": difficulties,
            "count": difficulties.len(),
            "query": {
                "from_tip": from_tip,
                "start_height": start_height,
                "end_height": end_height,
            }
        }))
    }
}

/// Tool for getting tokens in circulation
#[derive(Clone)]
pub struct GetTokensInCirculationTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetTokensInCirculationTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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
        let heights: Vec<u64> = params
            .get("heights")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::invalid_request("heights array is required".to_string()))?
            .iter()
            .filter_map(|v| v.as_u64())
            .collect();

        if heights.is_empty() {
            return Err(McpError::invalid_request("At least one height is required".to_string()));
        }

        let request = Request::new(GetBlocksRequest {
            heights: heights.clone(),
        });

        let mut response_stream = self
            .grpc_client
            .clone()
            .get_tokens_in_circulation(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get tokens in circulation: {e}")))?
            .into_inner();

        let mut circulation_data = Vec::new();
        while let Some(value_response) = response_stream
            .message()
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to read circulation stream: {e}")))?
        {
            circulation_data.push(json!({
                "height": value_response.height,
                "mined_rewards": value_response.mined_rewards,
                "spendable_rewards": value_response.spendable_rewards,
                "spendable_pre_mine": value_response.spendable_pre_mine,
                "total_spendable": value_response.total_spendable,
            }));
        }

        Ok(json!({
            "circulation_data": circulation_data,
            "count": circulation_data.len(),
            "requested_heights": heights,
        }))
    }
}

/// Tool for getting network state
#[derive(Clone)]
pub struct GetNetworkStateTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNetworkStateTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
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

        let response = self
            .grpc_client
            .clone()
            .get_network_state(request)
            .await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get network state: {e}")))?
            .into_inner();

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
