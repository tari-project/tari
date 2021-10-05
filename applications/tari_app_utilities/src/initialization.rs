use crate::{consts, utilities::ExitCodes};
use config::Config;
use std::{path::PathBuf, str::FromStr};
use structopt::StructOpt;
use tari_common::{
    configuration::{bootstrap::ApplicationType, Network},
    ConfigBootstrap,
    DatabaseType,
    GlobalConfig,
};

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

    log::info!(target: LOG_TARGET, "{} ({})", application_type, consts::APP_VERSION);

    // Populate the configuration struct
    let mut global_config = GlobalConfig::convert_from(application_type, cfg.clone())
        .map_err(|err| ExitCodes::ConfigError(err.to_string()))?;
    check_file_paths(&mut global_config, &bootstrap);

    if let Some(str) = bootstrap.network.clone() {
        log::info!(target: LOG_TARGET, "Network selection requested");
        let network = Network::from_str(&str);
        match network {
            Ok(network) => {
                log::info!(
                    target: LOG_TARGET,
                    "Network selection successful, current network is: {}",
                    network
                );
                global_config.network = network;
            },
            Err(_) => {
                log::warn!(
                    target: LOG_TARGET,
                    "Network selection was invalid, continuing with default network."
                );
            },
        }
    }

    Ok((bootstrap, global_config, cfg))
}

fn check_file_paths(config: &mut GlobalConfig, bootstrap: &ConfigBootstrap) {
    let prepend = bootstrap.base_path.clone();
    if !config.data_dir.is_absolute() {
        config.data_dir = concatenate_paths_normalized(prepend.clone(), config.data_dir.clone());
        if let DatabaseType::LMDB(_) = config.db_type {
            config.db_type = DatabaseType::LMDB(config.data_dir.join("db"));
        }
    }
    if !config.peer_db_path.is_absolute() {
        config.peer_db_path = concatenate_paths_normalized(prepend.clone(), config.peer_db_path.clone());
    }
    if !config.base_node_identity_file.is_absolute() {
        config.base_node_identity_file =
            concatenate_paths_normalized(prepend.clone(), config.base_node_identity_file.clone());
    }
    if !config.base_node_tor_identity_file.is_absolute() {
        config.base_node_tor_identity_file =
            concatenate_paths_normalized(prepend.clone(), config.base_node_tor_identity_file.clone());
    }
    if !config.console_wallet_db_file.is_absolute() {
        config.console_wallet_db_file =
            concatenate_paths_normalized(prepend.clone(), config.console_wallet_db_file.clone());
    }
    if !config.console_wallet_peer_db_path.is_absolute() {
        config.console_wallet_peer_db_path =
            concatenate_paths_normalized(prepend.clone(), config.console_wallet_peer_db_path.clone());
    }

    if !config.wallet_db_file.is_absolute() {
        config.wallet_db_file = concatenate_paths_normalized(prepend.clone(), config.wallet_db_file.clone());
    }
    if !config.wallet_peer_db_path.is_absolute() {
        config.wallet_peer_db_path = concatenate_paths_normalized(prepend.clone(), config.wallet_peer_db_path.clone());
    }
    if let Some(file_path) = config.console_wallet_notify_file.clone() {
        if file_path.is_absolute() {
            config.console_wallet_notify_file = Some(concatenate_paths_normalized(prepend, file_path));
        }
    }
}

fn concatenate_paths_normalized(prepend: PathBuf, extension_path: PathBuf) -> PathBuf {
    let mut result = prepend;
    for component in extension_path.components() {
        result.push(component);
    }
    result
}
