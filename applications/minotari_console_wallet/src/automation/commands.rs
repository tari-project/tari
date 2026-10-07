// Copyright 2020. The Tari Project
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
use std::{
    cmp::{max, min},
    collections::HashMap,
    convert::TryInto,
    fs::{self, File},
    io::{self, BufRead, BufReader, LineWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use dialoguer::Input as InputPrompt;
use digest::Digest;
use log::*;
use minotari_app_grpc::tls::certs::{generate_self_signed_certs, print_warning, write_cert_to_disk};
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use minotari_node_wallet_client::BaseNodeWalletClient;
use minotari_wallet::{
    TransactionStage,
    WalletConfig,
    WalletKeyManager,
    WalletSqlite,
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        UtxoSelectionCriteria,
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::UseOutput,
    },
    transaction_service::{
        handle::{TransactionEvent, TransactionServiceHandle},
        storage::models::{CompletedTransaction, WalletTransaction},
    },
    utxo_scanner_service::handle::UtxoScannerEvent,
};
use rand::{RngExt, distr::Alphanumeric};
use serde::Serialize;
use sha2::Sha256;
use tari_common::configuration::Network;
use tari_common_types::{
    emoji::EmojiId,
    epoch::VnEpoch,
    seeds::{cipher_seed::CipherSeed, seed_words::SeedWords},
    tari_address::TariAddress,
    transaction::{LegacyTransactionStatus, TxId},
    types::{
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        FixedHash,
        HashOutput,
        PrivateKey,
        UncompressedSignature,
    },
};
use tari_core::blocks::pre_mine::{get_embedded_pre_mine_json, get_pre_mine_items};
use tari_crypto::ristretto::RistrettoSecretKey;
use tari_p2p::{PeerSeedsConfig, auto_update::AutoUpdateConfig};
use tari_script::{CompressedCheckSigSchnorrSignature, push_pubkey_script};
use tari_shutdown::Shutdown;
use tari_transaction_components::{
    key_manager::{TariKeyId, TransactionKeyManagerInterface, error::KeyManagerError, wallet_types::WalletType},
    multisig::script::is_multisig_utxo,
    offline_signing::models::{
        PrepareDepositMultisigTransactionResult,
        PrepareOneSidedTransactionForSigningResult,
        PrepareWithdrawMultisigTransactionResult,
        SignedOneSidedTransactionResult,
        TransactionResult,
    },
    rpc::models::TxLocation,
    tari_amount::{MicroMinotari, Minotari},
    transaction_components::{
        EncryptedData,
        OutputFeatures,
        Transaction,
        TransactionInput,
        TransactionInputVersion,
        TransactionKernel,
        TransactionOutput,
        TransactionOutputVersion,
        UnblindedOutput,
        WalletOutput,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        one_sided::public_key_to_output_encryption_key,
    },
};
use tari_transaction_key_manager::legacy_key_manager::wallet_types::LegacyWalletType;
use tari_utilities::{ByteArray, SafePassword, encoding::MBase58, hex::Hex};
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};

use super::error::CommandError;
use crate::{
    automation::{
        PreMineSpendStep1SessionInfo,
        PreMineSpendStep2OutputsForLeader,
        PreMineSpendStep2OutputsForSelf,
        PreMineSpendStep3OutputsForParties,
        PreMineSpendStep3OutputsForSelf,
        PreMineSpendStep4OutputsForLeader,
        RecipientInfo,
        Step2OutputsForLeader,
        Step2OutputsForSelf,
        Step3OutputsForParties,
        Step3OutputsForSelf,
        Step4OutputsForLeader,
        utils::{
            create_pre_mine_output_dir,
            get_file_name,
            move_session_file_to_session_dir,
            out_dir,
            read_and_verify,
            read_session_info,
            read_verify_session_info,
            validate_session_id,
            write_json_object_to_file_as_line,
            write_to_json_file,
        },
    },
    cli::{CliCommands, CliRecipientInfo, MakeItRainTransactionType},
    init::init_wallet,
    recovery::{get_seed_from_seed_words, wallet_recovery},
};

pub const LOG_TARGET: &str = "wallet::automation::commands";
/// How long `import-paper-wallet` waits for its temporary wallet's tasks to exit before deleting the wallet directory.
const TEMP_WALLET_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
// Pre-mine file names
pub(crate) const FILE_EXTENSION: &str = "json";
pub(crate) const SPEND_SESSION_INFO: &str = "step_1_session_info";
pub(crate) const SPEND_STEP_2_LEADER: &str = "step_2_for_leader_from_";
pub(crate) const SPEND_STEP_2_SELF: &str = "step_2_for_self";
pub(crate) const SPEND_STEP_3_SELF: &str = "step_3_for_self";
pub(crate) const SPEND_STEP_3_PARTIES: &str = "step_3_for_parties";
pub(crate) const SPEND_STEP_4_LEADER: &str = "step_4_for_leader_from_";
/// Step 4's record of the outputs it has already signed, so that a run stopped by a full device nonce record can be
/// resumed without signing any output twice. See `load_step_4_progress`.
pub(crate) const SPEND_STEP_4_PROGRESS: &str = "step_4_progress";

#[derive(Debug)]
pub struct SentTransaction {}

/// encumbers a n-of-m transaction
#[allow(clippy::too_many_arguments)]
#[allow(clippy::mutable_key_type)]
async fn encumber_aggregate_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: MicroMinotari,
    expected_commitment: CompressedCommitment,
    script_input_shares: HashMap<CompressedPublicKey, CompressedCheckSigSchnorrSignature>,
    script_signature_public_nonces: Vec<CompressedPublicKey>,
    sender_offset_public_key_shares: Vec<CompressedPublicKey>,
    metadata_ephemeral_public_key_shares: Vec<CompressedPublicKey>,
    dh_shared_secret_shares: Vec<CompressedPublicKey>,
    recipient_address: TariAddress,
    original_maturity: u64,
    use_output: UseOutput,
    payment_id: MemoField,
) -> Result<
    (
        TxId,
        Transaction,
        CompressedPublicKey,
        CompressedPublicKey,
        CompressedPublicKey,
        CompressedPublicKey,
    ),
    CommandError,
> {
    println!("Getting connection to BaseNode and retrieving output(s)...");
    wallet_transaction_service
        .encumber_aggregate_utxo(
            fee_per_gram,
            expected_commitment,
            script_input_shares,
            script_signature_public_nonces,
            sender_offset_public_key_shares,
            metadata_ephemeral_public_key_shares,
            dh_shared_secret_shares,
            recipient_address,
            original_maturity,
            use_output,
            payment_id,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

async fn spend_backup_pre_mine_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: MicroMinotari,
    output_hash: HashOutput,
    expected_commitment: CompressedCommitment,
    recipient_address: TariAddress,
    payment_id: MemoField,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .spend_backup_pre_mine_utxo(
            fee_per_gram,
            output_hash,
            expected_commitment,
            recipient_address,
            payment_id,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// finalises an already encumbered a n-of-m transaction
// Schnorr signature / Ristretto scalar arithmetic, not integer arithmetic: cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
async fn finalise_aggregate_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    tx_id: u64,
    meta_signatures: Vec<CompressedSignature>,
    script_signatures: Vec<CompressedSignature>,
    wallet_script_secret_key: PrivateKey,
) -> Result<TxId, CommandError> {
    trace!(target: LOG_TARGET, "finalise_aggregate_utxo: start");

    let mut meta_sig = UncompressedSignature::default();
    for sig in &meta_signatures {
        meta_sig = &meta_sig + sig.to_schnorr_signature()?;
    }
    let mut script_sig = UncompressedSignature::default();
    for sig in &script_signatures {
        script_sig = &script_sig + sig.to_schnorr_signature()?;
    }
    trace!(target: LOG_TARGET, "finalise_aggregate_utxo: aggregated signatures");

    wallet_transaction_service
        .finalize_aggregate_utxo(
            tx_id,
            CompressedSignature::new_from_schnorr(meta_sig),
            CompressedSignature::new_from_schnorr(script_sig),
            wallet_script_secret_key,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// publishes a tari-SHA atomic swap HTLC transaction
pub async fn init_sha_atomic_swap(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroMinotari,
    selection_criteria: UtxoSelectionCriteria,
    dest_address: TariAddress,
    payment_id: MemoField,
) -> Result<(TxId, CompressedPublicKey, TransactionOutput), CommandError> {
    let (tx_id, pre_image, output) = wallet_transaction_service
        .send_sha_atomic_swap_transaction(
            dest_address,
            amount,
            selection_criteria,
            MicroMinotari::from(fee_per_gram),
            payment_id,
        )
        .await
        .map_err(CommandError::TransactionServiceError)?;
    Ok((tx_id, pre_image, output))
}

/// claims a tari-SHA atomic swap HTLC transaction
pub async fn finalise_sha_atomic_swap(
    mut output_service: OutputManagerHandle<WalletKeyManager>,
    mut transaction_service: TransactionServiceHandle,
    output_hash: FixedHash,
    pre_image: CompressedPublicKey,
    fee_per_gram: MicroMinotari,
    payment_id: MemoField,
) -> Result<TxId, CommandError> {
    let (tx_id, _fee, amount, tx) = output_service
        .create_claim_sha_atomic_swap_transaction(output_hash, pre_image, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, payment_id)
        .await?;
    Ok(tx_id)
}

/// claims a HTLC refund transaction
pub async fn claim_htlc_refund(
    mut output_service: OutputManagerHandle<WalletKeyManager>,
    mut transaction_service: TransactionServiceHandle,
    output_hash: FixedHash,
    fee_per_gram: MicroMinotari,
    payment_id: MemoField,
) -> Result<TxId, CommandError> {
    let (tx_id, _fee, amount, tx) = output_service
        .create_htlc_refund_transaction(output_hash, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, payment_id)
        .await?;
    Ok(tx_id)
}

pub async fn register_validator_node(
    amount: MicroMinotari,
    mut wallet_transaction_service: TransactionServiceHandle,
    validator_node_public_key: CompressedPublicKey,
    validator_node_signature: CompressedSignature,
    validator_node_claim_public_key: CompressedPublicKey,
    sidechain_deployment_key: Option<PrivateKey>,
    epoch: VnEpoch,
    selection_criteria: UtxoSelectionCriteria,
    fee_per_gram: MicroMinotari,
    payment_id: MemoField,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .register_validator_node(
            amount,
            validator_node_public_key,
            validator_node_signature,
            validator_node_claim_public_key,
            sidechain_deployment_key,
            epoch,
            selection_criteria,
            fee_per_gram,
            payment_id,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn send_one_sided_to_stealth_address(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroMinotari,
    selection_criteria: UtxoSelectionCriteria,
    dest_address: TariAddress,
    payment_id: MemoField,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_one_sided_to_stealth_address_transaction(
            dest_address,
            amount,
            selection_criteria,
            OutputFeatures::default(),
            MicroMinotari::from(fee_per_gram),
            payment_id,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn coin_split(
    amount_per_split: MicroMinotari,
    num_splits: usize,
    fee_per_gram: MicroMinotari,
    payment_id: MemoField,
    output_service: &mut OutputManagerHandle<WalletKeyManager>,
    transaction_service: &mut TransactionServiceHandle,
) -> Result<TxId, CommandError> {
    let (tx_id, tx, amount) = output_service
        .create_coin_split(vec![], amount_per_split, num_splits, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, payment_id)
        .await?;

    Ok(tx_id)
}

// casting here is okay. If the txns per second for this primary debug tool is a bit off its okay.
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::too_many_lines)]
pub async fn make_it_rain(
    wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    transactions_per_second: f64,
    duration: Duration,
    start_amount: MicroMinotari,
    increase_amount: MicroMinotari,
    start_time: DateTime<Utc>,
    destination: TariAddress,
    transaction_type: MakeItRainTransactionType,
    payment_id: MemoField,
) -> Result<(), CommandError> {
    // Limit the transactions per second to a reasonable range
    // Notes:
    // - The 'transactions_per_second' is best effort and not guaranteed.
    // - If a slower rate is requested as what is achievable, transactions will be delayed to match the rate.
    // - If a faster rate is requested as what is achievable, the maximum rate will be that of the integrated system.
    // - The default value of 25/s may not be achievable.
    let transactions_per_second = transactions_per_second.abs().clamp(0.01, 250.0);
    // We are spawning this command in parallel, thus not collecting transaction IDs
    tokio::task::spawn(async move {
        // Wait until specified test start time
        let now = Utc::now();
        let delay_ms = if start_time > now {
            println!(
                "`make-it-rain` scheduled to start at {}: payment_id \"{}\"",
                start_time,
                payment_id.payment_id_as_string()
            );
            start_time.signed_duration_since(now).num_milliseconds() as u64
        } else {
            0
        };

        debug!(
            target: LOG_TARGET,
            "make-it-rain delaying for {delay_ms:?} ms - scheduled to start at {start_time}"
        );
        sleep(Duration::from_millis(delay_ms)).await;

        let num_txs = (transactions_per_second * duration.as_secs() as f64) as usize;
        let started_at = Utc::now();

        struct TransactionSendStats {
            i: usize,
            tx_id: Result<TxId, CommandError>,
            delayed_for: Duration,
            submit_time: Duration,
        }
        println!(
            "\n`make-it-rain` starting {} {} transactions \"{}\"\n",
            num_txs,
            transaction_type,
            payment_id.payment_id_as_string()
        );
        let payment_id_clone = payment_id.clone();
        let (sender, mut receiver) = mpsc::channel(num_txs);
        {
            let sender = sender;
            for i in 0..num_txs {
                debug!(
                    target: LOG_TARGET,
                    "make-it-rain starting {} of {} {} transactions",
                    i.saturating_add(1),
                    num_txs,
                    transaction_type
                );
                let loop_started_at = Instant::now();
                let tx_service = wallet_transaction_service.clone();
                // Transaction details
                let amount = start_amount.saturating_add(increase_amount.saturating_mul(MicroMinotari::from(i as u64)));

                // Manage transaction submission rate
                let actual_ms = Utc::now().signed_duration_since(started_at).num_milliseconds();
                let target_ms = (i as f64 * (1000.0 / transactions_per_second)) as i64;
                trace!(
                    target: LOG_TARGET,
                    "make-it-rain {i}: target {target_ms:?} ms vs. actual {actual_ms:?} ms"
                );
                let remaining_ms = target_ms.saturating_sub(actual_ms);
                if remaining_ms > 0 {
                    // Maximum delay between Txs set to 120 s
                    let delay_ms = Duration::from_millis(remaining_ms.min(120_000i64) as u64);
                    trace!(
                        target: LOG_TARGET,
                        "make-it-rain {i}: delaying for {delay_ms:?} ms"
                    );
                    sleep(delay_ms).await;
                }
                let delayed_for = Instant::now();
                let sender_clone = sender.clone();
                let fee = fee_per_gram;
                let address = destination.clone();
                let payment_id_clone = payment_id.clone();
                tokio::task::spawn(async move {
                    let spawn_start = Instant::now();
                    // Send transaction
                    let tx_id = match transaction_type {
                        MakeItRainTransactionType::StealthOneSided => {
                            send_one_sided_to_stealth_address(
                                tx_service,
                                fee,
                                amount,
                                UtxoSelectionCriteria::default(),
                                address.clone(),
                                payment_id_clone,
                            )
                            .await
                        },
                    };
                    let submit_time = Instant::now();

                    if let Err(e) = sender_clone
                        .send(TransactionSendStats {
                            i: i.saturating_add(1),
                            tx_id,
                            delayed_for: delayed_for.duration_since(loop_started_at),
                            submit_time: submit_time.duration_since(spawn_start),
                        })
                        .await
                    {
                        warn!(
                            target: LOG_TARGET,
                            "make-it-rain: Error sending transaction send stats to channel: {e}"
                        );
                    }
                });
            }
        }
        while let Some(send_stats) = receiver.recv().await {
            match send_stats.tx_id {
                Ok(tx_id) => {
                    print!("{} ", send_stats.i);
                    io::stdout().flush().unwrap();
                    debug!(
                        target: LOG_TARGET,
                        "make-it-rain transaction {} ({}) submitted to queue, tx_id: {}, delayed for ({}ms), submit \
                         time ({}ms)",
                        send_stats.i,
                        transaction_type,
                        tx_id,
                        send_stats.delayed_for.as_millis(),
                        send_stats.submit_time.as_millis()
                    );
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "make-it-rain transaction {} ({}) error: {}",
                        send_stats.i,
                        transaction_type,
                        e,
                    );
                },
            }
        }
        debug!(
            target: LOG_TARGET,
            "make-it-rain concluded {num_txs} {transaction_type} transactions"
        );
        println!(
            "\n`make-it-rain` concluded {} {} transactions (\"{}\") at {}",
            num_txs,
            transaction_type,
            payment_id_clone.payment_id_as_string(),
            Utc::now(),
        );
    });

    Ok(())
}

pub async fn monitor_transactions(
    transaction_service: TransactionServiceHandle,
    tx_ids: Vec<TxId>,
    wait_stage: TransactionStage,
) -> Vec<SentTransaction> {
    let mut event_stream = transaction_service.get_event_stream();
    let mut results = Vec::new();
    debug!(target: LOG_TARGET, "monitor transactions wait_stage: {wait_stage:?}");
    println!(
        "Monitoring {} sent transactions to {:?} stage...",
        tx_ids.len(),
        wait_stage
    );

    loop {
        match event_stream.recv().await {
            Ok(event) => match &*event {
                TransactionEvent::TransactionSendResult(id, status) if tx_ids.contains(id) => {
                    debug!(target: LOG_TARGET, "tx send event for tx_id: {id}, {status}");
                    if wait_stage == TransactionStage::DirectSendOrSaf &&
                        (status.direct_send_result || status.store_and_forward_send_result)
                    {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                TransactionEvent::ReceivedTransactionReply(id) if tx_ids.contains(id) => {
                    debug!(target: LOG_TARGET, "tx reply event for tx_id: {id}");
                    if wait_stage == TransactionStage::Negotiated {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                TransactionEvent::TransactionBroadcast(id) if tx_ids.contains(id) => {
                    debug!(target: LOG_TARGET, "tx mempool broadcast event for tx_id: {id}");
                    if wait_stage == TransactionStage::Broadcast {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                TransactionEvent::TransactionMinedUnconfirmed {
                    tx_id,
                    num_confirmations,
                    is_valid,
                } if tx_ids.contains(tx_id) => {
                    debug!(
                        target: LOG_TARGET,
                        "tx mined unconfirmed event for tx_id: {tx_id}, confirmations: {num_confirmations}, is_valid: {is_valid}"
                    );
                    if wait_stage == TransactionStage::MinedUnconfirmed {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                TransactionEvent::TransactionMined { tx_id, is_valid } if tx_ids.contains(tx_id) => {
                    debug!(
                        target: LOG_TARGET,
                        "tx mined confirmed event for tx_id: {tx_id}, is_valid:{is_valid}"
                    );
                    if wait_stage == TransactionStage::Mined {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                _ => {},
            },
            // All event senders have gone (i.e. we take it that the node is shutting down)
            Err(broadcast::error::RecvError::Closed) => {
                debug!(
                    target: LOG_TARGET,
                    "All Transaction event senders have gone. Exiting `monitor_transactions` loop."
                );
                break;
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "monitor_transactions: {err}");
            },
        }
    }

    results
}

#[allow(clippy::too_many_lines)]
// Schnorr signature / Ristretto scalar arithmetic, not integer arithmetic: cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
pub async fn command_runner(
    config: &WalletConfig,
    commands: Vec<CliCommands>,
    wallet: WalletSqlite,
) -> Result<bool, CommandError> {
    let wait_stage = config.command_send_wait_stage;

    let mut transaction_service = wallet.transaction_service.clone();
    let mut output_service = wallet.output_manager_service.clone();
    let key_manager_service = wallet.key_manager_service.clone();

    let mut tx_ids = Vec::new();

    println!("==============");
    println!("Command Runner");
    println!("==============");

    let mut unban_peer_manager_peers = false;

    #[allow(clippy::enum_glob_use)]
    for (idx, parsed) in commands.into_iter().enumerate() {
        println!("\n{}. {:?}\n", idx.saturating_add(1), parsed);
        use crate::cli::CliCommands::*;
        match parsed {
            GetBalance => match output_service.clone().get_balance().await {
                Ok(balance) => {
                    debug!(target: LOG_TARGET, "get-balance concluded");
                    println!("{balance}");
                },
                Err(e) => eprintln!("GetBalance error! {e}"),
            },
            PreMineSpendGetOutputStatus => {
                let pre_mine_outputs = get_all_embedded_pre_mine_outputs()?;
                let output_hashes: Vec<HashOutput> = pre_mine_outputs.iter().map(|v| v.hash()).collect();
                let unspent_outputs = transaction_service.fetch_unspent_outputs(output_hashes).await?;

                let pre_mine_items = match get_pre_mine_items(Network::get_current_or_user_setting_or_default()) {
                    Ok(items) => items,
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        return Ok(false);
                    },
                };

                let (session_id, out_dir) = match create_pre_mine_output_dir(Some("pre_mine_status")) {
                    Ok(values) => values,
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        return Ok(false);
                    },
                };
                let csv_file_name = "pre_mine_items_with_status.csv";
                let csv_out_file = out_dir.join(csv_file_name);
                let mut file_stream =
                    File::create(&csv_out_file).expect("Could not create 'pre_mine_items_with_status.csv'");
                if let Err(e) =
                    file_stream.write_all("index,value,maturity,fail_safe_height,beneficiary,spent_status\n".as_bytes())
                {
                    eprintln!("\nError: Could not write pre-mine header ({e})\n");
                    return Ok(false);
                }

                for (index, item) in pre_mine_items.iter().enumerate() {
                    let unspent = unspent_outputs
                        .iter()
                        .any(|u| u.commitment() == &pre_mine_outputs.get(index).expect("Already checked").commitment);
                    if let Err(e) = file_stream.write_all(
                        format!(
                            "{},{},{},{},{},{},{}\n",
                            index,
                            item.value,
                            item.maturity,
                            item.original_maturity,
                            item.fail_safe_height,
                            item.beneficiary,
                            if unspent { "unspent" } else { "spent" },
                        )
                        .as_bytes(),
                    ) {
                        eprintln!("\nError: Could not write pre-mine item ({e})\n");
                        return Ok(false);
                    }
                }

                println!();
                println!("Concluded step 0 'pre-mine-spend-get-output-status'");
                println!("Your session ID is:                    '{session_id}'");
                println!("Your session's output directory is:    '{}'", out_dir.display());
                println!("Pre-mine output spent status saved to: '{csv_file_name}'");
                println!();
            },
            PreMineStart(args) => {
                let args_recipient_info = sort_args_recipient_info(args.recipient_info);
                if let Err(e) = verify_no_duplicate_indexes(&args_recipient_info) {
                    eprintln!("\nError: {e} duplicate output indexes detected!\n");
                    break;
                }

                let mut recipient_info = Vec::new();
                let mut error = false;
                for item in args_recipient_info {
                    if args.verify_unspent_outputs && !args.use_pre_mine_input_file {
                        let embedded_outputs = match get_embedded_pre_mine_outputs(item.output_indexes.clone(), None) {
                            Ok(outputs) => outputs,
                            Err(e) => {
                                eprintln!("\nError: {e}\n");
                                error = true;
                                break;
                            },
                        };
                        let output_hashes = embedded_outputs.iter().map(|v| v.hash()).collect::<Vec<_>>();

                        let unspent_outputs = transaction_service.fetch_unspent_outputs(output_hashes.clone()).await?;
                        if unspent_outputs.len() != output_hashes.len() {
                            let unspent_output_hashes = unspent_outputs.iter().map(|v| v.hash()).collect::<Vec<_>>();
                            let missing = output_hashes
                                .iter()
                                .filter(|&v| !unspent_output_hashes.iter().any(|u| u == v))
                                .collect::<Vec<_>>();
                            eprintln!(
                                "\nError: Outputs with output_hashes '{:?}' has already been spent!\n",
                                missing.iter().map(|v| v.to_hex()).collect::<Vec<_>>(),
                            );
                            error = true;
                            break;
                        }
                    }

                    for index in item.output_indexes {
                        recipient_info.push(RecipientInfo {
                            output_to_be_spend: index,
                            recipient_address: item.recipient_address.clone(),
                        });
                    }
                }
                if error {
                    break;
                }

                let (session_id, out_dir) = match create_pre_mine_output_dir(None) {
                    Ok(values) => values,
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        return Ok(false);
                    },
                };
                let session_info = PreMineSpendStep1SessionInfo {
                    session_id: session_id.clone(),
                    fee_per_gram: args.fee_per_gram,
                    recipient_info,
                    use_pre_mine_input_file: args.use_pre_mine_input_file,
                };

                warn_if_step_4_needs_app_restarts(session_info.recipient_info.len());
                let out_file = out_dir.join(get_file_name(SPEND_SESSION_INFO, None));
                write_to_json_file(&out_file, true, session_info)?;
                println!();
                println!("Concluded step 1 'pre-mine-spend-session-info'");
                println!("Your session ID is:                 '{session_id}'");
                println!("Your session's output directory is: '{}'", out_dir.display());
                println!("Session info saved to:              '{}'", out_file.display());
                println!(
                    "Send '{}' to parties for step 2",
                    get_file_name(SPEND_SESSION_INFO, None)
                );
                println!();
            },
            PreMineSpendBackupUtxo(args) => {
                match *key_manager_service.get_wallet_type().await {
                    WalletType::Ledger(_) => {},
                    _ => {
                        eprintln!("\nError: Wallet type must be 'Ledger' to spend pre-mine outputs!\n");
                        break;
                    },
                }

                let embedded_output = match get_embedded_pre_mine_outputs(vec![args.output_index], None) {
                    Ok(outputs) => outputs.first().expect("Already checked").clone(),
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        break;
                    },
                };
                let commitment = embedded_output.commitment.clone();
                let output_hash = embedded_output.hash();
                let memo = match MemoField::new_open_from_string(
                    &args.payment_id,
                    detect_tx_metadata(&wallet, &args.recipient_address).await,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("\nError: Could not create memo field from payment id string: {e}\n");
                        break;
                    },
                };

                match spend_backup_pre_mine_utxo(
                    transaction_service.clone(),
                    args.fee_per_gram,
                    output_hash,
                    commitment.clone(),
                    args.recipient_address,
                    memo,
                )
                .await
                {
                    Ok(tx_id) => {
                        println!();
                        println!("Concluded 'pre-mine-spend-backup-utxo'");
                        println!("Spend utxo: {} with tx_id: {}", commitment.to_hex(), tx_id);
                        println!();
                    },
                    Err(e) => {
                        eprintln!("\nError: Spent pre-mine transaction error! {e}\n");
                        break;
                    },
                }
            },
            PreMineStartParty(args) => {
                let mut alias = args.alias.clone();
                loop {
                    if alias.is_empty() || alias.contains(" ") {
                        eprintln!("\nError: Alias cannot contain spaces!\n");
                        alias = InputPrompt::<String>::new()
                            .with_prompt("Please enter an alias to use")
                            .interact()
                            .unwrap();
                        continue;
                    }
                    if alias.chars().any(|c| !c.is_alphanumeric() && c != '_') {
                        eprintln!(
                            "\nError: Alias contains invalid characters! Only alphanumeric and '_' are allowed.\n"
                        );
                        alias = InputPrompt::<String>::new()
                            .with_prompt("Please enter an alias to use")
                            .interact()
                            .unwrap();
                        continue;
                    }
                    break;
                }
                let mut input_file_path = args.input_file.clone();
                while input_file_path.is_none() {
                    eprintln!("\nError: Missing input file path!\n");
                    input_file_path = Some(
                        InputPrompt::<String>::new()
                            .with_prompt("Please enter the path to the input file")
                            .interact()
                            .unwrap(),
                    );
                }

                let file_path = PathBuf::from(input_file_path.unwrap());

                // Read session info
                let session_info = read_session_info::<PreMineSpendStep1SessionInfo>(file_path.clone())?;
                // Verify  session info
                // session_info.recipient_info
                // The session ID comes from a file supplied by the leader and is used as a directory name, so it must
                // be verified before any path is constructed with it
                if let Err(e) = validate_session_id(&session_info.session_id) {
                    eprintln!("\nError: {e}\n");
                    break;
                }

                let pre_mine_from_file =
                    match read_genesis_file_outputs(session_info.use_pre_mine_input_file, args.pre_mine_file_path) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {e}\n");
                            break;
                        },
                    };

                warn_if_step_4_needs_app_restarts(session_info.recipient_info.len());
                println!();
                let mut outputs_for_leader = Vec::with_capacity(session_info.recipient_info.len());
                let mut outputs_for_self = Vec::with_capacity(session_info.recipient_info.len());
                let mut error = false;
                for (i, recipient_info) in session_info.recipient_info.iter().enumerate() {
                    println!(
                        "  Start processing {} of {} transactions, current wallet {}",
                        i.saturating_add(1),
                        session_info.recipient_info.len(),
                        recipient_info.recipient_address
                    );
                    let output_index = recipient_info.output_to_be_spend;
                    let embedded_output =
                        match get_embedded_pre_mine_outputs(vec![output_index], pre_mine_from_file.clone()) {
                            Ok(outputs) => outputs.first().expect("Already checked").clone(),
                            Err(e) => {
                                eprintln!("\nError: {e}\n");
                                error = true;
                                break;
                            },
                        };
                    let commitment = embedded_output.commitment.clone();

                    // KNOWN, DELIBERATELY DEFERRED GAP: everywhere else these nonces would be reserved with
                    // `reserve_ephemeral_nonce`, which yields a handle to a nonce the device generated and can only
                    // spend once. These two cannot be, because they are created here in step 2 and signed with in
                    // step 3, with N of them outstanding across a file rather than a device session - and the
                    // device's nonce store is in RAM and holds eight. So they stay host indexed on the `Random`
                    // branch, and signing with them goes through `GetRawSchnorrSignatureLegacyNonce`.
                    //
                    // Two signatures under one of these nonces - or the script key and the sender offset key under
                    // one - give up this output's pre-mine keys. The device refuses a second use of a nonce index
                    // within one app run and shows every signature for approval; step 4 validates the self file,
                    // checks each signature's nonce against the public nonce stored here (which only catches edits
                    // that leave the stored public nonces alone - the file is unauthenticated), and never signs an
                    // output twice.
                    // What is still open (an app restart between two approvals, devices on 6.1.0 or earlier) is the
                    // "Residual" section of `minotari_ledger_wallet_common::legacy_nonce`; read that before touching
                    // either of these lines. `get_random_key` never returns a `Random` index below 2^32, which the
                    // device refuses.
                    let script_nonce_key = key_manager_service.get_random_key(None, Some(LedgerKeyBranch::Random))?;
                    let sender_offset_nonce =
                        key_manager_service.get_random_key(None, Some(LedgerKeyBranch::Random))?;

                    let pre_mine_script_key_id = TariKeyId::LedgerKey {
                        branch: LedgerKeyBranch::PreMine,
                        index: output_index as u64,
                    };
                    // The sender offset key has to be generated by the key manager - on a ledger wallet the device
                    // picks the index and never hands the key over - and it comes back with the script offset for
                    // this output, which step 4 hands to the leader. Because the only script key here is a pre-mine
                    // key, the device puts the sender offset key on the `PreMine` branch, which is what lets step 3
                    // sign the metadata signature with it through the legacy instruction.
                    let (script_offset, mut sender_offset_keys) =
                        key_manager_service.get_script_offset(std::slice::from_ref(&pre_mine_script_key_id), 1)?;
                    let sender_offset_key = match sender_offset_keys.pop() {
                        Some(key) => key,
                        None => {
                            eprintln!("\nError: No sender offset key returned for output {output_index}\n");
                            error = true;
                            break;
                        },
                    };
                    let shared_secret = key_manager_service.get_diffie_hellman_shared_secret(
                        &sender_offset_key.key_id,
                        recipient_info
                            .recipient_address
                            .public_view_key()
                            .ok_or(CommandError::InvalidArgument("Missing public view key".to_string()))?,
                    )?;
                    let shared_secret_public_key = CompressedPublicKey::from_canonical_bytes(shared_secret.as_bytes())?;
                    let pre_mine_public_script_key =
                        match key_manager_service.get_public_key_at_key_id(&pre_mine_script_key_id) {
                            Ok(key) => key,
                            Err(e) => {
                                eprintln!("\nError: Could not retrieve script key for output {output_index}: {e}\n");
                                error = true;
                                break;
                            },
                        };
                    let script_input_signature =
                        key_manager_service.sign_script_message(&pre_mine_script_key_id, commitment.as_bytes())?;

                    outputs_for_leader.push(Step2OutputsForLeader {
                        output_index,
                        recipient_address: recipient_info.recipient_address.clone(),
                        script_input_signature,
                        public_script_nonce_key: script_nonce_key.pub_key.clone(),
                        public_sender_offset_key: sender_offset_key.pub_key,
                        public_sender_offset_nonce_key: sender_offset_nonce.pub_key.clone(),
                        dh_shared_secret_public_key: shared_secret_public_key,
                        pre_mine_public_script_key,
                    });

                    outputs_for_self.push(Step2OutputsForSelf {
                        output_index,
                        recipient_address: recipient_info.recipient_address.clone(),
                        script_nonce_key_id: script_nonce_key.key_id,
                        sender_offset_key_id: sender_offset_key.key_id,
                        sender_offset_nonce_key_id: sender_offset_nonce.key_id,
                        pre_mine_script_key_id,
                        script_offset,
                        public_script_nonce_key: Some(script_nonce_key.pub_key),
                        public_sender_offset_nonce_key: Some(sender_offset_nonce.pub_key),
                    });
                    println!(
                        "    Processed {} of {} transactions",
                        i.saturating_add(1),
                        session_info.recipient_info.len()
                    );

                    if error {
                        break;
                    }
                }
                if error {
                    break;
                }
                // Defensive: `get_random_key` makes a repeat astronomically unlikely, but a self file that names one
                // nonce twice would have step 4 sign two keys under one nonce, which gives both keys up.
                if let Err(e) = step_2_nonce_ids_are_unique(&outputs_for_self) {
                    eprintln!("\nError: {e}\n");
                    break;
                }

                let out_dir = out_dir(&session_info.session_id)?;
                let out_file_leader = out_dir.join(get_file_name(SPEND_STEP_2_LEADER, Some(alias.clone())));
                write_json_object_to_file_as_line(&out_file_leader, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file_leader, false, PreMineSpendStep2OutputsForLeader {
                    outputs_for_leader,
                    alias: alias.clone(),
                })?;

                let out_file_self = out_dir.join(get_file_name(SPEND_STEP_2_SELF, None));
                write_json_object_to_file_as_line(&out_file_self, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file_self, false, PreMineSpendStep2OutputsForSelf {
                    outputs_for_self,
                    alias: alias.clone(),
                })?;

                println!();
                println!("Concluded step 2 'pre-mine-spend-party-details'");
                println!("Your session's output directory is '{}'", out_dir.display());
                move_session_file_to_session_dir(&session_info.session_id, &file_path)?;
                println!(
                    "Send '{}' to leader for step 3",
                    get_file_name(SPEND_STEP_2_LEADER, Some(alias))
                );
                println!();
            },
            PreMineEncumber(args) => {
                let session_info;
                // Read session info
                let mut session_id = args.session_id.clone();
                loop {
                    if session_id.is_empty() {
                        eprintln!("\nError: No session id present\n");
                        session_id = InputPrompt::<String>::new()
                            .with_prompt("Please enter a session id to use")
                            .interact()
                            .unwrap();
                        continue;
                    }
                    match read_verify_session_info::<PreMineSpendStep1SessionInfo>(&session_id) {
                        Ok(v) => session_info = v,
                        Err(_) => {
                            eprintln!("\nError: invalid session id\n");
                            session_id = InputPrompt::<String>::new()
                                .with_prompt("Please enter a session id to use")
                                .interact()
                                .unwrap();
                            continue;
                        },
                    }
                    break;
                }
                let session_info_indexed = session_info
                    .recipient_info
                    .iter()
                    .map(|v| (v.output_to_be_spend, v.recipient_address.clone()))
                    .collect::<Vec<_>>();

                // Read and verify party info
                let mut party_info = Vec::with_capacity(args.member.len());
                for name in args.member {
                    let file_name = get_file_name(SPEND_STEP_2_LEADER, Some(name.clone()));
                    println!("reading: {file_name}");
                    party_info.push(read_and_verify::<PreMineSpendStep2OutputsForLeader>(
                        &session_id,
                        &file_name,
                        &session_info,
                    )?);
                }
                let mut error = false;
                for party in &party_info {
                    let this_party_info = party
                        .outputs_for_leader
                        .iter()
                        .map(|v1| (v1.output_index, v1.recipient_address.clone()))
                        .collect::<Vec<_>>();

                    if session_info_indexed != this_party_info {
                        eprintln!(
                            "\nError: Mismatched recipient info from '{}', expected {:?} received {:?}!\n",
                            party.alias,
                            session_info_indexed
                                .iter()
                                .map(|(index, address)| (*index, address.to_hex().clone()))
                                .collect::<Vec<_>>(),
                            this_party_info
                                .iter()
                                .map(|(index, address)| (*index, address.to_hex().clone()))
                                .collect::<Vec<_>>(),
                        );
                        error = true;
                        break;
                    }
                }
                if error {
                    break;
                }

                // Flatten and transpose party_info to be indexed by output index
                let party_info_flattened = party_info
                    .iter()
                    .map(|v1| v1.outputs_for_leader.clone())
                    .collect::<Vec<_>>();
                let mut party_info_per_index =
                    Vec::with_capacity(party_info_flattened.first().expect("Already checked").len());
                for i in 0..party_info_flattened.first().expect("Already checked").len() {
                    let mut outputs_per_index = Vec::with_capacity(party_info_flattened.len());
                    for outputs in &party_info_flattened {
                        outputs_per_index.push(outputs.get(i).expect("Already checked").clone());
                    }
                    party_info_per_index.push(outputs_per_index);
                }

                let pre_mine_from_file =
                    match read_genesis_file_outputs(session_info.use_pre_mine_input_file, args.pre_mine_file_path) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {e}\n");
                            break;
                        },
                    };

                // Encumber outputs
                let mut outputs_for_parties = Vec::with_capacity(party_info_per_index.len());
                let mut outputs_for_self = Vec::with_capacity(party_info_per_index.len());
                let pre_mine_items = match get_pre_mine_items(Network::get_current_or_user_setting_or_default()) {
                    Ok(items) => items,
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        return Ok(true);
                    },
                };
                println!();
                for (i, indexed_info) in party_info_per_index.iter().enumerate() {
                    #[allow(clippy::mutable_key_type)]
                    let mut input_shares = HashMap::new();
                    let mut script_signature_public_nonces = Vec::with_capacity(indexed_info.len());
                    let mut sender_offset_public_key_shares = Vec::with_capacity(indexed_info.len());
                    let mut metadata_ephemeral_public_key_shares = Vec::with_capacity(indexed_info.len());
                    let mut dh_shared_secret_shares = Vec::with_capacity(indexed_info.len());
                    let current_index = indexed_info.first().expect("Already checked").output_index;
                    let current_recipient_address =
                        indexed_info.first().expect("Already checked").recipient_address.clone();
                    for item in indexed_info {
                        if current_index != item.output_index {
                            eprintln!(
                                "\nError: Mismatched output indexes detected! (expected {}, got {})\n",
                                current_index, item.output_index
                            );
                            error = true;
                            break;
                        }
                        if current_recipient_address != item.recipient_address {
                            eprintln!(
                                "\nError: Mismatched recipient addresses detected! (expected {}, got {})\n",
                                current_recipient_address, item.recipient_address
                            );
                            error = true;
                            break;
                        }
                        input_shares.insert(
                            item.pre_mine_public_script_key.clone(),
                            item.script_input_signature.clone(),
                        );
                        script_signature_public_nonces.push(item.public_script_nonce_key.clone());
                        sender_offset_public_key_shares.push(item.public_sender_offset_key.clone());
                        metadata_ephemeral_public_key_shares.push(item.public_sender_offset_nonce_key.clone());
                        dh_shared_secret_shares.push(item.dh_shared_secret_public_key.clone());
                    }
                    if error {
                        break;
                    }

                    let original_maturity = pre_mine_items
                        .get(current_index)
                        .expect("Already checked")
                        .original_maturity;
                    let embedded_output =
                        match get_embedded_pre_mine_outputs(vec![current_index], pre_mine_from_file.clone()) {
                            Ok(outputs) => outputs.first().expect("Already checked").clone(),
                            Err(e) => {
                                eprintln!("\nError: {e}\n");
                                error = true;
                                break;
                            },
                        };
                    let memo = match MemoField::new_open_from_string(
                        &args.payment_id,
                        detect_tx_metadata(&wallet, &current_recipient_address).await,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("\nError: Could not create memo field from payment id string: {e}\n");
                            break;
                        },
                    };

                    match encumber_aggregate_utxo(
                        transaction_service.clone(),
                        if session_info.use_pre_mine_input_file {
                            MicroMinotari::zero()
                        } else {
                            session_info.fee_per_gram
                        },
                        embedded_output.commitment.clone(),
                        input_shares,
                        script_signature_public_nonces,
                        sender_offset_public_key_shares,
                        metadata_ephemeral_public_key_shares,
                        dh_shared_secret_shares,
                        current_recipient_address,
                        original_maturity,
                        if pre_mine_from_file.is_some() {
                            UseOutput::AsProvided(Box::new(embedded_output))
                        } else {
                            UseOutput::FromBlockchain(embedded_output.hash())
                        },
                        memo,
                    )
                    .await
                    {
                        Ok((
                            tx_id,
                            transaction,
                            script_pubkey,
                            total_metadata_ephemeral_public_key,
                            total_script_nonce,
                            shared_secret,
                        )) => {
                            let input_0 = transaction.body.inputs().first().expect("Already checked");
                            let output_0 = transaction.body.outputs().first().expect("Already checked");
                            outputs_for_parties.push(Step3OutputsForParties {
                                output_index: current_index,
                                input_stack: input_0.input_data.clone(),
                                input_script: input_0.script().unwrap().clone(),
                                total_script_key: script_pubkey,
                                script_signature_ephemeral_commitment: input_0
                                    .script_signature
                                    .ephemeral_commitment()
                                    .clone(),
                                script_signature_ephemeral_pubkey: total_script_nonce,
                                output_commitment: transaction
                                    .body
                                    .outputs()
                                    .first()
                                    .expect("Already checked")
                                    .commitment()
                                    .clone(),
                                sender_offset_pubkey: transaction
                                    .body
                                    .outputs()
                                    .first()
                                    .expect("Already checked")
                                    .clone()
                                    .sender_offset_public_key,
                                metadata_signature_ephemeral_commitment: output_0
                                    .metadata_signature
                                    .ephemeral_commitment()
                                    .clone(),
                                metadata_signature_ephemeral_pubkey: total_metadata_ephemeral_public_key,
                                encrypted_data: output_0.encrypted_data.clone(),
                                output_features: output_0.features.clone(),
                                shared_secret,
                            });
                            outputs_for_self.push(Step3OutputsForSelf {
                                output_index: current_index,
                                tx_id,
                            });
                        },
                        Err(e) => {
                            eprintln!("\nError: Encumber aggregate transaction error! {e}\n");
                            error = true;
                            break;
                        },
                    }
                    println!(
                        "  Processed {} of {} transactions",
                        i.saturating_add(1),
                        party_info_per_index.len()
                    );
                }
                if error {
                    break;
                }

                let out_dir = out_dir(&session_id)?;
                let out_file = out_dir.join(get_file_name(SPEND_STEP_3_SELF, None));
                write_json_object_to_file_as_line(&out_file, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file, false, PreMineSpendStep3OutputsForSelf {
                    outputs_for_self,
                })?;

                let out_file = out_dir.join(get_file_name(SPEND_STEP_3_PARTIES, None));
                write_json_object_to_file_as_line(&out_file, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file, false, PreMineSpendStep3OutputsForParties {
                    outputs_for_parties,
                })?;

                println!();
                println!("Concluded step 3 'pre-mine-spend-encumber-aggregate-utxo'");
                println!(
                    "Send '{}' to parties for step 4",
                    get_file_name(SPEND_STEP_3_PARTIES, None)
                );
                println!();
            },
            ReplaceByFee(args) => {
                match transaction_service
                    .replace_by_fee(args.tx_id.into(), args.fee_increase)
                    .await
                {
                    Ok(tx_id) => {
                        debug!(target: LOG_TARGET, "replace-by-fee concluded with tx_id {tx_id}");
                        println!(
                            "Transaction {} replaced with higher fee, new tx_id: {}",
                            args.tx_id, tx_id
                        );
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("ReplaceByFee error! {e}"),
                }
            },
            UserPayForFee(args) => {
                match transaction_service
                    .user_pay_for_fee(args.tx_id.into(), args.destination, args.fee)
                    .await
                {
                    Ok(tx_id) => {
                        debug!(target: LOG_TARGET, "replace-by-fee concluded with tx_id {tx_id}");
                        println!(
                            "Transaction {} replaced with higher fee, new tx_id: {}",
                            args.tx_id, tx_id
                        );
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("ReplaceByFee error! {e}"),
                }
            },
            PreMineSigs(args) => {
                let session_info;
                // Read session info
                let mut session_id = args.session_id.clone();
                loop {
                    if session_id.is_empty() {
                        eprintln!("\nError: No session id present\n");
                        session_id = InputPrompt::<String>::new()
                            .with_prompt("Please enter a session id to use")
                            .interact()
                            .unwrap();
                        continue;
                    }
                    match read_verify_session_info::<PreMineSpendStep1SessionInfo>(&session_id) {
                        Ok(v) => session_info = v,
                        Err(_) => {
                            eprintln!("\nError: invalid session id\n");
                            session_id = InputPrompt::<String>::new()
                                .with_prompt("Please enter a session id to use")
                                .interact()
                                .unwrap();
                            continue;
                        },
                    }
                    break;
                }
                // Read leader input
                let leader_info_indexed = read_and_verify::<PreMineSpendStep3OutputsForParties>(
                    &session_id,
                    &get_file_name(SPEND_STEP_3_PARTIES, None),
                    &session_info,
                )?;
                // Read own party info
                let party_info_indexed = read_and_verify::<PreMineSpendStep2OutputsForSelf>(
                    &session_id,
                    &get_file_name(SPEND_STEP_2_SELF, None),
                    &session_info,
                )?;

                // Verify index consistency
                let session_info_indexes = session_info
                    .recipient_info
                    .iter()
                    .map(|v| v.output_to_be_spend)
                    .collect::<Vec<_>>();
                let leader_info_indexes = leader_info_indexed
                    .outputs_for_parties
                    .iter()
                    .map(|v| v.output_index)
                    .collect::<Vec<_>>();
                let party_info_indexes = party_info_indexed
                    .outputs_for_self
                    .iter()
                    .map(|v| v.output_index)
                    .collect::<Vec<_>>();
                if session_info_indexes != leader_info_indexes || session_info_indexes != party_info_indexes {
                    eprintln!(
                        "\nError: Mismatched output indexes detected! session {session_info_indexes:?} vs. leader \
                         {leader_info_indexes:?} vs. self {party_info_indexes:?}\n"
                    );
                    break;
                }

                // Every signature below uses a host indexed nonce through `GetRawSchnorrSignatureLegacyNonce`, so the
                // self file decides which nonce signs for which key. Check it is the shape step 2 writes before
                // anything is signed. See `minotari_ledger_wallet_common::legacy_nonce` for what a tampered file
                // could otherwise extract.
                if let Err(e) = validate_step_2_self_outputs(&party_info_indexed.outputs_for_self) {
                    eprintln!("\nError: {e}\n");
                    break;
                }

                // Never sign step 4 twice. Signing again over a changed step 3 file (a "please redo step 4" from the
                // leader) would put two signatures under each step 2 nonce, from which the leader could solve for this
                // party's pre-mine script and sender offset keys.
                let out_dir = out_dir(&session_id)?;
                let out_file = out_dir.join(get_file_name(
                    SPEND_STEP_4_LEADER,
                    Some(party_info_indexed.alias.clone()),
                ));
                if out_file.exists() {
                    eprintln!(
                        "\nError: step 4 has already been signed for this session ('{}' exists). Signing again under \
                         the same step 2 nonces would let the leader recover your pre-mine keys, so it is refused. \
                         Re-send the existing file to the leader instead; if the session has to change, start a new \
                         session from step 1.\n",
                        out_file.display()
                    );
                    break;
                }

                // Outputs a previous run of this step already signed - stopped, for instance, by a full device nonce
                // record. They are skipped below *even if the leader's step 3 file has changed since*: signing an
                // output a second time, over a different challenge, would put two signatures under its
                // step 2 nonces, which gives up this party's pre-mine keys. The progress file can only
                // ever prevent a signature.
                let progress_file = out_dir.join(get_file_name(SPEND_STEP_4_PROGRESS, None));
                let mut signed = match load_step_4_progress(&progress_file, &session_info, &party_info_indexes) {
                    Ok(signed) => signed,
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        break;
                    },
                };
                if !signed.is_empty() {
                    println!(
                        "Resuming step 4: {} of {} outputs were already signed and will not be signed again.",
                        signed.len(),
                        party_info_indexes.len()
                    );
                }

                let pre_mine_from_file =
                    match read_genesis_file_outputs(session_info.use_pre_mine_input_file, args.pre_mine_file_path) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {e}\n");
                            break;
                        },
                    };

                println!();
                let mut error = false;
                for (i, (leader_info, party_info)) in leader_info_indexed
                    .outputs_for_parties
                    .iter()
                    .zip(party_info_indexed.outputs_for_self.iter())
                    .enumerate()
                {
                    if signed.iter().any(|done| done.output_index == party_info.output_index) {
                        println!(
                            "  Output {} was signed by a previous run; skipping ({} of {})",
                            party_info.output_index,
                            i.saturating_add(1),
                            leader_info_indexed.outputs_for_parties.len()
                        );
                        continue;
                    }
                    let embedded_output = match get_embedded_pre_mine_outputs(
                        vec![party_info.output_index],
                        pre_mine_from_file.clone(),
                    ) {
                        Ok(outputs) => outputs.first().expect("Already checked").clone(),
                        Err(e) => {
                            eprintln!("\nError: {e}\n");
                            error = true;
                            break;
                        },
                    };

                    // Script signature
                    let challenge = TransactionInput::build_script_signature_challenge(
                        TransactionInputVersion::get_current_version(),
                        &leader_info.script_signature_ephemeral_commitment,
                        &leader_info.script_signature_ephemeral_pubkey,
                        &leader_info.input_script,
                        &leader_info.input_stack,
                        &leader_info.total_script_key,
                        &embedded_output.commitment,
                    );

                    // KNOWN, DELIBERATELY DEFERRED GAP: `script_nonce_key_id` is a host indexed nonce reserved
                    // back in step 2, so this routes to `GetRawSchnorrSignatureLegacyNonce`. The self file was
                    // validated above, and the nonce is compared below with the public nonce stored in the same
                    // (unauthenticated) file, which only catches edits that left that stored nonce alone. What blocks
                    // a redirect to a used nonce is the device, which refuses to reuse a nonce index for anything but
                    // this exact request until it restarts. See the "Residual" section of
                    // `minotari_ledger_wallet_common::legacy_nonce` for what is still open.
                    let script_signature = match key_manager_service.sign_with_nonce_and_challenge(
                        &party_info.pre_mine_script_key_id,
                        &party_info.script_nonce_key_id,
                        &challenge,
                    ) {
                        Ok(signature) => signature,
                        Err(e) => {
                            if is_legacy_nonce_store_full(&e) {
                                eprintln!("\n{STEP_4_STORE_FULL_MESSAGE}\n");
                            } else {
                                eprintln!("\nError: Script signature SignMessage error! {e}\n");
                            }
                            error = true;
                            break;
                        },
                    };
                    if Some(script_signature.get_compressed_public_nonce()) !=
                        party_info.public_script_nonce_key.as_ref()
                    {
                        eprintln!(
                            "\nError: the script signature for output {} was made with a nonce that is not the one \
                             step 2 published - the step 2 self file, or the device, has changed. The signature has \
                             already been made on the device but nothing has been written or sent; do not send \
                             anything from this session to the leader.\n",
                            party_info.output_index
                        );
                        error = true;
                        break;
                    }

                    // lets verify the script
                    let shared_secret =
                        match CompressedPublicKey::from_canonical_bytes(leader_info.shared_secret.as_bytes()) {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("\nError: Could not create public key from canonical bytes! {e}\n");
                                error = true;
                                break;
                            },
                        };

                    let encryption_key = public_key_to_output_encryption_key(&shared_secret)?;
                    let (committed_value, commitment_mask_private_key, _payment_id) = match EncryptedData::decrypt_data(
                        &encryption_key,
                        &leader_info.output_commitment,
                        &leader_info.encrypted_data,
                    ) {
                        Ok((value, mask, id)) => (value, mask, id),
                        Err(e) => {
                            eprintln!("\nError: Could not decrypt data! {e}\n");
                            error = true;
                            break;
                        },
                    };
                    let commitment_mask_key_id =
                        &key_manager_service.create_encrypted_key(commitment_mask_private_key.clone(), None)?;
                    match key_manager_service.verify_mask(
                        &leader_info.output_commitment,
                        commitment_mask_key_id,
                        committed_value.as_u64(),
                    ) {
                        Ok(()) => {},
                        Err(KeyManagerError::InvalidMask) => {
                            eprintln!("\nError: Could not verify mask! Commitment does not match value and mask\n");
                            error = true;
                            break;
                        },
                        Err(e) => {
                            eprintln!("\nError: Could not verify mask! {e}\n");
                            error = true;
                            break;
                        },
                    }
                    // now lets calculate the script with stealth key
                    let script_spending_key = key_manager_service.stealth_address_script_spending_key(
                        commitment_mask_key_id,
                        party_info.recipient_address.public_spend_key(),
                    )?;
                    let script = push_pubkey_script(&script_spending_key);

                    // Metadata signature. The script offset was computed in step 2, when the key manager generated
                    // the sender offset key.
                    let script_offset = party_info.script_offset.clone();
                    let challenge = TransactionOutput::build_metadata_signature_challenge(
                        TransactionOutputVersion::get_current_version(),
                        &script,
                        &leader_info.output_features,
                        &leader_info.sender_offset_pubkey,
                        &leader_info.metadata_signature_ephemeral_commitment,
                        &leader_info.metadata_signature_ephemeral_pubkey,
                        &leader_info.output_commitment,
                        &Covenant::default(),
                        &leader_info.encrypted_data,
                        MicroMinotari::zero(),
                    );

                    // KNOWN, DELIBERATELY DEFERRED GAP: `sender_offset_key_id` is the `PreMine` branch key
                    // `get_script_offset` issued in step 2, and `sender_offset_nonce_key_id` is a host indexed nonce
                    // reserved back in step 2, so this routes to `GetRawSchnorrSignatureLegacyNonce`.
                    //
                    // Two signatures under one nonce index would give up this output's sender offset private key,
                    // and the script offset then gives up the script private key; signing this key under the script
                    // signature's nonce would give up both at once. Against that: the self file was validated before
                    // anything was signed (distinct nonce ids, the expected key shapes), the device refuses a second
                    // use of a nonce index within one app run (`LegacyNonceReused`), the nonce is compared just below
                    // with the public nonce stored in the self file (an unauthenticated file, so this only catches
                    // edits that left the stored nonce alone), and this step never signs an output twice. A redirect
                    // to a nonce an earlier app run used, or a compromised host that can get the user to restart the
                    // device app between two approvals, is the residual - see
                    // `minotari_ledger_wallet_common::legacy_nonce`. It cannot
                    // reach `alpha`: a step 2 session file from before sender offsets moved to `PreMine` names a
                    // `OneSidedSenderOffset` key, which the legacy whitelist now refuses - redo step 2 for such a
                    // session. A step 2 file written with a ledger app before 6.1.1-pre.0 is invalid for a second
                    // reason too: its `Random` nonce indexes are at or above 2^32, and those now derive different
                    // keys (all 64 bits of the index count), so its public nonces no longer match. Redoing step 2
                    // draws fresh nonces, so there is no nonce reuse hazard in doing so.
                    // `minotari_ledger_wallet_common::legacy_nonce` sets out the derivation, the scope -
                    // pre-mine only, normal spends use device issued handles - and the TODO that closes it.
                    let metadata_signature = match key_manager_service.sign_with_nonce_and_challenge(
                        &party_info.sender_offset_key_id,
                        &party_info.sender_offset_nonce_key_id,
                        &challenge,
                    ) {
                        Ok(signature) => signature,
                        Err(e) => {
                            if is_legacy_nonce_store_full(&e) {
                                eprintln!("\n{STEP_4_STORE_FULL_MESSAGE}\n");
                            } else {
                                eprintln!("\nError: Metadata signature SignMessage error! {e}\n");
                            }
                            error = true;
                            break;
                        },
                    };
                    if Some(metadata_signature.get_compressed_public_nonce()) !=
                        party_info.public_sender_offset_nonce_key.as_ref()
                    {
                        eprintln!(
                            "\nError: the metadata signature for output {} was made with a nonce that is not the one \
                             step 2 published - the step 2 self file, or the device, has changed. The signature has \
                             already been made on the device but nothing has been written or sent; do not send \
                             anything from this session to the leader.\n",
                            party_info.output_index
                        );
                        error = true;
                        break;
                    }

                    if script_signature.get_signature() == CompressedSignature::default().get_signature() ||
                        metadata_signature.get_signature() == CompressedSignature::default().get_signature()
                    {
                        eprintln!(
                            "\nError: Script and/or metadata signatures not created (index {})!\n",
                            party_info.output_index
                        );
                        error = true;
                        break;
                    }

                    // Recorded before the next output is touched, so that a run stopped from here on never signs this
                    // output again.
                    let done = Step4OutputsForLeader {
                        output_index: party_info.output_index,
                        script_signature,
                        metadata_signature,
                        script_offset,
                    };
                    append_step_4_progress(&progress_file, &session_info, &done)?;
                    signed.push(done);

                    println!(
                        "  Processed {} of {} transactions",
                        i.saturating_add(1),
                        leader_info_indexed.outputs_for_parties.len()
                    );
                }
                if error {
                    break;
                }

                let outputs_for_leader = match merge_step_4_outputs(&party_info_indexes, signed) {
                    Ok(outputs) => outputs,
                    Err(e) => {
                        eprintln!("\nError: {e}\n");
                        break;
                    },
                };
                write_json_object_to_file_as_line(&out_file, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file, false, PreMineSpendStep4OutputsForLeader {
                    outputs_for_leader,
                    alias: party_info_indexed.alias.clone(),
                })?;

                println!();
                println!("Concluded step 4 'pre-mine-spend-input-output-sigs'");
                println!(
                    "Send '{}' to leader for step 5",
                    get_file_name(SPEND_STEP_4_LEADER, Some(party_info_indexed.alias))
                );
                println!();
            },
            PreMineSpendTx(args) => {
                unban_peer_manager_peers = true;

                // Read session info

                let session_info;
                let mut session_id = args.session_id.clone();
                loop {
                    if session_id.is_empty() {
                        eprintln!("\nError: No session id present\n");
                        session_id = InputPrompt::<String>::new()
                            .with_prompt("Please enter a session id to use")
                            .interact()
                            .unwrap();
                        continue;
                    }
                    match read_verify_session_info::<PreMineSpendStep1SessionInfo>(&session_id) {
                        Ok(v) => session_info = v,
                        Err(_) => {
                            eprintln!("\nError: invalid session id\n");
                            session_id = InputPrompt::<String>::new()
                                .with_prompt("Please enter a session id to use")
                                .interact()
                                .unwrap();
                            continue;
                        },
                    }
                    break;
                }

                // Read other parties info
                let mut party_info = Vec::with_capacity(args.member.len());
                for name in args.member {
                    let file_name = get_file_name(SPEND_STEP_4_LEADER, Some(name.clone()));
                    party_info.push(read_and_verify::<PreMineSpendStep4OutputsForLeader>(
                        &args.session_id,
                        &file_name,
                        &session_info,
                    )?);
                }
                // Read own party info
                let leader_info = read_and_verify::<PreMineSpendStep3OutputsForSelf>(
                    &args.session_id,
                    &get_file_name(SPEND_STEP_3_SELF, None),
                    &session_info,
                )?;

                // Verify index consistency
                let session_info_indexes = session_info
                    .recipient_info
                    .iter()
                    .map(|v| v.output_to_be_spend)
                    .collect::<Vec<_>>();
                let leader_info_indexes = leader_info
                    .outputs_for_self
                    .iter()
                    .map(|v| v.output_index)
                    .collect::<Vec<_>>();
                if session_info_indexes != leader_info_indexes {
                    eprintln!(
                        "\nError: Mismatched output indexes detected! session {session_info_indexes:?} vs. leader \
                         (self) {leader_info_indexes:?}\n"
                    );
                    break;
                }
                let mut error = false;
                for party in &party_info {
                    let party_info_indexes = party
                        .outputs_for_leader
                        .iter()
                        .map(|v| v.output_index)
                        .collect::<Vec<_>>();
                    if session_info_indexes != party_info_indexes {
                        eprintln!(
                            "\nError: Mismatched output indexes from '{}' detected! session {:?} vs. party {:?}\n",
                            party.alias, session_info_indexes, party_info_indexes
                        );
                        error = true;
                        break;
                    }
                }
                if error {
                    break;
                }

                // Flatten and transpose party_info to be indexed by output index
                let party_info_flattened = party_info
                    .iter()
                    .map(|v1| v1.outputs_for_leader.clone())
                    .collect::<Vec<_>>();
                let mut party_info_per_index =
                    Vec::with_capacity(party_info_flattened.first().expect("Already checked").len());
                let number_of_parties = party_info_flattened.len();
                for i in 0..party_info_flattened.first().expect("Already checked").len() {
                    let mut outputs_per_index = Vec::with_capacity(number_of_parties);
                    for outputs in &party_info_flattened {
                        outputs_per_index.push(outputs.get(i).expect("Already checked").clone());
                    }
                    party_info_per_index.push(outputs_per_index);
                }

                // Create finalized spend transactions
                for (i, (indexed_info, leader_self)) in party_info_per_index
                    .iter()
                    .zip(leader_info.outputs_for_self.iter())
                    .enumerate()
                {
                    let mut metadata_signatures = Vec::with_capacity(party_info_per_index.len());
                    let mut script_signatures = Vec::with_capacity(party_info_per_index.len());
                    let mut offset = PrivateKey::default();
                    for party_info in indexed_info {
                        metadata_signatures.push(party_info.metadata_signature.clone());
                        script_signatures.push(party_info.script_signature.clone());
                        offset = &offset + &party_info.script_offset;
                    }

                    if let Err(e) = finalise_aggregate_utxo(
                        transaction_service.clone(),
                        leader_self.tx_id.as_u64(),
                        metadata_signatures,
                        script_signatures,
                        offset,
                    )
                    .await
                    {
                        eprintln!(
                            "\nError: Error completing transaction '{}'! ({})\n",
                            leader_self.tx_id, e
                        );
                        error = true;
                        break;
                    }

                    // Collect all inputs, outputs and kernels that should go into the genesis block
                    println!();
                    println!("  Processed {} of {}", i.saturating_add(1), party_info_per_index.len());
                }
                if error {
                    break;
                }
                println!();
                println!("Concluded step 5 'pre-mine-spend-aggregate-transaction'");
                println!();
            },
            SendOneSidedToStealthAddress(args) => {
                let memo = match MemoField::new_open_from_string(
                    &args.payment_id,
                    detect_tx_metadata(&wallet, &args.destination).await,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!(
                            "SendOneSidedToStealthAddress error! Could not create memo field from payment id string: \
                             {e}"
                        );
                        continue;
                    },
                };
                match send_one_sided_to_stealth_address(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    UtxoSelectionCriteria::default(),
                    args.destination,
                    memo,
                )
                .await
                {
                    Ok(tx_id) => {
                        debug!(
                            target: LOG_TARGET,
                            "send-one-sided-to-stealth-address concluded with tx_id {tx_id}"
                        );
                        println!("Transaction completed. ID: {tx_id}");
                        println!("Waiting for broadcast confirmation...");
                        tx_ids.push(tx_id);
                        // Wait for broadcast confirmation
                        let broadcast_timeout = Duration::from_millis(config.grpc_broadcast_confirmation);
                        match timeout(broadcast_timeout, async {
                            loop {
                                if let Ok(Some(tx)) = transaction_service.get_any_transaction(tx_id).await {
                                    match tx.status() {
                                        LegacyTransactionStatus::Broadcast |
                                        LegacyTransactionStatus::MinedUnconfirmed |
                                        LegacyTransactionStatus::MinedConfirmed |
                                        LegacyTransactionStatus::OneSidedUnconfirmed |
                                        LegacyTransactionStatus::OneSidedConfirmed |
                                        LegacyTransactionStatus::MinedConfirmedLocked |
                                        LegacyTransactionStatus::OneSidedConfirmedLocked |
                                        LegacyTransactionStatus::CoinbaseConfirmedLocked |
                                        LegacyTransactionStatus::Imported => {
                                            break Ok(tx.status());
                                        },
                                        LegacyTransactionStatus::Rejected => {
                                            let reason = tx
                                                .cancelled_reason()
                                                .map(|r| format!("{r}"))
                                                .unwrap_or_else(|| "Unknown reason".to_string());
                                            break Err(reason);
                                        },
                                        _ => {
                                            sleep(Duration::from_millis(100)).await;
                                        },
                                    }
                                } else {
                                    sleep(Duration::from_millis(100)).await;
                                }
                            }
                        })
                        .await
                        {
                            Ok(Ok(status)) => {
                                println!(
                                    "Transaction {tx_id} successfully broadcast to the network (status: {status})."
                                );
                            },
                            Ok(Err(reason)) => {
                                eprintln!(
                                    "Transaction {tx_id} was completed but rejected during submission: {reason}. \
                                     Transaction is saved and can be retried."
                                );
                            },
                            Err(_) => {
                                println!(
                                    "Transaction {tx_id} completed but broadcast is still pending. Transaction is \
                                     saved and will be broadcast when possible."
                                );
                            },
                        }
                    },
                    Err(e) => eprintln!("Transaction abandoned: {e}"),
                }
            },
            MakeItRain(args) => {
                let transaction_type = args.transaction_type();
                let memo = match MemoField::new_open_from_string(
                    &args.payment_id,
                    detect_tx_metadata(&wallet, &args.destination).await,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("MakeItRain error! Could not create memo field from payment id string: {e}");
                        continue;
                    },
                };
                if let Err(e) = make_it_rain(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.transactions_per_second,
                    args.duration,
                    args.start_amount,
                    args.increase_amount,
                    args.start_time.unwrap_or_else(Utc::now),
                    args.destination,
                    transaction_type,
                    memo,
                )
                .await
                {
                    eprintln!("MakeItRain error! {e}");
                }
            },
            CoinSplit(args) => {
                let memo = match MemoField::new_open_from_string(&args.payment_id, TxType::CoinSplit) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("CoinSplit error! Could not create memo field from payment id string: {e}");
                        continue;
                    },
                };
                match coin_split(
                    args.amount_per_split,
                    args.num_splits,
                    args.fee_per_gram,
                    memo,
                    &mut output_service,
                    &mut transaction_service.clone(),
                )
                .await
                {
                    Ok(tx_id) => {
                        tx_ids.push(tx_id);
                        debug!(target: LOG_TARGET, "coin-split concluded with tx_id {tx_id}");
                        println!("Coin split succeeded");
                    },
                    Err(e) => eprintln!("CoinSplit error! {e}"),
                }
            },
            Whois(args) => {
                let public_key = args.public_key.into();
                let emoji_id = EmojiId::from(&public_key).to_string();

                println!("Public Key: {}", public_key.to_hex());
                println!("Emoji ID  : {emoji_id}");
            },
            ExportUtxos(args) => match output_service.get_unspent_outputs().await {
                Ok(utxos) => {
                    let mut unblinded_utxos: Vec<(UnblindedOutput, CompressedCommitment)> =
                        Vec::with_capacity(utxos.len());
                    for output in utxos {
                        let unblinded =
                            UnblindedOutput::from_wallet_output(output.wallet_output, &wallet.key_manager_service)?;
                        unblinded_utxos.push((unblinded, output.commitment));
                    }
                    let count = unblinded_utxos.len();
                    let sum: MicroMinotari = unblinded_utxos.iter().map(|utxo| utxo.0.value).sum();
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_utxos_to_csv_file(unblinded_utxos, file, args.with_private_keys) {
                            eprintln!("ExportUtxos error! {e}");
                        }
                    } else {
                        for (i, utxo) in unblinded_utxos.iter().enumerate() {
                            println!(
                                "{}. Value: {}, Spending Key: {:?}, Script Key: {:?}, Features: {}, Commitment: {}, \
                                 isMultisig: {}",
                                i.saturating_add(1),
                                utxo.0.value,
                                if args.with_private_keys {
                                    utxo.0.commitment_mask_key.to_hex()
                                } else {
                                    "*hidden*".to_string()
                                },
                                if args.with_private_keys {
                                    utxo.0.script_private_key.to_hex()
                                } else {
                                    "*hidden*".to_string()
                                },
                                utxo.0.features,
                                utxo.1.to_hex(),
                                is_multisig_utxo(&utxo.0.script)
                            );
                        }
                    }
                    println!("Total number of UTXOs: {count}");
                    println!("Total value of UTXOs: {sum}");
                },
                Err(e) => eprintln!("ExportUtxos error! {e}"),
            },
            ExportTx(args) => match transaction_service.get_any_transaction(args.tx_id.into()).await {
                Ok(Some(tx)) => {
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_tx_to_csv_file(tx, file) {
                            eprintln!("ExportTx error! {e}");
                        }
                    } else {
                        println!("Tx: {tx:?}");
                    }
                },
                Ok(None) => {
                    eprintln!("ExportTx error!, No tx found ")
                },
                Err(e) => eprintln!("ExportTx error! {e}"),
            },
            ImportTx(args) => {
                match load_tx_from_csv_file(args.input_file) {
                    Ok(txs) => {
                        for tx in txs {
                            match transaction_service.import_transaction(tx).await {
                                Ok(id) => println!("imported tx: {id}"),
                                Err(e) => eprintln!("Could not import tx {e}"),
                            };
                        }
                    },
                    Err(e) => eprintln!("ImportTx error! {e}"),
                };
            },
            ExportSpentUtxos(args) => match output_service.get_spent_outputs().await {
                Ok(utxos) => {
                    let mut unblinded_utxos: Vec<(UnblindedOutput, CompressedCommitment)> =
                        Vec::with_capacity(utxos.len());
                    for output in utxos {
                        let unblinded =
                            UnblindedOutput::from_wallet_output(output.wallet_output, &wallet.key_manager_service)?;
                        unblinded_utxos.push((unblinded, output.commitment));
                    }
                    let count = unblinded_utxos.len();
                    let sum: MicroMinotari = unblinded_utxos.iter().map(|utxo| utxo.0.value).sum();
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_utxos_to_csv_file(unblinded_utxos, file, args.with_private_keys) {
                            eprintln!("ExportSpentUtxos error! {e}");
                        }
                    } else {
                        for (i, utxo) in unblinded_utxos.iter().enumerate() {
                            println!(
                                "{}. Value: {}, Spending Key: {:?}, Script Key: {:?}, Features: {}",
                                i.saturating_add(1),
                                utxo.0.value,
                                if args.with_private_keys {
                                    utxo.0.commitment_mask_key.to_hex()
                                } else {
                                    "*hidden*".to_string()
                                },
                                if args.with_private_keys {
                                    utxo.0.script_private_key.to_hex()
                                } else {
                                    "*hidden*".to_string()
                                },
                                utxo.0.features
                            );
                        }
                    }
                    println!("Total number of UTXOs: {count}");
                    println!("Total value of UTXOs: {sum}");
                },
                Err(e) => eprintln!("ExportSpentUtxos error! {e}"),
            },
            CountUtxos => match output_service.get_unspent_outputs().await {
                Ok(utxos) => {
                    let utxos: Vec<WalletOutput> = utxos.into_iter().map(|v| v.wallet_output).collect();
                    let count = utxos.len();
                    let values: Vec<MicroMinotari> = utxos.iter().map(|utxo| utxo.value()).collect();
                    let sum: MicroMinotari = values.iter().sum();
                    println!("Total number of UTXOs: {count}");
                    println!("Total value of UTXOs : {sum}");
                    if let Some(min) = values.iter().min() {
                        println!("Minimum value UTXO   : {min}");
                    }
                    if count > 0 {
                        let average_val = sum.as_u64().div_euclid(count as u64);
                        let average = Minotari::from(MicroMinotari(average_val));
                        println!("Average value UTXO   : {average}");
                    }
                    if let Some(max) = values.iter().max() {
                        println!("Maximum value UTXO   : {max}");
                    }
                },
                Err(e) => eprintln!("CountUtxos error! {e}"),
            },
            InitShaAtomicSwap(args) => {
                let memo = match MemoField::new_open_from_string(&args.payment_id, TxType::ClaimAtomicSwap) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("CoinSplit error! Could not create memo field from payment id string: {e}");
                        continue;
                    },
                };
                match init_sha_atomic_swap(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    UtxoSelectionCriteria::default(),
                    args.destination,
                    memo,
                )
                .await
                {
                    Ok((tx_id, pre_image, output)) => {
                        debug!(target: LOG_TARGET, "minotari HTLC tx_id {tx_id}");
                        let hash: [u8; 32] = Sha256::digest(pre_image.as_bytes()).into();
                        println!("pre_image hex: {}", pre_image.to_hex());
                        println!("pre_image hash: {}", hash.to_hex());
                        println!("Output hash: {}", output.hash().to_hex());
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("InitShaAtomicSwap error! {e}"),
                }
            },
            FinaliseShaAtomicSwap(args) => {
                let memo = match MemoField::new_open_from_string(&args.payment_id, TxType::ClaimAtomicSwap) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!(
                            "FinaliseShaAtomicSwap error! Could not create memo field from payment id string: {e}"
                        );
                        continue;
                    },
                };
                match args.output_hash.first().expect("Already checked").clone().try_into() {
                    Ok(hash) => {
                        match finalise_sha_atomic_swap(
                            output_service.clone(),
                            transaction_service.clone(),
                            hash,
                            args.pre_image.into(),
                            config.fee_per_gram.into(),
                            memo,
                        )
                        .await
                        {
                            Ok(tx_id) => {
                                debug!(target: LOG_TARGET, "claiming minotari HTLC tx_id {tx_id}");
                                tx_ids.push(tx_id);
                            },
                            Err(e) => eprintln!("FinaliseShaAtomicSwap error! {e}"),
                        }
                    },
                    Err(e) => eprintln!("FinaliseShaAtomicSwap error! {e}"),
                }
            },
            ClaimShaAtomicSwapRefund(args) => {
                let memo = match MemoField::new_open_from_string(&args.payment_id, TxType::HtlcAtomicSwapRefund) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!(
                            "ClaimShaAtomicSwapRefund error! Could not create memo field from payment id string: {e}"
                        );
                        continue;
                    },
                };
                match args.output_hash.first().expect("Already checked").clone().try_into() {
                    Ok(hash) => {
                        match claim_htlc_refund(
                            output_service.clone(),
                            transaction_service.clone(),
                            hash,
                            config.fee_per_gram.into(),
                            memo,
                        )
                        .await
                        {
                            Ok(tx_id) => {
                                debug!(target: LOG_TARGET, "claiming minotari HTLC tx_id {tx_id}");
                                tx_ids.push(tx_id);
                            },
                            Err(e) => eprintln!("ClaimShaAtomicSwapRefund error! {e}"),
                        }
                    },
                    Err(e) => eprintln!("FinaliseShaAtomicSwap error! {e}"),
                }
            },
            RegisterValidatorNode(args) => {
                let memo = match MemoField::new_open_from_string(&args.payment_id, TxType::ValidatorNodeRegistration) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!(
                            "RegisterValidatorNode error! Could not create memo field from payment id string: {e}"
                        );
                        continue;
                    },
                };
                let tx_id = register_validator_node(
                    args.amount,
                    transaction_service.clone(),
                    args.validator_node_public_key.into(),
                    CompressedSignature::new(
                        args.validator_node_public_nonce.into(),
                        RistrettoSecretKey::from_vec(args.validator_node_signature.first().expect("Already checked"))?,
                    ),
                    args.validator_node_claim_public_key.into(),
                    if args.sidechain_deployment_key.is_empty() {
                        None
                    } else {
                        Some(RistrettoSecretKey::from_canonical_bytes(
                            args.sidechain_deployment_key.first().expect("Already checked"),
                        )?)
                    },
                    args.epoch,
                    UtxoSelectionCriteria::default(),
                    MicroMinotari::from(config.fee_per_gram),
                    memo,
                )
                .await?;
                debug!(target: LOG_TARGET, "Registering VN tx_id {tx_id}");
                tx_ids.push(tx_id);
            },
            CreateTlsCerts => match generate_self_signed_certs() {
                Ok((cacert, cert, private_key)) => {
                    print_warning();

                    write_cert_to_disk(config.config_dir.clone(), "wallet_ca.pem", &cacert)?;
                    write_cert_to_disk(config.config_dir.clone(), "server.pem", &cert)?;
                    write_cert_to_disk(config.config_dir.clone(), "server.key", &private_key)?;

                    println!();
                    println!("Certificates generated successfully.");
                    println!(
                        "To continue configuration move the `wallet_ca.pem` to the client service's \
                         `application/config/` directory. Restart the base node with the configuration \
                         grpc_tls_enabled=true"
                    );
                    println!();
                },
                Err(err) => eprintln!("Error generating certificates: {err}"),
            },
            Sync(args) => {
                let mut utxo_scanner = wallet.utxo_scanner_service.clone();
                let mut receiver = utxo_scanner.get_event_receiver();

                loop {
                    match receiver.recv().await {
                        Ok(event) => match event {
                            UtxoScannerEvent::ScanningRoundFailed {
                                num_retries,
                                retry_limit,
                                error,
                            } => {
                                println!("Scanning round failed. Retries: {num_retries}/{retry_limit}. Error: {error}");
                            },
                            UtxoScannerEvent::Progress {
                                current_height,
                                tip_height,
                                latency,
                                ..
                            } => {
                                println!(
                                    "Progress: {}/{} (Latency: {}ms)",
                                    current_height,
                                    tip_height,
                                    latency.as_millis()
                                );
                                if current_height >= args.sync_to_height && args.sync_to_height > 0 {
                                    break;
                                }
                            },
                            UtxoScannerEvent::Completed {
                                final_height,
                                time_taken,
                                ..
                            } => {
                                println!(
                                    "Completed! Height: {},  Time taken: {}",
                                    final_height,
                                    time_taken.as_secs()
                                );

                                break;
                            },
                        },
                        Err(e) => {
                            eprintln!("Sync error! {e}");
                            break;
                        },
                    }
                }
                println!("Starting validation process");
                let mut oms = wallet.output_manager_service.clone();
                oms.validate_txos().await?;
                let mut event = oms.get_event_stream();
                loop {
                    match event.recv().await {
                        Ok(event) => match *event {
                            OutputManagerEvent::TxoValidationSuccess(_) => {
                                println!("Validation succeeded");
                                break;
                            },
                            OutputManagerEvent::TxoValidationAlreadyBusy(_) => {
                                println!("Validation already busy");
                            },
                            _ => {
                                println!("Validation failed");
                                break;
                            },
                        },
                        Err(e) => {
                            eprintln!("Sync error! {e}");
                            break;
                        },
                    }
                }
                println!("balance as of scanning height");
                match output_service.clone().get_balance().await {
                    Ok(balance) => {
                        println!("{balance}");
                    },
                    Err(e) => eprintln!("GetBalance error! {e}"),
                }
            },
            ExportViewKeyAndSpendKey(args) => {
                let view_key = wallet.key_manager_service.get_view_key();
                let spend_key = wallet.key_manager_service.get_spend_key();
                let view_key_hex = view_key.pub_key.to_hex();
                let private_view_key_hex = wallet.key_manager_service.get_private_view_key().to_hex();
                let spend_key_hex = spend_key.pub_key.to_hex();
                let output_file = args.output_file;
                let birthday = wallet.db.get_wallet_birthday()?;
                #[derive(Serialize)]
                struct ViewKeyFile {
                    view_key: String,
                    public_view_key: String,
                    spend_key: String,
                    birthday: u16,
                }
                let view_key_file = ViewKeyFile {
                    view_key: private_view_key_hex.clone(),
                    public_view_key: view_key_hex.clone(),
                    spend_key: spend_key_hex.clone(),
                    birthday,
                };
                let view_key_file_json =
                    serde_json::to_string(&view_key_file).map_err(|e| CommandError::JsonFile(e.to_string()))?;
                if let Some(file) = output_file {
                    let file = File::create(file).map_err(|e| CommandError::JsonFile(e.to_string()))?;
                    let mut file = LineWriter::new(file);
                    writeln!(file, "{view_key_file_json}").map_err(|e| CommandError::JsonFile(e.to_string()))?;
                } else {
                    println!("View key: {private_view_key_hex}");
                    println!("Spend key: {spend_key_hex}");
                    println!("Birthday: {birthday}");
                }
            },
            ImportPaperWallet(args) => {
                // The temporary wallet holds the recovered master seed, so it gets a randomly named directory that is
                // removed again on every exit path. Creation fails rather than reusing an existing directory, so a
                // leftover from an earlier crash is never adopted, however unlikely a name collision is.
                let temp_path = config
                    .db_file
                    .parent()
                    .ok_or(CommandError::General("No parent".to_string()))?
                    .join(format!("temp-{}", random_alphanumeric(8)));
                println!("saving temp wallet in: {temp_path:?}");
                let temp_wallet_dir = TempWalletDir::create(temp_path.clone())?;
                // Owns the temporary wallet's services. It is triggered on every exit path below, and before the
                // directory is removed we wait, bounded by TEMP_WALLET_SHUTDOWN_TIMEOUT, for the tasks holding one of
                // its shutdown signals to exit. The drain only sees tasks that hold a signal, and a removal after a
                // timeout may still race an open database handle (on Windows the removal then fails and the error is
                // reported).
                let shutdown = Shutdown::new();
                let result: Result<(), CommandError> = async {
                    let passphrase = if args.passphrase.is_empty() {
                        None
                    } else {
                        Some(SafePassword::from(args.passphrase))
                    };
                    let seed = match (!args.seed_words.is_empty(), !args.cipher_seed.is_empty()) {
                        (true, false) => {
                            let seed_words = SeedWords::from_str(args.seed_words.as_str())
                                .map_err(|e| CommandError::General(e.to_string()))?;

                            get_seed_from_seed_words(&seed_words, passphrase)
                                .map_err(|e| CommandError::General(e.to_string()))?
                        },
                        (false, true) => {
                            let bytes = Vec::<u8>::from_monero_base58(args.cipher_seed.as_str())
                                .map_err(|e| CommandError::General(e.to_string()))?;
                            CipherSeed::from_enciphered_bytes(&bytes, passphrase)
                                .map_err(|e| CommandError::General(e.to_string()))?
                        },
                        (_, _) => {
                            return Err(CommandError::General(
                                "Either seed words or cipher seed must be provided".to_string(),
                            ));
                        },
                    };

                    let wallet_type = LegacyWalletType::DerivedKeys;
                    // The temporary wallet database is deleted again when this command finishes, so its password
                    // is never needed a second time; a random single-use one keeps the seed it holds unreadable
                    // if the database does survive (e.g. the process is killed before the guard can run).
                    let password = SafePassword::from(random_alphanumeric(32));
                    let shutdown_signal = shutdown.to_signal();
                    let mut new_config = config.clone();
                    // Directly set paths to temp_path. We cannot use set_base_path here because
                    // config paths may already be absolute (set during wallet initialization), and
                    // set_base_path only modifies relative paths.
                    new_config.data_dir = temp_path.clone();
                    new_config.config_dir = temp_path.join("config");
                    new_config.db_file = temp_path.join("console_wallet.db");

                    let peer_config = PeerSeedsConfig::default();
                    let new_wallet = init_wallet(
                        &new_config,
                        AutoUpdateConfig::default(),
                        peer_config,
                        password,
                        None,
                        Some(seed),
                        shutdown_signal,
                        true,
                        Some(wallet_type),
                    )
                    .await
                    .map_err(|e| CommandError::General(e.to_string()))?;
                    // config

                    wallet_recovery(&new_wallet, new_config.recovery_retry_limit)
                        .await
                        .map_err(|e| CommandError::General(e.to_string()))?;
                    print!("Wallet recovery completed");
                    let mut oms = new_wallet.output_manager_service.clone();
                    oms.validate_txos().await?;
                    let mut event = oms.get_event_stream();
                    loop {
                        match event.recv().await {
                            Ok(event) => match *event {
                                OutputManagerEvent::TxoValidationSuccess(_) => {
                                    println!("Validation succeeded");
                                    break;
                                },
                                OutputManagerEvent::TxoValidationAlreadyBusy(_) => {
                                    println!("Validation already busy");
                                },
                                _ => {
                                    println!("Validation failed");
                                    break;
                                },
                            },
                            Err(e) => {
                                eprintln!("Sync error! {e}");
                                break;
                            },
                        }
                    }
                    println!("balance as of scanning height");
                    match oms.clone().get_balance().await {
                        Ok(balance) => {
                            println!("{balance}");
                        },
                        Err(e) => eprintln!("GetBalance error! {e}"),
                    }
                    let mut tms = new_wallet.transaction_service.clone();
                    match tms
                        .scrape_wallet(
                            wallet
                                .get_wallet_one_sided_address()
                                .map_err(|e| CommandError::General(e.to_string()))?,
                            MicroMinotari::from(config.fee_per_gram),
                        )
                        .await
                        .map_err(CommandError::TransactionServiceError)
                    {
                        Ok(tx_id) => {
                            debug!(target: LOG_TARGET, "send-minotari concluded with tx_id {tx_id}");
                            let duration = config.command_send_wait_timeout;
                            match timeout(duration, monitor_transactions(tms.clone(), vec![tx_id], wait_stage)).await {
                                Ok(txs) => {
                                    debug!(
                                        target: LOG_TARGET,
                                        "monitor_transactions done to stage {wait_stage:?} with tx_ids: {txs:?}"
                                    );
                                    println!("Done! All transactions monitored to {wait_stage:?} stage.");
                                },
                                Err(_e) => {
                                    println!(
                                        "The configured timeout ({duration:#?}) was reached before all transactions \
                                         reached the {wait_stage:?} stage. See the logs for more info."
                                    );
                                },
                            }
                        },
                        Err(e) => eprintln!("SendMinotari error! {e}"),
                    }
                    Ok(())
                }
                .await;
                // The temporary wallet is out of scope; stop its services and wait (bounded) for them to exit.
                shutdown.trigger();
                if timeout(TEMP_WALLET_SHUTDOWN_TIMEOUT, shutdown.wait_for_listeners())
                    .await
                    .is_err()
                {
                    warn!(
                        target: LOG_TARGET,
                        "Timed out after {TEMP_WALLET_SHUTDOWN_TIMEOUT:.0?} waiting for the temporary wallet to shut \
                         down; removing it anyway"
                    );
                }
                // Remove explicitly rather than on drop, so that a failure to delete the seed-bearing database fails
                // the command instead of being logged and ignored. A command error takes precedence, but `remove`
                // consumed the drop guard, so a removal failure must be surfaced here even when the command failed.
                let removed = temp_wallet_dir.remove();
                if let (Err(_), Err(remove_error)) = (&result, &removed) {
                    error!(target: LOG_TARGET, "{remove_error}");
                    eprintln!("{remove_error}");
                }
                result?;
                removed?;
            },

            ShowPayRef(args) => {
                // Show transaction details first
                match transaction_service
                    .get_any_transaction(args.transaction_id.into())
                    .await
                {
                    Ok(Some(tx)) => {
                        println!("Transaction ID: {}", args.transaction_id);
                        let _status = match &tx {
                            WalletTransaction::Completed(completed_tx) => {
                                println!("Transaction status: Completed");
                                println!("Amount: {}", completed_tx.amount);
                                println!("Fee: {}", completed_tx.fee);
                                println!("Direction: {:?}", completed_tx.direction);
                                if let Some(height) = completed_tx.mined_height {
                                    println!("Mined at height: {height}");
                                }
                                if let Some(timestamp) = completed_tx.mined_timestamp {
                                    println!("Mined timestamp: {timestamp}");
                                }
                                if completed_tx.mined_in_block.is_some() {
                                    println!("\nReceived PayRefs for this transaction:");
                                    for (i, pay_ref) in completed_tx.calculate_received_payment_references().iter().enumerate() {
                                        println!("{}. PayRef: {}", i.saturating_add(1), pay_ref);
                                    }
                                    println!("\nSent PayRefs for this transaction:");
                                    for (i, pay_ref) in completed_tx.calculate_sent_payment_references().iter().enumerate() {
                                        println!("{}. PayRef: {}", i.saturating_add(1), pay_ref);
                                    }
                                    println!("\nChange PayRefs for this transaction:");
                                    for (i, pay_ref) in completed_tx.calculate_change_payment_references().iter().enumerate() {
                                        println!("{}. PayRef: {}", i.saturating_add(1), pay_ref);
                                    }
                                } else {
                                    println!("Payrefs: Transaction not mined yet.");
                                }
                                "Completed"
                            },
                            minotari_wallet::transaction_service::storage::models::WalletTransaction::PendingInbound(_) => {
                                println!("Transaction status: PendingInbound");
                                "PendingInbound"
                            },
                            minotari_wallet::transaction_service::storage::models::WalletTransaction::PendingOutbound(_) => {
                                println!("Transaction status: PendingOutbound");
                                "PendingOutbound"
                            },
                        };
                    },
                    Ok(None) => {
                        println!("Transaction ID {} not found", args.transaction_id);
                    },
                    Err(e) => eprintln!("ShowPayRef error! {e}"),
                }
            },
            FindPayRef(args) => match FixedHash::from_hex(&args.payment_reference_hex) {
                Ok(payref) => match transaction_service.get_payment_by_reference(payref).await {
                    Ok(Some(payment_details)) => {
                        println!("Found PayRef: {}", args.payment_reference_hex);
                        println!("Transaction ID: {}", payment_details.tx_id);
                        println!("Amount: {}", payment_details.amount);
                        println!("Direction: {:?}", payment_details.direction);
                        println!("Block height: {}", payment_details.block_height);
                        println!("Confirmations: {}", payment_details.confirmations);
                        if let Some(timestamp) = payment_details.timestamp {
                            println!("Timestamp: {timestamp}");
                        }
                        if let Some(payment_id) = &payment_details.payment_id {
                            println!("Payment ID: {}", String::from_utf8_lossy(payment_id));
                        }
                    },
                    Ok(None) => {
                        println!("No payment found for PayRef: {}", args.payment_reference_hex);
                    },
                    Err(e) => eprintln!("FindPayRef error! {e}"),
                },
                Err(e) => {
                    eprintln!("FindPayRef error! Invalid PayRef format: {e}");
                },
            },
            ListTx => {
                debug!(target: LOG_TARGET, "payref_debug: List all transactions command starting execution");
                match transaction_service
                    .get_completed_transactions(None, None, None, 0)
                    .await
                {
                    Ok(txs) => {
                        debug!(target: LOG_TARGET, "ListTxs command got {} transactions", txs.len());
                        if txs.is_empty() {
                            println!("No transactions.");
                            continue;
                        }
                        println!("Found {} transaction(s)", txs.len());
                        println!("{}", "=".repeat(80));

                        for (i, tx) in txs.iter().enumerate() {
                            println!("{}. Transaction ID: {}", i.saturating_add(1), tx.tx_id);
                            println!("   Amount: {}", tx.amount);
                            println!("   Direction: {:?}", tx.direction);
                            println!("   Status: {:?}", tx.status);
                            if let Some(height) = tx.mined_height {
                                println!("   Mined at height: {height}");
                            }
                            if let Some(timestamp) = tx.mined_timestamp {
                                println!("   Mined timestamp: {timestamp}");
                            }
                            if tx.mined_in_block.is_some() {
                                println!("\nReceived PayRefs for this transaction:");
                                for (i, pay_ref) in tx.calculate_received_payment_references().iter().enumerate() {
                                    println!("{}. PayRef: {}", i.saturating_add(1), pay_ref);
                                }
                                println!("\nSent PayRefs for this transaction:");
                                for (i, pay_ref) in tx.calculate_sent_payment_references().iter().enumerate() {
                                    println!("{}. PayRef: {}", i.saturating_add(1), pay_ref);
                                }
                                println!("\nChange PayRefs for this transaction:");
                                for (i, pay_ref) in tx.calculate_change_payment_references().iter().enumerate() {
                                    println!("{}. PayRef: {}", i.saturating_add(1), pay_ref);
                                }
                            } else {
                                println!("Payrefs: Transaction not mined yet.");
                            }
                            println!();
                        }
                    },
                    Err(e) => eprintln!("ListTxs error! {e}"),
                }
            },

            GetMultisigUtxoData(args) => {
                let mut transaction_service = wallet.transaction_service.clone();

                let output = transaction_service.get_multisig_utxo_data(args.utxo_commitment).await?;

                if let Some(file) = args.output_file {
                    if let Some(parent) = file.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| CommandError::JsonFile(format!("Failed to create directory: {}", e)))?;
                    }

                    let signature_json = serde_json::to_string(&output).map_err(|e| {
                        println!("Failed to convert signature to JSON: {}", e);
                        CommandError::General(format!("Failed to convert signature to JSON: {}", e))
                    })?;

                    fs::write(&file, signature_json)
                        .map_err(|err| CommandError::FileWriteError { file_path: file, err })?;
                }
            },

            SendMultisigUtxo(args) => {
                let mut transaction_service = wallet.transaction_service.clone();
                let mut schnorr_signatures = Vec::new();

                for signature in &args.schnorr_signatures {
                    let sig = <CompressedCheckSigSchnorrSignature as tari_utilities::message_format::MessageFormat>::from_binary(signature)
                        .map_err(|e| CommandError::General(format!("Failed to parse Schnorr signature: {}", e)))?;

                    schnorr_signatures.push(sig);
                }

                let tx_id = transaction_service
                    .send_multisig_utxo(args.utxo_commitment, args.recipient_address, schnorr_signatures)
                    .await?;

                tx_ids.push(tx_id);
                debug!(target: LOG_TARGET, "Utxo was sent with tx_id {}", tx_id);
            },

            CreateMultisigUtxo(args) => {
                if args.party_number as usize > args.public_keys.len() {
                    return Err(CommandError::General(
                        "party_number must be less than or equal to the number of public keys".to_string(),
                    ));
                }

                let public_keys = args
                    .public_keys
                    .iter()
                    .map(|pk| CompressedPublicKey::from(pk.clone()))
                    .collect::<Vec<_>>();

                let result = transaction_service
                    .create_multisig_utxo(args.amount, args.party_number, public_keys, args.recipient_address)
                    .await;

                match result {
                    Ok(tx_id) => {
                        tx_ids.push(tx_id);
                        debug!(target: LOG_TARGET, "Utxo changed to multisig with tx_id {}", tx_id);
                    },
                    Err(e) => {
                        eprintln!("Error creating multisig UTXO: {}", e);
                    },
                }
            },
            PrepareOneSidedTransactionForSigning(args) => {
                let memo = match MemoField::new_open_from_string(
                    &args.payment_id,
                    detect_tx_metadata(&wallet, &args.destination).await,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!(
                            "PrepareOneSidedTransactionForSigning error! Could not create memo field from payment id \
                             string: {e}"
                        );
                        continue;
                    },
                };

                let mut wallet_transaction_service = transaction_service.clone();
                let result = wallet_transaction_service
                    .prepare_one_sided_transaction_for_signing(
                        args.destination,
                        args.amount,
                        UtxoSelectionCriteria::default(),
                        OutputFeatures::default(),
                        MicroMinotari::from(config.fee_per_gram),
                        memo,
                    )
                    .await
                    .map_err(CommandError::TransactionServiceError);
                match result {
                    Ok(data) => {
                        let json_data = data
                            .to_json()
                            .map_err(|e| CommandError::SerializationError(e.to_string()))?;
                        fs::write(&args.output_file, json_data).map_err(|err| CommandError::FileWriteError {
                            file_path: args.output_file,
                            err,
                        })?;
                    },
                    Err(e) => eprintln!("PrepareOneSidedTransactionForSigning error! {e}"),
                }
            },
            PrepareDepositMultisigTransaction(args) => {
                let mut wallet_transaction_service = transaction_service.clone();

                let recipient = TariAddress::from_bytes(args.recipient_address.to_vec().as_slice())
                    .map_err(|e| CommandError::InvalidArgument(format!("Invalid recipient address: {e}")))?;

                let public_keys = args
                    .public_keys
                    .into_iter()
                    .map(|pk_bytes| {
                        CompressedPublicKey::from_canonical_bytes(pk_bytes.0.as_bytes())
                            .map_err(|e| CommandError::InvalidArgument(format!("Invalid public key: {e}")))
                    })
                    .collect::<Result<Vec<_>, CommandError>>()?;

                let result = wallet_transaction_service
                    .prepare_deposit_multisig_transaction(args.amount, args.party_number, public_keys, recipient)
                    .await?;

                println!("Prepared deposit multisig transaction: {:?}", result);
            },

            PrepareWithdrawMultisigTransaction(args) => {
                let mut wallet_transaction_service = transaction_service.clone();

                let recipient = TariAddress::from_bytes(args.recipient_address.to_vec().as_slice())
                    .map_err(|e| CommandError::InvalidArgument(format!("Invalid recipient address: {e}")))?;
                let mut schnorr_signatures = Vec::new();
                for signature in &args.schnorr_signatures {
                    let sig = <CompressedCheckSigSchnorrSignature as tari_utilities::message_format::MessageFormat>::from_binary(signature)
                        .map_err(|e| CommandError::General(format!("Failed to parse Schnorr signature: {}", e)))?;

                    schnorr_signatures.push(sig);
                }

                if schnorr_signatures.is_empty() {
                    return Err(CommandError::InvalidArgument("signatures cannot be empty".to_string()));
                }

                let commitment = CompressedCommitment::from_hex(&args.utxo_commitment)
                    .map_err(|e| CommandError::InvalidArgument(format!("Invalid UTXO commitment: {e}")))?;

                let result = wallet_transaction_service
                    .prepare_withdraw_multisig_transaction(commitment, schnorr_signatures, recipient)
                    .await?;

                println!("Prepared withdraw multisig transaction: {:?}", result);
            },

            SignOneSidedTransaction(args) => {
                let metadata = fs::metadata(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file.clone(),
                    err,
                })?;
                let max_size = 10_000_000; // 10MB limit
                if metadata.len() > max_size {
                    return Err(CommandError::InvalidArgument("Input file too large".to_string()));
                }

                let data = fs::read_to_string(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file,
                    err,
                })?;
                let request = PrepareOneSidedTransactionForSigningResult::from_json(&data)?;

                let mut wallet_transaction_service = transaction_service.clone();
                let result = wallet_transaction_service
                    .sign_one_sided_transaction(request)
                    .await
                    .map_err(CommandError::TransactionServiceError);
                match result {
                    Ok(data) => {
                        let json_data = data
                            .to_json()
                            .map_err(|e| CommandError::SerializationError(e.to_string()))?;
                        fs::write(&args.output_file, json_data).map_err(|err| CommandError::FileWriteError {
                            file_path: args.output_file,
                            err,
                        })?;
                    },
                    Err(e) => eprintln!("SignOneSidedTransaction error! {e}"),
                }
            },

            SignOneSidedDepositMultisigTransaction(args) => {
                let metadata = fs::metadata(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file.clone(),
                    err,
                })?;
                let max_size = 10_000_000; // 10MB limit
                if metadata.len() > max_size {
                    return Err(CommandError::InvalidArgument("Input file too large".to_string()));
                }

                let data = fs::read_to_string(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file,
                    err,
                })?;
                let request = PrepareDepositMultisigTransactionResult::from_json(&data)?;

                let mut wallet_transaction_service = transaction_service.clone();
                let result = wallet_transaction_service
                    .sign_one_sided_deposit_multisig_transaction(request)
                    .await
                    .map_err(CommandError::TransactionServiceError);
                match result {
                    Ok(data) => {
                        let json_data = data
                            .to_json()
                            .map_err(|e| CommandError::SerializationError(e.to_string()))?;
                        fs::write(&args.output_file, json_data).map_err(|err| CommandError::FileWriteError {
                            file_path: args.output_file,
                            err,
                        })?;
                    },
                    Err(e) => eprintln!("SignOneSidedDepositMultisigTransaction error! {e}"),
                }
            },

            SignOneSidedWithdrawMultisigTransaction(args) => {
                let metadata = fs::metadata(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file.clone(),
                    err,
                })?;
                let max_size = 10_000_000; // 10MB limit
                if metadata.len() > max_size {
                    return Err(CommandError::InvalidArgument("Input file too large".to_string()));
                }

                let data = fs::read_to_string(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file,
                    err,
                })?;
                let request = PrepareWithdrawMultisigTransactionResult::from_json(&data)?;

                let mut wallet_transaction_service = transaction_service.clone();
                let result = wallet_transaction_service
                    .sign_one_sided_withdraw_multisig_transaction(request)
                    .await
                    .map_err(CommandError::TransactionServiceError);
                match result {
                    Ok(data) => {
                        let json_data = data
                            .to_json()
                            .map_err(|e| CommandError::SerializationError(e.to_string()))?;
                        fs::write(&args.output_file, json_data).map_err(|err| CommandError::FileWriteError {
                            file_path: args.output_file,
                            err,
                        })?;
                    },
                    Err(e) => eprintln!("SignOneSidedWithdrawMultisigTransaction error! {e}"),
                }
            },
            BroadcastSignedOneSidedTransaction(args) => {
                let data = fs::read_to_string(&args.input_file).map_err(|err| CommandError::FileReadError {
                    file_path: args.input_file,
                    err,
                })?;
                let request = SignedOneSidedTransactionResult::from_json(&data)?;

                let mut wallet_transaction_service = transaction_service.clone();
                let result = wallet_transaction_service
                    .broadcast_signed_one_sided_transaction(request)
                    .await
                    .map_err(CommandError::TransactionServiceError);
                match result {
                    Ok(mut ids) => {
                        debug!(
                            target: LOG_TARGET,
                            "broadcast-signed-one-sided-transaction concluded with tx_id {:?}", ids
                        );
                        println!("Transaction ID: {:?}", ids);
                        tx_ids.append(&mut ids);
                    },
                    Err(e) => eprintln!("BroadcastSignedOneSidedTransaction error! {e}"),
                }
            },
            SignMessage(args) => {
                let mut commitment_bytes = [0u8; 32];
                let hex_msg = args.message.trim().trim_start_matches("0x");
                let msg_bytes = Vec::<u8>::from_hex(hex_msg)
                    .map_err(|e| CommandError::General(format!("message must be 32-byte hex: {}", e)))?;
                commitment_bytes.clone_from_slice(&msg_bytes);
                let sender_offset = args.sender_offset_key.as_ref().map(|pk| &pk.0);

                let signature = key_manager_service.sign_message_with_spend_key(&commitment_bytes, sender_offset)?;

                if let Some(file) = args.output_file {
                    if let Some(parent) = file.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| CommandError::JsonFile(format!("Failed to create directory: {}", e)))?;
                    }

                    let signature_binary = tari_utilities::message_format::MessageFormat::to_binary(&signature)
                        .map_err(|e| CommandError::General(format!("Failed to convert signature to binary: {}", e)))?;
                    let signature_json = serde_json::to_string(&signature_binary.to_hex())
                        .map_err(|e| CommandError::JsonFile(e.to_string()))?;

                    fs::write(&file, signature_json)
                        .map_err(|err| CommandError::FileWriteError { file_path: file, err })?;
                }
            },

            SignScriptMessage(args) => {
                let mut commitment_bytes = [0u8; 32];
                let hex_msg = args.message.trim().trim_start_matches("0x");
                let msg_bytes = Vec::<u8>::from_hex(hex_msg)
                    .map_err(|e| CommandError::General(format!("message must be 32-byte hex: {}", e)))?;
                commitment_bytes.clone_from_slice(&msg_bytes);

                let sender_offset = args.sender_offset_key.as_ref().map(|pk| &pk.0);
                let signature =
                    key_manager_service.sign_script_message_with_spend_key(&commitment_bytes, sender_offset)?;

                if let Some(file) = args.output_file {
                    if let Some(parent) = file.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| CommandError::JsonFile(format!("Failed to create directory: {}", e)))?;
                    }

                    let signature_binary = tari_utilities::message_format::MessageFormat::to_binary(&signature)
                        .map_err(|e| CommandError::General(format!("Failed to convert signature to binary: {}", e)))?;
                    let signature_json = serde_json::to_string(&signature_binary.to_hex())
                        .map_err(|e| CommandError::JsonFile(e.to_string()))?;

                    fs::write(&file, signature_json)
                        .map_err(|err| CommandError::FileWriteError { file_path: file, err })?;
                }
            },
            RescanWallet(args) => {
                if args.from_height == 0 {
                    wallet
                        .db
                        .clear_scanned_blocks()
                        .map_err(|e| CommandError::General(format!("{e}")))?;
                } else {
                    wallet
                        .db
                        .clear_scanned_blocks_from_and_higher(args.from_height)
                        .map_err(|e| CommandError::General(format!("{e}")))?;
                }
            },
            ExportAudit(args) => {
                match transaction_service
                    .get_completed_transactions(None, None, None, 0)
                    .await
                {
                    Ok(txs) => {
                        let filtered: Vec<_> = txs
                            .into_iter()
                            .filter(|tx| {
                                let ts = tx.mined_timestamp.unwrap_or(tx.timestamp);
                                if let Some(start) = args.start_date &&
                                    ts < start
                                {
                                    return false;
                                }
                                if let Some(end) = args.end_date &&
                                    ts > end
                                {
                                    return false;
                                }
                                true
                            })
                            .collect();
                        println!("Exporting {} transaction(s) to audit CSV...", filtered.len());
                        match write_audit_to_csv_file(filtered, args.output_file, args.conversion_rate, &args.currency)
                        {
                            Ok(()) => println!("Audit export complete."),
                            Err(e) => eprintln!("ExportAudit error! {e}"),
                        }
                    },
                    Err(e) => eprintln!("ExportAudit error! {e}"),
                }
            },
            DebugTransaction(args) => match transaction_service.get_completed_transaction(args.tx_id.into()).await {
                Ok(completed_tx) => {
                    println!("--- Completed Transaction ---");
                    println!("{:#?}", completed_tx);

                    match output_service.fetch_outputs_by_tx_id(args.tx_id.into()).await {
                        Ok(db_outputs) => {
                            let (input_outputs, received_outputs): (Vec<_>, Vec<_>) = db_outputs
                                .into_iter()
                                .partition(|o| o.spent_in_tx_id == Some(args.tx_id.into()));

                            println!(
                                "\n--- Inputs ({} DbWalletOutputs spent in this tx) ---",
                                input_outputs.len()
                            );
                            for (i, output) in input_outputs.iter().enumerate() {
                                println!("\nInput #{}", i.saturating_add(1));
                                println!("{:#?}", output);
                            }

                            println!(
                                "\n--- Outputs ({} DbWalletOutputs received in this tx) ---",
                                received_outputs.len()
                            );
                            for (i, output) in received_outputs.iter().enumerate() {
                                println!("\nOutput #{}", i.saturating_add(1));
                                println!("{:#?}", output);
                            }
                        },
                        Err(e) => eprintln!("DebugTransaction error fetching outputs: {e}"),
                    }
                },
                Err(e) => eprintln!("DebugTransaction error! Could not find completed transaction: {e}"),
            },
            ValidateTransaction(args) => {
                let tx_id: TxId = args.tx_id.into();
                match transaction_service.get_completed_transaction(tx_id).await {
                    Ok(completed_tx) => {
                        let has_signature = completed_tx.transaction_signature != CompressedSignature::default();
                        let num_confirmations_required = config.transaction_service_config.num_confirmations_required;
                        println!("--- Validate Transaction {} ---", tx_id);
                        println!("Current status: {}", completed_tx.status);
                        println!("Lock height: {}", completed_tx.lock_height);
                        if has_signature {
                            println!("Transaction has a signature, validating via base node query...");
                            let client = wallet.wallet_connectivity.obtain_base_node_wallet_rpc_client().await;
                            match client.get_tip_info().await {
                                Ok(tip_info) => {
                                    let tip = tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0);
                                    println!("Current chain tip height: {}", tip);
                                    let sig = &completed_tx.transaction_signature;
                                    match client
                                        .transaction_query(
                                            sig.get_compressed_public_nonce().as_bytes().to_vec(),
                                            sig.get_signature().as_bytes().to_vec(),
                                        )
                                        .await
                                    {
                                        Ok(response) => {
                                            if response.location == TxLocation::Mined {
                                                if let Some(mined_height) = response.mined_height {
                                                    let num_confirmations = tip.saturating_sub(mined_height);
                                                    println!("Transaction is MINED at height {}", mined_height);
                                                    println!("Confirmations: {}", num_confirmations);
                                                    if let Some(ref hash) = response.mined_header_hash {
                                                        println!("Mined in block: {}", hash.to_hex());
                                                    }
                                                    if let Some(ts) = response.mined_timestamp {
                                                        println!("Mined timestamp: {}", ts);
                                                    }

                                                    let is_confirmed = num_confirmations >= num_confirmations_required;
                                                    let is_locked = completed_tx.lock_height > tip;
                                                    let expected_status = if is_confirmed {
                                                        if is_locked {
                                                            completed_tx.status.mined_confirm_locked()
                                                        } else {
                                                            completed_tx.status.mined_confirm()
                                                        }
                                                    } else {
                                                        completed_tx.status.mined_unconfirm()
                                                    };

                                                    println!("Locked: {}", is_locked);
                                                    println!("Confirmed: {}", is_confirmed);

                                                    if completed_tx.status == expected_status {
                                                        println!("OK: Status '{}' is correct", completed_tx.status);
                                                    } else {
                                                        println!(
                                                            "FIX: Status mismatch! Wallet has '{}', expected '{}'. \
                                                             Updating...",
                                                            completed_tx.status, expected_status
                                                        );
                                                        let mined_in_block = response
                                                            .mined_header_hash
                                                            .and_then(|h| FixedHash::try_from(h.as_slice()).ok())
                                                            .unwrap_or_default();
                                                        let mined_ts = response.mined_timestamp.unwrap_or(0);
                                                        match transaction_service
                                                            .set_transaction_mined_height(
                                                                tx_id,
                                                                mined_height,
                                                                mined_in_block,
                                                                mined_ts,
                                                                expected_status,
                                                                tip,
                                                            )
                                                            .await
                                                        {
                                                            Ok(()) => println!("Status updated successfully"),
                                                            Err(e) => {
                                                                eprintln!("Error updating status: {e}")
                                                            },
                                                        }
                                                    }
                                                } else {
                                                    println!("Transaction is reported as mined but has no height");
                                                }
                                            } else {
                                                println!("Transaction is UNMINED (not found on chain)");
                                                if completed_tx.status.is_mined() {
                                                    println!(
                                                        "FIX: Wallet has status '{}' but chain says unmined. \
                                                         Updating...",
                                                        completed_tx.status
                                                    );
                                                    match transaction_service.set_transaction_as_unmined(tx_id).await {
                                                        Ok(()) => println!("Status updated to unmined"),
                                                        Err(e) => eprintln!("Error updating status: {e}"),
                                                    }
                                                }
                                            }
                                        },
                                        Err(e) => eprintln!("Error querying base node: {e}"),
                                    }
                                },
                                Err(e) => eprintln!("Error getting tip info: {e}"),
                            }
                        } else {
                            println!(
                                "Transaction has no signature (detected/imported), validating via output manager..."
                            );
                            match output_service.get_output_info_for_tx_id(tx_id).await {
                                Ok(output_info) => {
                                    println!("Output info: {:?}", output_info);
                                    if let (Some(mined_height), Some(block_hash)) =
                                        (output_info.mined_height, output_info.block_hash)
                                    {
                                        let client =
                                            wallet.wallet_connectivity.obtain_base_node_wallet_rpc_client().await;
                                        let tip = match client.get_tip_info().await {
                                            Ok(tip_info) => {
                                                tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0)
                                            },
                                            Err(e) => {
                                                eprintln!("Error getting tip info: {e}");
                                                0
                                            },
                                        };
                                        let num_confirmations = tip.saturating_sub(mined_height);
                                        println!("Transaction outputs MINED at height {}", mined_height);
                                        println!("Mined in block: {}", block_hash.to_hex());
                                        println!("Confirmations: {}", num_confirmations);
                                        println!("Current tip: {}", tip);

                                        let is_confirmed = num_confirmations >= num_confirmations_required;
                                        let is_locked = completed_tx.lock_height > tip;
                                        let expected_status = if is_confirmed {
                                            if is_locked {
                                                completed_tx.status.mined_confirm_locked()
                                            } else {
                                                completed_tx.status.mined_confirm()
                                            }
                                        } else {
                                            completed_tx.status.mined_unconfirm()
                                        };

                                        println!("Locked: {}", is_locked);
                                        println!("Confirmed: {}", is_confirmed);

                                        if completed_tx.status == expected_status {
                                            println!("OK: Status '{}' is correct", completed_tx.status);
                                        } else {
                                            println!(
                                                "FIX: Status mismatch! Wallet has '{}', expected '{}'. Updating...",
                                                completed_tx.status, expected_status
                                            );
                                            match transaction_service
                                                .set_transaction_mined_height(
                                                    tx_id,
                                                    mined_height,
                                                    block_hash,
                                                    0,
                                                    expected_status,
                                                    tip,
                                                )
                                                .await
                                            {
                                                Ok(()) => println!("Status updated successfully"),
                                                Err(e) => eprintln!("Error updating status: {e}"),
                                            }
                                        }
                                    } else {
                                        println!("Transaction outputs are NOT mined (not detected on chain)");
                                        if completed_tx.status.is_mined() {
                                            println!(
                                                "FIX: Wallet has status '{}' but outputs not mined. Updating...",
                                                completed_tx.status
                                            );
                                            match transaction_service.set_transaction_as_unmined(tx_id).await {
                                                Ok(()) => println!("Status updated to unmined"),
                                                Err(e) => eprintln!("Error updating status: {e}"),
                                            }
                                        }
                                    }
                                },
                                Err(e) => eprintln!("Error getting output info: {e}"),
                            }
                        }
                    },
                    Err(e) => eprintln!("ValidateTransaction error! Could not find completed transaction: {e}"),
                }
            },
            ValidateOutputs(args) => {
                use minotari_wallet::output_manager_service::storage::sqlite_db::{
                    ReceivedOutputInfoForBatch,
                    SpentOutputInfoForBatch,
                };

                println!("--- Validate and Fix Outputs ---");
                let client = wallet.wallet_connectivity.obtain_base_node_wallet_rpc_client().await;

                let tip_info = match client.get_tip_info().await {
                    Ok(info) => info,
                    Err(e) => {
                        eprintln!("Error getting tip info: {e}");
                        continue;
                    },
                };
                let tip_height = tip_info.metadata.map(|m| m.best_block_height()).unwrap_or(0);
                println!("Chain tip height: {}", tip_height);

                let num_confirmations_required = config.transaction_service_config.num_confirmations_required;

                let mut mined_updates = Vec::new();
                let mut spent_updates = Vec::new();
                let mut unmined_invalid = Vec::new();
                let mut unspent_updates = Vec::new();

                for hex in &args.commitments {
                    println!("\n--- Commitment: {} ---", hex);

                    let commitment = match CompressedCommitment::from_hex(hex) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Invalid commitment hex format: {}", e);
                            continue;
                        },
                    };

                    let db_output = match output_service
                        .get_outputs_by_commitments(vec![commitment.clone()])
                        .await
                    {
                        Ok(outputs) if !outputs.is_empty() => {
                            let output = outputs.into_iter().next().expect("checked not empty");
                            println!(
                                "Found in wallet DB - status: {}, hash: {}",
                                output.status,
                                output.hash.to_hex()
                            );
                            output
                        },
                        Ok(_) => {
                            println!("Output not found in wallet DB");
                            continue;
                        },
                        Err(e) => {
                            eprintln!("Error querying wallet DB: {}", e);
                            continue;
                        },
                    };

                    // Report wallet state
                    if let Some(mined_height) = db_output.mined_height {
                        println!("Wallet: mined at height {}", mined_height);
                        if let Some(block_hash) = db_output.mined_in_block {
                            println!("Wallet: mined in block {}", block_hash.to_hex());
                        }
                    } else {
                        println!("Wallet: unmined");
                    }

                    if let Some(height) = db_output.marked_deleted_at_height {
                        println!("Wallet: marked deleted at height {}", height);
                    }

                    if let Some(tx_id) = db_output.received_in_tx_id {
                        println!("Wallet: received in tx_id {}", tx_id);
                    }
                    if let Some(tx_id) = db_output.spent_in_tx_id {
                        println!("Wallet: spent in tx_id {}", tx_id);
                    }

                    // Validate against base node - mined info
                    let output_hash = db_output.hash.to_vec();
                    match client.get_utxos_mined_info(vec![output_hash.clone()], 2).await {
                        Ok(response) => {
                            let found_in_utxos = response.utxos.iter().find(|u| u.utxo_hash == output_hash);
                            let found_in_mempool = response.mempool_utxos.contains(&output_hash);

                            if let Some(mined_info) = found_in_utxos {
                                let mined_height = mined_info.mined_in_height;
                                let confirmations = tip_height.saturating_sub(mined_height);
                                let confirmed = confirmations >= num_confirmations_required;
                                println!(
                                    "Chain: MINED at height {} (confirmations: {}, confirmed: {})",
                                    mined_height, confirmations, confirmed
                                );
                                println!("Chain: mined in block {}", mined_info.mined_in_hash.to_hex());

                                match db_output.mined_height {
                                    Some(wallet_height) if wallet_height != mined_height => {
                                        let block_hash = FixedHash::try_from(mined_info.mined_in_hash.as_slice())
                                            .unwrap_or_default();
                                        mined_updates.push(ReceivedOutputInfoForBatch {
                                            commitment: commitment.clone(),
                                            mined_height,
                                            mined_in_block: block_hash,
                                            confirmed,
                                            mined_timestamp: mined_info.mined_in_timestamp,
                                        });
                                        println!(
                                            "FIX: wallet mined_height ({}) differs from chain ({}), updating",
                                            wallet_height, mined_height
                                        );
                                    },
                                    None => {
                                        let block_hash = FixedHash::try_from(mined_info.mined_in_hash.as_slice())
                                            .unwrap_or_default();
                                        mined_updates.push(ReceivedOutputInfoForBatch {
                                            commitment: commitment.clone(),
                                            mined_height,
                                            mined_in_block: block_hash,
                                            confirmed,
                                            mined_timestamp: mined_info.mined_in_timestamp,
                                        });
                                        println!(
                                            "FIX: wallet reports unmined but chain says mined at {}, updating",
                                            mined_height
                                        );
                                    },
                                    _ => {
                                        println!("OK: wallet mined_height matches chain");
                                    },
                                }
                            } else if found_in_mempool {
                                println!("Chain: IN MEMPOOL (not yet mined)");
                            } else {
                                println!("Chain: NOT FOUND in UTXO set or mempool");
                                if db_output.mined_height.is_some() {
                                    unmined_invalid.push(db_output.hash);
                                    println!(
                                        "FIX: wallet reports mined but chain says not found, marking as unmined and \
                                         invalid"
                                    );
                                }
                            }
                        },
                        Err(e) => eprintln!("Error querying base node for mined info: {e}"),
                    }

                    // Check if spent
                    match client.query_deleted_utxos(vec![output_hash], vec![]).await {
                        Ok(response) => {
                            if let Some(deleted_info) = response.utxos.first() {
                                match (&deleted_info.found_in_header, &deleted_info.spent_in_header) {
                                    (Some((found_height, _)), Some((spent_height, spent_hash))) => {
                                        let confirmations = tip_height.saturating_sub(*spent_height);
                                        let confirmed = confirmations >= num_confirmations_required;
                                        println!("Chain: output found at height {}", found_height);
                                        println!(
                                            "Chain: SPENT at height {} in block {} (confirmations: {}, confirmed: {})",
                                            spent_height,
                                            spent_hash.to_hex(),
                                            confirmations,
                                            confirmed
                                        );

                                        match db_output.marked_deleted_at_height {
                                            Some(wallet_height) if wallet_height != *spent_height => {
                                                let block_hash =
                                                    FixedHash::try_from(spent_hash.as_slice()).unwrap_or_default();
                                                spent_updates.push(SpentOutputInfoForBatch {
                                                    commitment: commitment.clone(),
                                                    confirmed,
                                                    mark_deleted_at_height: *spent_height,
                                                    mark_deleted_in_block: block_hash,
                                                });
                                                println!(
                                                    "FIX: wallet deleted_at_height ({}) differs from chain ({}), \
                                                     updating",
                                                    wallet_height, spent_height
                                                );
                                            },
                                            None => {
                                                let block_hash =
                                                    FixedHash::try_from(spent_hash.as_slice()).unwrap_or_default();
                                                spent_updates.push(SpentOutputInfoForBatch {
                                                    commitment: commitment.clone(),
                                                    confirmed,
                                                    mark_deleted_at_height: *spent_height,
                                                    mark_deleted_in_block: block_hash,
                                                });
                                                println!(
                                                    "FIX: wallet reports not deleted but chain says spent at {}, \
                                                     updating",
                                                    spent_height
                                                );
                                            },
                                            _ => {
                                                println!("OK: wallet spent state matches chain");
                                            },
                                        }
                                    },
                                    (Some((found_height, _)), None) => {
                                        println!("Chain: output found at height {} and NOT spent", found_height);
                                        if db_output.marked_deleted_at_height.is_some() {
                                            let confirmed = db_output
                                                .mined_height
                                                .map(|h| tip_height.saturating_sub(h) >= num_confirmations_required)
                                                .unwrap_or(false);
                                            unspent_updates.push((db_output.hash, confirmed));
                                            println!(
                                                "FIX: wallet reports deleted but chain says not spent, marking as \
                                                 unspent"
                                            );
                                        }
                                    },
                                    (None, _) => {
                                        println!("Chain: output NOT FOUND in deleted UTXO query");
                                    },
                                }
                            }
                        },
                        Err(e) => eprintln!("Error querying base node for deleted info: {e}"),
                    }
                }

                // Apply collected fixes
                if !mined_updates.is_empty() ||
                    !spent_updates.is_empty() ||
                    !unmined_invalid.is_empty() ||
                    !unspent_updates.is_empty()
                {
                    println!(
                        "\nApplying fixes: mined={}, spent={}, unmined_invalid={}, unspent={}",
                        mined_updates.len(),
                        spent_updates.len(),
                        unmined_invalid.len(),
                        unspent_updates.len()
                    );
                    match output_service
                        .update_output_validation_state(mined_updates, spent_updates, unmined_invalid, unspent_updates)
                        .await
                    {
                        Ok(()) => println!("Fixes applied successfully"),
                        Err(e) => eprintln!("Error applying fixes: {e}"),
                    }
                } else {
                    println!("\nNo fixes needed - all outputs match chain state");
                }
            },
            RevalidateAllTransactions => {
                println!("--- Revalidate All Transactions ---");
                match transaction_service.revalidate_all_transactions().await {
                    Ok(()) => println!("Transaction revalidation started successfully"),
                    Err(e) => eprintln!("RevalidateAllTransactions error: {e}"),
                }
            },
            RevalidateAllOutputs => {
                println!("--- Revalidate All Outputs ---");
                match output_service.revalidate_all_outputs().await {
                    Ok(request_key) => {
                        println!(
                            "Output revalidation started successfully (request key: {})",
                            request_key
                        )
                    },
                    Err(e) => eprintln!("RevalidateAllOutputs error: {e}"),
                }
            },
        }
    }

    // listen to event stream
    if tx_ids.is_empty() {
        trace!(
            target: LOG_TARGET,
            "Wallet command runner - no transactions to monitor."
        );
    } else {
        let duration = config.command_send_wait_timeout;
        debug!(
            target: LOG_TARGET,
            "wallet monitor_transactions timeout duration {duration:.2?}"
        );
        match timeout(
            duration,
            monitor_transactions(transaction_service.clone(), tx_ids, wait_stage),
        )
        .await
        {
            Ok(txs) => {
                debug!(
                    target: LOG_TARGET,
                    "monitor_transactions done to stage {wait_stage:?} with tx_ids: {txs:?}"
                );
                println!("Done! All transactions monitored to {wait_stage:?} stage.");
            },
            Err(_e) => {
                println!(
                    "The configured timeout ({duration:#?}) was reached before all transactions reached the \
                     {wait_stage:?} stage. See the logs for more info."
                );
            },
        }
    }

    Ok(unban_peer_manager_peers)
}

/// A random alphanumeric string of `len` characters.
///
/// Uses `ThreadRng`, a cryptographically secure generator seeded from the operating system and periodically reseeded
/// from it. That is what makes this suitable for generating a password.
fn random_alphanumeric(len: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

/// Owns a temporary wallet directory and removes it, and everything in it, when it goes out of scope.
///
/// The paper wallet import writes a wallet database containing the recovered master seed into this directory, so it
/// must not outlive the command - not when it returns early with an error, and not when it unwinds.
///
/// On the path where the command succeeds, call [`TempWalletDir::remove`] instead of letting the guard drop: a
/// failure to delete a database holding the master seed must not be reported as success. `Drop` is the backstop for
/// the error and unwind paths, where there is no result to propagate into, and can only log.
struct TempWalletDir {
    path: PathBuf,
}

impl TempWalletDir {
    /// Creates the directory (owner-only where the platform supports it) and takes ownership of it.
    fn create(path: PathBuf) -> Result<Self, CommandError> {
        create_owner_only_dir(&path)?;
        Ok(Self { path })
    }

    /// Removes the directory and everything in it, propagating any failure to the caller.
    ///
    /// The subsequent `Drop` finds the directory gone and does nothing, so this never removes twice.
    fn remove(self) -> Result<(), CommandError> {
        if !self.path.exists() {
            return Ok(());
        }
        println!("removing temp wallet in: {:?}", self.path);
        fs::remove_dir_all(&self.path).map_err(|e| {
            CommandError::General(format!(
                "Could not remove the temporary wallet directory '{}': {}. It contains wallet keys and should be \
                 deleted manually.",
                self.path.display(),
                e
            ))
        })
    }
}

impl Drop for TempWalletDir {
    fn drop(&mut self) {
        if !self.path.exists() {
            return;
        }
        println!("removing temp wallet in: {:?}", self.path);
        if let Err(e) = fs::remove_dir_all(&self.path) {
            error!(
                target: LOG_TARGET,
                "Could not remove temporary wallet directory '{}': {}", self.path.display(), e
            );
            eprintln!(
                "Could not remove temporary wallet directory '{}': {}. It contains wallet keys and should be deleted \
                 manually.",
                self.path.display(),
                e
            );
        }
    }
}

/// Creates `path`, and any missing parents, so that only the owner can read it (0700). The directory holds a wallet
/// database, so it must never be group- or world-readable.
///
/// The final component is created non-recursively, so this fails if it already exists. That is deliberate: the
/// directory must be a fresh one, never a leftover from an earlier crash or one planted by another user. `mkdir` is
/// subject to the umask, so the mode is set explicitly afterwards.
#[cfg(target_family = "unix")]
fn create_owner_only_dir(path: &Path) -> Result<(), CommandError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::DirBuilder::new().mode(0o700).create(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(target_family = "unix"))]
fn create_owner_only_dir(path: &Path) -> Result<(), CommandError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::DirBuilder::new().create(path)?;
    Ok(())
}

async fn detect_tx_metadata(wallet: &WalletSqlite, destination: &TariAddress) -> TxType {
    if let Ok(interactive_address) = wallet.get_wallet_interactive_address() {
        if let Ok(one_sided_address) = wallet.get_wallet_one_sided_address() {
            if *destination == interactive_address || *destination == one_sided_address {
                TxType::PaymentToSelf
            } else {
                TxType::PaymentToOther
            }
        } else if *destination == interactive_address {
            TxType::PaymentToSelf
        } else {
            TxType::PaymentToOther
        }
    } else {
        TxType::PaymentToOther
    }
}

fn read_genesis_file_outputs(
    use_pre_mine_input_file: bool,
    pre_mine_file_path: Option<PathBuf>,
) -> Result<Option<Vec<TransactionOutput>>, String> {
    if use_pre_mine_input_file {
        let file_path = if let Some(path) = pre_mine_file_path {
            let file = path.join(get_pre_mine_file_name());
            if !file.exists() {
                return Err(format!("Pre-mine file '{}' does not exist!", file.display()));
            }
            file
        } else {
            return Err("Missing pre-mine file! Need '--pre-mine-file-path <path_to_file>.'".to_string());
        };

        let file = File::open(file_path.clone())
            .map_err(|e| format!("Problem opening file '{}' ({})", file_path.display(), e))?;
        let reader = BufReader::new(file);

        let mut outputs = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|e| format!("Problem reading line in file '{}' ({})", file_path.display(), e))?;
            if let Ok(output) = serde_json::from_str::<TransactionOutput>(&line) {
                outputs.push(output);
            } else if serde_json::from_str::<TransactionKernel>(&line).is_ok() {
                // Do nothing here
            } else {
                return Err(format!("Error: Could not deserialize line: {line}"));
            }
        }
        if outputs.is_empty() {
            return Err(format!("No outputs found in '{}'", file_path.display()));
        }

        Ok(Some(outputs))
    } else {
        Ok(None)
    }
}

fn get_pre_mine_file_name() -> String {
    match Network::get_current_or_user_setting_or_default() {
        Network::MainNet => "mainnet_pre_mine.json".to_string(),
        Network::StageNet => "stagenet_pre_mine.json".to_string(),
        Network::NextNet => "nextnet_pre_mine.json".to_string(),
        Network::LocalNet => "esmeralda_pre_mine.json".to_string(),
        Network::Igor => "igor_pre_mine.json".to_string(),
        Network::Esmeralda => "esmeralda_pre_mine.json".to_string(),
    }
}

fn verify_no_duplicate_indexes(recipient_info: &[CliRecipientInfo]) -> Result<(), String> {
    let mut all_indexes = recipient_info
        .iter()
        .flat_map(|v| v.output_indexes.clone())
        .collect::<Vec<_>>();
    all_indexes.sort();
    let all_indexes_len = all_indexes.len();
    all_indexes.dedup();
    if all_indexes_len == all_indexes.len() {
        Ok(())
    } else {
        Err(format!(
            "{}",
            max(all_indexes_len, all_indexes.len()).saturating_sub(min(all_indexes_len, all_indexes.len()))
        ))
    }
}

fn sort_args_recipient_info(recipient_info: Vec<CliRecipientInfo>) -> Vec<CliRecipientInfo> {
    let mut args_recipient_info = recipient_info;
    args_recipient_info.sort_by_key(|a| a.recipient_address.to_hex());
    args_recipient_info.iter_mut().for_each(|v| v.output_indexes.sort());
    args_recipient_info
}

fn get_embedded_pre_mine_outputs(
    output_indexes: Vec<usize>,
    utxos: Option<Vec<TransactionOutput>>,
) -> Result<Vec<TransactionOutput>, CommandError> {
    let utxos = if let Some(val) = utxos {
        val
    } else {
        get_all_embedded_pre_mine_outputs()?
    };
    let mut fetched_outputs = Vec::with_capacity(output_indexes.len());
    for index in output_indexes {
        if index >= utxos.len() {
            return Err(CommandError::PreMine(format!(
                "Error: Invalid 'output_index' {} provided, pre-mine outputs only number {}!",
                index,
                utxos.len()
            )));
        }
        fetched_outputs.push(utxos.get(index).expect("Already checked").clone());
    }
    Ok(fetched_outputs)
}

fn get_all_embedded_pre_mine_outputs() -> Result<Vec<TransactionOutput>, CommandError> {
    let pre_mine_contents = get_embedded_pre_mine_json(Network::get_current_or_user_setting_or_default());
    let mut utxos = Vec::new();
    let lines_count = pre_mine_contents.lines().count();
    for (counter, line) in (1..).zip(pre_mine_contents.lines()) {
        if counter < lines_count {
            let utxo: Option<TransactionOutput> = serde_json::from_str(line).ok();
            if let Some(utxo) = utxo {
                utxos.push(utxo);
            }
        } else {
            break;
        }
    }

    Ok(utxos)
}

fn write_utxos_to_csv_file(
    utxos: Vec<(UnblindedOutput, CompressedCommitment)>,
    file_path: PathBuf,
    with_private_keys: bool,
) -> Result<(), CommandError> {
    let file = File::create(file_path).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    let mut csv_file = LineWriter::new(file);
    writeln!(
        csv_file,
        r##""index","version","value","spending_key","commitment","output_type","maturity","coinbase_extra","script","covenant","input_data","script_private_key","sender_offset_public_key","ephemeral_commitment","ephemeral_nonce","signature_u_x","signature_u_a","signature_u_y","script_lock_height","encrypted_data","minimum_value_promise","range_proof""##
    )
        .map_err(|e| CommandError::CSVFile(e.to_string()))?;
    for (i, (utxo, commitment)) in utxos.iter().enumerate() {
        writeln!(
            csv_file,
            r##""{}","V{}","{}","{}","{}","{:?}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}","{}""##,
            i.saturating_add(1),
            utxo.version.as_u8(),
            utxo.value.0,
            if with_private_keys { utxo.commitment_mask_key.to_hex() } else { "*hidden*".to_string() },
            commitment.to_hex(),
            utxo.features.output_type,
            utxo.features.maturity,
            String::from_utf8(utxo.features.coinbase_extra.to_vec())
                .unwrap_or_else(|_| utxo.features.coinbase_extra.to_hex()),
            utxo.script.to_hex(),
            utxo.covenant.to_bytes().to_hex(),
            utxo.input_data.to_hex(),
            if with_private_keys { utxo.script_private_key.to_hex() } else { "*hidden*".to_string() },
            utxo.sender_offset_public_key.to_hex(),
            utxo.metadata_signature.ephemeral_commitment().to_hex(),
            utxo.metadata_signature.ephemeral_pubkey().to_hex(),
            utxo.metadata_signature.u_x().to_hex(),
            utxo.metadata_signature.u_a().to_hex(),
            utxo.metadata_signature.u_y().to_hex(),
            utxo.script_lock_height,
            utxo.encrypted_data.to_byte_vec().to_hex(),
            utxo.minimum_value_promise.as_u64(),
            if let Some(proof) = utxo.range_proof.clone() {
                proof.to_hex()
            } else {
                "".to_string()
            },
        )
            .map_err(|e| CommandError::CSVFile(e.to_string()))?;
        debug!(
            target: LOG_TARGET,
            "UTXO {} exported: {:?}",
            i.saturating_add(1),
            utxo
        );
    }
    Ok(())
}

fn write_tx_to_csv_file(tx: WalletTransaction, file_path: PathBuf) -> Result<(), CommandError> {
    let file = File::create(file_path).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    let mut csv_file = LineWriter::new(file);
    let tx_string = serde_json::to_string(&tx).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    writeln!(csv_file, "{tx_string}").map_err(|e| CommandError::CSVFile(e.to_string()))?;

    Ok(())
}

fn load_tx_from_csv_file(file_path: PathBuf) -> Result<Vec<WalletTransaction>, CommandError> {
    let file_contents = fs::read_to_string(file_path).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    let mut results = Vec::new();
    for line in file_contents.lines() {
        if let Ok(tx) = serde_json::from_str(line) {
            results.push(tx);
        } else {
            return Err(CommandError::CSVFile("Could not read json file".to_string()));
        }
    }
    Ok(results)
}

fn write_audit_to_csv_file(
    transactions: Vec<CompletedTransaction>,
    file_path: PathBuf,
    conversion_rate: Option<f64>,
    currency: &str,
) -> Result<(), CommandError> {
    use tari_common_types::transaction::TransactionDirection;

    let file = File::create(file_path).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    let mut csv_file = LineWriter::new(file);

    // Write header
    writeln!(
        csv_file,
        r#""ID","Transaction Hash","Status","Transaction Type","DateTime (UTC)","From Address","To Address","Amount","AmountTicker","Amount ({currency})","Txn Fee","FeeTicker","Fee ({currency})""#,
        currency = currency,
    )
    .map_err(|e| CommandError::CSVFile(e.to_string()))?;

    for tx in &transactions {
        // Determine transaction type: Deposit = inbound/coinbase, Withdraw = outbound
        let tx_type = if tx.direction == TransactionDirection::Inbound || tx.status.is_coinbase() {
            "Deposit"
        } else {
            "Withdraw"
        };

        // Human-readable status
        let status = format!("{}", tx.status);

        // Transaction hash: use the excess signature nonce as the hash (first kernel)
        let tx_hash = tx
            .transaction
            .body
            .kernels()
            .first()
            .map(|k| format!("0x{}", k.hash().to_hex()))
            .unwrap_or_else(|| "N/A".to_string());

        // DateTime: prefer mined timestamp, fall back to creation timestamp
        let datetime = tx.mined_timestamp.unwrap_or(tx.timestamp);
        let datetime_str = datetime.format("%-m/%-d/%y %-H:%M").to_string();

        // Amount in base units (MicroMinotari) → display as Minotari
        let amount_minotari = tx.amount.as_u64() as f64 / 1_000_000.0;
        let fee_minotari = tx.fee.as_u64() as f64 / 1_000_000.0;

        // Fiat conversion
        let (amount_fiat, fee_fiat) = if let Some(rate) = conversion_rate {
            (
                format!("{}{:.2}", currency, amount_minotari * rate),
                format!("{}{:.2}", currency, fee_minotari * rate),
            )
        } else {
            ("N/A".to_string(), "N/A".to_string())
        };

        writeln!(
            csv_file,
            r#""{tx_id}","{tx_hash}","{status}","{tx_type}","{datetime}","{from_addr}","{to_addr}","{amount:.8}","XTM","{amount_fiat}","{fee:.8}","XTM","{fee_fiat}""#,
            tx_id = tx.tx_id,
            tx_hash = tx_hash,
            status = status,
            tx_type = tx_type,
            datetime = datetime_str,
            from_addr = tx.source_address,
            to_addr = tx.destination_address,
            amount = amount_minotari,
            amount_fiat = amount_fiat,
            fee = fee_minotari,
            fee_fiat = fee_fiat,
        )
        .map_err(|e| CommandError::CSVFile(e.to_string()))?;
    }

    Ok(())
}

/// What step 4 prints when the device's legacy used-nonce record is full.
const STEP_4_STORE_FULL_MESSAGE: &str = "The Ledger device's record of used pre-mine nonces is full (it holds one app \
                                         run's worth: 32 outputs). Progress is saved. Restart the Minotari Wallet app \
                                         on the Ledger device, then re-run this same step 4 command; it will continue \
                                         from the next output.";

/// Whether a signing error is the device refusing because its legacy used-nonce record is full.
fn is_legacy_nonce_store_full(error: &impl std::fmt::Display) -> bool {
    error.to_string().contains("LegacyNonceStoreFull")
}

/// How many pre-mine outputs one run of the device app can sign in step 4: two legacy signatures each.
const STEP_4_OUTPUTS_PER_APP_RUN: usize = minotari_ledger_wallet_common::legacy_nonce::LEGACY_NONCE_RECORD_SIZE / 2;

/// How many times the device app will have to be restarted for step 4 to sign `outputs` outputs.
fn step_4_app_restarts_needed(outputs: usize) -> usize {
    outputs.div_ceil(STEP_4_OUTPUTS_PER_APP_RUN).saturating_sub(1)
}

fn warn_if_step_4_needs_app_restarts(outputs: usize) {
    let restarts = step_4_app_restarts_needed(outputs);
    if restarts > 0 {
        println!(
            "\nWarning: this session has {outputs} outputs. On a Ledger device step 4 can sign \
             {STEP_4_OUTPUTS_PER_APP_RUN} outputs per run of the Minotari Wallet app, so step 4 will stop {restarts} \
             time(s) and ask you to restart the app on the device and re-run the same command; it continues where it \
             stopped.\n"
        );
    }
}

/// Load step 4's progress file: the outputs a previous run already signed. A missing file means nothing was signed.
///
/// Line one is the session header, as in every session file; each further line is one signed output. An entry for an
/// output that is not this party's, or a second entry for one output, is refused as a damaged or edited file - better
/// to stop than to guess which signature is the real one.
fn load_step_4_progress(
    path: &Path,
    session_info: &PreMineSpendStep1SessionInfo,
    party_output_indexes: &[usize],
) -> Result<Vec<Step4OutputsForLeader>, String> {
    let describe = |e: &dyn std::fmt::Display| format!("step 4 progress file '{}': {e}", path.display());
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(describe(&e)),
    };
    let mut lines = contents.lines().filter(|line| !line.trim().is_empty());
    let Some(header) = lines.next() else {
        return Ok(Vec::new());
    };
    let header: PreMineSpendStep1SessionInfo = serde_json::from_str(header).map_err(|e| describe(&e))?;
    if &header != session_info {
        return Err(describe(&"it belongs to a different session"));
    }
    let mut signed: Vec<Step4OutputsForLeader> = Vec::new();
    for line in lines {
        let output: Step4OutputsForLeader = serde_json::from_str(line).map_err(|e| describe(&e))?;
        if !party_output_indexes.contains(&output.output_index) {
            return Err(describe(&format!(
                "output {} is not one of this party's",
                output.output_index
            )));
        }
        if signed.iter().any(|done| done.output_index == output.output_index) {
            return Err(describe(&format!("output {} is recorded twice", output.output_index)));
        }
        signed.push(output);
    }
    Ok(signed)
}

/// Append one signed output to step 4's progress file, writing the session header first if the file is new.
fn append_step_4_progress(
    path: &Path,
    session_info: &PreMineSpendStep1SessionInfo,
    output: &Step4OutputsForLeader,
) -> Result<(), CommandError> {
    if !path.exists() {
        write_json_object_to_file_as_line(path, true, session_info)?;
    }
    write_json_object_to_file_as_line(path, false, output)
}

/// Put the signed outputs in this party's output order for the final step 4 file, refusing if any is missing.
fn merge_step_4_outputs(
    party_output_indexes: &[usize],
    signed: Vec<Step4OutputsForLeader>,
) -> Result<Vec<Step4OutputsForLeader>, String> {
    let mut merged = Vec::with_capacity(party_output_indexes.len());
    for index in party_output_indexes {
        match signed.iter().find(|done| done.output_index == *index) {
            Some(done) => merged.push(done.clone()),
            None => return Err(format!("output {index} has not been signed")),
        }
    }
    Ok(merged)
}

/// Every nonce key id a pre-mine step 2 self file names must be distinct, within and across outputs.
///
/// Two signatures under one legacy nonce give up the signing key, and with the step 2 script offset two different
/// keys under one nonce give up both. The device refuses such a pair too (its used-nonce record), but only within one
/// application run.
fn step_2_nonce_ids_are_unique(outputs: &[Step2OutputsForSelf]) -> Result<(), String> {
    let mut seen: Vec<&TariKeyId> = Vec::with_capacity(outputs.len().saturating_mul(2));
    for output in outputs {
        for nonce_id in [&output.script_nonce_key_id, &output.sender_offset_nonce_key_id] {
            if seen.contains(&nonce_id) {
                return Err(format!(
                    "the pre-mine step 2 self file names nonce '{nonce_id}' more than once (output {}). Signing with \
                     it twice would give up your pre-mine keys; redo step 2",
                    output.output_index
                ));
            }
            seen.push(nonce_id);
        }
    }
    Ok(())
}

/// Check a pre-mine step 2 self file is the shape step 2 writes, before step 4 signs anything with it.
///
/// - every nonce id is distinct ([`step_2_nonce_ids_are_unique`]);
/// - the script key is the `PreMine` key at the output's own genesis index, as step 2 derives it;
/// - the sender offset key is a `PreMine` key with the pre-mine sender offset bit set, as `get_script_offset` issues it
///   in pre-mine mode;
/// - every nonce id is a `Random` ledger key at an index the device will sign with (at or above 2^32);
/// - the public nonces step 2 published are stored, so step 4 can check each signature's nonce against them.
///
/// The file is unauthenticated, so this refuses shapes step 2 never writes; it cannot tell a nonce id from an earlier
/// session's from one of this session's. The device's used-nonce record (within one app run) is what stops a nonce
/// from being used twice.
fn validate_step_2_self_outputs(outputs: &[Step2OutputsForSelf]) -> Result<(), String> {
    use minotari_ledger_wallet_common::{
        legacy_nonce::MIN_LEGACY_NONCE_INDEX,
        script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT,
    };

    step_2_nonce_ids_are_unique(outputs)?;
    for output in outputs {
        let index = output.output_index;
        if output.public_script_nonce_key.is_none() || output.public_sender_offset_nonce_key.is_none() {
            return Err(format!(
                "the pre-mine step 2 self file for output {index} was written by an older build and does not record \
                 the public nonces it published; redo step 2"
            ));
        }
        let expected_script_key = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::PreMine,
            index: u64::try_from(index).map_err(|_| format!("output index {index} does not fit a key index"))?,
        };
        if output.pre_mine_script_key_id != expected_script_key {
            return Err(format!(
                "the pre-mine step 2 self file names script key '{}' for output {index}, but step 2 derives '{}'; the \
                 file has been changed - redo step 2",
                output.pre_mine_script_key_id, expected_script_key
            ));
        }
        match output.sender_offset_key_id {
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::PreMine,
                index: key_index,
            } if key_index & PRE_MINE_SENDER_OFFSET_INDEX_BIT != 0 => {},
            ref other => {
                return Err(format!(
                    "the pre-mine step 2 self file names sender offset key '{other}' for output {index}, which is not \
                     a pre-mine sender offset key; redo step 2"
                ));
            },
        }
        for nonce_id in [&output.script_nonce_key_id, &output.sender_offset_nonce_key_id] {
            match nonce_id {
                TariKeyId::LedgerKey {
                    branch: LedgerKeyBranch::Random,
                    index: nonce_index,
                } if *nonce_index >= MIN_LEGACY_NONCE_INDEX => {},
                other => {
                    return Err(format!(
                        "the pre-mine step 2 self file names nonce '{other}' for output {index}, which is not a nonce \
                         this device will sign with (a Random key at index 2^32 or above); redo step 2"
                    ));
                },
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    mod pre_mine_step_4_progress {
        use super::*;

        fn session() -> PreMineSpendStep1SessionInfo {
            PreMineSpendStep1SessionInfo {
                session_id: "abc".to_string(),
                ..Default::default()
            }
        }

        fn signed(output_index: usize) -> Step4OutputsForLeader {
            Step4OutputsForLeader {
                output_index,
                ..Default::default()
            }
        }

        #[test]
        fn a_missing_progress_file_means_nothing_was_signed() {
            let dir = tempfile::tempdir().unwrap();
            let loaded = load_step_4_progress(&dir.path().join("p.json"), &session(), &[1, 2]).unwrap();
            assert!(loaded.is_empty());
        }

        /// What is appended is what is loaded back, so a resumed run skips exactly the outputs already signed.
        #[test]
        fn appended_outputs_load_back_and_are_skipped() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("p.json");
            append_step_4_progress(&path, &session(), &signed(7)).unwrap();
            append_step_4_progress(&path, &session(), &signed(3)).unwrap();
            let loaded = load_step_4_progress(&path, &session(), &[3, 5, 7]).unwrap();
            assert_eq!(loaded, vec![signed(7), signed(3)]);
            assert!(loaded.iter().any(|done| done.output_index == 7));
            assert!(!loaded.iter().any(|done| done.output_index == 5));
        }

        #[test]
        fn a_progress_file_from_another_session_is_refused() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("p.json");
            append_step_4_progress(&path, &session(), &signed(7)).unwrap();
            let other = PreMineSpendStep1SessionInfo {
                session_id: "xyz".to_string(),
                ..Default::default()
            };
            assert!(load_step_4_progress(&path, &other, &[7]).is_err());
        }

        #[test]
        fn a_duplicate_or_foreign_output_in_progress_is_refused() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("p.json");
            append_step_4_progress(&path, &session(), &signed(7)).unwrap();
            append_step_4_progress(&path, &session(), &signed(7)).unwrap();
            assert!(
                load_step_4_progress(&path, &session(), &[7])
                    .unwrap_err()
                    .contains("twice")
            );

            let path = dir.path().join("q.json");
            append_step_4_progress(&path, &session(), &signed(9)).unwrap();
            assert!(
                load_step_4_progress(&path, &session(), &[7])
                    .unwrap_err()
                    .contains("not one of this party's")
            );
        }

        #[test]
        fn merging_puts_outputs_in_party_order_and_needs_all_of_them() {
            let merged = merge_step_4_outputs(&[3, 5, 7], vec![signed(7), signed(3), signed(5)]).unwrap();
            assert_eq!(merged, vec![signed(3), signed(5), signed(7)]);
            assert!(merge_step_4_outputs(&[3, 5], vec![signed(3)]).is_err());
        }

        #[test]
        fn app_restarts_needed_follow_the_record_size() {
            assert_eq!(STEP_4_OUTPUTS_PER_APP_RUN, 32);
            assert_eq!(step_4_app_restarts_needed(0), 0);
            assert_eq!(step_4_app_restarts_needed(32), 0);
            assert_eq!(step_4_app_restarts_needed(33), 1);
            assert_eq!(step_4_app_restarts_needed(65), 2);
        }

        #[test]
        fn the_store_full_error_is_recognised() {
            assert!(is_legacy_nonce_store_full(
                &"Ledger error: GetRawSchnorrSignatureLegacyNonce: LegacyNonceStoreFull - ..."
            ));
            assert!(!is_legacy_nonce_store_full(&"LegacyNonceReused"));
        }
    }

    mod pre_mine_step_2_self_file {
        use minotari_ledger_wallet_common::{
            legacy_nonce::MIN_LEGACY_NONCE_INDEX,
            script_offset::PRE_MINE_SENDER_OFFSET_INDEX_BIT,
        };
        use tari_common_types::types::CompressedPublicKey;

        use super::*;

        fn ledger_key(branch: LedgerKeyBranch, index: u64) -> TariKeyId {
            TariKeyId::LedgerKey { branch, index }
        }

        /// What step 2 writes for `output_index`, with nonces distinguished by `salt`.
        fn honest(output_index: usize, salt: u64) -> Step2OutputsForSelf {
            Step2OutputsForSelf {
                output_index,
                script_nonce_key_id: ledger_key(LedgerKeyBranch::Random, MIN_LEGACY_NONCE_INDEX | salt),
                sender_offset_nonce_key_id: ledger_key(
                    LedgerKeyBranch::Random,
                    MIN_LEGACY_NONCE_INDEX | (1 << 40) | salt,
                ),
                sender_offset_key_id: ledger_key(LedgerKeyBranch::PreMine, PRE_MINE_SENDER_OFFSET_INDEX_BIT | salt),
                pre_mine_script_key_id: ledger_key(LedgerKeyBranch::PreMine, output_index as u64),
                public_script_nonce_key: Some(CompressedPublicKey::default()),
                public_sender_offset_nonce_key: Some(CompressedPublicKey::default()),
                ..Default::default()
            }
        }

        #[test]
        fn an_honest_file_passes() {
            assert_eq!(validate_step_2_self_outputs(&[honest(3, 1), honest(9, 2)]), Ok(()));
        }

        /// The same-nonce, two-keys extraction, set up by editing the file.
        #[test]
        fn a_nonce_named_twice_within_an_output_is_refused() {
            let mut output = honest(3, 1);
            output.sender_offset_nonce_key_id = output.script_nonce_key_id.clone();
            let err = validate_step_2_self_outputs(&[output.clone()]).unwrap_err();
            assert!(err.contains("more than once"), "{err}");
            assert!(step_2_nonce_ids_are_unique(&[output]).is_err());
        }

        #[test]
        fn a_nonce_named_twice_across_outputs_is_refused() {
            let first = honest(3, 1);
            let mut second = honest(9, 2);
            second.script_nonce_key_id = first.sender_offset_nonce_key_id.clone();
            let err = validate_step_2_self_outputs(&[first, second]).unwrap_err();
            assert!(err.contains("more than once"), "{err}");
        }

        #[test]
        fn a_script_key_at_another_index_is_refused() {
            let mut output = honest(3, 1);
            output.pre_mine_script_key_id = ledger_key(LedgerKeyBranch::PreMine, 4);
            let err = validate_step_2_self_outputs(&[output]).unwrap_err();
            assert!(err.contains("script key"), "{err}");
        }

        #[test]
        fn a_sender_offset_key_without_the_pre_mine_bit_is_refused() {
            for key in [
                ledger_key(LedgerKeyBranch::PreMine, 5),
                ledger_key(
                    LedgerKeyBranch::OneSidedSenderOffset,
                    PRE_MINE_SENDER_OFFSET_INDEX_BIT | 5,
                ),
            ] {
                let mut output = honest(3, 1);
                output.sender_offset_key_id = key;
                let err = validate_step_2_self_outputs(&[output]).unwrap_err();
                assert!(err.contains("sender offset key"), "{err}");
            }
        }

        #[test]
        fn a_nonce_below_two_to_the_thirty_two_or_off_random_is_refused() {
            for nonce in [
                ledger_key(LedgerKeyBranch::Random, 7),
                ledger_key(LedgerKeyBranch::PreMine, MIN_LEGACY_NONCE_INDEX | 7),
            ] {
                let mut output = honest(3, 1);
                output.script_nonce_key_id = nonce;
                let err = validate_step_2_self_outputs(&[output]).unwrap_err();
                assert!(err.contains("not a nonce"), "{err}");
            }
        }

        /// A file from a build that did not store the published nonces cannot be checked, so it is refused.
        #[test]
        fn a_file_without_the_published_nonces_is_refused() {
            let mut output = honest(3, 1);
            output.public_sender_offset_nonce_key = None;
            let err = validate_step_2_self_outputs(&[output]).unwrap_err();
            assert!(err.contains("redo step 2"), "{err}");
        }

        /// The old file format still parses - so the refusal above is what the user sees, not a serde error.
        #[test]
        fn an_old_file_without_the_new_fields_still_parses() {
            let mut value = serde_json::to_value(honest(3, 1)).unwrap();
            let object = value.as_object_mut().unwrap();
            object.remove("public_script_nonce_key");
            object.remove("public_sender_offset_nonce_key");
            let parsed: Step2OutputsForSelf = serde_json::from_value(value).unwrap();
            assert_eq!(parsed.public_script_nonce_key, None);
        }
    }

    #[test]
    fn random_alphanumeric_is_the_requested_length_and_not_repeated() {
        let first = random_alphanumeric(32);
        let second = random_alphanumeric(32);
        assert_eq!(first.len(), 32);
        assert_eq!(second.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_alphanumeric()));
        // Two 32-character draws from a 62-symbol alphabet collide with probability 62^-32, i.e. about 1 in 10^57.
        // This is not a flaky assertion; it is the only one here that would catch a generator stubbed out to return a
        // constant, which is exactly the regression that would silently reinstate a fixed password.
        assert_ne!(first, second);
    }

    #[test]
    fn temp_wallet_dir_is_owner_only_and_removed_on_drop() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("temp-wallet");
        {
            let _guard = TempWalletDir::create(path.clone()).unwrap();
            assert!(path.is_dir());
            #[cfg(target_family = "unix")]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o700);
            }
            // The database (and anything else the wallet writes) must go with the directory.
            fs::write(path.join("console_wallet.db"), b"seed").unwrap();
        }
        assert!(!path.exists());
    }

    #[test]
    fn temp_wallet_dir_remove_deletes_the_directory_and_reports_success() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("temp-wallet");
        let guard = TempWalletDir::create(path.clone()).unwrap();
        fs::write(path.join("console_wallet.db"), b"seed").unwrap();

        guard.remove().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn temp_wallet_dir_is_never_created_over_an_existing_directory() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("temp-wallet");
        let _guard = TempWalletDir::create(path.clone()).unwrap();

        // A leftover directory must not be adopted - it could hold another run's seed database, or be planted.
        assert!(TempWalletDir::create(path).is_err());
    }

    #[test]
    fn temp_wallet_dir_is_removed_when_the_scope_exits_with_an_error() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().join("temp-wallet");

        fn fail(path: PathBuf) -> Result<(), CommandError> {
            let _guard = TempWalletDir::create(path)?;
            Err(CommandError::General("recovery failed".to_string()))
        }

        assert!(fail(path.clone()).is_err());
        assert!(!path.exists());
    }
}
