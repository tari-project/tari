#![recursion_limit = "1024"]
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
//#![cfg_attr(not(debug_assertions), deny(unused_variables))]
//#![cfg_attr(not(debug_assertions), deny(unused_imports))]
//#![cfg_attr(not(debug_assertions), deny(dead_code))]
//#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
//#![deny(unused_must_use)]
//#![deny(unreachable_patterns)]
//#![deny(unknown_lints)]
// Enable 'impl Trait' type aliases
#![feature(type_alias_impl_trait)]

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
/// It consists of the Base Node itself, a Wallet and a Miner
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
/// ```cargo run tari_base_node -- --create-id```
/// 
/// Subsequent runs
/// ```cargo run tari_base_node```
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
/// `toggle-mining` - Turns the miner on or off
/// `quit` - Exits the Base Node
/// `exit` - Same as quit

/// Used to display tabulated data
#[macro_use]
mod table;

/// Base node bootstrap code
mod bootstrap;
/// Utilities and helpers for building the base node instance
mod builder;
/// The command line interface definition and configuration
mod cli;
/// Application-specific constants
mod grpc;
/// Miner lib Todo hide behind feature flag
mod miner;
/// Parser module used to control user commands
mod parser;
/// Misc. long running base node tasks
mod tasks;
/// Utility functions
mod utils;
// recovery mode
mod recovery;

use futures::FutureExt;
use log::*;
use parser::Parser;
use rustyline::{config::OutputStreamType, error::ReadlineError, CompletionType, Config, EditMode, Editor};
use std::net::SocketAddr;
use structopt::StructOpt;
use tari_app_utilities::{
    identity_management::setup_node_identity,
    utilities::{setup_runtime, ExitCodes},
};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};
use tari_comms::peer_manager::PeerFeatures;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tonic::transport::Server;

pub const LOG_TARGET: &str = "base_node::app";

#[cfg(feature = "winservice")]
extern crate windows_service;

#[cfg(feature = "winservice")]
use std::{ffi::OsString, sync::mpsc, time::Duration};

use std::thread;
#[cfg(feature = "winservice")]
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    Result as ServiceResult,
};

#[cfg(feature = "winservice")]
const SERVICE_NAME: &str = "tari_base_node";
#[cfg(feature = "winservice")]
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

#[cfg(feature = "winservice")]
pub fn run() -> ServiceResult<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

#[cfg(feature = "winservice")]
define_windows_service!(ffi_service_main, service_main);

#[cfg(feature = "winservice")]
pub fn service_main(_arguments: Vec<OsString>) {
    if let Err(_e) = run_service() {
        // Handle the error
    }
}

#[cfg(feature = "winservice")]
pub fn run_service() -> ServiceResult<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                shutdown_tx.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            },

            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::new(5, 0),
        process_id: None,
    })?;

    let handle = thread::spawn(move || {
        let mut bootstrap = ConfigBootstrap::from_args();

        // Check and initialize configuration files
        bootstrap.init_dirs(ApplicationType::BaseNode).unwrap();

        // Load and apply configuration file
        let cfg = bootstrap.load_configuration().unwrap();

        // Initialise the logger
        bootstrap.initialize_logging().unwrap();

        // Populate the configuration struct
        let node_config = GlobalConfig::convert_from(cfg)
            .map_err(|err| {
                error!(target: LOG_TARGET, "The configuration file has an error. {}", err);
                ExitCodes::ConfigError(format!("The configuration file has an error. {}", err))
            })
            .unwrap();

        debug!(target: LOG_TARGET, "Using configuration: {:?}", node_config);

        // Set up the Tokio runtime
        let mut rt = setup_runtime(&node_config)
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                ExitCodes::UnknownError
            })
            .unwrap();

        // Load or create the Node identity
        let wallet_identity = setup_node_identity(
            &node_config.wallet_identity_file,
            &node_config.public_address,
            bootstrap.create_id ||
                // If the base node identity exists, we want to be sure that the wallet identity exists
                node_config.identity_file.exists(),
            PeerFeatures::COMMUNICATION_CLIENT,
        )
        .unwrap();

        let node_identity = setup_node_identity(
            &node_config.identity_file,
            &node_config.public_address,
            bootstrap.create_id,
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap();

        // This is the main and only shutdown trigger for the system.
        let shutdown = Shutdown::new();

        // Build, node, build!
        let ctx = rt
            .block_on(builder::configure_and_initialize_node(
                &node_config,
                node_identity,
                wallet_identity,
                shutdown.to_signal(),
                bootstrap.clean_orphans_db,
            ))
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                ExitCodes::UnknownError
            })
            .unwrap();

        if node_config.grpc_enabled {
            // Go, GRPC , go go
            let grpc = crate::grpc::base_node_grpc_server::BaseNodeGrpcServer::new(
                rt.handle().clone(),
                ctx.local_node(),
                node_config.clone(),
            );

            rt.spawn(run_grpc(grpc, node_config.grpc_address, shutdown.to_signal()));
        }

        // Run, node, run!
        let base_node_handle = rt.spawn(ctx.run());

        info!(
            target: LOG_TARGET,
            "Node has been successfully configured and initialized. Starting CLI loop."
        );

        match rt.block_on(base_node_handle) {
            Ok(_) => info!(target: LOG_TARGET, "Node shutdown successfully."),
            Err(e) => error!(target: LOG_TARGET, "Node has crashed: {}", e),
        }

        // Wait until tasks have shut down
        drop(rt);

    });

    loop {
        match shutdown_rx.recv_timeout(Duration::from_secs(5)) {
            // Break the loop either upon stop or channel disconnect
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => break,

            // Continue work if no events were received within the timeout
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        };
    }

    handle.join().unwrap();

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::new(5, 0),
        process_id: None,
    })?;

    Ok(())
}

#[cfg(feature = "winservice")]
fn main() -> windows_service::Result<()> {
    run()
}

/// Application entry point
#[cfg(not(feature = "winservice"))]
fn main() {
    match main_inner() {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => std::process::exit(exit_code.as_i32()),
    }
}

/// Sets up the base node and runs the cli_loop
fn main_inner() -> Result<(), ExitCodes> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(ApplicationType::BaseNode)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    // Populate the configuration struct
    let node_config = GlobalConfig::convert_from(cfg).map_err(|err| {
        error!(target: LOG_TARGET, "The configuration file has an error. {}", err);
        ExitCodes::ConfigError(format!("The configuration file has an error. {}", err))
    })?;

    debug!(target: LOG_TARGET, "Using configuration: {:?}", node_config);

    // Set up the Tokio runtime
    let mut rt = setup_runtime(&node_config).map_err(|err| {
        error!(target: LOG_TARGET, "{}", err);
        ExitCodes::UnknownError
    })?;

    // Load or create the Node identity
    let wallet_identity = setup_node_identity(
        &node_config.wallet_identity_file,
        &node_config.public_address,
        bootstrap.create_id ||
            // If the base node identity exists, we want to be sure that the wallet identity exists
            node_config.identity_file.exists(),
        PeerFeatures::COMMUNICATION_CLIENT,
    )?;

    let node_identity = setup_node_identity(
        &node_config.identity_file,
        &node_config.public_address,
        bootstrap.create_id,
        PeerFeatures::COMMUNICATION_NODE,
    )?;

    // Exit if create_id or init arguments were run
    if bootstrap.create_id {
        info!(
            target: LOG_TARGET,
            "Node ID created at '{}'. Done.",
            node_config.identity_file.to_string_lossy()
        );
        return Ok(());
    }

    // This is the main and only shutdown trigger for the system.
    let shutdown = Shutdown::new();

    if bootstrap.rebuild_db {
        info!(target: LOG_TARGET, "Node is in recovery mode, entering recovery");
        recovery::initiate_recover_db(&node_config)?;
        let _ = rt.block_on(recovery::run_recovery(&node_config));
        return Ok(());
    };

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }

    // Build, node, build!
    let ctx = rt
        .block_on(builder::configure_and_initialize_node(
            &node_config,
            node_identity,
            wallet_identity,
            shutdown.to_signal(),
            bootstrap.clean_orphans_db,
        ))
        .map_err(|err| {
            error!(target: LOG_TARGET, "{}", err);
            ExitCodes::UnknownError
        })?;

    if node_config.grpc_enabled {
        // Go, GRPC , go go
        let grpc = crate::grpc::base_node_grpc_server::BaseNodeGrpcServer::new(
            rt.handle().clone(),
            ctx.local_node(),
            node_config.clone(),
        );

        rt.spawn(run_grpc(grpc, node_config.grpc_address, shutdown.to_signal()));
    }

    // Run, node, run!
    let parser = Parser::new(rt.handle().clone(), &ctx, &node_config);
    cli::print_banner(parser.get_commands(), 3);
    let base_node_handle = rt.spawn(ctx.run());

    info!(
        target: LOG_TARGET,
        "Node has been successfully configured and initialized. Starting CLI loop."
    );

    #[cfg(not(feature = "winservice"))]
    cli_loop(parser, shutdown);

    match rt.block_on(base_node_handle) {
        Ok(_) => info!(target: LOG_TARGET, "Node shutdown successfully."),
        Err(e) => error!(target: LOG_TARGET, "Node has crashed: {}", e),
    }

    // Wait until tasks have shut down
    drop(rt);

    println!("Goodbye!");
    Ok(())
}

/// Runs the gRPC server
async fn run_grpc(
    grpc: crate::grpc::base_node_grpc_server::BaseNodeGrpcServer,
    grpc_address: SocketAddr,
    interrupt_signal: ShutdownSignal,
) -> Result<(), anyhow::Error>
{
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_address);

    Server::builder()
        .add_service(tari_app_grpc::tari_rpc::base_node_server::BaseNodeServer::new(grpc))
        .serve_with_shutdown(grpc_address, interrupt_signal.map(|_| ()))
        .await?;

    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}

/// Runs the Base Node
/// ## Parameters
/// `parser` - The parser to process input commands
/// `shutdown` - The trigger for shutting down
///
/// ## Returns
/// Doesn't return anything
fn cli_loop(parser: Parser, mut shutdown: Shutdown) {
    let cli_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .output_stream(OutputStreamType::Stdout)
        .build();
    let mut rustyline = Editor::with_config(cli_config);
    rustyline.set_helper(Some(parser));
    loop {
        let readline = rustyline.readline(">> ");
        match readline {
            Ok(line) => {
                rustyline.add_history_entry(line.as_str());
                if let Some(p) = rustyline.helper_mut().as_deref_mut() {
                    p.handle_command(&line, &mut shutdown)
                }
            },
            Err(ReadlineError::Interrupted) => {
                // shutdown section. Will shutdown all interfaces when ctrl-c was pressed
                println!("The node is shutting down because Ctrl+C was received...");
                info!(
                    target: LOG_TARGET,
                    "Termination signal received from user. Shutting node down."
                );
                if shutdown.trigger().is_err() {
                    error!(target: LOG_TARGET, "Shutdown signal failed to trigger");
                };
                break;
            },
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            },
        }
        if shutdown.is_triggered() {
            break;
        };
    }
}
