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

use crate::{
    utils::db::get_custom_base_node_peer_from_db,
    wallet_modes::{PeerConfig, WalletMode},
};
use log::*;
use rand::rngs::OsRng;
use rpassword::prompt_password_stdout;
use rustyline::Editor;
use std::{fs, path::PathBuf, str::FromStr, sync::Arc};
use tari_app_utilities::utilities::{setup_wallet_transport_type, ExitCodes};
use tari_common::{ConfigBootstrap, GlobalConfig, Network};
use tari_comms::{
    peer_manager::{Peer, PeerFeatures},
    NodeIdentity,
};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_core::{
    consensus::Network as NetworkType,
    transactions::types::{CryptoFactories, PrivateKey},
};
use tari_crypto::keys::SecretKey;
use tari_p2p::{
    initialization::CommsConfig,
    seed_peer::SeedPeer,
    transport::TransportType::Tor,
    DEFAULT_DNS_SEED_RESOLVER,
};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    base_node_service::config::BaseNodeServiceConfig,
    error::{WalletError, WalletStorageError},
    output_manager_service::{
        config::OutputManagerServiceConfig,
        protocols::txo_validation_protocol::TxoValidationType,
        storage::{
            database::{
                DbKeyValuePair as OutputDbKeyValuePair,
                KeyManagerState,
                OutputManagerBackend,
                WriteOperation as OutputWriteOperation,
            },
            sqlite_db::OutputManagerSqliteDatabase,
        },
    },
    storage::{
        database::{DbKey, DbKeyValuePair, DbValue, WalletBackend, WriteOperation},
        sqlite_utilities::initialize_sqlite_database_backends,
    },
    transaction_service::{
        config::{TransactionRoutingMechanism, TransactionServiceConfig},
        tasks::start_transaction_validation_and_broadcast_protocols::start_transaction_validation_and_broadcast_protocols,
    },
    types::ValidationRetryStrategy,
    wallet::WalletConfig,
    Wallet,
    WalletSqlite,
};

pub const LOG_TARGET: &str = "wallet::console_wallet::init";
/// The minimum buffer size for a tari application pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;
const TARI_WALLET_PASSWORD: &str = "TARI_WALLET_PASSWORD";

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
) -> Result<Option<String>, ExitCodes>
{
    if arg_password.is_some() {
        return Ok(arg_password);
    }

    let env = std::env::var_os(TARI_WALLET_PASSWORD);
    if let Some(p) = env {
        let env_password = Some(
            p.into_string()
                .map_err(|_| ExitCodes::IOError("Failed to convert OsString into String".to_string()))?,
        );
        return Ok(env_password);
    }

    if config_password.is_some() {
        return Ok(config_password);
    }

    let password = prompt_password("Wallet password: ")?;

    Ok(Some(password))
}

fn prompt_password(prompt: &str) -> Result<String, ExitCodes> {
    let password = loop {
        let pass = prompt_password_stdout(prompt).map_err(|e| ExitCodes::IOError(e.to_string()))?;
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
    config: &GlobalConfig,
    arg_password: Option<String>,
    shutdown_signal: ShutdownSignal,
) -> Result<(), ExitCodes>
{
    let mut wallet = init_wallet(config, arg_password, None, shutdown_signal).await?;

    let passphrase = prompt_password("New wallet password: ")?;
    let confirmed = prompt_password("Confirm new password: ")?;

    if passphrase != confirmed {
        return Err(ExitCodes::InputError("Passwords don't match!".to_string()));
    }

    wallet.remove_encryption().await?;

    wallet.apply_encryption(passphrase).await?;

    println!("Wallet password changed successfully.");

    Ok(())
}

/// Populates the PeerConfig struct from:
/// 1. The custom peer in the wallet if it exists
/// 2. The service peers defined in config they exist
/// 3. The peer seeds defined in config
pub async fn get_base_node_peer_config(
    config: &GlobalConfig,
    wallet: &mut WalletSqlite,
) -> Result<PeerConfig, ExitCodes>
{
    // custom
    let base_node_custom = get_custom_base_node_peer_from_db(wallet).await;

    // config
    let base_node_peers = config
        .wallet_base_node_service_peers
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExitCodes::ConfigError(format!("Malformed base node peer: {}", err)))?;

    // peer seeds
    let peer_seeds = config
        .peer_seeds
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExitCodes::ConfigError(format!("Malformed seed peer: {}", err)))?;

    let peer_config = PeerConfig::new(base_node_custom, base_node_peers, peer_seeds);
    debug!(target: LOG_TARGET, "base node peer config: {:?}", peer_config);

    Ok(peer_config)
}

/// Determines which mode the wallet should run in.
pub fn wallet_mode(bootstrap: ConfigBootstrap, boot_mode: WalletBoot) -> WalletMode {
    // Recovery mode
    if matches!(boot_mode, WalletBoot::Recovery) {
        return WalletMode::Recovery;
    }

    match (bootstrap.daemon_mode, bootstrap.input_file, bootstrap.command) {
        // TUI mode
        (false, None, None) => WalletMode::Tui,
        // GRPC daemon mode
        (true, None, None) => WalletMode::Grpc,
        // Script mode
        (false, Some(path), None) => WalletMode::Script(path),
        // Command mode
        (false, None, Some(command)) => WalletMode::Command(command),
        // Invalid combinations
        _ => WalletMode::Invalid,
    }
}

/// Get the notify program script path from config bootstrap or global config if provided
pub fn get_notify_script(bootstrap: &ConfigBootstrap, config: &GlobalConfig) -> Result<Option<PathBuf>, ExitCodes> {
    debug!(target: LOG_TARGET, "Checking args and config for notify script.");

    let notify_script = match (&bootstrap.wallet_notify, &config.console_wallet_notify_file) {
        // command line arg
        (Some(path), None) => {
            info!(
                target: LOG_TARGET,
                "Notify script set from command line argument: {:#?}", path
            );
            Some(path.clone())
        },
        // config
        (None, Some(path)) => {
            info!(target: LOG_TARGET, "Notify script set from config: {:#?}", path);
            Some(path.clone())
        },
        // both arg and config, log and use the arg
        (Some(path), Some(_)) => {
            warn!(
                target: LOG_TARGET,
                "Wallet notify script set from both command line argument and config file! Using the command line \
                 argument: {:?}",
                path
            );
            Some(path.clone())
        },
        _ => None,
    };

    if let Some(path) = &notify_script {
        if !path.exists() {
            let error = format!("Wallet notify script does not exist at path: {:#?}", path);
            return Err(ExitCodes::ConfigError(error));
        }
    }

    Ok(notify_script)
}

/// Set up the app environment and state for use by the UI
pub async fn init_wallet(
    config: &GlobalConfig,
    arg_password: Option<String>,
    master_key: Option<PrivateKey>,
    shutdown_signal: ShutdownSignal,
) -> Result<WalletSqlite, ExitCodes>
{
    fs::create_dir_all(
        &config
            .console_wallet_db_file
            .parent()
            .expect("console_wallet_db_file cannot be set to a root directory"),
    )
    .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet folder. {}", e)))?;
    fs::create_dir_all(&config.console_wallet_peer_db_path)
        .map_err(|e| ExitCodes::WalletError(format!("Error creating peer db folder. {}", e)))?;

    debug!(target: LOG_TARGET, "Running Wallet database migrations");

    // test encryption by initializing with no passphrase...
    let db_path = config.console_wallet_db_file.clone();
    let result = initialize_sqlite_database_backends(db_path.clone(), None);
    let (backends, wallet_encrypted) = match result {
        Ok(backends) => {
            // wallet is not encrypted
            (backends, false)
        },
        Err(e) => {
            if matches!(e, WalletStorageError::NoPasswordError) {
                // get supplied or prompt password
                let passphrase = get_or_prompt_password(arg_password.clone(), config.console_wallet_password.clone())?;
                let backends = initialize_sqlite_database_backends(db_path, passphrase)
                    .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)))?;

                (backends, true)
            } else {
                return Err(e)
                    .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)));
            }
        },
    };
    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend) = backends;

    debug!(
        target: LOG_TARGET,
        "Databases Initialized. Wallet encrypted? {}.", wallet_encrypted
    );
    let node_identity = match wallet_backend
        .fetch(&DbKey::Identity)
        .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)))?
    {
        Some(DbValue::Identity(v)) => Arc::new(v),
        _ => {
            let private_key = PrivateKey::random(&mut OsRng);
            let node_id = NodeIdentity::new(
                private_key,
                config.public_address.clone(),
                PeerFeatures::COMMUNICATION_CLIENT,
            )
            .map_err(|e| ExitCodes::WalletError(format!("We were unable to construct a node identity. {}", e)))?;
            wallet_backend
                .write(WriteOperation::Insert(DbKeyValuePair::Identity(Box::new(
                    node_id.clone(),
                ))))
                .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)))?;
            Arc::new(node_id)
        },
    };

    let transport_type = setup_wallet_transport_type(&config);
    let transport_type = match transport_type {
        Tor(mut tor_config) => {
            tor_config.identity = match wallet_backend
                .fetch(&DbKey::TorId)
                .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)))?
            {
                Some(DbValue::TorId(v)) => Some(Box::new(v)),
                _ => None,
            };
            Tor(tor_config)
        },
        _ => transport_type,
    };

    let comms_config = CommsConfig {
        node_identity,
        user_agent: format!("tari/wallet/{}", env!("CARGO_PKG_VERSION")),
        transport_type,
        datastore_path: config.console_wallet_peer_db_path.clone(),
        peer_database_name: "peers".to_string(),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        // TODO - make this configurable
        dht: DhtConfig {
            database_url: DbConnectionUrl::File(config.data_dir.join("dht-console-wallet.db")),
            auto_join: true,
            allow_test_addresses: config.allow_test_addresses,
            network: config.network.into(),
            flood_ban_max_msg_count: config.flood_ban_max_msg_count,
            ..Default::default()
        },
        // TODO: This should be false unless testing locally - make this configurable
        allow_test_addresses: config.allow_test_addresses,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        dns_seeds_name_server: DEFAULT_DNS_SEED_RESOLVER.parse().unwrap(),
        peer_seeds: Default::default(),
        dns_seeds: Default::default(),
        dns_seeds_use_dnssec: true,
    };

    let network = match &config.network {
        Network::MainNet => NetworkType::MainNet,
        Network::Ridcully => NetworkType::Ridcully,
        Network::LocalNet => NetworkType::LocalNet,
        Network::Stibbons => NetworkType::Stibbons,
        Network::Rincewind => unimplemented!("Rincewind has been retired"),
    };

    let base_node_service_config = BaseNodeServiceConfig::new(
        config.wallet_base_node_service_refresh_interval,
        config.wallet_base_node_service_request_max_age,
    );

    let factories = CryptoFactories::default();
    let mut wallet_config = WalletConfig::new(
        comms_config.clone(),
        factories,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: config.transaction_broadcast_monitoring_timeout,
            chain_monitoring_timeout: config.transaction_chain_monitoring_timeout,
            direct_send_timeout: config.transaction_direct_send_timeout,
            broadcast_send_timeout: config.transaction_broadcast_send_timeout,
            transaction_routing_mechanism: TransactionRoutingMechanism::from(
                config.transaction_routing_mechanism.clone(),
            ),
            num_confirmations_required: config.transaction_num_confirmations_required,
            ..Default::default()
        }),
        Some(OutputManagerServiceConfig {
            base_node_query_timeout: config.base_node_query_timeout,
            prevent_fee_gt_amount: config.prevent_fee_gt_amount,
            ..Default::default()
        }),
        network,
        Some(base_node_service_config),
        Some(config.buffer_size_base_node_wallet),
        Some(config.buffer_rate_limit_base_node_wallet),
    );
    wallet_config.buffer_size = std::cmp::max(BASE_NODE_BUFFER_MIN_SIZE, config.buffer_size_base_node);

    let recovery = set_master_key(&output_manager_backend, master_key).await?;

    let mut wallet = Wallet::new(
        wallet_config,
        wallet_backend,
        transaction_backend,
        output_manager_backend,
        contacts_backend,
        shutdown_signal,
    )
    .await
    .map_err(|e| {
        if let WalletError::CommsInitializationError(e) = e {
            ExitCodes::WalletError(e.to_friendly_string())
        } else {
            ExitCodes::WalletError(format!("Error creating Wallet Container: {}", e))
        }
    })?;
    if let Some(hs) = wallet.comms.hidden_service() {
        wallet
            .db
            .set_tor_identity(hs.tor_identity().clone())
            .await
            .map_err(|e| ExitCodes::WalletError(format!("Problem writing tor identity. {}", e)))?;
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
                return Err(ExitCodes::InputError("Passwords don't match!".to_string()));
            }

            (password, true)
        };

        wallet.apply_encryption(passphrase).await?;

        debug!(target: LOG_TARGET, "Wallet encrypted.");

        if interactive && !recovery {
            confirm_seed_words(&mut wallet).await?;
        }
    }

    Ok(wallet)
}

/// If a master key is provided, set the initial key manager state to use that master key.
/// Returns true if the master key was provided, which means recovery is required.
async fn set_master_key(
    output_manager_backend: &OutputManagerSqliteDatabase,
    master_key: Option<PrivateKey>,
) -> Result<bool, ExitCodes>
{
    if let Some(master_key) = master_key {
        let state = KeyManagerState {
            master_key,
            branch_seed: "".to_string(),
            primary_key_index: 0,
        };

        output_manager_backend
            .write(OutputWriteOperation::Insert(OutputDbKeyValuePair::KeyManagerState(
                state,
            )))
            .map_err(|e| ExitCodes::IOError(e.to_string()))?;

        Ok(true)
    } else {
        Ok(false)
    }
}

/// Starts the wallet by setting the base node peer, and restarting the transaction and broadcast protocols.
pub async fn start_wallet(wallet: &mut WalletSqlite, base_node: &Peer) -> Result<(), ExitCodes> {
    // TODO gRPC interfaces for setting base node
    debug!(target: LOG_TARGET, "Setting base node peer");

    let net_address = base_node
        .addresses
        .first()
        .ok_or_else(|| ExitCodes::ConfigError("Configured base node has no address!".to_string()))?
        .to_string();

    wallet
        .set_base_node_peer(base_node.public_key.clone(), net_address)
        .await
        .map_err(|e| ExitCodes::WalletError(format!("Error setting wallet base node peer. {}", e)))?;

    // Restart transaction protocols
    if let Err(e) = wallet.transaction_service.restart_transaction_protocols().await {
        error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
    }
    if let Err(e) = start_transaction_validation_and_broadcast_protocols(
        wallet.transaction_service.clone(),
        ValidationRetryStrategy::UntilSuccess,
    )
    .await
    {
        error!(
            target: LOG_TARGET,
            "Problem validating and restarting transaction protocols: {}", e
        );
    }

    // validate transaction outputs
    validate_txos(wallet).await?;

    Ok(())
}

async fn validate_txos(wallet: &mut WalletSqlite) -> Result<(), ExitCodes> {
    debug!(target: LOG_TARGET, "Starting TXO validations.");

    // Unspent TXOs
    wallet
        .output_manager_service
        .validate_txos(TxoValidationType::Unspent, ValidationRetryStrategy::UntilSuccess)
        .await
        .map_err(|e| {
            error!(target: LOG_TARGET, "Error validating Unspent TXOs: {}", e);
            ExitCodes::WalletError(e.to_string())
        })?;

    // Spent TXOs
    wallet
        .output_manager_service
        .validate_txos(TxoValidationType::Spent, ValidationRetryStrategy::UntilSuccess)
        .await
        .map_err(|e| {
            error!(target: LOG_TARGET, "Error validating Spent TXOs: {}", e);
            ExitCodes::WalletError(e.to_string())
        })?;

    // Invalid TXOs
    wallet
        .output_manager_service
        .validate_txos(TxoValidationType::Invalid, ValidationRetryStrategy::UntilSuccess)
        .await
        .map_err(|e| {
            error!(target: LOG_TARGET, "Error validating Invalid TXOs: {}", e);
            ExitCodes::WalletError(e.to_string())
        })?;

    debug!(target: LOG_TARGET, "TXO validations completed.");

    Ok(())
}

async fn confirm_seed_words(wallet: &mut WalletSqlite) -> Result<(), ExitCodes> {
    let seed_words = wallet.output_manager_service.get_seed_words().await?;

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
                return Err(ExitCodes::IOError(e.to_string()));
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
pub(crate) fn boot(bootstrap: &ConfigBootstrap, config: &GlobalConfig) -> Result<WalletBoot, ExitCodes> {
    let wallet_exists = config.console_wallet_db_file.exists();

    // forced recovery
    if bootstrap.recovery {
        if wallet_exists {
            return Err(ExitCodes::RecoveryError(format!(
                "Wallet already exists at {:#?}. Remove it if you really want to run recovery in this directory!",
                config.console_wallet_db_file
            )));
        }
        return Ok(WalletBoot::Recovery);
    }

    if wallet_exists {
        // normal startup of existing wallet
        Ok(WalletBoot::Existing)
    } else {
        // automation/wallet created with --password
        if bootstrap.password.is_some() {
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
                    return Err(ExitCodes::IOError(e.to_string()));
                },
            }
        }
    }
}
