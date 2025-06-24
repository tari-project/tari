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

use std::{
    convert::TryInto,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
        RwLock,
    },
    time::Instant,
};

use blake2::{digest::Update, Blake2s256, Digest};
use borsh::BorshSerialize;
use bytes::Bytes;
use hyper::{header::HeaderValue, Body, Request, Response, StatusCode, Uri};
use log::error;
use minotari_app_grpc::tari_rpc::{self, GetTipInfoRequest, SubmitBlockRequest};
use minotari_app_utilities::parse_miner_input::{BaseNodeGrpcClient, ShaP2PoolGrpcClient};
use rand::random;
use serde_json as json;
use serde_json::json;
use tari_common_types::tari_address::TariAddress;
use tari_core::{
    consensus::ConsensusManager,
    proof_of_work::{monero_randomx_difficulty, monero_rx, monero_rx::FixedByteArray, randomx_factory::RandomXFactory},
};
use tari_utilities::hex::Hex;
use tokio::time::timeout;
use tracing::{debug, info, trace, warn};
use url::Url;

use crate::{
    block_template_data::BlockTemplateRepository,
    block_template_manager::{BlockTemplateManager, MoneroMiningData},
    common::{json_rpc, monero_rpc::CoreRpcErrorCode, proxy, proxy::convert_json_to_hyper_json_response},
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    proxy::{
        monerod_method::MonerodMethod,
        utils::{convert_reqwest_response_to_hyper_json_response, request_bytes_to_value},
    },
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::inner";
/// The identifier used to identify the tari aux chain data
const TARI_CHAIN_ID: &str = "xtr";
const BUSY_QUALIFYING: &str = "BusyQualifyingMonerodUrl";

#[derive(Debug, Clone)]
pub struct InnerService {
    pub(crate) config: Arc<MergeMiningProxyConfig>,
    pub(crate) block_templates: BlockTemplateRepository,
    pub(crate) http_client: reqwest::Client,
    pub(crate) base_node_client: BaseNodeGrpcClient,
    pub(crate) p2pool_client: Option<ShaP2PoolGrpcClient>,
    pub(crate) initial_sync_achieved: Arc<AtomicBool>,
    pub(crate) current_monerod_server: Arc<RwLock<Option<String>>>,
    pub(crate) last_assigned_monerod_url: Arc<RwLock<Option<String>>>,
    pub(crate) randomx_factory: RandomXFactory,
    pub(crate) consensus_manager: ConsensusManager,
    pub(crate) wallet_payment_address: TariAddress,
}

impl InnerService {
    #[allow(clippy::cast_possible_wrap)]
    async fn handle_get_height(&self, monerod_resp: Response<json::Value>) -> Result<Response<Body>, MmProxyError> {
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
                    status: err,
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
        let reported_height = p2pool_height + monero_height + base_node_height;

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
        Ok(proxy::into_response(parts, &json))
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_submit_block(
        &self,
        request: Request<json::Value>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError> {
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
                )
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
            let monero_data = monero_rx::construct_monero_data(
                monero_block,
                block_data.template.monero_seed.clone(),
                block_data.aux_chain_hashes.clone(),
                block_data.template.tari_merge_mining_hash,
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
            self.block_templates.remove_outdated().await;
        }

        debug!(
            target: LOG_TARGET,
            "Sending submit_block response (proxy_submit_to_origin({})): {}", self.config.submit_to_origin, json_resp
        );
        Ok(proxy::into_response(parts, &json_resp))
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_get_block_template(
        &self,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError> {
        let (parts, mut monerod_resp) = monerod_resp.into_parts();
        debug!(
            target: LOG_TARGET,
            "handle_get_block_template: monero block #{}", monerod_resp["result"]["height"]
        );

        // If monderod returned an error, there is nothing further for us to do
        if !monerod_resp["error"].is_null() {
            return Ok(proxy::into_response(parts, &monerod_resp));
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
                debug!(target: LOG_TARGET, "{}", msg);
                println!("{}", msg);
                println!("Listening on {}...", self.config.listener_address);
            } else {
                let msg = format!(
                    "Initial base node sync not achieved, current height at #{} ... (waiting = {})",
                    metadata.as_ref().map(|h| h.best_block_height).unwrap_or_default(),
                    self.config.wait_for_initial_sync_at_startup,
                );
                debug!(target: LOG_TARGET, "{}", msg);
                println!("{}", msg);
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
            .map_err(|err| MmProxyError::InvalidMonerodResponse(format!("seed hash hex is invalid: {}", err)))?;
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
                "miner_reward": block_reward + total_fees,
            }),
        );

        debug!(target: LOG_TARGET, "Returning template result: {}", monerod_resp);
        Ok(proxy::into_response(parts, &monerod_resp))
    }

    async fn handle_get_block_header_by_hash(
        &self,
        request: Request<json::Value>,
        monero_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError> {
        let (parts, monero_resp) = monero_resp.into_parts();
        // If monero succeeded, we're done here
        if !monero_resp["result"].is_null() {
            return Ok(proxy::into_response(parts, &monero_resp));
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
                )
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
            return Ok(proxy::into_response(parts, &monero_resp));
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

                Ok(proxy::into_response(parts, &json_resp))
            },
            Err(err) if err.code() == tonic::Code::NotFound => {
                debug!(
                    target: LOG_TARGET,
                    "[get_header_by_hash] No minotari block header found with hash `{}`", hash_hex
                );
                Ok(proxy::into_response(parts, &monero_resp))
            },
            Err(err) => Err(MmProxyError::GrpcRequestError {
                status: err,
                details: "failed to get header by hash".to_string(),
            }),
        }
    }

    async fn handle_get_last_block_header(
        &self,
        monero_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError> {
        let (parts, monero_resp) = monero_resp.into_parts();
        if !monero_resp["error"].is_null() {
            return Ok(proxy::into_response(parts, &monero_resp));
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
        Ok(proxy::into_response(parts, &resp))
    }

    fn clear_current_monerod_server_lock(&self, last_assigned_server: Option<&str>, host_with_error: Option<&str>) {
        // Current
        let mut lock = self.current_monerod_server.write().expect("Write lock should not fail");
        let current = lock.clone();
        if let Some(host) = host_with_error {
            if let Some(server) = current.clone() {
                // If the error was reported on a previously assigned server, we do not clear the lock. This happens on
                // requests that timed out after a new server has been assigned.
                if !server.contains(host) {
                    trace!(
                        target: LOG_TARGET, "A new monerod server has already been assigned. Current: '{}', host with \
                        error: '{}'",
                        server, host
                    );
                    return;
                }
            }
        }
        *lock = None;
        // Last assigned
        if let Some(server) = last_assigned_server {
            let mut lock = self
                .last_assigned_monerod_url
                .write()
                .expect("Write lock should not fail");
            *lock = Some(server.to_string());
        }
        trace!(
            target: LOG_TARGET, "Monerod status - Current: 'None', Last assigned: {}",
            self.last_assigned_monerod_url.read().expect("Read lock should not fail").clone().unwrap_or_default()
        );
    }

    fn set_current_monerod_server_lock_busy(&self) {
        let mut lock = self.current_monerod_server.write().expect("Write lock should not fail");
        *lock = Some(BUSY_QUALIFYING.to_string());
        trace!(
            target: LOG_TARGET, "Monerod status - Current: '{}', Last assigned: {}",
            BUSY_QUALIFYING,
            self.last_assigned_monerod_url.read().expect("Read lock should not fail").clone().unwrap_or_default()
        );
    }

    fn update_monerod_server_locks(&self, server: &str) {
        // Current
        let mut lock = self.current_monerod_server.write().expect("Write lock should not fail");
        *lock = Some(server.to_string());
        // Last assigned
        let mut lock = self
            .last_assigned_monerod_url
            .write()
            .expect("Write lock should not fail");
        *lock = Some(server.to_string());
        trace!(target: LOG_TARGET, "Monerod status - Current: {}, Last assigned: {}", server, server);
    }

    #[allow(clippy::too_many_lines)]
    async fn get_monerod_url(&self, request_uri: &Uri) -> Result<Option<Url>, MmProxyError> {
        // Return the previously qualified monerod URL if it exists
        let mut busy_qualifying = 0;
        let start_reading_lock_time = Instant::now();
        loop {
            let lock_contents = {
                self.current_monerod_server
                    .read()
                    .expect("Read lock should not fail")
                    .clone()
            };
            if let Some(server) = lock_contents {
                // Give some time for the server to be qualified
                if server == BUSY_QUALIFYING {
                    let time_lapsed = start_reading_lock_time.elapsed();
                    if time_lapsed > self.config.monerod_connection_timeout {
                        return Err(MmProxyError::ServersUnavailable(BUSY_QUALIFYING.to_string()));
                    }
                    trace!(
                        target: LOG_TARGET,
                        "Waiting for lock data ({} - {:.2?}), {}, {}",
                        {busy_qualifying += 1; busy_qualifying}, time_lapsed, BUSY_QUALIFYING, request_uri.path()
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
                // Parse the URL if qualifying is done
                match format!("{}{}", server, request_uri.path()).parse::<Url>() {
                    Ok(url) => return Ok(Some(url)),
                    Err(err) => {
                        return if format!("{}/getheight", server).parse::<Url>().is_ok() {
                            Err(MmProxyError::InvalidMonerodRequest(request_uri.path().to_string()))
                        } else {
                            self.clear_current_monerod_server_lock(None, None);
                            Err(err.into())
                        };
                    },
                }
            }
            // If no server is qualified, proceed with qualifying
            break;
        }

        // Set the "busy qualifying" state
        self.set_current_monerod_server_lock_busy();

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
        pos = (pos + 1) % self.config.monerod_url.len();
        let (left, right) = self.config.monerod_url.split_at_checked(pos).ok_or_else(|| {
            self.clear_current_monerod_server_lock(Some(self.config.monerod_url[0].as_str()), None);
            MmProxyError::ConversionError("last_used_url".to_string())
        })?;
        let left = left.to_vec();
        let right = right.to_vec();
        let iter = right.iter().chain(left.iter());

        // Lock the current and last monerod server into the first available server
        for server in iter {
            let start = Instant::now();
            let url = match format!("{}{}", server, request_uri.path()).parse::<Url>() {
                Ok(val) => val,
                Err(e) => {
                    self.clear_current_monerod_server_lock(Some(server), None);
                    return if format!("{}/getheight", server).parse::<Url>().is_ok() {
                        Err(MmProxyError::InvalidMonerodRequest(request_uri.path().to_string()))
                    } else {
                        Err(e.into())
                    };
                },
            };
            let pos = self.config.monerod_url.iter().position(|x| x == server).unwrap_or(0);
            debug!(
                target: LOG_TARGET, "Trying to connect to Monerod server at: {} (entry {} of {})",
                url.as_str(), pos + 1, self.config.monerod_url.len()
            );
            match timeout(self.config.monerod_connection_timeout, reqwest::get(url.clone())).await {
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
                Ok(response) => {
                    self.update_monerod_server_locks(server);
                    info!(
                        target: LOG_TARGET,
                        "Monerod server available (response in {:.2?}, {} bytes): {}",
                        start.elapsed(),
                        match response {
                            Ok(data) => data.content_length().unwrap_or_default(),
                            Err(_) => 0,
                        },
                        url.as_str()
                    );
                    return Ok(Some(url));
                },
                Err(_) => {
                    warn!(
                        target: LOG_TARGET,
                        "Monerod server unavailable (timeout in {:.2?}): {}",
                        start.elapsed(), url.as_str()
                    );
                },
            }
        }

        // Clear the "busy qualifying" state
        self.clear_current_monerod_server_lock(None, None);
        Err(MmProxyError::ServersUnavailable(format!("{}", self.config.monerod_url)))
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
        let body: Bytes = request.body().clone();
        let json = json::from_slice::<json::Value>(&body[..]).unwrap_or_default();
        let request_id = json["id"].as_i64();
        let self_select_response = monerod_method == MonerodMethod::SubmitBlock && !self.config.submit_to_origin;

        let start = Instant::now();
        let json_response = if let Some(monerod_url) = self.get_monerod_url(request.uri()).await? {
            let mut headers = request.headers().clone();
            // Some public monerod setups (e.g. those that are reverse proxied by nginx) require the Host header.
            // The mmproxy is the direct client of monerod and so is responsible for setting this header.
            if let Some(host) = monerod_url.host_str() {
                let host: HeaderValue = match monerod_url.port_or_known_default() {
                    Some(port) => format!("{}:{}", host, port).parse()?,
                    None => host.parse()?,
                };
                headers.insert("host", host);
                debug!(
                    target: LOG_TARGET,
                    "Host header updated to match monerod_uri. Request headers: {:?} (trace_id: {})",
                    headers, trace_id
                );
            }
            let mut builder = self
                .http_client
                .request(request.method().clone(), monerod_url.clone())
                .headers(headers.clone())
                .timeout(self.config.monerod_connection_timeout);

            if self.config.monerod_use_auth {
                // Use HTTP basic auth. This is the only reason we are using `reqwest` over the standard hyper client.
                builder = builder.basic_auth(&self.config.monerod_username, Some(&self.config.monerod_password));
            }

            debug!(
                target: LOG_TARGET,
                "[monerod] '{}' request: {} {} (trace_id: {})",
                monerod_method, request.method(), monerod_url, trace_id
            );

            if self_select_response {
                let accept_response = json_rpc::default_block_accept_response(request_id);
                convert_json_to_hyper_json_response(accept_response, StatusCode::OK, monerod_url.clone()).await?
            } else {
                // Send the request to the current monerod server
                match timeout(self.config.monerod_connection_timeout, async {
                    builder.body(body.clone()).send().await
                })
                .await
                {
                    Ok(response) => match response.map_err(MmProxyError::MonerodRequestFailed) {
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
            convert_json_to_hyper_json_response(
                accept_response,
                StatusCode::OK,
                Url::parse("http://82.64.166.200:18081/json_rpc").expect("Invalid URL"),
            )
            .await?
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
    ) -> Result<Response<Body>, MmProxyError> {
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
                Ok(proxy::into_body_from_response(monerod_resp))
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
    ) -> Result<Response<Body>, MmProxyError> {
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
                    return Ok(monerod_resp.map(|json| json.to_string().into()));
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
