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
    fs,
    fs::File,
    io::{LineWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use digest::Digest;
use futures::FutureExt;
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;
use strum_macros::{Display, EnumIter, EnumString};
use tari_common_types::{
    emoji::EmojiId,
    transaction::TxId,
    types::{CommitmentFactory, FixedHash, PublicKey},
};
use tari_comms::{
    connectivity::{ConnectivityEvent, ConnectivityRequester},
    multiaddr::Multiaddr,
    types::CommsPublicKey,
};
use tari_comms_dht::{envelope::NodeDestination, DhtDiscoveryRequester};
use tari_core::transactions::{
    tari_amount::{uT, MicroTari, Tari},
    transaction_components::{
        CheckpointParameters,
        ContractAcceptanceRequirements,
        ContractAmendment,
        ContractDefinition,
        ContractUpdateProposal,
        OutputFeatures,
        SideChainConsensus,
        SideChainFeatures,
        TransactionOutput,
        UnblindedOutput,
    },
};
use tari_utilities::{hex::Hex, ByteArray, Hashable};
use tari_wallet::{
    assets::{
        ConstitutionChangeRulesFileFormat,
        ConstitutionDefinitionFileFormat,
        ContractAmendmentFileFormat,
        ContractDefinitionFileFormat,
        ContractSpecificationFileFormat,
        ContractUpdateProposalFileFormat,
        SignatureFileFormat,
    },
    error::WalletError,
    key_manager_service::{KeyManagerInterface, NextKeyResult},
    output_manager_service::{handle::OutputManagerHandle, resources::OutputManagerKeyManagerBranch},
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    TransactionStage,
    WalletConfig,
    WalletSqlite,
};
use tokio::{
    sync::{broadcast, mpsc},
    time::{sleep, timeout},
};

use super::error::CommandError;
use crate::{
    automation::prompt::{HexArg, Optional, Prompt, YesNo},
    cli::{
        CliCommands,
        ContractCommand,
        ContractSubcommand,
        InitAmendmentArgs,
        InitConstitutionArgs,
        InitDefinitionArgs,
        InitUpdateProposalArgs,
        PublishFileArgs,
    },
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
            OutputFeatures::default(),
            fee_per_gram * uT,
            message,
        )
        .await
        .map_err(CommandError::TransactionServiceError)
}

/// publishes a tari-SHA atomic swap HTLC transaction
pub async fn init_sha_atomic_swap(
    mut wallet_transaction_service: TransactionServiceHandle,
    fee_per_gram: u64,
    amount: MicroTari,
    dest_pubkey: PublicKey,
    message: String,
) -> Result<(TxId, PublicKey, TransactionOutput), CommandError> {
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
    output_hash: Vec<u8>,
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
    output_hash: Vec<u8>,
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
    dest_pubkey: PublicKey,
    message: String,
) -> Result<TxId, CommandError> {
    wallet_transaction_service
        .send_one_sided_transaction(
            dest_pubkey,
            amount,
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
        .create_coin_split(amount_per_split, num_splits as usize, fee_per_gram, None)
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
    negotiated: bool,
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
                    let tx_id = if negotiated {
                        send_tari(tx_service, fee, amount, pk.clone(), msg.clone()).await
                    } else {
                        send_one_sided(tx_service, fee, amount, pk.clone(), msg.clone()).await
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
                    println!("{}", balance);
                },
                Err(e) => eprintln!("GetBalance error! {}", e),
            },
            DiscoverPeer(args) => {
                if !online {
                    wait_for_comms(&connectivity_requester).await?;
                    online = true;
                }
                discover_peer(dht_service.clone(), args.dest_public_key.into()).await?
            },
            SendTari(args) => {
                let tx_id = send_tari(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.destination.into(),
                    args.message,
                )
                .await?;
                debug!(target: LOG_TARGET, "send-tari tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            SendOneSided(args) => {
                let tx_id = send_one_sided(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.destination.into(),
                    args.message,
                )
                .await?;
                debug!(target: LOG_TARGET, "send-one-sided tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            MakeItRain(args) => {
                make_it_rain(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.transactions_per_second,
                    args.duration,
                    args.start_amount,
                    args.increase_amount,
                    args.start_time.unwrap_or_else(Utc::now),
                    args.destination.into(),
                    !args.one_sided,
                    args.message,
                )
                .await?;
            },
            CoinSplit(args) => {
                let tx_id = coin_split(
                    args.amount_per_split,
                    args.num_splits,
                    args.fee_per_gram,
                    args.message,
                    &mut output_service,
                    &mut transaction_service.clone(),
                )
                .await?;
                tx_ids.push(tx_id);
                println!("Coin split succeeded");
            },
            Whois(args) => {
                let public_key = args.public_key.into();
                let emoji_id = EmojiId::from_pubkey(&public_key);

                println!("Public Key: {}", public_key.to_hex());
                println!("Emoji ID  : {}", emoji_id);
            },
            ExportUtxos(args) => {
                let utxos = output_service.get_unspent_outputs().await?;
                let count = utxos.len();
                let sum: MicroTari = utxos.iter().map(|utxo| utxo.value).sum();
                if let Some(file) = args.output_file {
                    write_utxos_to_csv_file(utxos, file)?;
                } else {
                    for (i, utxo) in utxos.iter().enumerate() {
                        println!("{}. Value: {} {}", i + 1, utxo.value, utxo.features);
                    }
                }
                println!("Total number of UTXOs: {}", count);
                println!("Total value of UTXOs: {}", sum);
            },
            ExportSpentUtxos(args) => {
                let utxos = output_service.get_spent_outputs().await?;
                let count = utxos.len();
                let sum: MicroTari = utxos.iter().map(|utxo| utxo.value).sum();
                if let Some(file) = args.output_file {
                    write_utxos_to_csv_file(utxos, file)?;
                } else {
                    for (i, utxo) in utxos.iter().enumerate() {
                        println!("{}. Value: {} {}", i + 1, utxo.value, utxo.features);
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
                    let average = Tari::from(MicroTari(average.round() as u64));
                    println!("Average value UTXO   : {}", average);
                }
                if let Some(max) = values.iter().max() {
                    println!("Maximum value UTXO   : {}", max);
                }
            },
            SetBaseNode(args) => {
                set_base_node_peer(wallet.clone(), args.public_key.into(), args.address).await?;
            },
            SetCustomBaseNode(args) => {
                let (public_key, net_address) =
                    set_base_node_peer(wallet.clone(), args.public_key.into(), args.address).await?;
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
            InitShaAtomicSwap(args) => {
                let (tx_id, pre_image, output) = init_sha_atomic_swap(
                    transaction_service.clone(),
                    config.fee_per_gram,
                    args.amount,
                    args.destination.into(),
                    args.message,
                )
                .await?;
                debug!(target: LOG_TARGET, "tari HTLC tx_id {}", tx_id);
                let hash: [u8; 32] = Sha256::digest(pre_image.as_bytes()).into();
                println!("pre_image hex: {}", pre_image.to_hex());
                println!("pre_image hash: {}", hash.to_hex());
                println!("Output hash: {}", output.hash().to_hex());
                tx_ids.push(tx_id);
            },
            FinaliseShaAtomicSwap(args) => {
                let tx_id = finalise_sha_atomic_swap(
                    output_service.clone(),
                    transaction_service.clone(),
                    args.output_hash[0].clone(),
                    args.pre_image.into(),
                    config.fee_per_gram.into(),
                    args.message,
                )
                .await?;
                debug!(target: LOG_TARGET, "claiming tari HTLC tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            ClaimShaAtomicSwapRefund(args) => {
                let tx_id = claim_htlc_refund(
                    output_service.clone(),
                    transaction_service.clone(),
                    args.output_hash[0].clone(),
                    config.fee_per_gram.into(),
                    args.message,
                )
                .await?;
                debug!(target: LOG_TARGET, "claiming tari HTLC tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            RevalidateWalletDb => {
                output_service
                    .revalidate_all_outputs()
                    .await
                    .map_err(CommandError::OutputManagerError)?;
                transaction_service
                    .revalidate_all_transactions()
                    .await
                    .map_err(CommandError::TransactionServiceError)?;
            },
            Contract(command) => {
                handle_contract_command(&wallet, command).await?;
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

async fn handle_contract_command(wallet: &WalletSqlite, command: ContractCommand) -> Result<(), CommandError> {
    match command.subcommand {
        ContractSubcommand::InitDefinition(args) => init_contract_definition_spec(wallet, args).await,
        ContractSubcommand::InitConstitution(args) => init_contract_constitution_spec(args),
        ContractSubcommand::InitUpdateProposal(args) => init_contract_update_proposal_spec(args),
        ContractSubcommand::InitAmendment(args) => init_contract_amendment_spec(args),
        ContractSubcommand::PublishDefinition(args) => publish_contract_definition(wallet, args).await,
        ContractSubcommand::PublishConstitution(args) => publish_contract_constitution(wallet, args).await,
        ContractSubcommand::PublishUpdateProposal(args) => publish_contract_update_proposal(wallet, args).await,
        ContractSubcommand::PublishAmendment(args) => publish_contract_amendment(wallet, args).await,
    }
}

async fn init_contract_definition_spec(wallet: &WalletSqlite, args: InitDefinitionArgs) -> Result<(), CommandError> {
    if args.dest_path.exists() {
        if args.force {
            println!("{} exists and will be overwritten.", args.dest_path.to_string_lossy());
        } else {
            println!(
                "{} exists. Use `--force` to overwrite.",
                args.dest_path.to_string_lossy()
            );
            return Ok(());
        }
    }
    let mut dest = args.dest_path;
    if dest.extension().is_none() {
        dest = dest.join("contract.json");
    }

    let contract_name = Prompt::new("Contract name (max 32 characters):")
        .skip_if_some(args.contract_name)
        .ask()?;
    if contract_name.as_bytes().len() > 32 {
        return Err(CommandError::InvalidArgument(
            "Contract name must be at most 32 bytes.".to_string(),
        ));
    }
    println!(
        "Wallet public key: {}",
        wallet.comms.node_identity().public_key().to_hex()
    );
    let use_wallet_pk = Prompt::new("Use wallet public key as issuer public key? (Y/N):")
        .skip_if_some(args.contract_issuer.as_ref().map(|_| "y".to_string()))
        .ask_parsed::<YesNo>()
        .map(|yn| yn.as_bool())?;

    let contract_issuer = if use_wallet_pk {
        args.contract_issuer
            .map(|s| PublicKey::from_hex(&s))
            .transpose()
            .map_err(|_| CommandError::InvalidArgument("Issuer public key hex is invalid.".to_string()))?
            .unwrap_or_else(|| wallet.comms.node_identity().public_key().clone())
    } else {
        let contract_issuer = Prompt::new("Issuer public Key (hex): (Press enter to generate a new one)")
            .with_default("")
            .skip_if_some(args.contract_issuer)
            .ask_parsed::<Optional<HexArg<PublicKey>>>()?
            .into_inner()
            .map(|v| v.into_inner());
        match contract_issuer {
            Some(pk) => pk,
            None => {
                let issuer_key_path = dest.parent().unwrap_or(dest.as_path()).join("issuer_keys.json");
                let issuer_public_key_path = Prompt::new("Enter path to generate new issuer public key:")
                    .with_default(issuer_key_path.to_string_lossy())
                    .ask_parsed::<PathBuf>()?;
                let key_result = wallet
                    .key_manager_service
                    .get_next_key(OutputManagerKeyManagerBranch::ContractIssuer.get_branch_key())
                    .await?;

                let public_key = key_result.to_public_key();
                write_to_issuer_key_file(&issuer_public_key_path, key_result)?;
                println!("Wrote to key file {}", issuer_public_key_path.to_string_lossy());
                public_key
            },
        }
    };

    let runtime = Prompt::new("Contract runtime:")
        .skip_if_some(args.runtime)
        .with_default("/tari/wasm/v0.1")
        .ask_parsed()?;

    let contract_definition = ContractDefinitionFileFormat {
        contract_name,
        contract_issuer,
        contract_spec: ContractSpecificationFileFormat {
            runtime,
            public_functions: vec![],
        },
    };
    write_json_file(&dest, &contract_definition).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    println!("Wrote {}", dest.to_string_lossy());
    Ok(())
}

fn init_contract_constitution_spec(args: InitConstitutionArgs) -> Result<(), CommandError> {
    if args.dest_path.exists() {
        if args.force {
            println!("{} exists and will be overwritten.", args.dest_path.to_string_lossy());
        } else {
            println!(
                "{} exists. Use `--force` to overwrite.",
                args.dest_path.to_string_lossy()
            );
            return Ok(());
        }
    }
    let dest = args.dest_path;

    let contract_id = Prompt::new("Contract id (hex):")
        .skip_if_some(args.contract_id)
        .ask_parsed()?;
    let committee: Vec<String> = Prompt::new("Validator committee ids (hex):").ask_repeatedly()?;
    let acceptance_period_expiry = Prompt::new("Acceptance period expiry (in blocks, integer):")
        .skip_if_some(args.acceptance_period_expiry)
        .with_default("50")
        .ask_parsed()?;
    let minimum_quorum_required = Prompt::new("Minimum quorum:")
        .skip_if_some(args.minimum_quorum_required)
        .with_default(committee.len().to_string())
        .ask_parsed()?;

    let constitution = ConstitutionDefinitionFileFormat {
        contract_id,
        validator_committee: committee.iter().map(|c| PublicKey::from_hex(c).unwrap()).collect(),
        consensus: SideChainConsensus::MerkleRoot,
        initial_reward: 0,
        acceptance_parameters: ContractAcceptanceRequirements {
            acceptance_period_expiry,
            minimum_quorum_required,
        },
        checkpoint_parameters: CheckpointParameters {
            minimum_quorum_required: 0,
            abandoned_interval: 0,
        },
        constitution_change_rules: ConstitutionChangeRulesFileFormat {
            change_flags: 0,
            requirements_for_constitution_change: None,
        },
    };

    write_json_file(&dest, &constitution).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    println!("Wrote {}", dest.to_string_lossy());
    Ok(())
}

fn init_contract_update_proposal_spec(args: InitUpdateProposalArgs) -> Result<(), CommandError> {
    if args.dest_path.exists() {
        if args.force {
            println!("{} exists and will be overwritten.", args.dest_path.to_string_lossy());
        } else {
            println!(
                "{} exists. Use `--force` to overwrite.",
                args.dest_path.to_string_lossy()
            );
            return Ok(());
        }
    }
    let dest = args.dest_path;

    let contract_id = Prompt::new("Contract id (hex):").skip_if_some(args.contract_id).ask()?;
    let proposal_id = Prompt::new("Proposal id (integer, unique inside the contract scope):")
        .skip_if_some(args.proposal_id)
        .with_default("0".to_string())
        .ask_parsed()?;
    let committee: Vec<String> = Prompt::new("Validator committee ids (hex):").ask_repeatedly()?;
    let acceptance_period_expiry = Prompt::new("Acceptance period expiry (in blocks, integer):")
        .skip_if_some(args.acceptance_period_expiry)
        .with_default("50".to_string())
        .ask_parsed()?;
    let minimum_quorum_required = Prompt::new("Minimum quorum:")
        .skip_if_some(args.minimum_quorum_required)
        .with_default(committee.len().to_string())
        .ask_parsed()?;

    let updated_constitution = ConstitutionDefinitionFileFormat {
        contract_id,
        validator_committee: committee.iter().map(|c| PublicKey::from_hex(c).unwrap()).collect(),
        consensus: SideChainConsensus::MerkleRoot,
        initial_reward: 0,
        acceptance_parameters: ContractAcceptanceRequirements {
            acceptance_period_expiry,
            minimum_quorum_required,
        },
        checkpoint_parameters: CheckpointParameters {
            minimum_quorum_required: 0,
            abandoned_interval: 0,
        },
        constitution_change_rules: ConstitutionChangeRulesFileFormat {
            change_flags: 0,
            requirements_for_constitution_change: None,
        },
    };

    let update_proposal = ContractUpdateProposalFileFormat {
        proposal_id,
        // TODO: use a private key to sign the proposal
        signature: SignatureFileFormat::default(),
        updated_constitution,
    };

    write_json_file(&dest, &update_proposal)?;
    println!("Wrote {}", dest.to_string_lossy());
    Ok(())
}

fn init_contract_amendment_spec(args: InitAmendmentArgs) -> Result<(), CommandError> {
    if args.dest_path.exists() {
        if args.force {
            println!("{} exists and will be overwritten.", args.dest_path.to_string_lossy());
        } else {
            println!(
                "{} exists. Use `--force` to overwrite.",
                args.dest_path.to_string_lossy()
            );
            return Ok(());
        }
    }
    let dest = args.dest_path;

    // check that the proposal file exists
    if !args.proposal_file_path.exists() {
        println!(
            "Proposal file path {} not found",
            args.proposal_file_path.to_string_lossy()
        );
        return Ok(());
    }
    // parse the JSON file with the proposal
    let update_proposal: ContractUpdateProposalFileFormat = read_json_file(&args.proposal_file_path)?;

    // read the activation_window value from the user
    let activation_window = Prompt::new("Activation window (in blocks, integer):")
        .skip_if_some(args.activation_window)
        .with_default("50".to_string())
        .ask_parsed()?;

    // create the amendment from the proposal
    let amendment = ContractAmendmentFileFormat {
        proposal_id: update_proposal.proposal_id,
        validator_committee: update_proposal.updated_constitution.validator_committee.clone(),
        // TODO: import the real signatures for all the proposal acceptances
        validator_signatures: Vec::new(),
        updated_constitution: update_proposal.updated_constitution,
        activation_window,
    };

    // write the amendment to the destination file
    write_json_file(&dest, &amendment)?;
    println!("Wrote {}", dest.to_string_lossy());

    Ok(())
}

async fn publish_contract_definition(wallet: &WalletSqlite, args: PublishFileArgs) -> Result<(), CommandError> {
    // open and parse the JSON file with the contract definition values
    let contract_definition: ContractDefinitionFileFormat =
        read_json_file(&args.file_path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let contract_definition_features = ContractDefinition::from(contract_definition);
    let contract_id_hex = contract_definition_features.calculate_contract_id().to_vec().to_hex();

    // create the contract definition transaction
    let mut asset_manager = wallet.asset_manager.clone();
    let (tx_id, transaction) = asset_manager
        .create_contract_definition(&contract_definition_features)
        .await?;

    // publish the contract definition transaction
    let message = format!("Contract definition for contract {}", contract_id_hex);
    let mut transaction_service = wallet.transaction_service.clone();
    transaction_service
        .submit_transaction(tx_id, transaction, 0.into(), message)
        .await?;

    println!(
        "Contract definition submitted: contract_id is {} (TxID: {})",
        contract_id_hex, tx_id,
    );
    println!("Done!");
    Ok(())
}

async fn publish_contract_constitution(wallet: &WalletSqlite, args: PublishFileArgs) -> Result<(), CommandError> {
    // parse the JSON file
    let constitution_definition: ConstitutionDefinitionFileFormat =
        read_json_file(&args.file_path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let side_chain_features = SideChainFeatures::try_from(constitution_definition.clone()).unwrap();

    let mut asset_manager = wallet.asset_manager.clone();
    let (tx_id, transaction) = asset_manager
        .create_constitution_definition(&side_chain_features)
        .await?;

    let message = format!(
        "Contract constitution with {} members for {}",
        constitution_definition.validator_committee.len(),
        constitution_definition.contract_id
    );
    let mut transaction_service = wallet.transaction_service.clone();
    transaction_service
        .submit_transaction(tx_id, transaction, 0.into(), message)
        .await?;

    Ok(())
}

async fn publish_contract_update_proposal(wallet: &WalletSqlite, args: PublishFileArgs) -> Result<(), CommandError> {
    // parse the JSON file
    let update_proposal: ContractUpdateProposalFileFormat =
        read_json_file(&args.file_path).map_err(|e| CommandError::JsonFile(e.to_string()))?;

    let contract_id_hex = update_proposal.updated_constitution.contract_id.clone();
    let contract_id = FixedHash::from_hex(&contract_id_hex).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let update_proposal_features = ContractUpdateProposal::try_from(update_proposal).map_err(CommandError::JsonFile)?;

    let mut asset_manager = wallet.asset_manager.clone();
    let (tx_id, transaction) = asset_manager
        .create_update_proposal(&contract_id, &update_proposal_features)
        .await?;

    let message = format!(
        "Contract update proposal {} for contract {}",
        update_proposal_features.proposal_id, contract_id_hex
    );

    let mut transaction_service = wallet.transaction_service.clone();
    transaction_service
        .submit_transaction(tx_id, transaction, 0.into(), message)
        .await?;

    println!(
        "Contract update proposal transaction submitted with tx_id={} for contract with contract_id={}",
        tx_id, contract_id_hex
    );

    Ok(())
}

async fn publish_contract_amendment(wallet: &WalletSqlite, args: PublishFileArgs) -> Result<(), CommandError> {
    // parse the JSON file
    let amendment: ContractAmendmentFileFormat =
        read_json_file(&args.file_path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let contract_id_hex = amendment.updated_constitution.contract_id.clone();
    let contract_id = FixedHash::from_hex(&contract_id_hex).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let amendment_features = ContractAmendment::try_from(amendment).map_err(CommandError::JsonFile)?;

    let mut asset_manager = wallet.asset_manager.clone();
    let (tx_id, transaction) = asset_manager
        .create_contract_amendment(&contract_id, &amendment_features)
        .await?;

    let message = format!(
        "Contract amendment {} for contract {}",
        amendment_features.proposal_id, contract_id_hex
    );

    let mut transaction_service = wallet.transaction_service.clone();
    transaction_service
        .submit_transaction(tx_id, transaction, 0.into(), message)
        .await?;

    println!(
        "Contract amendment transaction submitted with tx_id={} for contract with contract_id={}",
        tx_id, contract_id_hex
    );

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

fn write_json_file<P: AsRef<Path>, T: Serialize>(path: P, data: &T) -> Result<(), CommandError> {
    fs::create_dir_all(path.as_ref().parent().unwrap()).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    let file = File::create(path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    serde_json::to_writer_pretty(file, data).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    Ok(())
}

fn read_json_file<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T, CommandError> {
    let file = File::open(path).map_err(|e| CommandError::JsonFile(e.to_string()))?;
    serde_json::from_reader(file).map_err(|e| CommandError::JsonFile(e.to_string()))
}

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
