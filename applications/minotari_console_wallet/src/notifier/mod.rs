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
    io::Error,
    path::PathBuf,
    process::{Command, Output},
};

use log::*;
use minotari_wallet::{
    transaction_service::storage::models::{
        CompletedTransaction,
        InboundTransaction,
        OutboundTransaction,
        WalletTransaction,
    },
    WalletSqlite,
};
use tari_common_types::transaction::TxId;
use tari_utilities::hex::Hex;
use tokio::{runtime::Handle, sync::broadcast::Sender};
pub const LOG_TARGET: &str = "wallet::notifier";
pub const RECEIVED: &str = "received";
pub const SENT: &str = "sent";
pub const QUEUED: &str = "queued";
pub const CONFIRMATION: &str = "confirmation";
pub const MINED: &str = "mined";
pub const CANCELLED: &str = "cancelled";

#[derive(Clone)]
// FIXME
#[allow(dead_code)]
#[allow(clippy::large_enum_variant)]
pub enum WalletEventMessage {
    Completed {
        event: String,
        transaction: CompletedTransaction,
    },
    Outbound {
        event: String,
        transaction: OutboundTransaction,
    },
    Inbound {
        event: String,
        transaction: InboundTransaction,
    },
}
#[derive(Clone)]
pub struct Notifier {
    path: Option<PathBuf>,
    handle: Handle,
    wallet: WalletSqlite,
    event_broadcaster: Sender<WalletEventMessage>,
}

impl Notifier {
    pub fn new(
        path: Option<PathBuf>,
        handle: Handle,
        wallet: WalletSqlite,
        event_broadcaster: Sender<WalletEventMessage>,
    ) -> Self {
        Self {
            path,
            handle,
            wallet,
            event_broadcaster,
        }
    }

    /// Trigger a notification that a negotiated transaction was received.
    pub fn transaction_received(&self, tx_id: TxId) {
        debug!(target: LOG_TARGET, "transaction_received tx_id: {}", tx_id);

        if let Some(program) = self.path.clone() {
            let mut transaction_service = self.wallet.transaction_service.clone();
            let sender = self.event_broadcaster.clone();
            self.handle.spawn(async move {
                match transaction_service.get_completed_transaction(tx_id).await {
                    Ok(tx) => {
                        let args = args_from_complete(&tx, RECEIVED, None);
                        let result = Command::new(program).args(&args).output();
                        let _ignored = sender.send(WalletEventMessage::Completed {
                            event: RECEIVED.to_string(),
                            transaction: tx,
                        });
                        log(result);
                    },
                    Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
                }
            });
        } else {
            trace!(target: LOG_TARGET, "No script defined, not running.");
        }
    }

    /// Trigger a notification that a transaction was mined with a given number of confirmations.
    pub fn transaction_mined_unconfirmed(&self, tx_id: TxId, confirmations: u64) {
        debug!(target: LOG_TARGET, "transaction_mined_unconfirmed tx_id: {}", tx_id);

        if let Some(program) = self.path.clone() {
            let mut transaction_service = self.wallet.transaction_service.clone();
            let sender = self.event_broadcaster.clone();
            self.handle.spawn(async move {
                match transaction_service.get_completed_transaction(tx_id).await {
                    Ok(tx) => {
                        let args = args_from_complete(&tx, CONFIRMATION, Some(confirmations));
                        let result = Command::new(program).args(&args).output();
                        let message = WalletEventMessage::Completed {
                            event: CONFIRMATION.to_string(),
                            transaction: tx,
                        };
                        let _ignored = sender.send(message);
                        log(result);
                    },
                    Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
                }
            });
        } else {
            trace!(target: LOG_TARGET, "No script defined, not running.");
        }
    }

    /// Trigger a notification that a transaction was mined, with the accepted number of required confirmations.
    pub fn transaction_mined(&self, tx_id: TxId) {
        debug!(target: LOG_TARGET, "transaction_mined tx_id: {}", tx_id);

        if let Some(program) = self.path.clone() {
            let mut transaction_service = self.wallet.transaction_service.clone();
            let sender = self.event_broadcaster.clone();
            self.handle.spawn(async move {
                match transaction_service.get_completed_transaction(tx_id).await {
                    Ok(tx) => {
                        let confirmations = match transaction_service.get_num_confirmations_required().await {
                            Ok(n) => Some(n),
                            Err(e) => {
                                error!(target: LOG_TARGET, "Transaction service error: {}", e);
                                None
                            },
                        };

                        let args = args_from_complete(&tx, MINED, confirmations);
                        let result = Command::new(program).args(&args).output();
                        let message = WalletEventMessage::Completed {
                            event: MINED.to_string(),
                            transaction: tx,
                        };
                        let _ignored = sender.send(message);
                        log(result);
                    },
                    Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
                }
            });
        } else {
            trace!(target: LOG_TARGET, "No script defined, not running.");
        }
    }

    /// Trigger a notification that a pending transaction was sent or queued.
    pub fn transaction_sent_or_queued(&self, tx_id: TxId, is_sent: bool) {
        let event = if is_sent {
            debug!(target: LOG_TARGET, "Transaction sent tx_id: {}", tx_id);
            SENT
        } else {
            debug!(
                target: LOG_TARGET,
                "Transaction queued for further retry sending tx_id: {}", tx_id
            );
            QUEUED
        };

        if let Some(program) = self.path.clone() {
            let mut transaction_service = self.wallet.transaction_service.clone();
            let sender = self.event_broadcaster.clone();
            self.handle.spawn(async move {
                match transaction_service.get_pending_outbound_transactions().await {
                    Ok(txs) => {
                        if let Some(tx) = txs.get(&tx_id) {
                            let args = args_from_outbound(tx, event);
                            let result = Command::new(program).args(&args).output();
                            let message = WalletEventMessage::Outbound {
                                event: event.to_string(),
                                transaction: tx.clone(),
                            };
                            let _ignored = sender.send(message);

                            log(result);
                        } else {
                            error!(target: LOG_TARGET, "Not found in pending outbound set tx_id: {}", tx_id);
                        }
                    },
                    Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
                }
            });
        } else {
            trace!(target: LOG_TARGET, "No script defined, not running.");
        }
    }

    /// Trigger a notification that a transaction was cancelled.
    pub fn transaction_cancelled(&self, tx_id: TxId) {
        debug!(target: LOG_TARGET, "transaction_cancelled tx_id: {}", tx_id);

        if let Some(program) = self.path.clone() {
            let mut transaction_service = self.wallet.transaction_service.clone();
            let sender = self.event_broadcaster.clone();
            self.handle.spawn(async move {
                match transaction_service.get_any_transaction(tx_id).await {
                    Ok(Some(wallet_tx)) => {
                        let args = match wallet_tx {
                            WalletTransaction::Completed(tx) => {
                                let message = WalletEventMessage::Completed {
                                    event: CANCELLED.to_string(),
                                    transaction: tx.clone(),
                                };
                                let _ignored = sender.send(message);
                                args_from_complete(&tx, CANCELLED, None)
                            },
                            WalletTransaction::PendingInbound(tx) => {
                                let message = WalletEventMessage::Inbound {
                                    event: CANCELLED.to_string(),
                                    transaction: tx.clone(),
                                };
                                let _ignored = sender.send(message);
                                args_from_inbound(&tx, CANCELLED)
                            },
                            WalletTransaction::PendingOutbound(tx) => {
                                let message = WalletEventMessage::Outbound {
                                    event: CANCELLED.to_string(),
                                    transaction: tx.clone(),
                                };
                                let _ignored = sender.send(message);
                                args_from_outbound(&tx, CANCELLED)
                            },
                        };
                        // c
                        let result = Command::new(program).args(&args).output();
                        log(result);
                    },
                    Err(e) => error!(target: LOG_TARGET, "Transaction service error: {}", e),
                    _ => error!(target: LOG_TARGET, "Transaction not found tx_id: {}", tx_id),
                }
            });
        } else {
            trace!(target: LOG_TARGET, "No script defined, not running.");
        }
    }
}

fn log(result: Result<Output, Error>) {
    match result {
        Ok(output) => {
            let code = match output.status.code() {
                Some(code) => code.to_string(),
                None => "None (killed by signal)".to_string(),
            };
            debug!(target: LOG_TARGET, "Notify script succeeded with status code: {}", code);
        },
        Err(e) => {
            error!(target: LOG_TARGET, "Notify script failed! Error: {}", e);
        },
    }
}

fn args_from_complete(tx: &CompletedTransaction, event: &str, confirmations: Option<u64>) -> Vec<String> {
    trace!(target: LOG_TARGET, "Getting args from completed tx {:?}", tx);
    let amount = format!("{}", tx.amount);
    let status = format!("{}", tx.status);
    let direction = format!("{}", tx.direction);
    let payment_id = format!("{}", tx.payment_id);

    let kernel = tx.transaction.body.kernels().first();
    let (excess, public_nonce, signature) = match kernel {
        Some(kernel) => {
            let excess_sig = &kernel.excess_sig;
            (
                kernel.excess.to_hex(),
                excess_sig.get_public_nonce().to_hex(),
                excess_sig.get_signature().to_hex(),
            )
        },
        None => ("".to_string(), "".to_string(), "".to_string()),
    };

    let confirmations = match confirmations {
        Some(n) => n.to_string(),
        None => "".to_string(),
    };

    vec![
        String::from(event),
        amount,
        tx.tx_id.to_string(),
        tx.message.clone(),
        payment_id,
        tx.source_address.to_base58(),
        tx.destination_address.to_base58(),
        status,
        excess,
        public_nonce,
        signature,
        confirmations,
        direction,
    ]
}

fn args_from_outbound(tx: &OutboundTransaction, event: &str) -> Vec<String> {
    trace!(target: LOG_TARGET, "Getting args from outbound tx {:?}", tx);
    let amount = format!("{}", tx.amount);
    let status = format!("{}", tx.status);

    vec![
        String::from(event),
        amount,
        tx.tx_id.to_string(),
        tx.message.clone(),
        tx.destination_address.to_base58(),
        status,
        "outbound".to_string(),
    ]
}

fn args_from_inbound(tx: &InboundTransaction, event: &str) -> Vec<String> {
    trace!(target: LOG_TARGET, "Getting args from inbound tx {:?}", tx);
    let amount = format!("{}", tx.amount);
    let status = format!("{}", tx.status);

    vec![
        String::from(event),
        amount,
        tx.tx_id.to_string(),
        tx.message.clone(),
        tx.source_address.to_base58(),
        status,
        "inbound".to_string(),
    ]
}
