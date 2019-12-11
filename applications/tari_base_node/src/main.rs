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

use crate::builder::{create_and_save_id, load_identity};
use futures::{future, StreamExt};
use log::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tari_common::{load_configuration, GlobalConfig};
use tari_utilities::hex::Hex;
use tokio::{net::signal, runtime::Runtime};

const LOG_TARGET: &str = "base_node::app";

fn main() {
    cli::print_banner();
    // Create the tari data directory
    if let Err(e) = tari_common::dir_utils::create_data_directory() {
        println!(
            "We couldn't create a default Tari data directory and have to quit now. This makes us sad :(\n {}",
            e.to_string()
        );
        return;
    }

    // Parse and validate command-line arguments
    let arguments = cli::parse_cli_args();

    // Initialise the logger
    if !tari_common::initialize_logging(&arguments.bootstrap.log_config) {
        return;
    }

    // Load and apply configuration file
    let cfg = match load_configuration(&arguments.bootstrap) {
        Ok(cfg) => cfg,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return;
        },
    };

    // Populate the configuration struct
    let node_config = match GlobalConfig::convert_from(cfg) {
        Ok(c) => c,
        Err(e) => {
            error!(target: LOG_TARGET, "The configuration file has an error. {}", e);
            return;
        },
    };

    // Load or create the Node identity
    let node_id = match load_identity(&node_config.identity_file) {
        Ok(id) => id,
        Err(e) => {
            if !arguments.create_id {
                error!(
                    target: LOG_TARGET,
                    "Node identity information not found. {}. You can update the configuration file to point to a \
                     valid node identity file, or re-run the node with the --create_id flag to create anew identity.",
                    e
                );
                return;
            }
            debug!(target: LOG_TARGET, "Node id not found. {}. Creating new ID", e);
            match create_and_save_id(&node_config.identity_file, &node_config.address) {
                Ok(id) => {
                    info!(
                        target: LOG_TARGET,
                        "New node identity [{}] with public key {} has been created.",
                        id.node_id().to_hex(),
                        id.public_key().to_hex()
                    );
                    id
                },
                Err(e) => {
                    error!(target: LOG_TARGET, "Could not create new node id. {}.", e);
                    return;
                },
            }
        },
    };

    // Set up the Tokio runtime
    let rt = match setup_runtime(&node_config) {
        Ok(rt) => rt,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return;
        },
    };

    // Build, node, build!
    let (comms, node) = match builder::configure_and_initialize_node(&node_config, node_id, &rt) {
        Ok(n) => n,
        Err(e) => {
            error!(target: LOG_TARGET, "Could not instantiate node instance. {}", e);
            return;
        },
    };

    // Configure the shutdown daemon to listen for CTRL-C
    let flag = node.get_flag();
    if let Err(e) = handle_ctrl_c(&rt, flag) {
        error!(target: LOG_TARGET, "Could not configure Ctrl-C handling. {}", e);
        return;
    };

    // Run, node, run!
    let main = async move {
        node.run().await;
        debug!(
            target: LOG_TARGET,
            "The node has finished all it's work. initiating Comms stack shutdown"
        );
        match comms.shutdown() {
            Ok(()) => info!(target: LOG_TARGET, "The comms stack reported a clean shutdown"),
            Err(e) => warn!(
                target: LOG_TARGET,
                "The comms stack did not shut down cleanly: {}",
                e.to_string()
            ),
        }
    };
    rt.spawn(main);
    rt.shutdown_on_idle();
    println!("Goodbye!");
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
        .blocking_threads(num_blocking_threads)
        .core_threads(num_core_threads)
        .build()
        .map_err(|e| format!("There was an error while building the node runtime. {}", e.to_string()))
}

/// Set the interrupt flag on the node when Ctrl-C is entered
fn handle_ctrl_c(rt: &Runtime, flag: Arc<AtomicBool>) -> Result<(), String> {
    let ctrl_c = signal::ctrl_c().map_err(|e| e.to_string())?;
    let s = ctrl_c.take(1).for_each(move |_| {
        info!(
            target: LOG_TARGET,
            "Termination signal received from user. Shutting node down."
        );
        flag.store(true, Ordering::SeqCst);
        future::ready(())
    });
    rt.spawn(s);
    Ok(())
}
