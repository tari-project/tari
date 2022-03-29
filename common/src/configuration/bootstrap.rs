use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use structopt::StructOpt;

use super::error::ConfigError;
use crate::{
    dir_utils,
    logging,
    DEFAULT_BASE_NODE_LOG_CONFIG,
    DEFAULT_COLLECTIBLES_LOG_CONFIG,
    DEFAULT_CONFIG,
    DEFAULT_MERGE_MINING_PROXY_LOG_CONFIG,
    DEFAULT_MINING_NODE_LOG_CONFIG,
    DEFAULT_STRATUM_TRANSCODER_LOG_CONFIG,
    DEFAULT_WALLET_LOG_CONFIG,
};

pub fn prompt(question: &str) -> bool {
    println!("{}", question);
    let mut input = "".to_string();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();
    input == "y" || input.is_empty()
}

pub fn install_configuration<F>(application_type: ApplicationType, path: &Path, installer: F)
where F: Fn(ApplicationType, &Path) -> Result<(), std::io::Error> {
    if let Err(e) = installer(application_type, path) {
        println!(
            "Failed to install a new configuration file in {}: {}",
            path.to_str().unwrap_or("?"),
            e
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
    ValidatorNode,
    Collectibles,
}

impl ApplicationType {
    pub const fn as_str(&self) -> &'static str {
        use ApplicationType::*;
        match self {
            BaseNode => "Tari Base Node",
            ConsoleWallet => "Tari Console Wallet",
            MergeMiningProxy => "Tari Merge Mining Proxy",
            MiningNode => "Tari Mining Node",
            ValidatorNode => "Digital Assets Network Validator Node",
            StratumTranscoder => "Tari Stratum Transcoder",
            Collectibles => "Tari Collectibles",
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
            ValidatorNode => "validator-node",
            Collectibles => "collectibles",
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
            "validator-node" => Ok(ValidatorNode),
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
    use std::path::PathBuf;

    use structopt::StructOpt;
    use tempfile::tempdir;

    use crate::{
        configuration::bootstrap::ApplicationType,
        dir_utils,
        dir_utils::default_subdir,
        load_configuration,
        ConfigBootstrap,
        DEFAULT_BASE_NODE_LOG_CONFIG,
        DEFAULT_CONFIG,
    };

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
            ConfigBootstrap::from_iter_safe(vec!["", "--base_dir", data_path.as_str(), "--init", "--create-id"])
                .expect("failed to process arguments");

        // Initialize bootstrap dirs
        bootstrap
            .init_dirs(ApplicationType::BaseNode)
            .expect("failed to initialize dirs");
        let config_exists = std::path::Path::new(&bootstrap.config).exists();
        let log_config_exists = std::path::Path::new(&bootstrap.log_config).exists();
        // Load and apply configuration file
        let cfg = load_configuration(&bootstrap);

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
