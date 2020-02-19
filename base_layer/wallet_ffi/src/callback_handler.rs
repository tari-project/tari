// Copyright 2019. The Tari Project
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

use futures::{stream::Fuse, StreamExt};
use log::*;
use tari_broadcast_channel::Subscriber;
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::TxId,
    transaction_service::{
        handle::TransactionEvent,
        storage::database::{CompletedTransaction, InboundTransaction, TransactionBackend, TransactionDatabase},
    },
};

const LOG_TARGET: &str = "base_layer::wallet::transaction_service::callback_handler";

pub struct CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
    callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_discovery_process_complete: unsafe extern "C" fn(TxId, bool),
    db: TransactionDatabase<TBackend>,
    event_stream: Fuse<Subscriber<TransactionEvent>>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl<TBackend> CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    pub fn new(
        db: TransactionDatabase<TBackend>,
        event_stream: Fuse<Subscriber<TransactionEvent>>,
        shutdown_signal: ShutdownSignal,
        callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
        callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_discovery_process_complete: unsafe extern "C" fn(TxId, bool),
    ) -> Self
    {
        info!(
            target: LOG_TARGET,
            "ReceivedTransactionCallback -> Assigning Fn: {:?}", callback_received_transaction
        );
        info!(
            target: LOG_TARGET,
            "ReceivedTransactionReplyCallback -> Assigning Fn: {:?}", callback_received_transaction_reply
        );
        info!(
            target: LOG_TARGET,
            "ReceivedFinalizedTransactionCallback -> Assigning Fn: {:?}", callback_received_finalized_transaction
        );
        info!(
            target: LOG_TARGET,
            "TransactionBroadcastCallback -> Assigning Fn: {:?}", callback_transaction_broadcast
        );
        info!(
            target: LOG_TARGET,
            "TransactionMinedCallback -> Assigning Fn: {:?}", callback_transaction_mined
        );
        info!(
            target: LOG_TARGET,
            "DiscoveryProcessCompleteCallback -> Assigning Fn:  {:?}", callback_discovery_process_complete
        );

        Self {
            callback_received_transaction,
            callback_received_transaction_reply,
            callback_received_finalized_transaction,
            callback_transaction_broadcast,
            callback_transaction_mined,
            callback_discovery_process_complete,
            db,
            event_stream,
            shutdown_signal: Some(shutdown_signal),
        }
    }

    pub async fn start(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Callback Handler started without shutdown signal");

        info!(target: LOG_TARGET, "Transaction Service Callback Handler starting");

        loop {
            futures::select! {
                msg = self.event_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Callback Handler  {:?}", msg);
                    match (*msg).clone() {
                        TransactionEvent::ReceivedTransaction(tx_id) => {
                            self.receive_transaction_event(tx_id).await;
                        },
                        TransactionEvent::ReceivedTransactionReply(tx_id) => {
                            self.receive_transaction_reply_event(tx_id).await;
                        },
                        TransactionEvent::ReceivedFinalizedTransaction(tx_id) => {
                            self.receive_finalized_transaction_event(tx_id).await;
                        },
                        TransactionEvent::TransactionSendDiscoveryComplete(tx_id, result) => {
                            self.receive_discovery_process_result(tx_id, result);
                        },
                        TransactionEvent::TransactionBroadcast(tx_id) => {
                            self.receive_transaction_broadcast_event(tx_id).await;
                        },
                        TransactionEvent::TransactionMined(tx_id) => {
                            self.receive_transaction_mined_event(tx_id).await;
                        },
                        /// Only the above variants are mapped to callbacks
                        _ => (),
                    }
                },
                complete => {
                    info!(target: LOG_TARGET, "Callback Handler is exiting because all tasks have completed");
                    break;
                }
                 _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Transaction Callback Handler shutting down because the shutdown signal was received");
                    break;
                },
            }
        }
    }

    async fn receive_transaction_event(&mut self, tx_id: TxId) {
        match self.db.get_pending_inbound_transaction(tx_id).await {
            Ok(tx) => {
                trace!(
                    target: LOG_TARGET,
                    "Calling Received Transaction callback function for TxId: {}",
                    tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_received_transaction)(boxing);
                }
            },
            Err(e) => error!(
                target: LOG_TARGET,
                "Error retrieving Pending Inbound Transaction: {:?}", e
            ),
        }
    }

    async fn receive_transaction_reply_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id).await {
            Ok(tx) => {
                trace!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Reply callback function for TxId: {}",
                    tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_received_transaction_reply)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    async fn receive_finalized_transaction_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id).await {
            Ok(tx) => {
                trace!(
                    target: LOG_TARGET,
                    "Calling Received Finalized Transaction callback function for TxId: {}",
                    tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_received_finalized_transaction)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn receive_discovery_process_result(&mut self, tx_id: TxId, result: bool) {
        trace!(
            target: LOG_TARGET,
            "Calling Discovery Process Completed callback function for TxId: {} with result {}",
            tx_id,
            result
        );
        unsafe {
            (self.callback_discovery_process_complete)(tx_id, result);
        }
    }

    async fn receive_transaction_broadcast_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id).await {
            Ok(tx) => {
                trace!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Broadcast callback function for TxId: {}",
                    tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_transaction_broadcast)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    async fn receive_transaction_mined_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id).await {
            Ok(tx) => {
                trace!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Mined callback function for TxId: {}",
                    tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_transaction_mined)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }
}
