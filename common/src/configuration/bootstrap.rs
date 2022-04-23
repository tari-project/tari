// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    path::Path,
    str::FromStr,
};

use super::error::ConfigError;

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
    Miner,
    StratumTranscoder,
    ValidatorNode,
    Collectibles,
}

impl ApplicationType {
    pub const fn as_str(&self) -> &'static str {
        #[allow(clippy::enum_glob_use)]
        use ApplicationType::*;
        match self {
            BaseNode => "Tari Base Node",
            ConsoleWallet => "Tari Console Wallet",
            MergeMiningProxy => "Tari Merge Mining Proxy",
            Miner => "Tari Miner",
            ValidatorNode => "Digital Assets Network Validator Node",
            StratumTranscoder => "Tari Stratum Transcoder",
            Collectibles => "Tari Collectibles",
        }
    }

    pub const fn as_config_str(&self) -> &'static str {
        #[allow(clippy::enum_glob_use)]
        use ApplicationType::*;
        match self {
            BaseNode => "base_node",
            ConsoleWallet => "wallet",
            MergeMiningProxy => "merge_mining_proxy",
            Miner => "miner",
            StratumTranscoder => "stratum-transcoder",
            ValidatorNode => "validator-node",
            Collectibles => "collectibles",
        }
    }
}

impl FromStr for ApplicationType {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        #[allow(clippy::enum_glob_use)]
        use ApplicationType::*;
        match s {
            "base-node" | "base_node" => Ok(BaseNode),
            "console-wallet" | "console_wallet" => Ok(ConsoleWallet),
            "mm-proxy" | "mm_proxy" => Ok(MergeMiningProxy),
            "miner" => Ok(Miner),
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
