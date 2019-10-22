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

mod cli;
mod consts;

use crate::cli::ConfigBootstrap;
use config::Config;
use log::*;
use tari_common::default_config;

const LOG_TARGET: &str = "base_node::root";

fn main() {
    // Create the tari data directory
    if let Err(e) = tari_common::create_data_directory() {
        println!(
            "We couldn't create a default Tari data directory and have to quit now. This makes us sad :(\n {}",
            e.to_string()
        );
        return;
    }

    // Parse and validate command-line arguments
    let bootstrap = cli::parse_cli_args();

    // Initialise the logger
    if !initialize_logging(&bootstrap) {
        return;
    }

    // Load and apply configuration file
    let cfg = match load_configuration(&bootstrap) {
        Ok(cfg) => cfg,
        Err(s) => {
            error!(target: LOG_TARGET, "{}", s);
            return;
        },
    };

    // Set up the Tokio runtime
    // Configure the shutdown daemon to listen for CTRL-C
    // Build, node, build!
    // Run, node, run!
}

fn initialize_logging(bootstrap: &ConfigBootstrap) -> bool {
    println!(
        "Initializing logging according to {:?}",
        bootstrap.log_config.to_str().unwrap_or("[??]")
    );
    if let Err(e) = log4rs::init_file(bootstrap.log_config.clone(), Default::default()) {
        println!("We couldn't load a logging configuration file. {}", e.to_string());
        return false;
    }
    true
}

fn load_configuration(bootstrap: &ConfigBootstrap) -> Result<Config, String> {
    debug!(
        target: LOG_TARGET,
        "Loading configuration file from  {}",
        bootstrap.config.to_str().unwrap_or("[??]")
    );
    let mut cfg = default_config();
    // Load the configuration file
    let filename = bootstrap
        .config
        .to_str()
        .ok_or("Invalid config file path".to_string())?;
    let config_file = config::File::with_name(filename);
    match cfg.merge(config_file) {
        Ok(_) => {
            info!(target: LOG_TARGET, "Configuration file loaded.");
            Ok(cfg)
        },
        Err(e) => Err(format!(
            "There was an error loading the configuration file. {}",
            e.to_string()
        )),
    }
}
