// Copyright 2025. The Tari Project
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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::Arc;

use hyper::{Response, StatusCode, body::Bytes};
use log::{debug, info, trace, warn};
use minotari_app_grpc::tari_rpc::{
    self,
    GetNewBlockTemplateWithCoinbasesRequest,
    NewBlockCoinbase,
    pow_algo::PowAlgos,
};
use minotari_app_utilities::parse_miner_input::BaseNodeGrpcClient;
use serde_json::{Value, json};
use tari_common_types::tari_address::TariAddress;

use crate::{
    block_template_storage::BlockTemplateStorage,
    config::XmrigProxyConfig,
    error::XmrigProxyError,
    proxy::service::{ProxyBody, json_response},
};

const LOG_TARGET: &str = "minotari::xmrig_proxy";

/// The byte offset in the 76-byte mining blob where the extra nonce starts.
/// XMRig writes a per-thread extra nonce here so mining threads don't duplicate work.
/// This corresponds to the high 4 bytes of the u64 nonce field.
pub const TARI_BLOB_RESERVED_OFFSET: u32 = 35;

/// The total size of the Tari mining blob in bytes.
const TARI_MINING_BLOB_SIZE: usize = 76;
/// The pow_algo byte value for RandomXT (= 2).
const POW_ALGO_RANDOMXT: u8 = 2;

#[derive(Clone)]
pub struct InnerService {
    pub config: Arc<XmrigProxyConfig>,
    pub base_node_client: BaseNodeGrpcClient,
    pub block_templates: BlockTemplateStorage,
    pub wallet_payment_address: TariAddress,
}

impl InnerService {
    /// Handle a JSON-RPC request from XMRig.
    pub async fn handle(&self, body: Bytes) -> Result<Response<ProxyBody>, XmrigProxyError> {
        let json: Value = serde_json::from_slice(&body)?;
        let method = json.get("method").and_then(Value::as_str).unwrap_or("");
        trace!(target: LOG_TARGET, "Received method: {method}");
        match method {
            "getblocktemplate" => self.handle_get_block_template(&json).await,
            "submitblock" => self.handle_submit_block(&json).await,
            "getblockcount" | "get_height" => self.handle_get_height(&json).await,
            _ => {
                debug!(target: LOG_TARGET, "Unknown method: {method}");
                json_response(
                    StatusCode::OK,
                    &json_rpc_error(json["id"].as_i64(), -32601, "Method not found"),
                )
            },
        }
    }

    /// Handle a GET /get_height request (some mining software uses this).
    pub async fn handle_get(&self, path: &str) -> Result<Response<ProxyBody>, XmrigProxyError> {
        match path {
            "/get_height" | "/getblockcount" => self.handle_get_height(&json!({})).await,
            _ => json_response(StatusCode::NOT_FOUND, &json!({"error": "Not found"})),
        }
    }

    async fn handle_get_height(&self, req: &Value) -> Result<Response<ProxyBody>, XmrigProxyError> {
        let mut client = self.base_node_client.clone();
        let tip = client.get_tip_info(tari_rpc::Empty {}).await?.into_inner();
        let height = tip.metadata.as_ref().map(|m| m.best_block_height).unwrap_or(0);
        json_response(
            StatusCode::OK,
            &json_rpc_success(
                req["id"].as_i64(),
                json!({ "count": height, "status": "OK" }),
            ),
        )
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_get_block_template(&self, req: &Value) -> Result<Response<ProxyBody>, XmrigProxyError> {
        let mut client = self.base_node_client.clone();

        // Check if the base node has completed initial sync
        if self.config.wait_for_initial_sync_at_startup {
            let tip = client.get_tip_info(tari_rpc::Empty {}).await?.into_inner();
            if !tip.initial_sync_achieved {
                let height = tip.metadata.as_ref().map(|m| m.best_block_height).unwrap_or(0);
                let msg = format!("Base node initial sync not yet achieved at height #{height}. Waiting...");
                info!(target: LOG_TARGET, "{msg}");
                return json_response(
                    StatusCode::OK,
                    &json_rpc_error(req["id"].as_i64(), -1, &msg),
                );
            }
        }

        // Get a new RandomXT block template from the base node with a coinbase for our wallet
        let coinbase_extra = self.config.coinbase_extra.as_bytes().to_vec();
        let result = client
            .get_new_block_template_with_coinbases(GetNewBlockTemplateWithCoinbasesRequest {
                algo: Some(tari_rpc::PowAlgo {
                    pow_algo: PowAlgos::Randomxt.into(),
                }),
                max_weight: 0,
                coinbases: vec![NewBlockCoinbase {
                    address: self.wallet_payment_address.to_base58(),
                    value: 1,
                    stealth_payment: false,
                    revealed_value_proof: matches!(
                        self.config.range_proof_type,
                        tari_transaction_components::transaction_components::RangeProofType::RevealedValue
                    ),
                    coinbase_extra,
                }],
            })
            .await
            .map_err(|status| {
                warn!(target: LOG_TARGET, "Failed to get block template: {status}");
                XmrigProxyError::GrpcError(status)
            })?
            .into_inner();

        let block = result.block.ok_or_else(|| XmrigProxyError::MissingData("block".to_string()))?;
        let miner_data = result
            .miner_data
            .ok_or_else(|| XmrigProxyError::MissingData("miner_data".to_string()))?;
        let merge_mining_hash = result.merge_mining_hash; // 32-byte mining hash for RandomXT
        let vm_key = result.vm_key; // RandomX VM key (seed hash for XMRig)

        let height = block.header.as_ref().map(|h| h.height).unwrap_or(0);
        let prev_hash = block.header.as_ref().map(|h| h.prev_hash.clone()).unwrap_or_default();

        if merge_mining_hash.len() != 32 {
            return Err(XmrigProxyError::MissingData(format!(
                "merge_mining_hash has wrong length: {}",
                merge_mining_hash.len()
            )));
        }

        // Build the 76-byte XMRig-compatible mining blob:
        // | 3 bytes (zeros) | 32 bytes (mining_hash) | 8 bytes (nonce, big-endian) | 33 bytes (pow_algo + padding) |
        //
        // The nonce region (bytes 35..43) is structured so that:
        //   - bytes 35..39: extra nonce written per-thread by XMRig at reserved_offset
        //   - bytes 39..43: main nonce iterated by XMRig (at standard Monero nonce offset 39)
        let blob = build_tari_mining_blob(&merge_mining_hash, 0u64, POW_ALGO_RANDOMXT);
        let blob_hex = hex::encode(&blob);
        let seed_hex = hex::encode(&vm_key);
        let prev_hash_hex = hex::encode(&prev_hash);

        let target_difficulty = miner_data.target_difficulty;
        let expected_reward = miner_data.reward.saturating_add(miner_data.total_fees);

        // Store the block template keyed by the 32-byte mining hash
        let mining_hash_key: [u8; 32] = merge_mining_hash
            .as_slice()
            .try_into()
            .map_err(|_| XmrigProxyError::MissingData("mining hash not 32 bytes".to_string()))?;
        self.block_templates.store(mining_hash_key, block).await;

        debug!(
            target: LOG_TARGET,
            "Returning block template for height #{height}, difficulty={target_difficulty}, seed={seed_hex}"
        );

        json_response(
            StatusCode::OK,
            &json_rpc_success(req["id"].as_i64(), json!({
                "blocktemplate_blob": blob_hex,
                "blockhashing_blob": blob_hex,
                "seed_hash": seed_hex,
                "difficulty": target_difficulty,
                "height": height,
                "prev_hash": prev_hash_hex,
                "reserved_offset": TARI_BLOB_RESERVED_OFFSET,
                "expected_reward": expected_reward,
                "status": "OK",
                "untrusted": false,
            })),
        )
    }

    async fn handle_submit_block(&self, req: &Value) -> Result<Response<ProxyBody>, XmrigProxyError> {
        let params = match req["params"].as_array() {
            Some(p) => p,
            None => {
                return json_response(
                    StatusCode::OK,
                    &json_rpc_error(req["id"].as_i64(), -32602, "params must be an array"),
                );
            },
        };

        let blob_hex = match params.first().and_then(Value::as_str) {
            Some(s) => s,
            None => {
                return json_response(
                    StatusCode::OK,
                    &json_rpc_error(req["id"].as_i64(), -32602, "params[0] must be a hex string"),
                );
            },
        };

        let blob = hex::decode(blob_hex).map_err(|e| XmrigProxyError::InvalidRequest(e.to_string()))?;

        if blob.len() != TARI_MINING_BLOB_SIZE {
            return json_response(
                StatusCode::OK,
                &json_rpc_error(
                    req["id"].as_i64(),
                    -32602,
                    &format!(
                        "submitted blob has wrong length: {} (expected {TARI_MINING_BLOB_SIZE})",
                        blob.len()
                    ),
                ),
            );
        }

        // Extract the 32-byte mining hash from bytes 3..35
        let mining_hash: [u8; 32] = blob[3..35]
            .try_into()
            .map_err(|_| XmrigProxyError::InvalidRequest("bad mining hash slice".to_string()))?;

        // Extract the 8-byte nonce from bytes 35..43 (big-endian u64)
        let nonce_bytes: [u8; 8] = blob[35..43]
            .try_into()
            .map_err(|_| XmrigProxyError::InvalidRequest("bad nonce slice".to_string()))?;
        let nonce = u64::from_be_bytes(nonce_bytes);

        // Look up the stored block template
        let mut block = match self.block_templates.take(&mining_hash).await {
            Some(b) => b,
            None => {
                let hash_hex = hex::encode(mining_hash);
                warn!(
                    target: LOG_TARGET,
                    "No block template found for mining hash {hash_hex} - possible duplicate submission"
                );
                return json_response(
                    StatusCode::OK,
                    &json_rpc_error(req["id"].as_i64(), -1, "Block template not found or already submitted"),
                );
            },
        };

        // Update the nonce in the block header
        if let Some(ref mut header) = block.header {
            header.nonce = nonce;
        } else {
            return Err(XmrigProxyError::MissingData("block header".to_string()));
        }

        let height = block.header.as_ref().map(|h| h.height).unwrap_or(0);
        info!(target: LOG_TARGET, "Submitting block #{height} with nonce={nonce} to Tari node");

        // Submit to the Tari base node
        let mut client = self.base_node_client.clone();
        match client.submit_block(block).await {
            Ok(resp) => {
                let block_hash = hex::encode(resp.into_inner().block_hash);
                info!(target: LOG_TARGET, "Block #{height} accepted by node, hash={block_hash}");
                json_response(
                    StatusCode::OK,
                    &json_rpc_success(req["id"].as_i64(), json!({
                        "status": "OK",
                        "untrusted": false,
                    })),
                )
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Block #{height} rejected by node: {e}");
                json_response(
                    StatusCode::OK,
                    &json_rpc_error(req["id"].as_i64(), -5, &format!("Block rejected: {e}")),
                )
            },
        }
    }
}

/// Build a 76-byte XMRig-compatible mining blob for Tari RandomXT.
///
/// Format:
/// ```text
/// | 3 bytes (zeros) | 32 bytes (mining_hash) | 8 bytes (nonce big-endian) | 33 bytes (pow_algo + zeros) |
/// ```
///
/// - Bytes 35..39: high nonce bytes (XMRig writes per-thread extra nonce here, at `reserved_offset`)
/// - Bytes 39..43: low nonce bytes (XMRig iterates this 4-byte field at the standard Monero nonce offset)
pub fn build_tari_mining_blob(mining_hash: &[u8], nonce: u64, pow_algo: u8) -> Vec<u8> {
    let mut blob = vec![0u8; 3];
    blob.extend_from_slice(mining_hash);
    blob.extend_from_slice(&nonce.to_be_bytes());
    blob.push(pow_algo);
    blob.extend_from_slice(&[0u8; 32]);
    blob
}

fn json_rpc_success(id: Option<i64>, result: Value) -> Value {
    json!({
        "id": id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "result": result,
    })
}

fn json_rpc_error(id: Option<i64>, code: i32, message: &str) -> Value {
    json!({
        "id": id.unwrap_or(-1),
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message,
        },
    })
}
