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
        OutputType,
        base_node_client::BaseNodeClient,
        sha_p2_pool_client::ShaP2PoolClient,
    },
};
use tari_common::configuration::{
    Network,
    bootstrap::{ApplicationType, grpc_default_port},
};
use tari_common_types::{
    tari_address::{TariAddress, TariAddressFeatures},
    types::FixedHash,
};
use tari_core::{PrunedOutputMmr, block_output_mr_hash_from_pruned_mmr};
use tari_mmr::pruned_hashset::PrunedHashSet;
use tari_script::{Opcode, TariScript, push_pubkey_script};
use tari_transaction_components::transaction_components::TransactionOutput as CoreTransactionOutput;
use tari_utilities::hex::Hex;
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

/// Recompute the `block_output_mr` committed by a block header from its body outputs.
///
/// Converts each gRPC output into a core `TransactionOutput`, pushes non-coinbase outputs
/// into the normal output MMR, pushes coinbase outputs into the block output MMR,
/// and folds the normal output MMR root into the block output MMR root.
pub fn calculate_block_output_mr_from_grpc_outputs(
    outputs: &[minotari_app_grpc::tari_rpc::TransactionOutput],
) -> Result<FixedHash, String> {
    let mut normal_output_mmr = PrunedOutputMmr::new(PrunedHashSet::default());
    let mut block_output_mmr = PrunedOutputMmr::new(PrunedHashSet::default());

    for output in outputs {
        let core_output = CoreTransactionOutput::try_from(output.clone())
            .map_err(|e| format!("Invalid output in p2pool block body: {e}"))?;
        let output_hash = core_output.hash();
        if core_output.features.is_coinbase() {
            block_output_mmr
                .push(output_hash.to_vec())
                .map_err(|e| format!("Failed to push coinbase output to MMR: {e}"))?;
        } else {
            normal_output_mmr
                .push(output_hash.to_vec())
                .map_err(|e| format!("Failed to push normal output to MMR: {e}"))?;
        }
    }

    let normal_output_mr = normal_output_mmr
        .get_merkle_root()
        .map_err(|e| format!("Failed to calculate normal output MR: {e}"))?;
    block_output_mmr
        .push(normal_output_mr.to_vec())
        .map_err(|e| format!("Failed to push normal output MR into block output MMR: {e}"))?;

    block_output_mr_hash_from_pruned_mmr(&block_output_mmr)
        .map_err(|e| format!("Failed to calculate block output MR: {e}"))
}

/// Constructs a canonical revealed-value coinbase output for testing purposes.
pub fn make_test_coinbase_output(address: &TariAddress, value: u64) -> minotari_app_grpc::tari_rpc::TransactionOutput {
    let core_output = CoreTransactionOutput::new_current_version(
        tari_transaction_components::transaction_components::OutputFeatures::create_coinbase(
            10,
            None,
            tari_transaction_components::transaction_components::RangeProofType::RevealedValue,
        ),
        tari_common_types::types::CompressedCommitment::default(),
        None,
        push_pubkey_script(address.public_spend_key()),
        tari_common_types::types::CompressedPublicKey::default(),
        tari_common_types::types::ComAndPubSignature::default(),
        tari_transaction_components::transaction_components::covenants::Covenant::default(),
        tari_transaction_components::transaction_components::EncryptedData::default(),
        tari_transaction_components::MicroMinotari::from(value),
    );
    minotari_app_grpc::conversions::transaction_output::grpc_output_with_payref(core_output, None)
        .expect("Valid coinbase conversion")
}

fn output_matches_address(
    output: &minotari_app_grpc::tari_rpc::TransactionOutput,
    wallet_payment_address: &TariAddress,
    expected_script_bytes: &[u8],
) -> bool {
    output.script == expected_script_bytes ||
        TariScript::from_bytes(&output.script).is_ok_and(
            |script| matches!(script.as_slice(), [Opcode::PushPubKey(pk)] if **pk == *wallet_payment_address.public_spend_key()),
        )
}

fn coinbase_recipient(
    output: &minotari_app_grpc::tari_rpc::TransactionOutput,
    wallet_payment_address: &TariAddress,
) -> String {
    let Ok(script) = TariScript::from_bytes(&output.script) else {
        return "malformed_script".to_string();
    };
    let [Opcode::PushPubKey(pk)] = script.as_slice() else {
        return format!("script({script})");
    };
    TariAddress::new_single_address(
        (**pk).clone(),
        wallet_payment_address.network(),
        TariAddressFeatures::create_one_sided_only(),
    )
    .map_or_else(|_| pk.to_hex(), |address| address.to_base58())
}

/// Validate that a p2pool block template commits to its aggregate outputs via `header.block_output_mr`
/// and contains a non-stealth, revealed-value coinbase output whose script matches `wallet_payment_address`.
///
/// Security bounds:
/// 1. Recomputes `block_output_mr` from the body outputs and verifies equality with `header.block_output_mr`, binding
///    the checked coinbase set to the header PoW being mined.
/// 2. Assumes non-stealth, revealed-value coinbases as constructed by sha-p2pool. Stealth-addressed coinbases or
///    unrevealed BulletProofPlus coinbases fail safe.
/// 3. Note that because a miner only holds a public payment address (and not the wallet private view key), the miner
///    can verify the spend key script match and the revealed promise value, but cannot verify if the encrypted data was
///    encrypted with the miner's view key or whether the value represents the miner's full proportional share versus
///    dust.
pub fn verify_p2pool_block_coinbase(
    block: &Block,
    wallet_payment_address: &TariAddress,
    log_target: &str,
) -> Result<u64, String> {
    let header = block
        .header
        .as_ref()
        .ok_or_else(|| "P2Pool block is missing header".to_string())?;

    let body = block
        .body
        .as_ref()
        .ok_or_else(|| "P2Pool block is missing aggregate body".to_string())?;

    let calculated_block_output_mr = calculate_block_output_mr_from_grpc_outputs(&body.outputs)?;

    if header.block_output_mr.as_slice() != calculated_block_output_mr.as_slice() {
        let err_msg = format!(
            "P2Pool block header block_output_mr ({}) does not commit to body outputs (calculated: {})",
            header.block_output_mr.to_hex(),
            calculated_block_output_mr.to_hex()
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    let expected_script = push_pubkey_script(wallet_payment_address.public_spend_key());
    let expected_script_bytes = expected_script.to_bytes();

    let mut found_recipients = Vec::new();
    let mut total_paid_value = 0u64;
    let mut has_matching_coinbase = false;
    let mut coinbase_count = 0usize;
    let mut bp_plus_coinbase_detected = false;

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

        if output_matches_address(output, wallet_payment_address, &expected_script_bytes) {
            has_matching_coinbase = true;
            if output.minimum_value_promise == 0 {
                let range_proof_type = output.features.as_ref().map(|f| f.range_proof_type).unwrap_or(0);
                if range_proof_type == 0 {
                    bp_plus_coinbase_detected = true;
                }
            }
            total_paid_value = total_paid_value.saturating_add(output.minimum_value_promise);
        } else {
            found_recipients.push(coinbase_recipient(output, wallet_payment_address));
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
            "P2Pool block coinbase outputs do not pay configured wallet_payment_address '{}'. Found coinbase \
             recipients: [{}]",
            wallet_payment_address.to_base58(),
            found_recipients.join(", ")
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    if bp_plus_coinbase_detected && total_paid_value == 0 {
        let err_msg = format!(
            "P2Pool block coinbase output pays wallet_payment_address '{}' but uses BulletProofPlus range proof with \
             0 minimum_value_promise; p2pool coinbases must use revealed-value proofs",
            wallet_payment_address.to_base58()
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    if total_paid_value == 0 {
        let err_msg = format!(
            "P2Pool block coinbase output for wallet_payment_address '{}' provides 0 payout value",
            wallet_payment_address.to_base58()
        );
        log::error!(target: log_target, "{err_msg}");
        return Err(err_msg);
    }

    log::info!(
        target: log_target,
        "P2Pool block coinbase candidate found for address {} with revealed value promise of {} MicroMinotari",
        wallet_payment_address.to_base58(),
        total_paid_value
    );

    Ok(total_paid_value)
}

#[cfg(test)]
mod test {
    use std::process::{Command, Stdio};

    use minotari_app_grpc::{
        conversions::transaction_output::grpc_output_with_payref,
        tari_rpc::{AggregateBody, BlockHeader, TransactionOutput},
    };
    use tari_common_types::types::{ComAndPubSignature, CompressedCommitment, CompressedPublicKey, PrivateKey};
    use tari_transaction_components::{
        MicroMinotari,
        transaction_components::{
            EncryptedData,
            OutputFeatures as CoreOutputFeatures,
            RangeProofType as CoreRangeProofType,
            TransactionOutput as CoreTransactionOutput,
            covenants::Covenant,
        },
    };

    use super::*;

    const VALID_MAINNET_ADDRESS: &str =
        "126J92Yow5y9UoRFd1DNujPmVFq9C1ZeiYWT95UKxz5Y1rzbfjtHg4SCZS1dk83ivzt3m2XRQHTaYUk9SwmyeCvy5BJ";

    fn make_test_address() -> TariAddress {
        TariAddress::default()
    }

    fn make_other_address() -> TariAddress {
        let key = CompressedPublicKey::from_secret_key(&PrivateKey::from(42u64));
        TariAddress::new_single_address(
            key,
            TariAddress::default().network(),
            TariAddressFeatures::create_one_sided_only(),
        )
        .unwrap()
    }

    fn make_coinbase_output(address: &TariAddress, value: u64) -> TransactionOutput {
        make_test_coinbase_output(address, value)
    }

    fn make_bp_plus_coinbase_output(address: &TariAddress) -> TransactionOutput {
        let core_output = CoreTransactionOutput::new_current_version(
            CoreOutputFeatures::create_coinbase(10, None, CoreRangeProofType::BulletProofPlus),
            CompressedCommitment::default(),
            None,
            push_pubkey_script(address.public_spend_key()),
            CompressedPublicKey::default(),
            ComAndPubSignature::default(),
            Covenant::default(),
            EncryptedData::default(),
            MicroMinotari::from(0),
        );
        grpc_output_with_payref(core_output, None).unwrap()
    }

    fn make_test_block(outputs: Vec<TransactionOutput>, corrupt_mr: bool) -> Block {
        let block_output_mr = if corrupt_mr {
            vec![0xAA; 32]
        } else {
            calculate_block_output_mr_from_grpc_outputs(&outputs).unwrap().to_vec()
        };

        Block {
            header: Some(BlockHeader {
                block_output_mr,
                ..Default::default()
            }),
            body: Some(AggregateBody {
                inputs: vec![],
                outputs,
                kernels: vec![],
            }),
        }
    }

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

    #[test]
    fn test_verify_p2pool_block_coinbase_success() {
        let address = make_test_address();
        let coinbase = make_coinbase_output(&address, 1_500_000);
        let block = make_test_block(vec![coinbase], false);

        let result = verify_p2pool_block_coinbase(&block, &address, "test");
        assert_eq!(result.unwrap(), 1_500_000);
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_header_root_mismatch() {
        let address = make_test_address();
        let coinbase = make_coinbase_output(&address, 1_500_000);
        let block = make_test_block(vec![coinbase], true);

        let result = verify_p2pool_block_coinbase(&block, &address, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("does not commit to body outputs"));
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_wrong_address() {
        let configured_address = make_test_address();
        let attacker_address = make_other_address();
        let coinbase = make_coinbase_output(&attacker_address, 1_500_000);
        let block = make_test_block(vec![coinbase], false);

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
        let block = make_test_block(
            vec![
                make_coinbase_output(&other_address, 500_000),
                make_coinbase_output(&configured_address, 2_000_000),
            ],
            false,
        );

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert_eq!(result.unwrap(), 2_000_000);
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_bp_plus_rejected() {
        let configured_address = make_test_address();
        let block = make_test_block(vec![make_bp_plus_coinbase_output(&configured_address)], false);

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("BulletProofPlus"));
    }

    #[test]
    fn test_verify_p2pool_block_coinbase_no_coinbase_outputs() {
        let configured_address = make_test_address();
        let block = make_test_block(vec![], false);

        let result = verify_p2pool_block_coinbase(&block, &configured_address, "test");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("no coinbase outputs"));
    }
}
