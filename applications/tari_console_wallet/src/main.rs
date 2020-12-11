#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
#![recursion_limit = "512"]
use init::{change_password, get_base_node_peer_config, init_wallet, start_wallet, wallet_mode};
use log::*;
use structopt::StructOpt;
use tari_app_utilities::{identity_management::setup_node_identity, utilities::ExitCodes};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig};
use tari_comms::peer_manager::PeerFeatures;
use tari_shutdown::Shutdown;
use wallet_modes::{command_mode, grpc_mode, script_mode, tui_mode, WalletMode};

pub const LOG_TARGET: &str = "wallet::console_wallet::main";

mod automation;
mod grpc;
mod init;
mod ui;
mod utils;
pub mod wallet_modes;

/// Application entry point
fn main() {
    match main_inner() {
        Ok(_) => std::process::exit(0),
        Err(exit_code) => {
            eprintln!("Exiting with code: {}", exit_code);
            error!(target: LOG_TARGET, "Exiting with code: {}", exit_code);
            std::process::exit(exit_code.as_i32())
        },
    }
}

fn main_inner() -> Result<(), ExitCodes> {
    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .expect("Failed to build a runtime!");

    let mut shutdown = Shutdown::new();

    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(ApplicationType::ConsoleWallet)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    // Populate the configuration struct
    let config = GlobalConfig::convert_from(cfg).map_err(|err| {
        error!(target: LOG_TARGET, "The configuration file has an error. {}", err);
        ExitCodes::ConfigError(format!("The configuration file has an error. {}", err))
    })?;

    debug!(target: LOG_TARGET, "Using configuration: {:?}", config);
    // Load or create the Node identity
    let node_identity = setup_node_identity(
        &config.console_wallet_identity_file,
        &config.public_address,
        bootstrap.create_id,
        PeerFeatures::COMMUNICATION_CLIENT,
    )?;

    // Exit if create_id or init arguments were run
    if bootstrap.create_id {
        info!(
            target: LOG_TARGET,
            "Console wallet's node ID created at '{}'. Done.",
            config.console_wallet_identity_file.to_string_lossy()
        );
        return Ok(());
    }

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }

    // get command line password if provided
    let arg_password = bootstrap.password.clone();

    let shutdown_signal = shutdown.to_signal();

    if bootstrap.change_password {
        info!(target: LOG_TARGET, "Change password requested.");
        return runtime.block_on(change_password(&config, node_identity, arg_password, shutdown_signal));
    }

    // initialize wallet
    let mut wallet = runtime.block_on(init_wallet(&config, node_identity, arg_password, shutdown_signal))?;

    // get base node/s
    let base_node_config = runtime.block_on(get_base_node_peer_config(&config, &mut wallet))?;
    let base_node = base_node_config.get_base_node_peer()?;

    // start wallet
    runtime.block_on(start_wallet(&mut wallet, &base_node))?;

    debug!(target: LOG_TARGET, "Starting app");

    let handle = runtime.handle().clone();
    let result = match wallet_mode(bootstrap) {
        WalletMode::Tui => tui_mode(handle, config, wallet.clone(), base_node, base_node_config),
        WalletMode::Grpc => grpc_mode(handle, wallet.clone(), config),
        WalletMode::Script(path) => script_mode(handle, path, wallet.clone(), config),
        WalletMode::Command(command) => command_mode(handle, command, wallet.clone(), config),
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
