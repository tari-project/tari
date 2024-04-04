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
use minotari_app_grpc::tls::protocol_string;
use minotari_app_utilities::parse_miner_input::{
    base_node_socket_address,
    verify_base_node_grpc_mining_responses,
    wallet_payment_address,
    BaseNodeGrpcClient,
};
use minotari_node_grpc_client::{grpc, grpc::base_node_client::BaseNodeClient};
use minotari_wallet_grpc_client::ClientAuthenticationInterceptor;
use tari_common::{configuration::StringList, load_configuration, DefaultConfigLoader};
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use tari_core::proof_of_work::randomx_factory::RandomXFactory;
use tokio::time::Duration;
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

use crate::{
    block_template_data::BlockTemplateRepository,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    monero_fail::get_monerod_info,
    proxy::MergeMiningProxyService,
    Cli,
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy";

pub async fn start_merge_miner(cli: Cli) -> Result<(), anyhow::Error> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(&config_path, true, cli.non_interactive_mode, &cli)?;
    let mut config = MergeMiningProxyConfig::load_from(&cfg)?;
    config.set_base_path(cli.common.get_base_path());
    if config.use_dynamic_fail_data {
        let entries = get_monerod_info(15, Duration::from_secs(5), &config.monero_fail_url).await?;
        if !entries.is_empty() {
            config.monerod_url = StringList::from(entries.into_iter().map(|entry| entry.url).collect::<Vec<_>>());
        }
    }

    info!(target: LOG_TARGET, "Configuration: {:?}", config);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(25)
        .build()
        .map_err(MmProxyError::ReqwestError)?;

    let wallet_payment_address = wallet_payment_address(config.wallet_payment_address.clone(), config.network)?;
    let mut base_node_client = match connect_base_node(&config).await {
        Ok(client) => client,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not connect to base node: {}", e);
            let msg = "Could not connect to base node. \nIs the base node's gRPC running? Try running it with \
                       `--enable-grpc` or enable it in the config.";
            println!("{}", msg);
            return Err(e.into());
        },
    };
    if let Err(e) = verify_base_node_responses(&mut base_node_client).await {
        if let MmProxyError::BaseNodeNotResponding(_) = e {
            error!(target: LOG_TARGET, "{}", e.to_string());
            println!();
            let msg = "Are the base node's gRPC mining methods allowed in its 'config.toml'? Please ensure these \
                       methods are enabled in:\n  'grpc_server_allow_methods': \"get_new_block_template\", \
                       \"get_tip_info\", \"get_new_block\", \"submit_block\"";
            println!("{}", msg);
            println!();
            return Err(e.into());
        }
    }

    let listen_addr = multiaddr_to_socketaddr(&config.listener_address)?;
    let randomx_factory = RandomXFactory::new(config.max_randomx_vms);
    let randomx_service = MergeMiningProxyService::new(
        config,
        client,
        base_node_client,
        BlockTemplateRepository::new(),
        randomx_factory,
        wallet_payment_address,
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

async fn verify_base_node_responses(node_conn: &mut BaseNodeGrpcClient) -> Result<(), MmProxyError> {
    if let Err(e) = verify_base_node_grpc_mining_responses(node_conn, grpc::NewBlockTemplateRequest {
        algo: Some(grpc::PowAlgo {
            pow_algo: grpc::pow_algo::PowAlgos::Randomx.into(),
        }),
        max_weight: 0,
    })
    .await
    {
        return Err(MmProxyError::BaseNodeNotResponding(e));
    }
    Ok(())
}

async fn connect_base_node(config: &MergeMiningProxyConfig) -> Result<BaseNodeGrpcClient, MmProxyError> {
    let socketaddr = base_node_socket_address(config.base_node_grpc_address.clone(), config.network)?;
    let base_node_addr = format!(
        "{}{}",
        protocol_string(config.base_node_grpc_tls_domain_name.is_some()),
        socketaddr,
    );

    info!(target: LOG_TARGET, "👛 Connecting to base node at {}", base_node_addr);
    let mut endpoint = Endpoint::from_str(&base_node_addr)?;

    if let Some(domain_name) = config.base_node_grpc_tls_domain_name.as_ref() {
        let pem = tokio::fs::read(config.config_dir.join(&config.base_node_grpc_ca_cert_filename))
            .await
            .map_err(|e| MmProxyError::TlsConnectionError(e.to_string()))?;
        let ca = Certificate::from_pem(pem);

        let tls = ClientTlsConfig::new().ca_certificate(ca).domain_name(domain_name);
        endpoint = endpoint
            .tls_config(tls)
            .map_err(|e| MmProxyError::TlsConnectionError(e.to_string()))?;
    }

    let channel = endpoint
        .connect()
        .await
        .map_err(|e| MmProxyError::TlsConnectionError(e.to_string()))?;
    let node_conn = BaseNodeClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)?,
    );

    Ok(node_conn)
}
