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

use crate::{
    block_template_data::{BlockTemplateDataBuilder, BlockTemplateRepository},
    common::{json_rpc, merge_mining, monero_rpc::CoreRpcErrorCode, proxy},
    error::MmProxyError,
};
use bytes::Bytes;
use futures::TryFutureExt;
use hyper::{service::Service, Body, Method, Request, Response, StatusCode, Uri};
use json::json;
use jsonrpc::error::StandardError;
use reqwest::{ResponseBuilderExt, Url};
use serde_json as json;
use std::{
    cmp,
    cmp::min,
    convert::TryFrom,
    future::Future,
    io::Write,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Instant,
};
use tari_app_grpc::{tari_rpc as grpc, tari_rpc::GetCoinbaseRequest};
use tari_common::{GlobalConfig, Network};
use tari_core::{
    blocks::{Block, NewBlockTemplate},
    proof_of_work::monero_rx,
};
use tari_utilities::hex::Hex;
use tracing::{debug, error, info, instrument, trace, warn};

const LOG_TARGET: &str = "tari_mm_proxy::proxy";
/// The JSON object key name used for merge mining proxy response extensions
pub(crate) const MMPROXY_AUX_KEY_NAME: &str = "_aux";
/// The identifier used to identify the tari aux chain data
const TARI_CHAIN_ID: &str = "xtr";

#[derive(Debug, Clone)]
pub struct MergeMiningProxyConfig {
    pub network: Network,
    pub monerod_url: String,
    pub monerod_username: String,
    pub monerod_password: String,
    pub monerod_use_auth: bool,
    pub grpc_base_node_address: SocketAddr,
    pub grpc_console_wallet_address: SocketAddr,
    pub proxy_host_address: SocketAddr,
    pub proxy_submit_to_origin: bool,
    pub wait_for_initial_sync_at_startup: bool,
}

impl From<GlobalConfig> for MergeMiningProxyConfig {
    fn from(config: GlobalConfig) -> Self {
        Self {
            network: config.network,
            monerod_url: config.monerod_url,
            monerod_username: config.monerod_username,
            monerod_password: config.monerod_password,
            monerod_use_auth: config.monerod_use_auth,
            grpc_base_node_address: config.grpc_base_node_address,
            grpc_console_wallet_address: config.grpc_console_wallet_address,
            proxy_host_address: config.proxy_host_address,
            proxy_submit_to_origin: config.proxy_submit_to_origin,
            wait_for_initial_sync_at_startup: config.wait_for_initial_sync_at_startup,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MergeMiningProxyService {
    inner: InnerService,
}

impl MergeMiningProxyService {
    pub fn new(config: MergeMiningProxyConfig, block_templates: BlockTemplateRepository) -> Self {
        Self {
            inner: InnerService {
                config,
                block_templates,
                http_client: reqwest::Client::new(),
                initial_sync_achieved: Arc::new(AtomicBool::new(false)),
            },
        }
    }

    pub async fn check_connections<W: Write>(&self, w: &mut W) -> bool {
        let mut is_success = true;
        let inner = &self.inner;

        if inner.config.proxy_submit_to_origin {
            let _ = writeln!(
                w,
                "Solo mining configuration detected, configured to submit to Monero daemon."
            );
        } else {
            let _ = writeln!(
                w,
                "Pooled mining configuration detected, configured to not submit to Monero daemon."
            );
        }

        let _ = writeln!(w, "Connections:");

        let _ = write!(w, "- monerod ({})... ", inner.config.monerod_url);
        let monerod_uri = inner
            .get_fully_qualified_monerod_url(&Uri::from_static("/json_rpc"))
            .expect("Configuration error: Unable to parse monero_url");
        let result = inner
            .http_client
            .request(Method::POST, monerod_uri)
            .body(
                json::to_string(&jsonrpc::Request {
                    method: "get_version",
                    params: &[],
                    id: Default::default(),
                    jsonrpc: None,
                })
                .expect("conversion to json should always succeed"),
            )
            .send()
            .map_err(MmProxyError::MonerodRequestFailed)
            .and_then(|resp| async {
                resp.json::<jsonrpc::Response>()
                    .await
                    .map_err(MmProxyError::MonerodRequestFailed)
            })
            .await;

        match result {
            Ok(jsonrpc::Response { error: Some(error), .. }) => {
                let _ = writeln!(w, "❌ ({})", error.message);
                is_success = false;
            },
            Ok(jsonrpc::Response { result: Some(resp), .. }) => {
                let _ = writeln!(w, "✅ (v{})", resp["version"].as_u64().unwrap_or(0));
            },
            Ok(_) => {
                let _ = writeln!(w, "✅");
            },
            Err(err) => {
                let _ = writeln!(w, "❌ ({})", err);
                is_success = false;
            },
        }

        let _ = write!(w, "- Tari base node GRPC ({})... ", inner.config.grpc_base_node_address);
        match inner.connect_grpc_client().await {
            Ok(_) => {
                let _ = writeln!(w, "✅");
            },
            Err(err) => {
                let _ = writeln!(w, "❌ ({:?})", err);
                is_success = false;
            },
        }

        let _ = write!(
            w,
            "- Tari wallet GRPC ({})... ",
            inner.config.grpc_console_wallet_address
        );
        match inner.connect_grpc_wallet_client().await {
            Ok(_) => {
                let _ = writeln!(w, "✅");
            },
            Err(err) => {
                let _ = writeln!(w, "❌ ({:?})", err);
                is_success = false;
            },
        }

        is_success
    }
}

impl Service<Request<Body>> for MergeMiningProxyService {
    type Error = hyper::Error;
    type Response = Response<Body>;

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let inner = self.inner.clone();
        async move {
            match inner.handle(req).await {
                Ok(resp) => Ok(resp),
                Err(err) => {
                    error!(target: LOG_TARGET, "Error handling request: {}", err);

                    Ok(proxy::json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &json_rpc::standard_error_response(None, StandardError::InternalError, None),
                    )
                    .expect("unexpected failure"))
                },
            }
        }
    }
}

#[derive(Debug, Clone)]
struct InnerService {
    config: MergeMiningProxyConfig,
    block_templates: BlockTemplateRepository,
    http_client: reqwest::Client,
    initial_sync_achieved: Arc<AtomicBool>,
}

impl InnerService {
    #[instrument]
    async fn handle_get_height(&mut self, monerod_resp: Response<json::Value>) -> Result<Response<Body>, MmProxyError> {
        let (parts, mut json) = monerod_resp.into_parts();
        if json["height"].is_null() {
            error!(target: LOG_TARGET, r#"Monerod response was invalid: "height" is null"#);
            debug!(target: LOG_TARGET, "Invalid monerod response: {}", json);
            return Err(MmProxyError::InvalidMonerodResponse(
                "`height` field was missing from /get_height response".to_string(),
            ));
        }

        let mut base_node_client = self.connect_grpc_client().await?;
        trace!(target: LOG_TARGET, "Successful connection to base node GRPC");

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
            .map(|meta| meta.height_of_longest_chain)
            .ok_or_else(|| MmProxyError::GrpcResponseMissingField("metadata"))?;
        if result.get_ref().initial_sync_achieved != self.initial_sync_achieved.load(Ordering::Relaxed) {
            self.initial_sync_achieved
                .store(result.get_ref().initial_sync_achieved, Ordering::Relaxed);
            debug!(
                target: LOG_TARGET,
                "Tari base node initial sync status change to {}",
                result.get_ref().initial_sync_achieved
            );
        }

        debug!(
            target: LOG_TARGET,
            "Monero height = #{}, Tari base node height = #{}", json["height"], height
        );

        json["height"] = json!(cmp::max(json["height"].as_i64().unwrap_or_default(), height as i64));

        Ok(proxy::into_response(parts, &json))
    }

    async fn handle_submit_block(
        &mut self,
        request: Request<json::Value>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError>
    {
        let request = request.body();
        let (parts, mut json_resp) = monerod_resp.into_parts();

        debug!(target: LOG_TARGET, "handle_submit_block: submit request #{}", request);
        debug!(
            target: LOG_TARGET,
            "Params received: #{:?}",
            request["params"].as_array()
        );
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
            let monero_block = merge_mining::deserialize_monero_block_from_hex(param)?;
            debug!(target: LOG_TARGET, "Monero block: {}", monero_block);
            let hash = merge_mining::extract_tari_hash(&monero_block)
                .copied()
                .ok_or_else(|| MmProxyError::MissingDataError("Could not find Tari header in coinbase".to_string()))?;

            debug!(
                target: LOG_TARGET,
                "Tari Hash found in Monero block: {}",
                hex::encode(&hash)
            );

            let mut block_data = match self.block_templates.get(&hash).await {
                Some(d) => d,
                None => {
                    info!(
                        target: LOG_TARGET,
                        "Block `{}` submitted but no matching block template was found, possible duplicate submission",
                        hex::encode(&hash)
                    );
                    continue;
                },
            };

            let monero_data = merge_mining::construct_monero_data(monero_block, block_data.monero_seed.clone())?;

            let header_mut = block_data.tari_block.header.as_mut().unwrap();
            let height = header_mut.height;
            header_mut.pow.as_mut().unwrap().pow_data = bincode::serialize(&monero_data)?;

            let mut base_node_client = self.connect_grpc_client().await?;
            let start = Instant::now();
            match base_node_client.submit_block(block_data.tari_block).await {
                Ok(_) => {
                    json_resp = json_rpc::default_block_accept_response(request["id"].as_i64());
                    debug!(
                        target: LOG_TARGET,
                        "Submitted block #{} to Tari node in {:.0?} (SubmitBlock)",
                        height,
                        start.elapsed()
                    );
                    self.block_templates.remove(&hash).await;
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Problem submitting block #{} to Tari node, responded in  {:.0?} (SubmitBlock): {}",
                        height,
                        start.elapsed(),
                        err
                    );

                    // Tari rejected the block in self-select
                    if !self.config.proxy_submit_to_origin {
                        json_resp = json_rpc::error_response(
                            request["id"].as_i64(),
                            CoreRpcErrorCode::BlockNotAccepted.into(),
                            "Block not accepted",
                            None,
                        );
                    }
                },
            }

            self.block_templates.remove_outdated().await;
        }

        debug!(target: LOG_TARGET, "Sending submit_block response {}", json_resp);
        Ok(proxy::into_response(parts, &json_resp))
    }

    async fn handle_get_block_template(
        &mut self,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError>
    {
        let (parts, mut monerod_resp) = monerod_resp.into_parts();
        debug!(
            target: LOG_TARGET,
            "handle_get_block_template: monero block #{}", monerod_resp["result"]["height"]
        );

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

        let mut grpc_client = self.connect_grpc_client().await?;

        // Add merge mining tag on blocktemplate request
        debug!(target: LOG_TARGET, "Requested new block template from Tari base node");

        let grpc::NewBlockTemplateResponse {
            miner_data,
            new_block_template,
            initial_sync_achieved,
        } = grpc_client
            .get_new_block_template(grpc::PowAlgo {
                pow_algo: grpc::pow_algo::PowAlgos::Monero.into(),
            })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?
            .into_inner();

        let miner_data = miner_data.ok_or_else(|| MmProxyError::GrpcResponseMissingField("miner_data"))?;
        let new_block_template =
            new_block_template.ok_or_else(|| MmProxyError::GrpcResponseMissingField("new_block_template"))?;

        let block_reward = miner_data.reward;
        let total_fees = miner_data.total_fees;
        let tari_difficulty = miner_data.target_difficulty;

        if !self.initial_sync_achieved.load(Ordering::Relaxed) {
            if !initial_sync_achieved {
                let msg = format!(
                    "Initial base node sync not achieved, current height at #{} ... (waiting = {})",
                    new_block_template.header.as_ref().map(|h| h.height).unwrap_or_default(),
                    self.config.wait_for_initial_sync_at_startup,
                );
                debug!(target: LOG_TARGET, "{}", msg);
                println!("{}", msg);
                if self.config.wait_for_initial_sync_at_startup {
                    return Err(MmProxyError::MissingDataError(" ".to_string() + &msg));
                }
            } else {
                self.initial_sync_achieved.store(true, Ordering::Relaxed);
                let msg = format!(
                    "Initial base node sync achieved at height #{}",
                    new_block_template.header.as_ref().map(|h| h.height).unwrap_or_default()
                );
                debug!(target: LOG_TARGET, "{}", msg);
                println!("{}", msg);
                println!("Listening on {}...", self.config.proxy_host_address);
            }
        }

        info!(
            target: LOG_TARGET,
            "Received new block template from Tari base node for height #{}",
            new_block_template.header.as_ref().map(|h| h.height).unwrap_or_default(),
        );

        let template_block = NewBlockTemplate::try_from(new_block_template)
            .map_err(|e| MmProxyError::MissingDataError(format!("GRPC Conversion Error: {}", e)))?;

        debug!(target: LOG_TARGET, "Trying to connect to wallet");
        let mut grpc_wallet_client = self.connect_grpc_wallet_client().await?;
        let coinbase_response = grpc_wallet_client
            .get_coinbase(GetCoinbaseRequest {
                reward: block_reward,
                fee: total_fees,
                height: template_block.header.height,
            })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?;
        let coinbase_transaction = coinbase_response.into_inner().transaction;

        let coinbased_block = merge_mining::add_coinbase(coinbase_transaction, template_block)?;
        debug!(target: LOG_TARGET, "Added coinbase to new block template");
        let block = grpc_client
            .get_new_block(coinbased_block)
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block".to_string(),
            })?
            .into_inner();

        let mining_hash = block.merge_mining_hash;

        let tari_block = Block::try_from(
            block
                .block
                .clone()
                .ok_or_else(|| MmProxyError::MissingDataError("Tari block".to_string()))?,
        )
        .map_err(MmProxyError::MissingDataError)?;
        debug!(target: LOG_TARGET, "New block received from Tari: {}", (tari_block));

        let block_data = BlockTemplateDataBuilder::default();
        let block_data = block_data
            .tari_block(
                block
                    .block
                    .ok_or_else(|| MmProxyError::GrpcResponseMissingField("block"))?,
            )
            .tari_miner_data(miner_data);

        // Deserialize the block template blob
        let block_template_blob = &monerod_resp["result"]["blocktemplate_blob"]
            .to_string()
            .replace("\"", "");
        debug!(target: LOG_TARGET, "Deserializing Blocktemplate Blob into Monero Block",);
        let mut monero_block = merge_mining::deserialize_monero_block_from_hex(block_template_blob)?;

        debug!(target: LOG_TARGET, "Appending Merged Mining Tag",);
        // Add the Tari merge mining tag to the retrieved block template
        monero_rx::append_merge_mining_tag(&mut monero_block, &mining_hash)?;

        debug!(target: LOG_TARGET, "Creating blockhashing blob from blocktemplate blob",);
        // Must be done after the tag is inserted since it will affect the hash of the miner tx
        let blockhashing_blob = monero_rx::create_blockhashing_blob(&monero_block)?;

        debug!(target: LOG_TARGET, "blockhashing_blob:{}", blockhashing_blob);
        monerod_resp["result"]["blockhashing_blob"] = blockhashing_blob.into();

        let blocktemplate_blob = merge_mining::serialize_monero_block_to_hex(&monero_block)?;
        debug!(target: LOG_TARGET, "blocktemplate_blob:{}", block_template_blob);
        monerod_resp["result"]["blocktemplate_blob"] = blocktemplate_blob.into();

        let seed = monerod_resp["result"]["seed_hash"].to_string().replace("\"", "");

        let block_data = block_data.monero_seed(seed);

        let monero_difficulty = monerod_resp["result"]["difficulty"].as_u64().unwrap_or_default();

        let mining_difficulty = min(monero_difficulty, tari_difficulty);

        let block_data = block_data
            .monero_difficulty(monero_difficulty)
            .tari_difficulty(tari_difficulty);

        info!(
            target: LOG_TARGET,
            "Difficulties: Tari ({}), Monero({}), Selected({})", tari_difficulty, monero_difficulty, mining_difficulty
        );
        monerod_resp["result"]["difficulty"] = mining_difficulty.into();
        let monerod_resp = add_aux_data(monerod_resp, json!({ "base_difficulty": monero_difficulty }));
        let monerod_resp = append_aux_chain_data(
            monerod_resp,
            json!({
                "id": TARI_CHAIN_ID,
                "difficulty": tari_difficulty,
                // The merge mining hash, before the final block hash can be calculated
                "mining_hash": mining_hash.to_hex(),
                "miner_reward": block_reward + total_fees,
            }),
        );

        self.block_templates.save(mining_hash, block_data.build()?).await;

        debug!(target: LOG_TARGET, "Returning template result: {}", monerod_resp);
        Ok(proxy::into_response(parts, &monerod_resp))
    }

    async fn handle_get_block_header_by_hash(
        &self,
        request: Request<json::Value>,
        monero_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError>
    {
        let (parts, monero_resp) = monero_resp.into_parts();

        let request = request.into_body();
        let hash = request["params"]["hash"]
            .as_str()
            .ok_or_else(|| "hash parameter is not a string")
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
            // TODO: add aux data for corresponding tari header, if it exists.
            return Ok(proxy::into_response(parts, &monero_resp));
        }

        debug!(
            target: LOG_TARGET,
            "monerod could not find the block `{}`. Querying tari base node",
            hash.to_hex()
        );

        let mut client = self.connect_grpc_client().await?;
        let grpc::BlockHeaderResponse {
            header,
            reward,
            confirmations,
            difficulty,
            num_transactions,
        } = client
            .get_header_by_hash(grpc::GetHeaderByHashRequest { hash })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get header by hash".to_string(),
            })?
            .into_inner();
        let header = header.expect("Base node GRPC returned an empty header field when calling get_header_by_hash");

        let json_resp = json_rpc::success_response(
            request["id"].as_i64(),
            json!({
                "block_header": {
                   "block_size": 0, // TODO: block weight?
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
                   "timestamp": header.timestamp.map(|ts| ts.seconds.into()).unwrap_or_else(|| json!(null)),
               }
            }),
        );

        let json_resp = append_aux_chain_data(json_resp, json!({ "id": TARI_CHAIN_ID }));

        Ok(proxy::into_response(parts, &json_resp))
    }

    async fn connect_grpc_client(
        &self,
    ) -> Result<grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>, MmProxyError> {
        let client =
            grpc::base_node_client::BaseNodeClient::connect(format!("http://{}", self.config.grpc_base_node_address))
                .await?;
        Ok(client)
    }

    async fn connect_grpc_wallet_client(
        &self,
    ) -> Result<grpc::wallet_client::WalletClient<tonic::transport::Channel>, MmProxyError> {
        let client =
            grpc::wallet_client::WalletClient::connect(format!("http://{}", self.config.grpc_console_wallet_address))
                .await?;
        Ok(client)
    }

    fn get_fully_qualified_monerod_url(&self, uri: &Uri) -> Result<Url, MmProxyError> {
        let uri = format!("{}{}", self.config.monerod_url, uri.path()).parse::<Url>()?;
        Ok(uri)
    }

    /// Proxy a request received by this server to Monerod
    async fn proxy_request_to_monerod(
        &self,
        mut req: Request<Body>,
    ) -> Result<(Request<Bytes>, Response<json::Value>), MmProxyError>
    {
        let monerod_uri = self.get_fully_qualified_monerod_url(req.uri())?;
        let bytes = proxy::read_body_until_end(req.body_mut()).await?;
        let request = req.map(|_| bytes.freeze());

        let mut builder = self
            .http_client
            .request(request.method().clone(), monerod_uri.clone())
            .headers(request.headers().clone());

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
        let json_response = if self.config.proxy_submit_to_origin {
            let resp = builder
                // This is a cheap clone of the request body
                .body(request.body().clone())
                .send()
                .await
                .map_err(MmProxyError::MonerodRequestFailed)?;
            convert_reqwest_response_to_hyper_json_response(resp).await?
        } else {
            let json = json::from_slice::<json::Value>(&request.body()[..])?;
            match json["method"].as_str() {
                Some("submitblock") | Some("submit_block") => {
                    debug!(
                        target: LOG_TARGET,
                        "[monerod] skip: Proxy configured for self-select mode. Pool will submit to MoneroD, \
                         submitting to Tari.",
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
                    proxy::convert_json_to_hyper_json_response(accept_response, StatusCode::OK, monerod_uri.clone())
                        .await?
                },
                _ => {
                    let resp = builder
                        // This is a cheap clone of the request body
                        .body(request.body().clone())
                        .send()
                        .await
                        .map_err(MmProxyError::MonerodRequestFailed)?;
                    convert_reqwest_response_to_hyper_json_response(resp).await?
                },
            }
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
        &mut self,
        request: Request<Bytes>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError>
    {
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
                    _ => Ok(proxy::into_body_from_response(monerod_resp)),
                }
            },
            // Simply return the response "as is"
            _ => Ok(proxy::into_body_from_response(monerod_resp)),
        }
    }

    async fn handle(mut self, request: Request<Body>) -> Result<Response<Body>, MmProxyError> {
        debug!(target: LOG_TARGET, "Got request: {}", request.uri());
        debug!(target: LOG_TARGET, "Request headers: {:?}", request.headers());

        let (request, monerod_resp) = self.proxy_request_to_monerod(request).await?;
        // Any failed (!= 200 OK) responses from Monero are immediately returned to the requester
        if !monerod_resp.status().is_success() {
            // we dont break on xmrig returned error.
            warn!(
                target: LOG_TARGET,
                "Monerod returned an error: {}",
                monerod_resp.status()
            );
            return Ok(monerod_resp.map(|json| json.to_string().into()));
        }

        let response = self.get_proxy_response(request, monerod_resp).await?;
        Ok(response)
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

pub fn append_aux_chain_data(mut response: json::Value, chain_data: json::Value) -> json::Value {
    if response["result"].is_null() {
        return response;
    }
    let chains = match response["result"][MMPROXY_AUX_KEY_NAME]["chains"].as_array_mut() {
        Some(arr_mut) => arr_mut,
        None => {
            response["result"][MMPROXY_AUX_KEY_NAME]["chains"] = json!([]);
            response["result"][MMPROXY_AUX_KEY_NAME]["chains"]
                .as_array_mut()
                .unwrap()
        },
    };

    chains.push(chain_data);
    response
}
