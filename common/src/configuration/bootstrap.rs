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
    DEFAULT_BASE_NODE_LOG_CONFIG,
    DEFAULT_CONFIG,
    DEFAULT_MERGE_MINING_PROXY_LOG_CONFIG,
    DEFAULT_MINING_NODE_LOG_CONFIG,
    DEFAULT_STRATUM_TRANSCODER_LOG_CONFIG,
    DEFAULT_WALLET_LOG_CONFIG,
};
use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    path::{Path, PathBuf},
    str::FromStr,
};
use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
pub struct ConfigBootstrap {
    /// A path to a directory to store your files
    #[structopt(
        short,
        long,
        aliases = &["base_path", "base_dir", "base-dir"],
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
        alias = "log_config",
        env = "TARI_LOG_CONFIGURATION",
        hide_default_value(true),
        default_value = ""
    )]
    pub log_config: PathBuf,
    /// Create a default configuration file if it doesn't exist
    #[structopt(long)]
    pub init: bool,
    /// Create and save new node identity if one doesn't exist
    #[structopt(long, alias = "create_id")]
    pub create_id: bool,
    /// Run in non-interactive mode, with no UI.
    #[structopt(short, long, alias = "non-interactive")]
    pub non_interactive_mode: bool,
    /// This will rebuild the db, adding block for block in
    #[structopt(long, alias = "rebuild_db")]
    pub rebuild_db: bool,
    /// Path to input file of commands
    #[structopt(short, long, aliases = &["input", "script"], parse(from_os_str))]
    pub input_file: Option<PathBuf>,
    /// Single input command
    #[structopt(long)]
    pub command: Option<String>,
    /// This will clean out the orphans db at startup
    #[structopt(long, alias = "clean_orphans_db")]
    pub clean_orphans_db: bool,
    /// Supply the password for the console wallet
    #[structopt(long)]
    pub password: Option<String>,
    /// Change the password for the console wallet
    #[structopt(long, alias = "update-password")]
    pub change_password: bool,
    /// Force wallet recovery
    #[structopt(long, alias = "recover")]
    pub recovery: bool,
    /// Supply the optional wallet seed words for recovery on the command line
    #[structopt(long, alias = "seed_words")]
    pub seed_words: Option<String>,
    /// Supply the optional file name to save the wallet seed words into
    #[structopt(long, aliases = &["seed_words_file_name", "seed-words-file"], parse(from_os_str))]
    pub seed_words_file_name: Option<PathBuf>,
    /// Wallet notify script
    #[structopt(long, alias = "notify")]
    pub wallet_notify: Option<PathBuf>,
    /// Automatically exit wallet command/script mode when done
    #[structopt(long, alias = "auto-exit")]
    pub command_mode_auto_exit: bool,
    /// Mining node options
    #[structopt(long, alias = "mine-until-height")]
    pub mine_until_height: Option<u64>,
    #[structopt(long, alias = "max-blocks")]
    pub miner_max_blocks: Option<u64>,
    #[structopt(long, alias = "min-difficulty")]
    pub miner_min_diff: Option<u64>,
    #[structopt(long, alias = "max-difficulty")]
    pub miner_max_diff: Option<u64>,
    #[structopt(long, alias = "tracing")]
    pub tracing_enabled: bool,
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        result.push(component);
    }
    result
}

impl Default for ConfigBootstrap {
    fn default() -> Self {
        ConfigBootstrap {
            base_path: normalize_path(dir_utils::default_path("", None)),
            config: normalize_path(dir_utils::default_path(DEFAULT_CONFIG, None)),
            log_config: normalize_path(dir_utils::default_path(DEFAULT_BASE_NODE_LOG_CONFIG, None)),
            init: false,
            create_id: false,
            non_interactive_mode: false,
            rebuild_db: false,
            input_file: None,
            command: None,
            clean_orphans_db: false,
            password: None,
            change_password: false,
            recovery: false,
            seed_words: None,
            seed_words_file_name: None,
            wallet_notify: None,
            command_mode_auto_exit: false,
            mine_until_height: None,
            miner_max_blocks: None,
            miner_min_diff: None,
            miner_max_diff: None,
            tracing_enabled: false,
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
            self.config = normalize_path(dir_utils::default_path(DEFAULT_CONFIG, Some(&self.base_path)));
        }

        if self.log_config.to_str() == Some("") {
            match application_type {
                ApplicationType::BaseNode => {
                    self.log_config = normalize_path(dir_utils::default_path(
                        DEFAULT_BASE_NODE_LOG_CONFIG,
                        Some(&self.base_path),
                    ));
                },
                ApplicationType::ConsoleWallet => {
                    self.log_config = normalize_path(dir_utils::default_path(
                        DEFAULT_WALLET_LOG_CONFIG,
                        Some(&self.base_path),
                    ));
                },
                ApplicationType::MergeMiningProxy => {
                    self.log_config = normalize_path(dir_utils::default_path(
                        DEFAULT_MERGE_MINING_PROXY_LOG_CONFIG,
                        Some(&self.base_path),
                    ))
                },
                ApplicationType::StratumTranscoder => {
                    self.log_config = normalize_path(dir_utils::default_path(
                        DEFAULT_STRATUM_TRANSCODER_LOG_CONFIG,
                        Some(&self.base_path),
                    ))
                },
                ApplicationType::MiningNode => {
                    self.log_config = normalize_path(dir_utils::default_path(
                        DEFAULT_MINING_NODE_LOG_CONFIG,
                        Some(&self.base_path),
                    ))
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
                        install_configuration(&self.log_config, logging::install_default_base_node_logfile_config)
                    },
                    ApplicationType::ConsoleWallet => {
                        install_configuration(&self.log_config, logging::install_default_wallet_logfile_config)
                    },
                    ApplicationType::MergeMiningProxy => install_configuration(
                        &self.log_config,
                        logging::install_default_merge_mining_proxy_logfile_config,
                    ),
                    ApplicationType::StratumTranscoder => install_configuration(
                        &self.log_config,
                        logging::install_default_stratum_transcoder_logfile_config,
                    ),
                    ApplicationType::MiningNode => {
                        install_configuration(&self.log_config, logging::install_default_mining_node_logfile_config)
                    },
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
        load_configuration(self)
    }
}

pub fn prompt(question: &str) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationType {
    BaseNode,
    ConsoleWallet,
    MergeMiningProxy,
    MiningNode,
    StratumTranscoder,
}

impl ApplicationType {
    pub const fn as_str(&self) -> &'static str {
        use ApplicationType::*;
        match self {
            BaseNode => "Tari Base Node",
            ConsoleWallet => "Tari Console Wallet",
            MergeMiningProxy => "Tari Merge Mining Proxy",
            MiningNode => "Tari Mining Node",
            StratumTranscoder => "Tari Stratum Transcoder",
        }
    }

    pub const fn as_config_str(&self) -> &'static str {
        use ApplicationType::*;
        match self {
            BaseNode => "base_node",
            ConsoleWallet => "wallet",
            MergeMiningProxy => "merge_mining_proxy",
            MiningNode => "miner",
            StratumTranscoder => "stratum-transcoder",
        }
    }
}

impl FromStr for ApplicationType {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ApplicationType::*;
        match s {
            "base-node" | "base_node" => Ok(BaseNode),
            "console-wallet" | "console_wallet" => Ok(ConsoleWallet),
            "mm-proxy" | "mm_proxy" => Ok(MergeMiningProxy),
            "miner" => Ok(MiningNode),
            "stratum-proxy" => Ok(StratumTranscoder),
            _ => Err(ConfigError::new("Invalid ApplicationType", None)),
        }
    }
}

impl Display for ApplicationType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::{
        configuration::bootstrap::ApplicationType,
        dir_utils,
        dir_utils::default_subdir,
        load_configuration,
        ConfigBootstrap,
        DEFAULT_BASE_NODE_LOG_CONFIG,
        DEFAULT_CONFIG,
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
            "--seed-words-file-name",
            "no-seed-words-file-name-provided",
            "--seed-words",
            "purse soup tornado success arch expose submit",
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
        assert_eq!(
            bootstrap.seed_words_file_name.unwrap().to_str(),
            Some("no-seed-words-file-name-provided")
        );
        assert_eq!(
            bootstrap.seed_words.unwrap().as_str(),
            "purse soup tornado success arch expose submit"
        );

        // Test command line argument aliases
        let bootstrap = ConfigBootstrap::from_iter_safe(vec![
            "",
            "--base_path",
            "no-temp-path-created",
            "--log_config",
            "no-log-config-file-created",
            "--seed_words_file_name",
            "no-seed-words-file-name-provided",
            "--seed_words",
            "crunch zone nasty work zoo december three",
        ])
        .expect("failed to process arguments");
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created"));
        assert_eq!(bootstrap.log_config.to_str(), Some("no-log-config-file-created"));
        assert_eq!(
            bootstrap.seed_words_file_name.unwrap().to_str(),
            Some("no-seed-words-file-name-provided")
        );
        assert_eq!(
            bootstrap.seed_words.unwrap().as_str(),
            "crunch zone nasty work zoo december three"
        );
        let bootstrap = ConfigBootstrap::from_iter_safe(vec!["", "--base-dir", "no-temp-path-created"])
            .expect("failed to process arguments");
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created"));
        let bootstrap = ConfigBootstrap::from_iter_safe(vec!["", "--base_dir", "no-temp-path-created-again"])
            .expect("failed to process arguments");
        assert_eq!(bootstrap.base_path.to_str(), Some("no-temp-path-created-again"));

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
        let dir = &PathBuf::from(temp_dir.path().to_path_buf().display().to_string() + "/01/02/");
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
            .join("log/base-node/network.log")
            .exists();
        let log_base_layer_file_exists = std::path::Path::new(&bootstrap.base_path)
            .join("log/base-node/base_layer.log")
            .exists();
        let log_other_file_exists = std::path::Path::new(&bootstrap.base_path)
            .join("log/base-node/other.log")
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
            // windows CI had an error when this result was unwrapped
            // "The directory is not empty."
            match std::fs::remove_dir_all(&data_path.as_str()) {
                Ok(_) => {},
                Err(e) => println!("couldn't delete data path {}, error {}", &data_path, e),
            }
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
            &PathBuf::from(data_path + &DEFAULT_BASE_NODE_LOG_CONFIG.to_string())
        );

        // Assert logging results
        assert!(logging_initialized);
        assert!(log_network_file_exists);
        assert!(log_base_layer_file_exists);
        assert!(log_other_file_exists);
    }
}
