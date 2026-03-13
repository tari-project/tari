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

mod block_template_storage;
mod error;
mod inner;
mod service;

use std::time::Duration;

use futures::FutureExt;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use log::{error, info, warn};
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::base_node_client::BaseNodeClient,
};
use tari_common::MAX_GRPC_MESSAGE_SIZE;
use tari_common_types::{grpc_authentication::GrpcAuthentication, tari_address::TariAddress};
use tari_comms::{multiaddr::Multiaddr, utils::multiaddr::multiaddr_to_socketaddr};
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::transaction_components::RangeProofType;
use tokio::net::TcpListener;
use tonic::{codegen::InterceptedService, transport::Channel};

use self::{block_template_storage::BlockTemplateStorage, inner::InnerService, service::XmrigProxyService};

const LOG_TARGET: &str = "minotari::base_node::xmrig_proxy";
const CLEANUP_INTERVAL_SECS: u64 = 10 * 60;

/// Start the XMRig-compatible JSON-RPC proxy server embedded in the base node.
///
/// The proxy accepts connections from XMRig (configured with `"coin": "tari"`, `"daemon": true`)
/// and forwards `getblocktemplate`/`submitblock` calls to the base node's local gRPC server.
///
/// # Arguments
/// * `grpc_address` - the base node's own gRPC address (the proxy connects to this internally)
/// * `grpc_auth` - gRPC authentication to use when connecting to the local gRPC server
/// * `listener_address` - the address on which this proxy listens for XMRig connections
/// * `wallet_payment_address` - where mining rewards are sent
/// * `coinbase_extra` - optional extra data in the coinbase
/// * `range_proof_type` - range proof type for coinbase outputs
/// * `shutdown` - shutdown signal from the base node
pub async fn run_xmrig_proxy(
    grpc_address: Multiaddr,
    grpc_auth: GrpcAuthentication,
    listener_address: Multiaddr,
    wallet_payment_address: TariAddress,
    coinbase_extra: Vec<u8>,
    range_proof_type: RangeProofType,
    shutdown: ShutdownSignal,
) -> Result<(), anyhow::Error> {
    let base_node_client = connect_to_grpc(grpc_address, grpc_auth).await?;

    let listen_addr = multiaddr_to_socketaddr(&listener_address)?;
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
        base_node_client,
        block_templates,
        wallet_payment_address,
        coinbase_extra,
        range_proof_type,
    });

    match TcpListener::bind(listen_addr).await {
        Ok(listener) => {
            info!(target: LOG_TARGET, "XMRig proxy listening on {listen_addr}");
            println!(
                "XMRig proxy listening on {listen_addr}. Configure XMRig with: \"coin\": \"tari\", \"url\": \
                 \"{listen_addr}\", \"daemon\": true"
            );

            let mut shutdown = shutdown;
            loop {
                tokio::select! {
                    _ = &mut shutdown => {
                        info!(target: LOG_TARGET, "Shutdown signal received, stopping XMRig proxy");
                        break;
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((tcp, addr)) => {
                                info!(target: LOG_TARGET, "XMRig proxy: new connection from {addr}");
                                let svc = service.clone();
                                let io = TokioIo::new(tcp);
                                tokio::task::spawn(async move {
                                    if let Err(e) = http1::Builder::new().serve_connection(io, &svc).await {
                                        error!(target: LOG_TARGET, "XMRig proxy connection error: {e}");
                                    }
                                });
                            },
                            Err(e) => {
                                error!(target: LOG_TARGET, "XMRig proxy accept error: {e}");
                            },
                        }
                    }
                }
            }
            Ok(())
        },
        Err(e) => {
            warn!(
                target: LOG_TARGET,
                "Cannot bind XMRig proxy to '{listen_addr}': {e}. XMRig solo mining will not be available."
            );
            Err(e.into())
        },
    }
}

async fn connect_to_grpc(
    grpc_address: Multiaddr,
    grpc_auth: GrpcAuthentication,
) -> Result<BaseNodeClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>, anyhow::Error> {
    // Convert multiaddr to a URI that tonic can use
    let socket_addr = multiaddr_to_socketaddr(&grpc_address)?;
    let uri = format!("http://{socket_addr}");

    info!(target: LOG_TARGET, "XMRig proxy connecting to local gRPC at {uri}");

    let channel = tonic::transport::Channel::from_shared(uri)?
        .connect()
        .await
        .map_err(|e| anyhow::anyhow!("XMRig proxy failed to connect to base node gRPC: {e}"))?;

    let client = BaseNodeClient::with_interceptor(
        channel,
        ClientAuthenticationInterceptor::create(&grpc_auth)
            .map_err(|e| anyhow::anyhow!("XMRig proxy gRPC auth error: {e}"))?,
    )
    .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
    .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE);

    Ok(client)
}
