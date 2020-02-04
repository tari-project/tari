// Copyright 2019. The Tari Project
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

//! # Common logging and configuration utilities
//!
//! ## The global Tari configuration file
//!
//! A single configuration file (usually `~/.tari/config.toml` is used to manage settings for all Tari applications
//! and nodes running on a single system, whether it's a base node, validator node, or wallet.
//!
//! Setting of configuration parameters is applied using the following order of precedence:
//!
//! 1. Command-line argument
//! 2. Environment variable
//! 3. `config.toml` file value
//! 4. Configuration default
//!
//! The utilities exposed in this crate are opinionated, but flexible. In general, all data is stored in a `.tari`
//! folder under your home folder.
//!
//! ### Example - Loading and deserializing the global config file
//!
//! ```edition2018
//! # use tari_common::*;
//! let config = default_config();
//! let config = GlobalConfig::convert_from(config).unwrap();
//! assert_eq!(config.network, Network::MainNet);
//! assert_eq!(config.blocking_threads, 4);
//! ```

use clap::ArgMatches;
use std::path::{Path, PathBuf};

mod configuration;
#[macro_use]
mod logging;

pub mod dir_utils;
pub use configuration::{
    default_config,
    install_default_config_file,
    load_configuration,
    ConfigExtractor,
    ConfigurationError,
    DatabaseType,
    GlobalConfig,
    Network,
};
pub use logging::initialize_logging;
pub const DEFAULT_CONFIG: &str = "config.toml";
pub const DEFAULT_LOG_CONFIG: &str = "log4rs.yml";

/// A minimal parsed configuration object that's used to bootstrap the main Configuration.
pub struct ConfigBootstrap {
    pub config: PathBuf,
    /// The path to the log configuration file. It is set using the following precedence set:
    ///   1. from the command-line parameter,
    ///   2. from the `TARI_LOG_CONFIGURATION` environment variable,
    ///   3. from a default value, usually `~/.tari/log4rs.yml` (or OS equivalent).
    pub log_config: PathBuf,
}

impl Default for ConfigBootstrap {
    fn default() -> Self {
        ConfigBootstrap {
            config: dir_utils::default_path(DEFAULT_CONFIG),
            log_config: dir_utils::default_path(DEFAULT_LOG_CONFIG),
        }
    }
}

pub fn bootstrap_config_from_cli(matches: &ArgMatches) -> ConfigBootstrap {
    let config = matches
        .value_of("config")
        .map(PathBuf::from)
        .unwrap_or(dir_utils::default_path(DEFAULT_CONFIG));
    let log_config = matches.value_of("log_config").map(PathBuf::from);
    let log_config = logging::get_log_configuration_path(log_config);

    if !config.exists() && matches.is_present("init") {
        println!("Installing new config file at {}", config.to_str().unwrap_or("[??]"));
        install_configuration(&config, configuration::install_default_config_file);
    }

    if !log_config.exists() && matches.is_present("init") {
        println!(
            "Installing new logfile configuration at {}",
            log_config.to_str().unwrap_or("[??]")
        );
        install_configuration(&log_config, logging::install_default_logfile_config);
    }
    ConfigBootstrap { config, log_config }
}

pub fn install_configuration<F>(path: &Path, installer: F)
where F: Fn(&Path) -> Result<u64, std::io::Error> {
    if let Err(e) = installer(path) {
        println!(
            "We could not install a new configuration file in {}: {}",
            path.to_str().unwrap_or("?"),
            e.to_string()
        )
    }
}
