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
//

/// Utilities and helpers for building the base node instance
mod builder;
/// The command line interface definition and configuration
mod cli;
/// Application-specific constants
mod consts;
/// Miner lib Todo hide behind feature flag
mod miner;
/// Parser module used to control user commands
mod parser;

use crate::builder::{create_new_base_node_identity, load_identity};
use log::*;
use parser::Parser;
use rustyline::{config::OutputStreamType, error::ReadlineError, CompletionType, Config, EditMode, Editor};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tari_common::{load_configuration, GlobalConfig};
use tokio::runtime::Runtime;

pub const LOG_TARGET: &str = "base_node::app";

enum ExitCodes {
    ConfigError = 101,
    UnknownError = 102,
}

fn main() {
    cli::print_banner();
    match main_inner() {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => std::process::exit(exit_code as i32),
    }
}

fn main_inner() -> Result<(), ExitCodes> {
    // Create the tari data directory
    if let Err(e) = tari_common::dir_utils::create_data_directory() {
        println!(
            "We couldn't create a default Tari data directory and have to quit now. This makes us sad :(\n {}",
            e.to_string()
        );
        return Err(ExitCodes::ConfigError);
    }

    // Parse and validate command-line arguments
    let arguments = cli::parse_cli_args();

    // Initialise the logger
    if !tari_common::initialize_logging(&arguments.bootstrap.log_config) {
        return Err(ExitCodes::ConfigError);
    }

    // Load and apply configuration file
    let cfg = match load_configuration(&arguments.bootstrap) {
        Ok(cfg) => cfg,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return Err(ExitCodes::ConfigError);
        },
    };

    // Populate the configuration struct
    let node_config = match GlobalConfig::convert_from(cfg) {
        Ok(c) => c,
        Err(e) => {
            error!(target: LOG_TARGET, "The configuration file has an error. {}", e);
            return Err(ExitCodes::ConfigError);
        },
    };

    trace!(target: LOG_TARGET, "Configuration file: {:?}", node_config);

    // Load or create the Node identity
    let node_identity = match load_identity(&node_config.identity_file) {
        Ok(id) => id,
        Err(e) => {
            if !arguments.create_id {
                error!(
                    target: LOG_TARGET,
                    "Node identity information not found. {}. You can update the configuration file to point to a \
                     valid node identity file, or re-run the node with the --create_id flag to create a new identity.",
                    e
                );
                return Err(ExitCodes::ConfigError);
            }
            debug!(target: LOG_TARGET, "Node id not found. {}. Creating new ID", e);
            match create_new_base_node_identity(node_config.public_address.clone()) {
                Ok(id) => {
                    info!(
                        target: LOG_TARGET,
                        "New node identity [{}] with public key {} has been created.",
                        id.node_id(),
                        id.public_key()
                    );
                    id
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not create new node id. {}.", e);
                    return Err(ExitCodes::ConfigError);
                },
            }
        },
    };

    // Set up the Tokio runtime
    let mut rt = match setup_runtime(&node_config) {
        Ok(rt) => rt,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return Err(ExitCodes::UnknownError);
        },
    };

    // Build, node, build!
    let ctx = rt.block_on(async {
        builder::configure_and_initialize_node(&node_config, node_identity)
            .await
            .map_err(|err| {
                error!(target: LOG_TARGET, "{}", err);
                ExitCodes::UnknownError
            })
    })?;
    // Run, node, run!
    let parser = Parser::new(rt.handle().clone(), &ctx);
    let flag = ctx.interrupt_flag();
    let base_node_handle = rt.spawn(ctx.run(rt.handle().clone()));
    info!(
        target: LOG_TARGET,
        "Node has been successfully configured and initialized. Starting CLI loop."
    );
    cli_loop(parser, flag);
    match rt.block_on(base_node_handle) {
        Ok(_) => info!(target: LOG_TARGET, "Node shutdown successfully."),
        Err(e) => error!(target: LOG_TARGET, "Node has crashed: {}", e),
    }

    println!("Goodbye!");
    Ok(())
}

fn setup_runtime(config: &GlobalConfig) -> Result<Runtime, String> {
    let num_core_threads = config.core_threads;
    let num_blocking_threads = config.blocking_threads;

    debug!(
        target: LOG_TARGET,
        "Configuring the node to run on {} core threads and {} blocking worker threads.",
        num_core_threads,
        num_blocking_threads
    );
    tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .max_threads(num_core_threads + num_blocking_threads)
        .core_threads(num_core_threads)
        .build()
        .map_err(|e| format!("There was an error while building the node runtime. {}", e.to_string()))
}

fn cli_loop(parser: Parser, shutdown_flag: Arc<AtomicBool>) {
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
                    p.handle_command(&line)
                }
            },
            Err(ReadlineError::Interrupted) => {
                // shutdown section. Will shutdown all interfaces when ctrl-c was pressed
                println!("CTRL-C received");
                println!("Shutting down");
                info!(
                    target: LOG_TARGET,
                    "Termination signal received from user. Shutting node down."
                );
                shutdown_flag.store(true, Ordering::SeqCst);
                break;
            },
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            },
        }
        if shutdown_flag.load(Ordering::Relaxed) {
            break;
        };
    }
}
