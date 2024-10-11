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

use std::{net::SocketAddr, str::FromStr};

use dialoguer::Input as InputPrompt;
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    conversions::multiaddr::multiaddr_to_socketaddr,
    tari_rpc::{
        base_node_client::BaseNodeClient,
        sha_p2_pool_client::ShaP2PoolClient,
        Block,
        NewBlockTemplate,
        NewBlockTemplateRequest,
    },
};
use tari_common::configuration::{
    bootstrap::{grpc_default_port, ApplicationType},
    Network,
};
use tari_common_types::tari_address::TariAddress;
use tari_network::multiaddr::Multiaddr;
use thiserror::Error;
use tonic::{codegen::InterceptedService, transport::Channel, Code};

/// Error parsing input
#[derive(Debug, Error)]
pub enum ParseInputError {
    #[error("Could not convert data:{0}")]
    WalletPaymentAddress(String),
    #[error("Could not convert data:{0}")]
    BaseNodeSocketAddress(String),
}

/// Read base_node_socket_address arg or prompt for input
pub fn base_node_socket_address(
    base_node_grpc_address: Option<Multiaddr>,
    network: Network,
) -> Result<SocketAddr, ParseInputError> {
    match base_node_grpc_address {
        Some(address) => {
            println!("Base node gRPC address: '{}'", address);
            match multiaddr_to_socketaddr(&address) {
                Ok(val) => Ok(val),
                Err(e) => Err(ParseInputError::BaseNodeSocketAddress(format!(
                    "Error - base node socket address '{}' not valid ({:?})",
                    address, e
                ))),
            }
        },
        None => {
            println!();
            // Get it on the command line
            loop {
                let mut address = InputPrompt::<String>::new()
                    .with_prompt("Please enter 'base-node-grpc-address' ('quit' or 'exit' to quit) ")
                    .default(format!(
                        "/ip4/127.0.0.1/tcp/{}",
                        grpc_default_port(ApplicationType::BaseNode, network)
                    ))
                    .interact()
                    .unwrap();
                process_quit(&address);
                // Remove leading and trailing whitespace
                address = address.trim().to_string();
                let base_node_multi_address: Result<Multiaddr, String> =
                    address.parse().map_err(|e| format!("{:?}", e));
                match base_node_multi_address {
                    Ok(val) => match multiaddr_to_socketaddr(&val) {
                        Ok(val) => {
                            println!();
                            return Ok(val);
                        },
                        Err(e) => println!("  Error - base node socket address '{}' not valid ({:?})", val, e),
                    },
                    Err(e) => println!("  Error - base node gRPC address '{}' not valid ({:?})", address, e),
                }
            }
        },
    }
}

/// Read wallet_payment_address arg or prompt for input
pub fn wallet_payment_address(
    config_wallet_payment_address: String,
    network: Network,
) -> Result<TariAddress, ParseInputError> {
    // Verify config setting
    return match TariAddress::from_str(&config_wallet_payment_address) {
        Ok(address) => {
            if address == TariAddress::default() {
                println!();
                // Get it on the command line
                loop {
                    let mut address = InputPrompt::<String>::new()
                        .with_prompt("Please enter 'wallet-payment-address' ('quit' or 'exit' to quit) ")
                        .interact()
                        .unwrap();
                    process_quit(&address);
                    // Remove leading and trailing whitespace
                    address = address.trim().to_string();
                    let wallet_address: Result<TariAddress, String> = address.parse().map_err(|e| format!("{:?}", e));
                    match wallet_address {
                        Ok(val) => {
                            if val.network() == network {
                                return Ok(val);
                            } else {
                                println!(
                                    "  Error - wallet payment address '{}' does not match miner network '{}'",
                                    address, network
                                );
                            }
                        },
                        Err(e) => println!("  Error - wallet payment address '{}' not valid ({})", address, e),
                    }
                }
            }
            if address.network() != network {
                return Err(ParseInputError::WalletPaymentAddress(format!(
                    "Wallet payment address '{}' does not match miner network '{}'",
                    config_wallet_payment_address, network
                )));
            }
            Ok(address)
        },
        Err(err) => Err(ParseInputError::WalletPaymentAddress(format!(
            "Wallet payment address '{}' not valid ({})",
            config_wallet_payment_address, err
        ))),
    };
}

/// User requested quit
pub fn process_quit(command: &str) {
    if command.to_uppercase() == "QUIT" || command.to_uppercase() == "EXIT" {
        println!("\nUser requested quit (Press 'Enter')");
        wait_for_keypress();
        std::process::exit(0);
    }
}

/// Wait for a keypress before continuing
pub fn wait_for_keypress() {
    use std::io::{stdin, Read};
    let mut stdin = stdin();
    let buf: &mut [u8] = &mut [0; 2];
    let _unused = stdin.read(buf).expect("Error reading keypress");
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
    if let Err(e) = get_new_block_template {
        if e.code() == Code::PermissionDenied {
            return Err("'get_new_block_template'".to_string());
        }
    };
    let get_tip_info = node_conn.get_tip_info(minotari_app_grpc::tari_rpc::Empty {}).await;
    if let Err(e) = get_tip_info {
        if e.code() == Code::PermissionDenied {
            return Err("'get_tip_info'".to_string());
        }
    }
    let block_result = node_conn.get_new_block(NewBlockTemplate::default()).await;
    if let Err(e) = block_result {
        if e.code() == Code::PermissionDenied {
            return Err("'get_new_block'".to_string());
        }
    }
    if let Err(e) = node_conn.submit_block(Block::default()).await {
        if e.code() == Code::PermissionDenied {
            return Err("'submit_block'".to_string());
        }
    }
    Ok(())
}
