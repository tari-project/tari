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

use std::sync::Arc;

use futures::FutureExt;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use log::{error, info};
use minotari_app_utilities::parse_miner_input::{
    BaseNodeGrpcClient,
    prompt_for_base_node_address,
    verify_base_node_grpc_mining_responses,
    wallet_payment_address,
};
use minotari_node_grpc_client::grpc;
use minotari_wallet_grpc_client::ClientAuthenticationInterceptor;
use tari_common::{DefaultConfigLoader, MAX_GRPC_MESSAGE_SIZE, load_configuration};
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use tokio::{net::TcpListener, time::Duration};
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

use crate::{
    Cli,
    block_template_storage::BlockTemplateStorage,
    config::XmrigProxyConfig,
    error::XmrigProxyError,
    proxy::{inner::InnerService, service::XmrigProxyService},
};

const LOG_TARGET: &str = "minotari::xmrig_proxy";
const CLEANUP_INTERVAL_SECS: u64 = 10 * 60;

pub async fn start_xmrig_proxy(cli: Cli) -> Result<(), anyhow::Error> {
    let config_path = cli.common.config_path();
    let cfg = load_configuration(&config_path, true, cli.non_interactive_mode, &cli, cli.common.network)?;
    let mut config = XmrigProxyConfig::load_from(&cfg)?;
    config.set_base_path(cli.common.get_base_path());

    info!(target: LOG_TARGET, "Configuration: {config:?}");

    let wallet_address = wallet_payment_address(config.wallet_payment_address.clone(), config.network)?;

    let mut base_node_client = match connect_base_node(&config).await {
        Ok(client) => client,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not connect to base node: {e}");
            println!("Could not connect to base node. Is the base node gRPC running?");
            return Err(e.into());
        },
    };

    // Verify the base node supports the gRPC mining methods we need
    if let Err(e) = verify_base_node_grpc_mining_responses(&mut base_node_client, grpc::NewBlockTemplateRequest {
        algo: Some(grpc::PowAlgo {
            pow_algo: grpc::pow_algo::PowAlgos::Randomxt.into(),
        }),
        max_weight: 0,
    })
    .await
    {
        error!(target: LOG_TARGET, "{e}");
        println!(
            "Are the base node's gRPC mining methods enabled? Ensure these are set in 'config.toml':\n  \
             'grpc_server_allow_methods': \"get_new_block_template\", \"get_tip_info\", \
             \"get_new_block_template_with_coinbases\", \"submit_block\""
        );
        return Err(XmrigProxyError::BaseNodeNotResponding(e.to_string()).into());
    }

    let listen_addr = multiaddr_to_socketaddr(&config.listener_address)?;
    let block_templates = BlockTemplateStorage::new();

    // Periodic cleanup of expired templates
    let cleanup_storage = block_templates.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
        loop {
            interval.tick().await;
            if let Err(e) =
                std::panic::AssertUnwindSafe(cleanup_storage.remove_outdated()).catch_unwind().await
            {
                error!(target: LOG_TARGET, "Template cleanup panicked: {e:?}");
            }
        }
    });

    let service = XmrigProxyService::new(InnerService {
        config: Arc::new(config),
        base_node_client,
        block_templates,
        wallet_payment_address: wallet_address,
    });

    match TcpListener::bind(listen_addr).await {
        Ok(listener) => {
            info!(target: LOG_TARGET, "Listening on {listen_addr}...");
            println!("Tari XMRig proxy listening on {listen_addr}");
            println!("Configure XMRig with: \"coin\": \"tari\", \"url\": \"{listen_addr}\", \"daemon\": true");

            let mut shutdown = Box::pin(tokio::signal::ctrl_c());
            loop {
                tokio::select! {
                    _ = &mut shutdown => {
                        info!(target: LOG_TARGET, "Ctrl-C received, shutting down...");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((tcp, addr)) => {
                                info!(target: LOG_TARGET, "New connection from {addr}");
                                let svc = service.clone();
                                let io = TokioIo::new(tcp);
                                tokio::task::spawn(async move {
                                    if let Err(e) = http1::Builder::new().serve_connection(io, &svc).await {
                                        error!(target: LOG_TARGET, "Connection error: {e}");
                                    }
                                });
                            },
                            Err(e) => {
                                error!(target: LOG_TARGET, "Accept error: {e}");
                            },
                        }
                    }
                }
            }
            Ok(())
        },
        Err(e) => {
            error!(target: LOG_TARGET, "Cannot bind to '{listen_addr}': {e}");
            println!("Fatal: Cannot bind to '{listen_addr}'. Try a different port in the config.");
            Err(e.into())
        },
    }
}

async fn connect_base_node(config: &XmrigProxyConfig) -> Result<BaseNodeGrpcClient, XmrigProxyError> {
    let base_node_addr = if let Some(ref a) = config.base_node_grpc_address {
        a.clone()
    } else {
        prompt_for_base_node_address(config.network)
            .map_err(|e| XmrigProxyError::BaseNodeNotResponding(e.to_string()))?
    };

    info!(target: LOG_TARGET, "Connecting to base node at {base_node_addr}");
    let mut endpoint =
        Endpoint::new(base_node_addr).map_err(|e| XmrigProxyError::TlsConnectionError(e.to_string()))?;

    if let Some(domain_name) = config.base_node_grpc_tls_domain_name.as_ref() {
        let pem = tokio::fs::read(config.config_dir.join(&config.base_node_grpc_ca_cert_filename))
            .await
            .map_err(|e| XmrigProxyError::TlsConnectionError(e.to_string()))?;
        let ca = Certificate::from_pem(pem);
        let tls = ClientTlsConfig::new().ca_certificate(ca).domain_name(domain_name);
        endpoint = endpoint
            .tls_config(tls)
            .map_err(|e| XmrigProxyError::TlsConnectionError(e.to_string()))?;
    }

    let channel = endpoint
        .connect()
        .await
        .map_err(|e| XmrigProxyError::TlsConnectionError(e.to_string()))?;

    use minotari_node_grpc_client::grpc::base_node_client::BaseNodeClient;
    Ok(BaseNodeClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)
            .map_err(|e| XmrigProxyError::TlsConnectionError(e.to_string()))?,
    )
    .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
    .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE))
}

