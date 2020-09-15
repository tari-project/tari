#![cfg_attr(not(debug_assertions), deny(unused_variables))]
#![cfg_attr(not(debug_assertions), deny(unused_imports))]
#![cfg_attr(not(debug_assertions), deny(dead_code))]
#![cfg_attr(not(debug_assertions), deny(unused_extern_crates))]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(unknown_lints)]
use crate::{grpc::WalletGrpcServer, ui::App};
use log::*;
use std::{io::Stdout, net::SocketAddr, sync::Arc};
use structopt::StructOpt;
use tari_app_utilities::{
    identity_management::setup_node_identity,
    utilities::{
        create_peer_db_folder,
        create_wallet_folder,
        parse_peer_seeds,
        setup_wallet_transport_type,
        ExitCodes,
    },
};
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, GlobalConfig, Network};
use tari_comms::{peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_core::{consensus::Network as NetworkType, transactions::types::CryptoFactories};
use tari_p2p::initialization::CommsConfig;
use tari_wallet::{
    error::WalletError,
    storage::sqlite_utilities::initialize_sqlite_database_backends,
    transaction_service::config::TransactionServiceConfig,
    wallet::WalletConfig,
    Wallet,
    WalletSqlite,
};
use tokio::sync::RwLock;
use tonic::transport::Server;
use tui::backend::CrosstermBackend;

#[macro_use]
extern crate lazy_static;

pub const LOG_TARGET: &str = "wallet::app::main";
/// The minimum buffer size for a tari application pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;

mod dummy_data;
mod grpc;
mod ui;
mod utils;

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
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(ApplicationType::ConsoleWallet)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    // Populate the configuration struct
    let node_config = GlobalConfig::convert_from(cfg).map_err(|err| {
        error!(target: LOG_TARGET, "The configuration file has an error. {}", err);
        ExitCodes::ConfigError
    })?;

    debug!(target: LOG_TARGET, "Using configuration: {:?}", node_config);
    // Load or create the Node identity
    let wallet_identity = setup_node_identity(
        &node_config.wallet_identity_file,
        &node_config.public_address,
        bootstrap.create_id ||
            // If the base node identity exists, we want to be sure that the wallet identity exists
            node_config.identity_file.exists(),
        PeerFeatures::COMMUNICATION_CLIENT,
    )?;

    // Exit if create_id or init arguments were run
    if bootstrap.create_id {
        info!(
            target: LOG_TARGET,
            "Node ID created at '{}'. Done.",
            node_config.identity_file.to_string_lossy()
        );
        return Ok(());
    }

    if bootstrap.init {
        info!(target: LOG_TARGET, "Default configuration created. Done.");
        return Ok(());
    }
    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap();

    let wallet = runtime.block_on(setup_wallet(&node_config, wallet_identity))?;
    debug!(target: LOG_TARGET, "Starting app");

    let node_identity = wallet.comms.node_identity().as_ref().clone();
    let wallet = Arc::new(RwLock::new(wallet));
    let grpc = crate::grpc::WalletGrpcServer::new(wallet.clone());

    if !bootstrap.daemon_mode {
        runtime.spawn(run_grpc(grpc, node_config.grpc_address));
        let app = App::<CrosstermBackend<Stdout>>::new(
            "Tari Console Wallet".into(),
            &node_identity,
            wallet.clone(),
            node_config.network,
        );
        runtime.handle().enter(|| ui::run(app))?;
        println!("The wallet is shutting down.");
        info!(
            target: LOG_TARGET,
            "Termination signal received from user. Shutting wallet down."
        );
        // TODO: Shutdown wallet
        Ok(())
    } else {
        println!("Starting grpc server");
        runtime
            .block_on(run_grpc(grpc, node_config.grpc_address))
            .map_err(|err| ExitCodes::GrpcError(err))?;
        println!("Shutting down");
        Ok(())
    }
}

/// Setup the app environment and state for use by the UI
async fn setup_wallet(config: &GlobalConfig, node_identity: Arc<NodeIdentity>) -> Result<WalletSqlite, ExitCodes> {
    create_wallet_folder(
        &config
            .wallet_db_file
            .parent()
            .expect("wallet_db_file cannot be set to a root directory"),
    )
    .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet folder. {}", e)))?;
    create_peer_db_folder(&config.wallet_peer_db_path)
        .map_err(|e| ExitCodes::WalletError(format!("Error creating peer db folder. {}", e)))?;

    debug!(target: LOG_TARGET, "Running Wallet database migrations");
    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend) =
        initialize_sqlite_database_backends(config.wallet_db_file.clone(), None)
            .map_err(|e| ExitCodes::WalletError(format!("Error creating Wallet database backends. {}", e)))?;
    debug!(target: LOG_TARGET, "Databases Initialized");

    // TODO remove after next TestNet
    transaction_backend.migrate(node_identity.public_key().clone());

    let comms_config = CommsConfig {
        node_identity,
        user_agent: format!("tari/wallet/{}", env!("CARGO_PKG_VERSION")),
        transport_type: setup_wallet_transport_type(&config),
        datastore_path: config.wallet_peer_db_path.clone(),
        peer_database_name: "peers".to_string(),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        // TODO - make this configurable
        dht: DhtConfig {
            database_url: DbConnectionUrl::File(config.data_dir.join("dht-wallet.db")),
            auto_join: true,
            ..Default::default()
        },
        // TODO: This should be false unless testing locally - make this configurable
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
    };

    let network = match &config.network {
        Network::MainNet => NetworkType::MainNet,
        Network::Rincewind => NetworkType::Rincewind,
    };

    let factories = CryptoFactories::default();
    let mut wallet_config = WalletConfig::new(
        comms_config.clone(),
        factories,
        Some(TransactionServiceConfig {
            direct_send_timeout: comms_config.dht.discovery_request_timeout,
            ..Default::default()
        }),
        network,
    );
    wallet_config.buffer_size = std::cmp::max(BASE_NODE_BUFFER_MIN_SIZE, config.buffer_size_base_node);

    let mut wallet = Wallet::new(
        wallet_config,
        wallet_backend,
        transaction_backend.clone(),
        output_manager_backend,
        contacts_backend,
    )
    .await
    .map_err(|e| {
        if let WalletError::CommsInitializationError(ce) = e {
            ExitCodes::WalletError(format!("Error initializing Comms: {}", ce.to_friendly_string()))
        } else {
            ExitCodes::WalletError(format!("Error creating Wallet Container: {:?}", e))
        }
    })?;

    debug!(target: LOG_TARGET, "Setting peer seeds");

    // TODO update this to come from an explicit config field. This will be replaced by gRPC interface.
    if !config.peer_seeds.is_empty() {
        let seed_peers = parse_peer_seeds(&config.peer_seeds);
        wallet
            .set_base_node_peer(
                seed_peers[0].public_key.clone(),
                seed_peers[0]
                    .addresses
                    .first()
                    .expect("The seed peers should have an address")
                    .to_string(),
            )
            .await
            .map_err(|e| ExitCodes::WalletError(format!("Error setting wallet base node peer. {}", e)))?;
    }

    Ok(wallet)
}

async fn run_grpc(grpc: WalletGrpcServer, grpc_address: SocketAddr) -> Result<(), String> {
    info!(target: LOG_TARGET, "Starting GRPC on {}", grpc_address);

    Server::builder()
        .add_service(tari_app_grpc::tari_rpc::wallet_server::WalletServer::new(grpc))
        .serve(grpc_address)
        .await
        .map_err(|e| format!("GRPC server returned error:{}", e))?;
    info!(target: LOG_TARGET, "Stopping GRPC");
    Ok(())
}
