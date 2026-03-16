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

use hyper::{Response, StatusCode, body::Bytes};
use log::{debug, info, trace, warn};
use serde_json::{Value, json};
use tari_common_types::{
    tari_address::TariAddress,
    types::{
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        UncompressedCommitment,
        UncompressedPublicKey,
    },
};
use tari_core::{
    base_node::{LocalNodeCommsInterface, StateMachineHandle},
    consensus::BaseNodeConsensusManager,
    validation::tari_rx_vm_key_height,
};
use tari_transaction_components::{
    generate_coinbase_with_wallet_output,
    key_manager::{KeyManager, TariKeyId, TransactionKeyManagerInterface, TxoStage},
    tari_proof_of_work::PowAlgorithm,
    transaction_components::{
        CoinBaseExtra,
        KernelBuilder,
        RangeProofType,
        TransactionKernel,
        TransactionKernelVersion,
        memo_field::{MemoField, TxType},
    },
};
use tari_utilities::ByteArray;

use super::{
    block_template_storage::BlockTemplateStorage,
    error::XmrigProxyError,
    service::{ProxyBody, json_response},
};

const LOG_TARGET: &str = "minotari::base_node::xmrig_proxy";

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
    pub node_service: LocalNodeCommsInterface,
    pub consensus_rules: BaseNodeConsensusManager,
    /// State machine handle, available for future sync-status checks.
    #[allow(dead_code)]
    pub state_machine: StateMachineHandle,
    pub block_templates: BlockTemplateStorage,
    pub wallet_payment_address: TariAddress,
    pub coinbase_extra: Vec<u8>,
    pub range_proof_type: RangeProofType,
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
                    &json_rpc_error(
                        json.get("id").map(|v| v.as_i64()).unwrap_or_default(),
                        -32601,
                        "Method not found",
                    ),
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
        let mut handler = self.node_service.clone();
        let meta = handler.get_metadata().await?;
        let height = meta.best_block_height();
        json_response(
            StatusCode::OK,
            &json_rpc_success(
                req["id"].get("id").map(|v| v.as_i64()).unwrap_or_default(),
                json!({ "count": height, "status": "OK" }),
            ),
        )
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_get_block_template(&self, req: &Value) -> Result<Response<ProxyBody>, XmrigProxyError> {
        let mut handler = self.node_service.clone();

        // Get chain metadata to determine block height for weight/coinbase calculations
        let meta = handler.get_metadata().await?;
        let next_height = meta.best_block_height().saturating_add(1);

        let constants = self.consensus_rules.consensus_constants(next_height);
        let asking_weight = constants.max_block_transaction_weight();

        // Get a new RandomXT block template from the local node
        let mut new_template = handler
            .get_new_block_template(PowAlgorithm::RandomXT, asking_weight)
            .await
            .map_err(|e| {
                warn!(target: LOG_TARGET, "Failed to get block template: {e}");
                e
            })?;

        let height = new_template.header.height;
        // Capture target_difficulty from the template before it's consumed by get_new_block
        let target_difficulty = new_template.target_difficulty.as_u64();

        // Calculate the coinbase reward for this block
        let reward = self
            .consensus_rules
            .calculate_coinbase_and_fees(height, new_template.body.kernels())
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?
            .as_u64();

        // Validate coinbase count
        let max_coinbases = self
            .consensus_rules
            .consensus_constants(height)
            .max_block_coinbase_count();
        if 1 > max_coinbases {
            return Err(XmrigProxyError::InternalError(
                "No coinbases allowed by consensus".to_string(),
            ));
        }

        // Generate the coinbase output and kernel for our wallet address
        let coinbase_extra = CoinBaseExtra::try_from(self.coinbase_extra.clone())
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;
        let key_manager = KeyManager::new_random().map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;
        let script_key_id = TariKeyId::default();

        let (_, coinbase_output, coinbase_kernel, wallet_output) = generate_coinbase_with_wallet_output(
            0.into(),
            reward.into(),
            height,
            &coinbase_extra,
            &key_manager,
            &script_key_id,
            &self.wallet_payment_address,
            false, // stealth_payment
            constants,
            self.range_proof_type,
            MemoField::new_open(vec![], TxType::Coinbase).expect("empty user-data should always be valid"),
        )
        .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;

        new_template.body.add_output(coinbase_output);

        // Build the kernel signature
        let new_nonce = key_manager
            .get_random_key(None, None)
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;
        let total_nonce: UncompressedPublicKey = new_nonce
            .pub_key
            .to_public_key()
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;
        let total_excess: UncompressedCommitment = coinbase_kernel
            .excess
            .to_commitment()
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;
        let kernel_message = TransactionKernel::build_kernel_signature_message(
            TransactionKernelVersion::get_current_version(),
            coinbase_kernel.fee,
            coinbase_kernel.lock_height,
            &coinbase_kernel.features,
            &None,
        );
        let kernel_signature = key_manager
            .get_partial_txo_kernel_signature(
                wallet_output.commitment_mask_key_id(),
                &new_nonce.key_id,
                &CompressedPublicKey::new_from_pk(total_nonce),
                &CompressedPublicKey::new_from_pk(total_excess.as_public_key().clone()),
                TransactionKernelVersion::get_current_version(),
                &kernel_message,
                &coinbase_kernel.features,
                TxoStage::Output,
            )
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?
            .to_schnorr_signature()
            .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?;

        let kernel_new = KernelBuilder::new()
            .with_fee(0.into())
            .with_features(coinbase_kernel.features)
            .with_lock_height(coinbase_kernel.lock_height)
            .with_excess(&CompressedCommitment::from_commitment(
                coinbase_kernel
                    .excess
                    .to_commitment()
                    .map_err(|e| XmrigProxyError::InternalError(e.to_string()))?,
            ))
            .with_signature(CompressedSignature::new_from_schnorr(kernel_signature))
            .build()
            .unwrap();

        new_template.body.add_kernel(kernel_new);
        new_template.body.sort();

        // Ask the node to finalize the block (fills in MMR roots etc.)
        let new_block = handler.get_new_block(new_template).await.map_err(|e| {
            warn!(target: LOG_TARGET, "Failed to get new block: {e}");
            e
        })?;

        let block_height = new_block.header.height;
        let prev_hash = new_block.header.prev_hash.to_vec();

        // Compute the RandomXT mining hash
        let mining_hash = match new_block.header.pow.pow_algo {
            PowAlgorithm::RandomXT => new_block.header.mining_hash().to_vec(),
            algo => {
                return Err(XmrigProxyError::InternalError(format!(
                    "Expected RandomXT block template, got {algo:?}"
                )));
            },
        };

        if mining_hash.len() != 32 {
            return Err(XmrigProxyError::MissingData(format!(
                "mining_hash has wrong length: {}",
                mining_hash.len()
            )));
        }

        // Get the RandomX VM key (seed hash for XMRig) from the block at tari_rx_vm_key_height
        let vm_key_height = tari_rx_vm_key_height(block_height);
        let vm_key = *handler
            .get_header(vm_key_height)
            .await?
            .ok_or_else(|| XmrigProxyError::MissingData(format!("block header at height {vm_key_height} not found")))?
            .hash();

        // Build the 76-byte XMRig-compatible mining blob
        let blob = build_tari_mining_blob(&mining_hash, 0u64, POW_ALGO_RANDOMXT);
        let blob_hex = hex::encode(&blob);
        let seed_hex = hex::encode(vm_key);
        let prev_hash_hex = hex::encode(&prev_hash);

        let target_difficulty_val = target_difficulty;
        let expected_reward = reward;

        // Store the block template keyed by the 32-byte mining hash
        let mining_hash_key: [u8; 32] = mining_hash
            .as_slice()
            .try_into()
            .map_err(|_| XmrigProxyError::MissingData("mining hash not 32 bytes".to_string()))?;
        self.block_templates.store(mining_hash_key, new_block).await;

        debug!(
            target: LOG_TARGET,
            "Returning block template for height #{block_height}, seed={seed_hex}"
        );

        json_response(
            StatusCode::OK,
            &json_rpc_success(
                req["id"].as_i64(),
                json!({
                    "blocktemplate_blob": blob_hex,
                    "blockhashing_blob": blob_hex,
                    "seed_hash": seed_hex,
                    "difficulty": target_difficulty_val,
                    "height": block_height,
                    "prev_hash": prev_hash_hex,
                    "reserved_offset": TARI_BLOB_RESERVED_OFFSET,
                    "expected_reward": expected_reward,
                    "status": "OK",
                    "untrusted": false,
                }),
            ),
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
        let mining_hash: [u8; 32] = blob
            .get(3..35)
            .ok_or(XmrigProxyError::InvalidRequest("bad mining hash slice".to_string()))?
            .try_into()
            .map_err(|_| XmrigProxyError::InvalidRequest("bad mining hash slice".to_string()))?;

        // Extract the 8-byte nonce from bytes 35..43 (big-endian u64)
        let nonce_bytes: [u8; 8] = blob
            .get(35..43)
            .ok_or(XmrigProxyError::InvalidRequest("bad mining hash slice".to_string()))?
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
        block.header.nonce = nonce;

        let block_height = block.header.height;
        info!(target: LOG_TARGET, "Submitting block #{block_height} with nonce={nonce} to base node");

        // Submit to the base node via LocalNodeCommsInterface
        let mut handler = self.node_service.clone();
        match handler.submit_block(block).await {
            Ok(block_hash) => {
                let block_hash_hex = hex::encode(block_hash);
                info!(target: LOG_TARGET, "Block #{block_height} accepted, hash={block_hash_hex}");
                json_response(
                    StatusCode::OK,
                    &json_rpc_success(
                        req["id"].as_i64(),
                        json!({
                            "status": "OK",
                            "untrusted": false,
                        }),
                    ),
                )
            },
            Err(e) => {
                warn!(target: LOG_TARGET, "Block #{block_height} rejected: {e}");
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
