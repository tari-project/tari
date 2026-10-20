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

use dialoguer::Input as InputPrompt;
use minotari_app_grpc::{
    authentication::ClientAuthenticationInterceptor,
    tari_rpc::{
        Block,
        NewBlockTemplate,
        NewBlockTemplateRequest,
        OutputType,
        base_node_client::BaseNodeClient,
        sha_p2_pool_client::ShaP2PoolClient,
    },
};
use tari_common::configuration::{
    Network,
    bootstrap::{ApplicationType, grpc_default_port},
};
use tari_common_types::tari_address::{TariAddress, TariAddressFeatures};
use tari_script::{Opcode, TariScript, push_pubkey_script};
use tari_utilities::hex::Hex;
use thiserror::Error;
use tonic::{
    Code,
    codegen::InterceptedService,
    transport::{Channel, Uri},
};

/// Error parsing input
#[derive(Debug, Error)]
pub enum ParseInputError {
    #[error("Could not convert data:{0}")]
    WalletPaymentAddress(String),
    #[error("Could not convert data:{0}")]
    BaseNodeSocketAddress(String),
}

pub fn prompt_for_base_node_address(network: Network) -> Result<String, ParseInputError> {
    loop {
        let mut address = InputPrompt::<String>::new()
            .with_prompt("Please enter 'base-node-grpc-address' ('quit' or 'exit' to quit) ")
            .default(format!(
                "http://127.0.0.1:{}",
                grpc_default_port(ApplicationType::BaseNode, network)
            ))
            .interact()
            .unwrap();
        process_quit(&address);
        if Uri::from_str(&address).is_err() {
            println!("  Error - base node address '{address}' not valid");
            continue;
        }
        // Remove leading and trailing whitespace
        address = address.trim().to_string();
        return Ok(address);
    }
}

pub fn prompt_for_p2pool_address() -> Result<String, ParseInputError> {
    loop {
        let mut address = InputPrompt::<String>::new()
            .with_prompt("Please enter 'p2pool-grpc-address' ('quit' or 'exit' to quit) ")
            .default(format!("http://127.0.0.1:{}", 18145))
            .interact()
            .unwrap();
        process_quit(&address);
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
) -> Result<TariAddress, ParseInputError> {
    // Verify config setting
    match TariAddress::from_str(&config_wallet_payment_address) {
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
pub fn process_quit(command: &str) {
    if command.to_uppercase() == "QUIT" || command.to_uppercase() == "EXIT" {
        println!("\nUser requested quit (Press 'Enter')");
        wait_for_keypress();
        std::process::exit(0);
    }
}

/// Wait for a keypress before continuing
pub fn wait_for_keypress() {
    use std::io::{Read, stdin};
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

/// Validate that a p2pool block template contains at least one coinbase output paying the configured
/// `wallet_payment_address`.
///
/// Refuses to mine and logs at `error` level with both the configured address and the found coinbase recipient
/// addresses if no coinbase output pays `wallet_payment_address`.
///
/// When an output pays `wallet_payment_address`, logs the coinbase value paid at `info` level and returns the total
/// value in MicroMinotari.
pub fn verify_p2pool_block_coinbase(
    block: &Block,
    wallet_payment_address: &TariAddress,
    log_target: &str,
) -> Result<u64, String> {
    let body = block
        .body
        .as_ref()
        .ok_or_else(|| "P2Pool block is missing aggregate body".to_string())?;

    let expected_script = push_pubkey_script(wallet_payment_address.public_spend_key());
    let expected_script_bytes = expected_script.to_bytes();

    let mut found_recipients = Vec::new();
    let mut total_paid_value = 0u64;
    let mut has_matching_coinbase = false;
    let mut coinbase_count = 0usize;

    for output in &body.outputs {
        let is_coinbase = output
            .features
            .as_ref()
            .map(|f| f.output_type == OutputType::Coinbase as u32)
            .unwrap_or(false);

        if !is_coinbase {
            continue;
        }

        coinbase_count = coinbase_count.saturating_add(1);

        let matches_address = if output.script == expected_script_bytes {
            true
        } else if let Ok(script) = TariScript::from_bytes(&output.script) {
            matches!(script.as_slice(), [Opcode::PushPubKey(pk)] if **pk == *wallet_payment_address.public_spend_key())
        } else {
            false
        };

        if matches_address {
            has_matching_coinbase = true;
            total_paid_value = total_paid_value.saturating_add(output.minimum_value_promise);
        } else if let Ok(script) = TariScript::from_bytes(&output.script) {
            if let [Opcode::PushPubKey(pk)] = script.as_slice() {
                let addr_str = match TariAddress::new_single_address(
                    (**pk).clone(),
                    wallet_payment_address.network(),
                    TariAddressFeatures::create_one_sided_only(),
                ) {
                    Ok(addr) => addr.to_base58(),
                    Err(_) => pk.to_hex(),
                };
                found_recipients.push(addr_str);
            } else {
                found_recipients.push(format!("script({script})"));
            }
        } else {
            found_recipients.push("malformed_script".to_string());
        }
    }

    if coinbase_count == 0 {
        let err_msg = format!(
            "P2Pool block has no coinbase outputs; expected payment to wallet_payment_address '{}'",
            wallet_payment_address.to_base58()
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    if !has_matching_coinbase {
        let err_msg = format!(
            "P2Pool block coinbase outputs do not pay configured wallet_payment_address '{}'. Found coinbase recipients: [{}]",
            wallet_payment_address.to_base58(),
            found_recipients.join(", ")
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    if total_paid_value == 0 {
        let err_msg = format!(
            "P2Pool block coinbase output for wallet_payment_address '{}' has a zero verified payout value (reward must be positive)",
            wallet_payment_address.to_base58()
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    log::info!(
        target: log_target,
        "Coinbase payout verified for wallet_payment_address {}: {} MicroMinotari",
        wallet_payment_address.to_base58(),
        total_paid_value
    );

    Ok(total_paid_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minotari_app_grpc::tari_rpc::{AggregateBody, OutputFeatures, TransactionOutput};
    use tari_common_types::types::{CompressedPublicKey, PrivateKey};

    fn make_test_address() -> TariAddress {
        TariAddress::default()
    }

    fn make_other_address() -> TariAddress {
        let key = CompressedPublicKey::from_secret_key(&PrivateKey::from(42u64));
        TariAddress::new_single_address(key, TariAddress::default().network(), TariAddressFeatures::create_one_sided_only()).unwrap()
    }

    fn make_coinbase_output(address: &TariAddress, value: u64) -> TransactionOutput {
        let script = push_pubkey_script(address.public_spend_key());
        TransactionOutput {
            features: Some(OutputFeatures {
                output_type: OutputType::Coinbase as u32,
                maturity: 10,
                coinbase_extra: vec![],
                sidechain_feature: None,
                range_proof_type: 1, // RevealedValue
                version: 0,
            }),
            commitment: vec![0u8; 32],
            range_proof: None,
            hash: vec![],
            script: script.to_bytes(),
            sender_offset_public_key: vec![0u8; 32],
            metadata_signature: None,
            covenant: vec![],
            version: 0,
            encrypted_data: vec![],
            minimum_value_promise: value,
            payment_reference: vec![],
        }
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_success() {
        let address = make_test_address();
        let coinbase = make_coinbase_output(&address, 1_500_000);
        let block = Block {
            header: None,
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![coinbase],
                kernels: vec![],
            }),
        };

        let result = verify_p2pool_block_coinbase(&block, &address, "test");
        assert_eq!(result.unwrap(), 1_500_000);
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_wrong_address() {
        let configured_address = make_test_address();
        let attacker_address = make_other_address();
        let coinbase = make_coinbase_output(&attacker_address, 1_500_000);
        let block = Block {
            header: None,
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![coinbase],
                kernels: vec![],
            }),
        };

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains(&configured_address.to_base58()));
        assert!(err.contains(&attacker_address.to_base58()));
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_multiple_coinbases() {
        let configured_address = make_test_address();
        let other_address = make_other_address();
        let block = Block {
            header: None,
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![
                    make_coinbase_output(&other_address, 500_000),
                    make_coinbase_output(&configured_address, 2_000_000),
                ],
                kernels: vec![],
            }),
        };

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert_eq!(result.unwrap(), 2_000_000);
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_no_coinbase_outputs() {
        let configured_address = make_test_address();
        let block = Block {
            header: None,
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![],
                kernels: vec![],
            }),
        };

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("no coinbase outputs"));
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_zero_payment_rejected() {
        let configured_address = make_test_address();
        let attacker_address = make_other_address();
        let block = Block {
            header: None,
            body: Some(AggregateBody {
                inputs: vec![],
                outputs: vec![
                    make_coinbase_output(&configured_address, 0),
                    make_coinbase_output(&attacker_address, 5_000_000),
                ],
                kernels: vec![],
            }),
        };

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("zero verified payout value"));
    }
}
