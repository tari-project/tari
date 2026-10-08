//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#![allow(clippy::indexing_slicing)]
use std::{
    convert::TryInto,
    sync::{
        Arc,
        RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use blake2::{Blake2s256, Digest, digest::Update};
use borsh::BorshSerialize;
use bytes::Bytes;
use hyper::{Request, Response, StatusCode, Uri, header::HeaderValue as HyperHeaderValue};
use log::error;
use minotari_app_grpc::tari_rpc::{self, GetTipInfoRequest, SubmitBlockRequest};
use minotari_app_utilities::parse_miner_input::{BaseNodeGrpcClient, ShaP2PoolGrpcClient};
use rand::random;
use serde_json as json;
use serde_json::json;
use tari_common::configuration::utils::mask_value;
use tari_common_types::tari_address::TariAddress;
use tari_core::{
    consensus::BaseNodeConsensusManager,
    proof_of_work::{
        monero_randomx_difficulty,
        monero_rx,
        monero_rx::{CoinbasePrefixMode, FixedByteArray},
        randomx_factory::RandomXFactory,
    },
};
use tari_utilities::hex::Hex;
use tokio::time::timeout;
use tracing::{debug, info, trace, warn};
use url::Url;

use crate::{
    block_template_data::BlockTemplateRepository,
    block_template_manager::{BlockTemplateManager, MoneroMiningData},
    common::{
        json_rpc,
        monero_rpc::{CoreRpcErrorCode, TARI_MERGE_MINING_TAG_SIZE},
        proxy,
        proxy::convert_json_to_hyper_json_response,
    },
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    proxy::{
        monerod_method::MonerodMethod,
        service::ProxyBody,
        utils::{convert_reqwest_response_to_hyper_json_response, request_bytes_to_value},
    },
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::inner";
/// The identifier used to identify the tari aux chain data
const TARI_CHAIN_ID: &str = "xtr";
const BUSY_QUALIFYING: &str = "BusyQualifyingMonerodUrl";

/// The backoff applied once a sweep of every configured monerod server has failed. It doubles with each consecutive
/// failed sweep, which is what stops an upstream outage from re-sweeping the whole list on every single inbound
/// request, and is capped low enough that a recovery is still noticed promptly.
const MONEROD_QUALIFY_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const MONEROD_QUALIFY_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Where the proxy is in the process of qualifying one of the configured `monerod_url` entries.
///
/// All of the states are held behind one lock so that the transition out of [`MonerodState::Unqualified`] is a
/// single atomic check-and-set. This used to be a read lock that was taken and released, followed by a separate
/// write lock, which let two requests on different worker threads both observe that no sweep was running and both
/// start a full sweep of every configured server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MonerodState {
    /// Nothing is qualified and no sweep is running, so the next request starts one.
    Unqualified,
    /// A sweep is running; other requests wait for its result instead of starting one of their own.
    ///
    /// `consecutive_failures` carries how many sweeps have already failed in a row, so that the backoff applied if
    /// this sweep fails too keeps growing.
    Qualifying { consecutive_failures: u32 },
    /// This server answered the last probe and is the one requests are proxied to.
    Qualified(String),
    /// Every configured server failed the last sweep. No new sweep starts before `retry_at`, and until then
    /// requests fail fast rather than each waiting out a `monerod_connection_timeout` of their own.
    Failed {
        retry_at: Instant,
        consecutive_failures: u32,
    },
}

/// What [`InnerService::claim_monerod_qualification`] tells its caller to do.
#[derive(Debug)]
enum Qualification {
    /// Proxy to this already-qualified server.
    Use(String),
    /// The caller now owns [`MonerodState::Qualifying`] and must run a sweep.
    Claimed,
    /// Another request is sweeping; wait for it to publish a result.
    Wait,
    /// Every configured server failed recently, so fail this request immediately and retry after `retry_in`.
    BackingOff { retry_in: Duration },
}

/// The backoff to apply after `consecutive_failures` sweeps have failed in a row.
fn qualification_backoff(consecutive_failures: u32) -> Duration {
    let doublings = consecutive_failures.saturating_sub(1).min(16);
    MONEROD_QUALIFY_BACKOFF_INITIAL
        .saturating_mul(2u32.saturating_pow(doublings))
        .min(MONEROD_QUALIFY_BACKOFF_MAX)
}

/// Whether a probe response means the server can be proxied to.
///
/// A probe is a bare `GET` of the inbound request's path, so a `4xx` is expected from any monerod that sits behind a
/// reverse proxy which only allows `POST /json_rpc`: the server answered, which is all the probe needs to
/// establish. A `5xx` is different - that is typically a reverse proxy reporting that monerod itself is down - and
/// so is a `429`, which explicitly asks us not to send more traffic.
fn is_usable_probe_status(status: reqwest::StatusCode) -> bool {
    !status.is_server_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS
}

#[derive(Debug, Clone)]
pub struct InnerService {
    pub(crate) config: Arc<MergeMiningProxyConfig>,
    pub(crate) block_templates: BlockTemplateRepository,
    pub(crate) http_client: reqwest::Client,
    pub(crate) base_node_client: BaseNodeGrpcClient,
    pub(crate) p2pool_client: Option<ShaP2PoolGrpcClient>,
    pub(crate) initial_sync_achieved: Arc<AtomicBool>,
    pub(crate) monerod_state: Arc<RwLock<MonerodState>>,
    pub(crate) last_assigned_monerod_url: Arc<RwLock<Option<String>>>,
    pub(crate) randomx_factory: RandomXFactory,
    pub(crate) consensus_manager: BaseNodeConsensusManager,
    pub(crate) wallet_payment_address: TariAddress,
}

impl InnerService {
    #[allow(clippy::cast_possible_wrap)]
    #[allow(clippy::indexing_slicing)]
    async fn handle_get_height(
        &self,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        trace!(target: LOG_TARGET, "handle_get_height monerod_resp body: {}", monerod_resp.body());
        let (parts, mut json) = monerod_resp.into_parts();
        if json["height"].is_null() {
            warn!(target: LOG_TARGET, r#"Monerod response was invalid: "height" is null"#);
            warn!(target: LOG_TARGET, "Invalid monerod response: {}", json);
            return Err(MmProxyError::InvalidMonerodResponse(
                "`height` field was missing from /get_height response".to_string(),
            ));
        }

        let monero_height = json["height"].as_u64().unwrap_or_default();
        let monero_hash: Vec<u8> = Hex::from_hex(json["hash"].as_str().unwrap_or_default()).unwrap_or_default();
        let base_node_height;
        let base_node_hash;
        let mut p2pool_height = 0;
        let mut p2pool_hash = vec![];

        if let Some(mut p2pool_client) = self.p2pool_client.clone() {
            let p2pool_resp = p2pool_client.get_tip_info(GetTipInfoRequest {}).await?;
            let res = p2pool_resp.into_inner();

            base_node_height = res.node_height;
            base_node_hash = res.node_tip_hash.clone();
            p2pool_height = res.p2pool_rx_height;
            p2pool_hash = res.p2pool_rx_tip_hash.clone();
        } else {
            let mut base_node_client = self.base_node_client.clone();
            trace!(target: LOG_TARGET, "Successful connection to base node GRPC");

            let result = base_node_client.get_tip_info(tari_rpc::Empty {}).await.map_err(|err| {
                MmProxyError::GrpcRequestError {
                    status: Box::new(err),
                    details: "get_tip_info failed".to_string(),
                }
            })?;
            let res = result.into_inner();

            base_node_height = res.metadata.as_ref().map(|m| m.best_block_height).unwrap_or_default();
            base_node_hash = res
                .metadata
                .as_ref()
                .map(|m| m.best_block_hash.clone())
                .unwrap_or_default();
        }
        info!(
            target: LOG_TARGET,
            "Monero height = #{}, Minotari base node height = #{}, P2pool height: #{}", monero_height, base_node_height, p2pool_height
        );

        // Add them together. You could multiply them a factor, but xmrig will generally just check if the height or the
        // hash is different.
        let reported_height = p2pool_height
            .saturating_add(monero_height)
            .saturating_add(base_node_height);

        // As of xmrig 6.22.0, the block hash is stored separately and does not need to match the block template they
        // are mining, but if that changes in future, you might need to return the monero hash.

        let hash: Vec<u8> = Blake2s256::new()
            .chain(&monero_hash)
            .chain(&base_node_hash)
            .chain(&p2pool_hash)
            .finalize()
            .to_vec();

        json["height"] = json!(reported_height as i64);
        json["hash"] = json!(&(hash).to_hex());
        proxy::into_response(parts, &json)
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_submit_block(
        &self,
        request: Request<json::Value>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        let request = request.body();
        let (parts, mut json_resp) = monerod_resp.into_parts();

        info!(target: LOG_TARGET, "Block submit request #{}", request);
        let params = match request["params"].as_array() {
            Some(v) => v,
            None => {
                return proxy::json_response(
                    StatusCode::OK,
                    &json_rpc::error_response(
                        request["id"].as_i64(),
                        CoreRpcErrorCode::WrongParam.into(),
                        "`params` field is empty or an invalid type for submit block request. Expected an array.",
                        None,
                    ),
                );
            },
        };

        for (i, param) in params.iter().filter_map(|p| p.as_str()).enumerate() {
            trace!(target: LOG_TARGET, "handle_submit_block, param {} of {}", i, params.len());
            let monero_block = monero_rx::deserialize_monero_block_from_hex(param)?;
            trace!(target: LOG_TARGET, "Monero block: {}", monero_block);
            let hash = monero_rx::extract_aux_merkle_root_from_block(&monero_block)?.ok_or_else(|| {
                MmProxyError::MissingDataError("Could not find Minotari header in coinbase".to_string())
            })?;
            debug!(target: LOG_TARGET, "Minotari Hash found in Monero block: {}", hex::encode(hash));

            let mut block_data = match self.block_templates.get_final_template(&hash).await {
                Some(d) => d,
                None => {
                    info!(
                        target: LOG_TARGET,
                        "Could not submit block `{}`, no matching block template found, possible duplicate submission",
                        hex::encode(hash)
                    );
                    continue;
                },
            };
            // GHSA-3qmx-q9pv-f3m4: which coinbase wire format the node will accept is a function of the Tari
            // header's height, so the height has to be read before the pow data is built.
            let tari_header_height = block_data
                .template
                .tari_block
                .header
                .as_ref()
                .ok_or(MmProxyError::UnexpectedMissingData("tari_block.header".to_string()))?
                .height;
            let monero_data = monero_rx::construct_monero_data(
                monero_block,
                block_data.template.monero_seed.clone(),
                block_data.aux_chain_hashes.clone(),
                block_data.template.tari_merge_mining_hash,
                CoinbasePrefixMode::for_height(&self.consensus_manager, tari_header_height),
            )?;

            debug!(target: LOG_TARGET, "Monero PoW Data: {:?}", monero_data);

            let tari_header_mut = block_data
                .template
                .tari_block
                .header
                .as_mut()
                .ok_or(MmProxyError::UnexpectedMissingData("tari_block.header".to_string()))?;
            let pow_mut = tari_header_mut
                .pow
                .as_mut()
                .ok_or(MmProxyError::UnexpectedMissingData("tari_block.header.pow".to_string()))?;
            BorshSerialize::serialize(&monero_data, &mut pow_mut.pow_data)
                .map_err(|err| MmProxyError::ConversionError(err.to_string()))?;
            let tari_header = tari_header_mut
                .clone()
                .try_into()
                .map_err(MmProxyError::ConversionError)?;
            let mut base_node_client = self.base_node_client.clone();
            let p2pool_client = self.p2pool_client.clone();
            let start = Instant::now();
            let achieved_target = if self.config.check_tari_difficulty_before_submit {
                trace!(target: LOG_TARGET, "Starting calculate achieved Tari difficultly");
                let diff = monero_randomx_difficulty(
                    &tari_header,
                    &self.randomx_factory,
                    self.consensus_manager.get_genesis_block().hash(),
                    &self.consensus_manager,
                )?;
                info!(
                    target: LOG_TARGET,
                    "Difficulty achieved Tari difficultly - achieved {} vs. target {}",
                    diff,
                    block_data.template.tari_difficulty
                );
                diff.as_u64()
            } else {
                block_data.template.tari_difficulty
            };

            let height = tari_header_mut.height;
            info!(
                target: LOG_TARGET,
                "Checking if we must submit block #{} to Minotari node with achieved target {} and expected target: {}",
                height,
                achieved_target,
                block_data.template.tari_difficulty
            );
            if achieved_target >= block_data.template.tari_difficulty {
                let resp = match p2pool_client {
                    Some(mut client) => {
                        info!(target: LOG_TARGET, "Submiting to p2pool");
                        client
                            .submit_block(SubmitBlockRequest {
                                block: Some(block_data.template.tari_block),

                                wallet_payment_address: self.wallet_payment_address.to_hex(),
                            })
                            .await
                    },
                    None => base_node_client.submit_block(block_data.template.tari_block).await,
                };

                match resp {
                    Ok(resp) => {
                        json_resp = json_rpc::success_response(
                            request["id"].as_i64(),
                            json!({ "status": "OK", "untrusted": !self.initial_sync_achieved.load(Ordering::SeqCst) }),
                        );
                        let resp = resp.into_inner();
                        json_resp = crate::proxy::utils::append_aux_chain_data(
                            json_resp,
                            json!({"id": TARI_CHAIN_ID, "block_hash": resp.block_hash.to_hex()}),
                        );
                        debug!(
                            target: LOG_TARGET,
                            "Submitted block #{} to Minotari node in {:.0?} (SubmitBlock)",
                            height,
                            start.elapsed()
                        );
                        self.block_templates.remove_final_block_template(&hash).await;
                    },
                    Err(err) => {
                        warn!(
                            target: LOG_TARGET,
                            "Problem submitting block #{} to Tari node, responded in  {:.0?} (SubmitBlock): {}",
                            height,
                            start.elapsed(),
                            err
                        );

                        if !self.config.submit_to_origin {
                            // When "submit to origin" is turned off the block is never submitted to monerod, and so we
                            // need to construct an error message here.
                            json_resp = json_rpc::error_response(
                                request["id"].as_i64(),
                                CoreRpcErrorCode::BlockNotAccepted.into(),
                                "Block not accepted",
                                None,
                            );
                        }
                    },
                }
            };
        }

        debug!(
            target: LOG_TARGET,
            "Sending submit_block response (proxy_submit_to_origin({})): {}", self.config.submit_to_origin, json_resp
        );
        proxy::into_response(parts, &json_resp)
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_get_block_template(
        &self,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        let (parts, mut monerod_resp) = monerod_resp.into_parts();
        debug!(
            target: LOG_TARGET,
            "handle_get_block_template: monero block #{}", monerod_resp["result"]["height"]
        );

        // If monderod returned an error, there is nothing further for us to do
        if !monerod_resp["error"].is_null() {
            return proxy::into_response(parts, &monerod_resp);
        }

        if monerod_resp["result"]["difficulty"].is_null() {
            return Err(MmProxyError::InvalidMonerodResponse(
                "Expected `get_block_template` to include `result.difficulty` but it was `null`".to_string(),
            ));
        }

        if monerod_resp["result"]["blocktemplate_blob"].is_null() {
            return Err(MmProxyError::InvalidMonerodResponse(
                "Expected `get_block_template` to include `result.blocktemplate_blob` but it was `null`".to_string(),
            ));
        }

        if monerod_resp["result"]["blockhashing_blob"].is_null() {
            return Err(MmProxyError::InvalidMonerodResponse(
                "Expected `get_block_template` to include `result.blockhashing_blob` but it was `null`".to_string(),
            ));
        }

        if monerod_resp["result"]["seed_hash"].is_null() {
            return Err(MmProxyError::InvalidMonerodResponse(
                "Expected `get_block_template` to include `result.seed_hash` but it was `null`".to_string(),
            ));
        }

        let mut grpc_client = self.base_node_client.clone();

        // Add merge mining tag on blocktemplate request
        if !self.initial_sync_achieved.load(Ordering::SeqCst) {
            let tari_rpc::TipInfoResponse {
                initial_sync_achieved,
                metadata,
                ..
            } = grpc_client.get_tip_info(tari_rpc::Empty {}).await?.into_inner();

            if initial_sync_achieved {
                self.initial_sync_achieved.store(true, Ordering::SeqCst);
                let msg = format!(
                    "Initial base node sync achieved. Ready to mine at height #{}",
                    metadata.as_ref().map(|h| h.best_block_height).unwrap_or_default(),
                );
                debug!(target: LOG_TARGET, "{msg}");
                println!("{msg}");
                println!("Listening on {}...", self.config.listener_address);
            } else {
                let msg = format!(
                    "Initial base node sync not achieved, current height at #{} ... (waiting = {})",
                    metadata.as_ref().map(|h| h.best_block_height).unwrap_or_default(),
                    self.config.wait_for_initial_sync_at_startup,
                );
                debug!(target: LOG_TARGET, "{msg}");
                println!("{msg}");
                if self.config.wait_for_initial_sync_at_startup {
                    return Err(MmProxyError::MissingDataError(msg));
                }
            }
        }

        let new_block_manager = BlockTemplateManager::try_create(
            &mut grpc_client,
            self.p2pool_client.clone(),
            self.config.clone(),
            self.consensus_manager.clone(),
            self.wallet_payment_address.clone(),
        )?;

        let seed_hash = FixedByteArray::from_hex(&monerod_resp["result"]["seed_hash"].to_string().replace('\"', ""))
            .map_err(|err| MmProxyError::InvalidMonerodResponse(format!("seed hash hex is invalid: {err}")))?;
        let blocktemplate_blob = monerod_resp["result"]["blocktemplate_blob"]
            .to_string()
            .replace('\"', "");
        let difficulty = monerod_resp["result"]["difficulty"].as_u64().unwrap_or_default();
        let monero_mining_data = MoneroMiningData {
            seed_hash,
            blocktemplate_blob,
            difficulty,
        };

        let final_block_template_data = new_block_manager
            .get_next_tari_block_template(monero_mining_data)
            .await?;

        self.block_templates
            .save_final_block_template_if_key_unique(final_block_template_data.clone())
            .await;

        monerod_resp["result"]["blocktemplate_blob"] = final_block_template_data.blocktemplate_blob.clone().into();
        monerod_resp["result"]["blockhashing_blob"] = final_block_template_data.blockhashing_blob.clone().into();
        monerod_resp["result"]["difficulty"] = final_block_template_data.target_difficulty.as_u64().into();

        // We must shift the reserved_offset so the miner writes its nonce in the correct place,
        // preventing coinbase corruption.
        if let Some(offset) = monerod_resp["result"]["reserved_offset"].as_u64() {
            let tag_size = TARI_MERGE_MINING_TAG_SIZE as u64;
            monerod_resp["result"]["reserved_offset"] = offset.saturating_add(tag_size).into();
        }

        let tari_difficulty = final_block_template_data.template.tari_difficulty;
        let tari_height = final_block_template_data
            .template
            .tari_block
            .header
            .as_ref()
            .map(|h| h.height)
            .unwrap_or(0);
        let aux_chain_mr = hex::encode(final_block_template_data.aux_chain_mr.clone());
        let block_reward = final_block_template_data.template.tari_miner_data.reward;
        let total_fees = final_block_template_data.template.tari_miner_data.total_fees;
        let monerod_resp = crate::proxy::utils::add_aux_data(
            monerod_resp,
            json!({ "base_difficulty": final_block_template_data.template.monero_difficulty }),
        );
        let monerod_resp = crate::proxy::utils::append_aux_chain_data(
            monerod_resp,
            json!({
                "id": TARI_CHAIN_ID,
                "difficulty": tari_difficulty,
                "height": tari_height,
                // The aux chain merkle root, before the final block hash can be calculated
                "mining_hash": aux_chain_mr,
                "miner_reward": block_reward.saturating_add(total_fees),
            }),
        );

        debug!(target: LOG_TARGET, "Returning template result: {}", monerod_resp);
        proxy::into_response(parts, &monerod_resp)
    }

    async fn handle_get_block_header_by_hash(
        &self,
        request: Request<json::Value>,
        monero_resp: Response<json::Value>,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        let (parts, monero_resp) = monero_resp.into_parts();
        // If monero succeeded, we're done here
        if !monero_resp["result"].is_null() {
            return proxy::into_response(parts, &monero_resp);
        }

        let request = request.into_body();
        let hash = request["params"]["hash"]
            .as_str()
            .ok_or("hash parameter is not a string")
            .and_then(|hash| hex::decode(hash).map_err(|_| "hash parameter is not a valid hex value"));
        let hash = match hash {
            Ok(hash) => hash,
            Err(err) => {
                return proxy::json_response(
                    StatusCode::OK,
                    &json_rpc::error_response(request["id"].as_i64(), CoreRpcErrorCode::WrongParam.into(), err, None),
                );
            },
        };

        // If monero succeeded in finding the header, we're done here
        if !monero_resp["result"].is_null() ||
            monero_resp["result"]["block_header"]["hash"]
                .as_str()
                .map(|hash| !hash.is_empty())
                .unwrap_or(false)
        {
            debug!(target: LOG_TARGET, "monerod found block `{}`.", hash.to_hex());
            return proxy::into_response(parts, &monero_resp);
        }

        let hash_hex = hash.to_hex();
        debug!(
            target: LOG_TARGET,
            "monerod could not find the block `{}`. Querying tari base node", hash_hex
        );

        let mut client = self.base_node_client.clone();
        let resp = client
            .get_header_by_hash(tari_rpc::GetHeaderByHashRequest { hash })
            .await;
        match resp {
            Ok(resp) => {
                let json_block_header = crate::proxy::utils::try_into_json_block_header(resp.into_inner())?;

                debug!(
                    target: LOG_TARGET,
                    "[get_header_by_hash] Found minotari block header with hash `{}`", hash_hex
                );
                let json_resp =
                    json_rpc::success_response(request["id"].as_i64(), json!({ "block_header": json_block_header }));

                let json_resp = crate::proxy::utils::append_aux_chain_data(json_resp, json!({ "id": TARI_CHAIN_ID }));

                proxy::into_response(parts, &json_resp)
            },
            Err(err) if err.code() == tonic::Code::NotFound => {
                debug!(
                    target: LOG_TARGET,
                    "[get_header_by_hash] No minotari block header found with hash `{}`", hash_hex
                );
                proxy::into_response(parts, &monero_resp)
            },
            Err(err) => Err(MmProxyError::GrpcRequestError {
                status: Box::new(err),
                details: "failed to get header by hash".to_string(),
            }),
        }
    }

    async fn handle_get_last_block_header(
        &self,
        monero_resp: Response<json::Value>,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        let (parts, monero_resp) = monero_resp.into_parts();
        if !monero_resp["error"].is_null() {
            return proxy::into_response(parts, &monero_resp);
        }

        let mut client = self.base_node_client.clone();
        let tip_info = client.get_tip_info(tari_rpc::Empty {}).await?;
        let tip_info = tip_info.into_inner();
        let chain_metadata = tip_info.metadata.ok_or_else(|| {
            MmProxyError::UnexpectedTariBaseNodeResponse("get_tip_info returned no chain metadata".into())
        })?;

        let tip_header = client
            .get_header_by_hash(tari_rpc::GetHeaderByHashRequest {
                hash: chain_metadata.best_block_hash,
            })
            .await?;

        let tip_header = tip_header.into_inner();
        let json_block_header = crate::proxy::utils::try_into_json_block_header(tip_header)?;
        let resp = crate::proxy::utils::append_aux_chain_data(
            monero_resp,
            json!({
                "id": TARI_CHAIN_ID,
                "block_header": json_block_header,
            }),
        );
        proxy::into_response(parts, &resp)
    }

    /// Atomically inspects the qualification state and, when a sweep is needed, claims the right to run it.
    ///
    /// The whole decision is made inside one write lock, so exactly one request can ever hold
    /// [`MonerodState::Qualifying`] and sweeps are properly serialised.
    fn claim_monerod_qualification(&self) -> Qualification {
        let mut state = self.monerod_state.write().expect("Write lock should not fail");
        let (decision, consecutive_failures) = match &*state {
            MonerodState::Qualified(server) => (Qualification::Use(server.clone()), 0),
            MonerodState::Qualifying { .. } => (Qualification::Wait, 0),
            MonerodState::Unqualified => (Qualification::Claimed, 0),
            MonerodState::Failed {
                retry_at,
                consecutive_failures,
            } => match retry_at.checked_duration_since(Instant::now()) {
                Some(retry_in) if !retry_in.is_zero() => (Qualification::BackingOff { retry_in }, 0),
                _ => (Qualification::Claimed, *consecutive_failures),
            },
        };
        if matches!(decision, Qualification::Claimed) {
            *state = MonerodState::Qualifying { consecutive_failures };
        }
        decision
    }

    /// Records the last assigned monerod server, which is where the next sweep starts from.
    fn set_last_assigned_monerod_url(&self, server: Option<&str>) {
        if let Some(server) = server {
            let mut lock = self
                .last_assigned_monerod_url
                .write()
                .expect("Write lock should not fail");
            *lock = Some(server.to_string());
        }
    }

    /// Releases the [`MonerodState::Qualifying`] claim without a qualified server and without caching a failure,
    /// for the sweep's own error paths (a configured entry that cannot produce a URL at all).
    fn abort_qualification(&self, last_assigned_server: Option<&str>) {
        let mut lock = self.monerod_state.write().expect("Write lock should not fail");
        *lock = MonerodState::Unqualified;
        drop(lock);
        self.set_last_assigned_monerod_url(last_assigned_server);
        trace!(
            target: LOG_TARGET, "Monerod status - Current: 'None', Last assigned: {}",
            mask_value(
                "monerod_url",
                &self.last_assigned_monerod_url.read().expect("Read lock should not fail").clone().unwrap_or_default()
            )
        );
    }

    /// Caches the failure of a sweep of every configured server, with a backoff.
    ///
    /// Without this, each failure reset the state so that the very next inbound request re-swept every configured
    /// entry. With the 32 default entries and the 2s default connection timeout a failing sweep takes about a
    /// minute, while every concurrent request gave up after 2s with a `500` - which xmrig reads as a dead pool, so
    /// it disconnects and reconnects, driving an inbound connect/error/reconnect storm for the whole outage.
    fn record_failed_qualification(&self, first_probed: Option<&str>) {
        let mut state = self.monerod_state.write().expect("Write lock should not fail");
        let consecutive_failures = match &*state {
            MonerodState::Qualifying { consecutive_failures } => consecutive_failures.saturating_add(1),
            // Another request has already published a result; leave it be.
            _ => return,
        };
        let backoff = qualification_backoff(consecutive_failures);
        let now = Instant::now();
        *state = MonerodState::Failed {
            retry_at: now.checked_add(backoff).unwrap_or(now),
            consecutive_failures,
        };
        drop(state);
        self.set_last_assigned_monerod_url(first_probed);
        warn!(
            target: LOG_TARGET,
            "All {} configured monerod servers failed to respond ({} consecutive sweeps); requests fail fast for \
            the next {:.1?}",
            self.config.monerod_url.len(), consecutive_failures, backoff
        );
    }

    fn clear_current_monerod_server_lock(&self, last_assigned_server: Option<&str>, host_with_error: Option<&str>) {
        // Current
        let mut lock = self.monerod_state.write().expect("Write lock should not fail");
        if let MonerodState::Qualifying { .. } = &*lock {
            // A sweep is in progress and will publish its own result, so do not clobber it: another request would
            // otherwise be free to start a second, concurrent sweep.
            trace!(target: LOG_TARGET, "A monerod server is being qualified; leaving the state alone");
            return;
        }
        if let Some(host) = host_with_error
            && let MonerodState::Qualified(server) = &*lock
            // If the error was reported on a previously assigned server, we do not clear the lock. This happens on
            // requests that timed out after a new server has been assigned.
            && !server.contains(host)
        {
            trace!(
                target: LOG_TARGET, "A new monerod server has already been assigned. Current: '{}', host with \
                error: '{}'",
                mask_value("monerod_url", server), host
            );
            return;
        }
        *lock = MonerodState::Unqualified;
        drop(lock);
        // Last assigned
        self.set_last_assigned_monerod_url(last_assigned_server);
        trace!(
            target: LOG_TARGET, "Monerod status - Current: 'None', Last assigned: {}",
            mask_value(
                "monerod_url",
                &self.last_assigned_monerod_url.read().expect("Read lock should not fail").clone().unwrap_or_default()
            )
        );
    }

    fn update_monerod_server_locks(&self, server: &str) {
        // Current
        let mut lock = self.monerod_state.write().expect("Write lock should not fail");
        *lock = MonerodState::Qualified(server.to_string());
        drop(lock);
        // Last assigned
        self.set_last_assigned_monerod_url(Some(server));
        let shown = mask_value("monerod_url", server);
        trace!(target: LOG_TARGET, "Monerod status - Current: {}, Last assigned: {}", shown, shown);
    }

    /// Appends the path of the inbound request to a configured monerod server to get the URL to proxy to.
    ///
    /// An [`MmProxyError::InvalidMonerodRequest`] means the configured entry is fine and it is the inbound request
    /// path that cannot be appended to it; any other error means the configured entry itself is unusable.
    fn monerod_url_for(&self, server: &str, request_uri: &Uri) -> Result<Url, MmProxyError> {
        match format!("{}{}", server, request_uri.path()).parse::<Url>() {
            Ok(url) => Ok(url),
            Err(err) => {
                if format!("{server}/getheight").parse::<Url>().is_ok() {
                    return Err(MmProxyError::InvalidMonerodRequest(request_uri.path().to_string()));
                }
                Err(err.into())
            },
        }
    }

    async fn get_monerod_url(&self, request_uri: &Uri) -> Result<Option<Url>, MmProxyError> {
        let mut busy_qualifying = 0u64;
        let start_reading_lock_time = Instant::now();
        loop {
            match self.claim_monerod_qualification() {
                // Return the previously qualified monerod URL if it exists
                Qualification::Use(server) => {
                    return match self.monerod_url_for(&server, request_uri) {
                        Ok(url) => Ok(Some(url)),
                        Err(err @ MmProxyError::InvalidMonerodRequest(_)) => Err(err),
                        Err(err) => {
                            // The qualified entry cannot produce a URL at all, so stop proxying to it.
                            self.clear_current_monerod_server_lock(None, None);
                            Err(err)
                        },
                    };
                },
                Qualification::Claimed => return self.qualify_monerod_server(request_uri).await.map(Some),
                Qualification::BackingOff { retry_in } => {
                    // Fail fast with the cached failure instead of making this request - and every other request
                    // that arrives during the outage - wait out a `monerod_connection_timeout` of its own.
                    trace!(
                        target: LOG_TARGET,
                        "All monerod servers are known to be unavailable, retrying in {:.1?}, {}",
                        retry_in, request_uri.path()
                    );
                    return Err(MmProxyError::ServersUnavailable(format!(
                        "all {} configured monerod servers failed to respond, retrying in {:.1?}",
                        self.config.monerod_url.len(),
                        retry_in
                    )));
                },
                Qualification::Wait => {
                    // Give some time for the server to be qualified
                    let time_lapsed = start_reading_lock_time.elapsed();
                    if time_lapsed > self.config.monerod_connection_timeout {
                        return Err(MmProxyError::ServersUnavailable(BUSY_QUALIFYING.to_string()));
                    }
                    trace!(
                        target: LOG_TARGET,
                        "Waiting for lock data ({} - {:.2?}), {}, {}",
                        {busy_qualifying = busy_qualifying.saturating_add(1); busy_qualifying}, time_lapsed, BUSY_QUALIFYING, request_uri.path()
                    );
                    tokio::time::sleep(Duration::from_millis(50)).await;
                },
            }
        }
    }

    /// Probes each configured monerod server in turn, starting after the one that was assigned last, and locks onto
    /// the first one that answers.
    ///
    /// The caller must already hold [`MonerodState::Qualifying`]; every exit path from here publishes a new state.
    async fn qualify_monerod_server(&self, request_uri: &Uri) -> Result<Url, MmProxyError> {
        // Create an iterator to query the list, starting after the last used entry
        let last_used_url = {
            let lock = self
                .last_assigned_monerod_url
                .read()
                .expect("Read lock should not fail")
                .clone();
            lock.unwrap_or_default()
        };
        let mut pos = self
            .config
            .monerod_url
            .iter()
            .position(|x| x == &last_used_url)
            .unwrap_or(0);
        pos = pos
            .saturating_add(1)
            .checked_rem(self.config.monerod_url.len())
            .unwrap_or(0);
        let (left, right) = self.config.monerod_url.split_at_checked(pos).ok_or_else(|| {
            self.abort_qualification(Some(self.config.monerod_url[0].as_str()));
            MmProxyError::ConversionError("last_used_url".to_string())
        })?;
        let left = left.to_vec();
        let right = right.to_vec();
        let iter = right.iter().chain(left.iter());

        // Lock the current and last monerod server into the first available server
        let mut first_probed = None;
        for server in iter {
            let start = Instant::now();
            let url = match self.monerod_url_for(server, request_uri) {
                Ok(url) => url,
                Err(err) => {
                    self.abort_qualification(Some(server));
                    return Err(err);
                },
            };
            if first_probed.is_none() {
                first_probed = Some(server.clone());
            }
            let pos = self.config.monerod_url.iter().position(|x| x == server).unwrap_or(0);
            debug!(
                target: LOG_TARGET, "Trying to connect to Monerod server at: {} (entry {} of {})",
                mask_value("monerod_url", url.as_str()), pos.saturating_add(1), self.config.monerod_url.len()
            );
            let probe = self
                .http_client
                .get(url.clone())
                .timeout(self.config.monerod_connection_timeout);
            // For this availability check we deliberately do not provide the body of the request if it is a POST
            // request and turns it into an invalid GET request. This is because we are only interested in the
            // connection. A typical response of a monerod daemon upon an invalid POST request
            // `https://<host>/json_rpc` would be:
            //     "error": {
            //         "code": -32600,
            //         "message": "Invalid Request"
            //     },
            //     "id": 0,
            //     "jsonrpc": "2.0"
            // This approach is used to verify the server's availability without needing a valid request body.
            //
            // `timeout` yields `Result<Result<Response, reqwest::Error>, Elapsed>`, so the outer `Ok` only says
            // that the request finished inside the timeout, not that it succeeded. Both results have to be
            // destructured: `Ok(Err(_))` is a connection refused, a DNS failure or a rejected TLS handshake, and
            // treating it as "server available" is what made the proxy lock onto a hard-down server, fail the real
            // request, clear the lock and do it all again without ever failing over to a healthy entry.
            match timeout(self.config.monerod_connection_timeout, probe.send()).await {
                Ok(Ok(response)) if is_usable_probe_status(response.status()) => {
                    self.update_monerod_server_locks(server);
                    info!(
                        target: LOG_TARGET,
                        "Monerod server available (response in {:.2?}, status {}, {} bytes): {}",
                        start.elapsed(),
                        response.status(),
                        response.content_length().unwrap_or_default(),
                        mask_value("monerod_url", url.as_str())
                    );
                    return Ok(url);
                },
                Ok(Ok(response)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Monerod server unavailable (status {} in {:.2?}): {}",
                        response.status(), start.elapsed(), mask_value("monerod_url", url.as_str())
                    );
                },
                Ok(Err(err)) => {
                    // `without_url` keeps the monerod URL (which may carry credentials or a query) out of the log
                    warn!(
                        target: LOG_TARGET,
                        "Monerod server unavailable (request failed in {:.2?}, {}): {}",
                        start.elapsed(), err.without_url(), mask_value("monerod_url", url.as_str())
                    );
                },
                Err(_) => {
                    warn!(
                        target: LOG_TARGET,
                        "Monerod server unavailable (timeout in {:.2?}): {}",
                        start.elapsed(), mask_value("monerod_url", url.as_str())
                    );
                },
            }
        }

        // Cache the failure with a backoff, so the next inbound request does not re-sweep the whole list straight
        // away. The entry the sweep started at is recorded as the last assigned one so that the next sweep starts
        // at the entry after it: without that, every failed sweep re-probed the same entries in the same order from
        // the same offset, always burning the full connection timeout on whichever dead entry happened to be first.
        self.record_failed_qualification(first_probed.as_deref());
        Err(MmProxyError::ServersUnavailable(
            self.config
                .monerod_url
                .iter()
                .map(|url| mask_value("monerod_url", url))
                .collect::<Vec<_>>()
                .join(","),
        ))
    }

    // Modifies the Monero `getblocktemplate` request to reserve space for the Minotari merge mining tag.
    /// This function intercepts the JSON-RPC parameters and ensures that Monerod accounts for the
    /// extra space (35 bytes) required for the Minotari tag in the coinbase transaction. This is
    /// crucial for correct block weight and hashing blob calculation.
    ///
    /// # Logic
    /// * If `extra_nonce` is present (common with XMRig), it appends 35 bytes of padding (70 hex zeros) to it.
    /// * Otherwise, it increments the `reserve_size` parameter by 35 bytes.
    ///
    /// # Returns
    /// * `Ok(true)` if the JSON was modified (the caller must re-serialize the body).
    /// * `Ok(false)` if no modification was made (e.g. parameters were missing).
    /// * `Err(_)` if an error occurred during processing.
    fn modify_monero_template_request(&self, json: &mut serde_json::Value) -> Result<bool, MmProxyError> {
        let params = match json.get_mut("params").and_then(|p| p.as_object_mut()) {
            Some(p) => p,
            None => return Ok(false),
        };

        if let Some(extra_nonce) = params
            .get("extra_nonce")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            // XMRig sent `extra_nonce`. Append hex zeroes to force weight calculation.
            let padding = "0".repeat(TARI_MERGE_MINING_TAG_SIZE * 2);
            let new_extra_nonce = format!("{}{}", extra_nonce, padding);
            params.insert("extra_nonce".to_string(), serde_json::json!(new_extra_nonce));
            params.remove("reserve_size");
        } else {
            let tag_size = TARI_MERGE_MINING_TAG_SIZE as u64;
            let current_reserve = params.get("reserve_size").and_then(|v| v.as_u64()).unwrap_or(0);
            params.insert(
                "reserve_size".to_string(),
                serde_json::json!(current_reserve.saturating_add(tag_size)),
            );
        }

        Ok(true)
    }

    /// Proxy a request received by this server to Monerod
    #[allow(clippy::too_many_lines)]
    async fn proxy_request_to_monerod(
        &self,
        request: Request<Bytes>,
        monerod_method: MonerodMethod,
    ) -> Result<(Request<Bytes>, Response<json::Value>), MmProxyError> {
        let trace_id = random::<u64>();
        trace!(target: LOG_TARGET, "proxy_request_to_monerod: '{}' (trace_id: {})", monerod_method, trace_id);

        // This is a cheap clone of the request body
        let mut body: Bytes = request.body().clone();
        let mut json = json::from_slice::<json::Value>(&body[..]).unwrap_or_default();
        let request_id = json["id"].as_i64();
        let self_select_response = monerod_method == MonerodMethod::SubmitBlock && !self.config.submit_to_origin;

        // Intercept the getblocktemplate request and ask Monerod to reserve an extra 35 bytes.
        // This forces Monerod to correctly calculate the block weight penalty
        if monerod_method == MonerodMethod::GetBlockTemplate && self.modify_monero_template_request(&mut json)? {
            let json_bytes = serde_json::to_vec(&json).map_err(|e| MmProxyError::ConversionError(e.to_string()))?;
            body = Bytes::from(json_bytes);
        }

        let start = Instant::now();
        let json_response = if let Some(monerod_url) = self.get_monerod_url(request.uri()).await? {
            let mut headers = request.headers().clone();

            // We changed the body, let's remove the content length, so that "reqwest" recalculates it
            headers.remove(hyper::header::CONTENT_LENGTH);

            // Some public monerod setups (e.g. those that are reverse proxied by nginx) require the Host header.
            // The mmproxy is the direct client of monerod and so is responsible for setting this header.
            if let Some(host) = monerod_url.host_str() {
                let host: HyperHeaderValue = match monerod_url.port_or_known_default() {
                    Some(port) => format!("{host}:{port}").parse()?,
                    None => host.parse()?,
                };
                headers.insert("host", host);
                debug!(
                    target: LOG_TARGET,
                    "Host header updated to match monerod_uri. Request headers: {headers:?} (trace_id: {trace_id})"
                );
            }

            let mut builder = self
                .http_client
                .request(request.method().clone(), monerod_url.clone())
                .headers(headers)
                .timeout(self.config.monerod_connection_timeout);

            if self.config.monerod_use_auth {
                // Use HTTP basic auth. This is the only reason we are using `reqwest` over the standard hyper client.
                let password = String::from_utf8_lossy(self.config.monerod_password.reveal());
                builder = builder.basic_auth(&self.config.monerod_username, Some(password));
            }

            debug!(
                target: LOG_TARGET,
                "[monerod] '{}' request: {} {} (trace_id: {})",
                monerod_method, request.method(), mask_value("monerod_url", monerod_url.as_str()), trace_id
            );

            if self_select_response {
                let accept_response = json_rpc::default_block_accept_response(request_id);
                convert_json_to_hyper_json_response(accept_response, StatusCode::OK).await?
            } else {
                // Send the request to the current monerod server
                match timeout(self.config.monerod_connection_timeout, builder.body(body).send()).await {
                    // `without_url` keeps the monerod URL (which may carry credentials or a query) out of the error
                    Ok(response) => match response.map_err(|e| MmProxyError::MonerodRequestFailed(e.without_url())) {
                        Ok(val) => convert_reqwest_response_to_hyper_json_response(val).await?,
                        Err(e) => {
                            warn!(
                                target: LOG_TARGET,
                                "[monerod] '{}' request response '{}' (trace_id: {})",
                                monerod_method, e, trace_id
                            );
                            self.handle_monerod_error_response(monerod_url.host_str(), e)?
                        },
                    },
                    Err(e) => {
                        let err = MmProxyError::MonerodTimeout(e.to_string());
                        warn!(
                            target: LOG_TARGET,
                            "[monerod] '{}' request response '{}' (trace_id: {})",
                            monerod_method, err, trace_id
                        );
                        self.handle_monerod_error_response(monerod_url.host_str(), err)?
                    },
                }
            }
        } else if self_select_response {
            let accept_response = json_rpc::default_block_accept_response(request_id);
            convert_json_to_hyper_json_response(accept_response, StatusCode::OK).await?
        } else {
            let err = MmProxyError::ServersUnavailable("No monerod servers available".to_string());
            warn!(
                target: LOG_TARGET,
                "[monerod] '{}' request response '{}' (trace_id: {})",
                monerod_method, err, trace_id
            );
            self.handle_monerod_error_response(None, err)?
        };

        debug!(
            target: LOG_TARGET,
            "[monerod] '{}' response status = {},{} trace_id: {}, response time: {}ms",
            monerod_method,
            json_response.status(),
            if json_response.body()["error"].is_null() {
                "".to_string()
            } else {
                format!(" error = {},", json_response.body()["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error"))
            },
            trace_id, start.elapsed().as_millis(),
        );
        trace!(
            target: LOG_TARGET,
            "[monerod] '{}' response '{:?}' (trace_id: {})",
            monerod_method, json_response, trace_id
        );
        Ok((request, json_response))
    }

    fn handle_monerod_error_response(
        &self,
        host_with_error: Option<&str>,
        err: MmProxyError,
    ) -> Result<Response<serde_json::Value>, MmProxyError> {
        self.clear_current_monerod_server_lock(None, host_with_error);
        Err(err)
    }

    async fn get_proxy_response(
        &self,
        request: Request<Bytes>,
        monerod_resp: Response<json::Value>,
        monerod_method: MonerodMethod,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        let start = Instant::now();
        trace!(target: LOG_TARGET, "[get_proxy_response] '{}'", monerod_method);
        let proxy_response = match monerod_method {
            MonerodMethod::GetHeight => self.handle_get_height(monerod_resp).await,
            MonerodMethod::GetBlockTemplate => self.handle_get_block_template(monerod_resp).await,
            MonerodMethod::SubmitBlock => {
                self.handle_submit_block(request_bytes_to_value(request)?, monerod_resp)
                    .await
            },
            MonerodMethod::GetBlockHeaderByHash => {
                self.handle_get_block_header_by_hash(request_bytes_to_value(request)?, monerod_resp)
                    .await
            },
            MonerodMethod::GetLastBlockHeader => self.handle_get_last_block_header(monerod_resp).await,
            _ => {
                // Simply return the response "as is"
                proxy::into_body_from_response(monerod_resp)
            },
        };
        trace!(
            target: LOG_TARGET,
            "[get_proxy_response] '{}' response time: {}ms",
            monerod_method, start.elapsed().as_millis()
        );
        proxy_response
    }

    pub(crate) async fn handle(
        self,
        monerod_method: MonerodMethod,
        request: Request<Bytes>,
    ) -> Result<Response<ProxyBody>, MmProxyError> {
        let start = Instant::now();
        debug!(
            target: LOG_TARGET,
            "[handle request] '{}' method: {}, uri: {}, headers: {:?}, body: {}",
            monerod_method,
            request.method(),
            request.uri(),
            request.headers(),
            String::from_utf8_lossy(&request.body().clone()[..]),
        );

        match self.proxy_request_to_monerod(request, monerod_method).await {
            Ok((request, monerod_resp)) => {
                // Any failed (!= 200 OK) responses from Monero are immediately returned to the requester
                let monerod_status = monerod_resp.status();
                if !monerod_status.is_success() {
                    warn!(
                        target: LOG_TARGET,
                        "[handle request] '{}' monerod status: {}, response time: {}ms",
                        monerod_method, monerod_resp.status(), start.elapsed().as_millis()
                    );
                    return proxy::into_body_from_response(monerod_resp);
                }

                match self.get_proxy_response(request, monerod_resp, monerod_method).await {
                    Ok(response) => Ok(response),
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "[handle request] '{}' get_proxy_response error: {}, response time: {}ms",
                            monerod_method, e, start.elapsed().as_millis()
                        );
                        Err(e)
                    },
                }
            },
            Err(e) => {
                error!(
                    target: LOG_TARGET,
                    "[handle request] '{}' proxy_request_to_monerod error: {}, response time: {}ms",
                    monerod_method, e, start.elapsed().as_millis()
                );
                Err(e)
            },
        }
    }
}

#[cfg(test)]
mod test {
    use std::{convert::Infallible, net::SocketAddr};

    use http_body_util::Full;
    use hyper::{server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use minotari_node_grpc_client::grpc::base_node_client::BaseNodeClient;
    use minotari_wallet_grpc_client::{ClientAuthenticationInterceptor, GrpcAuthentication};
    use tari_common::configuration::{Network, StringList};
    use tokio::net::TcpListener;
    use tonic::transport::Endpoint;

    use super::*;

    const NETWORK: Network = Network::LocalNet;

    fn test_service(monerod_url: Vec<String>, monerod_connection_timeout: Duration) -> InnerService {
        let config = MergeMiningProxyConfig {
            monerod_url: StringList::from(monerod_url),
            monerod_connection_timeout,
            network: NETWORK,
            ..Default::default()
        };
        // The base node is never contacted by the qualification tests, so a lazy channel is enough.
        let base_node_client = BaseNodeClient::with_interceptor(
            Endpoint::from_static("http://127.0.0.1:18142").connect_lazy(),
            ClientAuthenticationInterceptor::create(&GrpcAuthentication::default()).unwrap(),
        );
        InnerService {
            config: Arc::new(config),
            block_templates: BlockTemplateRepository::new(),
            http_client: reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap(),
            base_node_client,
            p2pool_client: None,
            initial_sync_achieved: Arc::new(AtomicBool::new(false)),
            monerod_state: Arc::new(RwLock::new(MonerodState::Unqualified)),
            last_assigned_monerod_url: Arc::new(RwLock::new(None)),
            randomx_factory: RandomXFactory::new(1),
            consensus_manager: BaseNodeConsensusManager::builder(NETWORK).build().unwrap(),
            wallet_payment_address: TariAddress::default(),
        }
    }

    fn get_height_uri() -> Uri {
        "/get_height".parse().unwrap()
    }

    fn monerod_state(service: &InnerService) -> MonerodState {
        service.monerod_state.read().expect("Read lock should not fail").clone()
    }

    fn last_assigned(service: &InnerService) -> Option<String> {
        service
            .last_assigned_monerod_url
            .read()
            .expect("Read lock should not fail")
            .clone()
    }

    /// Returns the address of a port nothing is listening on, so connecting to it is refused outright. This is the
    /// `Ok(Err(reqwest::Error))` case: the probe finished well inside the timeout, but it failed.
    async fn refused_address() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        addr
    }

    /// Spawns a minimal HTTP server on an ephemeral port that answers every request with `status`.
    async fn spawn_monerod_stub(status: StatusCode) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let service = service_fn(move |_request| async move {
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(status)
                                .body(Full::new(Bytes::from_static(b"{}")))
                                .expect("valid response"),
                        )
                    });
                    let _result = http1::Builder::new().serve_connection(TokioIo::new(tcp), service).await;
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn qualification_is_claimed_by_exactly_one_request() {
        let service = test_service(vec!["http://127.0.0.1:18081".to_string()], Duration::from_secs(2));
        // The check and the claim are a single critical section, so the second caller is told to wait rather than
        // launching a sweep of its own.
        assert!(matches!(service.claim_monerod_qualification(), Qualification::Claimed));
        assert!(matches!(service.claim_monerod_qualification(), Qualification::Wait));
        assert!(matches!(monerod_state(&service), MonerodState::Qualifying { .. }));
    }

    #[tokio::test]
    async fn a_server_that_refuses_the_connection_is_not_qualified() {
        let addr = refused_address().await;
        let server = format!("http://{addr}");
        let service = test_service(vec![server], Duration::from_secs(5));

        // `timeout(.., send())` resolves to `Ok(Err(..))` here. Treating the outer `Ok` as "server available" used
        // to qualify this server, so the proxy then hammered a dead host without ever failing over.
        let err = service.get_monerod_url(&get_height_uri()).await.unwrap_err();
        assert!(matches!(err, MmProxyError::ServersUnavailable(_)), "{err}");
        assert!(matches!(monerod_state(&service), MonerodState::Failed { .. }));
    }

    #[tokio::test]
    async fn a_server_that_reports_a_server_error_is_not_qualified() {
        let addr = spawn_monerod_stub(StatusCode::SERVICE_UNAVAILABLE).await;
        let server = format!("http://{addr}");
        let service = test_service(vec![server], Duration::from_secs(5));

        let err = service.get_monerod_url(&get_height_uri()).await.unwrap_err();
        assert!(matches!(err, MmProxyError::ServersUnavailable(_)), "{err}");
        assert!(matches!(monerod_state(&service), MonerodState::Failed { .. }));
    }

    #[tokio::test]
    async fn a_server_that_answers_is_qualified() {
        let addr = spawn_monerod_stub(StatusCode::OK).await;
        let server = format!("http://{addr}");
        let service = test_service(vec![server.clone()], Duration::from_secs(5));

        let url = service.get_monerod_url(&get_height_uri()).await.unwrap().unwrap();
        assert_eq!(url.as_str(), format!("{server}/get_height"));
        assert_eq!(monerod_state(&service), MonerodState::Qualified(server.clone()));
        assert_eq!(last_assigned(&service), Some(server));
    }

    #[test]
    fn the_qualification_backoff_grows_and_is_capped() {
        assert_eq!(qualification_backoff(0), MONEROD_QUALIFY_BACKOFF_INITIAL);
        assert_eq!(qualification_backoff(1), MONEROD_QUALIFY_BACKOFF_INITIAL);
        assert_eq!(
            qualification_backoff(2),
            MONEROD_QUALIFY_BACKOFF_INITIAL.saturating_mul(2)
        );
        assert_eq!(
            qualification_backoff(3),
            MONEROD_QUALIFY_BACKOFF_INITIAL.saturating_mul(4)
        );
        assert_eq!(qualification_backoff(100), MONEROD_QUALIFY_BACKOFF_MAX);
        assert_eq!(qualification_backoff(u32::MAX), MONEROD_QUALIFY_BACKOFF_MAX);
    }

    #[tokio::test]
    async fn a_failed_sweep_backs_off_instead_of_re_sweeping_on_every_request() {
        let addr = refused_address().await;
        let service = test_service(vec![format!("http://{addr}")], Duration::from_secs(5));
        let uri = get_height_uri();

        service.get_monerod_url(&uri).await.unwrap_err();
        let state = monerod_state(&service);
        assert!(
            matches!(state, MonerodState::Failed {
                consecutive_failures: 1,
                ..
            }),
            "{state:?}"
        );

        // The failure is cached, so this request fails fast off the cached state. Had it swept again, the failure
        // count would have gone to 2.
        let err = service.get_monerod_url(&uri).await.unwrap_err();
        assert!(matches!(err, MmProxyError::ServersUnavailable(_)), "{err}");
        let state = monerod_state(&service);
        assert!(
            matches!(state, MonerodState::Failed {
                consecutive_failures: 1,
                ..
            }),
            "{state:?}"
        );
    }

    #[tokio::test]
    async fn a_sweep_is_allowed_again_once_the_backoff_has_elapsed() {
        let addr = refused_address().await;
        let service = test_service(vec![format!("http://{addr}")], Duration::from_secs(5));
        let uri = get_height_uri();

        // Pretend a sweep failed and its backoff window has already passed.
        *service.monerod_state.write().unwrap() = MonerodState::Failed {
            retry_at: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
            consecutive_failures: 3,
        };

        service.get_monerod_url(&uri).await.unwrap_err();
        // The sweep ran, and the failure count carried over so the backoff keeps growing.
        let state = monerod_state(&service);
        assert!(
            matches!(state, MonerodState::Failed {
                consecutive_failures: 4,
                ..
            }),
            "{state:?}"
        );
    }

    #[tokio::test]
    async fn a_sweep_in_progress_is_not_clobbered_by_a_failing_request() {
        let service = test_service(vec!["http://127.0.0.1:18081".to_string()], Duration::from_secs(2));
        assert!(matches!(service.claim_monerod_qualification(), Qualification::Claimed));

        // A request that fails against the previously qualified server must not reset the state while a sweep is
        // running, or a second concurrent sweep could start.
        service.clear_current_monerod_server_lock(None, Some("127.0.0.1"));
        assert!(matches!(monerod_state(&service), MonerodState::Qualifying { .. }));
    }

    #[tokio::test]
    async fn a_failed_sweep_advances_the_round_robin() {
        let mut servers = Vec::with_capacity(3);
        for _ in 0..3u32 {
            servers.push(format!("http://{}", refused_address().await));
        }
        let service = test_service(servers.clone(), Duration::from_secs(5));

        // The sweep starts at the entry after the last assigned one, and records where it started so that the next
        // sweep does not re-probe the same entries in the same order.
        service.get_monerod_url(&get_height_uri()).await.unwrap_err();
        assert_eq!(last_assigned(&service), Some(servers[1].clone()));

        *service.monerod_state.write().unwrap() = MonerodState::Unqualified;
        service.get_monerod_url(&get_height_uri()).await.unwrap_err();
        assert_eq!(last_assigned(&service), Some(servers[2].clone()));

        *service.monerod_state.write().unwrap() = MonerodState::Unqualified;
        service.get_monerod_url(&get_height_uri()).await.unwrap_err();
        assert_eq!(last_assigned(&service), Some(servers[0].clone()));
    }
}
