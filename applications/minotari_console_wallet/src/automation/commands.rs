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
    collections::HashMap,
    convert::TryInto,
    fs,
    fs::File,
    io,
    io::{LineWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use digest::{crypto_common::rand_core::OsRng, Digest};
use futures::FutureExt;
use log::*;
use minotari_app_grpc::tls::certs::{generate_self_signed_certs, print_warning, write_cert_to_disk};
use minotari_wallet::{
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::{handle::OutputManagerHandle, UtxoSelectionCriteria},
    transaction_service::{
        handle::{TransactionEvent, TransactionServiceHandle},
        storage::models::WalletTransaction,
    },
    TransactionStage,
    WalletConfig,
    WalletSqlite,
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;
use tari_common_types::{
    burnt_proof::BurntProof,
    emoji::EmojiId,
    tari_address::TariAddress,
    transaction::TxId,
    types::{Commitment, FixedHash, HashOutput, PrivateKey, PublicKey, Signature},
};
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityRequester},
    multiaddr::Multiaddr,
    types::CommsPublicKey,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::{
    covenants::Covenant,
    transactions::{
        key_manager::TransactionKeyManagerInterface,
        tari_amount::{uT, MicroMinotari, Minotari},
        transaction_components::{
            encrypted_data::PaymentId,
            OutputFeatures,
            Transaction,
            TransactionInput,
            TransactionInputVersion,
            TransactionOutput,
            TransactionOutputVersion,
            UnblindedOutput,
            WalletOutput,
        },
    },
};
use tari_crypto::{
    keys::SecretKey,
    ristretto::{pedersen::PedersenCommitment, RistrettoSecretKey},
};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{script, CheckSigSchnorrSignature};
use tari_utilities::{encoding::Base58, hex::Hex, ByteArray};
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};

use super::error::CommandError;
use crate::{
    automation::{
        utils::{
            get_file_name,
            move_session_file_to_session_dir,
            out_dir,
            read_and_verify,
            read_session_info,
            read_verify_session_info,
            write_json_object_to_file_as_line,
            write_to_json_file,
        },
        Step1SessionInfo,
        Step2OutputsForLeader,
        Step2OutputsForSelf,
        Step3OutputsForParties,
        Step3OutputsForSelf,
        Step4OutputsForLeader,
    },
    cli::{CliCommands, MakeItRainTransactionType},
    utils::db::{CUSTOM_BASE_NODE_ADDRESS_KEY, CUSTOM_BASE_NODE_PUBLIC_KEY_KEY},
};

pub const LOG_TARGET: &str = "wallet::automation::commands";
// Faucet file names
pub(crate) const FILE_EXTENSION: &str = "json";
pub(crate) const SESSION_INFO: &str = "step_1_session_info";
pub(crate) const STEP_2_LEADER: &str = "step_2_for_leader_from_";
pub(crate) const STEP_2_SELF: &str = "step_2_for_self";
pub(crate) const STEP_3_SELF: &str = "step_3_for_self";
pub(crate) const STEP_3_PARTIES: &str = "step_3_for_parties";
pub(crate) const STEP_4_LEADER: &str = "step_4_for_leader_from_";

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
    output_hash: HashOutput,
    expected_commitment: PedersenCommitment,
    script_input_shares: HashMap<PublicKey, CheckSigSchnorrSignature>,
    script_signature_public_nonces: Vec<PublicKey>,
    sender_offset_public_key_shares: Vec<PublicKey>,
    metadata_ephemeral_public_key_shares: Vec<PublicKey>,
    dh_shared_secret_shares: Vec<PublicKey>,
    recipient_address: TariAddress,
) -> Result<(TxId, Transaction, PublicKey, PublicKey, PublicKey), CommandError> {
    wallet_transaction_service
        .encumber_aggregate_utxo(
            fee_per_gram,
            output_hash,
            expected_commitment,
            script_input_shares,
            script_signature_public_nonces,
            sender_offset_public_key_shares,
            metadata_ephemeral_public_key_shares,
            dh_shared_secret_shares,
            recipient_address,
        )
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
    let mut meta_sig = Signature::default();
    for sig in &meta_signatures {
        meta_sig = &meta_sig + sig;
    }
    let mut script_sig = Signature::default();
    for sig in &script_signatures {
        script_sig = &script_sig + sig;
    }

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
        .set_base_node_peer(public_key.clone(), Some(address.clone()))
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
) -> Result<(), CommandError> {
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
            FaucetGenerateSessionInfo(args) => {
                let commitment = if let Ok(val) = Commitment::from_hex(&args.commitment) {
                    val
                } else {
                    eprintln!("\nError: Invalid 'commitment' provided!\n");
                    continue;
                };
                let hash = if let Ok(val) = FixedHash::from_hex(&args.output_hash) {
                    val
                } else {
                    eprintln!("\nError: Invalid 'output_hash' provided!\n");
                    continue;
                };

                if args.verify_unspent_outputs {
                    let unspent_outputs = transaction_service.fetch_unspent_outputs(vec![hash]).await?;
                    if unspent_outputs.is_empty() {
                        eprintln!(
                            "\nError: Output with output_hash '{}' has already been spent!\n",
                            args.output_hash
                        );
                        continue;
                    }
                    if unspent_outputs[0].commitment() != &commitment {
                        eprintln!(
                            "\nError: Mismatched commitment '{}' and output_hash '{}'; not for the same output!\n",
                            args.commitment, args.output_hash
                        );
                        continue;
                    }
                }

                let mut session_id = PrivateKey::random(&mut OsRng).to_base58();
                session_id.truncate(16);
                let session_info = Step1SessionInfo {
                    session_id: session_id.clone(),
                    commitment_to_spend: args.commitment,
                    output_hash: args.output_hash,
                    recipient_address: args.recipient_address,
                    fee_per_gram: args.fee_per_gram,
                };
                let out_dir = out_dir(&session_info.session_id)?;
                let out_file = out_dir.join(get_file_name(SESSION_INFO, None));
                write_to_json_file(&out_file, true, session_info)?;
                println!();
                println!("Concluded step 1 'faucet-generate-session-info'");
                println!("Your session ID is:                 '{}'", session_id);
                println!("Your session's output directory is: '{}'", out_dir.display());
                println!("Session info saved to:              '{}'", out_file.display());
                println!("Send '{}' to parties for step 2", get_file_name(SESSION_INFO, None));
                println!();
            },
            FaucetCreatePartyDetails(args) => {
                if args.alias.is_empty() || args.alias.contains(" ") {
                    eprintln!("\nError: Alias cannot contain spaces!\n");
                    continue;
                }
                if args.alias.chars().any(|c| !c.is_alphanumeric() && c != '_') {
                    eprintln!("\nError: Alias contains invalid characters! Only alphanumeric and '_' are allowed.\n");
                    continue;
                }

                let wallet_spend_key = wallet.key_manager_service.get_spend_key().await?;
                let script_nonce_key = key_manager_service.get_random_key().await?;
                let sender_offset_key = key_manager_service.get_random_key().await?;
                let sender_offset_nonce = key_manager_service.get_random_key().await?;

                // Read session info
                let session_info = read_session_info(args.input_file.clone())?;

                let commitment = Commitment::from_hex(&session_info.commitment_to_spend)?;
                let shared_secret = key_manager_service
                    .get_diffie_hellman_shared_secret(
                        &sender_offset_key.key_id,
                        session_info
                            .recipient_address
                            .public_view_key()
                            .ok_or(CommandError::InvalidArgument("Missing public view key".to_string()))?,
                    )
                    .await?;
                let shared_secret_public_key = PublicKey::from_canonical_bytes(shared_secret.as_bytes())?;

                let script_input_signature = key_manager_service
                    .sign_script_message(&wallet_spend_key.key_id, commitment.as_bytes())
                    .await?;

                let out_dir = out_dir(&session_info.session_id)?;
                let step_2_outputs_for_leader = Step2OutputsForLeader {
                    script_input_signature,
                    wallet_public_spend_key: wallet_spend_key.pub_key,
                    public_script_nonce_key: script_nonce_key.pub_key,
                    public_sender_offset_key: sender_offset_key.pub_key,
                    public_sender_offset_nonce_key: sender_offset_nonce.pub_key,
                    dh_shared_secret_public_key: shared_secret_public_key,
                };
                let out_file_leader = out_dir.join(get_file_name(STEP_2_LEADER, Some(args.alias.clone())));
                write_json_object_to_file_as_line(&out_file_leader, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file_leader, false, step_2_outputs_for_leader)?;

                let step_2_outputs_for_self = Step2OutputsForSelf {
                    alias: args.alias.clone(),
                    wallet_spend_key_id: wallet_spend_key.key_id,
                    script_nonce_key_id: script_nonce_key.key_id,
                    sender_offset_key_id: sender_offset_key.key_id,
                    sender_offset_nonce_key_id: sender_offset_nonce.key_id,
                };
                let out_file_self = out_dir.join(get_file_name(STEP_2_SELF, None));
                write_json_object_to_file_as_line(&out_file_self, true, session_info.clone())?;
                write_json_object_to_file_as_line(&out_file_self, false, step_2_outputs_for_self)?;

                println!();
                println!("Concluded step 2 'faucet-create-party-details'");
                println!("Your session's output directory is '{}'", out_dir.display());
                move_session_file_to_session_dir(&session_info.session_id, &args.input_file)?;
                println!(
                    "Send '{}' to leader for step 3",
                    get_file_name(STEP_2_LEADER, Some(args.alias))
                );
                println!();
            },
            FaucetEncumberAggregateUtxo(args) => {
                // Read session info
                let session_info = read_verify_session_info(&args.session_id)?;

                #[allow(clippy::mutable_key_type)]
                let mut input_shares = HashMap::new();
                let mut script_signature_public_nonces = Vec::with_capacity(args.input_file_names.len());
                let mut sender_offset_public_key_shares = Vec::with_capacity(args.input_file_names.len());
                let mut metadata_ephemeral_public_key_shares = Vec::with_capacity(args.input_file_names.len());
                let mut dh_shared_secret_shares = Vec::with_capacity(args.input_file_names.len());
                for file_name in args.input_file_names {
                    // Read party input
                    let party_info =
                        read_and_verify::<Step2OutputsForLeader>(&args.session_id, &file_name, &session_info)?;
                    input_shares.insert(party_info.wallet_public_spend_key, party_info.script_input_signature);
                    script_signature_public_nonces.push(party_info.public_script_nonce_key);
                    sender_offset_public_key_shares.push(party_info.public_sender_offset_key);
                    metadata_ephemeral_public_key_shares.push(party_info.public_sender_offset_nonce_key);
                    dh_shared_secret_shares.push(party_info.dh_shared_secret_public_key);
                }

                match encumber_aggregate_utxo(
                    transaction_service.clone(),
                    session_info.fee_per_gram,
                    FixedHash::from_hex(&session_info.output_hash)
                        .map_err(|e| CommandError::InvalidArgument(e.to_string()))?,
                    Commitment::from_hex(&session_info.commitment_to_spend)?,
                    input_shares,
                    script_signature_public_nonces,
                    sender_offset_public_key_shares,
                    metadata_ephemeral_public_key_shares,
                    dh_shared_secret_shares,
                    session_info.recipient_address.clone(),
                )
                .await
                {
                    Ok((
                        tx_id,
                        transaction,
                        script_pubkey,
                        total_metadata_ephemeral_public_key,
                        total_script_nonce,
                    )) => {
                        let out_dir = out_dir(&args.session_id)?;
                        let step_3_outputs_for_self = Step3OutputsForSelf { tx_id };
                        let out_file = out_dir.join(get_file_name(STEP_3_SELF, None));
                        write_json_object_to_file_as_line(&out_file, true, session_info.clone())?;
                        write_json_object_to_file_as_line(&out_file, false, step_3_outputs_for_self)?;

                        let step_3_outputs_for_parties = Step3OutputsForParties {
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
                        };
                        let out_file = out_dir.join(get_file_name(STEP_3_PARTIES, None));
                        write_json_object_to_file_as_line(&out_file, true, session_info.clone())?;
                        write_json_object_to_file_as_line(&out_file, false, step_3_outputs_for_parties)?;

                        println!();
                        println!("Concluded step 3 'faucet-encumber-aggregate-utxo'");
                        println!("Send '{}' to parties for step 4", get_file_name(STEP_3_PARTIES, None));
                        println!();
                    },
                    Err(e) => eprintln!("\nError: Encumber aggregate transaction error! {}\n", e),
                }
            },
            FaucetCreateInputOutputSigs(args) => {
                // Read session info
                let session_info = read_verify_session_info(&args.session_id)?;
                // Read leader input
                let leader_info = read_and_verify::<Step3OutputsForParties>(
                    &args.session_id,
                    &get_file_name(STEP_3_PARTIES, None),
                    &session_info,
                )?;
                // Read own party info
                let party_info = read_and_verify::<Step2OutputsForSelf>(
                    &args.session_id,
                    &get_file_name(STEP_2_SELF, None),
                    &session_info,
                )?;

                // Script signature
                let challenge = TransactionInput::build_script_signature_challenge(
                    &TransactionInputVersion::get_current_version(),
                    &leader_info.script_signature_ephemeral_commitment,
                    &leader_info.script_signature_ephemeral_pubkey,
                    &leader_info.input_script,
                    &leader_info.input_stack,
                    &leader_info.total_script_key,
                    &Commitment::from_hex(&session_info.commitment_to_spend)?,
                );

                let mut script_signature = Signature::default();
                match key_manager_service
                    .sign_with_challenge_and_message(
                        &party_info.wallet_spend_key_id,
                        &party_info.script_nonce_key_id,
                        &challenge,
                    )
                    .await
                {
                    Ok(signature) => {
                        script_signature = signature;
                    },
                    Err(e) => eprintln!("\nError: Script signature SignMessage error! {}\n", e),
                }

                // Metadata signature
                let script_offset = key_manager_service
                    .get_script_offset(&vec![party_info.wallet_spend_key_id], &vec![party_info
                        .sender_offset_key_id
                        .clone()])
                    .await?;
                let challenge = TransactionOutput::build_metadata_signature_challenge(
                    &TransactionOutputVersion::get_current_version(),
                    &script!(PushPubKey(Box::new(
                        session_info.recipient_address.public_spend_key().clone()
                    ))),
                    &leader_info.output_features,
                    &leader_info.sender_offset_pubkey,
                    &leader_info.metadata_signature_ephemeral_commitment,
                    &leader_info.metadata_signature_ephemeral_pubkey,
                    &leader_info.output_commitment,
                    &Covenant::default(),
                    &leader_info.encrypted_data,
                    MicroMinotari::zero(),
                );

                let mut metadata_signature = Signature::default();
                match key_manager_service
                    .sign_with_challenge_and_message(
                        &party_info.sender_offset_key_id,
                        &party_info.sender_offset_nonce_key_id,
                        &challenge,
                    )
                    .await
                {
                    Ok(signature) => {
                        metadata_signature = signature;
                    },
                    Err(e) => eprintln!("\nError: Metadata signature SignMessage error! {}\n", e),
                }

                if script_signature.get_signature() == Signature::default().get_signature() ||
                    metadata_signature.get_signature() == Signature::default().get_signature()
                {
                    eprintln!("\nError: Script and/or metadata signatures not created!\n")
                } else {
                    let step_4_outputs_for_leader = Step4OutputsForLeader {
                        script_signature,
                        metadata_signature,
                        script_offset,
                    };

                    let out_dir = out_dir(&args.session_id)?;
                    let out_file = out_dir.join(get_file_name(STEP_4_LEADER, Some(party_info.alias.clone())));
                    write_json_object_to_file_as_line(&out_file, true, session_info.clone())?;
                    write_json_object_to_file_as_line(&out_file, false, step_4_outputs_for_leader)?;

                    println!();
                    println!("Concluded step 4 'faucet-create-input-output-sigs'");
                    println!(
                        "Send '{}' to leader for step 5",
                        get_file_name(STEP_4_LEADER, Some(party_info.alias))
                    );
                    println!();
                }
            },
            FaucetSpendAggregateUtxo(args) => {
                // Read session info
                let session_info = read_verify_session_info(&args.session_id)?;

                let mut metadata_signatures = Vec::with_capacity(args.input_file_names.len());
                let mut script_signatures = Vec::with_capacity(args.input_file_names.len());
                let mut offset = PrivateKey::default();
                for file_name in args.input_file_names {
                    // Read party input
                    let party_info =
                        read_and_verify::<Step4OutputsForLeader>(&args.session_id, &file_name, &session_info)?;
                    metadata_signatures.push(party_info.metadata_signature);
                    script_signatures.push(party_info.script_signature);
                    offset = &offset + &party_info.script_offset;
                }

                // Read own party info
                let leader_info = read_and_verify::<Step3OutputsForSelf>(
                    &args.session_id,
                    &get_file_name(STEP_3_SELF, None),
                    &session_info,
                )?;

                match finalise_aggregate_utxo(
                    transaction_service.clone(),
                    leader_info.tx_id.as_u64(),
                    metadata_signatures,
                    script_signatures,
                    offset,
                )
                .await
                {
                    Ok(_v) => {
                        println!();
                        println!("Concluded step 5 'faucet-spend-aggregate-utxo'");
                        println!();
                    },
                    Err(e) => println!("\nError: Error completing transaction! {}\n", e),
                }
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

    Ok(())
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
            String::from_utf8(utxo.features.coinbase_extra.clone())
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
