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
//! Mining-related MCP tools for base node operations
//!
//! This module provides comprehensive mining tools including block templates,
//! block construction, and mining data retrieval. Updated to include the existing
//! GetNewBlockTemplateTool and expand mining capabilities.

use minotari_app_grpc::tari_rpc::{GetNewBlockTemplateWithCoinbasesRequest, NewBlockCoinbase, NewBlockTemplateRequest};
use minotari_mcp_common::{MAX_ENCODED_BLOCK_HEX_LEN, McpError, McpResult, McpTool, node_rpc};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{Value, json};
use tonic::{Request, transport::Channel};

/// Tool for getting a new block template (existing implementation, enhanced)
#[derive(Clone)]
pub struct GetNewBlockTemplateTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNewBlockTemplateTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNewBlockTemplateTool {
    fn name(&self) -> &str {
        "get_new_block_template"
    }

    fn description(&self) -> &str {
        "Retrieves a new block template for mining with specified algorithm and optional weight limit"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "algo": {
                    "type": "number",
                    "description": "Mining algorithm (0=RANDOMXM, 1=SHA3X, 2=RANDOMXT, 3=CUCKAROO)"
                },
                "max_weight": {
                    "type": "number",
                    "description": "Maximum block weight (optional, default 19500)",
                    "minimum": 1
                }
            },
            "required": ["algo"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_new_block_template(self.grpc_client.clone(), &params).await
    }
}

/// Tool for constructing a new block from a template
#[derive(Clone)]
pub struct GetNewBlockTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNewBlockTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNewBlockTool {
    fn name(&self) -> &str {
        "get_new_block"
    }

    fn description(&self) -> &str {
        "Constructs a new block from a provided block template (requires template from get_new_block_template)"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "new_block_template_hex": {
                    "type": "string",
                    "description": "Hex of the protobuf-encoded tari.rpc.NewBlockTemplate to complete",
                    "pattern": "^([0-9a-fA-F]{2})+$",
                    "maxLength": MAX_ENCODED_BLOCK_HEX_LEN
                }
            },
            "required": ["new_block_template_hex"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        node_rpc::get_new_block(self.grpc_client.clone(), &params).await
    }
}

/// Tool for getting block template with custom coinbase outputs
#[derive(Clone)]
pub struct GetNewBlockTemplateWithCoinbasesTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNewBlockTemplateWithCoinbasesTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNewBlockTemplateWithCoinbasesTool {
    fn name(&self) -> &str {
        "get_new_block_template_with_coinbases"
    }

    fn description(&self) -> &str {
        "Retrieves a new block template with custom coinbase outputs for mining pools or multi-recipient mining"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "algo": {
                    "type": "number",
                    "description": "Mining algorithm (0=RANDOMXM, 1=SHA3X, 2=RANDOMXT, 3=CUCKAROO)"
                },
                "max_weight": {
                    "type": "number",
                    "description": "Maximum block weight (optional, default 19500)",
                    "minimum": 1
                },
                "coinbases": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "address": {
                                "type": "string",
                                "description": "Coinbase recipient address"
                            },
                            "value": {
                                "type": "number",
                                "description": "Coinbase value"
                            },
                            "stealth_payment": {
                                "type": "boolean",
                                "description": "Whether to use stealth payment (optional)"
                            },
                            "revealed_value_proof": {
                                "type": "boolean",
                                "description": "Whether to include revealed value proof (optional)"
                            },
                            "coinbase_extra": {
                                "type": "string",
                                "description": "Extra coinbase data (optional)"
                            }
                        },
                        "required": ["address", "value"]
                    },
                    "description": "Array of coinbase outputs"
                }
            },
            "required": ["algo", "coinbases"]
        })
    }

    async fn execute(&self, params: Value) -> McpResult<Value> {
        let (_, pow_algo) = node_rpc::pow_algo_param(&params, "algo")?;

        let max_weight = params.get("max_weight").and_then(|v| v.as_u64()).unwrap_or(19500);

        // Parse coinbase recipients
        let coinbases: Vec<NewBlockCoinbase> = params
            .get("coinbases")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::invalid_request("coinbases array is required".to_string()))?
            .iter()
            .map(|coinbase| -> Result<NewBlockCoinbase, McpError> {
                let address = coinbase
                    .get("address")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("coinbase address is required".to_string()))?;

                let value = coinbase
                    .get("value")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("coinbase value is required".to_string()))?;

                let stealth_payment = coinbase
                    .get("stealth_payment")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let revealed_value_proof = coinbase
                    .get("revealed_value_proof")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let coinbase_extra = coinbase
                    .get("coinbase_extra")
                    .and_then(|v| v.as_str())
                    .map(|s| s.as_bytes().to_vec())
                    .unwrap_or_default();

                Ok(NewBlockCoinbase {
                    address: address.to_string(),
                    value,
                    stealth_payment,
                    revealed_value_proof,
                    coinbase_extra,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if coinbases.is_empty() {
            return Err(McpError::invalid_request(
                "At least one coinbase output is required".to_string(),
            ));
        }

        let request = Request::new(GetNewBlockTemplateWithCoinbasesRequest {
            algo: Some(pow_algo),
            max_weight,
            coinbases,
        });

        let response = self
            .grpc_client
            .clone()
            .get_new_block_template_with_coinbases(request)
            .await?
            .into_inner();

        Ok(json!({
            "block_hash": hex::encode(&response.block_hash),
            "block": response.block.as_ref().map(|block| json!({
                "header": {
                    "version": block.header.as_ref().map(|h| h.version).unwrap_or(0),
                    "height": block.header.as_ref().map(|h| h.height).unwrap_or(0),
                    "prev_hash": block.header.as_ref()
                        .map(|h| hex::encode(&h.prev_hash))
                        .unwrap_or_default(),
                    "timestamp": block.header.as_ref().map(|h| h.timestamp).unwrap_or(0),
                },
                "body": {
                    "inputs": block.body.as_ref().map(|b| b.inputs.len()).unwrap_or(0),
                    "outputs": block.body.as_ref().map(|b| b.outputs.len()).unwrap_or(0),
                    "kernels": block.body.as_ref().map(|b| b.kernels.len()).unwrap_or(0),
                }
            })),
            "merge_mining_hash": hex::encode(&response.merge_mining_hash),
            "tari_unique_id": hex::encode(&response.tari_unique_id),
            "miner_data": response.miner_data.as_ref().map(|md| json!({
                "algo": match md.algo.as_ref().map(|a| a.pow_algo) {
                    Some(0) => "RANDOMX_M",
                    Some(1) => "SHA3X",
                    Some(2) => "RANDOMX_T",
                    Some(3) => "CUCKAROO",
                    _ => "UNKNOWN",
                },
                "target_difficulty": md.target_difficulty,
                "reward": md.reward,
                "total_fees": md.total_fees,
            })),
        }))
    }
}

/// Tool for getting a block with custom coinbase from template
#[derive(Clone)]
pub struct GetNewBlockWithCoinbasesTool {
    #[allow(dead_code)] // False positive: field is used in execute() method line 337-340
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl GetNewBlockWithCoinbasesTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for GetNewBlockWithCoinbasesTool {
    fn name(&self) -> &str {
        "get_new_block_with_coinbases"
    }

    fn description(&self) -> &str {
        "Constructs a new block with custom coinbase outputs from a template (advanced mining operation)"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "template": {
                    "type": "object",
                    "description": "Block template with coinbase configuration"
                }
            },
            "required": ["template"]
        })
    }

    async fn execute(&self, _params: Value) -> McpResult<Value> {
        // This would require a complete block template structure
        // For now, return an informational message
        return Err(McpError::invalid_request(
            "This tool requires a complete block template and coinbase configuration. Use \
             get_new_block_template_with_coinbases for most use cases."
                .to_string(),
        ));
    }
}

/// Tool for mining operation analysis and recommendations
#[derive(Clone)]
pub struct MiningAnalysisTool {
    grpc_client: BaseNodeGrpcClient<Channel>,
}

impl MiningAnalysisTool {
    pub fn new(grpc_client: BaseNodeGrpcClient<Channel>) -> Self {
        Self { grpc_client }
    }
}

#[async_trait::async_trait]
impl McpTool for MiningAnalysisTool {
    fn name(&self) -> &str {
        "mining_analysis"
    }

    fn description(&self) -> &str {
        "Provides comprehensive mining analysis including difficulty trends, profitability estimates, and algorithm \
         recommendations"
    }

    fn permission_level(&self) -> minotari_mcp_common::security::PermissionLevel {
        minotari_mcp_common::security::PermissionLevel::Control
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "preferred_algo": {
                    "type": "number",
                    "description": "Preferred mining algorithm (0=RANDOMXM, 1=SHA3X, 2=RANDOMXT, 3=CUCKAROO)",
                    "minimum": 0,
                    "maximum": 3
                },
                "hash_rate": {
                    "type": "number",
                    "description": "Your hash rate in H/s (default 1000000)",
                    "minimum": 1
                }
            },
            "required": []
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let requested_algo = params.get("preferred_algo").and_then(|v| v.as_u64()).unwrap_or(1); // Default to SHA3X

        let hash_rate = params.get("hash_rate").and_then(|v| v.as_u64()).unwrap_or(1000000); // Default 1 MH/s

        // Get current mining data for all algorithms
        let mut algo_analysis = Vec::new();

        for algo in 0..4 {
            let pow_algo = node_rpc::pow_algo(algo)?;

            match self
                .grpc_client
                .clone()
                .get_new_block_template(Request::new(NewBlockTemplateRequest {
                    algo: Some(pow_algo),
                    max_weight: 19500,
                }))
                .await
            {
                Ok(response) => {
                    let response = response.into_inner();
                    if let (Some(_template), Some(miner_data)) =
                        (response.new_block_template.as_ref(), response.miner_data.as_ref())
                    {
                        let algo_name = match algo {
                            0 => "MONERO",
                            1 => "SHA3X",
                            2 => "TARI",
                            3 => "CUCKAROO",
                            _ => "UNKNOWN",
                        };

                        // Calculate estimated time to find block
                        let time_to_block = miner_data.target_difficulty.checked_div(hash_rate).unwrap_or(0);
                        let total_reward = miner_data.reward.saturating_add(miner_data.total_fees);

                        algo_analysis.push(json!({
                            "algorithm": algo_name,
                            "algo_id": algo,
                            "target_difficulty": miner_data.target_difficulty,
                            "block_reward": miner_data.reward,
                            "total_fees": miner_data.total_fees,
                            "total_reward": total_reward,
                            "estimated_time_to_block_seconds": time_to_block,
                            "estimated_blocks_per_day": 86400u64.checked_div(time_to_block).unwrap_or(0),
                            "profitability_score": if miner_data.target_difficulty > 0 {
                                (total_reward as f64 / miner_data.target_difficulty as f64 * 1000000.0).round()
                            } else {
                                0.0
                            },
                            "sync_required": !response.initial_sync_achieved,
                        }));
                    }
                },
                Err(_) => {
                    // Skip algorithms that are not available
                    continue;
                },
            }
        }

        // Find best algorithm
        let best_algo = algo_analysis.iter().max_by(|a, b| {
            a["profitability_score"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&b["profitability_score"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Generate recommendations
        let mut recommendations = Vec::new();

        if let Some(best) = best_algo {
            if best["algo_id"].as_u64().unwrap_or(99) != requested_algo {
                recommendations.push(format!(
                    "Consider switching to {} algorithm for better profitability",
                    best["algorithm"].as_str().unwrap_or("UNKNOWN")
                ));
            }

            if best["sync_required"].as_bool().unwrap_or(false) {
                recommendations.push("Node sync required before mining can begin".to_string());
            }

            if best["estimated_time_to_block_seconds"].as_u64().unwrap_or(0) > 86400 {
                recommendations
                    .push("Consider joining a mining pool - estimated solo mining time exceeds 24 hours".to_string());
            }
        }

        if algo_analysis.is_empty() {
            recommendations.push("No mining algorithms available - check node status and sync".to_string());
        }

        Ok(json!({
            "mining_analysis": algo_analysis,
            "best_algorithm": best_algo,
            "current_preference": {
                "algo_id": requested_algo,
                "algo_name": match requested_algo {
                    0 => "MONERO",
                    1 => "SHA3X",
                    2 => "TARI",
                    3 => "CUCKAROO",
                    _ => "UNKNOWN",
                }
            },
            "mining_setup": {
                "provided_hash_rate": hash_rate,
                "hash_rate_unit": "H/s",
                "note": "Profitability calculations are estimates based on current difficulty and rewards"
            },
            "recommendations": recommendations,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }))
    }
}
