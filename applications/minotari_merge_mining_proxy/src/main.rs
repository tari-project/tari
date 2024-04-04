// Copyright 2020. The Tari Project
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

use minotari_merge_mining_proxy::Cli;

mod block_template_data;
mod block_template_protocol;
mod cli;
mod common;
mod config;
mod error;
mod monero_fail;
mod proxy;
mod run_merge_miner;

#[cfg(test)]
mod test;

use std::io::stdout;

use clap::Parser;
use crossterm::{execute, terminal::SetTitle};
use log::*;
use minotari_app_utilities::consts;
use tari_common::initialize_logging;

const LOG_TARGET: &str = "minotari_mm_proxy::proxy";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let terminal_title = format!("Minotari Merge Mining Proxy - Version {}", consts::APP_VERSION);
    if let Err(e) = execute!(stdout(), SetTitle(terminal_title.as_str())) {
        println!("Error setting terminal title. {}", e)
    }

    let cli = Cli::parse();

    initialize_logging(
        &cli.common.log_config_path("proxy"),
        &cli.common.get_base_path(),
        include_str!("../log4rs_sample.yml"),
    )?;
    match run_merge_miner::start_merge_miner(cli).await {
        Ok(_) => Ok(()),
        Err(err) => {
            error!(target: LOG_TARGET, "Fatal error: {:?}", err);
            Err(err)
        },
    }
}
