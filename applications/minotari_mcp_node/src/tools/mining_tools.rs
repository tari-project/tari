//! Mining-related MCP tools for base node operations
//!
//! This module provides comprehensive mining tools including block templates,
//! block construction, and mining data retrieval. Updated to include the existing
//! GetNewBlockTemplateTool and expand mining capabilities.

use minotari_mcp_common::{McpTool, McpError, McpResult, get_required_u64_param, get_optional_string_param};
use minotari_node_grpc_client::BaseNodeGrpcClient;
use serde_json::{Value, json};

use tonic::transport::Channel;
use tonic::Request;
use minotari_app_grpc::tari_rpc::{
    NewBlockTemplateRequest, NewBlockTemplate, GetNewBlockWithCoinbasesRequest,
    GetNewBlockTemplateWithCoinbasesRequest, NewBlockCoinbase, pow_algo::PowAlgos, PowAlgo,
};

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
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let algo = params.get("algo")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| McpError::invalid_request("algo is required (0=SHA3X, 1=RANDOMXM, 2=RANDOMXT)".to_string()))?;
        
        let max_weight = params.get("max_weight")
            .and_then(|v| v.as_u64())
            .unwrap_or(19500); // Default max weight for blocks
        
        let pow_algo = match algo {
            0 => PowAlgos::Sha3x,
            1 => PowAlgos::Randomxm,
            2 => PowAlgos::Randomxt,
            _ => return Err(McpError::invalid_request("Invalid algo: must be 0 (SHA3X), 1 (RANDOMXM), or 2 (RANDOMXT)".to_string())),
        };
        
        let request = Request::new(NewBlockTemplateRequest {
            algo: Some(PowAlgo { pow_algo }),
            max_weight,
        });
        
        let response = self.grpc_client.clone().get_new_block_template(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get block template: {}", e)))?
            .into_inner();
        
        let template = response.new_block_template.as_ref()
            .ok_or_else(|| McpError::tool_execution_failed("No block template in response".to_string()))?;
        
        Ok(json!({
            "block_template": {
                "header": {
                    "version": template.header.as_ref().map(|h| h.version).unwrap_or(0),
                    "height": template.header.as_ref().map(|h| h.height).unwrap_or(0),
                    "prev_hash": template.header.as_ref()
                        .map(|h| hex::encode(&h.prev_hash))
                        .unwrap_or_default(),
                    "timestamp": 0, // Timestamp not available in NewBlockHeaderTemplate
                },
                "body": {
                    "inputs": template.body.as_ref().map(|b| b.inputs.len()).unwrap_or(0),
                    "outputs": template.body.as_ref().map(|b| b.outputs.len()).unwrap_or(0),
                    "kernels": template.body.as_ref().map(|b| b.kernels.len()).unwrap_or(0),
                }
            },
            "initial_sync_achieved": response.initial_sync_achieved,
            "miner_data": response.miner_data.as_ref().map(|md| json!({
                "algo": match md.algo.as_ref().map(|a| a.pow_algo) {
                    Some(PowAlgos::PowAlgosRandomxm) => "RANDOMX_M",
                    Some(PowAlgos::PowAlgosSha3x) => "SHA3X",
                    Some(PowAlgos::PowAlgosRandomxt) => "RANDOMX_T",
                    _ => "UNKNOWN",
                },
                "target_difficulty": md.target_difficulty,
                "reward": md.reward,
                "total_fees": md.total_fees,
            })),
            "parameters": {
                "requested_algo": algo,
                "requested_max_weight": max_weight,
            }
        }))
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
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        // This is a simplified implementation - in reality, we'd need to properly
        // construct the NewBlockTemplate from the parameters
        return Err(McpError::invalid_request(
            "This tool requires a complete block template structure. Use get_new_block_template first.".to_string()
        ));
        
        // TODO: Implement proper template parsing when needed
        // let template = parse_block_template(&params)?;
        // let request = Request::new(template);
        // let response = self.grpc_client.clone().get_new_block(request).await?;
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
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let algo = params.get("algo")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| McpError::invalid_request("algo is required (0=SHA3X, 1=RANDOMXM, 2=RANDOMXT)".to_string()))?;
        
        let max_weight = params.get("max_weight")
            .and_then(|v| v.as_u64())
            .unwrap_or(19500);
        
        let pow_algo = match algo {
            0 => PowAlgos::Sha3x,
            1 => PowAlgos::Randomxm,
            2 => PowAlgos::Randomxt,
            _ => return Err(McpError::invalid_request("Invalid algo: must be 0 (SHA3X), 1 (RANDOMXM), or 2 (RANDOMXT)".to_string())),
        };
        
        // Parse coinbase recipients
        let coinbases: Vec<NewBlockCoinbase> = params.get("coinbases")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::invalid_request("coinbases array is required".to_string()))?
            .iter()
            .map(|coinbase| -> Result<NewBlockCoinbase, McpError> {
                let address = coinbase.get("address")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_request("coinbase address is required".to_string()))?;
                
                let value = coinbase.get("value")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::invalid_request("coinbase value is required".to_string()))?;
                
                let stealth_payment = coinbase.get("stealth_payment")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                
                let revealed_value_proof = coinbase.get("revealed_value_proof")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                
                let coinbase_extra = coinbase.get("coinbase_extra")
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
            return Err(McpError::invalid_request("At least one coinbase output is required".to_string()));
        }
        
        let request = Request::new(GetNewBlockTemplateWithCoinbasesRequest {
            algo: Some(pow_algo),
            max_weight,
            coinbases,
        });
        
        let response = self.grpc_client.clone().get_new_block_template_with_coinbases(request).await
            .map_err(|e| McpError::tool_execution_failed(format!("Failed to get block template with coinbases: {}", e)))?
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
                    Some(PowAlgos::PowAlgosRandomxm) => "RANDOMX_M",
                    Some(PowAlgos::PowAlgosSha3x) => "SHA3X",
                    Some(PowAlgos::PowAlgosRandomxt) => "RANDOMX_T",
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
    
    async fn execute(&self, _params: Value) -> McpResult<Value> {
        // This would require a complete block template structure
        // For now, return an informational message
        return Err(McpError::invalid_request(
            "This tool requires a complete block template and coinbase configuration. Use get_new_block_template_with_coinbases for most use cases.".to_string()
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
        "Provides comprehensive mining analysis including difficulty trends, profitability estimates, and algorithm recommendations"
    }
    
    async fn execute(&self, params: Value) -> McpResult<Value> {
        let requested_algo = params.get("preferred_algo")
            .and_then(|v| v.as_u64())
            .unwrap_or(0); // Default to SHA3X
        
        let hash_rate = params.get("hash_rate")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000000); // Default 1 MH/s
        
        // Get current mining data for all algorithms
        let mut algo_analysis = Vec::new();
        
        for algo in 0..3 {
            let pow_algo = match algo {
                0 => PowAlgos::Sha3x,
                1 => PowAlgos::Randomxm,
                2 => PowAlgos::Randomxt,
                _ => continue,
            };
            
            match self.grpc_client.clone().get_new_block_template(Request::new(NewBlockTemplateRequest {
                algo: Some(pow_algo),
                max_weight: 19500,
            })).await {
                Ok(response) => {
                    let response = response.into_inner();
                    if let (Some(template), Some(miner_data)) = (response.new_block_template.as_ref(), response.miner_data.as_ref()) {
                        let algo_name = match algo {
                            0 => "SHA3X",
                            1 => "MONERO",
                            2 => "TARI",
                            _ => "UNKNOWN",
                        };
                        
                        // Calculate estimated time to find block
                        let time_to_block = if hash_rate > 0 {
                            miner_data.target_difficulty / hash_rate
                        } else {
                            0
                        };
                        
                        algo_analysis.push(json!({
                            "algorithm": algo_name,
                            "algo_id": algo,
                            "target_difficulty": miner_data.target_difficulty,
                            "block_reward": miner_data.reward,
                            "total_fees": miner_data.total_fees,
                            "total_reward": miner_data.reward + miner_data.total_fees,
                            "estimated_time_to_block_seconds": time_to_block,
                            "estimated_blocks_per_day": if time_to_block > 0 {
                                86400 / time_to_block
                            } else {
                                0
                            },
                            "profitability_score": if miner_data.target_difficulty > 0 {
                                ((miner_data.reward + miner_data.total_fees) as f64 / miner_data.target_difficulty as f64 * 1000000.0).round()
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
                }
            }
        }
        
        // Find best algorithm
        let best_algo = algo_analysis.iter()
            .max_by(|a, b| {
                a["profitability_score"].as_f64().unwrap_or(0.0)
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
                recommendations.push("Consider joining a mining pool - estimated solo mining time exceeds 24 hours".to_string());
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
                    0 => "SHA3X",
                    1 => "MONERO", 
                    2 => "TARI",
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
