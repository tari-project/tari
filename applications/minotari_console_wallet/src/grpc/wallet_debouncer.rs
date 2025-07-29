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

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use log::{info, trace, warn};
use minotari_app_grpc::tari_rpc::GetBalanceResponse;
use minotari_wallet::{
    connectivity_service::{OnlineStatus, WalletConnectivityInterface},
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::Balance,
    },
    transaction_service::handle::{TransactionEvent, TransactionServiceHandle},
    utxo_scanner_service::handle::{UtxoScannerEvent, UtxoScannerHandle},
    WalletSqlite,
};
use tari_shutdown::ShutdownSignal;
use tokio::sync::Mutex;
use tonic::Status;

const LOG_TARGET: &str = "wallet::ui::grpc::get_balance_debounced";
const CONNECTIVITY_CHECK_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

/// This struct is used to get the balance of the wallet, implementing a debouncer. When the `get_balance` method is
/// called the first time, the balance will be fetched from the backend after starting a task to monitor wallet events
/// that could change the balance. When these wallet events are received, a flag will be set to indicate that the
/// balance needs to be updated. When ever a client requests the balance, it will be fetched from the backend if the
/// flag is set and clear the flag, otherwise the cached balance will be returned.
///
/// Additionally, this struct implements enhanced online status detection:
/// - Tracks wallet scanning activity as an indicator of connectivity
/// - Proactively checks connectivity when no scanning occurs for 5 minutes
/// - Provides more reliable online/offline status for gRPC clients like mining applications
#[derive(Clone)]
pub struct WalletDebouncer {
    balance: Arc<Mutex<Balance>>,
    scanned_height: Arc<AtomicU64>,
    refresh_needed: Arc<AtomicBool>,
    intial_scanning_done: Arc<AtomicBool>,
    initial_validation_done: Arc<AtomicBool>,
    output_manager_service: OutputManagerHandle,
    transaction_service: TransactionServiceHandle,
    utxo_scanner_handle: UtxoScannerHandle,
    wallet: WalletSqlite,
    shutdown_signal: ShutdownSignal,
    event_monitor_started: Arc<AtomicBool>,
    last_scan_activity: Arc<AtomicU64>,
    connection_status: Arc<Mutex<OnlineStatus>>,
}

impl WalletDebouncer {
    /// Create a new WalletDebouncer instance.
    pub fn new(
        output_manager_service: OutputManagerHandle,
        transaction_service: TransactionServiceHandle,
        utxo_scanner_handle: UtxoScannerHandle,
        wallet: WalletSqlite,
        shutdown_signal: ShutdownSignal,
        scanned_height: u64,
    ) -> Self {
        Self {
            balance: Arc::new(Mutex::new(Balance {
                available_balance: 0.into(),
                pending_incoming_balance: 0.into(),
                pending_outgoing_balance: 0.into(),
                time_locked_balance: None,
            })),
            refresh_needed: Arc::new(AtomicBool::new(true)),
            intial_scanning_done: Arc::new(AtomicBool::new(false)),
            initial_validation_done: Arc::new(AtomicBool::new(false)),
            scanned_height: Arc::new(AtomicU64::new(scanned_height)),
            output_manager_service,
            transaction_service,
            utxo_scanner_handle,
            wallet,
            shutdown_signal,
            event_monitor_started: Arc::new(AtomicBool::new(false)),
            last_scan_activity: Arc::new(AtomicU64::new(scanned_height + 1)), /* Add 1 to pass initial connectivity
                                                                               * check */
            connection_status: Arc::new(Mutex::new(OnlineStatus::Online)),
        }
    }

    pub async fn start_event_monitor_if_needed(&mut self) {
        if !self.is_event_monitor_started() {
            trace!(target: LOG_TARGET, "start_event_monitor");
            let self_clone = self.clone();
            tokio::spawn(async move {
                self_clone.monitor_events().await;
            });
            self.event_monitor_started.store(true, Ordering::SeqCst);
        }
    }

    fn is_event_monitor_started(&self) -> bool {
        self.event_monitor_started.load(Ordering::SeqCst)
    }

    /// Get the balance of the wallet. This function will return the cached balance of the wallet if it is current, or
    /// fetch the balance from the output manager service if new wallet events were received that could change the
    /// balance.
    pub async fn get_balance(&mut self) -> Result<GetBalanceResponse, Status> {
        self.start_event_monitor_if_needed().await;
        let balance = if self.is_refresh_needed() {
            let mut output_manager_service = self.output_manager_service.clone();
            let balance = match output_manager_service.get_balance().await {
                Ok(b) => b,
                Err(e) => return Err(Status::not_found(format!("GetBalance error! {}", e))),
            };
            self.update_balance(balance.clone()).await;
            self.set_refresh_needed(false);
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

    fn is_refresh_needed(&self) -> bool {
        let refresh_needed = self.refresh_needed.load(Ordering::SeqCst);
        trace!(target: LOG_TARGET, "is_refresh_needed '{}'", refresh_needed);
        refresh_needed
    }

    pub fn is_initial_validation_done(&self) -> bool {
        self.initial_validation_done.load(Ordering::SeqCst)
    }

    fn set_refresh_needed(&self, refresh_needed: bool) {
        let old_value = self.refresh_needed.swap(refresh_needed, Ordering::SeqCst);
        if old_value != refresh_needed {
            trace!(target: LOG_TARGET, "set_refresh_needed '{}'", refresh_needed);
        }
    }

    async fn update_scanned_height(&self, scanned_height: u64) {
        let lock = self.scanned_height.load(Ordering::SeqCst);
        if lock != scanned_height {
            trace!(target: LOG_TARGET, "set_scanned_height '{}'", scanned_height);
            self.scanned_height.store(scanned_height, Ordering::SeqCst);
        }
    }

    pub async fn get_scanned_height(&mut self) -> u64 {
        self.start_event_monitor_if_needed().await;
        self.scanned_height.load(Ordering::SeqCst)
    }

    async fn monitor_events(&self) {
        let mut shutdown_signal = self.shutdown_signal.clone();
        let mut transaction_service_events = self.transaction_service.get_event_stream();
        let mut output_manager_service_events = self.output_manager_service.get_event_stream();
        let mut utxo_scanner_events = self.utxo_scanner_handle.clone().get_event_receiver();

        let self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.monitor_connectivity().await;
        });

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
                                TransactionEvent::TransactionMined { .. } |
                                TransactionEvent::TransactionMinedUnconfirmed { .. } |
                                TransactionEvent::TransactionImported(_)  => {
                                    self.set_refresh_needed(true);
                                },
                                TransactionEvent::TransactionValidationStateChanged{faux, ..} => {
                                    self.set_refresh_needed(true);
                                    #[allow(clippy::collapsible_if)]
                                    if faux {
                                        let intial = self.initial_validation_done.load(Ordering::SeqCst);
                                        if !intial {
                                            if self.intial_scanning_done.load(Ordering::SeqCst) {
                                                // we should only set this after we completed initial scanning and then completed faux tx validation
                                                self.initial_validation_done.store(true, Ordering::SeqCst);
                                            }
                                        }
                                    }
                                },
                                _ => (),
                            }
                        },
                        Err(e) => {
                            warn!(target: LOG_TARGET, "transaction_service_events '{}'", e);
                        },
                    }
                },
               result = output_manager_service_events.recv() => {
                    match result {
                        Ok(msg) => {
                            if let OutputManagerEvent::TxoValidationSuccess(_) = &*msg {
                                self.set_refresh_needed(true);
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
                            trace!(target: LOG_TARGET, "utxo_scanner_events '{:?}'", event);
                            match event {
                                UtxoScannerEvent::Progress {
                                    current_height,..
                                }=> {
                                    self.update_scanned_height(current_height).await;
                                    self.update_last_scan_activity().await;
                                }
                                UtxoScannerEvent::Completed {
                                    final_height,
                                    ..
                                }=> {
                                    self.intial_scanning_done.store(true, Ordering::SeqCst);
                                    self.update_scanned_height(final_height).await;
                                    self.update_last_scan_activity().await;
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

    /// Updates the last scan activity height
    /// This is called whenever the wallet successfully scans a block, indicating active connectivity.
    async fn update_last_scan_activity(&self) {
        let scanned_height = self.scanned_height.load(Ordering::SeqCst);
        self.last_scan_activity.store(scanned_height, Ordering::SeqCst);
        trace!(target: LOG_TARGET, "Updated scan activity height - wallet is online");
    }

    pub async fn get_connection_status(&self) -> OnlineStatus {
        *self.connection_status.lock().await
    }

    /// Background task that monitors connectivity proactively.
    /// Runs every 5 minutes to check if the wallet should test connectivity
    /// when no recent scanning activity has occurred.
    async fn monitor_connectivity(&self) {
        let mut shutdown_signal = self.shutdown_signal.clone();
        let mut interval = tokio::time::interval(CONNECTIVITY_CHECK_INTERVAL);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.check_connectivity().await;
                },
                _ = shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "Connectivity monitor shutting down");
                    break;
                }
            }
        }
    }

    /// Check if height has changed since last scan
    /// if not then get tip info if we get response we got online otherwise offline
    async fn check_connectivity(&self) -> OnlineStatus {
        let scanned_height = self.scanned_height.load(Ordering::SeqCst);
        let last_scan_activity = self.last_scan_activity.load(Ordering::SeqCst);

        // If no recent scanning activity, test connectivity
        let connectivity = if last_scan_activity == scanned_height {
            // No new blocks scanned - test if we can reach the base node
            let connectivity = self.wallet.wallet_connectivity.clone();
            connectivity.get_connectivity_status().await
        } else {
            // Recent scanning activity indicates we're online
            trace!(target: LOG_TARGET, "Recent scanning activity detected - wallet is online");
            OnlineStatus::Online
        };
        // Update connection status
        let mut connection_status_guard = self.connection_status.lock().await;
        *connection_status_guard = connectivity;
        connectivity
    }
}
