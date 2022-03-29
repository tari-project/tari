// Copyright 2019. The Tari Project
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
#![deny(clippy::needless_borrow)]

/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣶⣿⣿⣿⣿⣶⣦⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣤⣾⣿⡿⠋⠀⠀⠀⠀⠉⠛⠿⣿⣿⣶⣤⣀⠀⠀⠀⠀⠀⠀⢰⣿⣾⣾⣾⣾⣾⣾⣾⣾⣾⣿⠀⠀⠀⣾⣾⣾⡀⠀⠀⠀⠀⢰⣾⣾⣾⣾⣿⣶⣶⡀⠀⠀⠀⢸⣾⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⣿⣿⣶⣶⣤⣄⡀⠀⠀⠀⠀⠀⠉⠛⣿⣿⠀⠀⠀⠀⠀⠈⠉⠉⠉⠉⣿⣿⡏⠉⠉⠉⠉⠀⠀⣰⣿⣿⣿⣿⠀⠀⠀⠀⢸⣿⣿⠉⠉⠉⠛⣿⣿⡆⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⠀⠀⠀⠈⠙⣿⡿⠿⣿⣿⣿⣶⣶⣤⣤⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⠀⠀⢠⣿⣿⠃⣿⣿⣷⠀⠀⠀⢸⣿⣿⣀⣀⣀⣴⣿⣿⠃⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⣤⠀⠀⠀⢸⣿⡟⠀⠀⠀⠀⠀⠉⣽⣿⣿⠟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⠀⠀⣿⣿⣿⣤⣬⣿⣿⣆⠀⠀⢸⣿⣿⣿⣿⣿⡿⠟⠉⠀⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⣿⣿⣤⠀⢸⣿⡟⠀⠀⠀⣠⣾⣿⡿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⠀⣾⣿⣿⠿⠿⠿⢿⣿⣿⡀⠀⢸⣿⣿⠙⣿⣿⣿⣄⠀⠀⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⣿⣿⣼⣿⡟⣀⣶⣿⡿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⣰⣿⣿⠃⠀⠀⠀⠀⣿⣿⣿⠀⢸⣿⣿⠀⠀⠙⣿⣿⣷⣄⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⣿⣿⣿⣿⠛⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
/// ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀
///
/// # Tari Base Node
///
/// The Tari Base Node is a major application in the Tari Network
///
/// ## Running the Tari Base Node
///
/// Tor needs to be started first
/// ```
/// tor --allow-missing-torrc --ignore-missing-torrc \
///  --clientonly 1 --socksport 9050 --controlport 127.0.0.1:9051 \
///  --log "warn stdout" --clientuseipv6 1
/// ```
///
/// For the first run
/// `cargo run tari_base_node -- --create-id`
///
/// Subsequent runs
/// `cargo run tari_base_node`
///
/// ## Commands
///
/// `help` - Displays a list of commands
/// `get-balance` - Displays the balance of the wallet (available, pending incoming, pending outgoing)
/// `send-tari` - Sends Tari, the amount needs to be specified, followed by the destination (public key or emoji id) and
/// an optional message `get-chain-metadata` - Lists information about the blockchain of this Base Node
/// `list-peers` - Lists information about peers known by this base node
/// `ban-peer` - Bans a peer
/// `unban-peer` - Removes a ban for a peer
/// `list-connections` - Lists active connections to this Base Node
/// `list-headers` - Lists header information. Either the first header height and the last header height needs to be
/// specified, or the amount of headers from the top `check-db` - Checks the blockchain database for missing blocks and
/// headers `calc-timing` - Calculates the time average time taken to mine a given range of blocks
/// `discover-peer` - Attempts to discover a peer on the network, a public key or emoji id needs to be specified
/// `get-block` - Retrieves a block, the height of the block needs to be specified
/// `get-mempool-stats` - Displays information about the mempool
/// `get-mempool-state` - Displays state information for the mempool
/// `whoami` - Displays identity information about this Base Node and it's wallet
/// `quit` - Exits the Base Node
/// `exit` - Same as quit

/// Used to display tabulated data
#[macro_use]
mod table;

mod bootstrap;
mod builder;
mod cli;
mod commands;
mod grpc;
mod recovery;
mod utils;

mod base_node_config;
#[cfg(feature = "metrics")]
mod metrics;

use std::{env, net::SocketAddr, process, sync::Arc};

use clap::Parser as ClapParser;
use commands::{
    cli_loop::CliLoop,
    command::CommandContext,
    parser::Parser,
    reader::{CommandEvent, CommandReader},
    status_line::StatusLineOutput,
};
use futures::FutureExt;
use log::*;
use opentelemetry::{self, global, KeyValue};
use tari_app_utilities::{
    consts,
    identity_management::setup_node_identity,
    initialization::init_configuration,
    utilities::setup_runtime,
};
#[cfg(all(unix, feature = "libtor"))]
use tari_common::CommsTransport;
use tari_common::{
    configuration::{bootstrap::ApplicationType, CommonConfig},
    exit_codes::{ExitCode, ExitError},
    initialize_logging,
    load_configuration,
    ConfigBootstrap,
    DefaultConfigLoader,
    GlobalConfig,
};
use tari_comms::{
    peer_manager::PeerFeatures,
    tor::HiddenServiceControllerError,
    utils::multiaddr::multiaddr_to_socketaddr,
    NodeIdentity,
};
use tari_core::chain_storage::ChainStorageError;
#[cfg(all(unix, feature = "libtor"))]
use tari_libtor::tor::Tor;
use tari_p2p::auto_update::AutoUpdateConfig;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::task;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, Registry};

use crate::{base_node_config::BaseNodeConfig, cli::Cli};

const LOG_TARGET: &str = "tari::base_node::app";

/// Application entry point
fn main() {
    if let Err(err) = main_inner() {
        eprintln!("{:?}", err);
        let exit_code = err.exit_code;
        eprintln!("{}", exit_code.hint());
        error!(
            target: LOG_TARGET,
            "Exiting with code ({}): {:?}", exit_code as i32, err
        );
        process::exit(exit_code as i32);
    }
}

fn main_inner() -> Result<(), ExitError> {
    let cli = Cli::parse();

    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path.as_path(), true, &cli.config_property_overrides())?;
    initialize_logging(
        &cli.common.log_config_path("base_node"),
        include_str!("../log4rs_sample.yml"),
    )?;
    // let base_node_config = <BaseNodeConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load config");
    // debug!(target: LOG_TARGET, "Configuration: {:?}", config);

    // #[allow(unused_mut)] // config isn't mutated on windows
    // let (bootstrap, global_config, cfg) = init_configuration(ApplicationType::BaseNode)?;

    let common_config = <CommonConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load config");
    let mut base_node_config = <BaseNodeConfig as DefaultConfigLoader>::load_from(&cfg)?;
    base_node_config.set_base_path(common_config.base_path().as_path());
    debug!(
        target: LOG_TARGET,
        "Using base node configuration: {:?}", base_node_config
    );
    let auto_update_config =
        <AutoUpdateConfig as DefaultConfigLoader>::load_from(&cfg).expect("Failed to load auto-update config");

    // Load or create the Node identity
    let node_identity = setup_node_identity(
        base_node_config.identity_file.as_path(),
        &base_node_config.public_address,
        cli.create_id,
        PeerFeatures::COMMUNICATION_NODE,
    )?;

    // Exit if create_id or init arguments were run
    if cli.create_id {
        info!(
            target: LOG_TARGET,
            "Base node's node ID created at '{}'. Done.",
            base_node_config.identity_file.as_path().to_string_lossy(),
        );
        return Ok(());
    }

    if cli.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }

    // The shutdown trigger for the system
    let shutdown = Shutdown::new();

    // Set up the Tokio runtime
    let runtime = setup_runtime()?;

    // Run our own Tor instance, if configured
    // This is currently only possible on linux/macos
    #[cfg(all(unix, feature = "libtor"))]
    if config.base_node_use_libtor && matches!(config.comms_transport, CommsTransport::TorHiddenService { .. }) {
        let tor = Tor::initialize()?;
        config.comms_transport = tor.update_comms_transport(config.comms_transport)?;
        runtime.spawn(tor.run(shutdown.to_signal()));
        debug!(
            target: LOG_TARGET,
            "Updated Tor comms transport: {:?}", config.comms_transport
        );
    }

    // Run the base node
    runtime.block_on(run_node(
        node_identity,
        Arc::new(base_node_config),
        &common_config,
        auto_update_config,
        &cfg,
        cli,
        shutdown,
    ))?;

    // Shutdown and send any traces
    global::shutdown_tracer_provider();

    Ok(())
}

/// Sets up the base node and runs the cli_loop
async fn run_node(
    node_identity: Arc<NodeIdentity>,
    base_node_config: Arc<BaseNodeConfig>,
    common_config: &CommonConfig,
    auto_update_config: AutoUpdateConfig,
    cfg: &Config,
    cli: Cli,
    shutdown: Shutdown,
) -> Result<(), ExitError> {
    if cli.tracing_enabled {
        enable_tracing();
    }

    #[cfg(feature = "metrics")]
    {
        use crate::metrics::MetricsConfig;
        let config = <MetricsConfig as DefaultConfigLoader>::load_from(cfg)?;
        metrics::install(ApplicationType::BaseNode, &node_identity, &config, shutdown.to_signal());
    }

    log_mdc::insert("node-public-key", node_identity.public_key().to_string());
    log_mdc::insert("node-id", node_identity.node_id().to_string());

    if cli.rebuild_db {
        info!(target: LOG_TARGET, "Node is in recovery mode, entering recovery");
        recovery::initiate_recover_db(&base_node_config, common_config)?;
        recovery::run_recovery(&base_node_config, common_config)
            .await
            .map_err(|e| ExitError::new(ExitCode::RecoveryError, e))?;
        return Ok(());
    };

    // Build, node, build!
    let ctx = builder::configure_and_initialize_node(
        base_node_config.clone(),
        common_config,
        auto_update_config,
        node_identity,
        shutdown.to_signal(),
    )
    .await
    .map_err(|err| {
        for boxed_error in err.chain() {
            if let Some(HiddenServiceControllerError::TorControlPortOffline) = boxed_error.downcast_ref() {
                return ExitCode::TorOffline.into();
            }
            if let Some(ChainStorageError::DatabaseResyncRequired(reason)) = boxed_error.downcast_ref() {
                return ExitError::new(
                    ExitCode::DbInconsistentState,
                    format!("You may need to resync your database because {}", reason),
                );
            }

            // todo: find a better way to do this
            if boxed_error.to_string().contains("Invalid force sync peer") {
                println!("Please check your force sync peers configuration");
                return ExitError::new(ExitCode::ConfigError, boxed_error);
            }
        }
        ExitError::new(ExitCode::UnknownError, err)
    })?;

    if let Some(ref address) = base_node_config.grpc_address {
        // Go, GRPC, go go
        let grpc = crate::grpc::base_node_grpc_server::BaseNodeGrpcServer::from_base_node_context(&ctx);
        let socket_addr = multiaddr_to_socketaddr(address).map_err(|e| ExitError::new(ExitCode::ConfigError, e))?;
        task::spawn(run_grpc(grpc, socket_addr, shutdown.to_signal()));
    }

    // Run, node, run!
    let context = CommandContext::new(&ctx, shutdown);
    let main_loop = CliLoop::new(context, bootstrap.watch, bootstrap.non_interactive_mode);
    if cli.non_interactive_mode {
        println!("Node started in non-interactive mode (pid = {})", process::id());
    } else {
        info!(
            target: LOG_TARGET,
            "Node has been successfully configured and initialized. Starting CLI loop."
        );
    }
    task::spawn(main_loop.cli_loop());
    if !base_node_config.force_sync_peers.is_empty() {
        warn!(
            target: LOG_TARGET,
            "Force Sync Peers have been set! This node will only sync to the nodes in this set."
        );
    }

    ctx.run().await;

    println!("Goodbye!");
    Ok(())
}

fn enable_tracing() {
    // To run:
    // docker run -d -p6831:6831/udp -p6832:6832/udp -p16686:16686 -p14268:14268 jaegertracing/all-in-one:latest
    // To view the UI after starting the container (default):
    // http://localhost:16686
    global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("tari::base_node")
        .with_tags(vec![
            KeyValue::new("pid", process::id().to_string()),
            KeyValue::new(
                "current_exe",
                env::current_exe().unwrap().to_str().unwrap_or_default().to_owned(),
            ),
            KeyValue::new("version", consts::APP_VERSION),
        ])
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default().with(telemetry);
    tracing::subscriber::set_global_default(subscriber)
        .expect("Tracing could not be set. Try running without `--tracing-enabled`");
}

/// Runs the gRPC server
async fn run_grpc(
    grpc: crate::grpc::base_node_grpc_server::BaseNodeGrpcServer,
    grpc_address: SocketAddr,
    interrupt_signal: ShutdownSignal,
) -> Result<(), anyhow::Error> {
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_address);

    Server::builder()
        .add_service(tari_app_grpc::tari_rpc::base_node_server::BaseNodeServer::new(grpc))
        .serve_with_shutdown(grpc_address, interrupt_signal.map(|_| ()))
        .await
        .map_err(|err| {
            error!(target: LOG_TARGET, "GRPC encountered an error: {}", err);
            err
        })?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
