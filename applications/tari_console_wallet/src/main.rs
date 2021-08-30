#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
#![recursion_limit = "1024"]
use crate::{recovery::get_private_key_from_seed_words, wallet_modes::WalletModeConfig};
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
use std::{env, process};
use tari_app_utilities::{consts, initialization::init_configuration, utilities::ExitCodes};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap};
use tari_common_types::types::PrivateKey;
use tari_shutdown::Shutdown;
use tracing_subscriber::{layer::SubscriberExt, Registry};
use wallet_modes::{command_mode, grpc_mode, recovery_mode, script_mode, tui_mode, WalletMode};

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
        Err(exit_code) => {
            eprintln!("{:?}", exit_code);
            error!(
                target: LOG_TARGET,
                "Exiting with code ({}): {:?}",
                exit_code.as_i32(),
                exit_code
            );
            process::exit(exit_code.as_i32())
        },
    }
}

fn main_inner() -> Result<(), ExitCodes> {
    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .expect("Failed to build a runtime!");

    let (bootstrap, global_config, _) = init_configuration(ApplicationType::ConsoleWallet)?;

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

    let recovery_master_key: Option<PrivateKey> = get_recovery_master_key(boot_mode, &bootstrap)?;

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
    }

    enable_tracing_if_specified(&bootstrap);
    // get command line password if provided
    let arg_password = bootstrap.password.clone();
    let seed_words_file_name = bootstrap.seed_words_file_name.clone();

    let mut shutdown = Shutdown::new();
    let shutdown_signal = shutdown.to_signal();

    if bootstrap.change_password {
        info!(target: LOG_TARGET, "Change password requested.");
        return runtime.block_on(change_password(&global_config, arg_password, shutdown_signal));
    }

    // initialize wallet
    let mut wallet = runtime.block_on(init_wallet(
        &global_config,
        arg_password,
        seed_words_file_name,
        recovery_master_key,
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
        WalletMode::Invalid => Err(ExitCodes::InputError(
            "Invalid wallet mode - are you trying too many command options at once?".to_string(),
        )),
    };

    print!("\nShutting down wallet... ");
    if shutdown.trigger().is_ok() {
        runtime.block_on(wallet.wait_until_shutdown());
    } else {
        error!(target: LOG_TARGET, "No listeners for the shutdown signal!");
    }
    println!("Done.");

    result
}

fn get_recovery_master_key(
    boot_mode: WalletBoot,
    bootstrap: &ConfigBootstrap,
) -> Result<Option<PrivateKey>, ExitCodes> {
    if matches!(boot_mode, WalletBoot::Recovery) {
        let private_key = if bootstrap.seed_words.is_some() {
            let seed_words: Vec<String> = bootstrap
                .seed_words
                .clone()
                .unwrap()
                .split_whitespace()
                .map(|v| v.to_string())
                .collect();
            get_private_key_from_seed_words(seed_words)?
        } else {
            prompt_private_key_from_seed_words()?
        };
        Ok(Some(private_key))
    } else {
        Ok(None)
    }
}

fn enable_tracing_if_specified(bootstrap: &ConfigBootstrap) {
    if bootstrap.tracing_enabled {
        // To run: docker run -d -p6831:6831/udp -p6832:6832/udp -p16686:16686 -p14268:14268 \
        // jaegertracing/all-in-one:latest
        global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
        let tracer = opentelemetry_jaeger::new_pipeline()
            .with_service_name("tari::console_wallet")
            .with_tags(vec![KeyValue::new("pid", process::id().to_string()), KeyValue::new("current_exe", env::current_exe().unwrap().to_str().unwrap_or_default().to_owned())])
            // TODO: uncomment when using tokio 1
            // .install_batch(opentelemetry::runtime::Tokio)
            .install_simple()
            .unwrap();
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        let subscriber = Registry::default().with(telemetry);
        tracing::subscriber::set_global_default(subscriber)
            .expect("Tracing could not be set. Try running without `--tracing-enabled`");
    }
}
