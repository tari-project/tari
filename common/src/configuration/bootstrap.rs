//! # Building tari-based applications CLI
//!
//! To help with building tari-enabled CLI from scratch as easy as possible this crate exposes
//! [`ConfigBootstrap`] struct. ConfigBootstrap implements [`structopt::StructOpt`] trait, all CLI options
//! required for initializing configs can be embedded in any StructOpt derived struct.
//!
//! After loading ConfigBootstrap parameters it is necessary to call [`ConfigBootstrap::init_dirs()`] call
//! which would create necessary configuration files based on input parameters. This usually followed by:
//! - [`ConfigBootstrap::initialize_logging()`] would initialize log4rs logging.
//! - [`ConfigBootstrap::load_configuration()`] which would load [config::Config] from .tari config file.
//!
//! ## Example - CLI which is loading and deserializing the global config file
//!
//! ```ignore
//! # use tempdir::TempDir;
//! use tari_common::ConfigBootstrap;
//! use structopt::StructOpt;
//!
//! #[derive(StructOpt)]
//! /// The reference Tari cryptocurrency base node implementation
//! struct Arguments {
//!     /// Create and save new node identity if one doesn't exist
//!     #[structopt(long)]
//!     id: bool,
//!     #[structopt(flatten)]
//!     bootstrap: ConfigBootstrap,
//! }
//!
//! let mut args = Arguments::from_args();
//! # let temp_dir = TempDir::new(string(8).as_str()).unwrap();
//! # args.bootstrap.base_path = temp_dir.path().to_path_buf();
//! # args.bootstrap.init = true;
//! args.bootstrap.init_dirs();
//! args.bootstrap.initialize_logging();
//! let config = args.bootstrap.load_configuration();
//! assert_eq!(config.network, Network::MainNet);
//! assert_eq!(config.blocking_threads, 4);
//! # std::fs::remove_dir_all(&dir_utils::default_subdir("", Some(dir))).unwrap();
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
//!         --create     Create and save new node identity if one doesn't exist
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
use crate::{dir_utils, initialize_logging, logging, DEFAULT_CONFIG, DEFAULT_LOG_CONFIG};
use std::{
    io,
    path::{Path, PathBuf},
};
use structopt::{clap::ArgMatches, StructOpt};

#[derive(StructOpt, Debug)]
pub struct ConfigBootstrap {
    /// A path to a directory to store your files
    #[structopt(short, long, alias("base_dir"), hide_default_value(true), default_value = "")]
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
}

impl Default for ConfigBootstrap {
    fn default() -> Self {
        ConfigBootstrap {
            base_path: dir_utils::default_path("", None),
            config: dir_utils::default_path(DEFAULT_CONFIG, None),
            log_config: dir_utils::default_path(DEFAULT_LOG_CONFIG, None),
            init: false,
        }
    }
}

impl ConfigBootstrap {
    const ARGS: &'static [&'static str] = &["base-path", "base_dir", "config", "init", "log-config", "log_config"];

    /// Initialize configuration and directories based on ConfigBootstrap options.
    ///
    /// If not present it will create base directory (default ~/.tari/, depending on OS).
    /// Log and tari configs will be initialized in the base directory too.
    ///
    /// Without `--init` flag provided configuration and directories will be created only
    /// after user's confirmation.
    pub fn init_dirs(&mut self) -> Result<(), ConfigError> {
        if self.base_path.to_str() == Some("") {
            self.base_path = dir_utils::default_path("", None);
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

        let log_config = if self.log_config.to_str() == Some("") {
            None
        } else {
            Some(self.log_config.clone())
        };
        self.log_config = logging::get_log_configuration_path(log_config);

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
                install_configuration(&self.log_config, logging::install_default_logfile_config);
            }
        };
        Ok(())
    }

    /// Fill in ConfigBootstrap from clap ArgMatches.
    ///
    /// ## Example:
    /// ```edition2018
    /// # use structopt::clap::clap_app;
    /// # use tari_common::*;
    /// let matches = clap_app!(myapp =>
    ///     (@arg base_path: -b --("base-path") +takes_value "A path to a directory to store your files")
    ///     (@arg config: -c --config +takes_value "A path to the configuration file to use (config.toml)")
    ///     (@arg log_config: -l --("log-config") +takes_value "A path to the logfile configuration (log4rs.yml))")
    ///     (@arg init: -i --init "Create a default configuration file if it doesn't exist")
    /// ).get_matches();
    /// let bootstrap = ConfigBootstrap::from_matches(&matches);
    /// ```
    pub fn from_matches(matches: &ArgMatches) -> Result<Self, ConfigError> {
        let iter = matches
            .args
            .keys()
            .flat_map(|arg| match Self::ARGS.binary_search(arg) {
                Ok(_) => vec![
                    Some(std::ffi::OsString::from(format!("--{}", arg))),
                    matches.value_of_os(arg).map(|s| s.to_os_string()),
                ],
                _ => vec![],
            })
            .filter_map(|arg| arg);

        let mut vals: Vec<std::ffi::OsString> = iter.collect();
        vals.insert(0, "".into());
        Ok(ConfigBootstrap::from_iter_safe(vals.iter())?)
    }

    /// Set up application-level logging using the Log4rs configuration file
    /// based on supplied CLI arguments
    pub fn initialize_logging(&self) -> Result<(), ConfigError> {
        match initialize_logging(&self.log_config) {
            true => Ok(()),
            false => Err(ConfigError::new("failed to initalize logging", None)),
        }
    }

    /// Load configuration from files located based on supplied CLI arguments
    pub fn load_configuration(&self) -> Result<config::Config, ConfigError> {
        load_configuration(self).map_err(|source| ConfigError::new("failed to load configuration", Some(source)))
    }
}

/// Fill in ConfigBootstrap from clap ArgMatches
///
/// ```rust
/// # use structopt::clap::clap_app;
/// # use tari_common::*;
/// let matches = clap_app!(myapp =>
///     (version: "0.0.10")
///     (author: "The Tari Community")
///     (about: "The reference Tari cryptocurrency base node implementation")
///     (@arg base_path: -b --("base-path") +takes_value "A path to a directory to store your files")
///     (@arg config: -c --config +takes_value "A path to the configuration file to use (config.toml)")
///     (@arg log_config: -l --("log-config") +takes_value "A path to the logfile configuration (log4rs.yml))")
///     (@arg init: -i --init "Create a default configuration file if it doesn't exist")
///     (@arg create_id: --("create-id") "Create and save new node identity if one doesn't exist ")
/// ).get_matches();
/// let bootstrap = bootstrap_config_from_cli(&matches);
/// ```
/// ## Caveats
/// It will exit with code 1 if no base dir and fails to create one
pub fn bootstrap_config_from_cli(matches: &ArgMatches) -> ConfigBootstrap {
    let mut bootstrap = ConfigBootstrap::from_matches(matches).expect("failed to extract matches");
    match bootstrap.init_dirs() {
        Err(err) => {
            println!("{}", err);
            std::process::exit(1);
        },
        Ok(_) => bootstrap,
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

#[cfg(test)]
mod test {
    use super::ConfigBootstrap;
    use crate::{bootstrap_config_from_cli, dir_utils, dir_utils::default_subdir, load_configuration};
    use structopt::{clap::clap_app, StructOpt};
    use tari_test_utils::random::string;
    use tempdir::TempDir;

    #[test]
    fn test_bootstrap_from_matches() {
        // Create command line test data
        let app = clap_app!(myapp =>
            (@arg base_dir: -b --base_dir +takes_value "A path to a directory to store your files")
            (@arg config: -c --config +takes_value "A path to the configuration file to use (config.toml)")
            (@arg log_config: -l--log_config +takes_value "A path to the logfile configuration (log4rs.yml))")
            (@arg init: --init "Create a default configuration file if it doesn't exist")
        );
        let matches = app.clone().get_matches_from(vec![
            "",
            "--log_config",
            "no-file-created",
            "--config",
            "no-file-created",
            "--base_dir",
            "no-dir-created",
            "--init",
        ]);
        let bootstrap = ConfigBootstrap::from_matches(&matches).expect("failed to extract matches");
        assert!(bootstrap.init);
        assert_eq!(bootstrap.base_path.to_str(), Some("no-dir-created"));
        assert_eq!(bootstrap.log_config.to_str(), Some("no-file-created"));
        assert_eq!(bootstrap.config.to_str(), Some("no-file-created"));

        // Check aliases too
        let app = clap_app!(myapp =>
            (@arg ("base-path"): -b --("base-path") +takes_value "A path to a directory to store your files")
            (@arg config: -c --config +takes_value "A path to the configuration file to use (config.toml)")
            (@arg ("log-config"): -l --("log-config") +takes_value "A path to the logfile configuration (log4rs.yml))")
            (@arg init: --init "Create a default configuration file if it doesn't exist")
        );
        let matches = app.get_matches_from(vec![
            "",
            "--log-config",
            "no-file-created",
            "--base-path",
            "no-dir-created",
        ]);
        let bootstrap = ConfigBootstrap::from_matches(&matches).expect("failed to extract matches");
        assert!(!bootstrap.init);
        assert_eq!(bootstrap.base_path.to_str(), Some("no-dir-created"));
        assert_eq!(bootstrap.log_config.to_str(), Some("no-file-created"));
        assert_eq!(bootstrap.config.to_str(), Some(""));
    }

    #[test]
    fn test_bootstrap_config_from_cli_and_load_configuration() {
        let temp_dir = TempDir::new(string(8).as_str()).unwrap();
        let dir = &temp_dir.path().to_path_buf();
        // Create test folder
        dir_utils::create_data_directory(Some(dir)).unwrap();

        // Create command line test data
        let matches = clap_app!(myapp =>
            (version: "0.0.10")
            (author: "The Tari Community")
            (about: "The reference Tari cryptocurrency base node implementation")
            (@arg base_dir: -b --base_dir +takes_value "A path to a directory to store your files")
            (@arg config: -c --config +takes_value "A path to the configuration file to use (config.toml)")
            (@arg log_config: -l --log_config +takes_value "A path to the logfile configuration (log4rs.yml))")
            (@arg init: --init "Create a default configuration file if it doesn't exist")
            (@arg create_id: --create_id "Create and save new node identity if one doesn't exist ")
        )
        .get_matches_from(vec![
            "",
            "--base_dir",
            default_subdir("", Some(dir)).as_str(),
            "--init",
            "--create_id",
        ]);

        let bootstrap = ConfigBootstrap::from_matches(&matches).expect("failed to extract matches");
        assert!(bootstrap.init);
        assert_eq!(&bootstrap.base_path, dir);

        // Load bootstrap via former API
        let bootstrap = bootstrap_config_from_cli(&matches);
        let config_exists = std::path::Path::new(&bootstrap.config).exists();
        let log_config_exists = std::path::Path::new(&bootstrap.log_config).exists();
        // Load and apply configuration file
        let cfg = load_configuration(&bootstrap);

        // Cleanup test data
        if std::path::Path::new(&dir_utils::default_subdir("", Some(dir))).exists() {
            std::fs::remove_dir_all(&dir_utils::default_subdir("", Some(dir))).expect("failed to cleanup dirs");
        }

        // Assert results
        assert!(config_exists);
        assert!(log_config_exists);
        assert!(&cfg.is_ok());
    }

    #[test]
    fn test_bootstrap_config_from_structopt_derive() {
        let temp_dir = TempDir::new(string(8).as_str()).unwrap();
        let dir = &temp_dir.path().to_path_buf();
        // Create test folder
        dir_utils::create_data_directory(Some(dir)).unwrap();

        #[derive(StructOpt)]
        /// The reference Tari cryptocurrency base node implementation
        struct Arguments {
            /// Create and save new node identity if one doesn't exist
            #[structopt(long = "create_id")]
            create_id: bool,
            #[structopt(flatten)]
            bootstrap: super::ConfigBootstrap,
        }

        // Create command line test data
        let mut args = Arguments::from_iter_safe(vec![
            "",
            "--base_dir",
            default_subdir("", Some(dir)).as_str(),
            "--init",
            "--create_id",
        ])
        .expect("failed to process arguments");
        // Init bootstrap dirs
        args.bootstrap.init_dirs().expect("failed to initialize dirs");
        // Load and apply configuration file
        let cfg = load_configuration(&args.bootstrap);

        // Cleanup test data
        if std::path::Path::new(&dir_utils::default_subdir("", Some(dir))).exists() {
            std::fs::remove_dir_all(&dir_utils::default_subdir("", Some(dir))).unwrap();
        }

        // Assert results
        assert!(args.bootstrap.init);
        assert!(args.create_id);
        assert!(&cfg.is_ok());
    }

    #[test]
    fn check_homedir_is_used_by_default() {
        dir_utils::create_data_directory(None).unwrap();
        assert_eq!(
            dirs::home_dir().unwrap().join(".tari"),
            dir_utils::default_path("", None)
        );
    }
}
