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

#![allow(dead_code, unused)]

use std::{fs, io, path::PathBuf, str::FromStr, sync::Arc, time::Instant};

#[cfg(feature = "ledger")]
use ledger_transport_hid::{hidapi::HidApi, TransportNativeHID};
use log::*;
use minotari_app_utilities::identity_management::setup_node_identity;
use minotari_wallet::{
    error::{WalletError, WalletStorageError},
    output_manager_service::storage::database::OutputManagerDatabase,
    storage::{
        database::{WalletBackend, WalletDatabase},
        sqlite_utilities::initialize_sqlite_database_backends,
    },
    wallet::{derive_comms_secret_key, read_or_create_master_seed, read_or_create_wallet_type},
    Wallet,
    WalletConfig,
    WalletSqlite,
};
use rpassword::prompt_password_stdout;
use rustyline::Editor;
use tari_common::{
    configuration::{
        bootstrap::{grpc_default_port, prompt, ApplicationType},
        MultiaddrList,
        Network,
    },
    exit_codes::{ExitCode, ExitError},
};
use tari_common_types::wallet_types::WalletType;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures, PeerQuery},
    types::CommsPublicKey,
    NodeIdentity,
};
use tari_core::{consensus::ConsensusManager, transactions::CryptoFactories};
use tari_crypto::keys::PublicKey;
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::MnemonicLanguage};
use tari_p2p::{peer_seeds::SeedPeer, TransportType};
use tari_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, ByteArray, SafePassword};
use zxcvbn::zxcvbn;

use crate::{
    cli::Cli,
    utils::db::{get_custom_base_node_peer_from_db, set_custom_base_node_peer_in_db},
    wallet_modes::{PeerConfig, WalletMode},
    ApplicationConfig,
};

pub const LOG_TARGET: &str = "wallet::console_wallet::init";
const TARI_WALLET_PASSWORD: &str = "MINOTARI_WALLET_PASSWORD";
// Maxmimum number of times we prompt for confirmation of a new passphrase, to avoid driving the user insane with an
// infinite loop
const PASSPHRASE_SANITY_LIMIT: u8 = 3;

#[derive(Clone, Copy)]
pub enum WalletBoot {
    New,
    Existing,
    Recovery,
}

/// Get and confirm a passphrase from the user, with feedback
/// This is intended to be used for new or changed passphrases
///
/// You must provide the initial and confirmation prompts to pass to the user
///
/// We do several things:
/// - Prompt the user for a passphrase
/// - Have the user confirm the passphrase
/// - Score the passphrase
/// - If the passphrase is weak (or empty), give feedback and ask the user what to do:
///   - Proceed with the weak (or empty) passphrase
///   - Choose a better passphrase
///   - Cancel the operation
///
/// If the passphrase and confirmation don't match, or if the user cancels, returns an error
/// Otherwise, returns the passphrase as a `SafePassword`
fn get_new_passphrase(prompt: &str, confirm: &str) -> Result<SafePassword, ExitError> {
    // We may need to prompt for a passphrase multiple times
    loop {
        // Prompt the user for a passphrase and confirm it, up to the defined limit
        // This ensures an unlucky user doesn't get stuck
        let mut tries = 0;
        let mut passphrase = SafePassword::from(""); // initial value for scope
        loop {
            passphrase = prompt_password(prompt)?;
            let confirmed = prompt_password(confirm)?;

            // If they match, continue the process
            if passphrase.reveal() == confirmed.reveal() {
                break;
            }

            // If they don't match, keep prompting until we hit the sanity limit
            tries += 1;
            if tries == PASSPHRASE_SANITY_LIMIT {
                return Err(ExitError::new(ExitCode::InputError, "Passphrases don't match!"));
            }
            println!("Passphrases don't match! Try again.");
        }

        // Score the passphrase and provide feedback
        let weak = display_password_feedback(&passphrase);

        // If the passphrase is weak, see if the user wishes to change it
        if weak {
            println!("Would you like to choose a different passphrase?");
            println!("  y/Y: Yes, choose a different passphrase");
            println!("  n/N: No, use this passphrase");
            println!("  Enter anything else if you changed your mind and want to cancel");

            let mut input = "".to_string();
            std::io::stdin().read_line(&mut input);

            match input.trim().to_lowercase().as_str() {
                // Choose a different passphrase
                "y" => {
                    continue;
                },
                // Use this passphrase
                "n" => {
                    return Ok(passphrase);
                },
                // By default, we cancel to be safe
                _ => {
                    return Err(ExitError::new(
                        ExitCode::InputError,
                        "Canceling with unchanged passphrase!",
                    ));
                },
            }
        } else {
            // The passphrase is fine, so return it
            return Ok(passphrase);
        }
    }
}

/// Get feedback, if available, for a weak passphrase
fn get_password_feedback(passphrase: &SafePassword) -> Option<Vec<String>> {
    std::str::from_utf8(passphrase.reveal())
        .ok()
        .and_then(|passphrase| zxcvbn(passphrase, &[]).ok())
        .and_then(|scored| scored.feedback().to_owned())
        .map(|feedback| feedback.suggestions().to_owned())
        .map(|suggestion| suggestion.into_iter().map(|item| item.to_string()).collect())
}

/// Display passphrase feedback to the user
///
/// Returns `true` if and only if the passphrase is weak
fn display_password_feedback(passphrase: &SafePassword) -> bool {
    if passphrase.reveal().is_empty() {
        // The passphrase is empty, which the scoring library doesn't handle
        println!();
        println!("An empty password puts your wallet at risk against an attacker with access to this device.");
        println!("Use this only if you are sure that your device is safe from prying eyes!");
        println!();

        true
    } else if let Some(feedback) = get_password_feedback(passphrase) {
        // The scoring library provided feedback
        println!();
        println!(
            "The password you chose is weak; a determined attacker with access to your device may be able to guess it."
        );
        println!("You may want to consider changing it to a stronger one.");
        println!("Here are some suggestions:");
        for suggestion in feedback {
            println!("- {}", suggestion);
        }
        println!();

        true
    } else {
        // The Force is strong with this one
        false
    }
}

/// Gets the password provided by command line argument or environment variable if available.
/// Otherwise prompts for the password to be typed in.
pub fn get_or_prompt_password(
    arg_password: Option<SafePassword>,
    config_password: Option<SafePassword>,
) -> Result<SafePassword, ExitError> {
    if let Some(passphrase) = arg_password {
        return Ok(passphrase);
    }

    let env = std::env::var_os(TARI_WALLET_PASSWORD);
    if let Some(p) = env {
        let env_password = p
            .into_string()
            .map_err(|_| ExitError::new(ExitCode::IOError, "Failed to convert OsString into String"))?;
        return Ok(env_password.into());
    }

    if let Some(passphrase) = config_password {
        return Ok(passphrase);
    }

    let password = prompt_password("Wallet password: ")?;

    Ok(password)
}

fn prompt_password(prompt: &str) -> Result<SafePassword, ExitError> {
    let password = prompt_password_stdout(prompt).map_err(|e| ExitError::new(ExitCode::IOError, e))?;

    Ok(SafePassword::from(password))
}

/// Allows the user to change the password of the wallet.
pub async fn change_password(
    config: &ApplicationConfig,
    existing: SafePassword,
    shutdown_signal: ShutdownSignal,
    non_interactive_mode: bool,
) -> Result<(), ExitError> {
    let mut wallet = init_wallet(
        config,
        existing.clone(),
        None,
        None,
        shutdown_signal,
        non_interactive_mode,
        None,
    )
    .await?;

    // Get a new passphrase
    let new = get_new_passphrase("New wallet passphrase: ", "Confirm new passphrase: ")?;

    // Use the existing and new passphrases to attempt to change the wallet passphrase
    wallet.db.change_passphrase(&existing, &new).map_err(|e| match e {
        WalletStorageError::InvalidPassphrase => {
            ExitError::new(ExitCode::IncorrectOrEmptyPassword, "Your password was not changed.")
        },
        _ => ExitError::new(ExitCode::DatabaseError, "Your password was not changed."),
    })
}

/// Populates the PeerConfig struct from:
/// 1. The custom peer in the wallet config if it exists
/// 2. The custom peer in the wallet db if it exists
/// 3. The detected local base node if any
/// 4. The service peers defined in config they exist
/// 5. The peer seeds defined in config
pub async fn get_base_node_peer_config(
    config: &ApplicationConfig,
    wallet: &mut WalletSqlite,
    non_interactive_mode: bool,
) -> Result<PeerConfig, ExitError> {
    let mut use_custom_base_node_peer = false;
    let mut selected_base_node = match config.wallet.custom_base_node {
        Some(ref custom) => SeedPeer::from_str(custom)
            .map(|node| Some(Peer::from(node)))
            .map_err(|err| ExitError::new(ExitCode::ConfigError, format!("Malformed custom base node: {}", err)))?,
        None => {
            if let Some(custom_base_node_peer) = get_custom_base_node_peer_from_db(wallet) {
                use_custom_base_node_peer = true;
                Some(custom_base_node_peer)
            } else {
                None
            }
        },
    };

    // If the user has not explicitly set a base node in the config, we try detect one
    if !non_interactive_mode && config.wallet.custom_base_node.is_none() && !use_custom_base_node_peer {
        if let Some(detected_node) = detect_local_base_node(config.wallet.network).await {
            match selected_base_node {
                Some(ref base_node) if base_node.public_key == detected_node.public_key => {
                    // Skip asking because it's already set
                },
                Some(_) | None => {
                    println!(
                        "Local Base Node detected with public key {} and address {}",
                        detected_node.public_key,
                        detected_node
                            .addresses
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    if prompt(
                        "Would you like to use this base node? IF YOU DID NOT START THIS BASE NODE YOU SHOULD SELECT \
                         NO (Y/n)",
                    ) {
                        let address = detected_node.addresses.first().ok_or_else(|| {
                            ExitError::new(ExitCode::ConfigError, "No address found for detected base node")
                        })?;
                        set_custom_base_node_peer_in_db(wallet, &detected_node.public_key, address)?;
                        selected_base_node = Some(detected_node.into());
                    }
                },
            }
        }
    }
    let query = PeerQuery::new().select_where(|p| p.is_seed());
    let peer_seeds = wallet.comms.peer_manager().perform_query(query).await.map_err(|err| {
        ExitError::new(
            ExitCode::InterfaceError,
            format!("Could net get seed peers from peer manager: {}", err),
        )
    })?;

    // config
    let base_node_peers = config
        .wallet
        .base_node_service_peers
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExitError::new(ExitCode::ConfigError, format!("Malformed base node peer: {}", err)))?;

    let peer_config = PeerConfig::new(selected_base_node, base_node_peers, peer_seeds);
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

    match (cli.non_interactive_mode, cli.input_file.clone(), cli.command2.clone()) {
        // TUI mode
        (false, None, None) => WalletMode::Tui,
        // GRPC mode
        (true, None, None) => WalletMode::Grpc,
        // Script mode
        (_, Some(path), None) => WalletMode::Script(path),
        // Command mode
        (_, None, Some(command)) => WalletMode::Command(Box::new(command)), // WalletMode::Command(command),
        // Invalid combinations
        _ => WalletMode::Invalid,
    }
}

/// Set up the app environment and state for use by the UI
#[allow(clippy::too_many_lines)]
pub async fn init_wallet(
    config: &ApplicationConfig,
    arg_password: SafePassword,
    seed_words_file_name: Option<PathBuf>,
    recovery_seed: Option<CipherSeed>,
    shutdown_signal: ShutdownSignal,
    non_interactive_mode: bool,
    wallet_type: Option<WalletType>,
) -> Result<WalletSqlite, ExitError> {
    fs::create_dir_all(
        config
            .wallet
            .db_file
            .parent()
            .expect("console_wallet_db_file cannot be set to a root directory"),
    )
    .map_err(|e| ExitError::new(ExitCode::WalletError, format!("Error creating Wallet folder. {}", e)))?;
    fs::create_dir_all(&config.wallet.p2p.datastore_path)
        .map_err(|e| ExitError::new(ExitCode::WalletError, format!("Error creating peer db folder. {}", e)))?;

    debug!(target: LOG_TARGET, "Running Wallet database migrations");

    let db_path = &config.wallet.db_file;

    // wallet should be encrypted from the beginning, so we must require a password to be provided by the user
    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend, key_manager_backend) =
        initialize_sqlite_database_backends(db_path, arg_password, config.wallet.db_connection_pool_size)?;

    let wallet_db = WalletDatabase::new(wallet_backend);
    let output_db = OutputManagerDatabase::new(output_manager_backend.clone());

    debug!(target: LOG_TARGET, "Databases Initialized. Wallet is encrypted.",);

    let node_addresses = if config.wallet.p2p.public_addresses.is_empty() {
        match wallet_db.get_node_address()? {
            Some(addr) => MultiaddrList::from(vec![addr]),
            None => MultiaddrList::default(),
        }
    } else {
        config.wallet.p2p.public_addresses.clone()
    };

    let master_seed = read_or_create_master_seed(recovery_seed.clone(), &wallet_db)?;
    let wallet_type = read_or_create_wallet_type(wallet_type, &wallet_db);

    let node_identity = match config.wallet.identity_file.as_ref() {
        Some(identity_file) => {
            warn!(
                target: LOG_TARGET,
                "Node identity overridden by file {}",
                identity_file.to_string_lossy()
            );
            setup_node_identity(
                identity_file,
                node_addresses.to_vec(),
                true,
                PeerFeatures::COMMUNICATION_CLIENT,
            )?
        },
        None => setup_identity_from_db(&wallet_db, &master_seed, node_addresses.to_vec())?,
    };

    let mut wallet_config = config.wallet.clone();
    if let TransportType::Tor = config.wallet.p2p.transport.transport_type {
        wallet_config.p2p.transport.tor.identity = wallet_db.get_tor_id()?;
    }

    let consensus_manager = ConsensusManager::builder(config.wallet.network)
        .build()
        .map_err(|e| ExitError::new(ExitCode::WalletError, format!("Error consensus manager. {}", e)))?;
    let factories = CryptoFactories::default();

    let now = Instant::now();

    let mut wallet = Wallet::start(
        wallet_config,
        config.peer_seeds.clone(),
        config.auto_update.clone(),
        node_identity,
        consensus_manager,
        factories,
        wallet_db,
        output_db,
        transaction_backend,
        output_manager_backend,
        contacts_backend,
        key_manager_backend,
        shutdown_signal,
        master_seed,
        wallet_type.unwrap(),
    )
    .await
    .map_err(|e| match e {
        WalletError::CommsInitializationError(cie) => cie.to_exit_error(),
        e => ExitError::new(ExitCode::WalletError, format!("Error creating Wallet Container: {}", e)),
    })?;

    error!(
        target: LOG_TARGET,
        "Wallet started in {}ms", now.elapsed().as_millis()
    );

    if let Some(file_name) = seed_words_file_name {
        let seed_words = wallet.get_seed_words(&MnemonicLanguage::English)?.join(" ");
        let _result = fs::write(file_name, seed_words.reveal()).map_err(|e| {
            ExitError::new(
                ExitCode::WalletError,
                format!("Problem writing seed words to file: {}", e),
            )
        });
    };

    Ok(wallet)
}

async fn detect_local_base_node(network: Network) -> Option<SeedPeer> {
    use minotari_app_grpc::tari_rpc::{base_node_client::BaseNodeClient, Empty};
    let addr = format!(
        "http://127.0.0.1:{}",
        grpc_default_port(ApplicationType::BaseNode, network)
    );
    debug!(target: LOG_TARGET, "Checking for local base node at {}", addr);

    let mut node_conn = match BaseNodeClient::connect(addr).await.ok() {
        Some(conn) => conn,
        None => {
            debug!(target: LOG_TARGET, "No local base node detected");
            return None;
        },
    };
    let resp = node_conn.identify(Empty {}).await.ok()?;
    let identity = resp.get_ref();
    let public_key = CommsPublicKey::from_canonical_bytes(&identity.public_key).ok()?;
    let addresses = identity
        .public_addresses
        .iter()
        .filter_map(|s| Multiaddr::from_str(s).ok())
        .collect::<Vec<_>>();
    debug!(
        target: LOG_TARGET,
        "Local base node found with pk={} and addresses={}",
        public_key.to_hex(),
        addresses.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(",")
    );
    Some(SeedPeer::new(public_key, addresses))
}

fn setup_identity_from_db<D: WalletBackend + 'static>(
    wallet_db: &WalletDatabase<D>,
    master_seed: &CipherSeed,
    node_addresses: Vec<Multiaddr>,
) -> Result<Arc<NodeIdentity>, ExitError> {
    let node_features = wallet_db
        .get_node_features()?
        .unwrap_or(PeerFeatures::COMMUNICATION_CLIENT);

    let identity_sig = wallet_db.get_comms_identity_signature()?;

    let comms_secret_key = derive_comms_secret_key(master_seed)?;

    // This checks if anything has changed by validating the previous signature and if invalid, setting identity_sig
    // to None
    let identity_sig = identity_sig.filter(|sig| {
        let comms_public_key = CommsPublicKey::from_secret_key(&comms_secret_key);
        sig.is_valid(&comms_public_key, node_features, &node_addresses)
    });

    // SAFETY: we are manually checking the validity of this signature before adding Some(..)
    let node_identity = Arc::new(NodeIdentity::with_signature_unchecked(
        comms_secret_key,
        node_addresses,
        node_features,
        identity_sig,
    ));
    if !node_identity.is_signed() {
        node_identity.sign();
        // unreachable panic: signed above
        let sig = node_identity
            .identity_signature_read()
            .as_ref()
            .expect("unreachable panic")
            .clone();
        wallet_db.set_comms_identity_signature(sig)?;
    }

    Ok(node_identity)
}

/// Starts the wallet by setting the base node peer, and restarting the transaction and broadcast protocols.
pub async fn start_wallet(
    wallet: &mut WalletSqlite,
    base_node: &Peer,
    wallet_mode: &WalletMode,
) -> Result<(), ExitError> {
    debug!(target: LOG_TARGET, "Setting base node peer");

    let net_address = base_node
        .addresses
        .best()
        .ok_or_else(|| ExitError::new(ExitCode::ConfigError, "Configured base node has no address!"))?;

    wallet
        .set_base_node_peer(base_node.public_key.clone(), Some(net_address.address().clone()))
        .await
        .map_err(|e| {
            ExitError::new(
                ExitCode::WalletError,
                format!("Error setting wallet base node peer. {}", e),
            )
        })?;

    // Restart transaction protocols if not running in script or command modes
    if !matches!(wallet_mode, WalletMode::Command(_)) && !matches!(wallet_mode, WalletMode::Script(_)) {
        // NOTE: https://github.com/tari-project/tari/issues/5227
        debug!("revalidating all transactions");
        if let Err(e) = wallet.transaction_service.revalidate_all_transactions().await {
            error!(target: LOG_TARGET, "Failed to revalidate all transactions: {}", e);
        }

        debug!("restarting transaction protocols");
        if let Err(e) = wallet.transaction_service.restart_transaction_protocols().await {
            error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
        }

        debug!("validating transactions");
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
        ExitError::new(ExitCode::WalletError, e)
    })?;

    debug!(target: LOG_TARGET, "TXO validations started.");

    Ok(())
}

pub(crate) fn confirm_seed_words(wallet: &mut WalletSqlite) -> Result<(), ExitError> {
    let seed_words = wallet.get_seed_words(&MnemonicLanguage::English)?;

    println!();
    println!("=========================");
    println!("       IMPORTANT!        ");
    println!("=========================");
    println!("These are your wallet seed words.");
    println!("They can be used to recover your wallet and funds.");
    println!("WRITE THEM DOWN OR COPY THEM NOW. THIS IS YOUR ONLY CHANCE TO DO SO.");
    println!();
    println!("=========================");
    println!("{}", seed_words.join(" ").reveal());
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
                return Err(ExitError::new(ExitCode::IOError, e));
            },
        }
    }
}

pub(crate) fn confirm_direct_only_send(wallet: &mut WalletSqlite) -> Result<(), ExitError> {
    let seed_words = wallet.get_seed_words(&MnemonicLanguage::English)?;

    println!();
    println!("=========================");
    println!("       IMPORTANT!        ");
    println!("=========================");
    println!("This wallet is set to use DirectOnly.");
    println!("This is primarily used for testing and can result in not all messages being sent.");
    println!();
    println!("\x07"); // beep!

    let mut rl = Editor::<()>::new();
    loop {
        println!("I confirm this warning.");
        println!(r#"Type the word "confirm" , "yes" or "y" to continue."#);
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => match line.to_lowercase().as_ref() {
                "confirm" | "yes" | "y" => return Ok(()),
                _ => continue,
            },
            Err(e) => {
                return Err(ExitError::new(ExitCode::IOError, e));
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
fn boot(cli: &Cli, wallet_config: &WalletConfig) -> Result<WalletBoot, ExitError> {
    let wallet_exists = wallet_config.db_file.exists();

    // forced recovery
    if cli.recovery {
        if wallet_exists {
            return Err(ExitError::new(
                ExitCode::RecoveryError,
                format!(
                    "Wallet already exists at {:#?}. Remove it if you really want to run recovery in this directory!",
                    wallet_config.db_file
                ),
            ));
        }
        return Ok(WalletBoot::Recovery);
    }

    if cli.seed_words.is_some() && !wallet_exists {
        return Ok(WalletBoot::Recovery);
    }

    if wallet_exists {
        // normal startup of existing wallet
        Ok(WalletBoot::Existing)
    } else {
        // automation/wallet created with --password
        if cli.password.is_some() || wallet_config.password.is_some() {
            return Ok(WalletBoot::New);
        }

        // In non-interactive mode, we never prompt. Otherwise, it's not very non-interactive, now is it?
        if cli.non_interactive_mode {
            let msg = "Wallet does not exist and no password was given to create one. Since we're in non-interactive \
                       mode, we need to quit here. Try setting the MINOTARI_WALLET__PASSWORD envar, or setting \
                       --password on the command line";
            return Err(ExitError::new(ExitCode::WalletError, msg));
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
                    return Err(ExitError::new(ExitCode::IOError, e));
                },
            }
        }
    }
}

pub(crate) fn boot_with_password(
    cli: &Cli,
    wallet_config: &WalletConfig,
) -> Result<(WalletBoot, SafePassword), ExitError> {
    let boot_mode = boot(cli, wallet_config)?;

    if cli.password.is_some() {
        return Ok((boot_mode, cli.password.clone().unwrap()));
    }
    if wallet_config.password.is_some() {
        return Ok((boot_mode, wallet_config.password.clone().unwrap()));
    }

    let password = match boot_mode {
        // A new wallet requires entering and confirming a passphrase
        WalletBoot::New => {
            debug!(target: LOG_TARGET, "Prompting for passphrase for new wallet.");
            get_new_passphrase("Create wallet passphrase: ", "Confirm wallet passphrase: ")?
        },
        // Recovery from a seed requires entering and confirming a passphrase
        WalletBoot::Recovery => {
            debug!(target: LOG_TARGET, "Prompting for passphrase for wallet recovery.");
            get_new_passphrase("Create wallet passphrase: ", "Confirm wallet passphrase: ")?
        },
        // Opening an existing wallet only requires entering a passphrase
        WalletBoot::Existing => {
            debug!(target: LOG_TARGET, "Prompting for passphrase for existing wallet.");
            prompt_password("Enter wallet passphrase: ")?
        },
    };

    Ok((boot_mode, password))
}

pub fn prompt_wallet_type(
    boot_mode: WalletBoot,
    wallet_config: &WalletConfig,
    non_interactive: bool,
) -> Option<WalletType> {
    if non_interactive {
        return Some(WalletType::Software);
    }

    if wallet_config.wallet_type.is_some() {
        return wallet_config.wallet_type;
    }

    match boot_mode {
        WalletBoot::New => {
            #[cfg(not(feature = "ledger"))]
            return Some(WalletType::Software);

            #[cfg(feature = "ledger")]
            {
                if prompt("\r\nWould you like to use a connected hardware wallet? (Supported types: Ledger)") {
                    print!("Scanning for connected Ledger hardware device... ");
                    let err = "No connected device was found. Please make sure the device is plugged in before
            continuing.";
                    match TransportNativeHID::new(&HidApi::new().expect(err)) {
                        Ok(_) => {
                            println!("Device found.");
                            let account = prompt_ledger_account().expect("An account value");
                            Some(WalletType::Ledger(account))
                        },
                        Err(e) => panic!("{}", e),
                    }
                } else {
                    Some(WalletType::Software)
                }
            }
        },
        _ => None,
    }
}

pub fn prompt_ledger_account() -> Option<usize> {
    let question =
        "\r\nPlease enter an account number for your ledger. A simple 1-9, easily remembered numbers are suggested.";
    println!("{}", question);
    let mut input = "".to_string();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();
    match input.parse() {
        Ok(num) => Some(num),
        Err(_e) => Some(1),
    }
}

#[cfg(test)]
mod test {
    use tari_utilities::SafePassword;

    use super::get_password_feedback;

    #[test]
    fn weak_password() {
        let weak_password = SafePassword::from("weak");
        assert!(get_password_feedback(&weak_password).is_some());
    }

    #[test]
    fn strong_password() {
        let strong_password = SafePassword::from("This is a reasonably strong password!");
        assert!(get_password_feedback(&strong_password).is_none());
    }
}
