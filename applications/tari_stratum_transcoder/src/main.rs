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

mod common;
mod error;
mod proxy;

use crate::error::StratumTranscoderProxyError;
use futures::future;
use hyper::{service::make_service_fn, Server};
use proxy::{StratumTranscoderProxyConfig, StratumTranscoderProxyService};
use std::convert::{Infallible, TryFrom};
use structopt::StructOpt;
use tari_app_grpc::tari_rpc as grpc;
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};
use tokio::time::Duration;

#[tokio::main]
async fn main() -> Result<(), StratumTranscoderProxyError> {
    let config = initialize()?;

    let config = StratumTranscoderProxyConfig::try_from(config)?;
    let addr = config.transcoder_host_address;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(25)
        .build()
        .map_err(StratumTranscoderProxyError::ReqwestError)?;
    let base_node_client =
        grpc::base_node_client::BaseNodeClient::connect(format!("http://{}", config.grpc_base_node_address)).await?;
    let wallet_client =
        grpc::wallet_client::WalletClient::connect(format!("http://{}", config.grpc_console_wallet_address)).await?;
    let miningcore_service = StratumTranscoderProxyService::new(config, client, base_node_client, wallet_client);
    let service = make_service_fn(|_conn| future::ready(Result::<_, Infallible>::Ok(miningcore_service.clone())));

    match Server::try_bind(&addr) {
        Ok(builder) => {
            println!("Listening on {}...", addr);
            builder.serve(service).await?;
            Ok(())
        },
        Err(err) => {
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

/// Loads the configuration and sets up logging
fn initialize() -> Result<GlobalConfig, StratumTranscoderProxyError> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();
    // Check and initialize configuration files
    let application_type = ApplicationType::StratumTranscoder;
    bootstrap.init_dirs(application_type)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    #[cfg(feature = "envlog")]
    let _ = env_logger::try_init();
    // Initialise the logger
    #[cfg(not(feature = "envlog"))]
    bootstrap.initialize_logging()?;

    let cfg = GlobalConfig::convert_from(application_type, cfg, bootstrap.network)?;
    Ok(cfg)
}
