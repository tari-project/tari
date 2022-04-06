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

use std::{fs, path::PathBuf, str::FromStr, sync::Arc};

use log::*;
use rpassword::prompt_password_stdout;
use rustyline::Editor;
use tari_common::exit_codes::{ExitCode, ExitError};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures},
    tor::HiddenServiceControllerError,
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_core::transactions::CryptoFactories;
use tari_crypto::keys::PublicKey;
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::MnemonicLanguage};
use tari_p2p::{initialization::CommsInitializationError, peer_seeds::SeedPeer, TransportType};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    error::{WalletError, WalletStorageError},
    storage::{database::WalletDatabase, sqlite_utilities::initialize_sqlite_database_backends},
    wallet::{derive_comms_secret_key, read_or_create_master_seed},
    Wallet,
    WalletConfig,
    WalletSqlite,
};

use crate::{
    cli::Cli,
    utils::db::get_custom_base_node_peer_from_db,
    wallet_modes::{PeerConfig, WalletMode},
    ApplicationConfig,
};

pub const LOG_TARGET: &str = "wallet::console_wallet::init";
const TARI_WALLET_PASSWORD: &str = "TARI_WALLET_PASSWORD";

#[derive(Clone, Copy)]
pub enum WalletBoot {
    New,
    Existing,
    Recovery,
}

/// Gets the password provided by command line argument or environment variable if available.
/// Otherwise prompts for the password to be typed in.
pub fn get_or_prompt_password(
    arg_password: Option<String>,
    config_password: Option<String>,
) -> Result<Option<String>, ExitError> {
    if arg_password.is_some() {
        return Ok(arg_password);
    }

    let env = std::env::var_os(TARI_WALLET_PASSWORD);
    if let Some(p) = env {
        let env_password = Some(
            p.into_string()
                .map_err(|_| ExitError::new(ExitCode::IOError, &"Failed to convert OsString into String"))?,
        );
        return Ok(env_password);
    }

    if config_password.is_some() {
        return Ok(config_password);
    }

    let password = prompt_password("Wallet password: ")?;

    Ok(Some(password))
}

fn prompt_password(prompt: &str) -> Result<String, ExitError> {
    let password = loop {
        let pass = prompt_password_stdout(prompt).map_err(|e| ExitError::new(ExitCode::IOError, &e))?;
        if pass.is_empty() {
            println!("Password cannot be empty!");
            continue;
        } else {
            break pass;
        }
    };

    Ok(password)
}

/// Allows the user to change the password of the wallet.
pub async fn change_password(
    config: &ApplicationConfig,
    arg_password: Option<String>,
    shutdown_signal: ShutdownSignal,
) -> Result<(), ExitError> {
    let mut wallet = init_wallet(config, arg_password, None, None, shutdown_signal).await?;

    let passphrase = prompt_password("New wallet password: ")?;
    let confirmed = prompt_password("Confirm new password: ")?;

    if passphrase != confirmed {
        return Err(ExitError::new(ExitCode::InputError, &"Passwords don't match!"));
    }

    wallet
        .remove_encryption()
        .await
        .map_err(|e| ExitError::new(ExitCode::WalletError, &e))?;

    wallet
        .apply_encryption(passphrase)
        .await
        .map_err(|e| ExitError::new(ExitCode::WalletError, &e))?;

    println!("Wallet password changed successfully.");

    Ok(())
}

/// Populates the PeerConfig struct from:
/// 1. The custom peer in the wallet if it exists
/// 2. The service peers defined in config they exist
/// 3. The peer seeds defined in config
pub async fn get_base_node_peer_config(
    config: &ApplicationConfig,
    wallet: &mut WalletSqlite,
) -> Result<PeerConfig, ExitError> {
    // custom
    let mut base_node_custom = get_custom_base_node_peer_from_db(wallet).await;

    if let Some(custom) = config.wallet.custom_base_node.clone() {
        match SeedPeer::from_str(&custom) {
            Ok(node) => {
                base_node_custom = Some(Peer::from(node));
            },
            Err(err) => {
                return Err(ExitError::new(
                    ExitCode::ConfigError,
                    &format!("Malformed custom base node: {}", err),
                ));
            },
        }
    }

    // config
    let base_node_peers = config
        .wallet
        .base_node_service_peers
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExitError::new(ExitCode::ConfigError, &format!("Malformed base node peer: {}", err)))?;

    // peer seeds
    let peer_seeds = config
        .peer_seeds
        .peer_seeds
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExitError::new(ExitCode::ConfigError, &format!("Malformed seed peer: {}", err)))?;

    let peer_config = PeerConfig::new(base_node_custom, base_node_peers, peer_seeds);
    debug!(target: LOG_TARGET, "base node peer config: {:?}", peer_config);

    Ok(peer_config)
}

/// Determines which mode the wallet should run in.
pub(crate) fn wallet_mode(cli: &Cli, boot_mode: WalletBoot) -> WalletMode {
    // Recovery mode
    if matches!(boot_mode, WalletBoot::Recovery) {
        if cli.non_interactive_mode {
            return WalletMode::RecoveryDaemon;
        } else {
            return WalletMode::RecoveryTui;
        }
    }

    match (cli.non_interactive_mode, cli.input_file.clone(), cli.command.clone()) {
        // TUI mode
        (false, None, None) => WalletMode::Tui,
        // GRPC mode
        (true, None, None) => WalletMode::Grpc,
        // Script mode
        (_, Some(path), None) => WalletMode::Script(path),
        // Command mode
        (_, None, Some(command)) => WalletMode::Command(command),
        // Invalid combinations
        _ => WalletMode::Invalid,
    }
}

/// Set up the app environment and state for use by the UI
pub async fn init_wallet(
    config: &ApplicationConfig,
    arg_password: Option<String>,
    seed_words_file_name: Option<PathBuf>,
    recovery_seed: Option<CipherSeed>,
    shutdown_signal: ShutdownSignal,
) -> Result<WalletSqlite, ExitError> {
    fs::create_dir_all(
        &config
            .wallet
            .db_file
            .parent()
            .expect("console_wallet_db_file cannot be set to a root directory"),
    )
    .map_err(|e| ExitError::new(ExitCode::WalletError, &format!("Error creating Wallet folder. {}", e)))?;
    fs::create_dir_all(&config.wallet.p2p.datastore_path)
        .map_err(|e| ExitError::new(ExitCode::WalletError, &format!("Error creating peer db folder. {}", e)))?;

    debug!(target: LOG_TARGET, "Running Wallet database migrations");

    // test encryption by initializing with no passphrase...
    let db_path = &config.wallet.db_file;

    let result = initialize_sqlite_database_backends(db_path, None, config.wallet.connection_manager_pool_size);
    let (backends, wallet_encrypted) = match result {
        Ok(backends) => {
            // wallet is not encrypted
            (backends, false)
        },
        Err(WalletStorageError::NoPasswordError) => {
            // get supplied or prompt password
            let passphrase = get_or_prompt_password(arg_password.clone(), config.wallet.password.clone())?;
            let backends =
                initialize_sqlite_database_backends(db_path, passphrase, config.wallet.connection_manager_pool_size)?;
            (backends, true)
        },
        Err(e) => {
            return Err(e.into());
        },
    };
    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend, key_manager_backend) = backends;
    let wallet_db = WalletDatabase::new(wallet_backend);

    debug!(
        target: LOG_TARGET,
        "Databases Initialized. Wallet encrypted? {}.", wallet_encrypted
    );

    let node_address = match config.wallet.p2p.public_address.clone() {
        Some(addr) => addr,
        None => match wallet_db.get_node_address().await? {
            Some(addr) => addr,
            None => Multiaddr::empty(),
        },
    };

    let node_features = wallet_db
        .get_node_features()
        .await?
        .unwrap_or(PeerFeatures::COMMUNICATION_CLIENT);

    let identity_sig = wallet_db.get_comms_identity_signature().await?;

    let master_seed = read_or_create_master_seed(recovery_seed.clone(), &wallet_db).await?;
    let comms_secret_key = derive_comms_secret_key(&master_seed)?;

    // This checks if anything has changed by validating the previous signature and if invalid, setting identity_sig to
    // None
    let identity_sig = identity_sig.filter(|sig| {
        let comms_public_key = CommsPublicKey::from_secret_key(&comms_secret_key);
        sig.is_valid(&comms_public_key, node_features, [&node_address])
    });

    // SAFETY: we are manually checking the validity of this signature before adding Some(..)
    let node_identity = Arc::new(NodeIdentity::with_signature_unchecked(
        comms_secret_key,
        node_address,
        node_features,
        identity_sig,
    ));
    if !node_identity.is_signed() {
        node_identity.sign();
        // unreachable panic: signed above
        wallet_db
            .set_comms_identity_signature(
                node_identity
                    .identity_signature_read()
                    .as_ref()
                    .expect("unreachable panic")
                    .clone(),
            )
            .await?;
    }

    let mut wallet_config = config.wallet.clone();
    if let TransportType::Tor = config.wallet.p2p.transport.transport_type {
        wallet_config.p2p.transport.tor.identity = wallet_db.get_tor_id().await?;
    }

    let factories = CryptoFactories::default();

    let mut wallet = Wallet::start(
        config.wallet.clone(),
        config.peer_seeds.clone(),
        node_identity,
        factories,
        wallet_db,
        transaction_backend,
        output_manager_backend,
        contacts_backend,
        key_manager_backend,
        shutdown_signal,
        master_seed,
    )
    .await
    .map_err(|e| match e {
        WalletError::CommsInitializationError(CommsInitializationError::HiddenServiceControllerError(
            HiddenServiceControllerError::TorControlPortOffline,
        )) => ExitError::new(ExitCode::TorOffline, &e),
        WalletError::CommsInitializationError(e) => ExitError::new(ExitCode::WalletError, &e),
        e => ExitError::new(
            ExitCode::WalletError,
            &format!("Error creating Wallet Container: {}", e),
        ),
    })?;
    if let Some(hs) = wallet.comms.hidden_service() {
        wallet
            .db
            .set_tor_identity(hs.tor_identity().clone())
            .await
            .map_err(|e| ExitError::new(ExitCode::WalletError, &format!("Problem writing tor identity. {}", e)))?;
    }

    if !wallet_encrypted {
        debug!(target: LOG_TARGET, "Wallet is not encrypted.");

        // create using --password arg if supplied and skip seed words confirmation
        let (passphrase, interactive) = if let Some(password) = arg_password {
            debug!(target: LOG_TARGET, "Setting password from command line argument.");

            (password, false)
        } else {
            debug!(target: LOG_TARGET, "Prompting for password.");
            let password = prompt_password("Create wallet password: ")?;
            let confirmed = prompt_password("Confirm wallet password: ")?;

            if password != confirmed {
                return Err(ExitError::new(ExitCode::InputError, &"Passwords don't match!"));
            }

            (password, true)
        };

        wallet.apply_encryption(passphrase).await?;

        debug!(target: LOG_TARGET, "Wallet encrypted.");

        if interactive && recovery_seed.is_none() {
            match confirm_seed_words(&mut wallet).await {
                Ok(()) => {
                    print!("\x1Bc"); // Clear the screen
                },
                Err(error) => {
                    return Err(error);
                },
            };
        }
    }
    if let Some(file_name) = seed_words_file_name {
        let seed_words = wallet.get_seed_words(&MnemonicLanguage::English).await?.join(" ");
        let _result = fs::write(file_name, seed_words).map_err(|e| {
            ExitError::new(
                ExitCode::WalletError,
                &format!("Problem writing seed words to file: {}", e),
            )
        });
    };

    Ok(wallet)
}

/// Starts the wallet by setting the base node peer, and restarting the transaction and broadcast protocols.
pub async fn start_wallet(
    wallet: &mut WalletSqlite,
    base_node: &Peer,
    wallet_mode: &WalletMode,
) -> Result<(), ExitError> {
    // TODO gRPC interfaces for setting base node #LOGGED
    debug!(target: LOG_TARGET, "Setting base node peer");

    let net_address = base_node
        .addresses
        .first()
        .ok_or_else(|| ExitError::new(ExitCode::ConfigError, &"Configured base node has no address!"))?;

    wallet
        .set_base_node_peer(base_node.public_key.clone(), net_address.address.clone())
        .await
        .map_err(|e| {
            ExitError::new(
                ExitCode::WalletError,
                &format!("Error setting wallet base node peer. {}", e),
            )
        })?;

    // Restart transaction protocols if not running in script or command modes

    if !matches!(wallet_mode, WalletMode::Command(_)) && !matches!(wallet_mode, WalletMode::Script(_)) {
        if let Err(e) = wallet.transaction_service.restart_transaction_protocols().await {
            error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
        }
        if let Err(e) = wallet.transaction_service.validate_transactions().await {
            error!(
                target: LOG_TARGET,
                "Problem validating and restarting transaction protocols: {}", e
            );
        }

        // validate transaction outputs
        validate_txos(wallet).await?;
    }
    Ok(())
}

async fn validate_txos(wallet: &mut WalletSqlite) -> Result<(), ExitError> {
    debug!(target: LOG_TARGET, "Starting TXO validations.");

    wallet.output_manager_service.validate_txos().await.map_err(|e| {
        error!(target: LOG_TARGET, "Error validating Unspent TXOs: {}", e);
        ExitError::new(ExitCode::WalletError, &e)
    })?;

    debug!(target: LOG_TARGET, "TXO validations started.");

    Ok(())
}

async fn confirm_seed_words(wallet: &mut WalletSqlite) -> Result<(), ExitError> {
    let seed_words = wallet.get_seed_words(&MnemonicLanguage::English).await?;

    println!();
    println!("=========================");
    println!("       IMPORTANT!        ");
    println!("=========================");
    println!("These are your wallet seed words.");
    println!("They can be used to recover your wallet and funds.");
    println!("WRITE THEM DOWN OR COPY THEM NOW. THIS IS YOUR ONLY CHANCE TO DO SO.");
    println!();
    println!("=========================");
    println!("{}", seed_words.join(" "));
    println!("=========================");
    println!("\x07"); // beep!

    let mut rl = Editor::<()>::new();
    loop {
        println!("I confirm that I will never see these seed words again.");
        println!(r#"Type the word "confirm" to continue."#);
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => match line.to_lowercase().as_ref() {
                "confirm" => return Ok(()),
                _ => continue,
            },
            Err(e) => {
                return Err(ExitError::new(ExitCode::IOError, &e));
            },
        }
    }
}

/// Clear the terminal and print the Tari splash
pub fn tari_splash_screen(heading: &str) {
    // clear the terminal
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);

    println!("⠀⠀⠀⠀⠀⣠⣶⣿⣿⣿⣿⣶⣦⣀                                                         ");
    println!("⠀⢀⣤⣾⣿⡿⠋⠀⠀⠀⠀⠉⠛⠿⣿⣿⣶⣤⣀⠀⠀⠀⠀⠀⠀⢰⣿⣾⣾⣾⣾⣾⣾⣾⣾⣾⣿⠀⠀⠀⣾⣾⣾⡀⠀⠀⠀⠀⢰⣾⣾⣾⣾⣿⣶⣶⡀⠀⠀⠀⢸⣾⣿⠀");
    println!("⠀⣿⣿⣿⣿⣿⣶⣶⣤⣄⡀⠀⠀⠀⠀⠀⠉⠛⣿⣿⠀⠀⠀⠀⠀⠈⠉⠉⠉⠉⣿⣿⡏⠉⠉⠉⠉⠀⠀⣰⣿⣿⣿⣿⠀⠀⠀⠀⢸⣿⣿⠉⠉⠉⠛⣿⣿⡆⠀⠀⢸⣿⣿⠀");
    println!("⠀⣿⣿⠀⠀⠀⠈⠙⣿⡿⠿⣿⣿⣿⣶⣶⣤⣤⣿⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⠀⠀⢠⣿⣿⠃⣿⣿⣷⠀⠀⠀⢸⣿⣿⣀⣀⣀⣴⣿⣿⠃⠀⠀⢸⣿⣿⠀");
    println!("⠀⣿⣿⣤⠀⠀⠀⢸⣿⡟⠀⠀⠀⠀⠀⠉⣽⣿⣿⠟⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⠀⠀⣿⣿⣿⣤⣬⣿⣿⣆⠀⠀⢸⣿⣿⣿⣿⣿⡿⠟⠉⠀⠀⠀⢸⣿⣿⠀");
    println!("⠀⠀⠙⣿⣿⣤⠀⢸⣿⡟⠀⠀⠀⣠⣾⣿⡿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⠀⣾⣿⣿⠿⠿⠿⢿⣿⣿⡀⠀⢸⣿⣿⠙⣿⣿⣿⣄⠀⠀⠀⠀⢸⣿⣿⠀");
    println!("⠀⠀⠀⠀⠙⣿⣿⣼⣿⡟⣀⣶⣿⡿⠋⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣿⣿⡇⠀⠀⠀⣰⣿⣿⠃⠀⠀⠀⠀⣿⣿⣿⠀⢸⣿⣿⠀⠀⠙⣿⣿⣷⣄⠀⠀⢸⣿⣿⠀");
    println!("⠀⠀⠀⠀⠀⠀⠙⣿⣿⣿⣿⠛⠀                                                          ");
    println!("⠀⠀⠀⠀⠀⠀⠀⠀⠙⠁⠀                                                            ");
    println!("{}", heading);
    println!();
}

/// Prompts the user for a new wallet or to recover an existing wallet.
/// Returns the wallet bootmode indicating if it's a new or existing wallet, or if recovery is required.
pub(crate) fn boot(cli: &Cli, wallet_config: &WalletConfig) -> Result<WalletBoot, ExitError> {
    let wallet_exists = wallet_config.db_file.exists();

    // forced recovery
    if cli.recovery {
        if wallet_exists {
            return Err(ExitError::new(
                ExitCode::RecoveryError,
                &format!(
                    "Wallet already exists at {:#?}. Remove it if you really want to run recovery in this directory!",
                    wallet_config.db_file
                ),
            ));
        }
        return Ok(WalletBoot::Recovery);
    }

    if wallet_exists {
        // normal startup of existing wallet
        Ok(WalletBoot::Existing)
    } else {
        // automation/wallet created with --password
        if cli.password.is_some() {
            return Ok(WalletBoot::New);
        }

        // prompt for new or recovery
        let mut rl = Editor::<()>::new();

        loop {
            println!("1. Create a new wallet.");
            println!("2. Recover wallet from seed words.");
            let readline = rl.readline(">> ");
            match readline {
                Ok(line) => {
                    match line.as_ref() {
                        "1" | "c" | "n" | "create" => {
                            // new wallet
                            return Ok(WalletBoot::New);
                        },
                        "2" | "r" | "s" | "recover" => {
                            // recover wallet
                            return Ok(WalletBoot::Recovery);
                        },
                        _ => continue,
                    }
                },
                Err(e) => {
                    return Err(ExitError::new(ExitCode::IOError, &e));
                },
            }
        }
    }
}
