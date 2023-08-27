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
use std::time::{Duration, Instant};

use log::*;
use prometheus::{Encoder, TextEncoder};
use reqwest::{Client, Url};
use tokio::{time, time::MissedTickBehavior};

use crate::Registry;

const LOG_TARGET: &str = "base_node::metrics::push";

pub async fn start(endpoint: Url, push_interval: Duration, registry: Registry) {
    let mut interval = time::interval(push_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        interval.tick().await;

        if let Err(err) = push_metrics(&registry, endpoint.clone()).await {
            error!(target: LOG_TARGET, "{}", err);
        }
    }
}

async fn push_metrics(registry: &Registry, endpoint: Url) -> Result<(), anyhow::Error> {
    let client = Client::new();
    let timer = Instant::now();
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    let metrics = registry.gather();
    encoder.encode(&metrics, &mut buffer)?;
    let raw_metrics_data = String::from_utf8(buffer)?;
    client.post(endpoint).body(raw_metrics_data).send().await?;
    debug!(
        target: LOG_TARGET,
        "POSTed {} metrics in {:.2?}",
        metrics.len(),
        timer.elapsed()
    );
    Ok(())
}
