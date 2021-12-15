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
    convert::TryFrom,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use bytes::Bytes;
use hyper::{service::Service, Body, Method, Request, Response, StatusCode};
use json::json;
use jsonrpc::error::StandardError;
use serde_json as json;
use tari_app_grpc::{tari_rpc as grpc, tari_rpc::GetCoinbaseRequest};
use tari_common::{configuration::Network, GlobalConfig};
use tari_core::blocks::{Block, NewBlockTemplate};
use tari_utilities::{hex::Hex, message_format::MessageFormat};
use tracing::{debug, error};

use crate::{
    common::{
        json_rpc,
        json_rpc::{standard_error_response, try_into_json_block_header_response},
        mining,
        proxy,
    },
    error::StratumTranscoderProxyError,
};

const LOG_TARGET: &str = "tari_stratum_transcoder::transcoder";

#[derive(Debug, Clone)]
pub struct StratumTranscoderProxyConfig {
    pub network: Network,
    pub grpc_base_node_address: SocketAddr,
    pub grpc_console_wallet_address: SocketAddr,
    pub transcoder_host_address: SocketAddr,
}

impl TryFrom<GlobalConfig> for StratumTranscoderProxyConfig {
    type Error = std::io::Error;

    fn try_from(_config: GlobalConfig) -> Result<Self, Self::Error> {
        todo!("fix")
        // let grpc_base_node_address = multiaddr_to_socketaddr(&config.grpc_base_node_address)?;
        // let grpc_console_wallet_address = multiaddr_to_socketaddr(&config.grpc_console_wallet_address)?;
        // Ok(Self {
        //     network: config.network,
        //     grpc_base_node_address,
        //     grpc_console_wallet_address,
        //     transcoder_host_address: config.transcoder_host_address,
        // })
    }
}

#[derive(Debug, Clone)]
pub struct StratumTranscoderProxyService {
    inner: InnerService,
}

impl StratumTranscoderProxyService {
    pub fn new(
        config: StratumTranscoderProxyConfig,
        http_client: reqwest::Client,
        base_node_client: grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>,
        wallet_client: grpc::wallet_client::WalletClient<tonic::transport::Channel>,
    ) -> Self {
        Self {
            inner: InnerService {
                _config: config,
                _http_client: http_client,
                base_node_client,
                wallet_client,
            },
        }
    }
}

#[allow(clippy::type_complexity)]
impl Service<Request<Body>> for StratumTranscoderProxyService {
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Response<Body>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let inner = self.inner.clone();
        let future = async move {
            match inner.handle(req).await {
                Ok(resp) => Ok(resp),
                Err(err) => {
                    error!(target: LOG_TARGET, "Error handling request: {}", err);

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
    _config: StratumTranscoderProxyConfig,
    _http_client: reqwest::Client,
    base_node_client: grpc::base_node_client::BaseNodeClient<tonic::transport::Channel>,
    wallet_client: grpc::wallet_client::WalletClient<tonic::transport::Channel>,
}

impl InnerService {
    async fn handle_get_info(&self) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let mut client = self.base_node_client.clone();
        let tip_info = client.get_tip_info(grpc::Empty {}).await?.into_inner();
        let consensus_constants = client.get_constants(grpc::Empty {}).await?.into_inner();
        let sync_info = client.get_sync_info(grpc::Empty {}).await?.into_inner();
        let info_json;
        match tip_info.metadata {
            Some(metadata) => {
                info_json = json!({
                "jsonrpc": "2.0",
                "result": {
                    "blockchain_version": consensus_constants.blockchain_version,
                    "min_diff": consensus_constants.min_blake_pow_difficulty,
                    "lock_height": consensus_constants.coinbase_lock_height,
                    "max_block_interval": consensus_constants.difficulty_max_block_interval,
                    "max_weight": consensus_constants.max_block_transaction_weight,
                    "height_of_longest_chain": metadata.height_of_longest_chain,
                    "best_block": metadata.best_block.to_hex(),
                    "local_height": sync_info.local_height,
                    "tip_height": sync_info.tip_height,
                    "initial_sync_achieved": tip_info.initial_sync_achieved,
                    }
                })
            },
            None => {
                return Err(StratumTranscoderProxyError::UnexpectedTariBaseNodeResponse(
                    "Base node GRPC returned empty metadata when calling tip_info".into(),
                ))
            },
        }
        proxy::json_response(StatusCode::OK, &info_json)
    }

    async fn handle_get_block_template(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.body();
        let request_id = request["id"].as_i64();
        let mut grpc_client = self.base_node_client.clone();

        let grpc::NewBlockTemplateResponse {
            miner_data,
            new_block_template,
            initial_sync_achieved: _,
        } = grpc_client
            .get_new_block_template(grpc::NewBlockTemplateRequest {
                algo: Some(grpc::PowAlgo {
                    pow_algo: grpc::pow_algo::PowAlgos::Sha3.into(),
                }),
                max_weight: 0,
            })
            .await
            .map_err(|status| StratumTranscoderProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?
            .into_inner();

        let miner_data = miner_data.ok_or(StratumTranscoderProxyError::GrpcResponseMissingField("miner_data"))?;
        let new_block_template = new_block_template.ok_or(StratumTranscoderProxyError::GrpcResponseMissingField(
            "new_block_template",
        ))?;

        let block_reward = miner_data.reward;
        let total_fees = miner_data.total_fees;
        let tari_difficulty = miner_data.target_difficulty;

        let template_block = NewBlockTemplate::try_from(new_block_template)
            .map_err(|e| StratumTranscoderProxyError::MissingDataError(format!("GRPC Conversion Error: {}", e)))?;
        let tari_height = template_block.header.height;

        let mut grpc_wallet_client = self.wallet_client.clone();
        let coinbase_response = grpc_wallet_client
            .get_coinbase(GetCoinbaseRequest {
                reward: block_reward,
                fee: total_fees,
                height: tari_height,
            })
            .await
            .map_err(|status| StratumTranscoderProxyError::GrpcRequestError {
                status,
                details: "failed to get new block template".to_string(),
            })?;
        let coinbase_transaction = coinbase_response.into_inner().transaction;

        let coinbased_block = mining::add_coinbase(coinbase_transaction, template_block)?;

        let block = grpc_client
            .get_new_block(coinbased_block)
            .await
            .map_err(|status| StratumTranscoderProxyError::GrpcRequestError {
                status,
                details: "failed to get new block".to_string(),
            })?
            .into_inner();

        let tari_block = Block::try_from(
            block
                .block
                .ok_or_else(|| StratumTranscoderProxyError::MissingDataError("Tari block".to_string()))?,
        )
        .map_err(StratumTranscoderProxyError::MissingDataError)?;

        let tari_header = tari_block.header.clone();
        let tari_prev_hash = tari_header.prev_hash.to_hex();

        // todo remove unwraps
        let header_hex = hex::encode(tari_header.to_json().unwrap());
        let block_hex = hex::encode(tari_block.to_json().unwrap());

        let template_json = json!({
            "id": request_id.unwrap_or(-1),
            "jsonrpc": "2.0",
            "result": {
                "blockheader_blob": header_hex,
                "blocktemplate_blob": block_hex,
                "difficulty" : tari_difficulty,
                "height" : tari_height,
                "expected_reward": block_reward+total_fees,
                "prev_hash": tari_prev_hash,
            }
        });

        proxy::json_response(StatusCode::OK, &template_json)
    }

    async fn handle_submit_block(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.body();
        let params = match request["params"].as_array() {
            Some(v) => v,
            None => {
                return proxy::json_response(
                    StatusCode::OK,
                    &json_rpc::error_response(
                        request["id"].as_i64(),
                        1,
                        "`params` field is empty or an invalid type for submit block request. Expected an array.",
                        None,
                    ),
                )
            },
        };
        let mut json_response: Result<Response<Body>, StratumTranscoderProxyError> = proxy::json_response(
            StatusCode::OK,
            &json_rpc::error_response(request["id"].as_i64(), 2, "No block", None),
        );
        for param in params.iter().filter_map(|p| p.as_str()) {
            let block_hex = hex::decode(param);
            match block_hex {
                Ok(block_hex) => {
                    let block: Result<Block, serde_json::Error> =
                        serde_json::from_str(&String::from_utf8_lossy(&block_hex).to_string());
                    match block {
                        Ok(block) => {
                            let mut client = self.base_node_client.clone();
                            let grpc_block: tari_app_grpc::tari_rpc::Block = block.into();
                            match client.submit_block(grpc_block).await {
                                Ok(_) => {
                                    json_response = proxy::json_response(
                                        StatusCode::OK,
                                        &json_rpc::success_response(
                                            request["id"].as_i64(),
                                            json!({ "status": "OK", "untrusted": false }),
                                        ),
                                    )
                                },
                                Err(_) => {
                                    json_response = proxy::json_response(
                                        StatusCode::OK,
                                        &json_rpc::error_response(
                                            request["id"].as_i64(),
                                            3,
                                            "Block not accepted",
                                            None,
                                        ),
                                    )
                                },
                            }
                        },
                        Err(_) => {
                            json_response = proxy::json_response(
                                StatusCode::OK,
                                &json_rpc::error_response(request["id"].as_i64(), 4, "Invalid Block", None),
                            )
                        },
                    }
                },
                Err(_) => {
                    json_response = proxy::json_response(
                        StatusCode::OK,
                        &json_rpc::error_response(request["id"].as_i64(), 5, "Invalid Hex", None),
                    )
                },
            }
        }
        json_response
    }

    async fn handle_get_block_header_by_height(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.into_body();
        let mut height = request["params"]["height"].as_u64().unwrap_or(0);
        // bug for height = 0 (genesis block), streams indefinitely
        if height == 0 {
            height = 1;
        }
        let mut client = self.base_node_client.clone();
        let mut resp = client
            .get_blocks(grpc::GetBlocksRequest { heights: vec![height] })
            .await?
            .into_inner();
        let message = resp.message().await?;
        resp.trailers().await?; // drain stream
                                // todo: remove unwraps
        let resp = client
            .get_header_by_hash(grpc::GetHeaderByHashRequest {
                hash: message.unwrap().block.unwrap().header.unwrap().hash,
            })
            .await;
        match resp {
            Ok(resp) => {
                let json_response = try_into_json_block_header_response(resp.into_inner(), request["id"].as_i64())?;
                proxy::json_response(StatusCode::OK, &json_response)
            },
            Err(err) if err.code() == tonic::Code::NotFound => proxy::json_response(
                StatusCode::OK,
                &json_rpc::error_response(request["id"].as_i64(), 5, "Not found", None),
            ),
            Err(err) => Err(StratumTranscoderProxyError::GrpcRequestError {
                status: err,
                details: "failed to get header by height".to_string(),
            }),
        }
    }

    async fn handle_get_block_header_by_hash(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
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
                    &json_rpc::error_response(request["id"].as_i64(), -1, err, None),
                )
            },
        };

        let mut client = self.base_node_client.clone();
        let resp = client
            .get_header_by_hash(grpc::GetHeaderByHashRequest { hash: hash.clone() })
            .await;
        match resp {
            Ok(resp) => {
                let json_response = try_into_json_block_header_response(resp.into_inner(), request["id"].as_i64())?;

                debug!(
                    target: LOG_TARGET,
                    "[get_header_by_hash] Found tari block header with hash `{:?}`",
                    hash.clone()
                );

                proxy::json_response(StatusCode::OK, &json_response)
            },
            Err(err) if err.code() == tonic::Code::NotFound => {
                debug!(
                    target: LOG_TARGET,
                    "[get_header_by_hash] No tari block header found with hash `{:?}`", hash
                );
                proxy::json_response(
                    StatusCode::OK,
                    &json_rpc::error_response(request["id"].as_i64(), 5, "Not found", None),
                )
            },
            Err(err) => Err(StratumTranscoderProxyError::GrpcRequestError {
                status: err,
                details: "failed to get header by hash".to_string(),
            }),
        }
    }

    async fn handle_get_last_block_header(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.into_body();
        let mut client = self.base_node_client.clone();
        let tip_info = client.get_tip_info(grpc::Empty {}).await?;
        let tip_info = tip_info.into_inner();
        let chain_metadata = tip_info.metadata.ok_or_else(|| {
            StratumTranscoderProxyError::UnexpectedTariBaseNodeResponse(
                "get_tip_info returned no chain metadata".into(),
            )
        })?;

        let tip_header = client
            .get_header_by_hash(grpc::GetHeaderByHashRequest {
                hash: chain_metadata.best_block,
            })
            .await?;

        let tip_header = tip_header.into_inner();
        let json_response = try_into_json_block_header_response(tip_header, request["id"].as_i64())?;
        proxy::json_response(StatusCode::OK, &json_response)
    }

    async fn handle_get_balance(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.body();
        let request_id = request["id"].as_i64();
        let mut client = self.wallet_client.clone();
        let balances = client.get_balance(grpc::GetBalanceRequest {}).await?.into_inner();

        let json_response = json!({
             "id": request_id.unwrap_or(-1),
            "jsonrpc": "2.0",
            "result": {
                "available_balance": balances.available_balance,
                "pending_incoming_balance": balances.pending_incoming_balance,
                "pending_outgoing_balance": balances.pending_outgoing_balance,
            }
        });
        proxy::json_response(StatusCode::OK, &json_response)
    }

    async fn handle_get_fee(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.body();
        let transactions = match request["params"]["transactions"].as_array() {
            Some(v) => v,
            None => {
                return proxy::json_response(
                    StatusCode::OK,
                    &json_rpc::error_response(
                        request["id"].as_i64(),
                        1,
                        "`transactions` field is empty or an invalid type for transfer request.",
                        None,
                    ),
                )
            },
        };

        let mut grpc_transaction_info = Vec::new();
        for transaction in transactions.iter() {
            grpc_transaction_info.push(
                transaction["transaction_id"]
                    .as_str()
                    .unwrap()
                    .to_string()
                    .parse::<u64>()
                    .unwrap(),
            );
        }

        let mut client = self.wallet_client.clone();

        let transaction_info_results = client
            .get_transaction_info(grpc::GetTransactionInfoRequest {
                transaction_ids: grpc_transaction_info,
            })
            .await?
            .into_inner();
        let info_results = &transaction_info_results.transactions;

        let mut results = Vec::new();
        for info_result in info_results.iter() {
            let result = json!({
                "transaction_id":  info_result.tx_id,
                "fee": info_result.fee,
            });
            results.push(result.as_object().unwrap().clone());
        }

        let json_response = json!({
            "jsonrpc": "2.0",
            "result": {"fee_results" : results},
        });
        proxy::json_response(StatusCode::OK, &json_response)
    }

    async fn handle_transfer(
        &self,
        request: Request<json::Value>,
    ) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let request = request.body();
        let recipients = match request["params"]["recipients"].as_array() {
            Some(v) => v,
            None => {
                return proxy::json_response(
                    StatusCode::OK,
                    &json_rpc::error_response(
                        request["id"].as_i64(),
                        1,
                        "`recipients` field is empty or an invalid type for transfer request. Expected an array.",
                        None,
                    ),
                )
            },
        };

        let mut grpc_payments = Vec::new();

        let mut client = self.wallet_client.clone();
        let whoami_info = client.identify(grpc::GetIdentityRequest {}).await?.into_inner();
        let address = String::from_utf8_lossy(&whoami_info.public_key).to_string();
        let mut payment_to_self = false;
        for recipient in recipients.iter() {
            // One-sided transactions are not supported to paying yourself and it is also a waste to do so since you
            // will be paying fees for the transaction.
            if recipient["address"] == address {
                payment_to_self = true;
                continue;
            }
            grpc_payments.push(grpc::PaymentRecipient {
                address: recipient["address"].as_str().unwrap().to_string(),
                amount: recipient["amount"].as_u64().unwrap(),
                fee_per_gram: recipient["fee_per_gram"].as_u64().unwrap(),
                message: recipient["message"].as_str().unwrap().to_string(),
                payment_type: 1,
            });
        }

        let transfer_results = client
            .transfer(grpc::TransferRequest {
                recipients: grpc_payments,
            })
            .await?
            .into_inner();
        let transaction_results = &transfer_results.results;

        let mut results = Vec::new();
        for transaction_result in transaction_results.iter() {
            let result = json!({
                "address": transaction_result.address,
                "transaction_id": transaction_result.transaction_id,
                "is_success": transaction_result.is_success,
                "failure_message": transaction_result.failure_message,
            });
            results.push(result.as_object().unwrap().clone());
        }

        // Return success for payment to self, transaction ID of zero since payment wasn't
        // needed to be made, however is still needed to be returned so balances can be updated
        // for payments (this will usually be payouts of amounts due to the pool itself using
        // the same wallet address for both its' funds as well as the mining rewards).
        // Possibly a better alternative for this is to do an interactive payment to self?
        if payment_to_self {
            let result = json!({
                "address": address,
                "transaction_id": 0,
                "is_success": true,
                "failure_message": "",
            });
            results.push(result.as_object().unwrap().clone());
        }

        let json_response = json!({
            "jsonrpc": "2.0",
            "result": {"transaction_results" : results},
        });
        proxy::json_response(StatusCode::OK, &json_response)
    }

    async fn get_proxy_response(&self, request: Request<Bytes>) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let mut proxy_resp = Response::new(standard_error_response(Some(-1), StandardError::MethodNotFound, None));
        match request.method().clone() {
            Method::GET => match request.uri().path() {
                "/get_info" | "/getinfo" => self.handle_get_info().await,
                _ => Ok(proxy::into_body_from_response(proxy_resp)),
            },
            Method::POST => {
                let json = json::from_slice::<json::Value>(request.body())?;
                let request = request.map(move |_| json);
                match request.body()["method"].as_str().unwrap_or_default() {
                    "get_info" | "getinfo" => self.handle_get_info().await,
                    "submitblock" | "submit_block" => self.handle_submit_block(request).await,
                    "getblocktemplate" | "get_block_template" => self.handle_get_block_template(request).await,
                    "getblockheaderbyhash" | "get_block_header_by_hash" => {
                        self.handle_get_block_header_by_hash(request).await
                    },
                    "getblockheaderbyheight" | "get_block_header_by_height" => {
                        self.handle_get_block_header_by_height(request).await
                    },
                    "getlastblockheader" | "get_last_block_header" => self.handle_get_last_block_header(request).await,
                    "transfer" => self.handle_transfer(request).await,
                    "getbalance" | "get_balance" => self.handle_get_balance(request).await,
                    "getfee" | "get_fee" => self.handle_get_fee(request).await,
                    _ => {
                        let request = request.body();
                        proxy_resp = Response::new(standard_error_response(
                            request["id"].as_i64(),
                            StandardError::MethodNotFound,
                            None,
                        ));
                        Ok(proxy::into_body_from_response(proxy_resp))
                    },
                }
            },
            // Simply return the response "as is"
            _ => Ok(proxy::into_body_from_response(proxy_resp)),
        }
    }

    async fn handle(self, mut request: Request<Body>) -> Result<Response<Body>, StratumTranscoderProxyError> {
        let start = Instant::now();
        let bytes = proxy::read_body_until_end(request.body_mut()).await?;
        let request = request.map(|_| bytes.freeze());
        let method_name;
        match *request.method() {
            Method::GET => {
                let mut chars = request.uri().path().chars();
                chars.next();
                method_name = chars.as_str().to_string();
            },
            Method::POST => {
                let json = json::from_slice::<json::Value>(request.body()).unwrap_or_default();
                method_name = str::replace(json["method"].as_str().unwrap_or_default(), "\"", "");
            },
            _ => {
                method_name = "unsupported".to_string();
            },
        }

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

        let response = self.get_proxy_response(request).await?;
        println!(
            "Method: {}, Proxy Status: {}, Response Time: {}ms",
            method_name,
            response.status(),
            start.elapsed().as_millis()
        );
        Ok(response)
    }
}
