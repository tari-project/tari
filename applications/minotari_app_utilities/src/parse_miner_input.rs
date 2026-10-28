// Copyright 2021. The Tari Project
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

use std::str::FromStr;

use dialoguer::{Input as InputPrompt, console::Term};
use log::*;
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::{
        Block,
        NewBlockTemplate,
        NewBlockTemplateRequest,
        base_node_client::BaseNodeClient,
        sha_p2_pool_client::ShaP2PoolClient,
    },
};
use tari_common::configuration::{
    Network,
    bootstrap::{ApplicationType, grpc_default_port},
};
use tari_common_types::tari_address::TariAddress;
use thiserror::Error;
use tonic::{
    Code,
    codegen::InterceptedService,
    transport::{Channel, Uri},
};

const LOG_TARGET: &str = "minotari::app_utilities::parse_miner_input";

/// User requested quit from interactive prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("User requested quit")]
pub struct UserQuit;

/// Error parsing input
#[derive(Debug, Error)]
pub enum ParseInputError {
    #[error("Could not convert data:{0}")]
    WalletPaymentAddress(String),
    #[error("Could not convert data:{0}")]
    BaseNodeSocketAddress(String),
    #[error("Non-interactive mode (or missing terminal TTY) requires '{0}' to be configured")]
    NonInteractive(&'static str),
    #[error("Prompt interaction error: {0}")]
    PromptError(#[from] std::io::Error),
    #[error(transparent)]
    UserQuit(#[from] UserQuit),
}

pub fn prompt_for_base_node_address(network: Network, non_interactive: bool) -> Result<String, ParseInputError> {
    let default_address = format!(
        "http://127.0.0.1:{}",
        grpc_default_port(ApplicationType::BaseNode, network)
    );
    if non_interactive || !Term::stderr().is_term() {
        info!(
            target: LOG_TARGET,
            "Non-interactive mode or missing terminal: assuming default base node address '{default_address}'"
        );
        return Ok(default_address);
    }
    loop {
        let mut address = InputPrompt::<String>::new()
            .with_prompt("Please enter 'base-node-grpc-address' ('quit' or 'exit' to quit) ")
            .default(default_address.clone())
            .interact()?;
        process_quit(&address)?;
        if address == default_address {
            info!(target: LOG_TARGET, "Assumed default base node address '{default_address}'");
        }
        if Uri::from_str(&address).is_err() {
            println!("  Error - base node address '{address}' not valid");
            continue;
        }
        // Remove leading and trailing whitespace
        address = address.trim().to_string();
        return Ok(address);
    }
}

pub fn prompt_for_p2pool_address(non_interactive: bool) -> Result<String, ParseInputError> {
    let default_address = format!("http://127.0.0.1:{}", 18145);
    if non_interactive || !Term::stderr().is_term() {
        info!(
            target: LOG_TARGET,
            "Non-interactive mode or missing terminal: assuming default p2pool address '{default_address}'"
        );
        return Ok(default_address);
    }
    loop {
        let mut address = InputPrompt::<String>::new()
            .with_prompt("Please enter 'p2pool-grpc-address' ('quit' or 'exit' to quit) ")
            .default(default_address.clone())
            .interact()?;
        process_quit(&address)?;
        if address == default_address {
            info!(target: LOG_TARGET, "Assumed default p2pool address '{default_address}'");
        }
        if Uri::from_str(&address).is_err() {
            println!("  Error - p2pool address '{address}' not valid");
            continue;
        }
        // Remove leading and trailing whitespace
        address = address.trim().to_string();
        return Ok(address);
    }
}

/// Read wallet_payment_address arg or prompt for input
pub fn wallet_payment_address(
    config_wallet_payment_address: String,
    network: Network,
    non_interactive: bool,
) -> Result<TariAddress, ParseInputError> {
    // Verify config setting
    match TariAddress::from_str(&config_wallet_payment_address) {
        Ok(address) => {
            if address == TariAddress::default() {
                if non_interactive || !Term::stderr().is_term() {
                    return Err(ParseInputError::NonInteractive("wallet_payment_address"));
                }
                println!();
                // Get it on the command line
                loop {
                    let mut address = InputPrompt::<String>::new()
                        .with_prompt("Please enter 'wallet-payment-address' ('quit' or 'exit' to quit) ")
                        .interact()?;
                    process_quit(&address)?;
                    // Remove leading and trailing whitespace
                    address = address.trim().to_string();
                    let wallet_address: Result<TariAddress, String> = address.parse().map_err(|e| format!("{e:?}"));
                    match wallet_address {
                        Ok(val) => {
                            if val.network() == network {
                                return Ok(val);
                            } else {
                                println!(
                                    "  Error - wallet payment address '{address}' does not match miner network \
                                     '{network}'"
                                );
                            }
                        },
                        Err(e) => println!("  Error - wallet payment address '{address}' not valid ({e})"),
                    }
                }
            }
            if address.network() != network {
                return Err(ParseInputError::WalletPaymentAddress(format!(
                    "Wallet payment address '{config_wallet_payment_address}' does not match miner network '{network}'"
                )));
            }
            Ok(address)
        },
        Err(err) => Err(ParseInputError::WalletPaymentAddress(format!(
            "Wallet payment address '{config_wallet_payment_address}' not valid ({err})"
        ))),
    }
}

/// User requested quit
pub fn process_quit(command: &str) -> Result<(), UserQuit> {
    if command.to_uppercase() == "QUIT" || command.to_uppercase() == "EXIT" {
        return Err(UserQuit);
    }
    Ok(())
}

/// Base node gRPC client
pub type BaseNodeGrpcClient = BaseNodeClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;

/// SHA P2Pool gRPC client
pub type ShaP2PoolGrpcClient = ShaP2PoolClient<InterceptedService<Channel, ClientAuthenticationInterceptor>>;

/// Verify that the base node is responding to the mining gRPC requests
pub async fn verify_base_node_grpc_mining_responses(
    node_conn: &mut BaseNodeGrpcClient,
    pow_algo_request: NewBlockTemplateRequest,
) -> Result<(), String> {
    let get_new_block_template = node_conn.get_new_block_template(pow_algo_request).await;
    if let Err(e) = get_new_block_template &&
        e.code() == Code::PermissionDenied
    {
        return Err("'get_new_block_template'".to_string());
    }
    let get_tip_info = node_conn.get_tip_info(minotari_app_grpc::tari_rpc::Empty {}).await;
    if let Err(e) = get_tip_info &&
        e.code() == Code::PermissionDenied
    {
        return Err("'get_tip_info'".to_string());
    }
    let block_result = node_conn.get_new_block(NewBlockTemplate::default()).await;
    if let Err(e) = block_result &&
        e.code() == Code::PermissionDenied
    {
        return Err("'get_new_block'".to_string());
    }
    if let Err(e) = node_conn.submit_block(Block::default()).await &&
        e.code() == Code::PermissionDenied
    {
        return Err("'submit_block'".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::process::{Command, Stdio};

    use super::*;

    const VALID_MAINNET_ADDRESS: &str =
        "126J92Yow5y9UoRFd1DNujPmVFq9C1ZeiYWT95UKxz5Y1rzbfjtHg4SCZS1dk83ivzt3m2XRQHTaYUk9SwmyeCvy5BJ";

    #[test]
    fn test_process_quit_returns_user_quit() {
        assert!(matches!(process_quit("quit"), Err(UserQuit)));
        assert!(matches!(process_quit("QUIT"), Err(UserQuit)));
        assert!(matches!(process_quit("exit"), Err(UserQuit)));
        assert!(matches!(process_quit("EXIT"), Err(UserQuit)));
        assert!(process_quit("continue").is_ok());
        assert!(process_quit("").is_ok());
    }

    #[test]
    fn test_wallet_payment_address_non_interactive_error() {
        let default_addr = TariAddress::default().to_string();
        let res = wallet_payment_address(default_addr, Network::Esmeralda, true);
        match res {
            Err(ParseInputError::NonInteractive(key)) => assert_eq!(key, "wallet_payment_address"),
            _ => panic!("Expected NonInteractive error, got {res:?}"),
        }
    }

    #[test]
    fn test_prompt_for_base_node_address_non_interactive_fallback() {
        let res = prompt_for_base_node_address(Network::Esmeralda, true).expect("should fall back to default");
        let expected = format!(
            "http://127.0.0.1:{}",
            grpc_default_port(ApplicationType::BaseNode, Network::Esmeralda)
        );
        assert_eq!(res, expected);
    }

    #[test]
    fn test_prompt_for_p2pool_address_non_interactive_fallback() {
        let res = prompt_for_p2pool_address(true).expect("should fall back to default");
        assert_eq!(res, "http://127.0.0.1:18145");
    }

    #[test]
    fn test_wallet_payment_address_valid_config() {
        let valid_addr = TariAddress::from_str(VALID_MAINNET_ADDRESS).expect("fixture must be a valid TariAddress");
        let res = wallet_payment_address(VALID_MAINNET_ADDRESS.to_string(), valid_addr.network(), true)
            .expect("valid address should succeed");
        assert_eq!(res, valid_addr);
    }

    #[test]
    fn test_wallet_payment_address_network_mismatch() {
        let valid_addr = TariAddress::from_str(VALID_MAINNET_ADDRESS).expect("fixture must be a valid TariAddress");
        let other_net = if valid_addr.network() == Network::MainNet {
            Network::Esmeralda
        } else {
            Network::MainNet
        };
        let res = wallet_payment_address(VALID_MAINNET_ADDRESS.to_string(), other_net, true);
        assert!(matches!(res, Err(ParseInputError::WalletPaymentAddress(_))));
    }

    #[test]
    fn test_prompts_non_tty_with_null_stdin() {
        if std::env::var("TARI_TEST_NON_TTY_CHILD").is_ok() {
            assert!(!Term::stderr().is_term(), "child stderr must not be a TTY");
            let default_addr = TariAddress::default().to_string();
            let err = wallet_payment_address(default_addr, Network::Esmeralda, false)
                .expect_err("default address without TTY must error even when non_interactive is false");
            assert!(matches!(err, ParseInputError::NonInteractive("wallet_payment_address")));
            assert!(err.to_string().contains("wallet_payment_address"));
            assert!(err.to_string().contains("TTY"));

            let default_base = format!(
                "http://127.0.0.1:{}",
                grpc_default_port(ApplicationType::BaseNode, Network::Esmeralda)
            );
            let base_res = prompt_for_base_node_address(Network::Esmeralda, false)
                .expect("base node prompt without TTY must fall back to default");
            assert_eq!(base_res, default_base);

            let p2pool_res =
                prompt_for_p2pool_address(false).expect("p2pool prompt without TTY must fall back to default");
            assert_eq!(p2pool_res, "http://127.0.0.1:18145");
            return;
        }

        let exe = std::env::current_exe().expect("current test executable path");
        let status = Command::new(exe)
            .arg("--exact")
            .arg("parse_miner_input::test::test_prompts_non_tty_with_null_stdin")
            .env("TARI_TEST_NON_TTY_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("failed to spawn child test process");
        assert!(status.success(), "non-TTY child test failed with status: {status}");
    }
}
