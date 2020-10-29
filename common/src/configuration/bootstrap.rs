//! # Building tari-based applications CLI
//!
//! To help with building tari-enabled CLI from scratch as easy as possible this crate exposes
//! [`ConfigBootstrap`] struct. ConfigBootstrap implements [`structopt::StructOpt`] trait, all CLI options
//! required for initializing configs can be embedded in any StructOpt derived struct.
//!
//! After loading ConfigBootstrap parameters it is necessary to call [`ConfigBootstrap::init_dirs()`]
//! which would create necessary configuration files based on input parameters. This usually followed by:
//! - [`ConfigBootstrap::initialize_logging()`] would initialize log4rs logging.
//! - [`ConfigBootstrap::load_configuration()`] which would load [config::Config] from .tari config file.
//!
//! ## Example - CLI which is loading and deserializing the global config file
//!
//! ```ignore
//! use tari_common::ConfigBootstrap;
//!
//! // Parse and validate command-line arguments
//! let mut bootstrap = ConfigBootstrap::from_args();
//! // Check and initialize configuration files
//! bootstrap.init_dirs()?;
//! // Load and apply configuration file
//! let config = bootstrap.load_configuration()?;
//! // Initialise the logger
//! bootstrap.initialize_logging()?;
//! assert_eq!(config.network, Network::MainNet);
//! assert_eq!(config.core_threads, Some(4));
//! ```
//!
//! ```shell
//! > main -h
//! main 0.0.0
//! The reference Tari cryptocurrency base node implementation
//!
//! USAGE:
//!     main [FLAGS] [OPTIONS]
//!
//! FLAGS:
//!     -h, --help       Prints help information
//!         --create-id  Create and save new node identity if one doesn't exist
//!         --init       Create a default configuration file if it doesn't exist
//!     -V, --version    Prints version information
//!
//! OPTIONS:
//!         --base-path <base-path>      A path to a directory to store your files
//!         --config <config>            A path to the configuration file to use (config.toml)
//!         --log-config <log-config>    The path to the log configuration file. It is set using the following precedence
//!                                      set: [env: TARI_LOG_CONFIGURATION=]
//! ```

use super::{
    error::ConfigError,
    utils::{install_default_config_file, load_configuration},
};
use crate::{
    dir_utils,
    initialize_logging,
    logging,
    DEFAULT_CONFIG,
    DEFAULT_LOG_CONFIG,
    DEFAULT_MERGE_MINING_PROXY_LOG_CONFIG,
    DEFAULT_WALLET_LOG_CONFIG,
};
use std::{
    io,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct ConfigBootstrap {
    /// A path to a directory to store your files
    #[structopt(
        short,
        long,
        alias("base_path"),
        alias("base_dir"),
        alias("base-dir"),
        hide_default_value(true),
        default_value = ""
    )]
    pub base_path: PathBuf,
    /// A path to the configuration file to use (config.toml)
    #[structopt(short, long, hide_default_value(true), default_value = "")]
    pub config: PathBuf,
    /// The path to the log configuration file. It is set using the following precedence set
    #[structopt(
        short,
        long,
        alias("log_config"),
        env = "TARI_LOG_CONFIGURATION",
        hide_default_value(true),
        default_value = ""
    )]
    pub log_config: PathBuf,
    /// Create a default configuration file if it doesn't exist
    #[structopt(long)]
    pub init: bool,
    /// Create and save new node identity if one doesn't exist
    #[structopt(long, alias("create_id"))]
    pub create_id: bool,
    /// Run in daemon mode, with no interface
    #[structopt(short, long, alias("daemon"))]
    pub daemon_mode: bool,
    /// This will rebuild the db, adding block for block in
    #[structopt(long, alias("rebuild_db"))]
    pub rebuild_db: bool,
    /// Path to input file of commands
    #[structopt(short, long, alias("input"), parse(from_os_str))]
    pub input_file: Option<PathBuf>,
    /// Single input command
    #[structopt(long)]
    pub command: Option<String>,
    /// This will clean out the orphans db at startup
    #[structopt(long, alias("clean_orphans_db"))]
    pub clean_orphans_db: bool,
}

impl Default for ConfigBootstrap {
    fn default() -> Self {
        ConfigBootstrap {
            base_path: dir_utils::default_path("", None),
            config: dir_utils::default_path(DEFAULT_CONFIG, None),
            log_config: dir_utils::default_path(DEFAULT_LOG_CONFIG, None),
            init: false,
            create_id: false,
            daemon_mode: false,
            rebuild_db: false,
            input_file: None,
            command: None,
            clean_orphans_db: false,
        }
    }
}

impl ConfigBootstrap {
    /// Initialize configuration and directories based on ConfigBootstrap options.
    ///
    /// If not present it will create base directory (default ~/.tari/, depending on OS).
    /// Log and tari configs will be initialized in the base directory too.
    ///
    /// Without `--init` flag provided configuration and directories will be created only
    /// after user's confirmation.
    pub fn init_dirs(&mut self, application_type: ApplicationType) -> Result<(), ConfigError> {
        if self.base_path.to_str() == Some("") {
            self.base_path = dir_utils::default_path("", None);
        } else {
            self.base_path = dir_utils::absolute_path(&self.base_path);
        }

        // Create the tari data directory
        dir_utils::create_data_directory(Some(&self.base_path)).map_err(|err| {
            ConfigError::new(
                "We couldn't create a default Tari data directory and have to quit now. This makes us sad :(",
                Some(err.to_string()),
            )
        })?;

        if self.config.to_str() == Some("") {
            self.config = dir_utils::default_path(DEFAULT_CONFIG, Some(&self.base_path));
        }

        if self.log_config.to_str() == Some("") {
            match application_type {
                ApplicationType::BaseNode => {
                    self.log_config = dir_utils::default_path(DEFAULT_LOG_CONFIG, Some(&self.base_path));
                },
                ApplicationType::ConsoleWallet => {
                    self.log_config = dir_utils::default_path(DEFAULT_WALLET_LOG_CONFIG, Some(&self.base_path));
                },
                ApplicationType::MergeMiningProxy => {
                    self.log_config =
                        dir_utils::default_path(DEFAULT_MERGE_MINING_PROXY_LOG_CONFIG, Some(&self.base_path))
                },
            }
        }

        if !self.config.exists() {
            let install = if !self.init {
                prompt("Config file does not exist. We can create a default one for you now, or you can say 'no' here, \
                and generate a customised one at https://config.tari.com.\n\
                Would you like to try the default configuration (Y/n)?")
            } else {
                true
            };

            if install {
                println!(
                    "Installing new config file at {}",
                    self.config.to_str().unwrap_or("[??]")
                );
                install_configuration(&self.config, install_default_config_file);
            }
        }

        if !self.log_config.exists() {
            let install = if !self.init {
                prompt("Logging configuration file does not exist. Would you like to create a new one (Y/n)?")
            } else {
                true
            };
            if install {
                println!(
                    "Installing new logfile configuration at {}",
                    self.log_config.to_str().unwrap_or("[??]")
                );
                match application_type {
                    ApplicationType::BaseNode => {
                        install_configuration(&self.log_config, logging::install_default_logfile_config)
                    },
                    ApplicationType::ConsoleWallet => {
                        install_configuration(&self.log_config, logging::install_default_wallet_logfile_config)
                    },
                    ApplicationType::MergeMiningProxy => install_configuration(
                        &self.log_config,
                        logging::install_default_merge_mining_proxy_logfile_config,
                    ),
                }
            }
        };
        Ok(())
    }

    /// Set up application-level logging using the Log4rs configuration file
    /// based on supplied CLI arguments
    pub fn initialize_logging(&self) -> Result<(), ConfigError> {
        if initialize_logging(&self.log_config) {
            Ok(())
        } else {
            Err(ConfigError::new("Failed to initialize logging", None))
        }
    }

    /// Load configuration from files located based on supplied CLI arguments
    pub fn load_configuration(&self) -> Result<config::Config, ConfigError> {
        load_configuration(self).map_err(|source| ConfigError::new("failed to load configuration", Some(source)))
    }
}

fn prompt(question: &str) -> bool {
    println!("{}", question);
    let mut input = "".to_string();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();
    input == "y" || input.is_empty()
}

pub fn install_configuration<F>(path: &Path, installer: F)
where F: Fn(&Path) -> Result<(), std::io::Error> {
    if let Err(e) = installer(path) {
        println!(
            "We could not install a new configuration file in {}: {}",
            path.to_str().unwrap_or("?"),
            e.to_string()
        )
    }
}

pub enum ApplicationType {
    BaseNode,
    ConsoleWallet,
    MergeMiningProxy,
}

#[cfg(test)]
mod test {
    use crate::{
        configuration::bootstrap::ApplicationType,
        dir_utils,
        dir_utils::default_subdir,
        load_configuration,
        ConfigBootstrap,
        DEFAULT_CONFIG,
        DEFAULT_LOG_CONFIG,
    };
    use std::path::PathBuf;
    use structopt::StructOpt;
    use tempfile::tempdir;

    #[test]
    fn test_bootstrap_and_load_configuration() {
        // Test command line arguments
        let bootstrap = ConfigBootstrap::from_iter_safe(vec![
            "",
            "--init",
            "--create-id",
            "--rebuild_db",
            "--clean_orphans_db",
            "--base-path",
            "no-temp-path-created",
            "--log-config",
            "no-log-config-file-created",
            "--config",
            "no-config-file-created",
            "--command",
            "no-command-provided",
        ])
        .expect("failed to process arguments");
        assert!(bootstrap.init);
        assert!(bootstrap.create_id);
        assert!(bootstrap.rebuild_db);
        assert!(bootstrap.clean_orphans_db);
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created"));
        assert_eq!(bootstrap.log_config.to_str(), Some("no-log-config-file-created"));
        assert_eq!(bootstrap.config.to_str(), Some("no-config-file-created"));
        assert_eq!(bootstrap.command.unwrap(), "no-command-provided");

        // Test command line argument aliases
        let bootstrap = ConfigBootstrap::from_iter_safe(vec![
            "",
            "--base_path",
            "no-temp-path-created",
            "--log_config",
            "no-log-config-file-created",
        ])
        .expect("failed to process arguments");
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created"));
        assert_eq!(bootstrap.log_config.to_str(), Some("no-log-config-file-created"));
        let bootstrap = ConfigBootstrap::from_iter_safe(vec!["", "--base-dir", "no-temp-path-created"])
            .expect("failed to process arguments");
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created"));
        let bootstrap = ConfigBootstrap::from_iter_safe(vec!["", "--base_dir", "no-temp-path-created"])
            .expect("failed to process arguments");
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created"));

        // Check if log configuration file environment variable is recognized in the bootstrap
        // Note: This cannot be tested in parallel with any other `ConfigBootstrap::from_iter_safe` command
        std::env::set_var("TARI_LOG_CONFIGURATION", "~/fake-example");
        let bootstrap = ConfigBootstrap::from_iter_safe(vec![""]).expect("failed to process arguments");
        std::env::remove_var("TARI_LOG_CONFIGURATION");
        assert_eq!(bootstrap.log_config.to_str(), Some("~/fake-example"));
        assert_ne!(bootstrap.config.to_str(), Some("~/fake-example"));

        // Check if home_dir is used by default
        assert_eq!(
            dirs_next::home_dir().unwrap().join(".tari"),
            dir_utils::default_path("", None)
        );

        // Command line test data for config init test
        let temp_dir = tempdir().unwrap();
        let dir = &PathBuf::from(temp_dir.path().to_path_buf().display().to_string().to_owned() + "/01/02/");
        let data_path = default_subdir("", Some(dir));
        let mut bootstrap =
            ConfigBootstrap::from_iter_safe(vec!["", "--base_dir", &data_path.as_str(), "--init", "--create-id"])
                .expect("failed to process arguments");

        // Initialize bootstrap dirs
        bootstrap
            .init_dirs(ApplicationType::BaseNode)
            .expect("failed to initialize dirs");
        let config_exists = std::path::Path::new(&bootstrap.config).exists();
        let log_config_exists = std::path::Path::new(&bootstrap.log_config).exists();
        // Load and apply configuration file
        let cfg = load_configuration(&bootstrap);

        // Change current dir to test dir so logging can be initialized there and test data can be cleaned up
        let current_dir = std::env::current_dir().unwrap_or_default();
        if std::env::set_current_dir(&dir).is_err() {
            println!(
                "Logging initialized in {}, could not initialize in {}.",
                &current_dir.display(),
                &dir.display()
            );
        };

        // Initialize logging
        let logging_initialized = bootstrap.initialize_logging().is_ok();
        let log_network_file_exists = std::path::Path::new(&bootstrap.base_path)
            .join("log/network.log")
            .exists();
        let log_base_layer_file_exists = std::path::Path::new(&bootstrap.base_path)
            .join("log/base_layer.log")
            .exists();
        let log_other_file_exists = std::path::Path::new(&bootstrap.base_path)
            .join("log/other.log")
            .exists();

        // Change back to current dir
        if std::env::set_current_dir(&current_dir).is_err() {
            println!(
                "Working directory could not be changed back to {} after logging has been initialized. New working \
                 directory is {}",
                &current_dir.display(),
                &std::env::current_dir().unwrap_or_default().display()
            );
        };

        // Cleanup test data
        if std::path::Path::new(&data_path.as_str()).exists() {
            std::fs::remove_dir_all(&data_path.as_str()).unwrap();
        }

        // Assert bootstrap results
        assert_eq!(bootstrap.base_path, PathBuf::from(&data_path));
        assert!(bootstrap.init);
        assert!(bootstrap.create_id);
        assert!(&cfg.is_ok());
        assert!(config_exists);
        assert_eq!(
            &bootstrap.config,
            &PathBuf::from(data_path.to_owned() + &DEFAULT_CONFIG.to_string())
        );
        assert!(log_config_exists);
        assert_eq!(
            &bootstrap.log_config,
            &PathBuf::from(data_path.to_owned() + &DEFAULT_LOG_CONFIG.to_string())
        );

        // Assert logging results
        assert!(logging_initialized);
        assert!(log_network_file_exists);
        assert!(log_base_layer_file_exists);
        assert!(log_other_file_exists);
    }
}
