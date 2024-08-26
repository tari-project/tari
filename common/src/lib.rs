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

#[cfg(any(feature = "build", feature = "static-application-info"))]
pub mod build;
pub mod exit_codes;
pub mod network_check;
#[macro_use]
mod logging;
pub mod configuration;
pub use configuration::{
    bootstrap::install_configuration,
    error::ConfigError,
    loader::{ConfigLoader, ConfigPath, ConfigurationError, DefaultConfigLoader, SubConfigPath},
    name_server::DnsNameServer,
    utils::load_configuration,
};
pub mod dir_utils;

pub use logging::initialize_logging;

use crate::configuration::Network;

pub const DEFAULT_CONFIG: &str = "config/config.toml";
pub const DEFAULT_BASE_NODE_LOG_CONFIG: &str = "config/log4rs_base_node.yml";
pub const DEFAULT_WALLET_LOG_CONFIG: &str = "config/log4rs_console_wallet.yml";
pub const DEFAULT_MERGE_MINING_PROXY_LOG_CONFIG: &str = "config/log4rs_merge_mining_proxy.yml";
pub const DEFAULT_STRATUM_TRANSCODER_LOG_CONFIG: &str = "config/log4rs_miningcore_transcoder.yml";
pub const DEFAULT_MINER_LOG_CONFIG: &str = "config/log4rs_miner.yml";
pub const DEFAULT_COLLECTIBLES_LOG_CONFIG: &str = "config/log4rs_collectibles.yml";

pub(crate) const LOG_TARGET: &str = "common::config";

/// This is a static function that returns the genesis block hash for the specified network. This is useful for
/// applications that need to know the genesis block hash for a specific network, but do not have access to the
/// genesis block in tari_core. Test `fn test_get_static_genesis_block_hash()` in tari_core will fail if these values
/// are wrong.
pub fn get_static_genesis_block_hash(network: Network) -> &'static str {
    match network {
        Network::MainNet => "ba4379a1319a6315d5262f61761d3f609f5b8eb9fa30a05f0d18a80c25d6bae9",
        Network::StageNet => "cd073787a0bd8803a2546919523c687ccd88c8f0b39d652783530502e101f351",
        Network::NextNet => "5ae9384d705f8df49d7e5b5988297440a53bc8be48b8792f8bc0a2c3d17c3479",
        Network::Igor => "50ed5847a5b4b88dfd86fd48597801b72565a0e1ba14701fddbeaca356e8b4c3",
        Network::Esmeralda => "6598d13c5dcb398f5cad294473421bc2fed69071b56fada4387a6ad03a44df08",
        Network::LocalNet => "b693c14804ceaafaee77c2d01310a221960383128de6b0f36c581fb706332bb3",
    }
}
