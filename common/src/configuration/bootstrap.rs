// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt,
    fmt::{Display, Formatter},
    io,
    path::Path,
    str::FromStr,
};

use super::error::ConfigError;
use crate::configuration::Network;

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
            BaseNode => "Minotaiji Base Node",
            ConsoleWallet => "Minotaiji Wallet",
            MergeMiningProxy => "Minotaiji Merge Mining Proxy",
            Miner => "Minotaiji Miner",
            ValidatorNode => "Digital Assets Network Validator Node",
            StratumTranscoder => "Minotaiji Stratum Transcoder",
            Collectibles => "Taiji Collectibles",
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
            "collectibles" => Ok(Collectibles),
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

/// Gets the default grpc port for the given application and network
pub fn grpc_default_port(app_type: ApplicationType, network: Network) -> u16 {
    match app_type {
        ApplicationType::BaseNode => match network {
            Network::MainNet => 18102u16,
            Network::StageNet => 18172u16,
            Network::NextNet => 18182u16,
            Network::Esmeralda => 18142u16,
            Network::Igor => 18152u16,
            Network::LocalNet => 18162u16,
        },
        ApplicationType::ConsoleWallet => match network {
            Network::MainNet => 18103u16,
            Network::StageNet => 18173u16,
            Network::NextNet => 18183u16,
            Network::Esmeralda => 18143u16,
            Network::Igor => 18153u16,
            Network::LocalNet => 18163u16,
        },
        _ => unreachable!("Application {} not supported", app_type),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn application_type_as_str_test() {
        // get application type's
        let base_node = ApplicationType::BaseNode;
        let console_wallet = ApplicationType::ConsoleWallet;
        let mm_proxy = ApplicationType::MergeMiningProxy;
        let miner = ApplicationType::Miner;
        let stratum_transcoder = ApplicationType::StratumTranscoder;
        let validator_node = ApplicationType::ValidatorNode;
        let collectibles = ApplicationType::Collectibles;

        // test `as_str` method
        assert_eq!(base_node.as_str(), "Minotaiji Base Node");
        assert_eq!(console_wallet.as_str(), "Minotaiji Wallet");
        assert_eq!(mm_proxy.as_str(), "Minotaiji Merge Mining Proxy");
        assert_eq!(miner.as_str(), "Minotaiji Miner");
        assert_eq!(stratum_transcoder.as_str(), "Minotaiji Stratum Transcoder");
        assert_eq!(validator_node.as_str(), "Digital Assets Network Validator Node");
        assert_eq!(collectibles.as_str(), "Taiji Collectibles");

        // test `as_config_str` method
        assert_eq!(base_node.as_config_str(), "base_node");
        assert_eq!(console_wallet.as_config_str(), "wallet");
        assert_eq!(mm_proxy.as_config_str(), "merge_mining_proxy");
        assert_eq!(miner.as_config_str(), "miner");
        assert_eq!(stratum_transcoder.as_config_str(), "stratum-transcoder");
        assert_eq!(validator_node.as_config_str(), "validator-node");
        assert_eq!(collectibles.as_config_str(), "collectibles");
    }

    #[test]
    fn application_type_from_str_test() {
        // get application type's
        let node = ApplicationType::from_str("base-node").unwrap();
        let wallet = ApplicationType::from_str("console-wallet").unwrap();
        let mm_proxy = ApplicationType::from_str("mm-proxy").unwrap();
        let miner = ApplicationType::from_str("miner").unwrap();
        let stratum_transcoder = ApplicationType::from_str("stratum-proxy").unwrap();
        let validator = ApplicationType::from_str("validator-node").unwrap();
        let collectibles = ApplicationType::from_str("collectibles").unwrap();

        // asserts
        assert!(matches!(node, ApplicationType::BaseNode));
        assert!(matches!(wallet, ApplicationType::ConsoleWallet));
        assert!(matches!(mm_proxy, ApplicationType::MergeMiningProxy));
        assert!(matches!(miner, ApplicationType::Miner));
        assert!(matches!(stratum_transcoder, ApplicationType::StratumTranscoder));
        assert!(matches!(validator, ApplicationType::ValidatorNode));
        assert!(matches!(collectibles, ApplicationType::Collectibles));

        // in case of a non-specific message we should throw an error
        assert!(ApplicationType::from_str("random message").is_err());
    }
}
