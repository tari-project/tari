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
///  --log "notice stdout" --clientuseipv6 1
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

#[macro_use]
mod macros;

mod bootstrap;
mod builder;
mod cli;
mod command_handler;
mod grpc;
mod parser;
mod recovery;
mod status_line;
mod utils;

use crate::command_handler::{CommandHandler, StatusOutput};
use futures::{pin_mut, FutureExt};
use log::*;
use opentelemetry::{self, global, KeyValue};
use parser::Parser;
use rustyline::{config::OutputStreamType, error::ReadlineError, CompletionType, Config, EditMode, Editor};
use std::{
    env,
    net::SocketAddr,
    process,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_app_utilities::{
    consts,
    identity_management::setup_node_identity,
    initialization::init_configuration,
    utilities::setup_runtime,
};
use tari_common::{configuration::bootstrap::ApplicationType, exit_codes::ExitCodes, ConfigBootstrap, GlobalConfig};
use tari_comms::{peer_manager::PeerFeatures, tor::HiddenServiceControllerError};
use tari_core::chain_storage::ChainStorageError;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{
    runtime,
    task,
    time::{self},
};
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, Registry};

const LOG_TARGET: &str = "base_node::app";
/// Application entry point
fn main() {
    if let Err(exit_code) = main_inner() {
        exit_code.eprint_details();
        error!(
            target: LOG_TARGET,
            "Exiting with code ({}): {:?}",
            exit_code.as_i32(),
            exit_code
        );
        process::exit(exit_code.as_i32());
    }
}

fn main_inner() -> Result<(), ExitCodes> {
    let (bootstrap, node_config, _) = init_configuration(ApplicationType::BaseNode)?;

    debug!(target: LOG_TARGET, "Using configuration: {:?}", node_config);

    // Set up the Tokio runtime
    let rt = setup_runtime(&node_config).map_err(|e| {
        error!(target: LOG_TARGET, "{}", e);
        ExitCodes::UnknownError(e)
    })?;

    rt.block_on(run_node(node_config.into(), bootstrap))?;
    // Shutdown and send any traces
    global::shutdown_tracer_provider();
    Ok(())
}

/// Sets up the base node and runs the cli_loop
async fn run_node(node_config: Arc<GlobalConfig>, bootstrap: ConfigBootstrap) -> Result<(), ExitCodes> {
    if bootstrap.tracing_enabled {
        enable_tracing();
    }
    // Load or create the Node identity
    let node_identity = setup_node_identity(
        &node_config.base_node_identity_file,
        &node_config.public_address,
        bootstrap.create_id,
        PeerFeatures::COMMUNICATION_NODE,
    )?;

    // Exit if create_id or init arguments were run
    if bootstrap.create_id {
        info!(
            target: LOG_TARGET,
            "Base node's node ID created at '{}'. Done.",
            node_config.base_node_identity_file.to_string_lossy(),
        );
        return Ok(());
    }
    // This is the main and only shutdown trigger for the system.
    let shutdown = Shutdown::new();

    if bootstrap.rebuild_db {
        info!(target: LOG_TARGET, "Node is in recovery mode, entering recovery");
        recovery::initiate_recover_db(&node_config)?;
        recovery::run_recovery(&node_config)
            .await
            .map_err(|e| ExitCodes::RecoveryError(e.to_string()))?;
        return Ok(());
    };

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }

    // Build, node, build!
    let ctx = builder::configure_and_initialize_node(
        node_config.clone(),
        node_identity,
        shutdown.to_signal(),
        bootstrap.clean_orphans_db,
    )
    .await
    .map_err(|err| {
        for boxed_error in err.chain() {
            if let Some(HiddenServiceControllerError::TorControlPortOffline) = boxed_error.downcast_ref() {
                return ExitCodes::TorOffline;
            }
            if let Some(ChainStorageError::DatabaseResyncRequired(reason)) = boxed_error.downcast_ref() {
                return ExitCodes::DbInconsistentState(format!(
                    "You may need to resync your database because {}",
                    reason
                ));
            }

            // todo: find a better way to do this
            if boxed_error.to_string().contains("Invalid force sync peer") {
                println!("Please check your force sync peers configuration");
                return ExitCodes::ConfigError(boxed_error.to_string());
            }
        }
        ExitCodes::UnknownError(err.to_string())
    })?;

    if node_config.grpc_enabled {
        // Go, GRPC, go go
        let grpc = crate::grpc::base_node_grpc_server::BaseNodeGrpcServer::from_base_node_context(&ctx);
        task::spawn(run_grpc(grpc, node_config.grpc_base_node_address, shutdown.to_signal()));
    }

    // Run, node, run!
    let command_handler = Arc::new(CommandHandler::new(runtime::Handle::current(), &ctx));
    if bootstrap.non_interactive_mode {
        task::spawn(status_loop(command_handler, shutdown));
        println!("Node started in non-interactive mode (pid = {})", process::id());
    } else {
        let parser = Parser::new(command_handler);
        cli::print_banner(parser.get_commands(), 3);

        info!(
            target: LOG_TARGET,
            "Node has been successfully configured and initialized. Starting CLI loop."
        );

        task::spawn(cli_loop(parser, shutdown));
    }
    if !node_config.force_sync_peers.is_empty() {
        warn!(
            target: LOG_TARGET,
            "Force Sync Peers have been set! This node will only sync to the nodes in this set"
        );
    }

    ctx.run().await;

    println!("Goodbye!");
    Ok(())
}

fn enable_tracing() {
    // To run:
    // docker run -d -p6831:6831/udp -p6832:6832/udp -p16686:16686 -p14268:14268 jaegertracing/all-in-one:latest
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
            error!(target: LOG_TARGET, "GRPC encountered an  error:{}", err);
            err
        })?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}

async fn read_command(mut rustyline: Editor<Parser>) -> Result<(String, Editor<Parser>), String> {
    task::spawn_blocking(|| {
        let readline = rustyline.readline(">> ");

        match readline {
            Ok(line) => {
                rustyline.add_history_entry(line.as_str());
                Ok((line, rustyline))
            },
            Err(ReadlineError::Interrupted) => {
                // shutdown section. Will shutdown all interfaces when ctrl-c was pressed
                println!("The node is shutting down because Ctrl+C was received...");
                info!(
                    target: LOG_TARGET,
                    "Termination signal received from user. Shutting node down."
                );
                Err("Node is shutting down".to_string())
            },
            Err(err) => {
                println!("Error: {:?}", err);
                Err(err.to_string())
            },
        }
    })
    .await
    .expect("Could not spawn rustyline task")
}

fn status_interval(start_time: Instant) -> time::Sleep {
    let duration = match start_time.elapsed().as_secs() {
        0..=120 => Duration::from_secs(5),
        _ => Duration::from_secs(30),
    };
    time::sleep(duration)
}

async fn status_loop(command_handler: Arc<CommandHandler>, shutdown: Shutdown) {
    let start_time = Instant::now();
    let mut shutdown_signal = shutdown.to_signal();
    loop {
        let interval = status_interval(start_time);
        tokio::select! {
            biased;
            _ = shutdown_signal.wait() => {
                break;
            }

            _ = interval => {
               command_handler.status(StatusOutput::Log);
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
async fn cli_loop(parser: Parser, mut shutdown: Shutdown) {
    let cli_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .output_stream(OutputStreamType::Stdout)
        .build();
    let mut rustyline = Editor::with_config(cli_config);
    let command_handler = parser.get_command_handler();
    rustyline.set_helper(Some(parser));
    let read_command_fut = read_command(rustyline).fuse();
    pin_mut!(read_command_fut);

    let mut shutdown_signal = shutdown.to_signal();
    let start_time = Instant::now();
    let mut software_update_notif = command_handler.get_software_updater().new_update_notifier().clone();
    loop {
        let interval = status_interval(start_time);
        tokio::select! {
            res = &mut read_command_fut => {
                match res {
                    Ok((line, mut rustyline)) => {
                        if let Some(p) = rustyline.helper_mut().as_deref_mut() {
                            p.handle_command(line.as_str(), &mut shutdown);
                        }
                        if !shutdown.is_triggered() {
                            read_command_fut.set(read_command(rustyline).fuse());
                        }
                    },
                    Err(err) => {
                        // This happens when the node is shutting down.
                        debug!(target:  LOG_TARGET, "Could not read line from rustyline:{}", err);
                        break;
                    }
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
               command_handler.status(StatusOutput::Full);
            },
            _ = shutdown_signal.wait() => {
                break;
            }
        }
    }
}
