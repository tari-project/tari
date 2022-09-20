// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

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
//! `callback_faux_transaction_confirmed` - This will be called when an imported output, recovered output or one-sided
//!  transaction is detected as mined
//!
//! `callback_faux_transaction_unconfirmed` - This will be called when a recovered output or one-sided transaction is
//! freshly imported or when an imported transaction transitions from Imported to FauxUnconfirmed
//!
//! `callback_discovery_process_complete` - This will be called when a `send_transacion(..)` call is made to a peer
//! whose address is not known and a discovery process must be conducted. The outcome of the discovery process is
//! relayed via this callback
//!
//! `callback_base_node_sync_complete` - This is called when a Base Node Sync process is completed or times out. The
//! request_key is used to identify which request this callback references and a result of true means it was successful
//! and false that the process timed out and new one will be started

use std::{ops::Deref, sync::Arc};

use log::*;
use tari_common_types::transaction::TxId;
use tari_comms::types::CommsPublicKey;
use tari_comms_dht::event::{DhtEvent, DhtEventReceiver};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    connectivity_service::OnlineStatus,
    contacts_service::handle::{ContactsLivenessData, ContactsLivenessEvent},
    output_manager_service::{
        handle::{OutputManagerEvent, OutputManagerEventReceiver, OutputManagerHandle},
        service::Balance,
    },
    transaction_service::{
        handle::{TransactionEvent, TransactionEventReceiver, TransactionSendStatus},
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            models::{CompletedTransaction, InboundTransaction},
        },
    },
};
use tokio::sync::{broadcast, watch};

const LOG_TARGET: &str = "wallet::transaction_service::callback_handler";

pub struct CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
    callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut CompletedTransaction, u64),
    callback_faux_transaction_confirmed: unsafe extern "C" fn(*mut CompletedTransaction),
    callback_faux_transaction_unconfirmed: unsafe extern "C" fn(*mut CompletedTransaction, u64),
    callback_transaction_send_result: unsafe extern "C" fn(u64, *mut TransactionSendStatus),
    callback_transaction_cancellation: unsafe extern "C" fn(*mut CompletedTransaction, u64),
    callback_txo_validation_complete: unsafe extern "C" fn(u64, u64),
    callback_contacts_liveness_data_updated: unsafe extern "C" fn(*mut ContactsLivenessData),
    callback_balance_updated: unsafe extern "C" fn(*mut Balance),
    callback_transaction_validation_complete: unsafe extern "C" fn(u64, bool),
    callback_saf_messages_received: unsafe extern "C" fn(),
    callback_connectivity_status: unsafe extern "C" fn(u64),
    db: TransactionDatabase<TBackend>,
    transaction_service_event_stream: TransactionEventReceiver,
    output_manager_service_event_stream: OutputManagerEventReceiver,
    output_manager_service: OutputManagerHandle,
    dht_event_stream: DhtEventReceiver,
    shutdown_signal: Option<ShutdownSignal>,
    comms_public_key: CommsPublicKey,
    balance_cache: Balance,
    connectivity_status_watch: watch::Receiver<OnlineStatus>,
    contacts_liveness_events: broadcast::Receiver<Arc<ContactsLivenessEvent>>,
}

impl<TBackend> CallbackHandler<TBackend>
where TBackend: TransactionBackend + 'static
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: TransactionDatabase<TBackend>,
        transaction_service_event_stream: TransactionEventReceiver,
        output_manager_service_event_stream: OutputManagerEventReceiver,
        output_manager_service: OutputManagerHandle,
        dht_event_stream: DhtEventReceiver,
        shutdown_signal: ShutdownSignal,
        comms_public_key: CommsPublicKey,
        connectivity_status_watch: watch::Receiver<OnlineStatus>,
        contacts_liveness_events: broadcast::Receiver<Arc<ContactsLivenessEvent>>,
        callback_received_transaction: unsafe extern "C" fn(*mut InboundTransaction),
        callback_received_transaction_reply: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_received_finalized_transaction: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_broadcast: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_mined: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_transaction_mined_unconfirmed: unsafe extern "C" fn(*mut CompletedTransaction, u64),
        callback_faux_transaction_confirmed: unsafe extern "C" fn(*mut CompletedTransaction),
        callback_faux_transaction_unconfirmed: unsafe extern "C" fn(*mut CompletedTransaction, u64),
        callback_transaction_send_result: unsafe extern "C" fn(u64, *mut TransactionSendStatus),
        callback_transaction_cancellation: unsafe extern "C" fn(*mut CompletedTransaction, u64),
        callback_txo_validation_complete: unsafe extern "C" fn(u64, u64),
        callback_contacts_liveness_data_updated: unsafe extern "C" fn(*mut ContactsLivenessData),
        callback_balance_updated: unsafe extern "C" fn(*mut Balance),
        callback_transaction_validation_complete: unsafe extern "C" fn(u64, bool),
        callback_saf_messages_received: unsafe extern "C" fn(),
        callback_connectivity_status: unsafe extern "C" fn(u64),
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
            "FauxTransactionConfirmedCallback -> Assigning Fn: {:?}", callback_faux_transaction_confirmed
        );
        info!(
            target: LOG_TARGET,
            "FauxTransactionUnconfirmedCallback -> Assigning Fn: {:?}", callback_faux_transaction_unconfirmed
        );
        info!(
            target: LOG_TARGET,
            "TransactionSendResultCallback -> Assigning Fn:  {:?}", callback_transaction_send_result
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
            "ContactsLivenessDataUpdatedCallback -> Assigning Fn:  {:?}", callback_contacts_liveness_data_updated
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
        info!(
            target: LOG_TARGET,
            "ConnectivityStatusCallback -> Assigning Fn:  {:?}", callback_connectivity_status
        );

        Self {
            callback_received_transaction,
            callback_received_transaction_reply,
            callback_received_finalized_transaction,
            callback_transaction_broadcast,
            callback_transaction_mined,
            callback_transaction_mined_unconfirmed,
            callback_faux_transaction_confirmed,
            callback_faux_transaction_unconfirmed,
            callback_transaction_send_result,
            callback_transaction_cancellation,
            callback_txo_validation_complete,
            callback_contacts_liveness_data_updated,
            callback_balance_updated,
            callback_transaction_validation_complete,
            callback_saf_messages_received,
            callback_connectivity_status,
            db,
            transaction_service_event_stream,
            output_manager_service_event_stream,
            output_manager_service,
            dht_event_stream,
            shutdown_signal: Some(shutdown_signal),
            comms_public_key,
            balance_cache: Balance::zero(),
            connectivity_status_watch,
            contacts_liveness_events,
        }
    }

    #[allow(clippy::too_many_lines)]
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
                                    self.receive_transaction_event(tx_id);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::ReceivedTransactionReply(tx_id) => {
                                    self.receive_transaction_reply_event(tx_id);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::ReceivedFinalizedTransaction(tx_id) => {
                                    self.receive_finalized_transaction_event(tx_id);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionSendResult(tx_id, status) => {
                                    self.receive_transaction_send_result(tx_id, status);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionCancelled(tx_id, reason) => {
                                    self.receive_transaction_cancellation(tx_id, reason as u64);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionBroadcast(tx_id) => {
                                    self.receive_transaction_broadcast_event(tx_id);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionMined{tx_id, is_valid: _} => {
                                    self.receive_transaction_mined_event(tx_id);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionMinedUnconfirmed{tx_id, num_confirmations, is_valid: _} => {
                                    self.receive_transaction_mined_unconfirmed_event(tx_id, num_confirmations);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::FauxTransactionConfirmed{tx_id, is_valid: _} => {
                                    self.receive_faux_transaction_confirmed_event(tx_id);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::FauxTransactionUnconfirmed{tx_id, num_confirmations, is_valid: _} => {
                                    self.receive_faux_transaction_unconfirmed_event(tx_id, num_confirmations);
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionValidationStateChanged(_request_key)  => {
                                    self.trigger_balance_refresh().await;
                                },
                                TransactionEvent::TransactionValidationCompleted(request_key)  => {
                                    self.transaction_validation_complete_event(request_key.as_u64(), true);
                                },
                                TransactionEvent::TransactionValidationFailed(request_key)  => {
                                    self.transaction_validation_complete_event(request_key.as_u64(), false);
                                },
                                TransactionEvent::TransactionMinedRequestTimedOut(_tx_id) |
                                TransactionEvent::TransactionImported(_tx_id)|
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
                                    self.output_validation_complete_event(request_key,  0);
                                    self.trigger_balance_refresh().await;
                                },
                                OutputManagerEvent::TxoValidationInternalFailure(request_key) => {
                                    self.output_validation_complete_event(request_key,  1);
                                },
                                OutputManagerEvent::TxoValidationCommunicationFailure(request_key) => {
                                    self.output_validation_complete_event(request_key,  2);
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
                Ok(_) = self.connectivity_status_watch.changed() => {
                    let status  = *self.connectivity_status_watch.borrow();
                    trace!(target: LOG_TARGET, "Connectivity status change detected: {:?}", status);
                    self.connectivity_status_changed(status);
                },
                event = self.contacts_liveness_events.recv() => {
                    match event {
                        Ok(liveness_event) => {
                            match liveness_event.deref() {
                                ContactsLivenessEvent::StatusUpdated(data) => {
                                    trace!(target: LOG_TARGET,
                                        "Contacts Liveness Service Callback Handler event 'StatusUpdated'"
                                    );
                                    self.trigger_contacts_refresh(data.deref().clone());
                                }
                                ContactsLivenessEvent::NetworkSilence => {}
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!(target: LOG_TARGET, "Missed {} from Output Manager Service events", n);
                        }
                        Err(broadcast::error::RecvError::Closed) => {}
                    }
                }
                 _ = shutdown_signal.wait() => {
                    info!(target: LOG_TARGET, "Transaction Callback Handler shutting down because the shutdown signal was received");
                    break;
                },
            }
        }
    }

    fn receive_transaction_event(&mut self, tx_id: TxId) {
        match self.db.get_pending_inbound_transaction(tx_id) {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction callback function for u64: {}", tx_id
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

    fn receive_transaction_reply_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id) {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Reply callback function for u64: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_received_transaction_reply)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn receive_finalized_transaction_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id) {
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

    fn trigger_contacts_refresh(&mut self, data: ContactsLivenessData) {
        debug!(
            target: LOG_TARGET,
            "Calling Contacts Liveness Data Updated callback function for contact {}",
            data.public_key(),
        );
        let boxing = Box::into_raw(Box::new(data));
        unsafe {
            (self.callback_contacts_liveness_data_updated)(boxing);
        }
    }

    fn receive_transaction_send_result(&mut self, tx_id: TxId, status: TransactionSendStatus) {
        debug!(
            target: LOG_TARGET,
            "Calling Transaction Send Result callback function for TxId: {} with result {}", tx_id, status
        );
        let boxing = Box::into_raw(Box::new(status));
        unsafe {
            (self.callback_transaction_send_result)(tx_id.as_u64(), boxing);
        }
    }

    fn receive_transaction_cancellation(&mut self, tx_id: TxId, reason: u64) {
        let mut transaction = None;
        if let Ok(tx) = self.db.get_cancelled_completed_transaction(tx_id) {
            transaction = Some(tx);
        } else if let Ok(tx) = self.db.get_cancelled_pending_outbound_transaction(tx_id) {
            let mut outbound_tx = CompletedTransaction::from(tx);
            outbound_tx.source_public_key = self.comms_public_key.clone();
            transaction = Some(outbound_tx);
        } else if let Ok(tx) = self.db.get_cancelled_pending_inbound_transaction(tx_id) {
            let mut inbound_tx = CompletedTransaction::from(tx);
            inbound_tx.destination_public_key = self.comms_public_key.clone();
            transaction = Some(inbound_tx);
        } else {
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
                    (self.callback_transaction_cancellation)(boxing, reason);
                }
            },
        }
    }

    fn receive_transaction_broadcast_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id) {
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

    fn receive_transaction_mined_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id) {
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

    fn receive_transaction_mined_unconfirmed_event(&mut self, tx_id: TxId, confirmations: u64) {
        match self.db.get_completed_transaction(tx_id) {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Transaction Mined Unconfirmed callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_transaction_mined_unconfirmed)(boxing, confirmations);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn receive_faux_transaction_confirmed_event(&mut self, tx_id: TxId) {
        match self.db.get_completed_transaction(tx_id) {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Faux Transaction Confirmed callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_faux_transaction_confirmed)(boxing);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn receive_faux_transaction_unconfirmed_event(&mut self, tx_id: TxId, confirmations: u64) {
        match self.db.get_completed_transaction(tx_id) {
            Ok(tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Calling Received Faux Transaction Unconfirmed callback function for TxId: {}", tx_id
                );
                let boxing = Box::into_raw(Box::new(tx));
                unsafe {
                    (self.callback_faux_transaction_unconfirmed)(boxing, confirmations);
                }
            },
            Err(e) => error!(target: LOG_TARGET, "Error retrieving Completed Transaction: {:?}", e),
        }
    }

    fn transaction_validation_complete_event(&mut self, request_key: u64, success: bool) {
        debug!(
            target: LOG_TARGET,
            "Calling Transaction Validation Complete callback function for Request Key: {}", request_key,
        );
        unsafe {
            (self.callback_transaction_validation_complete)(request_key, success);
        }
    }

    fn output_validation_complete_event(&mut self, request_key: u64, success: u64) {
        debug!(
            target: LOG_TARGET,
            "Calling Output Validation Complete callback function for Request Key: {} with success = {:?}",
            request_key,
            success,
        );

        unsafe {
            (self.callback_txo_validation_complete)(request_key, success);
        }
    }

    fn saf_messages_received_event(&mut self) {
        debug!(target: LOG_TARGET, "Calling SAF Messages Received callback function");
        unsafe {
            (self.callback_saf_messages_received)();
        }
    }

    fn connectivity_status_changed(&mut self, status: OnlineStatus) {
        debug!(
            target: LOG_TARGET,
            "Calling Connectivity Status changed callback function"
        );
        unsafe {
            (self.callback_connectivity_status)(status as u64);
        }
    }
}
