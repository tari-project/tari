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
/// ```cargo run tari_base_node -- --create_id```
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

/// Utilities and helpers for building the base node instance
mod builder;
/// The command line interface definition and configuration
mod cli;
/// Miner lib Todo hide behind feature flag
mod miner;
/// Parser module used to control user commands
mod parser;
mod utils;

use crate::builder::{create_new_base_node_identity, load_identity};
use log::*;
use parser::Parser;
use rustyline::{config::OutputStreamType, error::ReadlineError, CompletionType, Config, EditMode, Editor};
use std::{path::PathBuf, sync::Arc};
use structopt::StructOpt;
use tari_common::{ConfigBootstrap, GlobalConfig};
use tari_comms::{multiaddr::Multiaddr, peer_manager::PeerFeatures, NodeIdentity};
use tari_shutdown::Shutdown;
use tokio::runtime::Runtime;

pub const LOG_TARGET: &str = "base_node::app";

/// Enum to show failure information
enum ExitCodes {
    ConfigError = 101,
    UnknownError = 102,
}

impl From<tari_common::ConfigError> for ExitCodes {
    fn from(err: tari_common::ConfigError) -> Self {
        error!(target: LOG_TARGET, "{}", err);
        Self::ConfigError
    }
}

/// Application entry point
fn main() {
    match main_inner() {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => std::process::exit(exit_code as i32),
    }
}

/// Sets up the base node and runs the cli_loop
fn main_inner() -> Result<(), ExitCodes> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs()?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    // Populate the configuration struct
    let node_config = GlobalConfig::convert_from(cfg).map_err(|err| {
        error!(target: LOG_TARGET, "The configuration file has an error. {}", err);
        ExitCodes::ConfigError
    })?;

    trace!(target: LOG_TARGET, "Using configuration: {:?}", node_config);

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

    // Build, node, build!
    let shutdown = Shutdown::new();
    let ctx = rt
        .block_on(builder::configure_and_initialize_node(
            &node_config,
            node_identity,
            wallet_identity,
            shutdown.to_signal(),
        ))
        .map_err(|err| {
            error!(target: LOG_TARGET, "{}", err);
            ExitCodes::UnknownError
        })?;

    // Exit if create_id or init arguments were run
    if bootstrap.create_id {
        info!(
            target: LOG_TARGET,
            "Node ID created at '{}'. Done.",
            node_config.identity_file.to_string_lossy()
        );
        return Ok(());
    }

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }

    // Run, node, run!
    let parser = Parser::new(rt.handle().clone(), &ctx);

    cli::print_banner(parser.get_commands(), 3);

    let base_node_handle = rt.spawn(ctx.run(rt.handle().clone()));

    info!(
        target: LOG_TARGET,
        "Node has been successfully configured and initialized. Starting CLI loop."
    );

    cli_loop(parser, shutdown);

    match rt.block_on(base_node_handle) {
        Ok(_) => info!(target: LOG_TARGET, "Node shutdown successfully."),
        Err(e) => error!(target: LOG_TARGET, "Node has crashed: {}", e),
    }

    println!("Goodbye!");
    Ok(())
}

/// Sets up the tokio runtime based on the configuration
/// ## Parameters
/// `config` - The configuration  of the base node
///
/// ## Returns
/// A result containing the runtime on success, string indicating the error on failure
fn setup_runtime(config: &GlobalConfig) -> Result<Runtime, String> {
    let num_core_threads = config.core_threads;
    let num_blocking_threads = config.blocking_threads;
    let num_mining_threads = config.num_mining_threads;

    debug!(
        target: LOG_TARGET,
        "Configuring the node to run on {} core threads, {} blocking worker threads and {} mining threads.",
        num_core_threads,
        num_blocking_threads,
        num_mining_threads
    );
    tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .max_threads(num_core_threads + num_blocking_threads + num_mining_threads)
        .core_threads(num_core_threads)
        .build()
        .map_err(|e| format!("There was an error while building the node runtime. {}", e.to_string()))
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

/// Loads the node identity, or creates a new one if the --create_id flag was specified
/// ## Parameters
/// `identity_file` - Reference to file path
/// `public_address` - Network address of the base node
/// `create_id` - Whether an identity needs to be created or not
/// `peer_features` - Enables features of the base node
///
/// # Return
/// A NodeIdentity wrapped in an atomic reference counter on success, the exit code indicating the reason on failure
fn setup_node_identity(
    identity_file: &PathBuf,
    public_address: &Multiaddr,
    create_id: bool,
    peer_features: PeerFeatures,
) -> Result<Arc<NodeIdentity>, ExitCodes>
{
    match load_identity(identity_file) {
        Ok(id) => Ok(Arc::new(id)),
        Err(e) => {
            if !create_id {
                error!(
                    target: LOG_TARGET,
                    "Node identity information not found. {}. You can update the configuration file to point to a \
                     valid node identity file, or re-run the node with the --create_id flag to create a new identity.",
                    e
                );
                return Err(ExitCodes::ConfigError);
            }

            debug!(target: LOG_TARGET, "Node id not found. {}. Creating new ID", e);

            match create_new_base_node_identity(identity_file, public_address.clone(), peer_features) {
                Ok(id) => {
                    info!(
                        target: LOG_TARGET,
                        "New node identity [{}] with public key {} has been created at {}.",
                        id.node_id(),
                        id.public_key(),
                        identity_file.to_string_lossy(),
                    );
                    Ok(Arc::new(id))
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not create new node id. {:?}.", e);
                    Err(ExitCodes::ConfigError)
                },
            }
        },
    }
}
