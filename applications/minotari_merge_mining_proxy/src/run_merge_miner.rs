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

use std::{convert::Infallible, str::FromStr};

use futures::future;
use hyper::{service::make_service_fn, Server};
use log::*;
use minotari_node_grpc_client::grpc::base_node_client::BaseNodeClient;
use minotari_wallet_grpc_client::ClientAuthenticationInterceptor;
use tari_common::{
    configuration::bootstrap::{grpc_default_port, ApplicationType},
    load_configuration,
    DefaultConfigLoader,
};
use tari_common_types::tari_address::TariAddress;
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use tari_core::proof_of_work::randomx_factory::RandomXFactory;
use tokio::time::Duration;
use tonic::{
    codegen::InterceptedService,
    transport::{Channel, Endpoint},
};

use crate::{
    block_template_data::BlockTemplateRepository,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    proxy::MergeMiningProxyService,
    Cli,
};
const LOG_TARGET: &str = "minotari_mm_proxy::proxy";

pub async fn start_merge_miner(cli: Cli) -> Result<(), anyhow::Error> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(&config_path, true, &cli)?;
    let mut config = MergeMiningProxyConfig::load_from(&cfg)?;
    setup_grpc_config(&mut config);

    let wallet_payment_address = TariAddress::from_str(&config.wallet_payment_address)
        .map_err(|err| MmProxyError::WalletPaymentAddress("'wallet_payment_address' ".to_owned() + &err.to_string()))?;
    if wallet_payment_address == TariAddress::default() {
        return Err(anyhow::Error::msg(
            "'wallet_payment_address' may not have the default value",
        ));
    }
    if wallet_payment_address.network() != config.network {
        return Err(anyhow::Error::msg(
            "'wallet_payment_address' network does not match miner network".to_string(),
        ));
    }

    info!(target: LOG_TARGET, "Configuration: {:?}", config);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(25)
        .build()
        .map_err(MmProxyError::ReqwestError)?;

    let base_node_client = connect_base_node(&config).await?;

    let listen_addr = multiaddr_to_socketaddr(&config.listener_address)?;
    let randomx_factory = RandomXFactory::new(config.max_randomx_vms);
    let randomx_service = MergeMiningProxyService::new(
        config,
        client,
        base_node_client,
        BlockTemplateRepository::new(),
        randomx_factory,
    )?;
    let service = make_service_fn(|_conn| future::ready(Result::<_, Infallible>::Ok(randomx_service.clone())));

    match Server::try_bind(&listen_addr) {
        Ok(builder) => {
            info!(target: LOG_TARGET, "Listening on {}...", listen_addr);
            println!("Listening on {}...", listen_addr);
            builder.serve(service).await?;
            Ok(())
        },
        Err(err) => {
            error!(target: LOG_TARGET, "Fatal: Cannot bind to '{}'.", listen_addr);
            println!("Fatal: Cannot bind to '{}'.", listen_addr);
            println!("It may be part of a Port Exclusion Range. Please try to use another port for the");
            println!("'proxy_host_address' in 'config/config.toml' and for the applicable RandomX '[pools][url]' or");
            println!("[pools][self-select]' config setting that can be found  in 'config/xmrig_config_***.json' or");
            println!("'<xmrig folder>/config.json'.");
            println!();
            Err(err.into())
        },
    }
}

async fn connect_base_node(
    config: &MergeMiningProxyConfig,
) -> Result<BaseNodeClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>, MmProxyError> {
    let base_node_addr = format!(
        "http://{}",
        multiaddr_to_socketaddr(
            &config
                .base_node_grpc_address
                .clone()
                .expect("Base node grpc address not found")
        )?
    );
    info!(target: LOG_TARGET, "👛 Connecting to base node at {}", base_node_addr);
    let channel = Endpoint::from_str(&base_node_addr)?.connect().await?;
    let node_conn = BaseNodeClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)?,
    );

    Ok(node_conn)
}

fn setup_grpc_config(config: &mut MergeMiningProxyConfig) {
    if config.base_node_grpc_address.is_none() {
        config.base_node_grpc_address = Some(
            format!(
                "/ip4/127.0.0.1/tcp/{}",
                grpc_default_port(ApplicationType::BaseNode, config.network)
            )
            .parse()
            .unwrap(),
        );
    }
}
