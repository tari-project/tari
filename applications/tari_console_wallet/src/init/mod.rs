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
use rpassword::prompt_password_stdout;
use std::{fs, str::FromStr, sync::Arc};
use tari_app_utilities::utilities::{setup_wallet_transport_type, ExitCodes};
use tari_common::{ConfigBootstrap, GlobalConfig, Network};
use tari_comms::{peer_manager::Peer, NodeIdentity};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_core::{consensus::Network as NetworkType, transactions::types::CryptoFactories};
use tari_p2p::{initialization::CommsConfig, seed_peer::SeedPeer, DEFAULT_DNS_SEED_RESOLVER};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    base_node_service::config::BaseNodeServiceConfig,
    error::{WalletError, WalletStorageError},
    output_manager_service::config::OutputManagerServiceConfig,
    storage::sqlite_utilities::initialize_sqlite_database_backends,
    transaction_service::config::TransactionServiceConfig,
    wallet::WalletConfig,
    Wallet,
    WalletSqlite,
};

pub const LOG_TARGET: &str = "wallet::console_wallet::init";
/// The minimum buffer size for a tari application pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;
const TARI_WALLET_PASSWORD: &str = "TARI_WALLET_PASSWORD";

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
    let password = prompt_password_stdout(prompt).map_err(|e| ExitCodes::IOError(e.to_string()))?;

    Ok(password)
}

pub async fn change_password(
    config: &GlobalConfig,
    node_identity: Arc<NodeIdentity>,
    arg_password: Option<String>,
    shutdown_signal: ShutdownSignal,
) -> Result<(), ExitCodes>
{
    let mut wallet = init_wallet(config, node_identity, arg_password, shutdown_signal).await?;

    let passphrase = prompt_password("New wallet password: ")?;
    let confirmed = prompt_password("Confirm new password: ")?;

    if passphrase != confirmed {
        return Err(ExitCodes::InputError("Passwords don't match!".to_string()));
    }

    wallet
        .remove_encryption()
        .await
        .map_err(|e| ExitCodes::WalletError(e.to_string()))?;

    wallet
        .apply_encryption(passphrase)
        .await
        .map_err(|e| ExitCodes::WalletError(e.to_string()))?;

    println!("Wallet password changed successfully.");

    Ok(())
}

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

pub fn wallet_mode(bootstrap: ConfigBootstrap) -> WalletMode {
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

/// Setup the app environment and state for use by the UI
pub async fn init_wallet(
    config: &GlobalConfig,
    node_identity: Arc<NodeIdentity>,
    arg_password: Option<String>,
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

    // TODO remove after next TestNet
    transaction_backend.migrate(node_identity.public_key().clone());

    let comms_config = CommsConfig {
        node_identity,
        user_agent: format!("tari/wallet/{}", env!("CARGO_PKG_VERSION")),
        transport_type: setup_wallet_transport_type(&config),
        datastore_path: config.console_wallet_peer_db_path.clone(),
        peer_database_name: "peers".to_string(),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        // TODO - make this configurable
        dht: DhtConfig {
            database_url: DbConnectionUrl::File(config.data_dir.join("dht-console-wallet.db")),
            auto_join: true,
            allow_test_addresses: config.allow_test_addresses,
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

    if !wallet_encrypted {
        debug!(target: LOG_TARGET, "Wallet is not encrypted.");

        // create using --password arg if supplied
        let passphrase = if let Some(password) = arg_password {
            debug!(target: LOG_TARGET, "Setting password from command line argument.");

            password
        } else {
            debug!(target: LOG_TARGET, "Prompting for password.");
            let password =
                prompt_password_stdout("Create wallet password: ").map_err(|e| ExitCodes::IOError(e.to_string()))?;
            let confirmed =
                prompt_password_stdout("Confirm wallet password: ").map_err(|e| ExitCodes::IOError(e.to_string()))?;

            if password != confirmed {
                return Err(ExitCodes::InputError("Passwords don't match!".to_string()));
            }

            password
        };

        wallet
            .apply_encryption(passphrase)
            .await
            .map_err(|e| ExitCodes::WalletError(e.to_string()))?;

        debug!(target: LOG_TARGET, "Wallet encrypted.");
    }

    Ok(wallet)
}

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
    if let Err(e) = wallet.transaction_service.restart_broadcast_protocols().await {
        error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
    }

    Ok(())
}
