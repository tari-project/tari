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

use crate::ui::state::AppStateInner;
use futures::stream::StreamExt;
use log::*;
use std::sync::Arc;
use tari_comms::{connectivity::ConnectivityEvent, peer_manager::Peer};
use tari_wallet::{
    base_node_service::{handle::BaseNodeEvent, service::BaseNodeState},
    output_manager_service::TxId,
    transaction_service::handle::TransactionEvent,
};
use tokio::sync::RwLock;

const LOG_TARGET: &str = "wallet::console_wallet::wallet_event_monitor";

pub struct WalletEventMonitor {
    app_state_inner: Arc<RwLock<AppStateInner>>,
}

impl WalletEventMonitor {
    pub fn new(app_state_inner: Arc<RwLock<AppStateInner>>) -> Self {
        Self { app_state_inner }
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self.app_state_inner.read().await.get_shutdown_signal();

        let mut transaction_service_events = self.app_state_inner.read().await.get_transaction_service_event_stream();

        let mut connectivity_events = self.app_state_inner.read().await.get_connectivity_event_stream();

        let mut base_node_events = self.app_state_inner.read().await.get_base_node_event_stream();

        info!(target: LOG_TARGET, "Wallet Event Monitor starting");
        loop {
            futures::select! {
                    result = transaction_service_events.select_next_some() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Wallet Event Monitor received wallet event {:?}", msg);
                                match (*msg).clone() {
                                    TransactionEvent::ReceivedTransaction(tx_id) |
                                    TransactionEvent::ReceivedTransactionReply(tx_id) |
                                    TransactionEvent::ReceivedFinalizedTransaction(tx_id) |
                                    TransactionEvent::TransactionCancelled(tx_id) |
                                    TransactionEvent::TransactionBroadcast(tx_id) |
                                    TransactionEvent::TransactionMined(tx_id) |
                                    TransactionEvent::TransactionMinedRequestTimedOut(tx_id) => {
                                        self.trigger_tx_state_refresh(tx_id).await;
                                    },
                                    TransactionEvent::TransactionDirectSendResult(tx_id, success) |
                                    TransactionEvent::TransactionStoreForwardSendResult(tx_id, success) => {
                                        if success {
                                            self.trigger_tx_state_refresh(tx_id).await;
                                        }
                                    },
                                    /// Only the above variants trigger state refresh
                                    _ => (),
                                }
                            },
                            Err(_) => debug!(target: LOG_TARGET, "Lagging read on Transaction Service event broadcast channel"),
                        }
                    },
                    result = connectivity_events.select_next_some() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Wallet Event Monitor received wallet event {:?}", msg);
                                match (*msg).clone() {
                                    ConnectivityEvent::PeerDisconnected(_) |
                                    ConnectivityEvent::ManagedPeerDisconnected(_) |
                                    ConnectivityEvent::PeerConnected(_) |
                                    ConnectivityEvent::PeerBanned(_) |
                                    ConnectivityEvent::PeerOffline(_) |
                                    ConnectivityEvent::PeerConnectionWillClose(_, _) => {
                                    self.trigger_peer_state_refresh().await;
                                    },
                                    /// Only the above variants trigger state refresh
                                    _ => (),
                                }
                            },
                            Err(_) => debug!(target: LOG_TARGET, "Lagging read on Connectivity event broadcast channel"),
                        }
                    },
                    result = base_node_events.select_next_some() => {
                        match result {
                            Ok(msg) => {
                                trace!(target: LOG_TARGET, "Wallet Event Monitor received base node event {:?}", msg);
                                match (*msg).clone() {
                                    BaseNodeEvent::BaseNodeState(state) => {
                                        self.trigger_base_node_state_refresh(state).await;
                                    }
                                    BaseNodeEvent::BaseNodePeerSet(peer) => {
                                        self.trigger_base_node_peer_refresh(*peer).await;
                                    }
                                }
                            },
                            Err(_) => debug!(target: LOG_TARGET, "Lagging read on base node event broadcast channel"),
                        }
                    },
                    complete => {
                        info!(target: LOG_TARGET, "Wallet Event Monitor is exiting because all tasks have completed");
                        break;
                    },
                     _ = shutdown_signal => {
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
    }

    async fn trigger_base_node_peer_refresh(&mut self, peer: Peer) {
        let mut inner = self.app_state_inner.write().await;

        if let Err(e) = inner.refresh_base_node_peer(peer).await {
            warn!(target: LOG_TARGET, "Error refresh app_state: {}", e);
        }
    }
}
