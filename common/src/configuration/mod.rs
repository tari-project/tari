//! # Configuration of tari applications
//!
//! Tari application consist of `common`, `base_node`, `wallet` and `application` configuration sections.
//! All tari apps follow traits implemented in this crate for ease and automation, for instance managing config files,
//! defaults configuration, overloading settings from subsections.
//!
//! ## Submodules
//!
//! - [bootstrap] - build CLI and manage/load configuration with [ConfigBootsrap] struct
//! - [global] - load GlobalConfig for Tari
//! - [loader] - build and load configuration modules in a tari-way
//! - [utils] - utilities for working with configuration
//!
//! ## Configuration file
//!
//! The tari configuration file (config.yml) is intended to be a single config file for all Tari desktop apps to use
//! to pull configuration variables, whether it's a testnet base node; wallet; validator node etc.
//!
//! The file lives in ~/.tari by default and has sections which will allow a specific app to determine
//! the config values it needs, e.g.
//!
//! ```toml
//! [common]
//! # Globally common variables
//! ...
//! [base_node]
//! # common vars for all base_node instances
//! [base_node.weatherwax]
//! # overrides for rincewnd testnet
//! [base_node.mainnet]
//! # overrides for mainnet
//! [wallet]
//! [wallet.weatherwax]
//! # etc..
//! ```

pub mod bootstrap;
pub mod error;
pub mod global;
pub mod loader;
mod network;
pub use network::Network;
mod merge_mining_config;
pub mod seconds;
pub mod utils;
mod validator_node_config;
pub mod writer;

pub use merge_mining_config::MergeMiningConfig;
pub use validator_node_config::ValidatorNodeConfig;
