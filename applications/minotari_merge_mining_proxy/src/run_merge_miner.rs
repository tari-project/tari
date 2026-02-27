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

use futures::FutureExt;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use log::*;
use minotari_app_grpc::tari_rpc::sha_p2_pool_client::ShaP2PoolClient;
use minotari_app_utilities::parse_miner_input::{
    prompt_for_base_node_address,
    prompt_for_p2pool_address,
    verify_base_node_grpc_mining_responses,
    wallet_payment_address,
    BaseNodeGrpcClient,
    ShaP2PoolGrpcClient,
};
use minotari_node_grpc_client::{grpc, grpc::base_node_client::BaseNodeClient};
use minotari_wallet_grpc_client::ClientAuthenticationInterceptor;
use tari_common::{load_configuration, DefaultConfigLoader, MAX_GRPC_MESSAGE_SIZE};
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use tari_core::proof_of_work::randomx_factory::RandomXFactory;
use tokio::{net::TcpListener, time::Duration};
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

use crate::{
    block_template_data::BlockTemplateRepository,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    proxy::service::MergeMiningProxyService,
    Cli,
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy";
const BLOCK_TEMPLATE_CLEANUP_INTERVAL: u64 = 10 * 60; // 10 minutes

#[allow(clippy::too_many_lines)]
pub async fn start_merge_miner(cli: Cli) -> Result<(), anyhow::Error> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(&config_path, true, cli.non_interactive_mode, &cli, cli.common.network)?;
    let mut config = MergeMiningProxyConfig::load_from(&cfg)?;
    config.set_base_path(cli.common.get_base_path());

    info!(target: LOG_TARGET, "Configuration: {config:?}");
    let agent = concat!("minotari_mm_proxy/", env!("CARGO_PKG_VERSION"));
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .user_agent(agent)
        .tcp_keepalive(Duration::from_secs(60))
        .build()
        .map_err(MmProxyError::ReqwestError)?;

    let wallet_payment_address = wallet_payment_address(config.wallet_payment_address.clone(), config.network)?;
    let mut base_node_client = match connect_base_node(&config).await {
        Ok(client) => client,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not connect to base node: {e}");
            let msg = "Could not connect to base node. \nIs the base node's gRPC running? Try running it with \
                       `--enable-grpc` or enable it in the config.";
            println!("{msg}");
            return Err(e.into());
        },
    };

    let p2pool_client = if config.p2pool_enabled {
        Some(connect_sha_p2pool(&config).await.map_err(|e| {
            error!(target: LOG_TARGET, "Could not connect to p2pool node: {e}");
            let msg = "Could not connect to p2pool node. \nIs the p2pool node's gRPC running? Try running it with \
                       `--enable-grpc` or enable it in the config.";
            println!("{msg}");
            e
        })?)
    } else {
        None
    };
    if let Err(e) = verify_base_node_responses(&mut base_node_client).await {
        if let MmProxyError::BaseNodeNotResponding(_) = e {
            error!(target: LOG_TARGET, "{e}");
            println!();
            let msg = "Are the base node's gRPC mining methods allowed in its 'config.toml'? Please ensure these \
                       methods are enabled in:\n  'grpc_server_allow_methods': \"get_new_block_template\", \
                       \"get_tip_info\", \"get_new_block\", \"submit_block\"";
            println!("{msg}");
            println!();
            return Err(e.into());
        }
    }

    let listen_addr = multiaddr_to_socketaddr(&config.listener_address)?;
    let randomx_factory = RandomXFactory::new(config.max_randomx_vms);
    let block_templates = BlockTemplateRepository::new();

    // Run clean up old templates every 10 minutes
    let cleanup_repo = block_templates.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(BLOCK_TEMPLATE_CLEANUP_INTERVAL));
        loop {
            interval.tick().await;
            if let Err(e) = std::panic::AssertUnwindSafe(cleanup_repo.remove_outdated())
                .catch_unwind()
                .await
            {
                error!(target: LOG_TARGET, "Block template cleanup task panicked: {:?}", e);
            }
        }
    });

    let randomx_service = MergeMiningProxyService::try_create(
        config,
        client,
        base_node_client,
        p2pool_client,
        block_templates,
        randomx_factory,
        wallet_payment_address,
    )?;

    match TcpListener::bind(listen_addr).await {
        Ok(listener) => {
            info!(target: LOG_TARGET, "Listening on {listen_addr}...");
            println!("Listening on {listen_addr}...");

            let mut shutdown = Box::pin(tokio::signal::ctrl_c());
            loop {
                let mut listen_fut = Box::pin(listener.accept());
                tokio::select! {
                    _ = &mut shutdown => {
                        info!(target: LOG_TARGET, "Ctrl-C received, shutting down merge mining proxy...");
                        println!("Ctrl-C: shutting down merge mining proxy...");
                        break;
                    }
                    result = &mut listen_fut => {
                        match result {
                            Ok((tcp, _)) => {
                                info!(target: LOG_TARGET, "Accepted new connection");
                                let svc = randomx_service.clone();
                                let io = TokioIo::new(tcp);

                                tokio::task::spawn(async move {
                                    if let Err(e) = http1::Builder::new().serve_connection(io, &svc).await {
                                        error!("Connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                error!(target: LOG_TARGET, "Error accepting connection: {}", e);
                            }

                        }
                    }

                }
            }
            Ok(())
        },
        Err(err) => {
            error!(target: LOG_TARGET, "Fatal: Cannot bind to '{listen_addr}'.");
            println!("Fatal: Cannot bind to '{listen_addr}'.");
            println!("It may be part of a Port Exclusion Range. Please try to use another port for the");
            println!("'proxy_host_address' in 'config/config.toml' and for the applicable RandomX '[pools][url]' or");
            println!("'[pools][self-select]' config setting that can be found in 'config/xmrig_config_***.json' or");
            println!("'<xmrig folder>/config.json'.");
            println!();
            Err(err.into())
        },
    }
}

async fn verify_base_node_responses(node_conn: &mut BaseNodeGrpcClient) -> Result<(), MmProxyError> {
    if let Err(e) = verify_base_node_grpc_mining_responses(node_conn, grpc::NewBlockTemplateRequest {
        algo: Some(grpc::PowAlgo {
            pow_algo: grpc::pow_algo::PowAlgos::Randomxm.into(),
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
    let base_node_addr;
    if let Some(ref a) = config.base_node_grpc_address {
        base_node_addr = a.clone();
    } else {
        base_node_addr = prompt_for_base_node_address(config.network)?;
    };

    info!(target: LOG_TARGET, "👛 Connecting to base node at {base_node_addr}");
    let mut endpoint = Endpoint::new(base_node_addr)?;

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
    )
    .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
    .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE);

    Ok(node_conn)
}

async fn connect_sha_p2pool(config: &MergeMiningProxyConfig) -> Result<ShaP2PoolGrpcClient, MmProxyError> {
    let p2pool_node_addr;
    if let Some(ref a) = config.p2pool_node_grpc_address {
        p2pool_node_addr = a.clone();
    } else {
        p2pool_node_addr = prompt_for_p2pool_address()?;
    };
    info!(target: LOG_TARGET, "👛 Connecting to p2pool node at {p2pool_node_addr}");
    let mut endpoint = Endpoint::new(p2pool_node_addr)?;

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
    let node_conn = ShaP2PoolClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)?,
    )
    .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
    .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE);

    Ok(node_conn)
}
