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

use crate::automation::command_parser::{ParsedArgument, ParsedCommand};
use chrono::{DateTime, Utc};

use log::*;
use std::{str::FromStr, time::Duration};
use strum_macros::{Display, EnumIter, EnumString};
use tari_common::GlobalConfig;
use tari_core::transactions::tari_amount::uT;
use tari_wallet::{
    output_manager_service::TxId,
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    WalletSqlite,
};
use tokio::{
    runtime::Handle,
    stream::StreamExt,
    time::{delay_for, timeout},
};

use super::error::CommandError;

pub const LOG_TARGET: &str = "tari_console_wallet::commands";

/// Enum representing commands used by the wallet
#[derive(Clone, PartialEq, Debug, Display, EnumIter, EnumString)]
#[strum(serialize_all = "kebab_case")]
pub enum WalletCommand {
    GetBalance,
    SendTari,
    MakeItRain,
}

#[derive(Debug, EnumString, PartialEq, Clone)]
pub enum TransactionStage {
    Initiated,
    DirectSendOrSaf,
    Negotiated,
    Broadcast,
    Mined,
    Timedout,
}

#[derive(Debug)]
pub struct SentTransaction {
    id: TxId,
    stage: TransactionStage,
}

pub async fn send_tari(
    mut wallet_transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<TxId, CommandError>
{
    // todo: consolidate "fee per gram" in codebase
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

    wallet_transaction_service
        .send_transaction(dest_pubkey, amount, fee_per_gram, message)
        .await
        .map_err(CommandError::Transaction)
}

pub async fn make_it_rain(
    handle: Handle,
    wallet_transaction_service: TransactionServiceHandle,
    args: Vec<ParsedArgument>,
) -> Result<Vec<TxId>, CommandError>
{
    use ParsedArgument::*;

    let txs = match args[0].clone() {
        Int(r) => Ok(r),
        _ => Err(CommandError::Argument),
    }?;

    let start_amount = match args[1].clone() {
        Amount(mtari) => Ok(mtari),
        _ => Err(CommandError::Argument),
    }?;

    let inc_amount = match args[2].clone() {
        Amount(mtari) => Ok(mtari),
        _ => Err(CommandError::Argument),
    }?;

    let start_time = match args[3].clone() {
        Date(dt) => Ok(dt as DateTime<Utc>),
        _ => Err(CommandError::Argument),
    }?;

    let public_key = match args[4].clone() {
        PublicKey(pk) => Ok(pk),
        _ => Err(CommandError::Argument),
    }?;

    let message = match args[5].clone() {
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

    let mut tx_ids = Vec::new();

    for i in 0..txs {
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
                            "direct send event for tx_id: {:?} {:?}", *id, success
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
                            "store and forward event for tx_id: {:?} {:?}", *id, success
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
                        debug!(target: LOG_TARGET, "reply event for tx_id: {:?}", *id);
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
                        debug!(target: LOG_TARGET, "mempool broadcast event for tx_id: {:?}", *id);
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
                    TransactionEvent::TransactionMined(id) if tx_ids.contains(id) => {
                        debug!(target: LOG_TARGET, "tx mined event for tx_id: {:?}", *id);
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

    let mut tx_ids = Vec::new();

    for (idx, parsed) in commands.into_iter().enumerate() {
        println!("{}. {}", idx + 1, parsed);

        match parsed.command {
            WalletCommand::GetBalance => match output_service.get_balance().await {
                Ok(balance) => {
                    println!("====== Wallet Balance ======");
                    println!("{}", balance);
                    println!("============================");
                },
                Err(e) => eprintln!("GetBalance error! {}", e),
            },
            WalletCommand::SendTari => {
                let tx_id = send_tari(transaction_service.clone(), parsed.args).await?;
                debug!(target: LOG_TARGET, "send-tari tx_id {}", tx_id);
                tx_ids.push(tx_id);
            },
            WalletCommand::MakeItRain => {
                let rain_ids = make_it_rain(handle.clone(), transaction_service.clone(), parsed.args).await?;
                tx_ids.extend(rain_ids);
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
