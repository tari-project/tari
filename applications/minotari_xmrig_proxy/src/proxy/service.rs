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

use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
};

use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Response, StatusCode, body::Incoming};
use log::{error, trace};
use serde_json::Value;

use crate::{error::XmrigProxyError, proxy::inner::InnerService};

const LOG_TARGET: &str = "minotari::xmrig_proxy::service";

pub type ProxyBody = Full<bytes::Bytes>;

/// Creates a JSON HTTP response.
pub fn json_response(status: StatusCode, body: &Value) -> Result<Response<ProxyBody>, XmrigProxyError> {
    let body_bytes = serde_json::to_vec(body)?;
    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(bytes::Bytes::from(body_bytes)))
        .expect("valid response"))
}

/// The Hyper service that handles incoming XMRig connections.
#[derive(Clone)]
pub struct XmrigProxyService {
    inner: InnerService,
}

impl XmrigProxyService {
    pub fn new(inner: InnerService) -> Self {
        Self { inner }
    }
}

impl hyper::service::Service<Request<Incoming>> for XmrigProxyService {
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response<ProxyBody>, Infallible>> + Send>>;
    type Response = Response<ProxyBody>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move {
            let method = req.method().clone();
            let path = req.uri().path().to_string();
            trace!(target: LOG_TARGET, "{method} {path}");

            let result = if method == Method::GET {
                inner.handle_get(&path).await
            } else {
                // Collect the request body
                match req.into_body().collect().await {
                    Ok(collected) => inner.handle(collected.to_bytes()).await,
                    Err(e) => {
                        error!(target: LOG_TARGET, "Failed to collect request body: {e}");
                        Err(XmrigProxyError::InvalidRequest(e.to_string()))
                    },
                }
            };

            Ok(match result {
                Ok(response) => response,
                Err(e) => {
                    error!(target: LOG_TARGET, "Handler error: {e}");
                    Response::builder()
                        .status(e.status_code())
                        .header("Content-Type", "application/json")
                        .body(Full::new(bytes::Bytes::from(
                            serde_json::to_vec(&serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": -1,
                                "error": {"code": -32603, "message": e.to_string()},
                            }))
                            .unwrap_or_default(),
                        )))
                        .expect("valid error response")
                },
            })
        })
    }
}
