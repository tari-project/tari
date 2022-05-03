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

use std::{env, process};

use clap::Parser;
use cli::Cli;
use init::{
    boot,
    change_password,
    get_base_node_peer_config,
    init_wallet,
    start_wallet,
    tari_splash_screen,
    WalletBoot,
};
use log::*;
use opentelemetry::{self, global, KeyValue};
use recovery::prompt_private_key_from_seed_words;
use tari_app_utilities::consts;
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
    initialize_logging,
    load_configuration,
};
use tari_key_manager::cipher_seed::CipherSeed;
#[cfg(all(unix, feature = "libtor"))]
use tari_libtor::tor::Tor;
use tari_shutdown::Shutdown;
use tracing_subscriber::{layer::SubscriberExt, Registry};
use wallet_modes::{command_mode, grpc_mode, recovery_mode, script_mode, tui_mode, WalletMode};

use crate::{config::ApplicationConfig, init::wallet_mode, recovery::get_seed_from_seed_words};

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

/// Application entry point
fn main() {
    // Uncomment to enable tokio tracing via tokio-console
    // console_subscriber::init();

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

    let config_path = cli.common.config_path();
    let cfg = load_configuration(config_path.as_path(), true, &cli.config_property_overrides())?;
    initialize_logging(
        &cli.common.log_config_path("wallet"),
        include_str!("../log4rs_sample.yml"),
    )?;

    #[cfg_attr(not(all(unix, feature = "libtor")), allow(unused_mut))]
    let mut config = ApplicationConfig::load_from(&cfg)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build a runtime!");

    if cli.tracing_enabled {
        enable_tracing();
    }

    info!(
        target: LOG_TARGET,
        "== {} ({}) ==",
        ApplicationType::ConsoleWallet,
        consts::APP_VERSION
    );

    let password = cli
        .password
        .as_ref()
        .or_else(|| config.wallet.password.as_ref())
        .map(|s| s.to_owned());

    if password.is_none() {
        tari_splash_screen("Console Wallet");
    }

    // check for recovery based on existence of wallet file
    let mut boot_mode = boot(&cli, &config.wallet)?;

    let recovery_seed = get_recovery_seed(boot_mode, &cli)?;

    // get command line password if provided
    let seed_words_file_name = cli.seed_words_file_name.clone();

    let mut shutdown = Shutdown::new();
    let shutdown_signal = shutdown.to_signal();

    if cli.change_password {
        info!(target: LOG_TARGET, "Change password requested.");
        return runtime.block_on(change_password(&config, password, shutdown_signal));
    }

    // Run our own Tor instance, if configured
    // This is currently only possible on linux/macos
    #[cfg(all(unix, feature = "libtor"))]
    if config.wallet.use_libtor && config.wallet.p2p.transport.is_tor() {
        let tor = Tor::initialize()?;
        tor.update_comms_transport(&mut config.wallet.p2p.transport)?;
        runtime.spawn(tor.run(shutdown.to_signal()));
        debug!(
            target: LOG_TARGET,
            "Updated Tor comms transport: {:?}", config.wallet.p2p.transport
        );
    }

    // initialize wallet
    let mut wallet = runtime.block_on(init_wallet(
        &config,
        password,
        seed_words_file_name,
        recovery_seed,
        shutdown_signal,
    ))?;

    // Check if there is an in progress recovery in the wallet's database
    if runtime.block_on(wallet.is_recovery_in_progress())? {
        println!("A Wallet Recovery was found to be in progress, continuing.");
        boot_mode = WalletBoot::Recovery;
    }

    // get base node/s
    let base_node_config = runtime.block_on(get_base_node_peer_config(&config, &mut wallet))?;
    let base_node_selected = base_node_config.get_base_node_peer()?;

    let wallet_mode = wallet_mode(&cli, boot_mode);

    // start wallet
    runtime.block_on(start_wallet(&mut wallet, &base_node_selected, &wallet_mode))?;

    debug!(target: LOG_TARGET, "Starting app");

    let handle = runtime.handle().clone();

    let result = match wallet_mode {
        WalletMode::Tui => tui_mode(handle, &config.wallet, &base_node_config, wallet.clone()),
        WalletMode::Grpc => grpc_mode(handle, &config.wallet, wallet.clone()),
        WalletMode::Script(path) => script_mode(handle, &cli, &config.wallet, &base_node_config, wallet.clone(), path),
        WalletMode::Command(command) => {
            command_mode(handle, &cli, &config.wallet, &base_node_config, wallet.clone(), command)
        },
        WalletMode::RecoveryDaemon | WalletMode::RecoveryTui => {
            recovery_mode(handle, &base_node_config, &config.wallet, wallet_mode, wallet.clone())
        },
        WalletMode::Invalid => Err(ExitError::new(
            ExitCode::InputError,
            &"Invalid wallet mode - are you trying too many command options at once?",
        )),
    };

    print!("\nShutting down wallet... ");
    shutdown.trigger();
    runtime.block_on(wallet.wait_until_shutdown());
    println!("Done.");

    result
}

fn get_recovery_seed(boot_mode: WalletBoot, cli: &Cli) -> Result<Option<CipherSeed>, ExitError> {
    if matches!(boot_mode, WalletBoot::Recovery) {
        let seed = if cli.seed_words.is_some() {
            let seed_words: Vec<String> = cli
                .seed_words
                .clone()
                .unwrap()
                .split_whitespace()
                .map(|v| v.to_string())
                .collect();
            get_seed_from_seed_words(seed_words)?
        } else {
            prompt_private_key_from_seed_words()?
        };
        Ok(Some(seed))
    } else {
        Ok(None)
    }
}

fn enable_tracing() {
    // To run:
    // docker run -d -p6831:6831/udp -p6832:6832/udp -p16686:16686 -p14268:14268 jaegertracing/all-in-one:latest
    // To view the UI after starting the container (default):
    // http://localhost:16686
    global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("tari::console_wallet")
        .with_tags(vec![
            KeyValue::new("pid", process::id().to_string()),
            KeyValue::new(
                "current_exe",
                env::current_exe().unwrap().to_str().unwrap_or_default().to_owned(),
            ),
        ])
        .install_batch(opentelemetry::runtime::Tokio)
        .unwrap();
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default().with(telemetry);
    tracing::subscriber::set_global_default(subscriber)
        .expect("Tracing could not be set. Try running without `--tracing-enabled`");
}
