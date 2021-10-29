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

use log::*;
use tari_common_types::transaction::TxId;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::event::{DhtEvent, DhtEventReceiver};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerEventReceiver, OutputManagerHandle},
        service::Balance,
    },
    transaction_service::{
        handle::{TransactionEvent, TransactionEventReceiver},
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::{CompletedTransaction, InboundTransaction},
        },
    },
};

const LOG_TARGET: &str = "wallet::transaction_service::callback_handler";

#[derive(Clone, Copy)]
enum CallbackValidationResults {
    Success,           // 0
    Aborted,           // 1
    Failure,           // 2
    BaseNodeNotInSync, // 3
}

pub struct CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
    callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut CompletedTransaction, u64),
    callback_direct_send_result: unsafe extern "C" fn(TxId, bool),
    callback_store_and_forward_send_result: unsafe extern "C" fn(TxId, bool),
    callback_transaction_cancellation: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_txo_validation_complete: unsafe extern "C" fn(u64, u8),
    callback_balance_updated: unsafe extern "C" fn(*mut Balance),
    callback_transaction_validation_complete: unsafe extern "C" fn(u64, u8),
    callback_saf_messages_received: unsafe extern "C" fn(),
    db: TransactionDatabase<TBackend>,
    transaction_service_event_stream: TransactionEventReceiver,
    output_manager_service_event_stream: OutputManagerEventReceiver,
    output_manager_service: OutputManagerHandle,
    dht_event_stream: DhtEventReceiver,
    shutdown_signal: Option<ShutdownSignal>,
    comms_public_key: CommsPublicKey,
    balance_cache: Balance,
}

#[allow(clippy::too_many_arguments)]
impl<TBackend> CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    pub fn new(
        db: TransactionDatabase<TBackend>,
        transaction_service_event_stream: TransactionEventReceiver,
        output_manager_service_event_stream: OutputManagerEventReceiver,
        output_manager_service: OutputManagerHandle,
        dht_event_stream: DhtEventReceiver,
        shutdown_signal: ShutdownSignal,
        comms_public_key: CommsPublicKey,
        callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
        callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut CompletedTransaction, u64),
        callback_direct_send_result: unsafe extern "C" fn(TxId, bool),
        callback_store_and_forward_send_result: unsafe extern "C" fn(TxId, bool),
        callback_transaction_cancellation: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_txo_validation_complete: unsafe extern "C" fn(TxId, u8),
        callback_balance_updated: unsafe extern "C" fn(*mut Balance),
        callback_transaction_validation_complete: unsafe extern "C" fn(TxId, u8),
        callback_saf_messages_received: unsafe extern "C" fn(),
    ) -> Self {
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
            "TransactionMinedUnconfirmedCallback -> Assigning Fn: {:?}", callback_transaction_mined_unconfirmed
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
            "TxoValidationCompleteCallback -> Assigning Fn:  {:?}", callback_txo_validation_complete
        );
        info!(
            target: LOG_TARGET,
            "BalanceUpdatedCallback -> Assigning Fn:  {:?}", callback_balance_updated
        );
        info!(
            target: LOG_TARGET,
            "TransactionValidationCompleteCallback -> Assigning Fn:  {:?}", callback_transaction_validation_complete
        );
        info!(
            target: LOG_TARGET,
            "SafMessagesReceivedCallback -> Assigning Fn:  {:?}", callback_saf_messages_received
        );

        Self {
            callback_received_transaction,
            callback_received_transaction_reply,
            callback_received_finalized_transaction,
            callback_transaction_broadcast,
            callback_transaction_mined,
            callback_transaction_mined_unconfirmed,
            callback_direct_send_result,
            callback_store_and_forward_send_result,
            callback_transaction_cancellation,
            callback_txo_validation_complete,
            callback_balance_updated,
            callback_transaction_validation_complete,
            callback_saf_messages_received,
            db,
            transaction_service_event_stream,
            output_manager_service_event_stream,
            output_manager_service,
            dht_event_stream,
            shutdown_signal: Some(shutdown_signal),
            comms_public_key,
            balance_cache: Balance::zero(),
        }
    }

    pub async fn start(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Callback Handler started without shutdown signal");

        info!(target: LOG_TARGET, "Transaction Service Callback Handler starting");

        loop {
            tokio::select! {
                result = self.transaction_service_event_stream.recv() => {
                    match result {
                        Ok(msg) => {
                            trace!(target: LOG_TARGET, "Transaction Service Callback Handler event {:?}", msg);
                            match (*msg).clone() {
                                TransactionEvent::ReceivedTransaction(tx_id) => {
                                    self.receive_transaction_event(tx_id).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::ReceivedTransactionReply(tx_id) => {
                                    self.receive_transaction_reply_event(tx_id).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::ReceivedFinalizedTransaction(tx_id) => {
                                    self.receive_finalized_transaction_event(tx_id).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionDirectSendResult(tx_id, result) => {
                                    self.receive_direct_send_result(tx_id, result);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionStoreForwardSendResult(tx_id, result) => {
                                    self.receive_store_and_forward_send_result(tx_id, result);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionCancelled(tx_id) => {
                                    self.receive_transaction_cancellation(tx_id).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionBroadcast(tx_id) => {
                                    self.receive_transaction_broadcast_event(tx_id).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionMined{tx_id, is_valid: _} => {
                                    self.receive_transaction_mined_event(tx_id).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionMinedUnconfirmed{tx_id, num_confirmations, is_valid: _} => {
                                    self.receive_transaction_mined_unconfirmed_event(tx_id, num_confirmations).await;
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionValidationSuccess(tx_id)  => {
                                    self.transaction_validation_complete_event(tx_id, CallbackValidationResults::Success);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionValidationFailure(tx_id)  => {
                                    self.transaction_validation_complete_event(tx_id, CallbackValidationResults::Failure);
                                },
                                TransactionEvent::TransactionValidationAborted(tx_id)  => {
                                    self.transaction_validation_complete_event(tx_id, CallbackValidationResults::Aborted);
                                },
                                TransactionEvent::TransactionValidationDelayed(tx_id)  => {
                                    self.transaction_validation_complete_event(tx_id, CallbackValidationResults::BaseNodeNotInSync);
                                },
                                TransactionEvent::TransactionMinedRequestTimedOut(_tx_id) |
                                TransactionEvent::TransactionImported(_tx_id) |
                                TransactionEvent::TransactionCompletedImmediately(_tx_id)
                                => {
                                    self.trigger_balance_refresh().await;
                                },
                                // Only the above variants are mapped to callbacks
                                _ => (),
                            }
                        },
                        Err(_e) => error!(target: LOG_TARGET, "Error reading from Transaction Service event broadcast channel"),
                    }
                },
                result = self.output_manager_service_event_stream.recv() => {
                    match result {
                        Ok(msg) => {
                            trace!(target: LOG_TARGET, "Output Manager Service Callback Handler event {:?}", msg);
                            match (*msg).clone() {
                                OutputManagerEvent::TxoValidationSuccess(request_key) => {
                                    self.output_validation_complete_event(request_key,  CallbackValidationResults::Success);
                                    self.trigger_balance_refresh().await;
                                },
                                OutputManagerEvent::TxoValidationFailure(request_key) => {
                                    self.output_validation_complete_event(request_key,  CallbackValidationResults::Failure);
                                },
                                OutputManagerEvent::TxoValidationAborted(request_key) => {
                                    self.output_validation_complete_event(request_key,  CallbackValidationResults::Aborted);
                                },
                                OutputManagerEvent::TxoValidationDelayed(request_key) => {
                                    self.output_validation_complete_event(request_key,  CallbackValidationResults::BaseNodeNotInSync);
                                },
                                // Only the above variants are mapped to callbacks
                                _ => (),
                            }
                        },
                        Err(_e) => error!(target: LOG_TARGET, "Error reading from Output Manager Service event broadcast channel"),
                    }
                },
                result = self.dht_event_stream.recv() => {
                    match result {
                        Ok(msg) => {
                            trace!(target: LOG_TARGET, "DHT Callback Handler event {:?}", msg);
                            if let DhtEvent::StoreAndForwardMessagesReceived = *msg {
                                self.saf_messages_received_event();
                            }
                        },
                        Err(_e) => error!(target: LOG_TARGET, "Error reading from DHT event broadcast channel"),
                    }
                }
                 _ = shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "Transaction Callback Handler shutting down because the shutdown signal was received");
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

    async fn trigger_balance_refresh(&mut self) {
        match self.output_manager_service.get_balance().await {
            Ok(balance) => {
                if balance != self.balance_cache {
                    self.balance_cache = balance.clone();
                    debug!(
                        target: LOG_TARGET,
                        "Calling Update Balance callback function: available {}, time locked {:?}, incoming {}, \
                         outgoing {}",
                        balance.available_balance,
                        balance.time_locked_balance,
                        balance.pending_incoming_balance,
                        balance.pending_outgoing_balance
                    );
                    let boxing = Box::into_raw(Box::new(balance));
                    unsafe {
                        (self.callback_balance_updated)(boxing);
                    }
                }
            },
            Err(e) => {
                error!(target: LOG_TARGET, "Could not obtain balance ({:?})", e);
            },
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
        } else if let Ok(tx) = self.db.get_cancelled_pending_outbound_transaction(tx_id).await {
            let mut outbound_tx = CompletedTransaction::from(tx);
            outbound_tx.source_public_key = self.comms_public_key.clone();
            transaction = Some(outbound_tx);
        } else if let Ok(tx) = self.db.get_cancelled_pending_inbound_transaction(tx_id).await {
            let mut inbound_tx = CompletedTransaction::from(tx);
            inbound_tx.destination_public_key = self.comms_public_key.clone();
            transaction = Some(inbound_tx);
        };

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

    async fn receive_transaction_mined_unconfirmed_event(&mut self, tx_id: TxId, confirmations: u64) {
        match self.db.get_completed_transaction(tx_id).await {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Mined callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_transaction_mined_unconfirmed)(boxing, confirmations);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn transaction_validation_complete_event(&mut self, request_key: u64, result: CallbackValidationResults) {
        debug!(
            target: LOG_TARGET,
            "Calling Transaction Validation Complete callback function for Request Key: {} with result {:?}",
            request_key,
            result as u8,
        );
        match result {
            CallbackValidationResults::Success => unsafe {
                (self.callback_transaction_validation_complete)(request_key, CallbackValidationResults::Success as u8);
            },
            CallbackValidationResults::Aborted => unsafe {
                (self.callback_transaction_validation_complete)(request_key, CallbackValidationResults::Aborted as u8);
            },
            CallbackValidationResults::Failure => unsafe {
                (self.callback_transaction_validation_complete)(request_key, CallbackValidationResults::Failure as u8);
            },
            CallbackValidationResults::BaseNodeNotInSync => unsafe {
                (self.callback_transaction_validation_complete)(
                    request_key,
                    CallbackValidationResults::BaseNodeNotInSync as u8,
                );
            },
        }
    }

    fn output_validation_complete_event(&mut self, request_key: u64, result: CallbackValidationResults) {
        debug!(
            target: LOG_TARGET,
            "Calling Output Validation Complete callback function for Request Key: {} with result {:?}",
            request_key,
            result as u8,
        );

        match result {
            CallbackValidationResults::Success => unsafe {
                (self.callback_txo_validation_complete)(request_key, CallbackValidationResults::Success as u8);
            },
            CallbackValidationResults::Aborted => unsafe {
                (self.callback_txo_validation_complete)(request_key, CallbackValidationResults::Aborted as u8);
            },
            CallbackValidationResults::Failure => unsafe {
                (self.callback_txo_validation_complete)(request_key, CallbackValidationResults::Failure as u8);
            },
            CallbackValidationResults::BaseNodeNotInSync => unsafe {
                (self.callback_txo_validation_complete)(
                    request_key,
                    CallbackValidationResults::BaseNodeNotInSync as u8,
                );
            },
        }
    }

    fn saf_messages_received_event(&mut self) {
        debug!(target: LOG_TARGET, "Calling SAF Messages Received callback function");
        unsafe {
            (self.callback_saf_messages_received)();
        }
    }
}
