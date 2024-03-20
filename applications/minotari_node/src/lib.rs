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
mod grpc;
#[cfg(feature = "metrics")]
mod metrics;
mod recovery;
mod utils;

use std::{process, sync::Arc};

use commands::{cli_loop::CliLoop, command::CommandContext};
use futures::FutureExt;
use log::*;
use minotari_app_grpc::{authentication::ServerAuthenticationInterceptor, tls::identity::read_identity};
use minotari_app_utilities::common_cli_args::CommonCliArgs;
use tari_common::{
    configuration::bootstrap::{grpc_default_port, ApplicationType},
    exit_codes::{ExitCode, ExitError},
};
use tari_common_types::grpc_authentication::GrpcAuthentication;
use tari_comms::{multiaddr::Multiaddr, utils::multiaddr::multiaddr_to_socketaddr, NodeIdentity};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::task;
use tonic::transport::{Identity, Server, ServerTlsConfig};

use crate::cli::Cli;
pub use crate::config::{ApplicationConfig, BaseNodeConfig, DatabaseType};
#[cfg(feature = "metrics")]
pub use crate::metrics::MetricsConfig;

const LOG_TARGET: &str = "minotari::base_node::app";

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
            log_level: None,
            network: None,
            config_property_overrides: vec![],
        },
        init: true,
        rebuild_db: false,
        non_interactive_mode: true,
        watch: None,
        profile_with_tokio_console: false,
        grpc_enabled: false,
        mining_enabled: false,
        second_layer_grpc_enabled: false,
    };

    run_base_node_with_cli(node_identity, config, cli, shutdown).await
}

/// Sets up the base node and runs the cli_loop
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

    log_mdc::insert("node-public-key", node_identity.public_key().to_string());
    log_mdc::insert("node-id", node_identity.node_id().to_string());
    if let Some(grpc) = config.base_node.grpc_address.as_ref() {
        log_mdc::insert("grpc", grpc.to_string());
    }

    if cli.rebuild_db {
        info!(target: LOG_TARGET, "Node is in recovery mode, entering recovery");
        recovery::initiate_recover_db(&config.base_node)?;
        recovery::run_recovery(&config.base_node)
            .await
            .map_err(|e| ExitError::new(ExitCode::RecoveryError, e))?;
        return Ok(());
    };

    // Build, node, build!
    let ctx = builder::configure_and_initialize_node(config.clone(), node_identity, shutdown.to_signal()).await?;

    if config.base_node.grpc_enabled {
        let grpc_address = config.base_node.grpc_address.clone().unwrap_or_else(|| {
            let port = grpc_default_port(ApplicationType::BaseNode, config.base_node.network);
            format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap()
        });
        // Go, GRPC, go go
        let grpc =
            grpc::base_node_grpc_server::BaseNodeGrpcServer::from_base_node_context(&ctx, config.base_node.clone());
        let auth = config.base_node.grpc_authentication.clone();

        let mut tls_identity = None;
        if config.base_node.grpc_tls_enabled {
            tls_identity = read_identity(config.base_node.config_dir.clone())
                .await
                .map(Some)
                .map_err(|e| ExitError::new(ExitCode::TlsConfigurationError, e.to_string()))?;
        }
        task::spawn(run_grpc(grpc, grpc_address, auth, tls_identity, shutdown.to_signal()));
    }

    // Run, node, run!
    let context = CommandContext::new(&ctx, shutdown);
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
    main_loop.cli_loop().await;

    ctx.wait_for_shutdown().await;

    println!("Goodbye!");
    Ok(())
}

/// Runs the gRPC server
async fn run_grpc(
    grpc: grpc::base_node_grpc_server::BaseNodeGrpcServer,
    grpc_address: Multiaddr,
    auth_config: GrpcAuthentication,
    tls_identity: Option<Identity>,
    interrupt_signal: ShutdownSignal,
) -> Result<(), anyhow::Error> {
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_address);

    let grpc_address = multiaddr_to_socketaddr(&grpc_address)?;
    let auth = ServerAuthenticationInterceptor::new(auth_config)
        .ok_or(anyhow::anyhow!("Unable to prepare server gRPC authentication"))?;
    let service = minotari_app_grpc::tari_rpc::base_node_server::BaseNodeServer::with_interceptor(grpc, auth);

    let mut server_builder = if let Some(identity) = tls_identity {
        Server::builder().tls_config(ServerTlsConfig::new().identity(identity))?
    } else {
        Server::builder()
    };

    server_builder
        .add_service(service)
        .serve_with_shutdown(grpc_address, interrupt_signal.map(|_| ()))
        .await
        .map_err(|err| {
            error!(target: LOG_TARGET, "GRPC encountered an error: {:?}", err);
            err
        })?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
