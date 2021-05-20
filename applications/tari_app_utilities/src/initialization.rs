use crate::utilities::ExitCodes;
use config::Config;
use structopt::StructOpt;
use tari_common::{configuration::bootstrap::ApplicationType, ConfigBootstrap, DatabaseType, GlobalConfig};

pub const LOG_TARGET: &str = "tari::application";

pub fn init_configuration(
    application_type: ApplicationType,
) -> Result<(ConfigBootstrap, GlobalConfig, Config), ExitCodes> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(application_type)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger
    bootstrap.initialize_logging()?;

    // Populate the configuration struct
    let mut global_config =
        GlobalConfig::convert_from(cfg.clone()).map_err(|err| ExitCodes::ConfigError(err.to_string()))?;
    check_file_paths(&mut global_config, &bootstrap);
    Ok((bootstrap, global_config, cfg))
}

fn check_file_paths(config: &mut GlobalConfig, bootstrap: &ConfigBootstrap) {
    let prepend = bootstrap.base_path.clone();
    if !config.data_dir.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.data_dir.clone());
        config.data_dir = path;
        if let DatabaseType::LMDB(_) = config.db_type {
            config.db_type = DatabaseType::LMDB(config.data_dir.join("db"));
        }
    }
    if !config.peer_db_path.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.peer_db_path.clone());
        config.peer_db_path = path;
    }
    if !config.base_node_identity_file.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.base_node_identity_file.clone());
        config.base_node_identity_file = path;
    }
    if !config.base_node_tor_identity_file.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.base_node_tor_identity_file.clone());
        config.base_node_tor_identity_file = path;
    }
    if !config.console_wallet_db_file.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.console_wallet_db_file.clone());
        config.console_wallet_db_file = path;
    }
    if !config.console_wallet_peer_db_path.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.console_wallet_peer_db_path.clone());
        config.console_wallet_peer_db_path = path;
    }
    if !config.console_wallet_identity_file.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.console_wallet_identity_file.clone());
        config.console_wallet_identity_file = path;
    }
    if !config.console_wallet_tor_identity_file.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.console_wallet_tor_identity_file.clone());
        config.console_wallet_tor_identity_file = path;
    }
    if !config.wallet_db_file.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.wallet_db_file.clone());
        config.wallet_db_file = path;
    }
    if !config.wallet_peer_db_path.is_absolute() {
        let mut path = prepend.clone();
        path.push(config.wallet_db_file.clone());
        config.wallet_db_file = path;
    }
    if let Some(file_path) = config.console_wallet_notify_file.clone() {
        if file_path.is_absolute() {
            let mut path = prepend;
            path.push(file_path);
            config.console_wallet_notify_file = Some(path);
        }
    }
}
