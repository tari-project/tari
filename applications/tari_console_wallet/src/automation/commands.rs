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
    convert::TryFrom,
    fs::File,
    io::{LineWriter, Write},
    str::FromStr,
    time::{Duration, Instant},
};

use chrono::Utc;
use digest::Digest;
use futures::FutureExt;
use log::*;
use sha2::Sha256;
use strum_macros::{Display, EnumIter, EnumString};
use tari_common::GlobalConfig;
use tari_common_types::{array::copy_into_fixed_array, emoji::EmojiId, transaction::TxId, types::PublicKey};
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityRequester},
    multiaddr::Multiaddr,
    types::CommsPublicKey,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::transactions::{
    tari_amount::{uT, MicroTari, Tari},
    transaction_components::{TransactionOutput, UnblindedOutput},
};
use tari_crypto::{
    keys::PublicKey as PublicKeyTrait,
    ristretto::pedersen::PedersenCommitmentFactory,
    tari_utilities::{ByteArray, Hashable},
};
use tari_utilities::hex::Hex;
use tari_wallet::{
    error::WalletError,
    output_manager_service::handle::OutputManagerHandle,
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    WalletSqlite,
};
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};

use super::error::CommandError;
use crate::{
    automation::command_parser::{ParsedArgument, ParsedCommand},
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
    CreateCommitteeDefinition,
}

#[derive(Debug, EnumString, PartialEq, Clone)]
pub enum TransactionStage {
    Initiated,
    DirectSendOrSaf,
    Negotiated,
    Broadcast,
    MinedUnconfirmed,
    Mined,
    Timedout,
}

#[derive(Debug)]
pub struct SentTransaction {}

fn get_transaction_parameters(args: Vec<ParsedArgument>) -> Result<(MicroTari, PublicKey, String), CommandError> {
    use ParsedArgument::*;
    let amount = match args[0].clone() {
        Amount(mtari) => Ok(mtari),
        _ => Err(CommandError::Argument),
    }?;

    let dest_pubkey = match args[1].clone() {
        PublicKey(key) => Ok(key),
        _ => Err(CommandError::Argument),
    }?;

    let message = match args[2].clone() {
        Text(msg) => Ok(msg),
        _ => Err(CommandError::Argument),
    }?;

    Ok((amount, dest_pubkey, message))
}

fn get_init_sha_atomic_swap_parameters(
    args: Vec<ParsedArgument>,
) -> Result<(MicroTari, PublicKey, String), CommandError> {
    use ParsedArgument::*;
    let amount = match args[0].clone() {
        Amount(mtari) => Ok(mtari),
        _ => Err(CommandError::Argument),
    }?;

    let dest_pubkey = match args[1].clone() {
        PublicKey(key) => Ok(key),
        _ => Err(CommandError::Argument),
    }?;

    let message = match args[2].clone() {
        Text(msg) => Ok(msg),
        _ => Err(CommandError::Argument),
    }?;

    Ok((amount, dest_pubkey, message))
}

/// Send a normal negotiated transaction to a recipient
pub async fn send_tari(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError> {
    let (amount, dest_pubkey, message) = get_transaction_parameters(args)?;
    wallet_transaction_service
        .send_transaction(dest_pubkey, amount, fee_per_gram * uT, message)
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// publishes a tari-SHA atomic swap HTLC transaction
pub async fn init_sha_atomic_swap(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    args: Vec<ParsedArgument>,
) -> Result<(TxId, PublicKey, TransactionOutput), CommandError> {
    let (amount, dest_pubkey, message) = get_init_sha_atomic_swap_parameters(args)?;

    let (tx_id, pre_image, output) = wallet_transaction_service
        .send_sha_atomic_swap_transaction(dest_pubkey, amount, fee_per_gram * uT, message)
        .await
        .map_err(CommandError::TransactionServiceError)?;
    Ok((tx_id, pre_image, output))
}

/// claims a tari-SHA atomic swap HTLC transaction
pub async fn finalise_sha_atomic_swap(
    mut output_service: OutputManagerHandle,
    mut transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError> {
    use ParsedArgument::*;
    let output = match args[0].clone() {
        Hash(output) => Ok(output),
        _ => Err(CommandError::Argument),
    }?;

    let pre_image = match args[1].clone() {
        PublicKey(key) => Ok(key),
        _ => Err(CommandError::Argument),
    }?;
    let (tx_id, _fee, amount, tx) = output_service
        .create_claim_sha_atomic_swap_transaction(output, pre_image, MicroTari(25))
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, "Claimed HTLC atomic swap".into())
        .await?;
    Ok(tx_id)
}

/// claims a HTLC refund transaction
pub async fn claim_htlc_refund(
    mut output_service: OutputManagerHandle,
    mut transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError> {
    use ParsedArgument::*;
    let output = match args[0].clone() {
        Hash(output) => Ok(output),
        _ => Err(CommandError::Argument),
    }?;

    let (tx_id, _fee, amount, tx) = output_service
        .create_htlc_refund_transaction(output, MicroTari(25))
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, "Claimed HTLC refund".into())
        .await?;
    Ok(tx_id)
}

/// Send a one-sided transaction to a recipient
pub async fn send_one_sided(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError> {
    let (amount, dest_pubkey, message) = get_transaction_parameters(args)?;
    wallet_transaction_service
        .send_one_sided_transaction(dest_pubkey, amount, fee_per_gram * uT, message)
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn coin_split(
    args: &[ParsedArgument],
    output_service: &mut OutputManagerHandle,
    transaction_service: &mut TransactionServiceHandle,
) -> Result<TxId, CommandError> {
    use ParsedArgument::*;
    let amount_per_split = match args[0] {
        Amount(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let num_splits = match args[1] {
        Int(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let fee_per_gram = match args[2] {
        Amount(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let (tx_id, tx, amount) = output_service
        .create_coin_split(amount_per_split, num_splits as usize, fee_per_gram, None)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, amount, "Coin split".into())
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
    args: &[ParsedArgument],
) -> Result<(CommsPublicKey, Multiaddr), CommandError> {
    let public_key = match args[0].clone() {
        ParsedArgument::PublicKey(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let net_address = match args[1].clone() {
        ParsedArgument::Address(a) => Ok(a),
        _ => Err(CommandError::Argument),
    }?;

    println!("Setting base node peer...");
    println!("{}::{}", public_key, net_address);
    wallet
        .set_base_node_peer(public_key.clone(), net_address.clone())
        .await?;
    Ok((public_key, net_address))
}

pub async fn discover_peer(
    mut dht_service: DhtDiscoveryRequester,
    args: Vec<ParsedArgument>,
) -> Result<(), CommandError> {
    use ParsedArgument::*;
    let dest_public_key = match args[0].clone() {
        PublicKey(key) => Ok(key),
        _ => Err(CommandError::Argument),
    }?;

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

pub async fn make_it_rain(
    wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    args: Vec<ParsedArgument>,
) -> Result<(), CommandError> {
    use ParsedArgument::*;

    let txps = match args[0].clone() {
        Float(r) => Ok(r),
        _ => Err(CommandError::Argument),
    }?;

    let duration = match args[1].clone() {
        Int(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let start_amount = match args[2].clone() {
        Amount(mtari) => Ok(mtari),
        _ => Err(CommandError::Argument),
    }?;

    let inc_amount = match args[3].clone() {
        Amount(mtari) => Ok(mtari),
        _ => Err(CommandError::Argument),
    }?;

    let start_time = match args[4].clone() {
        Date(dt) => Ok(dt),
        _ => Err(CommandError::Argument),
    }?;

    let public_key = match args[5].clone() {
        PublicKey(pk) => Ok(pk),
        _ => Err(CommandError::Argument),
    }?;

    let negotiated = match args[6].clone() {
        Negotiated(val) => Ok(val),
        _ => Err(CommandError::Argument),
    }?;

    let message = match args[7].clone() {
        Text(m) => Ok(m),
        _ => Err(CommandError::Argument),
    }?;

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

        let num_txs = (txps * duration as f64) as usize;
        let started_at = Utc::now();

        struct TransactionSendStats {
            i: usize,
            tx_id: Result<TxId, CommandError>,
            delayed_for: Duration,
            submit_time: Duration,
        }
        let transaction_type = if negotiated { "negotiated" } else { "one-sided" };
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
                let amount = start_amount + inc_amount * (i as u64);
                let send_args = vec![
                    ParsedArgument::Amount(amount),
                    ParsedArgument::PublicKey(public_key.clone()),
                    ParsedArgument::Text(message.clone()),
                ];
                // Manage transaction submission rate
                let actual_ms = (Utc::now() - started_at).num_milliseconds();
                let target_ms = (i as f64 / (txps / 1000.0)) as i64;
                if target_ms - actual_ms > 0 {
                    // Maximum delay between Txs set to 120 s
                    sleep(Duration::from_millis((target_ms - actual_ms).min(120_000i64) as u64)).await;
                }
                let delayed_for = Instant::now();
                let sender_clone = sender.clone();
                let fee = fee_per_gram;
                tokio::task::spawn(async move {
                    let spawn_start = Instant::now();
                    // Send transaction
                    let tx_id = if negotiated {
                        send_tari(tx_service, fee, send_args).await
                    } else {
                        send_one_sided(tx_service, fee, send_args).await
                    };
                    let submit_time = Instant::now();
                    tokio::task::spawn(async move {
                        print!("{} ", i + 1);
                    });
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
                TransactionEvent::TransactionDirectSendResult(id, success) if tx_ids.contains(id) => {
                    debug!(
                        target: LOG_TARGET,
                        "tx direct send event for tx_id: {}, success: {}", *id, success
                    );
                    if wait_stage == TransactionStage::DirectSendOrSaf {
                        results.push(SentTransaction {});
                        if results.len() == tx_ids.len() {
                            break;
                        }
                    }
                },
                TransactionEvent::TransactionStoreForwardSendResult(id, success) if tx_ids.contains(id) => {
                    debug!(
                        target: LOG_TARGET,
                        "tx store and forward event for tx_id: {}, success: {}", *id, success
                    );
                    if wait_stage == TransactionStage::DirectSendOrSaf {
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

pub async fn command_runner(
    commands: Vec<ParsedCommand>,
    wallet: WalletSqlite,
    config: GlobalConfig,
) -> Result<(), CommandError> {
    let wait_stage = TransactionStage::from_str(&config.wallet_command_send_wait_stage)
        .map_err(|e| CommandError::Config(e.to_string()))?;

    let mut transaction_service = wallet.transaction_service.clone();
    let mut output_service = wallet.output_manager_service.clone();
    let dht_service = wallet.dht_service.discovery_service_requester().clone();
    let connectivity_requester = wallet.comms.connectivity();
    let mut online = false;

    let mut tx_ids = Vec::new();

    println!("==============");
    println!("Command Runner");
    println!("==============");
    use WalletCommand::*;
    let wallet_config = config.wallet_config.clone().unwrap_or_default();
    for (idx, parsed) in commands.into_iter().enumerate() {
        println!("\n{}. {}\n", idx + 1, parsed);

        match parsed.command {
            GetBalance => match output_service.clone().get_balance().await {
                Ok(balance) => {
                    println!("{}", balance);
                },
                Err(e) => eprintln!("GetBalance error! {}", e),
            },
            DiscoverPeer => {
                if !online {
                    wait_for_comms(&connectivity_requester).await?;
                    online = true;
                }
                discover_peer(dht_service.clone(), parsed.args).await?
            },
            SendTari => {
                let tx_id = send_tari(transaction_service.clone(), wallet_config.fee_per_gram, parsed.args).await?;
                debug!(target: LOG_TARGET, "send-tari tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            SendOneSided => {
                let tx_id =
                    send_one_sided(transaction_service.clone(), wallet_config.fee_per_gram, parsed.args).await?;
                debug!(target: LOG_TARGET, "send-one-sided tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            MakeItRain => {
                make_it_rain(transaction_service.clone(), wallet_config.fee_per_gram, parsed.args).await?;
            },
            CoinSplit => {
                let tx_id = coin_split(&parsed.args, &mut output_service, &mut transaction_service.clone()).await?;
                tx_ids.push(tx_id);
                println!("Coin split succeeded");
            },
            Whois => {
                let public_key = match parsed.args[0].clone() {
                    ParsedArgument::PublicKey(key) => Ok(Box::new(key)),
                    _ => Err(CommandError::Argument),
                }?;
                let emoji_id = EmojiId::from_pubkey(&public_key);

                println!("Public Key: {}", public_key.to_hex());
                println!("Emoji ID  : {}", emoji_id);
            },
            ExportUtxos => {
                let utxos = output_service.get_unspent_outputs().await?;
                let count = utxos.len();
                let sum: MicroTari = utxos.iter().map(|utxo| utxo.value).sum();
                if parsed.args.is_empty() {
                    for (i, utxo) in utxos.iter().enumerate() {
                        println!("{}. Value: {} {}", i + 1, utxo.value, utxo.features);
                    }
                } else if let ParsedArgument::CSVFileName(file) = parsed.args[1].clone() {
                    write_utxos_to_csv_file(utxos, file)?;
                }
                println!("Total number of UTXOs: {}", count);
                println!("Total value of UTXOs: {}", sum);
            },
            ExportSpentUtxos => {
                let utxos = output_service.get_spent_outputs().await?;
                let count = utxos.len();
                let sum: MicroTari = utxos.iter().map(|utxo| utxo.value).sum();
                if parsed.args.is_empty() {
                    for (i, utxo) in utxos.iter().enumerate() {
                        println!("{}. Value: {} {}", i + 1, utxo.value, utxo.features);
                    }
                } else if let ParsedArgument::CSVFileName(file) = parsed.args[1].clone() {
                    write_utxos_to_csv_file(utxos, file)?;
                }
                println!("Total number of UTXOs: {}", count);
                println!("Total value of UTXOs: {}", sum);
            },
            CountUtxos => {
                let utxos = output_service.get_unspent_outputs().await?;
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
                    let average = Tari::try_from(average / 1_000_000f64)?;
                    println!("Average value UTXO   : {}", average);
                }
                if let Some(max) = values.iter().max() {
                    println!("Maximum value UTXO   : {}", max);
                }
            },
            SetBaseNode => {
                set_base_node_peer(wallet.clone(), &parsed.args).await?;
            },
            SetCustomBaseNode => {
                let (public_key, net_address) = set_base_node_peer(wallet.clone(), &parsed.args).await?;
                wallet
                    .db
                    .set_client_key_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string(), public_key.to_string())
                    .await?;
                wallet
                    .db
                    .set_client_key_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string(), net_address.to_string())
                    .await?;
                println!("Custom base node peer saved in wallet database.");
            },
            ClearCustomBaseNode => {
                wallet
                    .db
                    .clear_client_value(CUSTOM_BASE_NODE_PUBLIC_KEY_KEY.to_string())
                    .await?;
                wallet
                    .db
                    .clear_client_value(CUSTOM_BASE_NODE_ADDRESS_KEY.to_string())
                    .await?;
                println!("Custom base node peer cleared from wallet database.");
            },
            InitShaAtomicSwap => {
                let (tx_id, pre_image, output) = init_sha_atomic_swap(
                    transaction_service.clone(),
                    wallet_config.fee_per_gram,
                    parsed.clone().args,
                )
                .await?;
                debug!(target: LOG_TARGET, "tari HTLC tx_id {}", tx_id);
                let hash: [u8; 32] = Sha256::digest(pre_image.as_bytes()).into();
                println!("pre_image hex: {}", pre_image.to_hex());
                println!("pre_image hash: {}", hash.to_hex());
                println!("Output hash: {}", output.hash().to_hex());
                tx_ids.push(tx_id);
            },
            FinaliseShaAtomicSwap => {
                let tx_id =
                    finalise_sha_atomic_swap(output_service.clone(), transaction_service.clone(), parsed.args).await?;
                debug!(target: LOG_TARGET, "claiming tari HTLC tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            ClaimShaAtomicSwapRefund => {
                let tx_id = claim_htlc_refund(output_service.clone(), transaction_service.clone(), parsed.args).await?;
                debug!(target: LOG_TARGET, "claiming tari HTLC tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            RegisterAsset => {
                let name = parsed.args[0].to_string();
                let message = format!("Register asset: {}", name);
                let mut manager = wallet.asset_manager.clone();
                // todo: key manager #LOGGED
                let mut rng = rand::thread_rng();
                let (_, public_key) = PublicKey::random_keypair(&mut rng);
                let public_key_hex = public_key.to_hex();
                println!("Registering asset named: {name}");
                println!("with public key: {public_key_hex}");
                let (tx_id, transaction) = manager
                    .create_registration_transaction(name, public_key, vec![], None, None, vec![])
                    .await?;
                let _result = transaction_service
                    .submit_transaction(tx_id, transaction, 0.into(), message)
                    .await?;
                println!("Done!");
            },
            MintTokens => {
                println!("Minting tokens for asset");
                let public_key = match parsed.args[0] {
                    ParsedArgument::PublicKey(ref key) => Ok(key.clone()),
                    _ => Err(CommandError::Argument),
                }?;

                let unique_ids = parsed.args[1..]
                    .iter()
                    .map(|arg| {
                        let s = arg.to_string();
                        if let Some(s) = s.strip_prefix("0x") {
                            Hex::from_hex(s).map_err(|_| CommandError::Argument)
                        } else {
                            Ok(s.into_bytes())
                        }
                    })
                    .collect::<Result<Vec<Vec<u8>>, _>>()?;

                let mut asset_manager = wallet.asset_manager.clone();
                let asset = asset_manager.get_owned_asset_by_pub_key(&public_key).await?;
                println!("Asset name: {}", asset.name());

                let message = format!("Minting {} tokens for asset {}", unique_ids.len(), asset.name());
                let (tx_id, transaction) = asset_manager
                    .create_minting_transaction(
                        &public_key,
                        asset.owner_commitment(),
                        unique_ids.into_iter().map(|id| (id, None)).collect(),
                    )
                    .await?;
                let _result = transaction_service
                    .submit_transaction(tx_id, transaction, 0.into(), message)
                    .await?;
            },
            CreateInitialCheckpoint => {
                println!("Creating Initial Checkpoint for Asset");
                let asset_public_key = match parsed.args[0] {
                    ParsedArgument::PublicKey(ref key) => Ok(key.clone()),
                    _ => Err(CommandError::Argument),
                }?;

                let merkle_root = match parsed.args[1] {
                    ParsedArgument::Text(ref root) => {
                        let bytes = match &root[0..2] {
                            "0x" => Vec::<u8>::from_hex(&root[2..]).map_err(|_| CommandError::Argument)?,
                            _ => Vec::<u8>::from_hex(root).map_err(|_| CommandError::Argument)?,
                        };
                        copy_into_fixed_array(&bytes).map_err(|_| CommandError::Argument)?
                    },
                    _ => return Err(CommandError::Argument),
                };

                let message = format!("Initial asset checkpoint for {}", asset_public_key);

                let mut asset_manager = wallet.asset_manager.clone();
                let (tx_id, transaction) = asset_manager
                    .create_initial_asset_checkpoint(&asset_public_key, merkle_root)
                    .await?;
                let _result = transaction_service
                    .submit_transaction(tx_id, transaction, 0.into(), message)
                    .await?;
            },
            CreateCommitteeDefinition => {
                let asset_public_key = match parsed.args[0] {
                    ParsedArgument::PublicKey(ref key) => Ok(key.clone()),
                    _ => Err(CommandError::Argument),
                }?;
                let public_key_hex = asset_public_key.to_hex();
                println!("Creating Committee Checkpoint for Asset");
                println!("with public key {public_key_hex}");

                let committee_public_keys: Vec<PublicKey> = parsed.args[1..]
                    .iter()
                    .map(|pk| match pk {
                        ParsedArgument::PublicKey(ref key) => Ok(key.clone()),
                        _ => Err(CommandError::Argument),
                    })
                    .collect::<Result<_, _>>()?;

                let num_members = committee_public_keys.len();
                if num_members < 1 {
                    return Err(CommandError::Config("Committee has no members!".into()));
                }
                let message = format!(
                    "Committee checkpoint with {} members for {}",
                    num_members, asset_public_key
                );
                println!("with {num_members} committee members");

                let mut asset_manager = wallet.asset_manager.clone();
                // todo: effective sidechain height...
                let (tx_id, transaction) = asset_manager
                    .create_committee_definition(&asset_public_key, &committee_public_keys, 0)
                    .await?;

                let _result = transaction_service
                    .submit_transaction(tx_id, transaction, 0.into(), message)
                    .await?;
                println!("Done!");
            },
        }
    }

    // listen to event stream
    if !tx_ids.is_empty() {
        let duration = Duration::from_secs(config.wallet_command_send_wait_timeout);
        debug!(
            target: LOG_TARGET,
            "wallet monitor_transactions timeout duration {:?}", duration
        );
        match timeout(
            duration,
            monitor_transactions(transaction_service.clone(), tx_ids, wait_stage.clone()),
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
    } else {
        trace!(
            target: LOG_TARGET,
            "Wallet command runner - no transactions to monitor."
        );
    }

    Ok(())
}

fn write_utxos_to_csv_file(utxos: Vec<UnblindedOutput>, file_path: String) -> Result<(), CommandError> {
    let factory = PedersenCommitmentFactory::default();
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
            utxo.features.flags,
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
