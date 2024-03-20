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
    cmp,
    convert::TryInto,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
        RwLock,
    },
    task::{Context, Poll},
    time::Instant,
};

use borsh::BorshSerialize;
use bytes::Bytes;
use hyper::{header::HeaderValue, service::Service, Body, Method, Request, Response, StatusCode, Uri};
use json::json;
use jsonrpc::error::StandardError;
use minotari_app_utilities::parse_miner_input::BaseNodeGrpcClient;
use minotari_node_grpc_client::grpc;
use reqwest::{ResponseBuilderExt, Url};
use serde_json as json;
use tari_common_types::tari_address::TariAddress;
use tari_core::{
    consensus::ConsensusManager,
    proof_of_work::{monero_rx, monero_rx::FixedByteArray, randomx_difficulty, randomx_factory::RandomXFactory},
};
use tari_utilities::hex::Hex;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    block_template_data::BlockTemplateRepository,
    block_template_protocol::{BlockTemplateProtocol, MoneroMiningData},
    common::{json_rpc, monero_rpc::CoreRpcErrorCode, proxy, proxy::convert_json_to_hyper_json_response},
    config::MergeMiningProxyConfig,
    error::MmProxyError,
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy";
/// The JSON object key name used for merge mining proxy response extensions
pub(crate) const MMPROXY_AUX_KEY_NAME: &str = "_aux";
/// The identifier used to identify the tari aux chain data
const TARI_CHAIN_ID: &str = "xtr";

#[derive(Debug, Clone)]
pub struct MergeMiningProxyService {
    inner: InnerService,
}

impl MergeMiningProxyService {
    pub fn new(
        config: MergeMiningProxyConfig,
        http_client: reqwest::Client,
        base_node_client: BaseNodeGrpcClient,
        block_templates: BlockTemplateRepository,
        randomx_factory: RandomXFactory,
        wallet_payment_address: TariAddress,
    ) -> Result<Self, MmProxyError> {
        debug!(target: LOG_TARGET, "Config: {:?}", config);
        let consensus_manager = ConsensusManager::builder(config.network).build()?;
        Ok(Self {
            inner: InnerService {
                config: Arc::new(config),
                block_templates,
                http_client,
                base_node_client,
                initial_sync_achieved: Arc::new(AtomicBool::new(false)),
                current_monerod_server: Arc::new(RwLock::new(None)),
                last_assigned_monerod_server: Arc::new(RwLock::new(None)),
                randomx_factory,
                consensus_manager,
                wallet_payment_address,
            },
        })
    }
}

#[allow(clippy::type_complexity)]
impl Service<Request<Body>> for MergeMiningProxyService {
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Response<Body>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let inner = self.inner.clone();
        let future = async move {
            let bytes = match proxy::read_body_until_end(request.body_mut()).await {
                Ok(b) => b,
                Err(err) => {
                    warn!(target: LOG_TARGET, "Method: Unknown, Failed to read request: {:?}", err);
                    let resp = proxy::json_response(
                        StatusCode::BAD_REQUEST,
                        &json_rpc::standard_error_response(
                            None,
                            StandardError::InvalidRequest,
                            Some(json!({"details": err.to_string()})),
                        ),
                    )
                    .expect("unexpected failure");
                    return Ok(resp);
                },
            };
            let request = request.map(|_| bytes.freeze());
            let method_name = parse_method_name(&request);
            match inner.handle(&method_name, request).await {
                Ok(resp) => Ok(resp),
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "Method \"{}\" failed handling request: {:?}", method_name, err
                    );
                    Ok(proxy::json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &json_rpc::standard_error_response(
                            None,
                            StandardError::InternalError,
                            Some(json!({"details": err.to_string()})),
                        ),
                    )
                    .expect("unexpected failure"))
                },
            }
        };

        Box::pin(future)
    }
}

#[derive(Debug, Clone)]
struct InnerService {
    config: Arc<MergeMiningProxyConfig>,
    block_templates: BlockTemplateRepository,
    http_client: reqwest::Client,
    base_node_client: BaseNodeGrpcClient,
    initial_sync_achieved: Arc<AtomicBool>,
    current_monerod_server: Arc<RwLock<Option<String>>>,
    last_assigned_monerod_server: Arc<RwLock<Option<String>>>,
    randomx_factory: RandomXFactory,
    consensus_manager: ConsensusManager,
    wallet_payment_address: TariAddress,
}

impl InnerService {
    #[instrument(level = "trace")]
    #[allow(clippy::cast_possible_wrap)]
    async fn handle_get_height(&self, monerod_resp: Response<json::Value>) -> Result<Response<Body>, MmProxyError> {
        let (parts, mut json) = monerod_resp.into_parts();
        if json["height"].is_null() {
            error!(target: LOG_TARGET, r#"Monerod response was invalid: "height" is null"#);
            debug!(target: LOG_TARGET, "Invalid monerod response: {}", json);
            return Err(MmProxyError::InvalidMonerodResponse(
                "`height` field was missing from /get_height response".to_string(),
            ));
        }

        let mut base_node_client = self.base_node_client.clone();
        info!(target: LOG_TARGET, "Successful connection to base node GRPC");

        let result =
            base_node_client
                .get_tip_info(grpc::Empty {})
                .await
                .map_err(|err| MmProxyError::GrpcRequestError {
                    status: err,
                    details: "get_tip_info failed".to_string(),
                })?;
        let height = result
            .get_ref()
            .metadata
            .as_ref()
            .map(|meta| meta.best_block_height)
            .ok_or(MmProxyError::GrpcResponseMissingField("base node metadata"))?;
        if result.get_ref().initial_sync_achieved != self.initial_sync_achieved.load(Ordering::SeqCst) {
            self.initial_sync_achieved
                .store(result.get_ref().initial_sync_achieved, Ordering::SeqCst);
            debug!(
                target: LOG_TARGET,
                "Minotari base node initial sync status change to {}",
                result.get_ref().initial_sync_achieved
            );
        }

        info!(
            target: LOG_TARGET,
            "Monero height = #{}, Minotari base node height = #{}", json["height"], height
        );

        json["height"] = json!(cmp::max(json["height"].as_i64().unwrap_or_default(), height as i64));
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

        debug!(target: LOG_TARGET, "handle_submit_block: submit request #{}", request);
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

        for param in params.iter().filter_map(|p| p.as_str()) {
            let monero_block = monero_rx::deserialize_monero_block_from_hex(param)?;
            debug!(target: LOG_TARGET, "Monero block: {}", monero_block);
            let hash = monero_rx::extract_aux_merkle_root_from_block(&monero_block)?.ok_or_else(|| {
                MmProxyError::MissingDataError("Could not find Minotari header in coinbase".to_string())
            })?;

            debug!(
                target: LOG_TARGET,
                "Minotari Hash found in Monero block: {}",
                hex::encode(hash)
            );

            let mut block_data = match self.block_templates.get_final_template(&hash).await {
                Some(d) => d,
                None => {
                    info!(
                        target: LOG_TARGET,
                        "Block `{}` submitted but no matching block template was found, possible duplicate submission",
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
            let start = Instant::now();
            let achieved_target = if self.config.check_tari_difficulty_before_submit {
                trace!(target: LOG_TARGET, "Starting calculate achieved Tari difficultly");
                let diff = randomx_difficulty(
                    &tari_header,
                    &self.randomx_factory,
                    self.consensus_manager.get_genesis_block().hash(),
                    &self.consensus_manager,
                )?;
                trace!(
                    target: LOG_TARGET,
                    "Finished calculate achieved Tari difficultly - achieved {} vs. target {}",
                    diff,
                    block_data.template.tari_difficulty
                );
                diff.as_u64()
            } else {
                block_data.template.tari_difficulty
            };

            let height = tari_header_mut.height;
            if achieved_target >= block_data.template.tari_difficulty {
                match base_node_client.submit_block(block_data.template.tari_block).await {
                    Ok(resp) => {
                        if self.config.submit_to_origin {
                            json_resp = json_rpc::success_response(
                                request["id"].as_i64(),
                                json!({ "status": "OK", "untrusted": !self.initial_sync_achieved.load(Ordering::SeqCst) }),
                            );
                            let resp = resp.into_inner();
                            json_resp = append_aux_chain_data(
                                json_resp,
                                json!({"id": TARI_CHAIN_ID, "block_hash": resp.block_hash.to_hex()}),
                            );
                            debug!(
                                target: LOG_TARGET,
                                "Submitted block #{} to Minotari node in {:.0?} (SubmitBlock)",
                                height,
                                start.elapsed()
                            );
                        } else {
                            // self-select related, do not change.
                            json_resp = json_rpc::default_block_accept_response(request["id"].as_i64());
                            trace!(
                                target: LOG_TARGET,
                                "pool merged mining proxy_submit_to_origin({}) json_resp: {}",
                                self.config.submit_to_origin,
                                json_resp
                            );
                        }
                        self.block_templates.remove_final_block_template(&hash).await;
                    },
                    Err(err) => {
                        debug!(
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
            let grpc::TipInfoResponse {
                initial_sync_achieved,
                metadata,
                ..
            } = grpc_client.get_tip_info(grpc::Empty {}).await?.into_inner();

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

        let new_block_protocol = BlockTemplateProtocol::new(
            &mut grpc_client,
            self.config.clone(),
            self.consensus_manager.clone(),
            self.wallet_payment_address.clone(),
        )
        .await?;

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

        let final_block_template_data = new_block_protocol
            .get_next_block_template(monero_mining_data, &self.block_templates)
            .await?;

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
        let monerod_resp = add_aux_data(
            monerod_resp,
            json!({ "base_difficulty": final_block_template_data.template.monero_difficulty }),
        );
        let monerod_resp = append_aux_chain_data(
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
        let resp = client.get_header_by_hash(grpc::GetHeaderByHashRequest { hash }).await;
        match resp {
            Ok(resp) => {
                let json_block_header = try_into_json_block_header(resp.into_inner())?;

                debug!(
                    target: LOG_TARGET,
                    "[get_header_by_hash] Found minotari block header with hash `{}`", hash_hex
                );
                let json_resp =
                    json_rpc::success_response(request["id"].as_i64(), json!({ "block_header": json_block_header }));

                let json_resp = append_aux_chain_data(json_resp, json!({ "id": TARI_CHAIN_ID }));

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
        let tip_info = client.get_tip_info(grpc::Empty {}).await?;
        let tip_info = tip_info.into_inner();
        let chain_metadata = tip_info.metadata.ok_or_else(|| {
            MmProxyError::UnexpectedTariBaseNodeResponse("get_tip_info returned no chain metadata".into())
        })?;

        let tip_header = client
            .get_header_by_hash(grpc::GetHeaderByHashRequest {
                hash: chain_metadata.best_block_hash,
            })
            .await?;

        let tip_header = tip_header.into_inner();
        let json_block_header = try_into_json_block_header(tip_header)?;
        let resp = append_aux_chain_data(
            monero_resp,
            json!({
                "id": TARI_CHAIN_ID,
                "block_header": json_block_header,
            }),
        );
        Ok(proxy::into_response(parts, &resp))
    }

    async fn get_fully_qualified_monerod_url(&self, uri: &Uri) -> Result<Url, MmProxyError> {
        {
            let lock = self
                .current_monerod_server
                .read()
                .expect("Read lock should not fail")
                .clone();
            if let Some(server) = lock {
                let uri = format!("{}{}", server, uri.path()).parse::<Url>()?;
                return Ok(uri);
            }
        }

        let last_used_url = {
            let lock = self
                .last_assigned_monerod_server
                .read()
                .expect("Read lock should not fail")
                .clone();
            match lock {
                Some(url) => url,
                None => "".to_string(),
            }
        };

        // Query the list twice before giving up, starting after the last used entry
        let pos = if let Some(index) = self.config.monerod_url.iter().position(|x| x == &last_used_url) {
            index
        } else {
            0
        };
        let (left, right) = self.config.monerod_url.split_at(pos);
        let left = left.to_vec();
        let right = right.to_vec();
        let iter = right.iter().chain(left.iter()).chain(right.iter()).chain(left.iter());

        for next_url in iter {
            let uri = format!("{}{}", next_url, uri.path()).parse::<Url>()?;
            match reqwest::get(uri.clone()).await {
                Ok(_) => {
                    let mut lock = self.current_monerod_server.write().expect("Write lock should not fail");
                    *lock = Some(next_url.to_string());
                    let mut lock = self
                        .last_assigned_monerod_server
                        .write()
                        .expect("Write lock should not fail");
                    *lock = Some(next_url.to_string());
                    info!(target: LOG_TARGET, "Monerod server available: {:?}", uri.clone());
                    return Ok(uri);
                },
                Err(_) => {
                    warn!(target: LOG_TARGET, "Monerod server unavailable: {:?}", uri);
                },
            }
        }

        Err(MmProxyError::ServersUnavailable)
    }

    /// Proxy a request received by this server to Monerod
    async fn proxy_request_to_monerod(
        &self,
        request: Request<Bytes>,
    ) -> Result<(Request<Bytes>, Response<json::Value>), MmProxyError> {
        let monerod_uri = self.get_fully_qualified_monerod_url(request.uri()).await?;

        let mut headers = request.headers().clone();
        // Some public monerod setups (e.g. those that are reverse proxied by nginx) require the Host header.
        // The mmproxy is the direct client of monerod and so is responsible for setting this header.
        if let Some(host) = monerod_uri.host_str() {
            let host: HeaderValue = match monerod_uri.port_or_known_default() {
                Some(port) => format!("{}:{}", host, port).parse()?,
                None => host.parse()?,
            };
            headers.insert("host", host);
            debug!(
                target: LOG_TARGET,
                "Host header updated to match monerod_uri. Request headers: {:?}", headers
            );
        }
        let mut builder = self
            .http_client
            .request(request.method().clone(), monerod_uri.clone())
            .headers(headers);

        if self.config.monerod_use_auth {
            // Use HTTP basic auth. This is the only reason we are using `reqwest` over the standard hyper client.
            builder = builder.basic_auth(&self.config.monerod_username, Some(&self.config.monerod_password));
        }

        debug!(
            target: LOG_TARGET,
            "[monerod] request: {} {}",
            request.method(),
            monerod_uri,
        );

        let mut submit_block = false;
        let body: Bytes = request.body().clone();
        let json = json::from_slice::<json::Value>(&body[..]).unwrap_or_default();
        if let Some(method) = json["method"].as_str() {
            trace!(target: LOG_TARGET, "json[\"method\"]: {}", method);
            match method {
                "submitblock" | "submit_block" => {
                    submit_block = true;
                },
                _ => {},
            }
            trace!(
                target: LOG_TARGET,
                "submitblock({}), proxy_submit_to_origin({})",
                submit_block,
                self.config.submit_to_origin
            );
        }

        // If the request is a block submission and we are not submitting blocks
        // to the origin (self-select mode, see next comment for a full explanation)
        let json_response = if submit_block && !self.config.submit_to_origin {
            debug!(
                target: LOG_TARGET,
                "[monerod] skip: Proxy configured for self-select mode. Pool will submit to MoneroD, submitting to \
                 Minotari.",
            );

            // This is required for self-select configuration.
            // We are not submitting the block to Monero here (the pool does this),
            // we are only interested in intercepting the request for the purposes of
            // submitting the block to Tari which will only happen if the accept response
            // (which normally would occur for normal mining) is provided here.
            // There is no point in trying to submit the block to Monero here since the
            // share submitted by XMRig is only guaranteed to meet the difficulty of
            // min(Tari,Monero) since that is what was returned with the original template.
            // So it would otherwise be a duplicate submission of what the pool will do
            // itself (whether the miner submits directly to monerod or the pool does,
            // the pool is the only one being paid out here due to the nature
            // of self-select). Furthermore, discussions with devs from Monero and XMRig are
            // very much against spamming the nodes unnecessarily.
            // NB!: This is by design, do not change this without understanding
            // it's implications.
            let accept_response = json_rpc::default_block_accept_response(json["id"].as_i64());

            convert_json_to_hyper_json_response(accept_response, StatusCode::OK, monerod_uri.clone()).await?
        } else {
            let resp = builder
                // This is a cheap clone of the request body
                .body(body)
                .send()
                .await
                .map_err(MmProxyError::MonerodRequestFailed)?;
            convert_reqwest_response_to_hyper_json_response(resp).await?
        };

        let rpc_status = if json_response.body()["error"].is_null() {
            "ok"
        } else {
            json_response.body()["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
        };
        debug!(
            target: LOG_TARGET,
            "[monerod] response: status = {}, monerod_rpc = {}",
            json_response.status(),
            rpc_status
        );
        Ok((request, json_response))
    }

    async fn get_proxy_response(
        &self,
        request: Request<Bytes>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError> {
        match request.method().clone() {
            Method::GET => {
                // All get requests go to /request_name, methods do not have a body, optionally could have query params
                // if applicable.
                match request.uri().path() {
                    "/get_height" | "/getheight" => self.handle_get_height(monerod_resp).await,
                    _ => Ok(proxy::into_body_from_response(monerod_resp)),
                }
            },
            Method::POST => {
                // All post requests go to /json_rpc, body of request contains a field `method` to indicate which call
                // takes place.
                let json = json::from_slice::<json::Value>(request.body())?;
                let request = request.map(move |_| json);
                match request.body()["method"].as_str().unwrap_or_default() {
                    "submitblock" | "submit_block" => self.handle_submit_block(request, monerod_resp).await,
                    "getblocktemplate" | "get_block_template" => self.handle_get_block_template(monerod_resp).await,
                    "getblockheaderbyhash" | "get_block_header_by_hash" => {
                        self.handle_get_block_header_by_hash(request, monerod_resp).await
                    },
                    "getlastblockheader" | "get_last_block_header" => {
                        self.handle_get_last_block_header(monerod_resp).await
                    },

                    _ => Ok(proxy::into_body_from_response(monerod_resp)),
                }
            },
            // Simply return the response "as is"
            _ => Ok(proxy::into_body_from_response(monerod_resp)),
        }
    }

    async fn handle(self, method_name: &str, request: Request<Bytes>) -> Result<Response<Body>, MmProxyError> {
        let start = Instant::now();

        debug!(
            target: LOG_TARGET,
            "request: {} ({})",
            String::from_utf8_lossy(&request.body().clone()[..]),
            request
                .headers()
                .iter()
                .map(|(k, v)| format!("{}={}", k, String::from_utf8_lossy(v.as_ref())))
                .collect::<Vec<_>>()
                .join(","),
        );

        match self.proxy_request_to_monerod(request).await {
            Ok((request, monerod_resp)) => {
                // Any failed (!= 200 OK) responses from Monero are immediately returned to the requester
                let monerod_status = monerod_resp.status();
                if !monerod_status.is_success() {
                    // we dont break on monerod returning an error code.
                    warn!(
                        target: LOG_TARGET,
                        "Monerod returned an error: {}",
                        monerod_resp.status()
                    );
                    debug!(
                        "Method: {}, MoneroD Status: {}, Proxy Status: N/A, Response Time: {}ms",
                        method_name,
                        monerod_status,
                        start.elapsed().as_millis()
                    );
                    return Ok(monerod_resp.map(|json| json.to_string().into()));
                }

                let response = self.get_proxy_response(request, monerod_resp).await?;
                debug!(
                    "Method: {}, MoneroD Status: {}, Proxy Status: {}, Response Time: {}ms",
                    method_name,
                    monerod_status,
                    response.status(),
                    start.elapsed().as_millis()
                );
                Ok(response)
            },
            Err(e) => {
                // Monero Server encountered a problem processing the request, reset the current monerod server
                let mut lock = self.current_monerod_server.write().expect("Write lock should not fail");
                *lock = None;
                Err(e)
            },
        }
    }
}

async fn convert_reqwest_response_to_hyper_json_response(
    resp: reqwest::Response,
) -> Result<Response<json::Value>, MmProxyError> {
    let mut builder = Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    headers.extend(resp.headers().iter().map(|(name, value)| (name.clone(), value.clone())));

    builder = builder
        .version(resp.version())
        .status(resp.status())
        .url(resp.url().clone());

    let body = resp.json().await.map_err(MmProxyError::MonerodRequestFailed)?;
    let resp = builder.body(body)?;
    Ok(resp)
}

/// Add mmproxy extensions object to JSON RPC success response
pub fn add_aux_data(mut response: json::Value, mut ext: json::Value) -> json::Value {
    if response["result"].is_null() {
        return response;
    }
    match response["result"][MMPROXY_AUX_KEY_NAME].as_object_mut() {
        Some(obj_mut) => {
            let ext_mut = ext
                .as_object_mut()
                .expect("invalid parameter: expected `ext: json::Value` to be an object but it was not");
            obj_mut.append(ext_mut);
        },
        None => {
            response["result"][MMPROXY_AUX_KEY_NAME] = ext;
        },
    }
    response
}

/// Append chain data to the result object. If the result object is null, a JSON object is created.
///
/// ## Panics
///
/// If response["result"] is not a JSON object type or null.
pub fn append_aux_chain_data(mut response: json::Value, chain_data: json::Value) -> json::Value {
    let result = &mut response["result"];
    if result.is_null() {
        *result = json!({});
    }
    let chains = match result[MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut() {
        Some(arr_mut) => arr_mut,
        None => {
            result[MMPROXY_AUX_KEY_NAME]["chains"] = json!([]);
            result[MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut().unwrap()
        },
    };

    chains.push(chain_data);
    response
}

fn try_into_json_block_header(header: grpc::BlockHeaderResponse) -> Result<json::Value, MmProxyError> {
    let grpc::BlockHeaderResponse {
        header,
        reward,
        confirmations,
        difficulty,
        num_transactions,
    } = header;
    let header = header.ok_or_else(|| {
        MmProxyError::UnexpectedTariBaseNodeResponse(
            "Base node GRPC returned an empty header field when calling get_header_by_hash".into(),
        )
    })?;

    Ok(json!({
        "block_size": 0,
        "depth": confirmations,
        "difficulty": difficulty,
        "hash": header.hash.to_hex(),
        "height": header.height,
        "major_version": header.version,
        "minor_version": 0,
        "nonce": header.nonce,
        "num_txes": num_transactions,
        // Cannot be an orphan
        "orphan_status": false,
        "prev_hash": header.prev_hash.to_hex(),
        "reward": reward,
        "timestamp": header.timestamp
    }))
}

fn parse_method_name(request: &Request<Bytes>) -> String {
    match *request.method() {
        Method::GET => {
            let mut chars = request.uri().path().chars();
            chars.next();
            chars.as_str().to_string()
        },
        Method::POST => {
            let json = json::from_slice::<json::Value>(request.body()).unwrap_or_default();
            str::replace(json["method"].as_str().unwrap_or_default(), "\"", "")
        },
        _ => "unsupported".to_string(),
    }
}
