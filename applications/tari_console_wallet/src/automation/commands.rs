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
    convert::{From, TryInto},
    fs,
    fs::File,
    io,
    io::{LineWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use digest::Digest;
use futures::FutureExt;
use log::*;
use rand::rngs::OsRng;
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;
use strum_macros::{Display, EnumIter, EnumString};
use tari_app_grpc::authentication::salted_password::create_salted_hashed_password;
use tari_common_types::{
    emoji::EmojiId,
    transaction::TxId,
    types::{Commitment, CommitmentFactory, FixedHash, PrivateKey, PublicKey, Signature},
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
        tari_amount::{uT, MicroTari, Tari},
        transaction_components::{
            EncryptedValue,
            OutputFeatures,
            Transaction,
            TransactionInput,
            TransactionInputVersion,
            TransactionOutput,
            TransactionOutputVersion,
            UnblindedOutput,
        },
    },
};
use tari_crypto::keys::SecretKey;
use tari_script::{script, ExecutionStack, TariScript};
use tari_utilities::{hex::Hex, ByteArray};
use tari_wallet::{
    connectivity_service::WalletConnectivityInterface,
    error::WalletError,
    key_manager_service::{KeyManagerInterface, NextKeyResult},
    output_manager_service::{handle::OutputManagerHandle, UtxoSelectionCriteria},
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    TransactionStage,
    WalletConfig,
    WalletSqlite,
};
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};
use zeroize::Zeroizing;

use super::error::CommandError;
use crate::{
    cli::{CliCommands, MakeItRainTransactionType},
    utils::db::{CUSTOM_BASE_NODE_ADDRESS_KEY, CUSTOM_BASE_NODE_PUBLIC_KEY_KEY},
};

pub const LOG_TARGET: &str = "wallet::automation::commands";

/// Enum representing commands used by the wallet
#[derive(Clone, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum WalletCommand {
    GetBalance,
    SendTari,
    SendOneSided,
    CreateKeyPair,
    CreateAggregateSignatureUtxo,
    SignMessage,
    EncumberAggregateUtxo,
    MakeItRain,
    CoinSplit,
    DiscoverPeer,
    Whois,
    ExportUtxos,
    ExportSpentUtxos,
    CountUtxos,
    SetBaseNode,
    SetCustomBaseNode,
    ClearCustomBaseNode,
    InitShaAtomicSwap,
    FinaliseShaAtomicSwap,
    ClaimShaAtomicSwapRefund,
    RegisterAsset,
    MintTokens,
    CreateInitialCheckpoint,
    RevalidateWalletDb,
}

#[derive(Debug)]
pub struct SentTransaction {}

/// Send a normal negotiated transaction to a recipient
pub async fn send_tari(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroTari,
    dest_pubkey: PublicKey,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_transaction(
            dest_pubkey,
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
    amount: MicroTari,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .burn_tari(amount, UtxoSelectionCriteria::default(), fee_per_gram * uT, message)
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn create_aggregate_signature_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    amount: MicroTari,
    fee_per_gram: MicroTari,
    n: u8,
    m: u8,
    public_keys: Vec<PublicKey>,
    message: String,
) -> Result<(TxId, FixedHash), CommandError> {
    let mut msg = [0u8; 32];
    msg.copy_from_slice(message.as_bytes());

    wallet_transaction_service
        .create_aggregate_signature_utxo(amount, fee_per_gram, n, m, public_keys, msg)
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// encumbers a n-of-m transaction
async fn encumber_aggregate_utxo(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: MicroTari,
    output_hash: String,
    signatures: Vec<Signature>,
    total_script_pubkey: PublicKey,
    total_offset_pubkey: PublicKey,
    total_signature_nonce: PublicKey,
    metadata_signature_nonce: PublicKey,
    wallet_script_secret_key: String,
) -> Result<(TxId, Transaction, PublicKey), CommandError> {
    wallet_transaction_service
        .encumber_aggregate_utxo(
            fee_per_gram,
            output_hash,
            signatures,
            total_script_pubkey,
            total_offset_pubkey,
            total_signature_nonce,
            metadata_signature_nonce,
            wallet_script_secret_key,
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
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    dest_pubkey: PublicKey,
    message: String,
) -> Result<(TxId, PublicKey, TransactionOutput), CommandError> {
    let (tx_id, pre_image, output) = wallet_transaction_service
        .send_sha_atomic_swap_transaction(dest_pubkey, amount, selection_criteria, fee_per_gram * uT, message)
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
    fee_per_gram: MicroTari,
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
    fee_per_gram: MicroTari,
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

/// Send a one-sided transaction to a recipient
pub async fn send_one_sided(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    dest_pubkey: PublicKey,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_one_sided_transaction(
            dest_pubkey,
            amount,
            selection_criteria,
            OutputFeatures::default(),
            fee_per_gram * uT,
            message,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn send_one_sided_to_stealth_address(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroTari,
    selection_criteria: UtxoSelectionCriteria,
    dest_pubkey: PublicKey,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_one_sided_to_stealth_address_transaction(
            dest_pubkey,
            amount,
            selection_criteria,
            OutputFeatures::default(),
            fee_per_gram * uT,
            message,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn coin_split(
    amount_per_split: MicroTari,
    num_splits: usize,
    fee_per_gram: MicroTari,
    message: String,
    output_service: &mut OutputManagerHandle,
    transaction_service: &mut TransactionServiceHandle,
) -> Result<TxId, CommandError> {
    let (tx_id, tx, amount) = output_service
        .create_coin_split(vec![], amount_per_split, num_splits as usize, fee_per_gram)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, message)
        .await?;

    Ok(tx_id)
}

pub fn sign_message(private_key: String, challenge: String) -> Result<Signature, CommandError> {
    let private_key =
        PrivateKey::from_hex(private_key.as_str()).map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
    let challenge = challenge.as_bytes();

    let nonce = PrivateKey::random(&mut OsRng);
    let signature = Signature::sign(private_key, nonce, challenge).map_err(CommandError::FailedSignature)?;

    Ok(signature)
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
    wallet.set_base_node_peer(public_key.clone(), address.clone()).await?;
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

#[allow(clippy::too_many_lines)]
pub async fn make_it_rain(
    wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    transactions_per_second: u32,
    duration: Duration,
    start_amount: MicroTari,
    increase_amount: MicroTari,
    start_time: DateTime<Utc>,
    destination: PublicKey,
    transaction_type: MakeItRainTransactionType,
    message: String,
) -> Result<(), CommandError> {
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

        let num_txs = (f64::from(transactions_per_second) * duration.as_secs() as f64) as usize;
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
                let target_ms = (i as f64 / f64::from(transactions_per_second) / 1000.0) as i64;
                if target_ms - actual_ms > 0 {
                    // Maximum delay between Txs set to 120 s
                    sleep(Duration::from_millis((target_ms - actual_ms).min(120_000i64) as u64)).await;
                }
                let delayed_for = Instant::now();
                let sender_clone = sender.clone();
                let fee = fee_per_gram;
                let pk = destination.clone();
                let msg = message.clone();
                tokio::task::spawn(async move {
                    let spawn_start = Instant::now();
                    // Send transaction
                    let tx_id = match transaction_type {
                        MakeItRainTransactionType::Interactive => {
                            send_tari(tx_service, fee, amount, pk.clone(), msg.clone()).await
                        },
                        MakeItRainTransactionType::OneSided => {
                            send_one_sided(
                                tx_service,
                                fee,
                                amount,
                                UtxoSelectionCriteria::default(),
                                pk.clone(),
                                msg.clone(),
                            )
                            .await
                        },
                        MakeItRainTransactionType::StealthOneSided => {
                            send_one_sided_to_stealth_address(
                                tx_service,
                                fee,
                                amount,
                                UtxoSelectionCriteria::default(),
                                pk.clone(),
                                msg.clone(),
                            )
                            .await
                        },
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
    let key_manager_service = wallet.key_manager_service.clone();
    let dht_service = wallet.dht_service.discovery_service_requester().clone();
    let connectivity_requester = wallet.comms.connectivity();
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
            BurnTari(args) => {
                match burn_tari(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.message,
                )
                .await
                {
                    Ok(tx_id) => {
                        debug!(target: LOG_TARGET, "burn tari concluded with tx_id {}", tx_id);
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("BurnTari error! {}", e),
                }
            },
            CreateKeyPair(args) => match key_manager_service.create_key_pair(args.key_branch).await {
                Ok((sk, pk)) => {
                    println!(
                        "New key pair: 
                                1. secret key: {}, 
                                2. public key: {}",
                        *Zeroizing::new(sk.to_hex()),
                        pk.to_hex()
                    )
                },
                Err(e) => eprintln!("CreateKeyPair error! {}", e),
            },
            CreateAggregateSignatureUtxo(args) => match create_aggregate_signature_utxo(
                transaction_service.clone(),
                args.amount,
                args.fee_per_gram,
                args.n,
                args.m,
                args.public_keys
                    .iter()
                    .map(|pk| PublicKey::from(pk.clone()))
                    .collect::<Vec<_>>(),
                args.message,
            )
            .await
            {
                Ok((tx_id, output_hash)) => {
                    println!(
                        "Created an utxo with n-of-m aggregate public key, with:
                            1. n = {},
                            2. m = {}, 
                            3. tx id = {},
                            4. output hash = {}",
                        args.n, args.m, tx_id, output_hash
                    )
                },
                Err(e) => eprintln!("CreateAggregateSignatureUtxo error! {}", e),
            },
            SignMessage(args) => match sign_message(args.private_key, args.challenge) {
                Ok(sgn) => {
                    println!(
                        "Sign message: 
                                1. signature: {},
                                2. public nonce: {}",
                        sgn.get_signature().to_hex(),
                        sgn.get_public_nonce().to_hex(),
                    )
                },
                Err(e) => eprintln!("SignMessage error! {}", e),
            },
            EncumberAggregateUtxo(args) => {
                let mut total_script_pub_key = PublicKey::default();
                for sig in args.script_pubkeys {
                    total_script_pub_key = sig.into();
                }
                let mut total_offset_pub_key = PublicKey::default();
                for sig in args.offset_pubkeys {
                    total_offset_pub_key = sig.into();
                }
                let mut total_sig_nonce = PublicKey::default();
                for sig in args.script_signature_nonces {
                    total_sig_nonce = sig.into();
                }
                let mut total_meta_nonce = PublicKey::default();
                for sig in args.metadata_signature_nonces {
                    total_meta_nonce = sig.into();
                }
                match encumber_aggregate_utxo(
                    transaction_service.clone(),
                    args.fee_per_gram,
                    args.output_hash,
                    args.signatures.iter().map(|sgn| sgn.clone().into()).collect::<Vec<_>>(),
                    total_script_pub_key,
                    total_offset_pub_key,
                    total_sig_nonce,
                    total_meta_nonce,
                    args.wallet_script_secret_key,
                )
                .await
                {
                    Ok((tx_id, transaction, script_pubkey)) => {
                        println!(
                            "Encumber aggregate utxo:
                            1. Tx_id: {}
                            2. input_commitment: {},
                            3. input_stack_hex: {},
                            4. input_script_hex: {},
                            5. total_script_key_hex: {},
                            6. total_script_nonce_hex: {},
                            7. output_commitment: {},
                            8. output_hash: {},
                            9. sender_offset_pubkey: {},
                            10. meta_signature_nonce: {},
                            11. total_public_offset: {}",
                            tx_id,
                            transaction.body.inputs()[0].commitment().unwrap().to_hex(),
                            transaction.body.inputs()[0].input_data.to_hex(),
                            transaction.body.inputs()[0].script().unwrap().to_hex(),
                            script_pubkey.to_hex(),
                            transaction.body.inputs()[0].script_signature.public_nonce().to_hex(),
                            transaction.body.outputs()[0].commitment().to_hex(),
                            transaction.body.outputs()[0].hash().to_hex(),
                            transaction.body.outputs()[0].sender_offset_public_key.to_hex(),
                            transaction.body.outputs()[0].metadata_signature.public_nonce().to_hex(),
                            transaction.script_offset.to_hex(),
                        )
                    },
                    Err(e) => println!("Encumber aggregate transaction error! {}", e),
                }
            },
            SpendAggregateUtxo(args) => {
                let mut offset = PrivateKey::default();
                for key in args.script_offset_keys {
                    let secret_key =
                        PrivateKey::from_hex(&key).map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                    offset = &offset + &secret_key;
                }

                match finalise_aggregate_utxo(
                    transaction_service.clone(),
                    args.ix_id,
                    args.meta_signatures
                        .iter()
                        .map(|sgn| sgn.clone().into())
                        .collect::<Vec<_>>(),
                    args.script_signatures
                        .iter()
                        .map(|sgn| sgn.clone().into())
                        .collect::<Vec<_>>(),
                    offset,
                )
                .await
                {
                    Ok(_v) => println!("Transactions successfully completed"),
                    Err(e) => println!("Error completing transaction! {}", e),
                }
            },
            CreateScriptSig(args) => {
                let private_key =
                    PrivateKey::from_hex(&args.secret_key).map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let private_nonce = PrivateKey::from_hex(&args.secret_nonce)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let script = TariScript::from_hex(&args.input_script)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let input_data = ExecutionStack::from_hex(&args.input_stack)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let commitment =
                    Commitment::from_hex(&args.commitment).map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let total_nonce = Commitment::from_hex(&args.total_nonce)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let challenge = TransactionInput::build_script_challenge(
                    TransactionInputVersion::get_current_version(),
                    &total_nonce,
                    &script,
                    &input_data,
                    &args.total_script_key.into(),
                    &commitment,
                );
                let signature =
                    Signature::sign(private_key, private_nonce, &challenge).map_err(CommandError::FailedSignature)?;
                println!(
                    "Sign script sig:
                                1. signature: {},
                                2. public nonce: {}",
                    signature.get_signature().to_hex(),
                    signature.get_public_nonce().to_hex(),
                )
            },
            CreateMetaSig(args) => {
                let private_key = PrivateKey::from_hex(&args.secret_offset_key)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let private_script_key = PrivateKey::from_hex(&args.secret_script_key)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let private_nonce = PrivateKey::from_hex(&args.secret_nonce)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let offset = private_script_key - &private_key;
                let script = script!(Nop);
                let commitment =
                    Commitment::from_hex(&args.commitment).map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let covenant = Covenant::default();
                let encrypted_value = EncryptedValue::default();
                let output_features = OutputFeatures::default();
                let total_nonce = Commitment::from_hex(&args.total_nonce)
                    .map_err(|e| CommandError::InvalidArgument(e.to_string()))?;
                let minimum_value_promise = MicroTari::zero();
                trace!(
                    target: LOG_TARGET,
                    "version: {:?}",
                    TransactionOutputVersion::get_current_version()
                );
                trace!(target: LOG_TARGET, "script: {:?}", script);
                trace!(target: LOG_TARGET, "output features: {:?}", output_features);
                let offsetkey: PublicKey = args.total_meta_key.clone().into();
                trace!(target: LOG_TARGET, "sender_offset_public_key: {:?}", offsetkey);
                trace!(target: LOG_TARGET, "nonce_commitment: {:?}", total_nonce);
                trace!(target: LOG_TARGET, "commitment: {:?}", commitment);
                trace!(target: LOG_TARGET, "covenant: {:?}", covenant);
                trace!(target: LOG_TARGET, "encrypted_value: {:?}", encrypted_value);
                trace!(target: LOG_TARGET, "minimum_value_promise: {:?}", minimum_value_promise);
                let challenge = TransactionOutput::build_metadata_signature_challenge(
                    TransactionOutputVersion::get_current_version(),
                    &script,
                    &output_features,
                    &args.total_meta_key.into(),
                    &total_nonce,
                    &commitment,
                    &covenant,
                    &encrypted_value,
                    minimum_value_promise,
                );
                trace!(target: LOG_TARGET, "meta challange: {:?}", challenge);
                let signature =
                    Signature::sign(private_key, private_nonce, &challenge).map_err(CommandError::FailedSignature)?;
                println!(
                    "Sign meta sig:
                                1. signature: {},
                                2. public nonce: {},
                     Script offset: {}",
                    signature.get_signature().to_hex(),
                    signature.get_public_nonce().to_hex(),
                    offset.to_hex(),
                )
            },
            SendTari(args) => {
                match send_tari(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.destination.into(),
                    args.message,
                )
                .await
                {
                    Ok(tx_id) => {
                        debug!(target: LOG_TARGET, "send-tari concluded with tx_id {}", tx_id);
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("SendTari error! {}", e),
                }
            },
            SendOneSided(args) => {
                match send_one_sided(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    UtxoSelectionCriteria::default(),
                    args.destination.into(),
                    args.message,
                )
                .await
                {
                    Ok(tx_id) => {
                        debug!(target: LOG_TARGET, "send-one-sided concluded with tx_id {}", tx_id);
                        tx_ids.push(tx_id);
                    },
                    Err(e) => eprintln!("SendOneSided error! {}", e),
                }
            },
            SendOneSidedToStealthAddress(args) => {
                match send_one_sided_to_stealth_address(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    UtxoSelectionCriteria::default(),
                    args.destination.into(),
                    args.message,
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
                    args.destination.into(),
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
                let emoji_id = EmojiId::from_public_key(&public_key).to_emoji_string();

                println!("Public Key: {}", public_key.to_hex());
                println!("Emoji ID  : {}", emoji_id);
            },
            ExportUtxos(args) => match output_service.get_unspent_outputs().await {
                Ok(utxos) => {
                    let count = utxos.len();
                    let sum: MicroTari = utxos.iter().map(|utxo| utxo.value).sum();
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_utxos_to_csv_file(utxos, file) {
                            eprintln!("ExportUtxos error! {}", e);
                        }
                    } else {
                        for (i, utxo) in utxos.iter().enumerate() {
                            println!("{}. Value: {} {}", i + 1, utxo.value, utxo.features);
                        }
                    }
                    println!("Total number of UTXOs: {}", count);
                    println!("Total value of UTXOs: {}", sum);
                },
                Err(e) => eprintln!("ExportUtxos error! {}", e),
            },
            ExportSpentUtxos(args) => match output_service.get_spent_outputs().await {
                Ok(utxos) => {
                    let count = utxos.len();
                    let sum: MicroTari = utxos.iter().map(|utxo| utxo.value).sum();
                    if let Some(file) = args.output_file {
                        if let Err(e) = write_utxos_to_csv_file(utxos, file) {
                            eprintln!("ExportSpentUtxos error! {}", e);
                        }
                    } else {
                        for (i, utxo) in utxos.iter().enumerate() {
                            println!("{}. Value: {} {}", i + 1, utxo.value, utxo.features);
                        }
                    }
                    println!("Total number of UTXOs: {}", count);
                    println!("Total value of UTXOs: {}", sum);
                },
                Err(e) => eprintln!("ExportSpentUtxos error! {}", e),
            },
            CountUtxos => match output_service.get_unspent_outputs().await {
                Ok(utxos) => {
                    let count = utxos.len();
                    let values: Vec<MicroTari> = utxos.iter().map(|utxo| utxo.value).collect();
                    let sum: MicroTari = values.iter().sum();
                    println!("Total number of UTXOs: {}", count);
                    println!("Total value of UTXOs : {}", sum);
                    if let Some(min) = values.iter().min() {
                        println!("Minimum value UTXO   : {}", min);
                    }
                    if count > 0 {
                        let average = f64::from(sum) / count as f64;
                        let average = Tari::from(MicroTari(average.round() as u64));
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
                    args.destination.into(),
                    args.message,
                )
                .await
                {
                    Ok((tx_id, pre_image, output)) => {
                        debug!(target: LOG_TARGET, "tari HTLC tx_id {}", tx_id);
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
                            debug!(target: LOG_TARGET, "claiming tari HTLC tx_id {}", tx_id);
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
                            debug!(target: LOG_TARGET, "claiming tari HTLC tx_id {}", tx_id);
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
            HashGrpcPassword(args) => {
                match config
                    .grpc_authentication
                    .username_password()
                    .ok_or_else(|| CommandError::General("GRPC basic auth is not configured".to_string()))
                {
                    Ok((username, password)) => {
                        match create_salted_hashed_password(password.reveal())
                            .map_err(|e| CommandError::General(e.to_string()))
                        {
                            Ok(hashed_password) => {
                                if args.short {
                                    println!("{}", *hashed_password);
                                } else {
                                    println!("Your hashed password is:");
                                    println!("{}", *hashed_password);
                                    println!();
                                    println!(
                                        "Use HTTP basic auth with username '{}' and the hashed password to make GRPC \
                                         requests",
                                        username
                                    );
                                }
                            },
                            Err(e) => eprintln!("HashGrpcPassword error! {}", e),
                        }
                    },
                    Err(e) => eprintln!("HashGrpcPassword error! {}", e),
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

fn write_utxos_to_csv_file(utxos: Vec<UnblindedOutput>, file_path: PathBuf) -> Result<(), CommandError> {
    let factory = CommitmentFactory::default();
    let file = File::create(file_path).map_err(|e| CommandError::CSVFile(e.to_string()))?;
    let mut csv_file = LineWriter::new(file);
    writeln!(
        csv_file,
        r##""index","value","spending_key","commitment","flags","maturity","script","input_data","script_private_key","sender_offset_public_key","public_nonce","signature_u","signature_v""##
    )
    .map_err(|e| CommandError::CSVFile(e.to_string()))?;
    for (i, utxo) in utxos.iter().enumerate() {
        writeln!(
            csv_file,
            r##""{}","{}","{}","{}","{:?}","{}","{}","{}","{}","{}","{}","{}","{}""##,
            i + 1,
            utxo.value.0,
            utxo.spending_key.to_hex(),
            utxo.as_transaction_input(&factory)?
                .commitment()
                .map_err(|e| CommandError::WalletError(WalletError::TransactionError(e)))?
                .to_hex(),
            utxo.features.output_type,
            utxo.features.maturity,
            utxo.script.to_hex(),
            utxo.input_data.to_hex(),
            utxo.script_private_key.to_hex(),
            utxo.sender_offset_public_key.to_hex(),
            utxo.metadata_signature.public_nonce().to_hex(),
            utxo.metadata_signature.u().to_hex(),
            utxo.metadata_signature.v().to_hex(),
        )
        .map_err(|e| CommandError::CSVFile(e.to_string()))?;
    }
    Ok(())
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
fn write_to_issuer_key_file<P: AsRef<Path>>(path: P, key_result: NextKeyResult) -> Result<(), CommandError> {
    let file_exists = path.as_ref().exists();
    let mut root = if file_exists {
        read_json_file::<_, Vec<serde_json::Value>>(&path).map_err(|e| CommandError::JsonFile(e.to_string()))?
    } else {
        vec![]
    };
    let json = serde_json::json!({
        "name": format!("issuer-key-{}", key_result.index),
        "public_key": key_result.to_public_key().to_hex(),
        "secret_key": key_result.key.to_hex(),
    });
    root.push(json);
    write_json_file(path, &root).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    Ok(())
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
            .and_then(|m| m.height_of_longest_chain),
        None => None,
    }
}
