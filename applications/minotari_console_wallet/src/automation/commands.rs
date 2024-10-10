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
    fs,
    fs::File,
    io,
    io::{BufRead, BufReader, LineWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use digest::Digest;
use futures::FutureExt;
use log::*;
use minotari_app_grpc::tls::certs::{generate_self_signed_certs, print_warning, write_cert_to_disk};
use minotari_wallet::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::UseOutput,
        UtxoSelectionCriteria,
    },
    transaction_service::{
        handle::{TransactionEvent, TransactionServiceHandle},
        storage::models::WalletTransaction,
    },
    utxo_scanner_service::handle::UtxoScannerEvent,
    TransactionStage,
    WalletConfig,
    WalletSqlite,
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;
use tari_common::configuration::Network;
use tari_common_types::{
    burnt_proof::BurntProof,
    emoji::EmojiId,
    key_branches::TransactionKeyManagerBranch,
    tari_address::TariAddress,
    transaction::TxId,
    types::{Commitment, FixedHash, HashOutput, PrivateKey, PublicKey, Signature},
    wallet_types::WalletType,
};
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityRequester},
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerQuery},
    types::CommsPublicKey,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::{
    blocks::pre_mine::get_pre_mine_items,
    covenants::Covenant,
    one_sided::shared_secret_to_output_encryption_key,
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        tari_amount::{uT, MicroMinotari, Minotari},
        transaction_components::{
            encrypted_data::PaymentId,
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
        },
        CryptoFactories,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    dhke::DiffieHellmanSharedSecret,
    ristretto::{pedersen::PedersenCommitment, RistrettoSecretKey},
};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager_service::{KeyId, KeyManagerInterface},
    SeedWords,
};
use tari_p2p::{auto_update::AutoUpdateConfig, peer_seeds::SeedPeer, PeerSeedsConfig};
use tari_script::{push_pubkey_script, CheckSigSchnorrSignature};
use tari_shutdown::Shutdown;
use tari_utilities::{hex::Hex, ByteArray, SafePassword};
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};

use super::error::CommandError;
use crate::{
    automation::{
        utils::{
            create_pre_mine_output_dir,
            get_file_name,
            move_session_file_to_session_dir,
            out_dir,
            read_and_verify,
            read_session_info,
            read_verify_session_info,
            write_json_object_to_file_as_line,
            write_to_json_file,
        },
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
    },
    cli::{CliCommands, CliRecipientInfo, MakeItRainTransactionType},
    init::init_wallet,
    recovery::{get_seed_from_seed_words, wallet_recovery},
    utils::db::{get_custom_base_node_peer_from_db, CUSTOM_BASE_NODE_ADDRESS_KEY, CUSTOM_BASE_NODE_PUBLIC_KEY_KEY},
    wallet_modes::PeerConfig,
};

pub const LOG_TARGET: &str = "wallet::automation::commands";
// Pre-mine file names
pub(crate) const FILE_EXTENSION: &str = "json";
pub(crate) const SPEND_SESSION_INFO: &str = "step_1_session_info";
pub(crate) const SPEND_STEP_2_LEADER: &str = "step_2_for_leader_from_";
pub(crate) const SPEND_STEP_2_SELF: &str = "step_2_for_self";
pub(crate) const SPEND_STEP_3_SELF: &str = "step_3_for_self";
pub(crate) const SPEND_STEP_3_PARTIES: &str = "step_3_for_parties";
pub(crate) const SPEND_STEP_4_LEADER: &str = "step_4_for_leader_from_";

#[derive(Debug)]
pub struct SentTransaction {}

/// Send a normal negotiated transaction to a recipient
pub async fn send_tari(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroMinotari,
    destination: TariAddress,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_transaction(
            destination,
            amount,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram * uT,
            message,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn burn_tari(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroMinotari,
    message: String,
) -> Result<(TxId, BurntProof), CommandError> {
    wallet_transaction_service
        .burn_tari(
            amount,
            UtxoSelectionCriteria::default(),
            fee_per_gram * uT,
            message,
            None,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// encumbers a n-of-m transaction
#[allow(clippy::too_many_arguments)]
#[allow(clippy::mutable_key_type)]
async fn encumber_aggregate_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: MicroMinotari,
    expected_commitment: PedersenCommitment,
    script_input_shares: HashMap<PublicKey, CheckSigSchnorrSignature>,
    script_signature_public_nonces: Vec<PublicKey>,
    sender_offset_public_key_shares: Vec<PublicKey>,
    metadata_ephemeral_public_key_shares: Vec<PublicKey>,
    dh_shared_secret_shares: Vec<PublicKey>,
    recipient_address: TariAddress,
    original_maturity: u64,
    use_output: UseOutput,
) -> Result<(TxId, Transaction, PublicKey, PublicKey, PublicKey, PublicKey), CommandError> {
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
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

async fn spend_backup_pre_mine_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: MicroMinotari,
    output_hash: HashOutput,
    expected_commitment: PedersenCommitment,
    recipient_address: TariAddress,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .spend_backup_pre_mine_utxo(fee_per_gram, output_hash, expected_commitment, recipient_address)
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// finalises an already encumbered a n-of-m transaction
async fn finalise_aggregate_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    tx_id: u64,
    meta_signatures: Vec<Signature>,
    script_signatures: Vec<Signature>,
    wallet_script_secret_key: PrivateKey,
) -> Result<TxId, CommandError> {
    trace!(target: LOG_TARGET, "finalise_aggregate_utxo: start");

    let mut meta_sig = Signature::default();
    for sig in &meta_signatures {
        meta_sig = &meta_sig + sig;
    }
    let mut script_sig = Signature::default();
    for sig in &script_signatures {
        script_sig = &script_sig + sig;
    }
    trace!(target: LOG_TARGET, "finalise_aggregate_utxo: aggregated signatures");

    wallet_transaction_service
        .finalize_aggregate_utxo(tx_id, meta_sig, script_sig, wallet_script_secret_key)
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
    message: String,
) -> Result<(TxId, PublicKey, TransactionOutput), CommandError> {
    let (tx_id, pre_image, output) = wallet_transaction_service
        .send_sha_atomic_swap_transaction(dest_address, amount, selection_criteria, fee_per_gram * uT, message)
        .await
        .map_err(CommandError::TransactionServiceError)?;
    Ok((tx_id, pre_image, output))
}

/// claims a tari-SHA atomic swap HTLC transaction
pub async fn finalise_sha_atomic_swap(
    mut output_service: OutputManagerHandle,
    mut transaction_service: TransactionServiceHandle,
    output_hash: FixedHash,
    pre_image: PublicKey,
    fee_per_gram: MicroMinotari,
    message: String,
) -> Result<TxId, CommandError> {
    let (tx_id, _fee, amount, tx) = output_service
        .create_claim_sha_atomic_swap_transaction(output_hash, pre_image, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, message)
        .await?;
    Ok(tx_id)
}

/// claims a HTLC refund transaction
pub async fn claim_htlc_refund(
    mut output_service: OutputManagerHandle,
    mut transaction_service: TransactionServiceHandle,
    output_hash: FixedHash,
    fee_per_gram: MicroMinotari,
    message: String,
) -> Result<TxId, CommandError> {
    let (tx_id, _fee, amount, tx) = output_service
        .create_htlc_refund_transaction(output_hash, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, message)
        .await?;
    Ok(tx_id)
}

pub async fn register_validator_node(
    amount: MicroMinotari,
    mut wallet_transaction_service: TransactionServiceHandle,
    validator_node_public_key: PublicKey,
    validator_node_signature: Signature,
    selection_criteria: UtxoSelectionCriteria,
    fee_per_gram: MicroMinotari,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .register_validator_node(
            amount,
            validator_node_public_key,
            validator_node_signature,
            selection_criteria,
            fee_per_gram,
            message,
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
    message: String,
    payment_id: PaymentId,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_one_sided_to_stealth_address_transaction(
            dest_address,
            amount,
            selection_criteria,
            OutputFeatures::default(),
            fee_per_gram * uT,
            message,
            payment_id,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn coin_split(
    amount_per_split: MicroMinotari,
    num_splits: usize,
    fee_per_gram: MicroMinotari,
    message: String,
    output_service: &mut OutputManagerHandle,
    transaction_service: &mut TransactionServiceHandle,
) -> Result<TxId, CommandError> {
    let (tx_id, tx, amount) = output_service
        .create_coin_split(vec![], amount_per_split, num_splits, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, message)
        .await?;

    Ok(tx_id)
}

async fn wait_for_comms(connectivity_requester: &ConnectivityRequester) -> Result<(), CommandError> {
    let mut connectivity = connectivity_requester.get_event_subscription();
    print!("Waiting for connectivity... ");
    let timeout = sleep(Duration::from_secs(30));
    tokio::pin!(timeout);
    let mut timeout = timeout.fuse();
    loop {
        tokio::select! {
            // Wait for the first base node connection
            Ok(ConnectivityEvent::PeerConnected(conn)) = connectivity.recv() => {
                if conn.peer_features().is_node() {
                    println!("✅");
                    return Ok(());
                }
            },
            () = &mut timeout => {
                println!("❌");
                return Err(CommandError::Comms("Timed out".to_string()));
            }
        }
    }
}

async fn set_base_node_peer(
    mut wallet: WalletSqlite,
    public_key: PublicKey,
    address: Multiaddr,
) -> Result<(CommsPublicKey, Multiaddr), CommandError> {
    println!("Setting base node peer...");
    println!("{}::{}", public_key, address);
    wallet
        .set_base_node_peer(public_key.clone(), Some(address.clone()), None)
        .await?;
    Ok((public_key, address))
}

pub async fn discover_peer(
    mut dht_service: DhtDiscoveryRequester,
    dest_public_key: PublicKey,
) -> Result<(), CommandError> {
    let start = Instant::now();
    println!("🌎 Peer discovery started.");
    match dht_service
        .discover_peer(
            dest_public_key.clone(),
            NodeDestination::PublicKey(Box::new(dest_public_key)),
        )
        .await
    {
        Ok(peer) => {
            println!("⚡️ Discovery succeeded in {}ms.", start.elapsed().as_millis());
            println!("{}", peer);
        },
        Err(err) => {
            println!("💀 Discovery failed: '{:?}'", err);
        },
    }

    Ok(())
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
    message: String,
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
                "`make-it-rain` scheduled to start at {}: msg \"{}\"",
                start_time, message
            );
            (start_time - now).num_milliseconds() as u64
        } else {
            0
        };

        debug!(
            target: LOG_TARGET,
            "make-it-rain delaying for {:?} ms - scheduled to start at {}", delay_ms, start_time
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
            num_txs, transaction_type, message
        );
        let (sender, mut receiver) = mpsc::channel(num_txs);
        {
            let sender = sender;
            for i in 0..num_txs {
                debug!(
                    target: LOG_TARGET,
                    "make-it-rain starting {} of {} {} transactions",
                    i + 1,
                    num_txs,
                    transaction_type
                );
                let loop_started_at = Instant::now();
                let tx_service = wallet_transaction_service.clone();
                // Transaction details
                let amount = start_amount + increase_amount * (i as u64);

                // Manage transaction submission rate
                let actual_ms = (Utc::now() - started_at).num_milliseconds();
                let target_ms = (i as f64 * (1000.0 / transactions_per_second)) as i64;
                trace!(
                    target: LOG_TARGET,
                    "make-it-rain {}: target {:?} ms vs. actual {:?} ms", i, target_ms, actual_ms
                );
                if target_ms - actual_ms > 0 {
                    // Maximum delay between Txs set to 120 s
                    let delay_ms = Duration::from_millis((target_ms - actual_ms).min(120_000i64) as u64);
                    trace!(
                        target: LOG_TARGET,
                        "make-it-rain {}: delaying for {:?} ms", i, delay_ms
                    );
                    sleep(delay_ms).await;
                }
                let delayed_for = Instant::now();
                let sender_clone = sender.clone();
                let fee = fee_per_gram;
                let address = destination.clone();
                let msg = message.clone();
                tokio::task::spawn(async move {
                    let spawn_start = Instant::now();
                    // Send transaction
                    let tx_id = match transaction_type {
                        MakeItRainTransactionType::Interactive => {
                            send_tari(tx_service, fee, amount, address.clone(), msg.clone()).await
                        },
                        MakeItRainTransactionType::StealthOneSided => {
                            send_one_sided_to_stealth_address(
                                tx_service,
                                fee,
                                amount,
                                UtxoSelectionCriteria::default(),
                                address.clone(),
                                msg.clone(),
                                PaymentId::Empty,
                            )
                            .await
                        },
                        MakeItRainTransactionType::BurnTari => burn_tari(tx_service, fee, amount, msg.clone())
                            .await
                            .map(|(tx_id, _)| tx_id),
                    };
                    let submit_time = Instant::now();

                    if let Err(e) = sender_clone
                        .send(TransactionSendStats {
                            i: i + 1,
                            tx_id,
                            delayed_for: delayed_for.duration_since(loop_started_at),
                            submit_time: submit_time.duration_since(spawn_start),
                        })
                        .await
                    {
                        warn!(
                            target: LOG_TARGET,
                            "make-it-rain: Error sending transaction send stats to channel: {}",
                            e.to_string()
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
                        e.to_string(),
                    );
                },
            }
        }
        debug!(
            target: LOG_TARGET,
            "make-it-rain concluded {} {} transactions", num_txs, transaction_type
        );
        println!(
            "\n`make-it-rain` concluded {} {} transactions (\"{}\") at {}",
            num_txs,
            transaction_type,
            message,
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
    debug!(target: LOG_TARGET, "monitor transactions wait_stage: {:?}", wait_stage);
    println!(
        "Monitoring {} sent transactions to {:?} stage...",
        tx_ids.len(),
        wait_stage
    );

    loop {
        match event_stream.recv().await {
            Ok(event) => match &*event {
                TransactionEvent::TransactionSendResult(id, status) if tx_ids.contains(id) => {
                    debug!(target: LOG_TARGET, "tx send event for tx_id: {}, {}", *id, status);
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
                    debug!(target: LOG_TARGET, "tx reply event for tx_id: {}", *id);
                    if wait_stage == TransactionStage::Negotiated {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                TransactionEvent::TransactionBroadcast(id) if tx_ids.contains(id) => {
                    debug!(target: LOG_TARGET, "tx mempool broadcast event for tx_id: {}", *id);
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
                        "tx mined unconfirmed event for tx_id: {}, confirmations: {}, is_valid: {}",
                        *tx_id,
                        num_confirmations,
                        is_valid
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
                        "tx mined confirmed event for tx_id: {}, is_valid:{}", *tx_id, is_valid
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
                warn!(target: LOG_TARGET, "monitor_transactions: {}", err);
            },
        }
    }

    results
}

#[allow(clippy::too_many_lines)]
pub async fn command_runner(
    config: &WalletConfig,
    commands: Vec<CliCommands>,
    wallet: WalletSqlite,
) -> Result<bool, CommandError> {
    let wait_stage = config.command_send_wait_stage;

    let mut transaction_service = wallet.transaction_service.clone();
    let mut output_service = wallet.output_manager_service.clone();
    let dht_service = wallet.dht_service.discovery_service_requester().clone();
    let connectivity_requester = wallet.comms.connectivity();
    let key_manager_service = wallet.key_manager_service.clone();
    let mut online = false;

    let mut tx_ids = Vec::new();

    println!("==============");
    println!("Command Runner");
    println!("==============");

    let (_current_index, mut peer_list) =
        if let Some((index, list)) = wallet.wallet_connectivity.get_base_node_peer_manager_state() {
            (index, list)
        } else {
            (0, vec![])
        };
    let mut unban_peer_manager_peers = false;

    #[allow(clippy::enum_glob_use)]
    for (idx, parsed) in commands.into_iter().enumerate() {
        println!("\n{}. {:?}\n", idx + 1, parsed);
        use crate::cli::CliCommands::*;
        match parsed {
            GetBalance => match output_service.clone().get_balance().await {
                Ok(balance) => {
                    debug!(target: LOG_TARGET, "get-balance concluded");
                    println!("{}", balance);
                },
                Err(e) => eprintln!("GetBalance error! {}", e),
            },
            DiscoverPeer(args) => {
                if !online {
                    match wait_for_comms(&connectivity_requester).await {
                        Ok(..) => {
                            online = true;
                        },
                        Err(e) => {
                            eprintln!("DiscoverPeer error! {}", e);
                            continue;
                        },
                    }
                }
                if let Err(e) = discover_peer(dht_service.clone(), args.dest_public_key.into()).await {
                    eprintln!("DiscoverPeer error! {}", e);
                }
            },
            BurnMinotari(args) => {
                match burn_tari(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.message,
                )
                .await
                {
                    Ok((tx_id, proof)) => {
                        debug!(target: LOG_TARGET, "burn minotari concluded with tx_id {}", tx_id);
                        println!("Burnt {} Minotari in tx_id: {}", args.amount, tx_id);
                        println!("The following can be used to claim the burnt funds:");
                        println!();
                        println!("claim_public_key: {}", proof.reciprocal_claim_public_key);
                        println!("commitment: {}", proof.commitment.as_public_key());
                        println!("ownership_proof: {:?}", proof.ownership_proof);
                        println!("ownership_proof: {:?}", proof.range_proof);
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("BurnMinotari error! {}", e),
                }
            },
            PreMineSpendGetOutputStatus => {
                let pre_mine_outputs = get_all_embedded_pre_mine_outputs()?;
                let output_hashes: Vec<HashOutput> = pre_mine_outputs.iter().map(|v| v.hash()).collect();
                let unspent_outputs = transaction_service.fetch_unspent_outputs(output_hashes).await?;

                let pre_mine_items = match get_pre_mine_items(Network::get_current_or_user_setting_or_default()).await {
                    Ok(items) => items,
                    Err(e) => {
                        eprintln!("\nError: {}\n", e);
                        return Ok(false);
                    },
                };

                let (session_id, out_dir) = match create_pre_mine_output_dir(Some("pre_mine_status")) {
                    Ok(values) => values,
                    Err(e) => {
                        eprintln!("\nError: {}\n", e);
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
                    eprintln!("\nError: Could not write pre-mine header ({})\n", e);
                    return Ok(false);
                }

                for (index, item) in pre_mine_items.iter().enumerate() {
                    let unspent = unspent_outputs
                        .iter()
                        .any(|u| u.commitment() == &pre_mine_outputs[index].commitment);
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
                        eprintln!("\nError: Could not write pre-mine item ({})\n", e);
                        return Ok(false);
                    }
                }

                println!();
                println!("Concluded step 0 'pre-mine-spend-get-output-status'");
                println!("Your session ID is:                    '{}'", session_id);
                println!("Your session's output directory is:    '{}'", out_dir.display());
                println!("Pre-mine output spent status saved to: '{}'", csv_file_name);
                println!();
            },
            PreMineSpendSessionInfo(args) => {
                match *key_manager_service.get_wallet_type().await {
                    WalletType::Ledger(_) => {},
                    _ => {
                        eprintln!("\nError: Wallet type must be 'Ledger' to spend pre-mine outputs!\n");
                        break;
                    },
                }

                let args_recipient_info = sort_args_recipient_info(args.recipient_info);
                if let Err(e) = verify_no_duplicate_indexes(&args_recipient_info) {
                    eprintln!("\nError: {} duplicate output indexes detected!\n", e);
                    break;
                }

                let mut recipient_info = Vec::new();
                let mut error = false;
                for item in args_recipient_info {
                    if args.verify_unspent_outputs && !args.use_pre_mine_input_file {
                        let embedded_outputs = match get_embedded_pre_mine_outputs(item.output_indexes.clone(), None) {
                            Ok(outputs) => outputs,
                            Err(e) => {
                                eprintln!("\nError: {}\n", e);
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
                        eprintln!("\nError: {}\n", e);
                        return Ok(false);
                    },
                };
                let session_info = PreMineSpendStep1SessionInfo {
                    session_id: session_id.clone(),
                    fee_per_gram: args.fee_per_gram,
                    recipient_info,
                    use_pre_mine_input_file: args.use_pre_mine_input_file,
                };

                let out_file = out_dir.join(get_file_name(SPEND_SESSION_INFO, None));
                write_to_json_file(&out_file, true, session_info)?;
                println!();
                println!("Concluded step 1 'pre-mine-spend-session-info'");
                println!("Your session ID is:                 '{}'", session_id);
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
                    Ok(outputs) => outputs[0].clone(),
                    Err(e) => {
                        eprintln!("\nError: {}\n", e);
                        break;
                    },
                };
                let commitment = embedded_output.commitment.clone();
                let output_hash = embedded_output.hash();

                match spend_backup_pre_mine_utxo(
                    transaction_service.clone(),
                    args.fee_per_gram,
                    output_hash,
                    commitment.clone(),
                    args.recipient_address,
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
                        eprintln!("\nError: Spent pre-mine transaction error! {}\n", e);
                        break;
                    },
                }
            },
            PreMineSpendPartyDetails(args) => {
                match *key_manager_service.get_wallet_type().await {
                    WalletType::Ledger(_) => {},
                    _ => {
                        eprintln!("\nError: Wallet type must be 'Ledger' to spend pre-mine outputs!\n");
                        break;
                    },
                }

                if args.alias.is_empty() || args.alias.contains(" ") {
                    eprintln!("\nError: Alias cannot contain spaces!\n");
                    break;
                }
                if args.alias.chars().any(|c| !c.is_alphanumeric() && c != '_') {
                    eprintln!("\nError: Alias contains invalid characters! Only alphanumeric and '_' are allowed.\n");
                    break;
                }
                let args_recipient_info = sort_args_recipient_info(args.recipient_info);
                if let Err(e) = verify_no_duplicate_indexes(&args_recipient_info) {
                    eprintln!("\nError: {} duplicate output indexes detected!\n", e);
                    break;
                }

                // Read session info
                let session_info = read_session_info::<PreMineSpendStep1SessionInfo>(args.input_file.clone())?;
                // Verify  session info
                let args_recipient_info_flat = args_recipient_info
                    .iter()
                    .flat_map(|v1| {
                        v1.output_indexes
                            .iter()
                            .map(|&v2| RecipientInfo {
                                output_to_be_spend: v2,
                                recipient_address: v1.recipient_address.clone(),
                            })
                            .collect::<Vec<RecipientInfo>>()
                    })
                    .collect::<Vec<RecipientInfo>>();
                if args_recipient_info_flat != session_info.recipient_info {
                    eprintln!(
                        "\nError: Mismatched recipient info! leader {:?} vs. self {:?}\n",
                        session_info
                            .recipient_info
                            .iter()
                            .map(|v| (v.output_to_be_spend, v.recipient_address.clone()))
                            .collect::<Vec<_>>(),
                        args_recipient_info_flat
                            .iter()
                            .map(|v| (v.output_to_be_spend, v.recipient_address.clone()))
                            .collect::<Vec<_>>()
                    );
                    break;
                }

                let pre_mine_from_file =
                    match read_genesis_file_outputs(session_info.use_pre_mine_input_file, args.pre_mine_file_path) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {}\n", e);
                            break;
                        },
                    };

                println!();
                let mut outputs_for_leader = Vec::with_capacity(args_recipient_info.len());
                let mut outputs_for_self = Vec::with_capacity(args_recipient_info.len());
                let mut error = false;
                for (i, recipient_info) in args_recipient_info.iter().enumerate() {
                    println!(
                        "  Start processing {} of {} recipients, current wallet {}",
                        i + 1,
                        args_recipient_info.len(),
                        recipient_info.recipient_address
                    );
                    let embedded_outputs = match get_embedded_pre_mine_outputs(
                        recipient_info.output_indexes.clone(),
                        pre_mine_from_file.clone(),
                    ) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {}\n", e);
                            error = true;
                            break;
                        },
                    };
                    let commitments = embedded_outputs
                        .iter()
                        .map(|v| v.commitment.clone())
                        .collect::<Vec<_>>();

                    for (j, (output_index, commitment)) in
                        recipient_info.output_indexes.iter().zip(commitments.iter()).enumerate()
                    {
                        let script_nonce_key = key_manager_service.get_random_key().await?;
                        let sender_offset_key = key_manager_service.get_random_key().await?;
                        let sender_offset_nonce = key_manager_service.get_random_key().await?;
                        let shared_secret = key_manager_service
                            .get_diffie_hellman_shared_secret(
                                &sender_offset_key.key_id,
                                recipient_info
                                    .recipient_address
                                    .public_view_key()
                                    .ok_or(CommandError::InvalidArgument("Missing public view key".to_string()))?,
                            )
                            .await?;
                        let shared_secret_public_key = PublicKey::from_canonical_bytes(shared_secret.as_bytes())?;

                        let pre_mine_script_key_id = KeyId::Managed {
                            branch: TransactionKeyManagerBranch::PreMine.get_branch_key(),
                            index: *output_index as u64,
                        };
                        let pre_mine_public_script_key = match key_manager_service
                            .get_public_key_at_key_id(&pre_mine_script_key_id)
                            .await
                        {
                            Ok(key) => key,
                            Err(e) => {
                                eprintln!(
                                    "\nError: Could not retrieve script key for output {}: {}\n",
                                    output_index, e
                                );
                                error = true;
                                break;
                            },
                        };
                        let script_input_signature = key_manager_service
                            .sign_script_message(&pre_mine_script_key_id, commitment.as_bytes())
                            .await?;

                        outputs_for_leader.push(Step2OutputsForLeader {
                            output_index: *output_index,
                            recipient_address: recipient_info.recipient_address.clone(),
                            script_input_signature,
                            public_script_nonce_key: script_nonce_key.pub_key,
                            public_sender_offset_key: sender_offset_key.pub_key,
                            public_sender_offset_nonce_key: sender_offset_nonce.pub_key,
                            dh_shared_secret_public_key: shared_secret_public_key,
                            pre_mine_public_script_key,
                        });

                        outputs_for_self.push(Step2OutputsForSelf {
                            output_index: *output_index,
                            recipient_address: recipient_info.recipient_address.clone(),
                            script_nonce_key_id: script_nonce_key.key_id,
                            sender_offset_key_id: sender_offset_key.key_id,
                            sender_offset_nonce_key_id: sender_offset_nonce.key_id,
                            pre_mine_script_key_id,
                        });
                        println!(
                            "    Processed {} of {} transactions",
                            j + 1,
                            recipient_info.output_indexes.len()
                        );
                    }
                    if error {
                        break;
                    }
                }
                if error {
                    break;
                }

                let out_dir = out_dir(&session_info.session_id)?;
                let out_file_leader = out_dir.join(get_file_name(SPEND_STEP_2_LEADER, Some(args.alias.clone())));
                write_json_object_to_file_as_line(&out_file_leader, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file_leader, false, PreMineSpendStep2OutputsForLeader {
                    outputs_for_leader,
                    alias: args.alias.clone(),
                })?;

                let out_file_self = out_dir.join(get_file_name(SPEND_STEP_2_SELF, None));
                write_json_object_to_file_as_line(&out_file_self, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file_self, false, PreMineSpendStep2OutputsForSelf {
                    outputs_for_self,
                    alias: args.alias.clone(),
                })?;

                println!();
                println!("Concluded step 2 'pre-mine-spend-party-details'");
                println!("Your session's output directory is '{}'", out_dir.display());
                move_session_file_to_session_dir(&session_info.session_id, &args.input_file)?;
                println!(
                    "Send '{}' to leader for step 3",
                    get_file_name(SPEND_STEP_2_LEADER, Some(args.alias))
                );
                println!();
            },
            PreMineSpendEncumberAggregateUtxo(args) => {
                match *key_manager_service.get_wallet_type().await {
                    WalletType::Ledger(_) => {},
                    _ => {
                        eprintln!("\nError: Wallet type must be 'Ledger' to spend pre-mine outputs!\n");
                        break;
                    },
                }

                temp_ban_peers(&wallet, &mut peer_list).await;
                unban_peer_manager_peers = true;

                // Read session info
                let session_info = read_verify_session_info::<PreMineSpendStep1SessionInfo>(&args.session_id)?;
                let session_info_indexed = session_info
                    .recipient_info
                    .iter()
                    .map(|v| (v.output_to_be_spend, v.recipient_address.clone()))
                    .collect::<Vec<_>>();

                // Read and verify party info
                let mut party_info = Vec::with_capacity(args.input_file_names.len());
                for file_name in args.input_file_names {
                    party_info.push(read_and_verify::<PreMineSpendStep2OutputsForLeader>(
                        &args.session_id,
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
                let mut party_info_per_index = Vec::with_capacity(party_info_flattened[0].len());
                for i in 0..party_info_flattened[0].len() {
                    let mut outputs_per_index = Vec::with_capacity(party_info_flattened.len());
                    for outputs in &party_info_flattened {
                        outputs_per_index.push(outputs[i].clone());
                    }
                    party_info_per_index.push(outputs_per_index);
                }

                let pre_mine_from_file =
                    match read_genesis_file_outputs(session_info.use_pre_mine_input_file, args.pre_mine_file_path) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {}\n", e);
                            break;
                        },
                    };

                // Encumber outputs
                let mut outputs_for_parties = Vec::with_capacity(party_info_per_index.len());
                let mut outputs_for_self = Vec::with_capacity(party_info_per_index.len());
                let pre_mine_items = match get_pre_mine_items(Network::get_current_or_user_setting_or_default()).await {
                    Ok(items) => items,
                    Err(e) => {
                        eprintln!("\nError: {}\n", e);
                        lift_temp_ban_peers(&wallet, &mut peer_list).await;
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
                    let current_index = indexed_info[0].output_index;
                    let current_recipient_address = indexed_info[0].recipient_address.clone();
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

                    let original_maturity = pre_mine_items[current_index].original_maturity;
                    let embedded_output =
                        match get_embedded_pre_mine_outputs(vec![current_index], pre_mine_from_file.clone()) {
                            Ok(outputs) => outputs[0].clone(),
                            Err(e) => {
                                eprintln!("\nError: {}\n", e);
                                error = true;
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
                            UseOutput::AsProvided(embedded_output)
                        } else {
                            UseOutput::FromBlockchain(embedded_output.hash())
                        },
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
                            outputs_for_parties.push(Step3OutputsForParties {
                                output_index: current_index,
                                input_stack: transaction.body.inputs()[0].clone().input_data,
                                input_script: transaction.body.inputs()[0].script().unwrap().clone(),
                                total_script_key: script_pubkey,
                                script_signature_ephemeral_commitment: transaction.body.inputs()[0]
                                    .script_signature
                                    .ephemeral_commitment()
                                    .clone(),
                                script_signature_ephemeral_pubkey: total_script_nonce,
                                output_commitment: transaction.body.outputs()[0].commitment().clone(),
                                sender_offset_pubkey: transaction.body.outputs()[0].clone().sender_offset_public_key,
                                metadata_signature_ephemeral_commitment: transaction.body.outputs()[0]
                                    .metadata_signature
                                    .ephemeral_commitment()
                                    .clone(),
                                metadata_signature_ephemeral_pubkey: total_metadata_ephemeral_public_key,
                                encrypted_data: transaction.body.outputs()[0].clone().encrypted_data,
                                output_features: transaction.body.outputs()[0].clone().features,
                                shared_secret,
                            });
                            outputs_for_self.push(Step3OutputsForSelf {
                                output_index: current_index,
                                tx_id,
                            });
                        },
                        Err(e) => {
                            eprintln!("\nError: Encumber aggregate transaction error! {}\n", e);
                            error = true;
                            break;
                        },
                    }
                    println!("  Processed {} of {} transactions", i + 1, party_info_per_index.len());
                }
                if error {
                    break;
                }

                let out_dir = out_dir(&args.session_id)?;
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
            PreMineSpendInputOutputSigs(args) => {
                match *key_manager_service.get_wallet_type().await {
                    WalletType::Ledger(_) => {},
                    _ => {
                        eprintln!("\nError: Wallet type must be 'Ledger' to spend pre-mine outputs!\n");
                        break;
                    },
                }

                // Read session info
                let session_info = read_verify_session_info::<PreMineSpendStep1SessionInfo>(&args.session_id)?;
                // Read leader input
                let leader_info_indexed = read_and_verify::<PreMineSpendStep3OutputsForParties>(
                    &args.session_id,
                    &get_file_name(SPEND_STEP_3_PARTIES, None),
                    &session_info,
                )?;
                // Read own party info
                let party_info_indexed = read_and_verify::<PreMineSpendStep2OutputsForSelf>(
                    &args.session_id,
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
                        "\nError: Mismatched output indexes detected! session {:?} vs. leader {:?} vs. self {:?}\n",
                        session_info_indexes, leader_info_indexes, party_info_indexes
                    );
                    break;
                }

                let pre_mine_from_file =
                    match read_genesis_file_outputs(session_info.use_pre_mine_input_file, args.pre_mine_file_path) {
                        Ok(outputs) => outputs,
                        Err(e) => {
                            eprintln!("\nError: {}\n", e);
                            break;
                        },
                    };

                println!();
                let mut outputs_for_leader = Vec::with_capacity(party_info_indexed.outputs_for_self.len());
                let mut error = false;
                for (i, (leader_info, party_info)) in leader_info_indexed
                    .outputs_for_parties
                    .iter()
                    .zip(party_info_indexed.outputs_for_self.iter())
                    .enumerate()
                {
                    let embedded_output = match get_embedded_pre_mine_outputs(
                        vec![party_info.output_index],
                        pre_mine_from_file.clone(),
                    ) {
                        Ok(outputs) => outputs[0].clone(),
                        Err(e) => {
                            eprintln!("\nError: {}\n", e);
                            error = true;
                            break;
                        },
                    };

                    // Script signature
                    let challenge = TransactionInput::build_script_signature_challenge(
                        &TransactionInputVersion::get_current_version(),
                        &leader_info.script_signature_ephemeral_commitment,
                        &leader_info.script_signature_ephemeral_pubkey,
                        &leader_info.input_script,
                        &leader_info.input_stack,
                        &leader_info.total_script_key,
                        &embedded_output.commitment,
                    );

                    let script_signature = match key_manager_service
                        .sign_with_nonce_and_challenge(
                            &party_info.pre_mine_script_key_id,
                            &party_info.script_nonce_key_id,
                            &challenge,
                        )
                        .await
                    {
                        Ok(signature) => signature,
                        Err(e) => {
                            eprintln!("\nError: Script signature SignMessage error! {}\n", e);
                            error = true;
                            break;
                        },
                    };

                    // lets verify the script
                    let shared_secret = match DiffieHellmanSharedSecret::<PublicKey>::from_canonical_bytes(
                        leader_info.shared_secret.as_bytes(),
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("\nError: Could not create shared secret from canonical bytes! {}\n", e);
                            error = true;
                            break;
                        },
                    };

                    let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
                    let (committed_value, commitment_mask_private_key, _payment_id) = match EncryptedData::decrypt_data(
                        &encryption_key,
                        &leader_info.output_commitment,
                        &leader_info.encrypted_data,
                    ) {
                        Ok((value, mask, id)) => (value, mask, id),
                        Err(e) => {
                            eprintln!("\nError: Could not decrypt data! {}\n", e);
                            error = true;
                            break;
                        },
                    };
                    let commitment_mask_key_id = &key_manager_service
                        .import_key(commitment_mask_private_key.clone())
                        .await?;
                    match key_manager_service
                        .verify_mask(
                            &leader_info.output_commitment,
                            commitment_mask_key_id,
                            committed_value.as_u64(),
                        )
                        .await
                    {
                        Ok(_) => {},
                        Err(e) => {
                            eprintln!("\nError: Could not verify mask! {}\n", e);
                            error = true;
                            break;
                        },
                    }
                    // now lets calculate the script with stealth key
                    let script_spending_key = key_manager_service
                        .stealth_address_script_spending_key(
                            commitment_mask_key_id,
                            party_info.recipient_address.public_spend_key(),
                        )
                        .await?;
                    let script = push_pubkey_script(&script_spending_key);

                    // Metadata signature
                    let script_offset = key_manager_service
                        .get_script_offset(&vec![party_info.pre_mine_script_key_id.clone()], &vec![party_info
                            .sender_offset_key_id
                            .clone()])
                        .await?;
                    let challenge = TransactionOutput::build_metadata_signature_challenge(
                        &TransactionOutputVersion::get_current_version(),
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

                    let metadata_signature = match key_manager_service
                        .sign_with_nonce_and_challenge(
                            &party_info.sender_offset_key_id,
                            &party_info.sender_offset_nonce_key_id,
                            &challenge,
                        )
                        .await
                    {
                        Ok(signature) => signature,
                        Err(e) => {
                            eprintln!("\nError: Metadata signature SignMessage error! {}\n", e);
                            error = true;
                            break;
                        },
                    };

                    if script_signature.get_signature() == Signature::default().get_signature() ||
                        metadata_signature.get_signature() == Signature::default().get_signature()
                    {
                        eprintln!(
                            "\nError: Script and/or metadata signatures not created (index {})!\n",
                            party_info.output_index
                        );
                        error = true;
                        break;
                    }

                    outputs_for_leader.push(Step4OutputsForLeader {
                        output_index: party_info.output_index,
                        script_signature,
                        metadata_signature,
                        script_offset,
                    });

                    println!(
                        "  Processed {} of {} transactions",
                        i + 1,
                        leader_info_indexed.outputs_for_parties.len()
                    );
                }
                if error {
                    break;
                }

                let out_dir = out_dir(&args.session_id)?;
                let out_file = out_dir.join(get_file_name(
                    SPEND_STEP_4_LEADER,
                    Some(party_info_indexed.alias.clone()),
                ));
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
            PreMineSpendAggregateTransaction(args) => {
                match *key_manager_service.get_wallet_type().await {
                    WalletType::Ledger(_) => {},
                    _ => {
                        eprintln!("\nError: Wallet type must be 'Ledger' to spend pre-mine outputs!\n");
                        break;
                    },
                }

                temp_ban_peers(&wallet, &mut peer_list).await;
                unban_peer_manager_peers = true;

                // Read session info
                let session_info = read_verify_session_info::<PreMineSpendStep1SessionInfo>(&args.session_id)?;

                // Read other parties info
                let mut party_info = Vec::with_capacity(args.input_file_names.len());
                for file_name in args.input_file_names {
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
                        "\nError: Mismatched output indexes detected! session {:?} vs. leader (self) {:?}\n",
                        session_info_indexes, leader_info_indexes
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
                let mut party_info_per_index = Vec::with_capacity(party_info_flattened[0].len());
                let number_of_parties = party_info_flattened.len();
                for i in 0..party_info_flattened[0].len() {
                    let mut outputs_per_index = Vec::with_capacity(number_of_parties);
                    for outputs in &party_info_flattened {
                        outputs_per_index.push(outputs[i].clone());
                    }
                    party_info_per_index.push(outputs_per_index);
                }

                // Create finalized spend transactions
                let mut inputs = Vec::new();
                let mut outputs = Vec::new();
                let mut kernels = Vec::new();
                let mut kernel_offset = PrivateKey::default();
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
                    if session_info.use_pre_mine_input_file {
                        match transaction_service.get_any_transaction(leader_self.tx_id).await {
                            Ok(Some(WalletTransaction::Completed(tx))) => {
                                // Fees must be zero
                                match tx.transaction.body.get_total_fee() {
                                    Ok(fee) => {
                                        if fee != MicroMinotari::zero() {
                                            eprintln!(
                                                "\nError: Transaction {} fee ({}) for does not equal zero!\n",
                                                tx.tx_id, fee
                                            );
                                            error = true;
                                            break;
                                        }
                                    },
                                    Err(e) => {
                                        eprintln!("\nError: Transaction {}! ({})\n", tx.tx_id, e);
                                        error = true;
                                        break;
                                    },
                                }

                                let mut utxo_sum = Commitment::default();
                                for output in tx.transaction.body.outputs() {
                                    outputs.push(output.clone());
                                    utxo_sum = &utxo_sum + &output.commitment;
                                }
                                for input in tx.transaction.body.inputs() {
                                    inputs.push(input.clone());
                                    match input.commitment() {
                                        Ok(commitment) => utxo_sum = &utxo_sum - commitment,
                                        Err(e) => {
                                            eprintln!("\nError: Input commitment ({})!\n", e);
                                            error = true;
                                            break;
                                        },
                                    }
                                }
                                if error {
                                    break;
                                }
                                let mut kernel_sum = Commitment::default();
                                for kernel in tx.transaction.body.kernels() {
                                    kernels.push(kernel.clone());
                                    kernel_sum = &kernel_sum + &kernel.excess;
                                }
                                kernel_offset = &kernel_offset + &tx.transaction.offset;
                                // Ensure that the balance equation holds:
                                //   sum(output commitments) - sum(input  commitments) =  sum(kernel excesses) +
                                // total_offset
                                let offset = CryptoFactories::default()
                                    .commitment
                                    .commit_value(&tx.transaction.offset, 0);
                                if utxo_sum != &kernel_sum + &offset {
                                    eprintln!(
                                        "\nError: Transaction {} balance: UTXO sum {} vs. kernel sum + offset {}!\n",
                                        tx.tx_id,
                                        utxo_sum.to_hex(),
                                        (&kernel_sum + &offset).to_hex()
                                    );
                                    error = true;
                                    break;
                                }
                            },
                            Ok(_) => {
                                eprintln!(
                                    "\nError: Transaction '{}' is not in a completed state!\n",
                                    leader_self.tx_id
                                );
                                break;
                            },
                            Err(e) => {
                                eprintln!("\nError: Transaction '{}' not found! ({})\n", leader_self.tx_id, e);
                                break;
                            },
                        }
                    }

                    println!("  Processed {} of {}", i + 1, party_info_per_index.len());
                }
                if error {
                    break;
                }

                let file_name = get_pre_mine_addition_file_name();
                let out_dir_path = out_dir(&args.session_id)?;
                let out_file = out_dir_path.join(&file_name);
                if session_info.use_pre_mine_input_file {
                    // Ensure that the balance equation holds:
                    //   sum(output commitments) - sum(input  commitments) =  sum(kernel excesses) + kernel_offset
                    let mut utxo_sum = Commitment::default();
                    for output in &outputs {
                        utxo_sum = &utxo_sum + &output.commitment;
                    }
                    for input in &inputs {
                        match input.commitment() {
                            Ok(commitment) => utxo_sum = &utxo_sum - commitment,
                            Err(e) => {
                                eprintln!("\nError: Input commitment ({})!\n", e);
                                break;
                            },
                        }
                    }
                    let mut kernel_sum = Commitment::default();
                    for kernel in &kernels {
                        kernel_sum = &kernel_sum + &kernel.excess;
                    }
                    let offset = CryptoFactories::default().commitment.commit_value(&kernel_offset, 0);
                    if utxo_sum != &kernel_sum + &offset {
                        eprintln!(
                            "\nError: Transactions balance: UTXO sum {} vs. kernel sum + offset {}!\n",
                            utxo_sum.to_hex(),
                            (&kernel_sum + &offset).to_hex()
                        );
                    }

                    let mut file_stream = match File::create(&out_file) {
                        Ok(file) => file,
                        Err(e) => {
                            eprintln!("\nError: Could not create the pre-mine file ({})\n", e);
                            break;
                        },
                    };

                    let mut error = false;
                    inputs.sort();
                    for input in &inputs {
                        let input_s = match serde_json::to_string(&input) {
                            Ok(val) => val,
                            Err(e) => {
                                eprintln!("\nError: Could not serialize UTXO ({})\n", e);
                                error = true;
                                break;
                            },
                        };
                        if let Err(e) = file_stream.write_all(format!("{}\n", input_s).as_bytes()) {
                            eprintln!("\nError: Could not write UTXO to file ({})\n", e);
                            error = true;
                            break;
                        }
                    }
                    if error {
                        break;
                    }
                    outputs.sort();
                    for output in &outputs {
                        let utxo_s = match serde_json::to_string(&output) {
                            Ok(val) => val,
                            Err(e) => {
                                eprintln!("\nError: Could not serialize UTXO ({})\n", e);
                                error = true;
                                break;
                            },
                        };
                        if let Err(e) = file_stream.write_all(format!("{}\n", utxo_s).as_bytes()) {
                            eprintln!("\nError: Could not write UTXO to file ({})\n", e);
                            error = true;
                            break;
                        }
                    }
                    if error {
                        break;
                    }
                    kernels.sort();
                    for kernel in &kernels {
                        let kernel_s = match serde_json::to_string(&kernel) {
                            Ok(val) => val,
                            Err(e) => {
                                eprintln!("\nError: Could not serialize kernel ({})\n", e);
                                break;
                            },
                        };
                        if let Err(e) = file_stream.write_all(format!("{}\n", kernel_s).as_bytes()) {
                            eprintln!("\nError: Could not write the genesis file ({})\n", e);
                            error = true;
                            break;
                        }
                    }
                    if error {
                        break;
                    }
                    let kernel_offset_s = match serde_json::to_string(&kernel_offset) {
                        Ok(val) => val,
                        Err(e) => {
                            eprintln!("\nError: Could not serialize kernel offset ({})\n", e);
                            break;
                        },
                    };
                    if let Err(e) = file_stream.write_all(format!("{}\n", kernel_offset_s).as_bytes()) {
                        eprintln!("\nError: Could not write the genesis file ({})\n", e);
                        break;
                    }
                }

                println!();
                if session_info.use_pre_mine_input_file {
                    println!(
                        "Genesis block immediate pre-mine spend information: '{}' in '{}'",
                        file_name,
                        out_dir_path.display()
                    );
                }
                println!("Concluded step 5 'pre-mine-spend-aggregate-transaction'");
                println!();
            },
            SendMinotari(args) => {
                match send_tari(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.destination,
                    args.message,
                )
                .await
                {
                    Ok(tx_id) => {
                        debug!(target: LOG_TARGET, "send-minotari concluded with tx_id {}", tx_id);
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("SendMinotari error! {}", e),
                }
            },
            SendOneSidedToStealthAddress(args) => {
                match send_one_sided_to_stealth_address(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    UtxoSelectionCriteria::default(),
                    args.destination,
                    args.message,
                    PaymentId::Empty,
                )
                .await
                {
                    Ok(tx_id) => {
                        debug!(
                            target: LOG_TARGET,
                            "send-one-sided-to-stealth-address concluded with tx_id {}", tx_id
                        );
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("SendOneSidedToStealthAddress error! {}", e),
                }
            },
            MakeItRain(args) => {
                let transaction_type = args.transaction_type();
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
                    args.message,
                )
                .await
                {
                    eprintln!("MakeItRain error! {}", e);
                }
            },
            CoinSplit(args) => {
                match coin_split(
                    args.amount_per_split,
                    args.num_splits,
                    args.fee_per_gram,
                    args.message,
                    &mut output_service,
                    &mut transaction_service.clone(),
                )
                .await
                {
                    Ok(tx_id) => {
                        tx_ids.push(tx_id);
                        debug!(target: LOG_TARGET, "coin-split concluded with tx_id {}", tx_id);
                        println!("Coin split succeeded");
                    },
                    Err(e) => eprintln!("CoinSplit error! {}", e),
                }
            },
            Whois(args) => {
                let public_key = args.public_key.into();
                let emoji_id = EmojiId::from(&public_key).to_string();

                println!("Public Key: {}", public_key.to_hex());
                println!("Emoji ID  : {}", emoji_id);
            },
            ExportUtxos(args) => match output_service.get_unspent_outputs().await {
                Ok(utxos) => {
                    let mut unblinded_utxos: Vec<(UnblindedOutput, Commitment)> = Vec::with_capacity(utxos.len());
                    for output in utxos {
                        let unblinded =
                            UnblindedOutput::from_wallet_output(output.wallet_output, &wallet.key_manager_service)
                                .await?;
                        unblinded_utxos.push((unblinded, output.commitment));
                    }
                    let count = unblinded_utxos.len();
                    let sum: MicroMinotari = unblinded_utxos.iter().map(|utxo| utxo.0.value).sum();
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_utxos_to_csv_file(unblinded_utxos, file, args.with_private_keys) {
                            eprintln!("ExportUtxos error! {}", e);
                        }
                    } else {
                        for (i, utxo) in unblinded_utxos.iter().enumerate() {
                            println!(
                                "{}. Value: {}, Spending Key: {:?}, Script Key: {:?}, Features: {}",
                                i + 1,
                                utxo.0.value,
                                if args.with_private_keys {
                                    utxo.0.spending_key.to_hex()
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
                    println!("Total number of UTXOs: {}", count);
                    println!("Total value of UTXOs: {}", sum);
                },
                Err(e) => eprintln!("ExportUtxos error! {}", e),
            },
            ExportTx(args) => match transaction_service.get_any_transaction(args.tx_id.into()).await {
                Ok(Some(tx)) => {
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_tx_to_csv_file(tx, file) {
                            eprintln!("ExportTx error! {}", e);
                        }
                    } else {
                        println!("Tx: {:?}", tx);
                    }
                },
                Ok(None) => {
                    eprintln!("ExportTx error!, No tx found ")
                },
                Err(e) => eprintln!("ExportTx error! {}", e),
            },
            ImportTx(args) => {
                match load_tx_from_csv_file(args.input_file) {
                    Ok(txs) => {
                        for tx in txs {
                            match transaction_service.import_transaction(tx).await {
                                Ok(id) => println!("imported tx: {}", id),
                                Err(e) => eprintln!("Could not import tx {}", e),
                            };
                        }
                    },
                    Err(e) => eprintln!("ImportTx error! {}", e),
                };
            },
            ExportSpentUtxos(args) => match output_service.get_spent_outputs().await {
                Ok(utxos) => {
                    let mut unblinded_utxos: Vec<(UnblindedOutput, Commitment)> = Vec::with_capacity(utxos.len());
                    for output in utxos {
                        let unblinded =
                            UnblindedOutput::from_wallet_output(output.wallet_output, &wallet.key_manager_service)
                                .await?;
                        unblinded_utxos.push((unblinded, output.commitment));
                    }
                    let count = unblinded_utxos.len();
                    let sum: MicroMinotari = unblinded_utxos.iter().map(|utxo| utxo.0.value).sum();
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_utxos_to_csv_file(unblinded_utxos, file, args.with_private_keys) {
                            eprintln!("ExportSpentUtxos error! {}", e);
                        }
                    } else {
                        for (i, utxo) in unblinded_utxos.iter().enumerate() {
                            println!(
                                "{}. Value: {}, Spending Key: {:?}, Script Key: {:?}, Features: {}",
                                i + 1,
                                utxo.0.value,
                                if args.with_private_keys {
                                    utxo.0.spending_key.to_hex()
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
                    println!("Total number of UTXOs: {}", count);
                    println!("Total value of UTXOs: {}", sum);
                },
                Err(e) => eprintln!("ExportSpentUtxos error! {}", e),
            },
            CountUtxos => match output_service.get_unspent_outputs().await {
                Ok(utxos) => {
                    let utxos: Vec<WalletOutput> = utxos.into_iter().map(|v| v.wallet_output).collect();
                    let count = utxos.len();
                    let values: Vec<MicroMinotari> = utxos.iter().map(|utxo| utxo.value).collect();
                    let sum: MicroMinotari = values.iter().sum();
                    println!("Total number of UTXOs: {}", count);
                    println!("Total value of UTXOs : {}", sum);
                    if let Some(min) = values.iter().min() {
                        println!("Minimum value UTXO   : {}", min);
                    }
                    if count > 0 {
                        let average_val = sum.as_u64().div_euclid(count as u64);
                        let average = Minotari::from(MicroMinotari(average_val));
                        println!("Average value UTXO   : {}", average);
                    }
                    if let Some(max) = values.iter().max() {
                        println!("Maximum value UTXO   : {}", max);
                    }
                },
                Err(e) => eprintln!("CountUtxos error! {}", e),
            },
            SetBaseNode(args) => {
                if let Err(e) = set_base_node_peer(wallet.clone(), args.public_key.into(), args.address).await {
                    eprintln!("SetBaseNode error! {}", e);
                }
            },
            SetCustomBaseNode(args) => {
                match set_base_node_peer(wallet.clone(), args.public_key.into(), args.address).await {
                    Ok((public_key, net_address)) => {
                        if let Err(e) = wallet
                            .db
                            .set_client_key_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string(), public_key.to_string())
                        {
                            eprintln!("SetCustomBaseNode error! {}", e);
                        } else if let Err(e) = wallet
                            .db
                            .set_client_key_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string(), net_address.to_string())
                        {
                            eprintln!("SetCustomBaseNode error! {}", e);
                        } else {
                            println!("Custom base node peer saved in wallet database.");
                        }
                    },
                    Err(e) => eprintln!("SetCustomBaseNode error! {}", e),
                }
            },
            ClearCustomBaseNode => {
                match wallet
                    .db
                    .clear_client_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string())
                {
                    Ok(_) => match wallet.db.clear_client_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string()) {
                        Ok(true) => {
                            println!("Custom base node peer cleared from wallet database.")
                        },
                        Ok(false) => {
                            println!("Warning - custom base node peer not cleared from wallet database.")
                        },
                        Err(e) => eprintln!("ClearCustomBaseNode error! {}", e),
                    },
                    Err(e) => eprintln!("ClearCustomBaseNode error! {}", e),
                }
            },
            InitShaAtomicSwap(args) => {
                match init_sha_atomic_swap(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    UtxoSelectionCriteria::default(),
                    args.destination,
                    args.message,
                )
                .await
                {
                    Ok((tx_id, pre_image, output)) => {
                        debug!(target: LOG_TARGET, "minotari HTLC tx_id {}", tx_id);
                        let hash: [u8; 32] = Sha256::digest(pre_image.as_bytes()).into();
                        println!("pre_image hex: {}", pre_image.to_hex());
                        println!("pre_image hash: {}", hash.to_hex());
                        println!("Output hash: {}", output.hash().to_hex());
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("InitShaAtomicSwap error! {}", e),
                }
            },
            FinaliseShaAtomicSwap(args) => match args.output_hash[0].clone().try_into() {
                Ok(hash) => {
                    match finalise_sha_atomic_swap(
                        output_service.clone(),
                        transaction_service.clone(),
                        hash,
                        args.pre_image.into(),
                        config.fee_per_gram.into(),
                        args.message,
                    )
                    .await
                    {
                        Ok(tx_id) => {
                            debug!(target: LOG_TARGET, "claiming minotari HTLC tx_id {}", tx_id);
                            tx_ids.push(tx_id);
                        },
                        Err(e) => eprintln!("FinaliseShaAtomicSwap error! {}", e),
                    }
                },
                Err(e) => eprintln!("FinaliseShaAtomicSwap error! {}", e),
            },
            ClaimShaAtomicSwapRefund(args) => match args.output_hash[0].clone().try_into() {
                Ok(hash) => {
                    match claim_htlc_refund(
                        output_service.clone(),
                        transaction_service.clone(),
                        hash,
                        config.fee_per_gram.into(),
                        args.message,
                    )
                    .await
                    {
                        Ok(tx_id) => {
                            debug!(target: LOG_TARGET, "claiming minotari HTLC tx_id {}", tx_id);
                            tx_ids.push(tx_id);
                        },
                        Err(e) => eprintln!("ClaimShaAtomicSwapRefund error! {}", e),
                    }
                },
                Err(e) => eprintln!("FinaliseShaAtomicSwap error! {}", e),
            },

            RevalidateWalletDb => {
                if let Err(e) = output_service
                    .revalidate_all_outputs()
                    .await
                    .map_err(CommandError::OutputManagerError)
                {
                    eprintln!("RevalidateWalletDb error! {}", e);
                }
                if let Err(e) = transaction_service
                    .revalidate_all_transactions()
                    .await
                    .map_err(CommandError::TransactionServiceError)
                {
                    eprintln!("RevalidateWalletDb error! {}", e);
                }
            },
            RegisterValidatorNode(args) => {
                let tx_id = register_validator_node(
                    args.amount,
                    transaction_service.clone(),
                    args.validator_node_public_key.into(),
                    Signature::new(
                        args.validator_node_public_nonce.into(),
                        RistrettoSecretKey::from_vec(&args.validator_node_signature)?,
                    ),
                    UtxoSelectionCriteria::default(),
                    config.fee_per_gram * uT,
                    args.message,
                )
                .await?;
                debug!(target: LOG_TARGET, "Registering VN tx_id {}", tx_id);
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
                Err(err) => eprintln!("Error generating certificates: {}", err),
            },
            Sync(args) => {
                let mut utxo_scanner = wallet.utxo_scanner_service.clone();
                let mut receiver = utxo_scanner.get_event_receiver();

                if !online {
                    match wait_for_comms(&connectivity_requester).await {
                        Ok(..) => {
                            online = true;
                        },
                        Err(e) => {
                            eprintln!("Sync error! {}", e);
                            continue;
                        },
                    }
                }

                loop {
                    match receiver.recv().await {
                        Ok(event) => match event {
                            UtxoScannerEvent::ConnectingToBaseNode(_) => {
                                println!("Connecting to base node...");
                            },
                            UtxoScannerEvent::ConnectedToBaseNode(_, _) => {
                                println!("Connected to base node");
                            },
                            UtxoScannerEvent::ConnectionFailedToBaseNode { .. } => {
                                println!("Failed to connect to base node");
                            },
                            UtxoScannerEvent::ScanningRoundFailed {
                                num_retries,
                                retry_limit,
                                error,
                            } => {
                                println!(
                                    "Scanning round failed. Retries: {}/{}. Error: {}",
                                    num_retries, retry_limit, error
                                );
                            },
                            UtxoScannerEvent::Progress {
                                current_height,
                                tip_height,
                            } => {
                                println!("Progress: {}/{}", current_height, tip_height);
                                if current_height >= args.sync_to_height && args.sync_to_height > 0 {
                                    break;
                                }
                            },
                            UtxoScannerEvent::Completed {
                                final_height,
                                num_recovered,
                                value_recovered,
                                time_taken,
                            } => {
                                println!(
                                    "Completed! Height: {}, UTXOs recovered: {}, Value recovered: {}, Time taken: {}",
                                    final_height,
                                    num_recovered,
                                    value_recovered,
                                    time_taken.as_secs()
                                );

                                break;
                            },
                            UtxoScannerEvent::ScanningFailed => {
                                println!("Scanning failed");
                                break;
                            },
                        },
                        Err(e) => {
                            eprintln!("Sync error! {}", e);
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
                            eprintln!("Sync error! {}", e);
                            break;
                        },
                    }
                }
                println!("balance as of scanning height");
                match output_service.clone().get_balance().await {
                    Ok(balance) => {
                        println!("{}", balance);
                    },
                    Err(e) => eprintln!("GetBalance error! {}", e),
                }
            },
            ExportViewKeyAndSpendKey(args) => {
                let view_key = wallet.key_manager_service.get_view_key().await?;
                let spend_key = wallet.key_manager_service.get_spend_key().await?;
                let view_key_hex = view_key.pub_key.to_hex();
                let private_view_key_hex = wallet.key_manager_service.get_private_view_key().await?.to_hex();
                let spend_key_hex = spend_key.pub_key.to_hex();
                let output_file = args.output_file;
                #[derive(Serialize)]
                struct ViewKeyFile {
                    view_key: String,
                    public_view_key: String,
                    spend_key: String,
                }
                let view_key_file = ViewKeyFile {
                    view_key: private_view_key_hex.clone(),
                    public_view_key: view_key_hex.clone(),
                    spend_key: spend_key_hex.clone(),
                };
                let view_key_file_json =
                    serde_json::to_string(&view_key_file).map_err(|e| CommandError::JsonFile(e.to_string()))?;
                if let Some(file) = output_file {
                    let file = File::create(file).map_err(|e| CommandError::JsonFile(e.to_string()))?;
                    let mut file = LineWriter::new(file);
                    writeln!(file, "{}", view_key_file_json).map_err(|e| CommandError::JsonFile(e.to_string()))?;
                } else {
                    println!("View key: {}", private_view_key_hex);
                    println!("Spend key: {}", spend_key_hex);
                }
            },
            ImportPaperWallet(args) => {
                let temp_path = config
                    .db_file
                    .parent()
                    .ok_or(CommandError::General("No parent".to_string()))?
                    .join("temp");
                println!("saving temp wallet in: {:?}", temp_path);
                {
                    let passphrase = if args.passphrase.is_empty() {
                        None
                    } else {
                        Some(SafePassword::from(args.passphrase))
                    };
                    let seed = match (
                        args.seed_words.as_str().is_empty(),
                        args.cipher_seed.as_str().is_empty(),
                    ) {
                        (true, false) => {
                            let seed_words = SeedWords::from_str(args.seed_words.as_str())
                                .map_err(|e| CommandError::General(e.to_string()))?;

                            get_seed_from_seed_words(&seed_words, passphrase)
                                .map_err(|e| CommandError::General(e.to_string()))?
                        },
                        (false, true) => {
                            let bytes = bs58::decode(args.cipher_seed.as_str())
                                .into_vec()
                                .map_err(|e| CommandError::General(e.to_string()))?;
                            CipherSeed::from_enciphered_bytes(&bytes, passphrase)
                                .map_err(|e| CommandError::General(e.to_string()))?
                        },
                        (_, _) => {
                            return Err(CommandError::General(
                                "Either seed words or cipher seed must be provided".to_string(),
                            ))
                        },
                    };

                    let wallet_type = WalletType::DerivedKeys;
                    let password = SafePassword::from("password".to_string());
                    let shutdown = Shutdown::new();
                    let shutdown_signal = shutdown.to_signal();
                    let mut new_config = config.clone();
                    new_config.set_base_path(temp_path.clone());

                    let peer_config = PeerSeedsConfig::default();
                    let mut new_wallet = init_wallet(
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

                    let query = PeerQuery::new().select_where(|p| p.is_seed());
                    let peer_seeds = wallet
                        .comms
                        .peer_manager()
                        .perform_query(query)
                        .await
                        .map_err(|e| CommandError::General(e.to_string()))?;
                    // config
                    let base_node_peers = config
                        .base_node_service_peers
                        .iter()
                        .map(|s| SeedPeer::from_str(s))
                        .map(|r| r.map(Peer::from))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| CommandError::General(e.to_string()))?;
                    let selected_base_node = match config.custom_base_node {
                        Some(ref custom) => SeedPeer::from_str(custom)
                            .map(|node| Some(Peer::from(node)))
                            .map_err(|e| CommandError::General(e.to_string()))?,
                        None => get_custom_base_node_peer_from_db(&wallet),
                    };

                    let peer_config = PeerConfig::new(selected_base_node, base_node_peers, peer_seeds);

                    let base_nodes = peer_config
                        .get_base_node_peers()
                        .map_err(|e| CommandError::General(e.to_string()))?;
                    new_wallet
                        .set_base_node_peer(
                            base_nodes[0].public_key.clone(),
                            Some(
                                base_nodes[0]
                                    .last_address_used()
                                    .ok_or(CommandError::General("No address found".to_string()))?,
                            ),
                            Some(base_nodes),
                        )
                        .await
                        .map_err(|e| CommandError::General(e.to_string()))?;
                    wallet_recovery(&new_wallet, &peer_config, new_config.recovery_retry_limit)
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
                                eprintln!("Sync error! {}", e);
                                break;
                            },
                        }
                    }
                    println!("balance as of scanning height");
                    match oms.clone().get_balance().await {
                        Ok(balance) => {
                            println!("{}", balance);
                        },
                        Err(e) => eprintln!("GetBalance error! {}", e),
                    }
                    let mut tms = new_wallet.transaction_service.clone();
                    match tms
                        .scrape_wallet(
                            wallet
                                .get_wallet_one_sided_address()
                                .await
                                .map_err(|e| CommandError::General(e.to_string()))?,
                            config.fee_per_gram * uT,
                        )
                        .await
                        .map_err(CommandError::TransactionServiceError)
                    {
                        Ok(tx_id) => {
                            debug!(target: LOG_TARGET, "send-minotari concluded with tx_id {}", tx_id);
                            let duration = config.command_send_wait_timeout;
                            match timeout(duration, monitor_transactions(tms.clone(), vec![tx_id], wait_stage)).await {
                                Ok(txs) => {
                                    debug!(
                                        target: LOG_TARGET,
                                        "monitor_transactions done to stage {:?} with tx_ids: {:?}", wait_stage, txs
                                    );
                                    println!("Done! All transactions monitored to {:?} stage.", wait_stage);
                                },
                                Err(_e) => {
                                    println!(
                                        "The configured timeout ({:#?}) was reached before all transactions reached \
                                         the {:?} stage. See the logs for more info.",
                                        duration, wait_stage
                                    );
                                },
                            }
                        },
                        Err(e) => eprintln!("SendMinotari error! {}", e),
                    }
                }
                println!("removing temp wallet in: {:?}", temp_path);
                fs::remove_dir_all(temp_path)?;
            },
        }
    }
    if unban_peer_manager_peers {
        lift_temp_ban_peers(&wallet, &mut peer_list).await;
        return Ok(true);
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
            "wallet monitor_transactions timeout duration {:.2?}", duration
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
                    "monitor_transactions done to stage {:?} with tx_ids: {:?}", wait_stage, txs
                );
                println!("Done! All transactions monitored to {:?} stage.", wait_stage);
            },
            Err(_e) => {
                println!(
                    "The configured timeout ({:#?}) was reached before all transactions reached the {:?} stage. See \
                     the logs for more info.",
                    duration, wait_stage
                );
            },
        }
    }

    Ok(unban_peer_manager_peers)
}

async fn temp_ban_peers(wallet: &WalletSqlite, peer_list: &mut Vec<Peer>) {
    for peer in peer_list {
        let _unused = wallet
            .comms
            .connectivity()
            .remove_peer_from_allow_list(peer.node_id.clone())
            .await;
        let _unused = wallet
            .comms
            .connectivity()
            .ban_peer_until(
                peer.node_id.clone(),
                Duration::from_secs(24 * 60 * 60),
                "Busy with pre-mine spend".to_string(),
            )
            .await;
    }
}

async fn lift_temp_ban_peers(wallet: &WalletSqlite, peer_list: &mut Vec<Peer>) {
    for peer in peer_list {
        let _unused = wallet
            .comms
            .connectivity()
            .ban_peer_until(
                peer.node_id.clone(),
                Duration::from_millis(1),
                "Busy with pre-mine spend".to_string(),
            )
            .await;
        let _unused = wallet
            .comms
            .connectivity()
            .add_peer_to_allow_list(peer.node_id.clone())
            .await;
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
                return Err(format!("Error: Could not deserialize line: {}", line));
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

fn get_pre_mine_addition_file_name() -> String {
    match Network::get_current_or_user_setting_or_default() {
        Network::MainNet => "mainnet_pre_mine_addition.json".to_string(),
        Network::StageNet => "stagenet_pre_mine_addition.json".to_string(),
        Network::NextNet => "nextnet_pre_mine_addition.json".to_string(),
        Network::LocalNet => "esmeralda_pre_mine_addition.json".to_string(),
        Network::Igor => "igor_pre_mine_addition.json".to_string(),
        Network::Esmeralda => "esmeralda_pre_mine_addition.json".to_string(),
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
            max(all_indexes_len, all_indexes.len()) - min(all_indexes_len, all_indexes.len())
        ))
    }
}

fn sort_args_recipient_info(recipient_info: Vec<CliRecipientInfo>) -> Vec<CliRecipientInfo> {
    let mut args_recipient_info = recipient_info;
    args_recipient_info.sort_by(|a, b| a.recipient_address.to_hex().cmp(&b.recipient_address.to_hex()));
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
        fetched_outputs.push(utxos[index].clone());
    }
    Ok(fetched_outputs)
}

fn get_all_embedded_pre_mine_outputs() -> Result<Vec<TransactionOutput>, CommandError> {
    let pre_mine_contents = match Network::get_current_or_user_setting_or_default() {
        Network::MainNet => {
            include_str!("../../../../base_layer/core/src/blocks/pre_mine/mainnet_pre_mine.json")
        },
        Network::StageNet => {
            include_str!("../../../../base_layer/core/src/blocks/pre_mine/stagenet_pre_mine.json")
        },
        Network::NextNet => {
            include_str!("../../../../base_layer/core/src/blocks/pre_mine/nextnet_pre_mine.json")
        },
        Network::LocalNet => {
            include_str!("../../../../base_layer/core/src/blocks/pre_mine/esmeralda_pre_mine.json")
        },
        Network::Igor => {
            include_str!("../../../../base_layer/core/src/blocks/pre_mine/igor_pre_mine.json")
        },
        Network::Esmeralda => {
            include_str!("../../../../base_layer/core/src/blocks/pre_mine/esmeralda_pre_mine.json")
        },
    };
    let mut utxos = Vec::new();
    let mut counter = 1;
    let lines_count = pre_mine_contents.lines().count();
    for line in pre_mine_contents.lines() {
        if counter < lines_count {
            let utxo: TransactionOutput =
                serde_json::from_str(line).map_err(|e| CommandError::PreMine(format!("{}", e)))?;
            utxos.push(utxo);
        } else {
            break;
        }
        counter += 1;
    }

    Ok(utxos)
}

fn write_utxos_to_csv_file(
    utxos: Vec<(UnblindedOutput, Commitment)>,
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
            i + 1,
            utxo.version.as_u8(),
            utxo.value.0,
            if with_private_keys {utxo.spending_key.to_hex()} else { "*hidden*".to_string() },
            commitment.to_hex(),
            utxo.features.output_type,
            utxo.features.maturity,
            String::from_utf8(utxo.features.coinbase_extra.to_vec())
                .unwrap_or_else(|_| utxo.features.coinbase_extra.to_hex()),
            utxo.script.to_hex(),
            utxo.covenant.to_bytes().to_hex(),
            utxo.input_data.to_hex(),
            if with_private_keys {utxo.script_private_key.to_hex()} else { "*hidden*".to_string() },
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
            i + 1,
            utxo
        );
    }
    Ok(())
}

fn write_tx_to_csv_file(tx: WalletTransaction, file_path: PathBuf) -> Result<(), CommandError> {
    let file = File::create(file_path).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    let mut csv_file = LineWriter::new(file);
    let tx_string = serde_json::to_string(&tx).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    writeln!(csv_file, "{}", tx_string).map_err(|e| CommandError::CSVFile(e.to_string()))?;

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

#[allow(dead_code)]
fn write_json_file<P: AsRef<Path>, T: Serialize>(path: P, data: &T) -> Result<(), CommandError> {
    fs::create_dir_all(path.as_ref().parent().unwrap()).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let file = File::create(path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    serde_json::to_writer_pretty(file, data).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    Ok(())
}

#[allow(dead_code)]
fn read_json_file<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T, CommandError> {
    let file = File::open(path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    serde_json::from_reader(file).map_err(|e| CommandError::JsonFile(e.to_string()))
}

#[allow(dead_code)]
async fn get_tip_height(wallet: &WalletSqlite) -> Option<u64> {
    let client = wallet
        .wallet_connectivity
        .clone()
        .obtain_base_node_wallet_rpc_client_timeout(Duration::from_secs(10))
        .await;

    match client {
        Some(mut client) => client
            .get_tip_info()
            .await
            .ok()
            .and_then(|t| t.metadata)
            .map(|m| m.best_block_height),
        None => None,
    }
}
