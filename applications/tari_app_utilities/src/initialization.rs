// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use config::Config;
use structopt::StructOpt;
use tari_common::{
    configuration::{bootstrap::ApplicationType, Network},
    exit_codes::ExitError,
    ConfigBootstrap,
    DatabaseType,
    GlobalConfig,
};

use crate::consts;

pub const LOG_TARGET: &str = "tari::application";

pub fn init_configuration(
    application_type: ApplicationType,
) -> Result<(ConfigBootstrap, GlobalConfig, Config), ExitError> {
    // Parse and validate command-line arguments
    let mut bootstrap = ConfigBootstrap::from_args();

    // Check and initialize configuration files
    bootstrap.init_dirs(application_type)?;

    // Load and apply configuration file
    let cfg = bootstrap.load_configuration()?;

    // Initialise the logger (Comment out to enable tokio tracing via tokio-console)
    bootstrap.initialize_logging()?;

    log::info!(target: LOG_TARGET, "{} ({})", application_type, consts::APP_VERSION);

    // Populate the configuration struct
    let mut global_config = GlobalConfig::convert_from(application_type, cfg.clone(), bootstrap.network.clone())?;

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
                global_config.data_dir = PathBuf::from(str);
                if let DatabaseType::LMDB(_) = global_config.db_type {
                    global_config.db_type = DatabaseType::LMDB(global_config.data_dir.join("db"));
                }
                global_config.comms_peer_db_path = global_config.data_dir.join("peer_db");
                global_config.wallet_peer_db_path = global_config.data_dir.join("wallet_peer_db");
                global_config.console_wallet_peer_db_path = global_config.data_dir.join("console_wallet_peer_db");
            },
            Err(e) => {
                log::error!(target: LOG_TARGET, "Network selection was invalid, exiting.");
                return Err(e.into());
            },
        }
    }

    if let Some(str) = bootstrap.custom_base_node.clone() {
        global_config.wallet_custom_base_node = Some(str);
    }

    check_file_paths(&mut global_config, &bootstrap);

    Ok((bootstrap, global_config, cfg))
}

fn check_file_paths(config: &mut GlobalConfig, bootstrap: &ConfigBootstrap) {
    let prepend = bootstrap.base_path.clone();
    if !config.data_dir.is_absolute() {
        config.data_dir = concatenate_paths_normalized(prepend.clone(), &config.data_dir.clone());
        if let DatabaseType::LMDB(_) = config.db_type {
            config.db_type = DatabaseType::LMDB(config.data_dir.join("db"));
        }
    }
    if !config.comms_peer_db_path.is_absolute() {
        config.comms_peer_db_path = concatenate_paths_normalized(prepend.clone(), &config.comms_peer_db_path.clone());
    }
    if !config.base_node_identity_file.is_absolute() {
        config.base_node_identity_file =
            concatenate_paths_normalized(prepend.clone(), &config.base_node_identity_file.clone());
    }
    if !config.base_node_tor_identity_file.is_absolute() {
        config.base_node_tor_identity_file =
            concatenate_paths_normalized(prepend.clone(), &config.base_node_tor_identity_file.clone());
    }
    if !config.console_wallet_db_file.is_absolute() {
        config.console_wallet_db_file =
            concatenate_paths_normalized(prepend.clone(), &config.console_wallet_db_file.clone());
    }
    if !config.console_wallet_peer_db_path.is_absolute() {
        config.console_wallet_peer_db_path =
            concatenate_paths_normalized(prepend.clone(), &config.console_wallet_peer_db_path.clone());
    }

    if !config.wallet_db_file.is_absolute() {
        config.wallet_db_file = concatenate_paths_normalized(prepend.clone(), &config.wallet_db_file.clone());
    }
    if !config.wallet_peer_db_path.is_absolute() {
        config.wallet_peer_db_path = concatenate_paths_normalized(prepend.clone(), &config.wallet_peer_db_path.clone());
    }
    if let Some(file_path) = config.console_wallet_notify_file.clone() {
        if file_path.is_absolute() {
            config.console_wallet_notify_file = Some(concatenate_paths_normalized(prepend, &file_path));
        }
    }
}

fn concatenate_paths_normalized(prepend: PathBuf, extension_path: &PathBuf) -> PathBuf {
    let mut result = prepend;
    for component in extension_path.components() {
        result.push(component);
    }
    result
}
