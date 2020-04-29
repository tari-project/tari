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
//! 3. `config.toml` file value (see details: [configuration])
//! 4. Configuration default
//!
//! The utilities exposed in this crate are opinionated, but flexible. In general, all data is stored in a `.tari`
//! folder under your home folder.
//!
//! ## Custom application configuration
//!
//! Tari configuration file allows adding custom application specific sections. Tari is using [config] crate
//! to load configurations and gives access to [`config::Config`] struct so that apps might be flexible.
//! Though as tari apps follow certain configurability assumptions, tari_common provides helper traits
//! which automate those with minimal code.
//!
//! ## CLI helpers
//!
//! Bootstrapping tari configuration files might be customized via CLI or env settings. To help with building
//! tari-enabled CLI from scratch as easy as possible this crate exposes [ConfigBootstrap] struct which
//! implements [structopt::StructOpt] trait and can be easily reused in any CLI.
//!
//! ## Example - CLI which is loading and deserializing the global config file
//!
//! ```edition2018
//! # use tari_common::*;
//! # use tari_test_utils::random::string;
//! # use tempdir::TempDir;
//! # use structopt::StructOpt;
//! let mut args = ConfigBootstrap::from_args();
//! # let temp_dir = TempDir::new(string(8).as_str()).unwrap();
//! # args.base_path = temp_dir.path().to_path_buf();
//! # args.init = true;
//! args.init_dirs();
//! let config = args.load_configuration().unwrap();
//! let global = GlobalConfig::convert_from(config).unwrap();
//! assert_eq!(global.network, Network::Rincewind);
//! assert_eq!(global.blocking_threads, 4);
//! # std::fs::remove_dir_all(temp_dir).unwrap();
//! ```

pub mod configuration;
#[macro_use]
mod logging;

pub mod protobuf_build;
pub use configuration::error::ConfigError;

pub mod dir_utils;
pub use configuration::{
    bootstrap::{install_configuration, ConfigBootstrap},
    global::{CommsTransport, DatabaseType, GlobalConfig, Network, SocksAuthentication, TorControlAuthentication},
    loader::{ConfigExtractor, ConfigLoader, ConfigPath, ConfigurationError, DefaultConfigLoader, NetworkConfigPath},
    utils::{default_config, install_default_config_file, load_configuration},
};
pub use logging::initialize_logging;

pub const DEFAULT_CONFIG: &str = "config.toml";
pub const DEFAULT_LOG_CONFIG: &str = "log4rs.yml";

pub(crate) const LOG_TARGET: &str = "common::config";
