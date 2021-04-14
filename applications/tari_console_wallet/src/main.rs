#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
#![recursion_limit = "1024"]
use crate::recovery::get_private_key_from_seed_words;
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
use recovery::prompt_private_key_from_seed_words;
use std::process;
use tari_app_utilities::{initialization::init_configuration, utilities::ExitCodes};
use tari_common::configuration::bootstrap::ApplicationType;
use tari_core::transactions::types::PrivateKey;
use tari_shutdown::Shutdown;
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
            eprintln!("{}", exit_code);
            error!(
                target: LOG_TARGET,
                "Exiting with code ({}): {}",
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

    let (bootstrap, config, _) = init_configuration(ApplicationType::ConsoleWallet)?;

    debug!(target: LOG_TARGET, "Using configuration: {:?}", config);
    debug!(target: LOG_TARGET, "Using bootstrap: {:?}", bootstrap);

    // get command line password if provided
    let arg_password = bootstrap.password.clone();

    if arg_password.is_none() {
        tari_splash_screen("Console Wallet");
    }

    // check for recovery based on existence of wallet file
    let mut boot_mode = boot(&bootstrap, &config)?;

    let master_key: Option<PrivateKey> = if matches!(boot_mode, WalletBoot::Recovery) {
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
        Some(private_key)
    } else {
        None
    };

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
        return runtime.block_on(change_password(&config, arg_password, shutdown_signal));
    }

    // initialize wallet
    let mut wallet = runtime.block_on(init_wallet(
        &config,
        arg_password,
        seed_words_file_name,
        master_key,
        shutdown_signal,
    ))?;

    // Check if there is an in progress recovery in the wallet's database
    if runtime.block_on(wallet.is_recovery_in_progress())? {
        println!("A Wallet Recovery was found to be in progress, continuing.");
        boot_mode = WalletBoot::Recovery;
    }

    // get base node/s
    let base_node_config = runtime.block_on(get_base_node_peer_config(&config, &mut wallet))?;
    let base_node = base_node_config.get_base_node_peer()?;

    let wallet_mode = wallet_mode(&bootstrap, boot_mode);

    // start wallet
    runtime.block_on(start_wallet(&mut wallet, &base_node, &wallet_mode))?;

    // optional path to notify script
    let notify_script = get_notify_script(&bootstrap, &config)?;

    debug!(target: LOG_TARGET, "Starting app");

    let handle = runtime.handle().clone();
    let result = match wallet_mode {
        WalletMode::Tui => tui_mode(
            handle,
            config,
            wallet.clone(),
            base_node,
            base_node_config,
            notify_script,
        ),
        WalletMode::Grpc => grpc_mode(handle, wallet.clone(), config),
        WalletMode::Script(path) => script_mode(handle, path, wallet.clone(), config),
        WalletMode::Command(command) => command_mode(handle, command, wallet.clone(), config),
        WalletMode::Recovery => recovery_mode(
            handle,
            config,
            wallet.clone(),
            base_node,
            base_node_config,
            notify_script,
        ),
        WalletMode::Invalid => Err(ExitCodes::InputError(
            "Invalid wallet mode - are you trying too many command options at once?".to_string(),
        )),
    };

    print!("Shutting down wallet... ");
    if shutdown.trigger().is_ok() {
        runtime.block_on(wallet.wait_until_shutdown());
    } else {
        error!(target: LOG_TARGET, "No listeners for the shutdown signal!");
    }
    println!("Done.");

    result
}
