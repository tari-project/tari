//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::process;

use clap::Parser;
use log::*;
use minotari_console_wallet::{run_wallet_with_cli, ApplicationConfig, Cli};
use tari_common::{
    configuration::bootstrap::{grpc_default_port, ApplicationType},
    exit_codes::ExitError,
    initialize_logging,
    load_configuration,
};
use tari_shutdown::Shutdown;

pub const LOG_TARGET: &str = "wallet::console_wallet::main";

mod automation;
mod cli;
mod config;
mod grpc;
mod init;
mod notifier;
mod recovery;
mod ui;
mod utils;
mod wallet_modes;

fn main() {
    match main_inner() {
        Ok(_) => process::exit(0),
        Err(err) => {
            eprintln!("{}", err);
            let exit_code = err.exit_code;
            if let Some(hint) = exit_code.hint() {
                eprintln!();
                eprintln!("{}", hint);
                eprintln!();
            }

            error!(
                target: LOG_TARGET,
                "Exiting with code ({}): {:?}: {}", exit_code as i32, exit_code, err
            );
            process::exit(exit_code as i32)
        },
    }
}

fn main_inner() -> Result<(), ExitError> {
    let cli = Cli::parse();

    let cfg = load_configuration(cli.common.config_path(), true, cli.non_interactive_mode, &cli)?;
    let base_path = cli.common.get_base_path();
    initialize_logging(
        &cli.common.log_config_path("wallet"),
        cli.common.log_path.as_ref().unwrap_or(&base_path),
        include_str!("../log4rs_sample.yml"),
    )?;

    if cli.profile_with_tokio_console {
        // Uncomment to enable tokio tracing via tokio-console
        console_subscriber::init();
    }

    let mut config = ApplicationConfig::load_from(&cfg)?;

    setup_grpc_config(&mut config);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build a runtime!");

    let mut shutdown = Shutdown::new();
    run_wallet_with_cli(&mut shutdown, runtime, &mut config, cli)
}

fn setup_grpc_config(config: &mut ApplicationConfig) {
    if config.wallet.grpc_address.is_none() {
        config.wallet.grpc_address = Some(
            format!(
                "/ip4/127.0.0.1/tcp/{}",
                grpc_default_port(ApplicationType::ConsoleWallet, config.wallet.network)
            )
            .parse()
            .unwrap(),
        );
    }
}
