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
#![feature(type_alias_impl_trait)]
#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]

mod block_template_data;
mod error;
mod helpers;
mod proxy;
#[cfg(test)]
mod test;

use crate::{block_template_data::BlockTemplateRepository, error::MmProxyError};
use futures::future;
use hyper::{service::make_service_fn, Server};
use proxy::{MergeMiningProxyConfig, MergeMiningProxyService};
use std::{convert::Infallible, io};
use structopt::StructOpt;
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};

#[tokio_macros::main]
async fn main() -> Result<(), MmProxyError> {
    let config = initialize()?;

    let config = MergeMiningProxyConfig::from(config);
    let addr = config.proxy_host_address;

    let xmrig_service = MergeMiningProxyService::new(config, BlockTemplateRepository::new());
    if !xmrig_service.check_connections(&mut io::stdout()).await {
        println!(
            "Warning: some services have not been started or are mis-configured in the proxy config. The proxy will \
             remain running and connect to these services on demand."
        );
    }
    let service = make_service_fn(|_conn| future::ready(Result::<_, Infallible>::Ok(xmrig_service.clone())));

    println!("\nListening on {}...\n", addr);
    Server::bind(&addr).serve(service).await?;

    Ok(())
}

/// Loads the configuration and sets up logging
fn initialize() -> Result<GlobalConfig, MmProxyError> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();
    // Check and initialize configuration files
    bootstrap.init_dirs(ApplicationType::MergeMiningProxy)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    #[cfg(feature = "envlog")]
    let _ = env_logger::try_init();
    // Initialise the logger
    #[cfg(not(feature = "envlog"))]
    bootstrap.initialize_logging()?;

    let cfg = GlobalConfig::convert_from(cfg)?;
    Ok(cfg)
}
