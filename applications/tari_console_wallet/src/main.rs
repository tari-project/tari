#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
#![recursion_limit = "512"]
use log::*;
use std::{fs, str::FromStr, sync::Arc};
use structopt::StructOpt;
use tari_app_utilities::{
    identity_management::setup_node_identity,
    utilities::{setup_wallet_transport_type, ExitCodes},
};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig, Network};
use tari_comms::{
    peer_manager::{Peer, PeerFeatures},
    NodeIdentity,
};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_core::{consensus::Network as NetworkType, transactions::types::CryptoFactories};
use tari_p2p::{initialization::CommsConfig, seed_peer::SeedPeer, DEFAULT_DNS_SEED_RESOLVER};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_wallet::{
    base_node_service::config::BaseNodeServiceConfig,
    error::WalletError,
    storage::sqlite_utilities::initialize_sqlite_database_backends,
    transaction_service::config::TransactionServiceConfig,
    wallet::WalletConfig,
    Wallet,
    WalletSqlite,
};
use wallet_modes::{command_mode, grpc_mode, script_mode, tui_mode, WalletMode};

#[macro_use]
extern crate lazy_static;

pub const LOG_TARGET: &str = "wallet::console_wallet::main";
/// The minimum buffer size for a tari application pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;

mod automation;
mod dummy_data;
mod grpc;
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

    let base_nodes = get_base_node_peers(&config)?;
    let base_node = base_nodes.first().expect("No base node service peer defined.").clone();

    let wallet = runtime.block_on(setup_wallet(&config, node_identity, base_nodes, shutdown.to_signal()))?;

    debug!(target: LOG_TARGET, "Starting app");

    let handle = runtime.handle().clone();
    let result = match wallet_mode(bootstrap) {
        WalletMode::Tui => tui_mode(handle, config, wallet.clone(), base_node),
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

fn get_base_node_peers(config: &GlobalConfig) -> Result<Vec<Peer>, ExitCodes> {
    // get the configured base node service peers
    let mut base_node_peers = config
        .wallet_base_node_service_peers
        .iter()
        .map(|s| SeedPeer::from_str(s))
        .map(|r| r.map(Peer::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| ExitCodes::ConfigError(format!("Malformed base node service peer: {}", err)))?;

    // add the peer seeds, if fallback is configured
    if config.wallet_base_node_service_fallback_enabled {
        let peer_seeds = config
            .peer_seeds
            .iter()
            .map(|s| SeedPeer::from_str(s))
            .map(|r| r.map(Peer::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| ExitCodes::ConfigError(format!("Malformed seed peer: {}", err)))?;

        base_node_peers.extend(peer_seeds);
    }

    if base_node_peers.is_empty() {
        Err(ExitCodes::ConfigError(
            "No base node peers or peer seeds defined in config!".to_string(),
        ))
    } else {
        Ok(base_node_peers)
    }
}

fn wallet_mode(bootstrap: ConfigBootstrap) -> WalletMode {
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
async fn setup_wallet(
    config: &GlobalConfig,
    node_identity: Arc<NodeIdentity>,
    base_nodes: Vec<Peer>,
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
    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend) =
        initialize_sqlite_database_backends(config.console_wallet_db_file.clone(), None)
            .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)))?;
    debug!(target: LOG_TARGET, "Databases Initialized");

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
            ..Default::default()
        },
        // TODO: This should be false unless testing locally - make this configurable
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        dns_seeds_name_server: DEFAULT_DNS_SEED_RESOLVER.parse().unwrap(),
        peer_seeds: Default::default(),
        dns_seeds: Default::default(),
        dns_seeds_use_dnssec: true,
    };

    let network = match &config.network {
        Network::MainNet => NetworkType::MainNet,
        Network::Rincewind => NetworkType::Rincewind,
        Network::Ridcully => NetworkType::Ridcully,
        Network::LocalNet => NetworkType::LocalNet,
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
            direct_send_timeout: comms_config.dht.discovery_request_timeout,
            ..Default::default()
        }),
        network,
        Some(base_node_service_config),
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

    // TODO gRPC interfaces for setting base node
    debug!(target: LOG_TARGET, "Setting base node peer");
    wallet
        .set_base_node_peers(base_nodes)
        .await
        .map_err(|e| ExitCodes::WalletError(format!("Error setting wallet base node peer. {}", e)))?;

    // Restart transaction protocols
    if let Err(e) = wallet.transaction_service.restart_transaction_protocols().await {
        error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
    }
    if let Err(e) = wallet.transaction_service.restart_broadcast_protocols().await {
        error!(target: LOG_TARGET, "Problem restarting transaction protocols: {}", e);
    }

    Ok(wallet)
}
