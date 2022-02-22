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
mod commands;
mod grpc;
mod recovery;
mod utils;

#[cfg(feature = "metrics")]
mod metrics;

use std::{
    env,
    net::SocketAddr,
    process,
    sync::Arc,
    time::{Duration, Instant},
};

use commands::{
    command_handler::{CommandHandler, StatusLineOutput},
    parser::Parser,
    performer::Performer,
    reader::{CommandEvent, CommandReader},
};
use futures::FutureExt;
use log::*;
use opentelemetry::{self, global, KeyValue};
use rustyline::{config::OutputStreamType, CompletionType, Config, EditMode, Editor};
use tari_app_utilities::{
    consts,
    identity_management::setup_node_identity,
    initialization::init_configuration,
    utilities::setup_runtime,
};
#[cfg(all(unix, feature = "libtor"))]
use tari_common::CommsTransport;
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
    ConfigBootstrap,
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
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{task, time};
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, Registry};

const LOG_TARGET: &str = "base_node::app";

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
    #[allow(unused_mut)] // config isn't mutated on windows
    let (bootstrap, mut config, _) = init_configuration(ApplicationType::BaseNode)?;
    debug!(target: LOG_TARGET, "Using configuration: {:?}", config);

    // Load or create the Node identity
    let node_identity = setup_node_identity(
        &config.base_node_identity_file,
        &config.comms_public_address,
        bootstrap.create_id,
        PeerFeatures::COMMUNICATION_NODE,
    )?;

    // Exit if create_id or init arguments were run
    if bootstrap.create_id {
        info!(
            target: LOG_TARGET,
            "Base node's node ID created at '{}'. Done.",
            config.base_node_identity_file.to_string_lossy(),
        );
        return Ok(());
    }

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }

    // The shutdown trigger for the system
    let shutdown = Shutdown::new();

    // Set up the Tokio runtime
    let runtime = setup_runtime(&config)?;

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
    runtime.block_on(run_node(node_identity, config.into(), bootstrap, shutdown))?;

    // Shutdown and send any traces
    global::shutdown_tracer_provider();

    Ok(())
}

/// Sets up the base node and runs the cli_loop
async fn run_node(
    node_identity: Arc<NodeIdentity>,
    config: Arc<GlobalConfig>,
    bootstrap: ConfigBootstrap,
    shutdown: Shutdown,
) -> Result<(), ExitError> {
    if bootstrap.tracing_enabled {
        enable_tracing();
    }

    #[cfg(feature = "metrics")]
    {
        metrics::install(
            ApplicationType::BaseNode,
            &node_identity,
            &config,
            &bootstrap,
            shutdown.to_signal(),
        );
    }

    log_mdc::insert("node-public-key", node_identity.public_key().to_string());
    log_mdc::insert("node-id", node_identity.node_id().to_string());

    if bootstrap.rebuild_db {
        info!(target: LOG_TARGET, "Node is in recovery mode, entering recovery");
        recovery::initiate_recover_db(&config)?;
        recovery::run_recovery(&config)
            .await
            .map_err(|e| ExitError::new(ExitCode::RecoveryError, e))?;
        return Ok(());
    };

    // Build, node, build!
    let ctx = builder::configure_and_initialize_node(
        config.clone(),
        node_identity,
        shutdown.to_signal(),
        bootstrap.clean_orphans_db,
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

    if let Some(ref base_node_config) = config.base_node_config {
        if let Some(ref address) = base_node_config.grpc_address {
            // Go, GRPC, go go
            let grpc = crate::grpc::base_node_grpc_server::BaseNodeGrpcServer::from_base_node_context(&ctx);
            let socket_addr = multiaddr_to_socketaddr(address).map_err(|e| ExitError::new(ExitCode::ConfigError, e))?;
            task::spawn(run_grpc(grpc, socket_addr, shutdown.to_signal()));
        }
    }

    // Run, node, run!
    let command_handler = CommandHandler::new(&ctx);
    if bootstrap.non_interactive_mode {
        task::spawn(status_loop(command_handler, shutdown));
        println!("Node started in non-interactive mode (pid = {})", process::id());
    } else {
        info!(
            target: LOG_TARGET,
            "Node has been successfully configured and initialized. Starting CLI loop."
        );
        task::spawn(cli_loop(command_handler, config.clone(), shutdown));
    }
    if !config.force_sync_peers.is_empty() {
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

fn get_status_interval(start_time: Instant, long_interval: Duration) -> time::Sleep {
    let duration = match start_time.elapsed().as_secs() {
        0..=120 => Duration::from_secs(5),
        _ => long_interval,
    };
    time::sleep(duration)
}

async fn status_loop(mut command_handler: CommandHandler, shutdown: Shutdown) {
    let start_time = Instant::now();
    let mut shutdown_signal = shutdown.to_signal();
    let status_interval = command_handler.global_config().base_node_status_line_interval;
    loop {
        let interval = get_status_interval(start_time, status_interval);
        tokio::select! {
            biased;
            _ = shutdown_signal.wait() => {
                break;
            }

            _ = interval => {
               command_handler.status(StatusLineOutput::Log).await.ok();
            },
        }
    }
}

/// Runs the Base Node CLI loop
/// ## Parameters
/// `parser` - The parser to process input commands
/// `shutdown` - The trigger for shutting down
///
/// ## Returns
/// Doesn't return anything
async fn cli_loop(command_handler: CommandHandler, config: Arc<GlobalConfig>, mut shutdown: Shutdown) {
    let parser = Parser::new();
    commands::cli::print_banner(parser.get_commands(), 3);

    let mut performer = Performer::new(command_handler);
    let cli_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .output_stream(OutputStreamType::Stdout)
        .auto_add_history(true)
        .build();
    let mut rustyline = Editor::with_config(cli_config);
    rustyline.set_helper(Some(parser));
    let mut reader = CommandReader::new(rustyline);

    let mut shutdown_signal = shutdown.to_signal();
    let start_time = Instant::now();
    let mut software_update_notif = performer.get_software_updater().new_update_notifier().clone();
    let mut first_signal = false;
    // TODO: Add heartbeat here
    // Show status immediately on startup
    let _ = performer.status(StatusLineOutput::StdOutAndLog).await;
    loop {
        let interval = get_status_interval(start_time, config.base_node_status_line_interval);
        tokio::select! {
            res = reader.next_command() => {
                if let Some(event) = res {
                    match event {
                        CommandEvent::Command(line) => {
                            first_signal = false;
                            let fut = performer.handle_command(line.as_str(), &mut shutdown);
                            let res = time::timeout(Duration::from_secs(30), fut).await;
                            if let Err(_err) = res {
                                println!("Time for command execution elapsed: `{}`", line);
                            }
                        }
                        CommandEvent::Interrupt => {
                            if !first_signal {
                                println!("Are you leaving already? Press Ctrl-C again to terminate the node.");
                                first_signal = true;
                            } else {
                                break;
                            }
                        }
                        CommandEvent::Error(err) => {
                            // TODO: Not sure we have to break here
                            // This happens when the node is shutting down.
                            debug!(target:  LOG_TARGET, "Could not read line from rustyline:{}", err);
                            break;
                        }
                    }
                } else {
                    break;
                }
            },
            Ok(_) = software_update_notif.changed() => {
                if let Some(ref update) = *software_update_notif.borrow() {
                    println!(
                        "Version {} of the {} is available: {} (sha: {})",
                        update.version(),
                        update.app(),
                        update.download_url(),
                        update.to_hash_hex()
                    );
                }
            }
            _ = interval => {
                // TODO: Execute `watch` command here + use the result
                let _ = performer.status(StatusLineOutput::StdOutAndLog).await;
            },
            _ = shutdown_signal.wait() => {
                break;
            }
        }
    }
}
