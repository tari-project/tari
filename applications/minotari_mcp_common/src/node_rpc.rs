// Copyright 2025, The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Base node gRPC calls shared by the manual node MCP tools and the auto-discovered tools.
//!
//! Each function builds the request from the tool's JSON arguments, calls the node and shapes the response.
//! Server-streaming RPCs are never collected unbounded.

use minotari_node_grpc_client::{
    BaseNodeGrpcClient,
    grpc::{
        Block,
        Empty,
        FetchMatchingUtxosRequest,
        GetBlocksRequest,
        GetHeaderByHashRequest,
        GetMempoolTransactionsRequest,
        GetPeersRequest,
        HeightRequest,
        ListHeadersRequest,
        NewBlockTemplate,
        NewBlockTemplateRequest,
        PowAlgo,
        SearchKernelsRequest,
        SearchUtxosRequest,
        Signature,
        SubmitTransactionRequest,
        SubmitTransactionResult,
        Transaction,
        pow_algo::PowAlgos,
    },
};
use prost::Message;
use serde_json::{Value, json};
use tonic::{codec::Streaming, transport::Channel};

use crate::{McpError, McpResult, clean_upstream_text, get_required_string_param};

/// Cap on items collected from any server-streaming RPC, and on request lists that drive one
pub const MAX_STREAM_ITEMS: usize = 512;
/// Default and maximum `limit` for `GetMempoolTransactions`
pub const DEFAULT_MEMPOOL_TX_LIMIT: u64 = 100;
pub const MAX_MEMPOOL_TX_LIMIT: u64 = 1000;

type Client = BaseNodeGrpcClient<Channel>;

/// Read at most `cap` messages from a server stream
pub(crate) async fn collect<T>(mut stream: Streaming<T>, cap: usize) -> McpResult<Vec<T>> {
    let mut items = Vec::new();
    while items.len() < cap &&
        let Some(item) = stream.message().await?
    {
        items.push(item);
    }
    Ok(items)
}

/// Read up to `limit` messages, and whether the stream had more
pub async fn collect_limited<T>(stream: Streaming<T>, limit: u64) -> McpResult<(Vec<T>, bool)> {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let items = collect(stream, limit.saturating_add(1)).await?;
    Ok(split_limited(items, limit))
}

/// Given up to `limit + 1` items, keep `limit` and report whether there was an extra one
fn split_limited<T>(mut items: Vec<T>, limit: usize) -> (Vec<T>, bool) {
    let truncated = items.len() > limit;
    items.truncate(limit);
    (items, truncated)
}

/// The `limit` argument, defaulting to `default` and clamped to `1..=max`
pub fn limit_param(params: &Value, default: u64, max: u64) -> u64 {
    params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(default)
        .clamp(1, max)
}

/// Hex of a protobuf-encoded message, the form [decode_hex_message] reads back
pub fn encode_hex_message<M: Message>(message: &M) -> String {
    hex::encode(message.encode_to_vec())
}

/// Decode a hex-encoded protobuf message from the `key` string argument
pub fn decode_hex_message<M: prost::Message + Default>(params: &Value, key: &str) -> McpResult<M> {
    let hex_str = get_required_string_param(params, key)?;
    let bytes = hex::decode(&hex_str).map_err(|e| McpError::invalid_request(format!("Invalid hex in {key}: {e}")))?;
    M::decode(bytes.as_slice()).map_err(|e| McpError::invalid_request(format!("Invalid protobuf in {key}: {e}")))
}

/// Read a required, non-empty, capped `heights` array
fn heights(params: &Value) -> McpResult<Vec<u64>> {
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
    check_list_len("heights", heights.len())?;
    Ok(heights)
}

/// Read a required, capped array of hex strings
fn hex_list(params: &Value, key: &str) -> McpResult<Vec<Vec<u8>>> {
    let items = params
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| McpError::invalid_request(format!("{key} array is required")))?;
    check_list_len(key, items.len())?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .and_then(|s| hex::decode(s).ok())
                .ok_or_else(|| McpError::invalid_request(format!("{key} entries must be hex strings")))
        })
        .collect()
}

fn check_list_len(key: &str, len: usize) -> McpResult<()> {
    if len > MAX_STREAM_ITEMS {
        return Err(McpError::invalid_request(format!(
            "{key} has {len} entries, at most {MAX_STREAM_ITEMS} are allowed"
        )));
    }
    Ok(())
}

/// The node streams a height range oldest first, so a range longer than the cap would silently lose the newest
/// heights. The node uses `start_height..=end_height` when `end_height` is set, else the `from_tip + 1` heights ending
/// at the tip.
fn check_height_span(from_tip: u64, start_height: u64, end_height: u64) -> McpResult<()> {
    let span = if end_height > 0 {
        end_height.saturating_sub(start_height).saturating_add(1)
    } else {
        from_tip.saturating_add(1)
    };
    if span > MAX_STREAM_ITEMS as u64 {
        return Err(McpError::invalid_request(format!(
            "Requested {span} heights, at most {MAX_STREAM_ITEMS} are allowed"
        )));
    }
    Ok(())
}

fn block_summary(block: &Block) -> Value {
    json!({
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
    })
}

fn peer_summary(peer: &minotari_node_grpc_client::grpc::Peer) -> Value {
    json!({
        "public_key": hex::encode(&peer.public_key),
        "node_id": hex::encode(&peer.node_id),
        "addresses": peer.addresses.iter().map(|addr| {
            clean_upstream_text(&String::from_utf8_lossy(&addr.address))
        }).collect::<Vec<_>>(),
        "last_connection": peer.last_connection,
        "flags": peer.flags,
        "banned_until": peer.banned_until,
        "banned_reason": clean_upstream_text(&peer.banned_reason),
        "offline_at": peer.offline_at,
        "features": peer.features,
        "supported_protocols": peer.supported_protocols.iter()
            .map(|proto| clean_upstream_text(&String::from_utf8_lossy(proto)))
            .collect::<Vec<_>>(),
        "user_agent": clean_upstream_text(&peer.user_agent),
    })
}

#[allow(clippy::cast_possible_truncation)]
pub async fn list_headers(mut client: Client, params: &Value) -> McpResult<Value> {
    let from_height = params.get("from_height").and_then(|v| v.as_u64()).unwrap_or(0);
    let num_headers = params
        .get("num_headers")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .min(MAX_STREAM_ITEMS as u64);
    let sorting = params.get("sorting").and_then(|v| v.as_i64()).unwrap_or(0) as i32; // Default to SORTING_DESC

    let request = ListHeadersRequest {
        from_height,
        num_headers,
        sorting,
    };
    let stream = client.list_headers(request).await?.into_inner();

    let headers: Vec<Value> = collect(stream, MAX_STREAM_ITEMS)
        .await?
        .iter()
        .map(|header_response| {
            json!({
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
            })
        })
        .collect();

    Ok(json!({
        "headers": headers,
        "count": headers.len(),
        "from_height": from_height,
        "num_headers": num_headers,
        "sorting": if sorting == 0 { "desc" } else { "asc" }
    }))
}

pub async fn get_header_by_hash(mut client: Client, params: &Value) -> McpResult<Value> {
    let hash_hex = get_required_string_param(params, "hash")?;
    let hash = hex::decode(&hash_hex).map_err(|e| McpError::invalid_request(format!("Invalid hex hash: {e}")))?;

    let response = client
        .get_header_by_hash(GetHeaderByHashRequest { hash })
        .await?
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

pub async fn get_blocks(mut client: Client, params: &Value) -> McpResult<Value> {
    let heights = heights(params)?;
    let stream = client
        .get_blocks(GetBlocksRequest {
            heights: heights.clone(),
        })
        .await?
        .into_inner();

    let blocks: Vec<Value> = collect(stream, MAX_STREAM_ITEMS)
        .await?
        .iter()
        .filter_map(|historical_block| historical_block.block.as_ref().map(block_summary))
        .collect();

    Ok(json!({
        "blocks": blocks,
        "count": blocks.len(),
        "requested_heights": heights,
    }))
}

pub async fn get_tip_info(mut client: Client) -> McpResult<Value> {
    let response = client.get_tip_info(Empty {}).await?.into_inner();
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

pub async fn get_sync_info(mut client: Client) -> McpResult<Value> {
    let response = client.get_sync_info(Empty {}).await?.into_inner();

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

pub async fn get_network_difficulty(mut client: Client, params: &Value) -> McpResult<Value> {
    let from_tip = params.get("from_tip").and_then(|v| v.as_u64()).unwrap_or(0);
    let start_height = params.get("start_height").and_then(|v| v.as_u64()).unwrap_or(0);
    let end_height = params.get("end_height").and_then(|v| v.as_u64()).unwrap_or(0);
    check_height_span(from_tip, start_height, end_height)?;

    let request = HeightRequest {
        from_tip,
        start_height,
        end_height,
    };
    let stream = client.get_network_difficulty(request).await?.into_inner();

    let difficulties: Vec<Value> = collect(stream, MAX_STREAM_ITEMS)
        .await?
        .iter()
        .map(|difficulty_response| {
            json!({
                "height": difficulty_response.height,
                "difficulty": difficulty_response.difficulty,
                "adjusted_difficulty": difficulty_response.adjusted_difficulty,
                "estimated_hash_rate": difficulty_response.estimated_hash_rate,
                "timestamp": difficulty_response.timestamp,
                "pow_algo": difficulty_response.pow_algo,
                "sha3x_estimated_hash_rate": difficulty_response.sha3x_estimated_hash_rate,
                "monero_randomx_estimated_hash_rate": difficulty_response.monero_randomx_estimated_hash_rate,
                "tari_randomx_estimated_hash_rate": difficulty_response.tari_randomx_estimated_hash_rate,
            })
        })
        .collect();

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

pub async fn get_tokens_in_circulation(mut client: Client, params: &Value) -> McpResult<Value> {
    let heights = heights(params)?;
    let stream = client
        .get_tokens_in_circulation(GetBlocksRequest {
            heights: heights.clone(),
        })
        .await?
        .into_inner();

    let circulation_data: Vec<Value> = collect(stream, MAX_STREAM_ITEMS)
        .await?
        .iter()
        .map(|value_response| {
            json!({
                "height": value_response.height,
                "mined_rewards": value_response.mined_rewards,
                "spendable_rewards": value_response.spendable_rewards,
                "spendable_pre_mine": value_response.spendable_pre_mine,
                "total_spendable": value_response.total_spendable,
            })
        })
        .collect();

    Ok(json!({
        "circulation_data": circulation_data,
        "count": circulation_data.len(),
        "requested_heights": heights,
    }))
}

pub async fn get_mempool_stats(mut client: Client) -> McpResult<Value> {
    let response = client.get_mempool_stats(Empty {}).await?.into_inner();

    Ok(json!({
        "unconfirmed_txs": response.unconfirmed_txs,
        "reorg_txs": response.reorg_txs,
        "unconfirmed_weight": response.unconfirmed_weight,
        "total_transactions": response.unconfirmed_txs.saturating_add(response.reorg_txs),
    }))
}

pub async fn get_mempool_transactions(mut client: Client, params: &Value) -> McpResult<Value> {
    let limit = limit_param(params, DEFAULT_MEMPOOL_TX_LIMIT, MAX_MEMPOOL_TX_LIMIT);
    let stream = client
        .get_mempool_transactions(GetMempoolTransactionsRequest {})
        .await?
        .into_inner();
    let (responses, truncated) = collect_limited(stream, limit).await?;

    let transactions: Vec<Value> = responses
        .iter()
        .filter_map(|tx_response| tx_response.transaction.as_ref())
        .map(|transaction| {
            json!({
                "offset": hex::encode(&transaction.offset),
                "body": {
                    "inputs": transaction.body.as_ref()
                        .map(|body| body.inputs.len())
                        .unwrap_or(0),
                    "outputs": transaction.body.as_ref()
                        .map(|body| body.outputs.len())
                        .unwrap_or(0),
                    "kernels": transaction.body.as_ref()
                        .map(|body| body.kernels.len())
                        .unwrap_or(0),
                },
                "total_fee": transaction.body.as_ref()
                    .map(|body| body.kernels.iter()
                        .map(|k| k.fee)
                        .sum::<u64>())
                    .unwrap_or(0),
                "total_weight": transaction.body.as_ref()
                    .map(|body| {
                        body.inputs.len().saturating_add(body.outputs.len()).saturating_add(body.kernels.len())
                    })
                    .unwrap_or(0),
            })
        })
        .collect();

    Ok(json!({
        "transactions": transactions,
        "count": transactions.len(),
        "limit": limit,
        "note": if truncated {
            format!("Results limited to {limit} transactions. Use limit parameter to adjust.")
        } else {
            "All mempool transactions returned".to_string()
        }
    }))
}

pub async fn get_network_status(mut client: Client) -> McpResult<Value> {
    let response = client.get_network_status(Empty {}).await?.into_inner();

    let status = if response.num_node_connections >= 8 {
        "EXCELLENT"
    } else if response.num_node_connections >= 4 {
        "GOOD"
    } else if response.num_node_connections >= 1 {
        "LIMITED"
    } else {
        "DISCONNECTED"
    };

    Ok(json!({
        "status": status,
        "avg_latency_ms": response.avg_latency_ms,
        "num_node_connections": response.num_node_connections,
        "connection_status": {
            "excellent": response.num_node_connections >= 8,
            "good": response.num_node_connections >= 4,
            "limited": response.num_node_connections >= 1,
            "disconnected": response.num_node_connections == 0,
        },
        "performance": {
            "latency_status": if response.avg_latency_ms <= 100 {
                "EXCELLENT"
            } else if response.avg_latency_ms <= 300 {
                "GOOD"
            } else if response.avg_latency_ms <= 1000 {
                "FAIR"
            } else {
                "POOR"
            },
            "recommended_min_connections": 4,
            "optimal_connections": 8,
        }
    }))
}

#[allow(clippy::indexing_slicing)]
pub async fn list_connected_peers(mut client: Client) -> McpResult<Value> {
    let response = client.list_connected_peers(Empty {}).await?.into_inner();

    let connected_peers: Vec<Value> = response
        .connected_peers
        .iter()
        .map(|peer| {
            let mut info = peer_summary(peer);
            info["connection_status"] = json!({
                "is_banned": peer.banned_until > 0,
                "is_online": peer.offline_at == 0,
                "connection_age_seconds": if peer.last_connection > 0 {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .saturating_sub(peer.last_connection)
                } else {
                    0
                }
            });
            info
        })
        .collect();

    let peer_count = connected_peers.len();
    let banned_count = connected_peers
        .iter()
        .filter(|peer| peer["connection_status"]["is_banned"].as_bool().unwrap_or(false))
        .count();
    let online_count = connected_peers
        .iter()
        .filter(|peer| peer["connection_status"]["is_online"].as_bool().unwrap_or(false))
        .count();

    Ok(json!({
        "connected_peers": connected_peers,
        "summary": {
            "total_peers": peer_count,
            "online_peers": online_count,
            "banned_peers": banned_count,
            "healthy_peers": online_count.saturating_sub(banned_count),
        },
        "network_health": {
            "status": if online_count >= 8 {
                "EXCELLENT"
            } else if online_count >= 4 {
                "GOOD"
            } else if online_count >= 1 {
                "LIMITED"
            } else {
                "DISCONNECTED"
            },
            "diversity_score": if peer_count > 0 {
                let unique_user_agents: std::collections::HashSet<String> = connected_peers.iter()
                    .filter_map(|peer| peer["user_agent"].as_str())
                    .map(|ua| ua.to_string())
                    .collect();
                (unique_user_agents.len() as f64 / peer_count as f64 * 100.0).round()
            } else {
                0.0
            }
        }
    }))
}

#[allow(clippy::indexing_slicing)]
pub async fn get_peers(mut client: Client, params: &Value) -> McpResult<Value> {
    let limit = limit_param(params, 50, MAX_STREAM_ITEMS as u64);
    let stream = client.get_peers(GetPeersRequest {}).await?.into_inner();
    let (responses, truncated) = collect_limited(stream, limit).await?;

    let all_peers: Vec<Value> = responses
        .iter()
        .filter_map(|peer_response| peer_response.peer.as_ref())
        .map(|peer| {
            let mut info = peer_summary(peer);
            info["status"] = json!({
                "is_connected": peer.offline_at == 0,
                "is_banned": peer.banned_until > 0,
                "last_seen": if peer.last_connection > 0 {
                    peer.last_connection
                } else {
                    peer.offline_at
                },
                "connection_attempts": peer.flags, // Assuming flags indicate connection attempts
            });
            info
        })
        .collect();

    let connected_count = all_peers
        .iter()
        .filter(|peer| peer["status"]["is_connected"].as_bool().unwrap_or(false))
        .count();
    let banned_count = all_peers
        .iter()
        .filter(|peer| peer["status"]["is_banned"].as_bool().unwrap_or(false))
        .count();
    let total_count = all_peers.len();

    Ok(json!({
        "peers": all_peers,
        "summary": {
            "total_known_peers": total_count,
            "connected_peers": connected_count,
            "disconnected_peers": total_count.saturating_sub(connected_count),
            "banned_peers": banned_count,
            "healthy_peers": connected_count.saturating_sub(banned_count),
            "connection_rate": if total_count > 0 {
                (connected_count as f64 / total_count as f64 * 100.0).round()
            } else {
                0.0
            }
        },
        "metadata": {
            "limit_applied": limit,
            "results_truncated": truncated,
            "note": if truncated {
                format!("Results limited to {limit} peers. Use limit parameter to adjust.")
            } else {
                "All known peers returned".to_string()
            }
        }
    }))
}

/// Mining algorithm numbering shared by every tool, matching `tari.rpc.PowAlgo.PowAlgos`
pub const POW_ALGO_HELP: &str = "0=RANDOMXM, 1=SHA3X, 2=RANDOMXT";

/// The `key` argument as a mining algorithm, numbered as in [POW_ALGO_HELP]
pub fn pow_algo_param(params: &Value, key: &str) -> McpResult<(u64, PowAlgo)> {
    let algo = params
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| McpError::invalid_request(format!("{key} is required ({POW_ALGO_HELP})")))?;
    Ok((algo, pow_algo(algo)?))
}

/// A mining algorithm numbered as in [POW_ALGO_HELP]
pub fn pow_algo(algo: u64) -> McpResult<PowAlgo> {
    let pow_algo = match algo {
        0 => PowAlgos::Randomxm,
        1 => PowAlgos::Sha3x,
        2 => PowAlgos::Randomxt,
        _ => {
            return Err(McpError::invalid_request(format!(
                "Invalid algo {algo}: must be one of {POW_ALGO_HELP}"
            )));
        },
    };
    Ok(PowAlgo {
        pow_algo: pow_algo.into(),
    })
}

pub async fn get_new_block_template(mut client: Client, params: &Value) -> McpResult<Value> {
    let (algo, pow_algo) = pow_algo_param(params, "algo")?;
    let max_weight = params.get("max_weight").and_then(|v| v.as_u64()).unwrap_or(19500); // Default max weight for blocks

    let request = NewBlockTemplateRequest {
        algo: Some(pow_algo),
        max_weight,
    };
    let response = client.get_new_block_template(request).await?.into_inner();

    let template = response
        .new_block_template
        .as_ref()
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
        "new_block_template_hex": encode_hex_message(template),
        "initial_sync_achieved": response.initial_sync_achieved,
        "miner_data": response.miner_data.as_ref().map(|md| json!({
            "algo": match md.algo.as_ref().map(|a| a.pow_algo) {
                Some(0) => "RANDOMX_M",
                Some(1) => "SHA3X",
                Some(2) => "RANDOMX_T",
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

/// Complete `new_block_template_hex` (the hex of a protobuf-encoded `tari.rpc.NewBlockTemplate`) into a block.
/// The block is returned as `block_hex`, ready for `submit_block`.
pub async fn get_new_block(mut client: Client, params: &Value) -> McpResult<Value> {
    let template: NewBlockTemplate = decode_hex_message(params, "new_block_template_hex")?;
    let response = client.get_new_block(template).await?.into_inner();

    Ok(json!({
        "block_hash": hex::encode(&response.block_hash),
        "block": response.block.as_ref().map(block_summary),
        "block_hex": response.block.as_ref().map(encode_hex_message),
        "merge_mining_hash": hex::encode(&response.merge_mining_hash),
        "tari_unique_id": hex::encode(&response.tari_unique_id),
        "vm_key": hex::encode(&response.vm_key),
        "miner_data": response.miner_data.as_ref().map(|md| json!({
            "algo": md.algo.as_ref().map(|a| a.pow_algo),
            "target_difficulty": md.target_difficulty,
            "reward": md.reward,
            "total_fees": md.total_fees,
        })),
    }))
}

/// Submit `block_hex`: the hex of a protobuf-encoded `tari.rpc.Block`
pub async fn submit_block(mut client: Client, params: &Value) -> McpResult<Value> {
    let block: Block = decode_hex_message(params, "block_hex")?;
    let response = client.submit_block(block).await?.into_inner();

    Ok(json!({
        "success": true,
        "block_hash": hex::encode(&response.block_hash),
    }))
}

/// Submit `transaction_hex`: the hex of a protobuf-encoded `tari.rpc.Transaction`
pub async fn submit_transaction(mut client: Client, params: &Value) -> McpResult<Value> {
    let transaction: Transaction = decode_hex_message(params, "transaction_hex")?;
    let response = client
        .submit_transaction(SubmitTransactionRequest {
            transaction: Some(transaction),
        })
        .await?
        .into_inner();

    let result = response.result();
    Ok(json!({
        "success": result == SubmitTransactionResult::Accepted,
        "result": result.as_str_name(),
    }))
}

pub async fn get_version(mut client: Client) -> McpResult<Value> {
    let response = client.get_version(Empty {}).await?.into_inner();
    Ok(json!({
        "version": response.version,
        "network": response.network,
    }))
}

pub async fn search_kernels(mut client: Client, params: &Value) -> McpResult<Value> {
    let items = params
        .get("signatures")
        .and_then(|v| v.as_array())
        .ok_or_else(|| McpError::invalid_request("signatures array is required"))?;
    check_list_len("signatures", items.len())?;
    let signatures = items
        .iter()
        .map(|sig| {
            let field = |key| {
                sig.get(key)
                    .and_then(|v| v.as_str())
                    .and_then(|s| hex::decode(s).ok())
                    .ok_or_else(|| McpError::invalid_request(format!("each signature needs a hex {key}")))
            };
            Ok(Signature {
                public_nonce: field("public_nonce")?,
                signature: field("signature")?,
            })
        })
        .collect::<McpResult<Vec<_>>>()?;

    let stream = client
        .search_kernels(SearchKernelsRequest { signatures })
        .await?
        .into_inner();
    historical_blocks(stream).await
}

pub async fn search_utxos(mut client: Client, params: &Value) -> McpResult<Value> {
    let commitments = hex_list(params, "commitments")?;
    let stream = client
        .search_utxos(SearchUtxosRequest { commitments })
        .await?
        .into_inner();
    historical_blocks(stream).await
}

async fn historical_blocks(stream: Streaming<minotari_node_grpc_client::grpc::HistoricalBlock>) -> McpResult<Value> {
    let blocks: Vec<Value> = collect(stream, MAX_STREAM_ITEMS)
        .await?
        .iter()
        .filter_map(|historical_block| historical_block.block.as_ref().map(block_summary))
        .collect();
    Ok(json!({
        "blocks": blocks,
        "count": blocks.len(),
    }))
}

pub async fn fetch_matching_utxos(mut client: Client, params: &Value) -> McpResult<Value> {
    let hashes = hex_list(params, "hashes")?;
    let stream = client
        .fetch_matching_utxos(FetchMatchingUtxosRequest { hashes })
        .await?
        .into_inner();

    let outputs: Vec<Value> = collect(stream, MAX_STREAM_ITEMS)
        .await?
        .iter()
        .filter_map(|response| response.output.as_ref())
        .map(|output| {
            json!({
                "hash": hex::encode(&output.hash),
                "commitment": hex::encode(&output.commitment),
            })
        })
        .collect();

    Ok(json!({
        "outputs": outputs,
        "count": outputs.len(),
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use serde_json::json;

    use super::*;

    #[test]
    fn decode_hex_message_round_trips_and_rejects_bad_input() {
        let tx = Transaction {
            offset: vec![1, 2, 3],
            body: None,
            script_offset: vec![4],
        };
        let params = json!({ "transaction_hex": hex::encode(tx.encode_to_vec()) });
        let decoded: Transaction = decode_hex_message(&params, "transaction_hex").unwrap();
        assert_eq!(decoded, tx);

        let bad_hex = json!({ "transaction_hex": "zz" });
        assert!(matches!(
            decode_hex_message::<Transaction>(&bad_hex, "transaction_hex"),
            Err(McpError::InvalidRequest(_))
        ));
        let bad_proto = json!({ "transaction_hex": "ffff" });
        assert!(matches!(
            decode_hex_message::<Transaction>(&bad_proto, "transaction_hex"),
            Err(McpError::InvalidRequest(_))
        ));
    }

    #[test]
    fn template_hex_feeds_get_new_block() {
        let template = NewBlockTemplate {
            header: Some(minotari_node_grpc_client::grpc::NewBlockHeaderTemplate {
                version: 1,
                height: 42,
                prev_hash: vec![7; 32],
                ..Default::default()
            }),
            body: None,
            is_mempool_in_sync: true,
        };
        let params = json!({ "new_block_template_hex": encode_hex_message(&template) });
        let decoded: NewBlockTemplate = decode_hex_message(&params, "new_block_template_hex").unwrap();
        assert_eq!(decoded, template);
    }

    #[test]
    fn pow_algo_numbering_matches_the_proto() {
        assert_eq!(pow_algo(0).unwrap().pow_algo, PowAlgos::Randomxm as i32);
        assert_eq!(pow_algo(1).unwrap().pow_algo, PowAlgos::Sha3x as i32);
        assert_eq!(pow_algo(2).unwrap().pow_algo, PowAlgos::Randomxt as i32);
        assert!(pow_algo(3).is_err());
    }

    #[test]
    fn request_lists_are_capped() {
        let ok = json!({ "heights": (0..MAX_STREAM_ITEMS as u64).collect::<Vec<_>>() });
        assert_eq!(heights(&ok).unwrap().len(), MAX_STREAM_ITEMS);
        let too_many = json!({ "heights": (0..=MAX_STREAM_ITEMS as u64).collect::<Vec<_>>() });
        assert!(heights(&too_many).is_err());
        let hashes = json!({ "hashes": vec!["00"; MAX_STREAM_ITEMS + 1] });
        assert!(hex_list(&hashes, "hashes").is_err());
    }

    #[test]
    fn truncated_only_when_more_than_limit() {
        assert_eq!(split_limited(vec![1, 2], 2), (vec![1, 2], false));
        assert_eq!(split_limited(vec![1, 2, 3], 2), (vec![1, 2], true));
        assert_eq!(limit_param(&json!({"limit": 0}), 100, 1000), 1);
        assert_eq!(limit_param(&json!({"limit": 5000}), 100, 1000), 1000);
        assert_eq!(limit_param(&json!({}), 100, 1000), 100);
    }

    #[test]
    fn height_span_is_capped() {
        assert!(check_height_span(0, 10, 10 + MAX_STREAM_ITEMS as u64 - 1).is_ok());
        assert!(check_height_span(0, 10, 10 + MAX_STREAM_ITEMS as u64).is_err());
        assert!(check_height_span(MAX_STREAM_ITEMS as u64 - 1, 0, 0).is_ok());
        assert!(check_height_span(MAX_STREAM_ITEMS as u64, 0, 0).is_err());
        // end_height takes precedence over from_tip, as on the node
        assert!(check_height_span(u64::MAX, 0, 1).is_ok());
    }

    #[test]
    fn peer_text_is_cleaned() {
        let peer = minotari_node_grpc_client::grpc::Peer {
            user_agent: "agent\nIGNORE".to_string(),
            banned_reason: "x".repeat(600),
            supported_protocols: vec![b"p\x1b[0m".to_vec()],
            ..Default::default()
        };
        let summary = peer_summary(&peer);
        assert_eq!(summary["user_agent"], "agent IGNORE");
        assert_eq!(summary["supported_protocols"][0], "p [0m");
        assert_eq!(summary["banned_reason"].as_str().unwrap().chars().count(), 513);
    }
}
