//  Copyright 2022. The Tari Project
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

// non-64-bit not supported
minotari_app_utilities::deny_non_64_bit_archs!();

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

pub use cli::{
    BurnMinotariArgs,
    Cli,
    CliCommands,
    CoinSplitArgs,
    DiscoverPeerArgs,
    ExportUtxosArgs,
    MakeItRainArgs,
    SendMinotariArgs,
    SetBaseNodeArgs,
    WhoisArgs,
};
use init::{change_password, get_base_node_peer_config, init_wallet, start_wallet, tari_splash_screen, WalletBoot};
use log::*;
use minotari_app_utilities::{common_cli_args::CommonCliArgs, consts};
use minotari_wallet::transaction_service::config::TransactionRoutingMechanism;
use recovery::{get_seed_from_seed_words, prompt_private_key_from_seed_words};
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
};
use tari_key_manager::cipher_seed::CipherSeed;
#[cfg(all(unix, feature = "libtor"))]
use tari_libtor::tor::Tor;
use tari_shutdown::Shutdown;
use tari_utilities::SafePassword;
use tokio::runtime::Runtime;
use wallet_modes::{command_mode, grpc_mode, recovery_mode, script_mode, tui_mode, WalletMode};

pub use crate::config::ApplicationConfig;
use crate::init::{boot_with_password, confirm_direct_only_send, confirm_seed_words, prompt_wallet_type, wallet_mode};

pub const LOG_TARGET: &str = "wallet::console_wallet::main";

pub fn run_wallet(shutdown: &mut Shutdown, runtime: Runtime, config: &mut ApplicationConfig) -> Result<(), ExitError> {
    let data_dir = config.wallet.data_dir.clone();
    let data_dir_str = data_dir.clone().into_os_string().into_string().unwrap();

    let mut config_path = data_dir;
    config_path.push("config.toml");

    let cli = Cli {
        common: CommonCliArgs {
            base_path: data_dir_str,
            config: config_path.into_os_string().into_string().unwrap(),
            log_config: None,
            log_level: None,
            network: None,
            config_property_overrides: vec![],
        },
        password: None,
        change_password: false,
        recovery: false,
        seed_words: None,
        seed_words_file_name: None,
        non_interactive_mode: true,
        input_file: None,
        command: None,
        wallet_notify: None,
        command_mode_auto_exit: false,
        grpc_enabled: true,
        grpc_address: None,
        command2: None,
        profile_with_tokio_console: false,
    };

    run_wallet_with_cli(shutdown, runtime, config, cli)
}

#[allow(clippy::too_many_lines)]
pub fn run_wallet_with_cli(
    shutdown: &mut Shutdown,
    runtime: Runtime,
    config: &mut ApplicationConfig,
    cli: Cli,
) -> Result<(), ExitError> {
    info!(
        target: LOG_TARGET,
        "== {} ({}) ==",
        ApplicationType::ConsoleWallet,
        consts::APP_VERSION
    );

    let password = get_password(config, &cli);

    if password.is_none() {
        tari_splash_screen("Console Wallet");
    }

    // check for recovery based on existence of wallet file
    let (mut boot_mode, password) = boot_with_password(&cli, &config.wallet)?;

    let recovery_seed = get_recovery_seed(boot_mode, &cli)?;

    // This is deactivated at the moment as full support is not yet complete
    let wallet_type = prompt_wallet_type(boot_mode, &config.wallet, cli.non_interactive_mode);

    // get command line password if provided
    let seed_words_file_name = cli.seed_words_file_name.clone();

    let shutdown_signal = shutdown.to_signal();

    if cli.change_password {
        info!(target: LOG_TARGET, "Change password requested.");
        return runtime.block_on(change_password(
            config,
            password,
            shutdown_signal,
            cli.non_interactive_mode,
        ));
    }

    // Run our own Tor instance, if configured
    // This is currently only possible on linux/macos
    #[cfg(all(unix, feature = "libtor"))]
    if config.wallet.use_libtor && config.wallet.p2p.transport.is_tor() {
        let tor = Tor::initialize()?;
        tor.update_comms_transport(&mut config.wallet.p2p.transport)?;
        tor.run_background();
        debug!(
            target: LOG_TARGET,
            "Updated Tor comms transport: {:?}", config.wallet.p2p.transport
        );
    }

    let on_init = matches!(boot_mode, WalletBoot::New);
    let not_recovery = recovery_seed.is_none();

    // initialize wallet
    let mut wallet = runtime.block_on(init_wallet(
        config,
        password,
        seed_words_file_name,
        recovery_seed,
        shutdown_signal,
        cli.non_interactive_mode,
        wallet_type,
    ))?;

    if !cli.non_interactive_mode &&
        config.wallet.transaction_service_config.transaction_routing_mechanism ==
            TransactionRoutingMechanism::DirectOnly
    {
        match confirm_direct_only_send(&mut wallet) {
            Ok(()) => {
                print!("\x1Bc"); // Clear the screen
            },
            Err(error) => {
                return Err(error);
            },
        };
    }

    // if wallet is being set for the first time, wallet seed words are prompted on the screen
    if !cli.non_interactive_mode && not_recovery && on_init {
        match confirm_seed_words(&mut wallet) {
            Ok(()) => {
                print!("\x1Bc"); // Clear the screen
            },
            Err(error) => {
                return Err(error);
            },
        };
    }

    // Check if there is an in progress recovery in the wallet's database
    if wallet.is_recovery_in_progress()? {
        println!("A Wallet Recovery was found to be in progress, continuing.");
        boot_mode = WalletBoot::Recovery;
    }

    // get base node/s
    let base_node_config =
        runtime.block_on(get_base_node_peer_config(config, &mut wallet, cli.non_interactive_mode))?;
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
        WalletMode::Command(command) => command_mode(
            handle,
            &cli,
            &config.wallet,
            &base_node_config,
            wallet.clone(),
            *command,
        ),

        WalletMode::RecoveryDaemon | WalletMode::RecoveryTui => {
            recovery_mode(handle, &base_node_config, &config.wallet, wallet_mode, wallet.clone())
        },
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

fn get_password(config: &ApplicationConfig, cli: &Cli) -> Option<SafePassword> {
    cli.password
        .as_ref()
        .or(config.wallet.password.as_ref())
        .map(|s| s.to_owned())
}

fn get_recovery_seed(boot_mode: WalletBoot, cli: &Cli) -> Result<Option<CipherSeed>, ExitError> {
    if matches!(boot_mode, WalletBoot::Recovery) {
        let seed = if let Some(ref seed_words) = cli.seed_words {
            get_seed_from_seed_words(seed_words)?
        } else {
            prompt_private_key_from_seed_words()?
        };
        Ok(Some(seed))
    } else {
        Ok(None)
    }
}
