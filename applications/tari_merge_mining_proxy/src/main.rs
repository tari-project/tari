// Copyright 2020. The Tari Project
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

#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

mod block_template_data;
mod block_template_protocol;
mod cli;
mod common;
mod config;
mod error;
mod proxy;

#[cfg(test)]
mod test;

use std::convert::Infallible;

use clap::Parser;
use futures::future;
use hyper::{service::make_service_fn, Server};
use log::*;
use proxy::MergeMiningProxyService;
use tari_app_grpc::tari_rpc as grpc;
use tari_common::{initialize_logging, load_configuration, DefaultConfigLoader};
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use tokio::time::Duration;

use crate::{
    block_template_data::BlockTemplateRepository,
    cli::Cli,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
};
const LOG_TARGET: &str = "tari_mm_proxy::proxy";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path.as_path(), true, &cli.common.config_property_overrides)?;
    initialize_logging(
        &cli.common.log_config_path("proxy"),
        include_str!("../log4rs_sample.yml"),
    )?;
    let config = MergeMiningProxyConfig::load_from(&cfg).expect("Failed to load config");
    debug!(target: LOG_TARGET, "Configuration: {:?}", config);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(25)
        .build()
        .map_err(MmProxyError::ReqwestError)?;
    let base_node = multiaddr_to_socketaddr(&config.grpc_base_node_address)?;
    info!(target: LOG_TARGET, "Connecting to base node at {}", base_node);
    let base_node_client = grpc::base_node_client::BaseNodeClient::connect(format!("http://{}", base_node)).await?;
    let wallet = multiaddr_to_socketaddr(&config.grpc_console_wallet_address)?;
    info!(target: LOG_TARGET, "Connecting to wallet at {}", wallet);
    let wallet_client = grpc::wallet_client::WalletClient::connect(format!("http://{}", wallet)).await?;
    let addr = multiaddr_to_socketaddr(&config.proxy_host_address)?;
    let xmrig_service = MergeMiningProxyService::new(
        config,
        client,
        base_node_client,
        wallet_client,
        BlockTemplateRepository::new(),
    );
    let service = make_service_fn(|_conn| future::ready(Result::<_, Infallible>::Ok(xmrig_service.clone())));

    match Server::try_bind(&addr) {
        Ok(builder) => {
            info!(target: LOG_TARGET, "Listening on {}...", addr);
            println!("Listening on {}...", addr);
            builder.serve(service).await?;
            Ok(())
        },
        Err(err) => {
            error!(target: LOG_TARGET, "Fatal: Cannot bind to '{}'.", addr);
            println!("Fatal: Cannot bind to '{}'.", addr);
            println!("It may be part of a Port Exclusion Range. Please try to use another port for the");
            println!("'proxy_host_address' in 'config/config.toml' and for the applicable XMRig '[pools][url]' or");
            println!("[pools][self-select]' config setting that can be found  in 'config/xmrig_config_***.json' or");
            println!("'<xmrig folder>/config.json'.");
            println!();
            Err(err.into())
        },
    }
}
