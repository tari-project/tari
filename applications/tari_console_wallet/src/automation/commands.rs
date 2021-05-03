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

use super::error::CommandError;
use crate::automation::command_parser::{ParsedArgument, ParsedCommand};
use chrono::{DateTime, Utc};
use futures::{FutureExt, StreamExt};
use log::*;
use std::{
    fs::File,
    io::{LineWriter, Write},
    str::FromStr,
    time::{Duration, Instant},
};
use strum_macros::{Display, EnumIter, EnumString};
use tari_common::GlobalConfig;
use tari_comms::connectivity::{ConnectivityEvent, ConnectivityRequester};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::{
    tari_utilities::hex::Hex,
    transactions::{
        tari_amount::{uT, MicroTari, Tari},
        types::PublicKey,
    },
};
use tari_crypto::ristretto::pedersen::PedersenCommitmentFactory;
use tari_wallet::{
    output_manager_service::{handle::OutputManagerHandle, TxId},
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    util::emoji::EmojiId,
    WalletSqlite,
};
use tokio::{
    runtime::Handle,
    time::{delay_for, timeout},
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
    CountUtxos,
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
pub struct SentTransaction {
    id: TxId,
    stage: TransactionStage,
}

fn get_transaction_parameters(
    args: Vec<ParsedArgument>,
) -> Result<(MicroTari, MicroTari, PublicKey, String), CommandError> {
    // TODO: Consolidate "fee per gram" in codebase
    let fee_per_gram = 25 * uT;

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

    Ok((fee_per_gram, amount, dest_pubkey, message))
}

/// Send a normal negotiated transaction to a recipient
pub async fn send_tari(
    mut wallet_transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError>
{
    let (fee_per_gram, amount, dest_pubkey, message) = get_transaction_parameters(args)?;
    wallet_transaction_service
        .send_transaction(dest_pubkey, amount, fee_per_gram, message)
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// Send a one-sided transaction to a recipient
pub async fn send_one_sided(
    mut wallet_transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError>
{
    let (fee_per_gram, amount, dest_pubkey, message) = get_transaction_parameters(args)?;
    wallet_transaction_service
        .send_one_sided_transaction(dest_pubkey, amount, fee_per_gram, message)
        .await
        .map_err(CommandError::TransactionServiceError)
}

pub async fn coin_split(
    args: &[ParsedArgument],
    output_service: &mut OutputManagerHandle,
    transaction_service: &mut TransactionServiceHandle,
) -> Result<TxId, CommandError>
{
    use ParsedArgument::*;
    let amount_per_split = match args[0] {
        Amount(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let num_splits = match args[1] {
        Int(s) => Ok(s),
        _ => Err(CommandError::Argument),
    }?;

    let (tx_id, tx, fee, amount) = output_service
        .create_coin_split(amount_per_split, num_splits as usize, MicroTari(100), None)
        .await?;
    transaction_service
        .submit_transaction(tx_id, tx, fee, amount, "Coin split".into())
        .await?;

    Ok(tx_id)
}

async fn wait_for_comms(connectivity_requester: &ConnectivityRequester) -> Result<bool, CommandError> {
    let mut connectivity = connectivity_requester.get_event_subscription().fuse();
    print!("Waiting for connectivity... ");
    let mut timeout = delay_for(Duration::from_secs(30)).fuse();
    loop {
        futures::select! {
            result = connectivity.select_next_some() => {
                if let Ok(msg) = result {
                    if let ConnectivityEvent::PeerConnected(_) = (*msg).clone() {
                        println!("✅");
                        return Ok(true);
                    }
                }
            },
            () = timeout => {
                println!("❌");
                return Err(CommandError::Comms("Timed out".to_string()));
            }
        }
    }
}

pub async fn discover_peer(
    mut dht_service: DhtDiscoveryRequester,
    args: Vec<ParsedArgument>,
) -> Result<(), CommandError>
{
    use ParsedArgument::*;
    let dest_public_key = match args[0].clone() {
        PublicKey(key) => Ok(Box::new(key)),
        _ => Err(CommandError::Argument),
    }?;

    let start = Instant::now();
    println!("🌎 Peer discovery started.");
    match dht_service
        .discover_peer(dest_public_key.clone(), NodeDestination::PublicKey(dest_public_key))
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
    handle: Handle,
    wallet_transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<Vec<TxId>, CommandError>
{
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
        Date(dt) => Ok(dt as DateTime<Utc>),
        _ => Err(CommandError::Argument),
    }?;

    let public_key = match args[5].clone() {
        PublicKey(pk) => Ok(pk),
        _ => Err(CommandError::Argument),
    }?;

    let message = match args[6].clone() {
        Text(m) => Ok(m),
        _ => Err(CommandError::Argument),
    }?;

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
    delay_for(Duration::from_millis(delay_ms)).await;

    let num_txs = (txps * duration as f64) as usize;

    let mut tx_ids = Vec::new();
    let started_at = Utc::now();

    for i in 0..num_txs {
        // Manage Tx rate
        let actual_ms = (Utc::now() - started_at).num_milliseconds();
        let target_ms = (i as f64 / (txps / 1000.0)) as i64;
        if target_ms - actual_ms > 0 {
            // Maximum delay between Txs set to 120 s
            delay_for(Duration::from_millis((target_ms - actual_ms).min(120_000i64) as u64)).await;
        }
        // Send Tx
        let amount = start_amount + inc_amount * (i as u64);
        let send_args = vec![
            ParsedArgument::Amount(amount),
            ParsedArgument::PublicKey(public_key.clone()),
            ParsedArgument::Text(message.clone()),
        ];
        let tx_service = wallet_transaction_service.clone();
        let tx_id = handle
            .spawn(send_tari(tx_service, send_args))
            .await
            .map_err(CommandError::Join)??;

        debug!(target: LOG_TARGET, "make-it-rain tx_id: {}", tx_id);
        tx_ids.push(tx_id);
    }

    Ok(tx_ids)
}

pub async fn monitor_transactions(
    transaction_service: TransactionServiceHandle,
    tx_ids: Vec<TxId>,
    wait_stage: TransactionStage,
) -> Vec<SentTransaction>
{
    let mut event_stream = transaction_service.get_event_stream_fused();
    let mut results = Vec::new();
    debug!(target: LOG_TARGET, "monitor transactions wait_stage: {:?}", wait_stage);
    println!(
        "Monitoring {} sent transactions to {:?} stage...",
        tx_ids.len(),
        wait_stage
    );

    loop {
        match event_stream.next().await {
            Some(event_result) => match event_result {
                Ok(event) => match &*event {
                    TransactionEvent::TransactionDirectSendResult(id, success) if tx_ids.contains(id) => {
                        debug!(
                            target: LOG_TARGET,
                            "tx direct send event for tx_id: {}, success: {}", *id, success
                        );
                        if wait_stage == TransactionStage::DirectSendOrSaf {
                            results.push(SentTransaction {
                                id: *id,
                                stage: TransactionStage::DirectSendOrSaf,
                            });
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
                            results.push(SentTransaction {
                                id: *id,
                                stage: TransactionStage::DirectSendOrSaf,
                            });
                            if results.len() == tx_ids.len() {
                                break;
                            }
                        }
                    },
                    TransactionEvent::ReceivedTransactionReply(id) if tx_ids.contains(id) => {
                        debug!(target: LOG_TARGET, "tx reply event for tx_id: {}", *id);
                        if wait_stage == TransactionStage::Negotiated {
                            results.push(SentTransaction {
                                id: *id,
                                stage: TransactionStage::Negotiated,
                            });
                            if results.len() == tx_ids.len() {
                                break;
                            }
                        }
                    },
                    TransactionEvent::TransactionBroadcast(id) if tx_ids.contains(id) => {
                        debug!(target: LOG_TARGET, "tx mempool broadcast event for tx_id: {}", *id);
                        if wait_stage == TransactionStage::Broadcast {
                            results.push(SentTransaction {
                                id: *id,
                                stage: TransactionStage::Broadcast,
                            });
                            if results.len() == tx_ids.len() {
                                break;
                            }
                        }
                    },
                    TransactionEvent::TransactionMinedUnconfirmed(id, confirmations) if tx_ids.contains(id) => {
                        debug!(
                            target: LOG_TARGET,
                            "tx mined unconfirmed event for tx_id: {}, confirmations: {}", *id, confirmations
                        );
                        if wait_stage == TransactionStage::MinedUnconfirmed {
                            results.push(SentTransaction {
                                id: *id,
                                stage: TransactionStage::MinedUnconfirmed,
                            });
                            if results.len() == tx_ids.len() {
                                break;
                            }
                        }
                    },
                    TransactionEvent::TransactionMined(id) if tx_ids.contains(id) => {
                        debug!(target: LOG_TARGET, "tx mined confirmed event for tx_id: {}", *id);
                        if wait_stage == TransactionStage::Mined {
                            results.push(SentTransaction {
                                id: *id,
                                stage: TransactionStage::Mined,
                            });
                            if results.len() == tx_ids.len() {
                                break;
                            }
                        }
                    },
                    _ => {},
                },
                Err(e) => {
                    eprintln!("RecvError in monitor_transactions: {:?}", e);
                    break;
                },
            },
            None => {
                warn!(
                    target: LOG_TARGET,
                    "`None` result in event in monitor_transactions loop"
                );
                break;
            },
        }
    }

    results
}

pub async fn command_runner(
    handle: Handle,
    commands: Vec<ParsedCommand>,
    wallet: WalletSqlite,
    config: GlobalConfig,
) -> Result<(), CommandError>
{
    let wait_stage = TransactionStage::from_str(&config.wallet_command_send_wait_stage)
        .map_err(|e| CommandError::Config(e.to_string()))?;

    let transaction_service = wallet.transaction_service.clone();
    let mut output_service = wallet.output_manager_service.clone();
    let dht_service = wallet.dht_service.discovery_service_requester().clone();
    let connectivity_requester = wallet.comms.connectivity();
    let mut online = false;

    let mut tx_ids = Vec::new();

    println!("==============");
    println!("Command Runner");
    println!("==============");
    use WalletCommand::*;
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
                    online = wait_for_comms(&connectivity_requester).await?;
                }
                discover_peer(dht_service.clone(), parsed.args).await?
            },
            SendTari => {
                let tx_id = send_tari(transaction_service.clone(), parsed.args).await?;
                debug!(target: LOG_TARGET, "send-tari tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            SendOneSided => {
                let tx_id = send_one_sided(transaction_service.clone(), parsed.args).await?;
                debug!(target: LOG_TARGET, "send-one-sided tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            MakeItRain => {
                let rain_ids = make_it_rain(handle.clone(), transaction_service.clone(), parsed.args).await?;
                tx_ids.extend(rain_ids);
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
                    let factory = PedersenCommitmentFactory::default();
                    let file = File::create(file).map_err(|e| CommandError::CSVFile(e.to_string()))?;
                    let mut csv_file = LineWriter::new(file);
                    writeln!(
                        csv_file,
                        r##""#","Value (uT)","Spending Key","Commitment","Flags","Maturity""##
                    )
                    .map_err(|e| CommandError::CSVFile(e.to_string()))?;
                    for (i, utxo) in utxos.iter().enumerate() {
                        writeln!(
                            csv_file,
                            r##""{}","{}","{}","{}","{:?}","{}""##,
                            i + 1,
                            utxo.value.0,
                            utxo.spending_key.to_hex(),
                            utxo.as_transaction_input(&factory)?.commitment.to_hex(),
                            utxo.features.flags,
                            utxo.features.maturity,
                        )
                        .map_err(|e| CommandError::CSVFile(e.to_string()))?;
                    }
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
                    let average = Tari::from(average / 1_000_000f64);
                    println!("Average value UTXO   : {}", average);
                }
                if let Some(max) = values.iter().max() {
                    println!("Maximum value UTXO   : {}", max);
                }
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
                    "The configured timeout ({:#?}s) was reached before all transactions reached the {:?} stage. See \
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
