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

//! # Wallet Callback Handler
//! This CallbackHandler will monitor event streams from the various wallet services and on relevant events will call
//! the assigned callbacks to provide asynchronous feedback to the client application that an event has occured
//!
//! ## Callbacks  
//! `callback_received_transaction` - This will be called when an inbound transaction is received from an external
//! wallet
//!
//! `callback_received_transaction_reply` - This will be called when a reply is received for a pending outbound
//! transaction that is waiting to be negotiated
//!
//! `callback_received_finalized_transaction` - This will be called when a Finalized version on an Inbound transaction
//! is received from the Sender of a transaction
//!
//! `callback_transaction_broadcast` - This will be  called when a Finalized transaction is detected a Broadcast to a
//! base node mempool.
//!
//! `callback_transaction_mined` - This will be called when a Broadcast transaction is detected as mined via a base
//! node request
//!
//! `callback_discovery_process_complete` - This will be called when a `send_transacion(..)` call is made to a peer
//! whose address is not known and a discovery process must be conducted. The outcome of the discovery process is
//! relayed via this callback
//!
//! `callback_base_node_sync_complete` - This is called when a Base Node Sync process is completed or times out. The
//! request_key is used to identify which request this callback references and a result of true means it was successful
//! and false that the process timed out and new one will be started

use futures::{stream::Fuse, StreamExt};
use log::*;
use std::os::raw::c_void;
use tari_broadcast_channel::Subscriber;
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::{handle::OutputManagerEvent, TxId},
    transaction_service::{
        handle::{TransactionEvent, TransactionEventReceiver},
        storage::database::{CompletedTransaction, InboundTransaction, TransactionBackend, TransactionDatabase},
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::callback_handler";

pub struct CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
    callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_direct_send_result: unsafe extern "C" fn(TxId, bool),
    callback_store_and_forward_send_result: unsafe extern "C" fn(TxId, bool),
    callback_transaction_cancellation: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_base_node_sync_complete: unsafe extern "C" fn(TxId, bool),
    callback_comms_shutdown_finished: unsafe extern "C" fn(),
    db: TransactionDatabase<TBackend>,
    transaction_service_event_stream: Fuse<TransactionEventReceiver>,
    output_manager_service_event_stream: Fuse<Subscriber<OutputManagerEvent>>,
    shutdown_signal: Option<ShutdownSignal>,
}

#[allow(clippy::too_many_arguments)]
impl<TBackend> CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    pub fn new(
        db: TransactionDatabase<TBackend>,
        transaction_service_event_stream: Fuse<TransactionEventReceiver>,
        output_manager_service_event_stream: Fuse<Subscriber<OutputManagerEvent>>,
        shutdown_signal: ShutdownSignal,
        callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
        callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_direct_send_result: unsafe extern "C" fn(TxId, bool),
        callback_store_and_forward_send_result: unsafe extern "C" fn(TxId, bool),
        callback_transaction_cancellation: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_base_node_sync_complete: unsafe extern "C" fn(u64, bool),
        callback_comms_shutdown_finished: unsafe extern "C" fn(),
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
            "DirectSendResultCallback -> Assigning Fn:  {:?}", callback_direct_send_result
        );
        info!(
            target: LOG_TARGET,
            "StoreAndForwardSendResultCallback -> Assigning Fn:  {:?}", callback_store_and_forward_send_result
        );
        info!(
            target: LOG_TARGET,
            "TransactionCancellationCallback -> Assigning Fn:  {:?}", callback_transaction_cancellation
        );
        info!(
            target: LOG_TARGET,
            "BaseNodeSyncCompleteCallback -> Assigning Fn:  {:?}", callback_base_node_sync_complete
        );
        info!(
            target: LOG_TARGET,
            "CommsShutdownCallback -> Assigning Fn:  {:?}", callback_comms_shutdown_finished
        );

        Self {
            callback_received_transaction,
            callback_received_transaction_reply,
            callback_received_finalized_transaction,
            callback_transaction_broadcast,
            callback_transaction_mined,
            callback_direct_send_result,
            callback_store_and_forward_send_result,
            callback_transaction_cancellation,
            callback_base_node_sync_complete,
            callback_comms_shutdown_finished,
            db,
            transaction_service_event_stream,
            output_manager_service_event_stream,
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
                result = self.transaction_service_event_stream.select_next_some() => {
                    match result {
                        Ok(msg) => {
                            trace!(target: LOG_TARGET, "Transaction Service Callback Handler event {:?}", msg);
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
                                TransactionEvent::TransactionDirectSendResult(tx_id, result) => {
                                    self.receive_direct_send_result(tx_id, result);
                                },
                                TransactionEvent::TransactionStoreForwardSendResult(tx_id, result) => {
                                    self.receive_store_and_forward_send_result(tx_id, result);
                                },
                                 TransactionEvent::TransactionCancelled(tx_id) => {
                                    self.receive_transaction_cancellation(tx_id).await;
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
                        Err(e) => error!(target: LOG_TARGET, "Error reading from Transaction Service event broadcast channel"),
                    }
                },
                msg = self.output_manager_service_event_stream.select_next_some() => {
                    trace!(target: LOG_TARGET, "Output Manager Service Callback Handler event {:?}", msg);
                    match (*msg).clone() {
                        OutputManagerEvent::ReceiveBaseNodeResponse(request_key) => {
                            self.receive_sync_process_result(request_key, true);
                        },
                        OutputManagerEvent::BaseNodeSyncRequestTimedOut(request_key) => {
                            self.receive_sync_process_result(request_key, false);
                        }
                        /// Only the above variants are mapped to callbacks
                        _ => (),
                    }
                },
                complete => {
                    info!(target: LOG_TARGET, "Callback Handler is exiting because all tasks have completed");
                    break;
                },
                 _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "Transaction Callback Handler shutting down because the shutdown signal was received");
                    self.callback_comms_shutdown_finished();
                    break;
                },
            }
        }
    }

    async fn receive_transaction_event(&mut self, tx_id: TxId) {
        match self.db.get_pending_inbound_transaction(tx_id).await {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction callback function for TxId: {}", tx_id
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
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Reply callback function for TxId: {}", tx_id
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
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Finalized Transaction callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_received_finalized_transaction)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn receive_direct_send_result(&mut self, tx_id: TxId, result: bool) {
        debug!(
            target: LOG_TARGET,
            "Calling Direct Send Result callback function for TxId: {} with result {}", tx_id, result
        );
        unsafe {
            (self.callback_direct_send_result)(tx_id, result);
        }
    }

    fn receive_store_and_forward_send_result(&mut self, tx_id: TxId, result: bool) {
        debug!(
            target: LOG_TARGET,
            "Calling Store and Forward Send Result callback function for TxId: {} with result {}", tx_id, result
        );
        unsafe {
            (self.callback_store_and_forward_send_result)(tx_id, result);
        }
    }

    async fn receive_transaction_cancellation(&mut self, tx_id: TxId) {
        let mut transaction = None;
        if let Ok(tx) = self.db.get_cancelled_completed_transaction(tx_id).await {
            transaction = Some(tx);
        } else {
            if let Ok(tx) = self.db.get_cancelled_pending_outbound_transaction(tx_id).await {
                transaction = Some(CompletedTransaction::from(tx));
            } else {
                if let Ok(tx) = self.db.get_cancelled_pending_inbound_transaction(tx_id).await {
                    transaction = Some(CompletedTransaction::from(tx));
                }
            }
        }

        match transaction {
            None => error!(
                target: LOG_TARGET,
                "Error retrieving Cancelled Transaction TxId {}", tx_id
            ),
            Some(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Transaction Cancellation callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_transaction_cancellation)(boxing);
                }
            },
        }
    }

    async fn receive_transaction_broadcast_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id).await {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Broadcast callback function for TxId: {}", tx_id
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
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Mined callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_transaction_mined)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn receive_sync_process_result(&mut self, request_key: u64, result: bool) {
        debug!(
            target: LOG_TARGET,
            "Calling Base Node Sync result callback function for Request Key: {} with result {}", request_key, result
        );
        unsafe {
            (self.callback_base_node_sync_complete)(request_key, result);
        }
    }

    fn callback_comms_shutdown_finished(&mut self) {
        debug!(target: LOG_TARGET, "Calling Comms Shutdown Finished function",);
        unsafe {
            (self.callback_comms_shutdown_finished)();
        }
    }
}
