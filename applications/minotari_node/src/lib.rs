// Copyright 2022. The Tari Project
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

// non-64-bit not supported
minotari_app_utilities::deny_non_64_bit_archs!();

#[macro_use]
mod table;

mod bootstrap;
mod builder;
pub mod cli;
mod commands;
pub mod config;
mod consensus_constants_tracker;
mod grpc;
mod grpc_method;
#[cfg(feature = "metrics")]
mod metrics;
mod recovery;
mod utils;

mod http;
mod xmrig_proxy;
use std::{process, sync::Arc};

use commands::{cli_loop::CliLoop, command::CommandContext};
use futures::FutureExt;
pub use grpc_method::GrpcMethod;
pub use http::HttpCacheConfig;
use log::*;
use minotari_app_grpc::{
    authentication::ServerAuthenticationInterceptor,
    tari_rpc::{
        self,
        base_node_server::SERVICE_NAME as BASE_NODE_GRPC_SERVICE_NAME,
        readiness_status::State as ReadinessState,
    },
    tls::identity::read_identity,
};
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use tari_common::{
    MAX_GRPC_MESSAGE_SIZE,
    configuration::bootstrap::{ApplicationType, grpc_default_port},
    exit_codes::{ExitCode, ExitError},
};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_comms::{NodeIdentity, multiaddr::Multiaddr, utils::multiaddr::multiaddr_to_socketaddr};
use tari_core::base_node::{StateMachineHandle, state_machine_service::states::StatusInfo};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    task::{self, JoinHandle},
    time::timeout,
};
use tonic::{
    codegen::InterceptedService,
    transport::{Identity, Server, ServerTlsConfig},
};
use tonic_health::{ServingStatus, server::HealthReporter};

pub use crate::config::{ApplicationConfig, BaseNodeConfig, DatabaseType};
#[cfg(feature = "metrics")]
pub use crate::metrics::MetricsConfig;
use crate::{cli::Cli, grpc::readiness_grpc_server::ReadinessGrpcServer};

const LOG_TARGET: &str = "minotari::base_node::app";
const GRPC_HEALTH_OVERALL_SERVICE_NAME: &str = "";

pub async fn run_base_node(
    shutdown: Shutdown,
    node_identity: Arc<NodeIdentity>,
    config: Arc<ApplicationConfig>,
) -> Result<(), ExitError> {
    let data_dir = config.base_node.data_dir.clone();
    let data_dir_str = data_dir.clone().into_os_string().into_string().unwrap();

    let mut config_path = data_dir.clone();
    config_path.push("config.toml");

    let cli = Cli {
        common: CommonCliArgs {
            base_path: data_dir_str,
            config: config_path.into_os_string().into_string().unwrap(),
            log_config: None,
            log_path: None,
            network: None,
            config_property_overrides: vec![],
        },
        init: true,
        rebuild_db: false,
        non_interactive_mode: true,
        watch: None,
        profile_with_tokio_console: false,
        mining_enabled: false,
        grpc_enabled: false,
        grpc_address: None,
        second_layer_grpc_enabled: false,
        disable_splash_screen: true,
        print_env: false,
        libtor_data_dir: None,
    };

    run_base_node_with_cli(node_identity, config, cli, shutdown).await
}

/// Sets up the base node and runs the cli_loop
#[allow(clippy::too_many_lines)]
pub async fn run_base_node_with_cli(
    node_identity: Arc<NodeIdentity>,
    config: Arc<ApplicationConfig>,
    cli: Cli,
    shutdown: Shutdown,
) -> Result<(), ExitError> {
    #[cfg(feature = "metrics")]
    {
        metrics::install(
            ApplicationType::BaseNode,
            &node_identity,
            &config.metrics,
            shutdown.to_signal(),
        );
    }

    let (grpc_address, auth, tls_identity) = prepare_grpc_params(&config).await?;

    let (readiness_grpc_server, readiness_handler) = ReadinessGrpcServer::new();
    let mut readiness_grpc_shutdown = Shutdown::new();
    let mut readiness_task: Option<JoinHandle<Result<(), anyhow::Error>>> = None;
    if config.base_node.grpc_enabled && config.base_node.grpc_readiness_enabled {
        readiness_task = Some(task::spawn(run_grpc(
            readiness_grpc_server,
            grpc_address.clone(),
            auth.clone(),
            tls_identity.clone(),
            None,
            readiness_grpc_shutdown.to_signal(),
        )));
    } else {
        info!(target: LOG_TARGET, "Readiness gRPC server will not be started.");
    }
    readiness_handler.send_readiness_status(ReadinessState::StartingUp);

    if cli.rebuild_db {
        info!(target: LOG_TARGET, "Node is in recovery mode, entering recovery");
        readiness_handler.send_readiness_status(ReadinessState::RecoveringPreparing);
        recovery::initiate_recover_db(&config.base_node)?;
        readiness_handler.send_readiness_status(ReadinessState::RecoveringRebuilding);
        recovery::run_recovery(&config.base_node, readiness_handler)
            .await
            .map_err(|e| ExitError::new(ExitCode::RecoveryError, e))?;
        return Ok(());
    };

    // Build, node, build!
    let ctx =
        builder::configure_and_initialize_node(config.clone(), node_identity, shutdown.to_signal(), &readiness_handler)
            .await?;

    ctx.start()
        .map_err(|e| ExitError::new(ExitCode::DatabaseError, format!("Could not start database.{e:?}")))?;

    // Run, node, run!
    let context = CommandContext::new(&ctx, shutdown.clone(), cli.common.config_property_overrides.clone());
    readiness_handler.send_readiness_status(ReadinessState::Ready);

    readiness_grpc_shutdown.trigger();
    if let Some(task) = readiness_task {
        // The readiness gRPC server listens on the same address as the main gRPC server,
        // so we MUST ensure its socket is fully released before binding the main server,
        // otherwise the main bind will fail with EADDRINUSE.
        const READINESS_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
        // Keep an abort handle so that on timeout we can cancel the task and await its
        // termination, which guarantees the listener is dropped before we continue. Simply
        // dropping the JoinHandle does NOT cancel the task.
        let abort_handle = task.abort_handle();
        match timeout(READINESS_SHUTDOWN_TIMEOUT, task).await {
            Ok(Ok(Ok(()))) => {
                info!(target: LOG_TARGET, "Readiness gRPC server shutdown successfully");
            },
            Ok(Ok(Err(e))) => {
                error!(target: LOG_TARGET, "Readiness gRPC server returned an error: {e}");
            },
            Ok(Err(e)) => {
                error!(target: LOG_TARGET, "Readiness gRPC server task failed: {e}");
            },
            Err(_elapsed) => {
                error!(
                    target: LOG_TARGET,
                    "Readiness gRPC server shutdown did not complete in {:?}; aborting it to release the listener",
                    READINESS_SHUTDOWN_TIMEOUT
                );
                abort_handle.abort();
                // Yield once so the abort is observed and the task's listener is dropped
                // before we attempt to bind the main gRPC server on the same address.
                tokio::task::yield_now().await;
            },
        }
    }

    // Go, GRPC, go go
    let grpc = grpc::base_node_grpc_server::BaseNodeGrpcServer::from_base_node_context(&ctx, config.base_node.clone());

    if config.base_node.grpc_enabled {
        // Spawn the main gRPC server and keep its handle so a bind failure (e.g. EADDRINUSE)
        // is surfaced as a fatal startup error rather than silently disabling gRPC for the
        // lifetime of the process.
        let grpc_handle = task::spawn(run_grpc(
            grpc,
            grpc_address.clone(),
            auth.clone(),
            tls_identity,
            Some(ctx.state_machine()),
            shutdown.to_signal(),
        ));

        // Give the server a brief window to either bind successfully or fail. tonic does the
        // bind inside the future, so a successful bind means the future is still pending after
        // this short wait. A bind error returns essentially immediately.
        match timeout(std::time::Duration::from_millis(500), grpc_handle).await {
            Err(_still_running) => {
                // Still running after the wait → bind succeeded.
            },
            Ok(Ok(Ok(()))) => {
                // Server returned Ok before we even got here — only possible on immediate
                // shutdown signal, which would be unusual at this stage but is not an error.
                info!(target: LOG_TARGET, "GRPC server returned during startup window");
            },
            Ok(Ok(Err(e))) => {
                return Err(ExitError::new(
                    ExitCode::GrpcError,
                    format!("Failed to start gRPC server on {grpc_address}: {e}"),
                ));
            },
            Ok(Err(e)) => {
                return Err(ExitError::new(
                    ExitCode::GrpcError,
                    format!("gRPC server task panicked during startup: {e}"),
                ));
            },
        }
    }

    // Start the built-in XMRig proxy if enabled
    if config.base_node.xmrig_proxy_enabled {
        if config.base_node.xmrig_proxy_wallet_payment_address.is_empty() {
            warn!(
                target: LOG_TARGET,
                "xmrig_proxy_enabled is true but xmrig_proxy_wallet_payment_address is not set. XMRig proxy will not \
                 start."
            );
        } else {
            let proxy_wallet_address = config.base_node.xmrig_proxy_wallet_payment_address.clone();
            let proxy_listener = config.base_node.xmrig_proxy_address.clone();
            let proxy_extra = config.base_node.xmrig_proxy_coinbase_extra.as_bytes().to_vec();
            let proxy_range_proof = config.base_node.xmrig_proxy_range_proof_type;
            let signal = shutdown.to_signal();
            let network = config.base_node.network;
            let node_service = ctx.local_node();
            let consensus_rules = ctx.consensus_rules().clone();
            let state_machine = ctx.state_machine();

            match proxy_wallet_address.parse::<tari_common_types::tari_address::TariAddress>() {
                Ok(wallet_addr) if wallet_addr.network() == network => {
                    task::spawn(async move {
                        if let Err(e) = xmrig_proxy::run_xmrig_proxy(
                            node_service,
                            consensus_rules,
                            state_machine,
                            proxy_listener,
                            wallet_addr,
                            proxy_extra,
                            proxy_range_proof,
                            signal,
                        )
                        .await
                        {
                            error!(target: LOG_TARGET, "XMRig proxy error: {e}");
                        }
                    });
                },
                Ok(wallet_addr) => {
                    warn!(
                        target: LOG_TARGET,
                        "Invalid xmrig_proxy_wallet_payment_address: address network '{}' does not match node \
                         network '{network}'. XMRig proxy will not start.",
                        wallet_addr.network()
                    );
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Invalid xmrig_proxy_wallet_payment_address: {e}. XMRig proxy will not start."
                    );
                },
            }
        }
    }

    let main_loop = CliLoop::new(context, cli.watch, cli.non_interactive_mode);
    if cli.non_interactive_mode {
        println!("Node started in non-interactive mode (pid = {})", process::id());
    } else {
        info!(
            target: LOG_TARGET,
            "Node has been successfully configured and initialized. Starting CLI loop."
        );
    }
    if !config.base_node.force_sync_peers.is_empty() {
        warn!(
            target: LOG_TARGET,
            "Force Sync Peers have been set! This node will only sync to the nodes in this set."
        );
    }

    info!(target: LOG_TARGET, "Minotari base node has STARTED");
    main_loop.cli_loop(cli.disable_splash_screen).await;
    ctx.wait_for_shutdown().await;

    println!("Goodbye!");
    Ok(())
}

/// Runs the gRPC server
async fn run_grpc<T: tari_rpc::base_node_server::BaseNode>(
    grpc: T,
    grpc_address: Multiaddr,
    auth_config: GrpcAuthentication,
    tls_identity: Option<Identity>,
    health_state_machine: Option<StateMachineHandle>,
    interrupt_signal: ShutdownSignal,
) -> Result<(), anyhow::Error> {
    info!(target: LOG_TARGET, "Starting GRPC on {grpc_address}");

    let grpc_address = multiaddr_to_socketaddr(&grpc_address)?;
    let auth = ServerAuthenticationInterceptor::new(auth_config)
        .ok_or(anyhow::anyhow!("Unable to prepare server gRPC authentication"))?;

    let sized_server = minotari_app_grpc::tari_rpc::base_node_server::BaseNodeServer::new(grpc)
        .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE)
        .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE);
    let service = InterceptedService::new(sized_server, auth);

    let mut server_builder = if let Some(identity) = tls_identity {
        Server::builder().tls_config(ServerTlsConfig::new().identity(identity))?
    } else {
        Server::builder()
    };

    if let Some(state_machine_handle) = health_state_machine {
        let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
        let mut status_watch = state_machine_handle.get_status_info_watch();
        let is_serving = is_grpc_health_serving(&status_watch.borrow_and_update());
        update_grpc_health_status(&mut health_reporter, is_serving).await;
        spawn_grpc_health_updater(health_reporter, status_watch, is_serving);

        server_builder
            .add_service(service)
            .add_service(health_service)
            .serve_with_shutdown(grpc_address, interrupt_signal.map(|_| ()))
            .await
    } else {
        server_builder
            .add_service(service)
            .serve_with_shutdown(grpc_address, interrupt_signal.map(|_| ()))
            .await
    }
    .map_err(|err| {
        error!(target: LOG_TARGET, "GRPC encountered an error: {err:?}");
        err
    })?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}

fn spawn_grpc_health_updater(
    mut health_reporter: HealthReporter,
    mut status_watch: tokio::sync::watch::Receiver<StatusInfo>,
    mut last_is_serving: bool,
) {
    task::spawn(async move {
        loop {
            if status_watch.changed().await.is_err() {
                break;
            }
            let is_serving = is_grpc_health_serving(&status_watch.borrow());
            if is_serving == last_is_serving {
                continue;
            }
            update_grpc_health_status(&mut health_reporter, is_serving).await;
            last_is_serving = is_serving;
        }
    });
}

async fn update_grpc_health_status(health_reporter: &mut HealthReporter, is_serving: bool) {
    health_reporter
        .set_service_status(GRPC_HEALTH_OVERALL_SERVICE_NAME, grpc_serving_status(is_serving))
        .await;
    health_reporter
        .set_service_status(BASE_NODE_GRPC_SERVICE_NAME, grpc_serving_status(is_serving))
        .await;
}

fn grpc_serving_status(is_serving: bool) -> ServingStatus {
    if is_serving {
        ServingStatus::Serving
    } else {
        ServingStatus::NotServing
    }
}

fn is_grpc_health_serving(status: &StatusInfo) -> bool {
    status.bootstrapped && status.state_info.is_synced()
}

/// Prepares the parameters required to call the `run_grpc` function
async fn prepare_grpc_params(
    config: &ApplicationConfig,
) -> Result<(Multiaddr, GrpcAuthentication, Option<Identity>), ExitError> {
    let grpc_address = config.base_node.grpc_address.clone().unwrap_or_else(|| {
        let port = grpc_default_port(ApplicationType::BaseNode, config.base_node.network);
        format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap()
    });

    let auth = config.base_node.grpc_authentication.clone();

    let mut tls_identity = None;
    if config.base_node.grpc_tls_enabled {
        tls_identity = read_identity(config.base_node.config_dir.clone())
            .await
            .map(Some)
            .map_err(|e| ExitError::new(ExitCode::TlsConfigurationError, e.to_string()))?;
    }

    Ok((grpc_address, auth, tls_identity))
}
