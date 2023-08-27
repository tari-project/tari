//  Copyright 2021, The Taiji Project
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

use std::{convert::Infallible, net::SocketAddr, string::FromUtf8Error};

use prometheus::{Encoder, Registry, TextEncoder};
use tokio::{task, task::JoinError};
use warp::{reject::Reject, Filter, Rejection, Reply};

const LOG_TARGET: &str = "app::metrics_server";

pub async fn start(listen_addr: SocketAddr, registry: Registry) {
    let route = warp::path!("metrics")
        .and(with(registry))
        .and_then(metrics_text_handler);

    log::info!(target: LOG_TARGET, "Metrics server started on {}", listen_addr);
    let routes = route.with(warp::log("metrics_server"));
    warp::serve(routes).run(listen_addr).await;
}

async fn metrics_text_handler(registry: Registry) -> Result<impl Reply, Rejection> {
    task::spawn_blocking::<_, Result<_, Error>>(move || {
        let text_encoder = TextEncoder::new();
        let mut buffer = Vec::new();
        text_encoder.encode(&registry.gather(), &mut buffer)?;
        let encoded = String::from_utf8(buffer)?;
        Ok(encoded)
    })
    .await
    .map_err(Error::BlockingThreadFailed)?
    .map_err(Into::into)
}

fn with<T: Clone + Send>(t: T) -> impl Filter<Extract = (T,), Error = Infallible> + Clone {
    warp::any().map(move || t.clone())
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Failed to encode: {0}")]
    PrometheusEncodeFailed(#[from] prometheus::Error),
    #[error("Failed to encode: {0}")]
    FromUtf8(#[from] FromUtf8Error),
    #[error("Failed to spawn blocking thread: {0}")]
    BlockingThreadFailed(#[from] JoinError),
}

impl Reject for Error {}
