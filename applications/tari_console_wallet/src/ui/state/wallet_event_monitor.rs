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

use std::sync::Arc;

use log::*;
use tari_common_types::transaction::TxId;
use tari_comms::{connectivity::ConnectivityEvent, peer_manager::Peer};
use tari_wallet::{
    base_node_service::{handle::BaseNodeEvent, service::BaseNodeState},
    connectivity_service::WalletConnectivityInterface,
    output_manager_service::handle::OutputManagerEvent,
    transaction_service::handle::TransactionEvent,
};
use tokio::sync::{broadcast, RwLock};

use crate::{
    notifier::Notifier,
    ui::state::{AppStateInner, EventListItem},
};

const LOG_TARGET: &str = "wallet::console_wallet::wallet_event_monitor";

pub struct WalletEventMonitor {
    app_state_inner: Arc<RwLock<AppStateInner>>,
    balance_enquiry_debounce_tx: broadcast::Sender<()>,
}

impl WalletEventMonitor {
    pub fn new(
        app_state_inner: Arc<RwLock<AppStateInner>>,
        balance_enquiry_debounce_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            app_state_inner,
            balance_enquiry_debounce_tx,
        }
    }

    pub async fn run(mut self, notifier: Notifier) {
        let mut shutdown_signal = self.app_state_inner.read().await.get_shutdown_signal();
        let mut transaction_service_events = self.app_state_inner.read().await.get_transaction_service_event_stream();

        let mut output_manager_service_events = self
            .app_state_inner
            .read()
            .await
            .get_output_manager_service_event_stream();

        let mut connectivity_events = self.app_state_inner.read().await.get_connectivity_event_stream();
        let wallet_connectivity = self.app_state_inner.read().await.get_wallet_connectivity();
        let mut connectivity_status = wallet_connectivity.get_connectivity_status_watch();
        let mut base_node_changed = wallet_connectivity.get_current_base_node_watcher();

        let mut base_node_events = self.app_state_inner.read().await.get_base_node_event_stream();
        let mut software_update_notif = self
            .app_state_inner
            .read()
            .await
            .get_software_updater()
            .new_update_notifier()
            .clone();

        info!(target: LOG_TARGET, "Wallet Event Monitor starting");
        loop {
            tokio::select! {
                    result = transaction_service_events.recv() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Wallet Event Monitor received wallet transaction service event {:?}", msg);
                            self.app_state_inner.write().await.add_event(EventListItem{event_type: "TransactionEvent".to_string(), desc: (&*msg).to_string() });
                                match (*msg).clone() {
                                    TransactionEvent::ReceivedFinalizedTransaction(tx_id) => {
                                        self.trigger_tx_state_refresh(tx_id).await;
                                        self.trigger_balance_refresh();
                                        notifier.transaction_received(tx_id);
                                    },
                                    TransactionEvent::TransactionMinedUnconfirmed{tx_id, num_confirmations, is_valid: _}  |
                                    TransactionEvent::FauxTransactionUnconfirmed{tx_id, num_confirmations, is_valid: _}=> {
                                        self.trigger_confirmations_refresh(tx_id, num_confirmations).await;
                                        self.trigger_tx_state_refresh(tx_id).await;
                                        self.trigger_balance_refresh();
                                        notifier.transaction_mined_unconfirmed(tx_id, num_confirmations);
                                    },
                                    TransactionEvent::TransactionMined{tx_id, is_valid: _} |
                                    TransactionEvent::FauxTransactionConfirmed{tx_id, is_valid: _}=> {
                                        self.trigger_confirmations_cleanup(tx_id).await;
                                        self.trigger_tx_state_refresh(tx_id).await;
                                        self.trigger_balance_refresh();
                                        notifier.transaction_mined(tx_id);
                                    },
                                    TransactionEvent::TransactionCancelled(tx_id, _) => {
                                        self.trigger_tx_state_refresh(tx_id).await;
                                        self.trigger_balance_refresh();
                                        notifier.transaction_cancelled(tx_id);
                                    },
                                    TransactionEvent::ReceivedTransaction(tx_id) |
                                    TransactionEvent::ReceivedTransactionReply(tx_id) |
                                    TransactionEvent::TransactionBroadcast(tx_id) |
                                    TransactionEvent::TransactionMinedRequestTimedOut(tx_id) |
                                    TransactionEvent::TransactionImported(tx_id)  => {
                                        self.trigger_tx_state_refresh(tx_id).await;
                                        self.trigger_balance_refresh();
                                    },
                                    TransactionEvent::TransactionDirectSendResult(tx_id, true) |
                                    TransactionEvent::TransactionStoreForwardSendResult(tx_id, true) |
                                    TransactionEvent::TransactionCompletedImmediately(tx_id) => {
                                        self.trigger_tx_state_refresh(tx_id).await;
                                        self.trigger_balance_refresh();
                                        notifier.transaction_sent(tx_id);
                                    },
                                    TransactionEvent::TransactionValidationStateChanged(_) => {
                                        self.trigger_full_tx_state_refresh().await;
                                        self.trigger_balance_refresh();
                                    },
                                    // Only the above variants trigger state refresh
                                    _ => (),
                                }
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(target: LOG_TARGET, "Missed {} from Transaction events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {}
                        }
                    },
                    Ok(_) = connectivity_status.changed() => {
                        trace!(target: LOG_TARGET, "Wallet Event Monitor received wallet connectivity status changed");
                        self.trigger_peer_state_refresh().await;
                    },
                    Ok(_) = software_update_notif.changed() => {
                        trace!(target: LOG_TARGET, "Wallet Event Monitor received wallet auto update status changed");
                        let update = software_update_notif.borrow().as_ref().cloned();
                        if let Some(update) = update {
                            self.add_notification(format!(
                                "Version {} of the {} is available: {} (sha: {})",
                                update.version(),
                                update.app(),
                                update.download_url(),
                                update.to_hash_hex()
                            )).await;
                        }
                    },
                    result = connectivity_events.recv() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Wallet Event Monitor received wallet connectivity event {:?}", msg);
                                match msg {
                                    ConnectivityEvent::PeerConnected(_) |
                                    ConnectivityEvent::PeerDisconnected(_) => {
                                        self.trigger_peer_state_refresh().await;
                                    },
                                    // Only the above variants trigger state refresh
                                    _ => (),
                                }
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(target: LOG_TARGET, "Missed {} from Connectivity events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {}
                        }
                    },
                    _ = base_node_changed.changed() => {
                        let peer = base_node_changed.borrow().as_ref().cloned();
                        if let Some(peer) = peer {
                            self.trigger_base_node_peer_refresh(peer).await;
                            self.trigger_balance_refresh();
                        }
                    }
                    result = base_node_events.recv() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Wallet Event Monitor received base node event {:?}", msg);
                                if let BaseNodeEvent::BaseNodeStateChanged(state) = (*msg).clone() {
                                        self.trigger_base_node_state_refresh(state).await;
                                }
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(target: LOG_TARGET, "Missed {} from Base node Service events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {}
                        }
                    },
                    result = output_manager_service_events.recv() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Output Manager Service Callback Handler event {:?}", msg);
                                if let OutputManagerEvent::TxoValidationSuccess(_) = &*msg {
                                    self.trigger_balance_refresh();
                                }
                            },
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!(target: LOG_TARGET, "Missed {} from Output Manager Service events", n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {}
                        }
                    },
                    _ = shutdown_signal.wait() => {
                        info!(target: LOG_TARGET, "Wallet Event Monitor shutting down because the shutdown signal was received");
                        break;
                    },
            }
        }
    }

    async fn trigger_tx_state_refresh(&mut self, tx_id: TxId) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_single_transaction_state(tx_id).await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    async fn trigger_confirmations_refresh(&mut self, tx_id: TxId, confirmations: u64) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_single_confirmation_state(tx_id, confirmations).await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    async fn trigger_confirmations_cleanup(&mut self, tx_id: TxId) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.cleanup_single_confirmation_state(tx_id).await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    async fn trigger_full_tx_state_refresh(&mut self) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_full_transaction_state().await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    async fn trigger_peer_state_refresh(&mut self) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_connected_peers_state().await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    async fn trigger_base_node_state_refresh(&mut self, state: BaseNodeState) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_base_node_state(state).await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }

        if inner.has_time_locked_balance() {
            if let Err(e) = self.balance_enquiry_debounce_tx.send(()) {
                warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
            }
        }
    }

    async fn trigger_base_node_peer_refresh(&mut self, peer: Peer) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_base_node_peer(peer).await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    fn trigger_balance_refresh(&mut self) {
        if let Err(e) = self.balance_enquiry_debounce_tx.send(()) {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }

    async fn add_notification(&mut self, notification: String) {
        let mut inner = self.app_state_inner.write().await;
        inner.add_notification(notification);
    }
}
