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
        TransactionOutput as GrpcTransactionOutput,
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
    types::CompressedPublicKey,
};
use tari_script::{Opcode, TariScript, push_pubkey_script};
use tari_transaction_components::{
    MicroMinotari,
    transaction_components::{RangeProofType, TransactionOutput, calculate_block_output_mr},
};
use tari_utilities::hex::{Hex, to_hex};
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

fn describe_coinbase_output_recipient(script_bytes: &[u8], network: Network) -> String {
    if let Ok(script) = TariScript::from_bytes(script_bytes) &&
        let Some(Opcode::PushPubKey(pk)) = script.as_slice().first()
    {
        match TariAddress::new_single_address((**pk).clone(), network, TariAddressFeatures::create_one_sided_only()) {
            Ok(addr) => format!("{} (spend_key={})", addr.to_base58(), pk.to_hex()),
            Err(_) => format!("spend_key={}", pk.to_hex()),
        }
    } else {
        format!("script={}", to_hex(script_bytes))
    }
}

/// Summary of verified coinbase outputs paying `wallet_payment_address` in a p2pool block template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedP2PoolCoinbasePayout {
    /// Sum of `minimum_value_promise` across matching `RevealedValue` coinbase outputs.
    pub revealed_amount: MicroMinotari,
    /// Number of matching `RevealedValue` coinbase outputs with a positive value.
    pub revealed_outputs: usize,
    /// Number of matching `BulletProofPlus` coinbase outputs whose amount is hidden by a range proof.
    pub confidential_outputs: usize,
}

fn decode_and_verify_block_outputs<'a>(
    block: &'a Block,
    wallet_payment_address: &TariAddress,
    spend_key: &CompressedPublicKey,
    log_target: &str,
) -> Result<Vec<(&'a GrpcTransactionOutput, TransactionOutput)>, String> {
    let Some(header) = block.header.as_ref() else {
        let msg = format!(
            "Refusing to mine p2pool block: block header is missing (expected coinbase paying wallet_payment_address \
             {} (spend_key={}))",
            wallet_payment_address.to_base58(),
            spend_key.to_hex(),
        );
        error!(target: log_target, "{msg}");
        return Err(msg);
    };

    let Some(body) = block.body.as_ref() else {
        let msg = format!(
            "Refusing to mine p2pool block: block body is missing (expected coinbase paying wallet_payment_address {} \
             (spend_key={}))",
            wallet_payment_address.to_base58(),
            spend_key.to_hex(),
        );
        error!(target: log_target, "{msg}");
        return Err(msg);
    };

    if header.block_output_mr.is_empty() {
        let msg = format!(
            "Refusing to mine p2pool block: block header block_output_mr is empty (expected coinbase paying \
             wallet_payment_address {} (spend_key={}))",
            wallet_payment_address.to_base58(),
            spend_key.to_hex(),
        );
        error!(target: log_target, "{msg}");
        return Err(msg);
    }

    let mut domain_outputs = Vec::with_capacity(body.outputs.len());
    let mut core_outputs = Vec::with_capacity(body.outputs.len());
    for grpc_output in &body.outputs {
        let domain_output = TransactionOutput::try_from(grpc_output.clone()).map_err(|e| {
            let msg = format!("Refusing to mine p2pool block: invalid output in block body: {e}");
            error!(target: log_target, "{msg}");
            msg
        })?;
        core_outputs.push(domain_output.clone());
        domain_outputs.push((grpc_output, domain_output));
    }

    let computed_block_output_mr = calculate_block_output_mr(&core_outputs).map_err(|e| {
        let msg = format!("Refusing to mine p2pool block: failed to compute block_output_mr from body outputs: {e}");
        error!(target: log_target, "{msg}");
        msg
    })?;

    if header.block_output_mr.as_slice() != computed_block_output_mr.as_slice() {
        let msg = format!(
            "Refusing to mine p2pool block: block header block_output_mr ({}) does not match body outputs ({})",
            to_hex(&header.block_output_mr),
            computed_block_output_mr.to_hex(),
        );
        error!(target: log_target, "{msg}");
        return Err(msg);
    }

    Ok(domain_outputs)
}

fn tally_verified_coinbase_outputs(
    domain_outputs: &[(&GrpcTransactionOutput, TransactionOutput)],
    wallet_payment_address: &TariAddress,
    expected_script_bytes: &[u8],
    log_target: &str,
) -> Result<VerifiedP2PoolCoinbasePayout, String> {
    let mut revealed_outputs = 0usize;
    let mut confidential_outputs = 0usize;
    let mut revealed_amount = MicroMinotari::zero();
    let mut other_coinbase_recipients = Vec::new();
    let mut coinbase_count = 0usize;

    for (output, domain_output) in domain_outputs {
        if !domain_output.features.is_coinbase() {
            continue;
        }
        coinbase_count = coinbase_count.saturating_add(1);
        if output.script != expected_script_bytes {
            other_coinbase_recipients.push(describe_coinbase_output_recipient(
                &output.script,
                wallet_payment_address.network(),
            ));
            continue;
        }

        if domain_output.verify_metadata_signature().is_err() {
            other_coinbase_recipients.push("invalid_metadata_signature".to_string());
            continue;
        }

        match domain_output.features.range_proof_type {
            RangeProofType::RevealedValue => {
                if domain_output.minimum_value_promise > MicroMinotari::zero() {
                    revealed_outputs = revealed_outputs.saturating_add(1);
                    revealed_amount = revealed_amount.saturating_add(domain_output.minimum_value_promise);
                } else {
                    other_coinbase_recipients.push(describe_coinbase_output_recipient(
                        &output.script,
                        wallet_payment_address.network(),
                    ));
                }
            },
            RangeProofType::BulletProofPlus => {
                if domain_output.proof.is_some() {
                    confidential_outputs = confidential_outputs.saturating_add(1);
                } else {
                    other_coinbase_recipients.push("missing_bulletproof_plus".to_string());
                }
            },
        }
    }

    if revealed_outputs == 0 && confidential_outputs == 0 {
        let found_summary = if coinbase_count == 0 {
            "none (no coinbase outputs in block)".to_string()
        } else {
            other_coinbase_recipients.join(", ")
        };
        let msg = format!(
            "Refusing to mine p2pool block: no coinbase output pays configured wallet_payment_address {} \
             (spend_key={}); coinbase output recipients found: [{}]",
            wallet_payment_address.to_base58(),
            wallet_payment_address.public_spend_key().to_hex(),
            found_summary,
        );
        error!(target: log_target, "{msg}");
        return Err(msg);
    }

    Ok(VerifiedP2PoolCoinbasePayout {
        revealed_amount,
        revealed_outputs,
        confidential_outputs,
    })
}

fn log_verified_coinbase_payout(
    payout: VerifiedP2PoolCoinbasePayout,
    wallet_payment_address: &TariAddress,
    log_target: &str,
) {
    if payout.confidential_outputs == 0 {
        info!(
            target: log_target,
            "Verified p2pool block template coinbase payout of {} ({} uT) across {} output(s) to \
             wallet_payment_address {}",
            payout.revealed_amount,
            payout.revealed_amount.as_u64(),
            payout.revealed_outputs,
            wallet_payment_address.to_base58(),
        );
    } else if payout.revealed_outputs == 0 {
        info!(
            target: log_target,
            "Verified p2pool block template confidential coinbase payout across {} BulletProofPlus output(s) to \
             wallet_payment_address {} (amount hidden by range proof)",
            payout.confidential_outputs,
            wallet_payment_address.to_base58(),
        );
    } else {
        info!(
            target: log_target,
            "Verified p2pool block template coinbase payout of {} ({} uT) across {} revealed output(s) plus {} \
             confidential BulletProofPlus output(s) to wallet_payment_address {}",
            payout.revealed_amount,
            payout.revealed_amount.as_u64(),
            payout.revealed_outputs,
            payout.confidential_outputs,
            wallet_payment_address.to_base58(),
        );
    }
}

/// Verify that a block returned by a p2pool node binds `block.body.outputs` to `block.header.block_output_mr`
/// and contains at least one coinbase output paying `wallet_payment_address` (using the
/// `push_pubkey_script(wallet_payment_address.public_spend_key())` script produced by `generate_coinbase` and a
/// valid output metadata signature), logging the verified payout at `info` or logging both addresses at `error`
/// and returning an error if none does.
///
/// Note that this check verifies "paid to me" (that the header-committed block body includes at least one
/// non-zero coinbase output locked to `wallet_payment_address`'s public spend key), not "paid the right amount"
/// (a dust payout still passes, though the revealed amount is logged at `info`) nor that the encrypted data uses
/// the miner's view key.
pub fn verify_p2pool_coinbase_payout(
    block: &Block,
    wallet_payment_address: &TariAddress,
    log_target: &str,
) -> Result<VerifiedP2PoolCoinbasePayout, String> {
    let spend_key = wallet_payment_address.public_spend_key();
    if *spend_key == CompressedPublicKey::default() || spend_key.to_public_key().is_err() {
        let msg = format!(
            "Refusing to mine p2pool block: configured wallet_payment_address {} does not have a valid non-identity \
             public spend key",
            wallet_payment_address.to_base58(),
        );
        error!(target: log_target, "{msg}");
        return Err(msg);
    }

    let domain_outputs = decode_and_verify_block_outputs(block, wallet_payment_address, spend_key, log_target)?;

    // Note: sha-p2pool currently constructs coinbase outputs with `stealth_payment = false`, which locks the output
    // to `push_pubkey_script(wallet_payment_address.public_spend_key())`. If sha-p2pool switches to stealth coinbase
    // scripts in the future, this script check will need to be updated accordingly or every template will be rejected.
    let expected_script = push_pubkey_script(spend_key);
    let expected_script_bytes = expected_script.to_bytes();

    let payout = tally_verified_coinbase_outputs(
        &domain_outputs,
        wallet_payment_address,
        &expected_script_bytes,
        log_target,
    )?;
    log_verified_coinbase_payout(payout, wallet_payment_address, log_target);

    Ok(payout)
}

#[cfg(test)]
mod test {
    use std::{
        process::{Command, Stdio},
        sync::{Mutex, OnceLock},
    };

    use log::{Level, LevelFilter, Log, Metadata, Record};
    use minotari_app_grpc::{
        conversions::transaction_output::grpc_output_with_payref,
        tari_rpc::{AggregateBody as GrpcAggregateBody, BlockHeader as GrpcBlockHeader},
    };
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::{TariAddress, TariAddressFeatures},
        types::{CompressedPublicKey, PrivateKey},
    };
    use tari_transaction_components::{
        MicroMinotari,
        consensus::ConsensusConstantsBuilder,
        generate_coinbase,
        key_manager::KeyManager,
        transaction_components::{
            CoinBaseExtra,
            RangeProofType,
            memo_field::{MemoField, TxType},
        },
    };

    use super::*;

    const VALID_MAINNET_ADDRESS: &str =
        "126J92Yow5y9UoRFd1DNujPmVFq9C1ZeiYWT95UKxz5Y1rzbfjtHg4SCZS1dk83ivzt3m2XRQHTaYUk9SwmyeCvy5BJ";

    struct CaptureLogger;

    fn captured_logs() -> &'static Mutex<Vec<(Level, String)>> {
        static LOGS: OnceLock<Mutex<Vec<(Level, String)>>> = OnceLock::new();
        LOGS.get_or_init(|| Mutex::new(Vec::new()))
    }

    impl Log for CaptureLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            if let Ok(mut logs) = captured_logs().lock() {
                logs.push((record.level(), format!("{}", record.args())));
            }
        }

        fn flush(&self) {}
    }

    fn init_test_logger() {
        static INIT: OnceLock<()> = OnceLock::new();
        INIT.get_or_init(|| {
            let _ = log::set_boxed_logger(Box::new(CaptureLogger));
            log::set_max_level(LevelFilter::Info);
        });
    }

    fn sample_address(seed: u64) -> TariAddress {
        let view_key = CompressedPublicKey::from_secret_key(&PrivateKey::from(seed));
        let spend_key = CompressedPublicKey::from_secret_key(&PrivateKey::from(seed + 100));
        TariAddress::new_dual_address(
            view_key,
            spend_key,
            Network::LocalNet,
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
        .unwrap()
    }

    fn block_with_coinbase_proof_type(
        recipient: &TariAddress,
        reward: MicroMinotari,
        range_proof_type: RangeProofType,
    ) -> Block {
        let key_manager = KeyManager::new_random().unwrap();
        let constants = ConsensusConstantsBuilder::new(Network::LocalNet).build();
        let (coinbase_output, coinbase_kernel) = generate_coinbase(
            MicroMinotari::zero(),
            reward,
            1,
            &CoinBaseExtra::default(),
            &key_manager,
            recipient,
            false,
            &constants,
            range_proof_type,
            MemoField::new_open(vec![], TxType::Coinbase).unwrap(),
        )
        .unwrap();
        let block_output_mr = calculate_block_output_mr(std::slice::from_ref(&coinbase_output)).unwrap();
        let grpc_output = grpc_output_with_payref(coinbase_output, None).unwrap();
        Block {
            header: Some(GrpcBlockHeader {
                block_output_mr: block_output_mr.to_vec(),
                output_mr: block_output_mr.to_vec(),
                ..Default::default()
            }),
            body: Some(GrpcAggregateBody {
                inputs: vec![],
                outputs: vec![grpc_output],
                kernels: vec![coinbase_kernel.into()],
            }),
        }
    }

    fn block_with_coinbase(recipient: &TariAddress, reward: MicroMinotari) -> Block {
        block_with_coinbase_proof_type(recipient, reward, RangeProofType::RevealedValue)
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
    fn accepts_block_paying_configured_address_and_logs_value_at_info() {
        init_test_logger();
        let miner_address = sample_address(1);
        let reward = MicroMinotari::from(123_456_789);
        let block = block_with_coinbase(&miner_address, reward);

        let paid = verify_p2pool_coinbase_payout(&block, &miner_address, "test::p2pool")
            .expect("coinbase paying configured address must be accepted");
        assert_eq!(paid, VerifiedP2PoolCoinbasePayout {
            revealed_amount: reward,
            revealed_outputs: 1,
            confidential_outputs: 0,
        });

        let logs = captured_logs().lock().unwrap();
        assert!(
            logs.iter().any(|(level, msg)| {
                *level == Level::Info && msg.contains("123456789") && msg.contains(&miner_address.to_base58())
            }),
            "expected info log with paid value and miner address, got: {logs:?}"
        );
    }

    #[test]
    fn accepts_confidential_bulletproof_plus_coinbase_without_logging_zero_payout() {
        init_test_logger();
        let miner_address = sample_address(2);
        let reward = MicroMinotari::from(50_000_000);
        let block = block_with_coinbase_proof_type(&miner_address, reward, RangeProofType::BulletProofPlus);

        let paid = verify_p2pool_coinbase_payout(&block, &miner_address, "test::p2pool")
            .expect("confidential BulletProofPlus coinbase paying configured address must be accepted");
        assert_eq!(paid, VerifiedP2PoolCoinbasePayout {
            revealed_amount: MicroMinotari::zero(),
            revealed_outputs: 0,
            confidential_outputs: 1,
        });

        let logs = captured_logs().lock().unwrap();
        assert!(
            logs.iter().any(|(level, msg)| {
                *level == Level::Info &&
                    msg.contains("confidential coinbase payout") &&
                    msg.contains(&miner_address.to_base58()) &&
                    !msg.contains("0 uT")
            }),
            "expected confidential info log without '0 uT', got: {logs:?}"
        );
    }

    #[test]
    fn rejects_block_paying_different_address_and_logs_both_at_error() {
        init_test_logger();
        let miner_address = sample_address(10);
        let attacker_address = sample_address(20);
        let block = block_with_coinbase(&attacker_address, MicroMinotari::from(50_000_000));

        let err = verify_p2pool_coinbase_payout(&block, &miner_address, "test::p2pool")
            .expect_err("coinbase paying a different address must be rejected");
        assert!(err.contains(&miner_address.to_base58()));
        assert!(err.contains(&attacker_address.public_spend_key().to_hex()));

        let logs = captured_logs().lock().unwrap();
        assert!(
            logs.iter().any(|(level, msg)| {
                *level == Level::Error &&
                    msg.contains(&miner_address.to_base58()) &&
                    msg.contains(&attacker_address.public_spend_key().to_hex())
            }),
            "expected error log with both addresses, got: {logs:?}"
        );
    }

    #[test]
    fn rejects_block_when_header_output_mr_does_not_match_body() {
        init_test_logger();
        let miner_address = sample_address(30);
        let attacker_address = sample_address(40);
        let mut block = block_with_coinbase(&miner_address, MicroMinotari::from(50_000_000));
        let attacker_block = block_with_coinbase(&attacker_address, MicroMinotari::from(50_000_000));

        // Body pays the miner, but header commits to the attacker's outputs
        block.header = attacker_block.header;

        let err = verify_p2pool_coinbase_payout(&block, &miner_address, "test::p2pool")
            .expect_err("mismatched header block_output_mr must be rejected");
        assert!(err.contains("block_output_mr"));
    }

    #[test]
    fn rejects_block_when_header_block_output_mr_is_empty() {
        init_test_logger();
        let miner_address = sample_address(35);
        let mut block = block_with_coinbase(&miner_address, MicroMinotari::from(50_000_000));
        block.header.as_mut().unwrap().block_output_mr.clear();

        let err = verify_p2pool_coinbase_payout(&block, &miner_address, "test::p2pool")
            .expect_err("empty header block_output_mr must be rejected instead of falling back to output_mr");
        assert!(err.contains("block_output_mr is empty"));
    }

    #[test]
    fn accepts_single_and_interactive_only_addresses_with_valid_spend_key() {
        init_test_logger();
        let dual_recipient = sample_address(400);
        let single_address = TariAddress::new_single_address(
            dual_recipient.public_spend_key().clone(),
            Network::LocalNet,
            TariAddressFeatures::create_interactive_only(),
        )
        .unwrap();
        let reward = MicroMinotari::from(25_000_000);
        let block = block_with_coinbase(&dual_recipient, reward);

        let paid = verify_p2pool_coinbase_payout(&block, &single_address, "test::p2pool")
            .expect("single address with a valid spend key must be accepted on p2pool");
        assert_eq!(paid.revealed_amount, reward);
        assert_eq!(paid.revealed_outputs, 1);
    }
}
