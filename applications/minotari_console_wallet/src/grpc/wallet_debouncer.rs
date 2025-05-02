//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::sync::Arc;

use log::{info, trace, warn};
use minotari_app_grpc::tari_rpc::GetBalanceResponse;
use minotari_wallet::{
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInterface},
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::Balance,
    },
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    utxo_scanner_service::handle::{UtxoScannerEvent, UtxoScannerHandle},
};
use tari_shutdown::ShutdownSignal;
use tokio::sync::Mutex;
use tonic::Status;

const LOG_TARGET: &str = "wallet::ui::grpc::get_balance_debounced";

/// This struct is used to get the balance of the wallet, implementing a debouncer. When the `get_balance` method is
/// called the first time, the balance will be fetched from the backend after starting a task to monitor wallet events
/// that could change the balance. When these wallet events are received, a flag will be set to indicate that the
/// balance needs to be updated. When ever a client requests the balance, it will be fetched from the backend if the
/// flag is set and clear the flag, otherwise the cached balance will be returned.
#[derive(Clone)]
pub struct WalletDebouncer {
    balance: Arc<Mutex<Balance>>,
    scanned_height: Arc<Mutex<u64>>,
    refresh_needed: Arc<Mutex<bool>>,
    output_manager_service: OutputManagerHandle,
    transaction_service: TransactionServiceHandle,
    wallet_connectivity: WalletConnectivityHandle,
    utxo_scanner_handle: UtxoScannerHandle,
    shutdown_signal: ShutdownSignal,
    event_monitor_started: Arc<Mutex<bool>>,
}

impl WalletDebouncer {
    /// Create a new WalletDebouncer instance.
    pub fn new(
        output_manager_service: OutputManagerHandle,
        transaction_service: TransactionServiceHandle,
        wallet_connectivity: WalletConnectivityHandle,
        utxo_scanner_handle: UtxoScannerHandle,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            balance: Arc::new(Mutex::new(Balance {
                available_balance: 0.into(),
                pending_incoming_balance: 0.into(),
                pending_outgoing_balance: 0.into(),
                time_locked_balance: None,
            })),
            refresh_needed: Arc::new(Mutex::new(true)),
            scanned_height: Arc::new(Mutex::new(0)),
            output_manager_service,
            transaction_service,
            wallet_connectivity,
            utxo_scanner_handle,
            shutdown_signal,
            event_monitor_started: Arc::new(Mutex::new(false)),
        }
    }

    async fn start_event_monitor(&mut self) {
        trace!(target: LOG_TARGET, "start_event_monitor");
        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.monitor_events().await;
        });
        let mut lock = self.event_monitor_started.lock().await;
        *lock = true;
    }

    async fn is_event_monitor_started(&self) -> bool {
        *self.event_monitor_started.lock().await
    }

    /// Get the balance of the wallet. This function will return the cached balance of the wallet if it is current, or
    /// fetch the balance from the output manager service if new wallet events were received that could change the
    /// balance.
    pub async fn get_balance(&mut self) -> Result<GetBalanceResponse, Status> {
        if !self.is_event_monitor_started().await {
            self.start_event_monitor().await;
        }
        let balance = if self.is_refresh_needed().await {
            let mut output_manager_service = self.output_manager_service.clone();
            let balance = match output_manager_service.get_balance().await {
                Ok(b) => b,
                Err(e) => return Err(Status::not_found(format!("GetBalance error! {}", e))),
            };
            self.update_balance(balance.clone()).await;
            self.set_refresh_needed(false).await;
            balance
        } else {
            (*self.balance.lock().await).clone()
        };
        Ok(GetBalanceResponse {
            available_balance: balance.available_balance.into(),
            pending_incoming_balance: balance.pending_incoming_balance.into(),
            pending_outgoing_balance: balance.pending_outgoing_balance.into(),
            timelocked_balance: balance.time_locked_balance.unwrap_or_default().into(),
        })
    }

    async fn update_balance(&mut self, balance: Balance) {
        let mut lock = self.balance.lock().await;
        *lock = balance;
    }

    async fn is_refresh_needed(&self) -> bool {
        let refresh_needed = *self.refresh_needed.lock().await;
        trace!(target: LOG_TARGET, "is_refresh_needed '{}'", refresh_needed);
        refresh_needed
    }

    async fn set_refresh_needed(&self, refresh_needed: bool) {
        let mut lock = self.refresh_needed.lock().await;
        if *lock != refresh_needed {
            trace!(target: LOG_TARGET, "set_refresh_needed '{}'", refresh_needed);
            *lock = refresh_needed;
        }
    }

    async fn update_scanned_height(&self, scanned_height: u64) {
        let mut lock = self.scanned_height.lock().await;
        if *lock != scanned_height {
            trace!(target: LOG_TARGET, "set_scanned_height '{}'", scanned_height);
            *lock = scanned_height;
        }
    }

    pub async fn get_scanned_height(&mut self) -> u64 {
        if !self.is_event_monitor_started().await {
            self.start_event_monitor().await;
        }
        *self.scanned_height.lock().await
    }

    async fn monitor_events(&self) {
        let mut shutdown_signal = self.shutdown_signal.clone();
        let mut transaction_service_events = self.transaction_service.get_event_stream();
        let mut base_node_changed = self.wallet_connectivity.clone().get_current_base_node_watcher();
        let mut output_manager_service_events = self.output_manager_service.get_event_stream();
        let mut utxo_scanner_events = self.utxo_scanner_handle.clone().get_event_receiver();

        loop {
            tokio::select! {
                result = transaction_service_events.recv() => {
                    match result {
                        Ok(msg) => {
                            match (*msg).clone() {
                                TransactionEvent::ReceivedTransaction(..) |
                                TransactionEvent::ReceivedTransactionReply(..) |
                                TransactionEvent::ReceivedFinalizedTransaction(_) |
                                TransactionEvent::TransactionSendResult(..) |
                                TransactionEvent::TransactionCompletedImmediately(..) |
                                TransactionEvent::TransactionCancelled(..) |
                                TransactionEvent::TransactionBroadcast(..) |
                                TransactionEvent::DetectedTransactionUnconfirmed { .. } |
                                TransactionEvent::DetectedTransactionConfirmed { .. } |
                                TransactionEvent::TransactionMinedUnconfirmed { .. } |
                                TransactionEvent::TransactionMined { .. } |
                                TransactionEvent::TransactionValidationStateChanged(..) => {
                                    self.set_refresh_needed(true).await;
                                },
                                _ => (),
                            }
                        },
                        Err(e) => {
                            warn!(target: LOG_TARGET, "transaction_service_events '{}'", e);
                        },
                    }
                },
                _ = base_node_changed.changed() => {
                    self.set_refresh_needed(true).await;
                },
                result = output_manager_service_events.recv() => {
                    match result {
                        Ok(msg) => {
                            if let OutputManagerEvent::TxoValidationSuccess(_) = &*msg {
                                self.set_refresh_needed(true).await;
                            }
                        },
                        Err(e) => {
                            warn!(target: LOG_TARGET, "output_manager_service_events '{}'", e);
                        },
                    }
                },
                result = utxo_scanner_events.recv() => {
                    match result {
                        Ok(event) => {
                            match event {
                                UtxoScannerEvent::Progress {
                                    current_height,..
                                }=> {
                                    self.update_scanned_height(current_height).await;
                                }
                                UtxoScannerEvent::Completed {
                                    final_height,
                                    ..
                                }=> {
                                    self.update_scanned_height(final_height).await;
                                },
                                _ => {}
                            }
                        },
                        Err(e) => {
                            warn!(target: LOG_TARGET, "Problem with utxo scanner: {}",e);
                        },
                    }
                },
                _ = shutdown_signal.wait() => {
                    info!(
                        target: LOG_TARGET,
                        "get_balance_debounced event monitor shutting down because the shutdown signal was received"
                    );
                    break;
                }
            }
        }
    }
}
