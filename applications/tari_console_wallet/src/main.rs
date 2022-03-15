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

#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
#![deny(clippy::redundant_clone)]
#![recursion_limit = "1024"]
use std::{env, process};

use init::{
    boot,
    change_password,
    get_base_node_peer_config,
    get_notify_script,
    init_wallet,
    start_wallet,
    tari_splash_screen,
    wallet_mode,
    WalletBoot,
};
use log::*;
use opentelemetry::{self, global, KeyValue};
use recovery::prompt_private_key_from_seed_words;
use tari_app_utilities::{consts, initialization::init_configuration};
#[cfg(all(unix, feature = "libtor"))]
use tari_common::CommsTransport;
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
    ConfigBootstrap,
};
use tari_key_manager::cipher_seed::CipherSeed;
#[cfg(all(unix, feature = "libtor"))]
use tari_libtor::tor::Tor;
use tari_shutdown::Shutdown;
use tracing_subscriber::{layer::SubscriberExt, Registry};
use wallet_modes::{command_mode, grpc_mode, recovery_mode, script_mode, tui_mode, WalletMode};

use crate::{recovery::get_seed_from_seed_words, wallet_modes::WalletModeConfig};

pub const LOG_TARGET: &str = "wallet::console_wallet::main";

mod automation;
mod grpc;
mod init;
mod notifier;
mod recovery;
mod ui;
mod utils;
pub mod wallet_modes;

/// Application entry point
fn main() {
    match main_inner() {
        Ok(_) => process::exit(0),
        Err(err) => {
            eprintln!("{:?}", err);
            let exit_code = err.exit_code;
            error!(
                target: LOG_TARGET,
                "Exiting with code ({}): {:?}", exit_code as i32, exit_code
            );
            process::exit(exit_code as i32)
        },
    }
}

fn main_inner() -> Result<(), ExitError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build a runtime!");

    #[allow(unused_mut)] // config isn't mutated on windows
    let (bootstrap, mut global_config, _) = init_configuration(ApplicationType::ConsoleWallet)?;

    if bootstrap.tracing_enabled {
        enable_tracing();
    }

    info!(
        target: LOG_TARGET,
        "== {} ({}) ==",
        ApplicationType::ConsoleWallet,
        consts::APP_VERSION
    );

    debug!(target: LOG_TARGET, "Using configuration: {:?}", global_config);
    debug!(target: LOG_TARGET, "Using bootstrap: {:?}", bootstrap);

    // get command line password if provided
    let arg_password = bootstrap.password.clone();

    if arg_password.is_none() {
        tari_splash_screen("Console Wallet");
    }

    // check for recovery based on existence of wallet file
    let mut boot_mode = boot(&bootstrap, &global_config)?;

    let recovery_seed: Option<CipherSeed> = get_recovery_seed(boot_mode, &bootstrap)?;

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
    }

    // get command line password if provided
    let arg_password = bootstrap.password.clone();
    let seed_words_file_name = bootstrap.seed_words_file_name.clone();

    let mut shutdown = Shutdown::new();
    let shutdown_signal = shutdown.to_signal();

    if bootstrap.change_password {
        info!(target: LOG_TARGET, "Change password requested.");
        return runtime.block_on(change_password(&global_config, arg_password, shutdown_signal));
    }

    // Run our own Tor instance, if configured
    // This is currently only possible on linux/macos
    #[cfg(all(unix, feature = "libtor"))]
    if global_config.console_wallet_use_libtor &&
        matches!(global_config.comms_transport, CommsTransport::TorHiddenService { .. })
    {
        let tor = Tor::initialize()?;
        global_config.comms_transport = tor.update_comms_transport(global_config.comms_transport)?;
        runtime.spawn(tor.run(shutdown.to_signal()));
        debug!(
            target: LOG_TARGET,
            "Updated Tor comms transport: {:?}", global_config.comms_transport
        );
    }

    // initialize wallet
    let mut wallet = runtime.block_on(init_wallet(
        &global_config,
        arg_password,
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
    let base_node_config = runtime.block_on(get_base_node_peer_config(&global_config, &mut wallet))?;
    let base_node_selected = base_node_config.get_base_node_peer()?;

    let wallet_mode = wallet_mode(&bootstrap, boot_mode);

    // start wallet
    runtime.block_on(start_wallet(&mut wallet, &base_node_selected, &wallet_mode))?;

    // optional path to notify script
    let notify_script = get_notify_script(&bootstrap, &global_config)?;

    debug!(target: LOG_TARGET, "Starting app");

    let handle = runtime.handle().clone();
    let config = WalletModeConfig {
        base_node_config,
        base_node_selected,
        bootstrap,
        global_config,
        handle,
        notify_script,
        wallet_mode: wallet_mode.clone(),
    };
    let result = match wallet_mode {
        WalletMode::Tui => tui_mode(config, wallet.clone()),
        WalletMode::Grpc => grpc_mode(config, wallet.clone()),
        WalletMode::Script(path) => script_mode(config, wallet.clone(), path),
        WalletMode::Command(command) => command_mode(config, wallet.clone(), command),
        WalletMode::RecoveryDaemon | WalletMode::RecoveryTui => recovery_mode(config, wallet.clone()),
        WalletMode::Invalid => Err(ExitError::new(
            ExitCode::InputError,
            "Invalid wallet mode - are you trying too many command options at once?",
        )),
    };

    print!("\nShutting down wallet... ");
    shutdown.trigger();
    runtime.block_on(wallet.wait_until_shutdown());
    println!("Done.");

    result
}

fn get_recovery_seed(boot_mode: WalletBoot, bootstrap: &ConfigBootstrap) -> Result<Option<CipherSeed>, ExitError> {
    if matches!(boot_mode, WalletBoot::Recovery) {
        let seed = if bootstrap.seed_words.is_some() {
            let seed_words: Vec<String> = bootstrap
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
