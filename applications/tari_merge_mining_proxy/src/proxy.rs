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
    error::MmProxyError,
    helpers,
    helpers::default_accept,
};
use bytes::BytesMut;
use futures::StreamExt;
use hyper::{
    body::Bytes,
    http::{header, response::Parts, HeaderValue},
    service::Service,
    Body,
    Method,
    Request,
    Response,
    StatusCode,
    Uri,
    Version,
};
use jsonrpc::error::StandardError;
use reqwest::{ResponseBuilderExt, Url};
use serde_json as json;
use std::{
    cmp,
    cmp::min,
    convert::TryFrom,
    future::Future,
    net::SocketAddr,
    task::{Context, Poll},
};
use tari_app_grpc::{tari_rpc as grpc, tari_rpc::GetCoinbaseRequest};
use tari_common::{GlobalConfig, Network};
use tari_core::{blocks::NewBlockTemplate, proof_of_work::monero_rx};
use tracing::{debug, error, info, instrument, trace, warn};

pub const LOG_TARGET: &str = "tari_mm_proxy::xmrig";

#[derive(Debug, Clone)]
pub struct MergeMiningProxyConfig {
    pub network: Network,
    pub monerod_url: String,
    pub monerod_username: String,
    pub monerod_password: String,
    pub monerod_use_auth: bool,
    pub grpc_address: SocketAddr,
    pub grpc_wallet_address: SocketAddr,
}

impl From<GlobalConfig> for MergeMiningProxyConfig {
    fn from(config: GlobalConfig) -> Self {
        Self {
            network: config.network,
            monerod_url: config.monerod_url,
            monerod_username: config.monerod_username,
            monerod_password: config.monerod_password,
            monerod_use_auth: config.monerod_use_auth,
            grpc_address: config.grpc_address,
            grpc_wallet_address: config.grpc_wallet_address,
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
            },
        }
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

                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(standard_rpc_error(StandardError::InternalError, None))
                        .unwrap())
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
            .into_inner()
            .metadata
            .map(|meta| meta.height_of_longest_chain)
            .ok_or_else(|| MmProxyError::GrpcResponseMissingField("metadata"))?;
        debug!(
            target: LOG_TARGET,
            "Monero height = {}, Tari base node height = {}", json["height"], height
        );

        json["height"] = json::json!(cmp::max(json["height"].as_i64().unwrap_or_default(), height as i64));

        Ok(into_body(parts, json))
    }

    async fn handle_submit_block(
        &mut self,
        request: Request<json::Value>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError>
    {
        let resp = monerod_resp.body();
        if resp["result"]["status"] != "OK" {
            return Err(MmProxyError::InvalidMonerodResponse(format!(
                "Response status failed: {:#}",
                resp["result"]
            )));
        }

        let params = match request.body()["params"].as_array() {
            Some(v) => v,
            None => {
                return Ok(Response::builder()
                    .body(standard_rpc_error(
                        StandardError::InvalidParams,
                        Some(
                            "`params` field is empty or an invalid type for submit block request. Expected an array."
                                .into(),
                        ),
                    ))
                    .unwrap())
            },
        };

        for param in params.iter().map(|p| p.as_str()).filter_map(|p| p) {
            let monero_block = helpers::deserialize_monero_block_from_hex(param)?;
            let hash = match helpers::extract_tari_hash(&monero_block) {
                Some(h) => *h,
                None => {
                    return Err(MmProxyError::MissingDataError(
                        "Could not find Tari header in coinbase".to_string(),
                    ))
                },
            };

            if let Some(mut block_data) = self.block_templates.get(&hash).await {
                let monero_data = helpers::construct_monero_data(monero_block, block_data.monero_seed)?;

                let pow_data = bincode::serialize(&monero_data)?;
                let h = block_data.tari_block.header.as_mut().unwrap();
                let p = h.pow.as_mut().unwrap();
                p.pow_data = pow_data;

                let mut base_node_client = self.connect_grpc_client().await?;
                base_node_client
                    .submit_block(block_data.tari_block)
                    .await
                    .map_err(|status| MmProxyError::GrpcRequestError {
                        status,
                        details: "failed to submit block".to_string(),
                    })?;

                self.block_templates.remove(&hash).await;
            } else {
                info!(
                    target: LOG_TARGET,
                    "Block submitted but no matching block template was found"
                );
            }
        }
        // Return the Monero response as is
        let (parts, json) = monerod_resp.into_parts();
        Ok(into_body(parts, json))
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
        let new_block_template_response = grpc_client
            .get_new_block_template(grpc::PowAlgo {
                pow_algo: grpc::pow_algo::PowAlgos::Monero.into(),
            })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?;

        let new_block_template_response = new_block_template_response.into_inner();

        let new_block_template_reward = new_block_template_response.block_reward;
        let new_block_template = new_block_template_response
            .new_block_template
            .ok_or_else(|| MmProxyError::GrpcResponseMissingField("new_block_template"))?;
        info!(
            target: LOG_TARGET,
            "Received new block template from Tari base node for height #{}",
            new_block_template.header.as_ref().map(|h| h.height).unwrap_or_default(),
        );

        let template_block = NewBlockTemplate::try_from(new_block_template.clone())
            .map_err(|e| MmProxyError::MissingDataError(format!("GRPC Conversion Error: {}", e)))?;

        let mut grpc_wallet_client = self.connect_grpc_wallet_client().await?;
        let coinbase_response = grpc_wallet_client
            .get_coinbase(GetCoinbaseRequest {
                reward: new_block_template_reward,
                fee: u64::from(template_block.body.get_total_fee()),
                height: template_block.header.height,
            })
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?;
        let coinbase_transaction = coinbase_response.into_inner().transaction;

        let coinbased_block = helpers::add_coinbase(coinbase_transaction, template_block)?;
        debug!(target: LOG_TARGET, "Added coinbase to new block template");
        let block = grpc_client
            .get_new_block(coinbased_block)
            .await
            .map_err(|status| MmProxyError::GrpcRequestError {
                status,
                details: "failed to get new block".to_string(),
            })?
            .into_inner();

        debug!(target: LOG_TARGET, "New block received from Tari: {:?}", block);
        let mining_data = block
            .clone()
            .miner_data
            .ok_or_else(|| MmProxyError::GrpcResponseMissingField("mining_data"))?;

        let block_data = BlockTemplateDataBuilder::default();
        let block_data = block_data
            .tari_block(
                block
                    .block
                    .ok_or_else(|| MmProxyError::GrpcResponseMissingField("block"))?,
            )
            .tari_miner_data(mining_data.clone());

        // Deserialize the block template blob
        let block_template_blob = &monerod_resp["result"]["blocktemplate_blob"]
            .to_string()
            .replace("\"", "");
        debug!(target: LOG_TARGET, "Deserializing Blocktemplate Blob into Monero Block",);
        let mut monero_block = helpers::deserialize_monero_block_from_hex(block_template_blob)?;

        debug!(target: LOG_TARGET, "Appending Merged Mining Tag",);
        // Add the Tari merge mining tag to the retrieved block template
        monero_rx::append_merge_mining_tag(&mut monero_block, mining_data.merge_mining_hash.as_slice())?;

        debug!(target: LOG_TARGET, "Creating blockhashing blob from blocktemplate blob",);
        // Must be done after the tag is inserted since it will affect the hash of the miner tx
        let blockhashing_blob = monero_rx::create_blockhashing_blob(&monero_block)?;

        debug!(target: LOG_TARGET, "blockhashing_blob:{}", blockhashing_blob);
        monerod_resp["result"]["blockhashing_blob"] = blockhashing_blob.into();

        let blocktemplate_blob = helpers::serialize_monero_block_to_hex(&monero_block)?;
        debug!(target: LOG_TARGET, "blocktemplate_blob:{}", block_template_blob);
        monerod_resp["result"]["blocktemplate_blob"] = blocktemplate_blob.into();

        let seed = monerod_resp["result"]["seed_hash"].to_string().replace("\"", "");

        let block_data = block_data.monero_seed(seed);

        let monero_difficulty: u64 = monerod_resp["result"]["difficulty"].as_u64().unwrap_or_default();
        let tari_difficulty = mining_data.target_difficulty;

        let mining_difficulty = min(monero_difficulty, tari_difficulty);

        let block_data = block_data
            .monero_difficulty(monero_difficulty)
            .tari_difficulty(tari_difficulty);

        self.block_templates
            .save(mining_data.merge_mining_hash, block_data.build()?)
            .await;
        info!(
            target: LOG_TARGET,
            "Difficulties: Tari ({}), Monero({}),Selected({})", tari_difficulty, monero_difficulty, mining_difficulty
        );
        monerod_resp["result"]["difficulty"] = mining_difficulty.into();

        debug!(target: LOG_TARGET, "Returning template result: {}", monerod_resp);
        Ok(into_body(parts, monerod_resp))
    }

    async fn connect_grpc_client(
        &self,
    ) -> Result<grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>, MmProxyError> {
        let client =
            grpc::base_node_client::BaseNodeClient::connect(format!("http://{}", self.config.grpc_address)).await?;
        Ok(client)
    }

    async fn connect_grpc_wallet_client(
        &self,
    ) -> Result<grpc::wallet_client::WalletClient<tonic::transport::Channel>, MmProxyError> {
        let client =
            grpc::wallet_client::WalletClient::connect(format!("http://{}", self.config.grpc_wallet_address)).await?;
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
        let bytes = read_body_until_end(req.body_mut()).await?;
        let request = req.map(|_| bytes.freeze());

        let mut should_proxy = true;
        let mut submit_block = false;
        let body: Bytes = request.body().clone();
        let json = json::from_slice::<json::Value>(&body[..]).unwrap_or_default();

        if json["method"] == "submitblock" || json["method"] == "submit_block" {
            submit_block = true;

            // TODO: determine if achieved difficulty from solution matches monero's difficulty and submit it

            // if transient.current_difficulty.unwrap_or_default() < transient.monero_difficulty.unwrap_or_default()
            // {
            //      debug!(target: LOG_TARGET, "json: {:#}", json);
            //
            //      let params = &json["params"][0];
            //      let params_string = params.to_string().replace("\"", "");
            //
            //      debug!(target: LOG_TARGET, "param_string: {:#}", params_string);
            //
            //     let monero_block = helpers::deserialize_monero_block_from_hex(params_string)?;
            //     debug!(target: LOG_TARGET, "monero_block: {:?}", monero_block);
            //
            //     let hash = helpers::extract_tari_hash(&monero_block).ok_or_else(||
            // MmProxyError::MissingDataError("Could not find Tari hash in Monero block".to_string()))?;
            //
            should_proxy = false;
            // }
        }

        if should_proxy {
            debug!(
                target: LOG_TARGET,
                "Proxying request: {} {} {}",
                request.method(),
                monerod_uri,
                json["method"]
            );
            let mut builder = self
                .http_client
                .request(request.method().clone(), monerod_uri)
                .headers(request.headers().clone());

            if self.config.monerod_use_auth {
                // Use HTTP basic auth. This is the only reason we are using `reqwest` over the standard hyper client.
                builder = builder.basic_auth(&self.config.monerod_username, Some(&self.config.monerod_password));
            }

            let resp = builder
                // This is a cheap clone of the request body
                .body(request.body().clone())
                .send()
                .await
                .map_err(MmProxyError::MonerodRequestFailed)?;
            let json_response = convert_reqwest_response_to_hyper_json_response(resp).await?;

            if submit_block {
                debug!(target: LOG_TARGET, "Submit response {}", json_response.body());
            } else {
                debug!(target: LOG_TARGET, "Received response {}", json_response.body());
            }
            Ok((request, json_response))
        } else {
            debug!(
                target: LOG_TARGET,
                "Non-proxying request: {} {} {}",
                request.method(),
                monerod_uri,
                json["method"]
            );
            let json_response = convert_json_to_hyper_json_response(default_accept(&json), monerod_uri).await?;
            debug!(target: LOG_TARGET, "Received response: {}", json);
            if submit_block {
                debug!(target: LOG_TARGET, "Submit response {}", json_response.body());
            } else {
                debug!(target: LOG_TARGET, "Received response {}", json_response.body());
            }
            Ok((request, json_response))
        }
    }

    async fn get_proxy_response(
        &mut self,
        request: Request<Bytes>,
        monerod_resp: Response<json::Value>,
    ) -> Result<Response<Body>, MmProxyError>
    {
        match request.method().clone() {
            Method::GET => {
                // Map requests to monerod requests (with aliases)
                match request.uri().path() {
                    "/get_height" | "/getheight" => self.handle_get_height(monerod_resp).await,
                    _ => Ok(into_body_from_response(monerod_resp)),
                }
            },
            Method::POST => {
                // Try parse the request into JSON, it is allowed fail because if we've made it this far, then monerod
                // accepted the request
                let json = json::from_slice::<json::Value>(request.body())?;
                let json_method = json.clone();
                let request = request.map(move |_| json);
                // All post requests go to /json_rpc, body of request contains a field `method` to indicate which call
                // takes place.
                match json_method["method"].as_str().unwrap_or_default() {
                    "submitblock" | "submit_block" => self.handle_submit_block(request, monerod_resp).await,
                    "getblocktemplate" | "get_block_template" => self.handle_get_block_template(monerod_resp).await,
                    _ => Ok(into_body_from_response(monerod_resp)),
                }
            },
            // Simply return the response "as is"
            _ => Ok(into_body_from_response(monerod_resp)),
        }
    }

    async fn handle(mut self, request: Request<Body>) -> Result<Response<Body>, MmProxyError> {
        debug!(target: LOG_TARGET, "Got request: {}", request.uri());
        debug!(target: LOG_TARGET, "Request headers: {:?}", request.headers());
        let (request, monerod_resp) = self.proxy_request_to_monerod(request).await?;
        // Any failed (!= 200 OK) responses from Monero are immediately returned to the requester
        if !monerod_resp.status().is_success() {
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

fn standard_rpc_error(err: jsonrpc::error::StandardError, data: Option<json::Value>) -> Body {
    // TODO: jsonrpc's API is not particularly ergonomic
    json::to_string(&jsonrpc::error::result_to_response(
        Err(jsonrpc::error::standard_error(err, data)),
        json::Value::from(-1i32),
    ))
    .expect("jsonrpc's serialization implementation is expected to always succeed")
    .into()
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

async fn convert_json_to_hyper_json_response(
    resp: json::Value,
    url: Url,
) -> Result<Response<json::Value>, MmProxyError>
{
    let mut builder = Response::builder();

    let headers = builder
        .headers_mut()
        .expect("headers_mut errors only when the builder has an error (e.g invalid header value)");
    headers.append("Content-Type", HeaderValue::from_str("application/json").unwrap());

    builder = builder.version(Version::HTTP_11).status(StatusCode::OK).url(url);

    let body = resp;
    let resp = builder.body(body)?;
    Ok(resp)
}

fn into_body<T: ToString>(mut parts: Parts, content: T) -> Response<Body> {
    let resp = content.to_string();
    // Ensure that the content length header is correct
    parts.headers.insert(header::CONTENT_LENGTH, resp.len().into());
    Response::from_parts(parts, resp.into())
}

fn into_body_from_response<T: ToString>(resp: Response<T>) -> Response<Body> {
    let (parts, body) = resp.into_parts();
    into_body(parts, body)
}

/// Reads the `Body` until there is no more to read
pub(super) async fn read_body_until_end(body: &mut Body) -> Result<BytesMut, MmProxyError> {
    // TOOD: Perhaps there is a more efficient way to do this
    let mut bytes = BytesMut::new();
    while let Some(data) = body.next().await {
        let data = data?;
        bytes.extend(data);
    }
    Ok(bytes)
}
