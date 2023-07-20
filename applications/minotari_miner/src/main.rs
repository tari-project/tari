// Copyright 2021. The Tari Project
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

use std::io::stdout;

use clap::Parser;
use crossterm::{execute, terminal::SetTitle};
use log::*;
use minotari_app_utilities::consts;
use run_miner::start_miner;
use tari_common::{exit_codes::ExitError, initialize_logging};
use tokio::runtime::Runtime;

use crate::cli::Cli;

pub const LOG_TARGET: &str = "minotari::miner::main";
pub const LOG_TARGET_FILE: &str = "minotari::logging::miner::main";

mod cli;
mod config;
mod difficulty;
mod errors;
mod miner;
mod run_miner;
mod stratum;
mod utils;

/// Application entry point
fn main() {
    let rt = Runtime::new().expect("Failed to start tokio runtime");
    let terminal_title = format!("MinoTari Miner - Version {}", consts::APP_VERSION);
    if let Err(e) = execute!(stdout(), SetTitle(terminal_title.as_str())) {
        println!("Error setting terminal title. {}", e)
    }
    match rt.block_on(main_inner()) {
        Ok(_) => std::process::exit(0),
        Err(err) => {
            eprintln!("Fatal error: {:?}", err);
            let exit_code = err.exit_code;
            error!(target: LOG_TARGET, "Exiting with code: {:?}", exit_code);
            std::process::exit(exit_code as i32)
        },
    }
}

async fn main_inner() -> Result<(), ExitError> {
    let cli = Cli::parse();
    initialize_logging(
        &cli.common.log_config_path("miner"),
        &cli.common.get_base_path(),
        include_str!("../log4rs_sample.yml"),
    )?;
    start_miner(cli).await
}
