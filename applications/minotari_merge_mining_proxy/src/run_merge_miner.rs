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

use std::{future::Future, net::SocketAddr, pin::pin, sync::Arc};

use futures::FutureExt;
use hyper::{
    Request,
    Response,
    body::{Body, Incoming},
    server::conn::http1,
    service::Service as HyperService,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use log::*;
use minotari_app_grpc::tari_rpc::sha_p2_pool_client::ShaP2PoolClient;
use minotari_app_utilities::parse_miner_input::{
    BaseNodeGrpcClient,
    ShaP2PoolGrpcClient,
    prompt_for_base_node_address,
    prompt_for_p2pool_address,
    verify_base_node_grpc_mining_responses,
    wallet_payment_address,
};
use minotari_node_grpc_client::{grpc, grpc::base_node_client::BaseNodeClient};
use minotari_wallet_grpc_client::ClientAuthenticationInterceptor;
use tari_common::{DefaultConfigLoader, MAX_GRPC_MESSAGE_SIZE, load_configuration};
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use tari_core::proof_of_work::randomx_factory::RandomXFactory;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    time::Duration,
};
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint};

use crate::{
    Cli,
    block_template_data::BlockTemplateRepository,
    config::MergeMiningProxyConfig,
    error::MmProxyError,
    proxy::service::MergeMiningProxyService,
};

const LOG_TARGET: &str = "minotari_mm_proxy::proxy";
const BLOCK_TEMPLATE_CLEANUP_INTERVAL: u64 = 10 * 60; // 10 minutes
/// The time an inbound connection is given to transmit the request line and headers of each request it makes,
/// including the first. This is also, in effect, the keep-alive idle timeout: hyper restarts the timer every time it
/// waits for a new request on an established connection.
///
/// This is the same value as hyper's own default, but it is set explicitly: hyper's default is only applied when a
/// timer is attached to the builder, and when it is not, the timeout is silently discarded with a `warn!` instead of
/// being reported as an error. Setting it explicitly means a missing timer would panic loudly at startup rather than
/// leaving the proxy with no inbound timeout at all.
const INBOUND_HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Once the configured inbound connection lifetime is reached, the connection is shut down gracefully and given this
/// long to finish the request that is in flight (if any) before it is dropped outright.
const INBOUND_CONNECTION_SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

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
    match tokio::time::timeout(
        Duration::from_secs(30),
        verify_base_node_responses(&mut base_node_client),
    )
    .await
    {
        Ok(Err(e)) if matches!(e, MmProxyError::BaseNodeNotResponding(_)) => {
            error!(target: LOG_TARGET, "{e}");
            println!();
            let msg = "Are the base node's gRPC mining methods allowed in its 'config.toml'? Please ensure these \
                       methods are enabled in:\n  'grpc_server_allow_methods': \"get_new_block_template\", \
                       \"get_tip_info\", \"get_new_block\", \"submit_block\"";
            println!("{msg}");
            println!();
            return Err(e.into());
        },
        Err(_timeout) => {
            warn!(
                target: LOG_TARGET,
                "Base node verification timed out; proceeding without full verification"
            );
        },
        _ => {},
    }

    let listen_addr = multiaddr_to_socketaddr(&config.listener_address)?;
    let randomx_factory = RandomXFactory::new(config.max_randomx_vms);
    let block_templates = BlockTemplateRepository::new();
    // Read before `config` is moved into the service.
    let max_concurrent_connections = config.max_concurrent_connections;
    let inbound_connection_lifetime = config.inbound_connection_lifetime;

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

            run_accept_loop(
                listener,
                randomx_service,
                max_concurrent_connections,
                INBOUND_HEADER_READ_TIMEOUT,
                inbound_connection_lifetime,
                async {
                    let _result = tokio::signal::ctrl_c().await;
                    info!(target: LOG_TARGET, "Ctrl-C received, shutting down merge mining proxy...");
                    println!("Ctrl-C: shutting down merge mining proxy...");
                },
            )
            .await;
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

/// Accepts inbound (miner-side) connections until `shutdown` resolves.
///
/// Inbound concurrency is bounded by `max_concurrent_connections`: each connection task owns a semaphore permit for
/// its entire lifetime, so the permit is released no matter how the task ends. Connections that arrive while the
/// permits are exhausted are closed immediately rather than queued, because the accept loop itself must keep draining
/// the kernel backlog - stalling it would turn a single slow client into a complete outage for every other miner.
async fn run_accept_loop<S, B>(
    listener: TcpListener,
    service: S,
    max_concurrent_connections: usize,
    header_read_timeout: Duration,
    connection_lifetime: Duration,
    shutdown: impl Future<Output = ()>,
) where
    S: HyperService<Request<Incoming>, Response = Response<B>> + Clone + Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
    B: Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // A limit of zero would refuse every connection, which is never what an operator means by it.
    let connection_limit = Arc::new(Semaphore::new(max_concurrent_connections.max(1)));
    let mut shutdown = pin!(shutdown);
    loop {
        let mut listen_fut = pin!(listener.accept());
        tokio::select! {
            _ = &mut shutdown => {
                break;
            }
            result = &mut listen_fut => {
                match result {
                    Ok((tcp, peer_addr)) => {
                        let permit = match Arc::clone(&connection_limit).try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                // Dropping `tcp` here closes the socket, so the file descriptor is released
                                // straight away.
                                warn!(
                                    target: LOG_TARGET,
                                    "Inbound connection limit ({max_concurrent_connections}) reached, refusing \
                                    connection from {peer_addr}"
                                );
                                continue;
                            },
                        };
                        debug!(target: LOG_TARGET, "Accepted new connection from {peer_addr}");
                        let service = service.clone();
                        tokio::task::spawn(async move {
                            // Held for the lifetime of the connection.
                            let _permit = permit;
                            serve_inbound_connection(
                                tcp,
                                service,
                                peer_addr,
                                header_read_timeout,
                                connection_lifetime,
                            )
                            .await;
                        });
                    }
                    Err(e) => {
                        error!(target: LOG_TARGET, "Error accepting connection: {e}");
                    }
                }
            }
        }
    }
}

/// Serves a single inbound (miner-side) connection, bounding how long it may live.
///
/// Two timeouts apply. `header_read_timeout` bounds the time the peer is given to transmit the head of each request
/// it makes, and so also acts as the keep-alive idle timeout. `connection_lifetime` bounds the total lifetime of the
/// connection, which catches the cases the header read timeout cannot: a peer that transmits a request head and then
/// stalls part way through the body, or one that dribbles bytes slowly enough to keep resetting the read.
///
/// When the lifetime is reached the connection is shut down gracefully, so a request that is in flight (a block
/// submission, say) still gets [`INBOUND_CONNECTION_SHUTDOWN_GRACE`] to complete before the socket is dropped.
async fn serve_inbound_connection<S, B>(
    tcp: TcpStream,
    service: S,
    peer_addr: SocketAddr,
    header_read_timeout: Duration,
    connection_lifetime: Duration,
) where
    S: HyperService<Request<Incoming>, Response = Response<B>>,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: Body + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // A timer must be attached to the builder: without one, hyper resolves both its own default and any explicitly
    // configured `header_read_timeout` against an empty timer. The default is silently discarded (a `warn!` log is
    // all that is emitted), which is how this connection path came to have no inbound timeout at all.
    let conn = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(header_read_timeout)
        .serve_connection(TokioIo::new(tcp), service);
    let mut conn = pin!(conn);

    let result = tokio::select! {
        result = conn.as_mut() => result,
        _ = tokio::time::sleep(connection_lifetime) => {
            debug!(
                target: LOG_TARGET,
                "Inbound connection from {peer_addr} reached its maximum lifetime of {connection_lifetime:?}, \
                shutting it down"
            );
            conn.as_mut().graceful_shutdown();
            match tokio::time::timeout(INBOUND_CONNECTION_SHUTDOWN_GRACE, conn).await {
                Ok(result) => result,
                Err(_) => {
                    warn!(
                        target: LOG_TARGET,
                        "Inbound connection from {peer_addr} did not shut down within \
                        {INBOUND_CONNECTION_SHUTDOWN_GRACE:?}, closing it"
                    );
                    return;
                },
            }
        }
    };
    if let Err(e) = result {
        error!(target: LOG_TARGET, "Connection error ({peer_addr}): {e}");
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

    const MAX_RETRIES: u32 = 10;
    const RETRY_DELAY: Duration = Duration::from_millis(500);

    for attempt in 1..=MAX_RETRIES {
        let mut endpoint = Endpoint::new(base_node_addr.clone())?;

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

        match endpoint.connect().await {
            Ok(channel) => {
                let node_conn = BaseNodeClient::with_interceptor(
                    channel,
                    ClientAuthenticationInterceptor::create(&config.base_node_grpc_authentication)?,
                )
                .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
                .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE);
                return Ok(node_conn);
            },
            Err(e) if attempt < MAX_RETRIES => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to connect to base node (attempt {attempt}/{MAX_RETRIES}): {e}. Retrying..."
                );
                tokio::time::sleep(RETRY_DELAY).await;
            },
            Err(e) => return Err(MmProxyError::TlsConnectionError(e.to_string())),
        }
    }

    unreachable!()
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

#[cfg(test)]
mod test {
    use std::convert::Infallible;

    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::service::service_fn;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::oneshot,
    };

    use super::*;

    const REQUEST_HEAD: &[u8] = b"GET /get_height HTTP/1.1\r\nHost: localhost\r\n\r\n";
    const STATUS_LINE: &[u8] = b"HTTP/1.1 200";

    async fn ok_response(_request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
        Ok(Response::new(Full::new(Bytes::from_static(b"ok"))))
    }

    /// Returns `true` if a complete request/response round trip succeeds on a new connection to `addr`.
    async fn round_trip(addr: SocketAddr) -> bool {
        let Ok(mut client) = TcpStream::connect(addr).await else {
            return false;
        };
        if client.write_all(REQUEST_HEAD).await.is_err() {
            return false;
        }
        let mut buf = [0u8; STATUS_LINE.len()];
        matches!(
            tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut buf)).await,
            Ok(Ok(_))
        ) && buf == STATUS_LINE
    }

    /// Drains whatever the peer still has to send and returns `true` if it then closed the connection. Either a
    /// clean EOF or a reset is accepted: whether the kernel sends a FIN or an RST depends on whether there is still
    /// unread data buffered on the other side.
    async fn wait_for_close(client: &mut TcpStream) -> bool {
        let drain = async {
            let mut buf = [0u8; 1024];
            loop {
                match client.read(&mut buf).await {
                    Ok(0) | Err(_) => return,
                    Ok(_) => continue,
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5), drain).await.is_ok()
    }

    #[tokio::test]
    async fn inbound_connection_with_no_request_head_is_reaped() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let served = tokio::spawn(async move {
            let (tcp, peer_addr) = listener.accept().await.unwrap();
            serve_inbound_connection(
                tcp,
                service_fn(ok_response),
                peer_addr,
                Duration::from_millis(200),
                Duration::from_secs(60),
            )
            .await;
        });

        // Connect and never send anything. Before the timer was attached to the builder, hyper discarded its
        // default header read timeout and this socket, its task and its read buffer were pinned forever.
        let mut client = TcpStream::connect(addr).await.unwrap();
        assert!(
            wait_for_close(&mut client).await,
            "the header read timeout did not close an idle connection"
        );
        served.await.unwrap();
    }

    #[tokio::test]
    async fn inbound_connection_is_closed_at_its_maximum_lifetime() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let served = tokio::spawn(async move {
            let (tcp, peer_addr) = listener.accept().await.unwrap();
            serve_inbound_connection(
                tcp,
                service_fn(ok_response),
                peer_addr,
                // Long enough that only the lifetime cap can close this connection.
                Duration::from_secs(60),
                Duration::from_millis(200),
            )
            .await;
        });

        // Complete one request so the connection is established and kept alive, then go quiet. Abandoned
        // keep-alive sessions like this (a miner killed with SIGKILL, or a NAT that drops the flow without
        // sending a FIN) are what accumulated one socket, one task and one read buffer each, forever.
        let mut client = TcpStream::connect(addr).await.unwrap();
        client.write_all(REQUEST_HEAD).await.unwrap();
        let mut buf = [0u8; STATUS_LINE.len()];
        tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(buf, STATUS_LINE);

        assert!(
            wait_for_close(&mut client).await,
            "the connection outlived its maximum lifetime"
        );
        served.await.unwrap();
    }

    #[tokio::test]
    async fn accept_loop_refuses_connections_above_the_concurrency_cap() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let accept_loop = tokio::spawn(run_accept_loop(
            listener,
            service_fn(ok_response),
            // One connection at a time, so the cap is trivial to saturate.
            1,
            Duration::from_secs(60),
            Duration::from_secs(60),
            async move {
                let _result = shutdown_rx.await;
            },
        ));

        // The first connection is served and, because it is kept alive, holds the only permit.
        let mut first = TcpStream::connect(addr).await.unwrap();
        first.write_all(REQUEST_HEAD).await.unwrap();
        let mut buf = [0u8; STATUS_LINE.len()];
        tokio::time::timeout(Duration::from_secs(5), first.read_exact(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(buf, STATUS_LINE);

        // The second connection must be closed straight away rather than queued behind the permit or left to
        // accumulate as an unserved file descriptor.
        let mut second = TcpStream::connect(addr).await.unwrap();
        assert!(
            wait_for_close(&mut second).await,
            "a connection over the concurrency cap was not refused"
        );

        // Ending the first connection releases its permit, so the proxy starts serving again.
        drop(first);
        let mut served_again = false;
        for _ in 0..50u32 {
            if round_trip(addr).await {
                served_again = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(served_again, "the connection permit was not released");

        shutdown_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), accept_loop)
            .await
            .unwrap()
            .unwrap();
    }
}
