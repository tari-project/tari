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
    future::Future,
    pin::Pin,
    sync::{atomic::AtomicBool, Arc, RwLock},
    task::{Context, Poll},
};

use hyper::{Body, Request, Response, StatusCode};
use jsonrpc::error::StandardError;
use minotari_app_utilities::parse_miner_input::{BaseNodeGrpcClient, ShaP2PoolGrpcClient};
use serde_json::json;
use tari_common_types::tari_address::TariAddress;
use tari_comms::protocol::rpc::__macro_reexports::Service;
use tari_core::{consensus::ConsensusManager, proof_of_work::randomx_factory::RandomXFactory};
use tracing::{error, trace, warn};

use crate::{
    block_template_data::BlockTemplateRepository,
    common::{json_rpc, proxy},
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    proxy::{inner::InnerService, monerod_method::parse_monerod_rpc_method},
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy::service";

#[derive(Debug, Clone)]
pub struct MergeMiningProxyService {
    inner: InnerService,
}

impl MergeMiningProxyService {
    pub fn try_create(
        config: MergeMiningProxyConfig,
        http_client: reqwest::Client,
        base_node_client: BaseNodeGrpcClient,
        p2pool_client: Option<ShaP2PoolGrpcClient>,
        block_templates: BlockTemplateRepository,
        randomx_factory: RandomXFactory,
        wallet_payment_address: TariAddress,
    ) -> Result<Self, MmProxyError> {
        trace!(target: LOG_TARGET, "Config: {:?}", config);
        let consensus_manager = ConsensusManager::builder(config.network).build()?;
        // Assign the slowest response monerod server as the last assigned monerod server
        let last_assigned_monerod_url = config.monerod_url.last().cloned();
        Ok(Self {
            inner: InnerService {
                config: Arc::new(config),
                block_templates,
                http_client,
                base_node_client,
                p2pool_client,
                initial_sync_achieved: Arc::new(AtomicBool::new(false)),
                current_monerod_server: Arc::new(RwLock::new(None)),
                last_assigned_monerod_url: Arc::new(RwLock::new(last_assigned_monerod_url)),
                monerod_cache_values: Arc::new(RwLock::new(None)),
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
            let monerod_method = parse_monerod_rpc_method(request.method(), request.uri(), request.body());

            match inner.handle(monerod_method, request).await {
                Ok(resp) => Ok(resp),
                Err(err) => {
                    error!(target: LOG_TARGET, "Method \"{}\" failed handling request: {:?}", monerod_method, err);
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
