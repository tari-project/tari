// Copyright 2019. The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[cfg(test)]
mod test {
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use chrono::{NaiveDateTime, Utc};
    use rand::rngs::OsRng;
    use tari_common_types::{
        transaction::{TransactionDirection, TransactionStatus},
        types::{BlindingFactor, PrivateKey, PublicKey},
    };
    use tari_comms_dht::event::DhtEvent;
    use tari_core::transactions::{
        tari_amount::{uT, MicroTari},
        transaction_components::Transaction,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    };
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey};
    use tari_service_framework::reply_channel;
    use tari_shutdown::Shutdown;
    use tari_wallet::{
        connectivity_service::OnlineStatus,
        contacts_service::{
            handle::{ContactsLivenessData, ContactsLivenessEvent},
            service::{ContactMessageType, ContactOnlineStatus},
            storage::database::Contact,
        },
        output_manager_service::{
            handle::{OutputManagerEvent, OutputManagerHandle},
            service::Balance,
        },
        test_utils::make_wallet_database_connection,
        transaction_service::{
            handle::{TransactionEvent, TransactionSendStatus},
            storage::{
                database::TransactionDatabase,
                models::{CompletedTransaction, InboundTransaction, OutboundTransaction, TxCancellationReason},
                sqlite_db::TransactionServiceSqliteDatabase,
            },
        },
    };
    use tokio::{
        runtime::Runtime,
        sync::{broadcast, watch},
        time::Instant,
    };

    use crate::{callback_handler::CallbackHandler, output_manager_service_mock::MockOutputManagerService};

    #[derive(Debug)]
    #[allow(clippy::struct_excessive_bools)]
    struct CallbackState {
        pub received_tx_callback_called: bool,
        pub received_tx_reply_callback_called: bool,
        pub received_finalized_tx_callback_called: bool,
        pub broadcast_tx_callback_called: bool,
        pub mined_tx_callback_called: bool,
        pub mined_tx_unconfirmed_callback_called: u64,
        pub faux_tx_confirmed_callback_called: bool,
        pub faux_tx_unconfirmed_callback_called: u64,
        pub direct_send_callback_called: u32,
        pub store_and_forward_send_callback_called: u32,
        pub transaction_queued_for_retry_callback_called: u32,
        pub tx_cancellation_callback_called_completed: bool,
        pub tx_cancellation_callback_called_inbound: bool,
        pub tx_cancellation_callback_called_outbound: bool,
        pub callback_txo_validation_completed: bool,
        pub callback_txo_validation_communication_failure: bool,
        pub callback_txo_validation_internal_failure: bool,
        pub callback_txo_validation_already_busy: bool,
        pub callback_contacts_liveness_data_updated: u32,
        pub callback_balance_updated: u32,
        pub callback_transaction_validation_complete: u32,
        pub saf_messages_received: bool,
        pub connectivity_status_callback_called: u64,
    }

    impl CallbackState {
        fn new() -> Self {
            Self {
                received_tx_callback_called: false,
                received_tx_reply_callback_called: false,
                received_finalized_tx_callback_called: false,
                broadcast_tx_callback_called: false,
                mined_tx_callback_called: false,
                mined_tx_unconfirmed_callback_called: 0,
                faux_tx_confirmed_callback_called: false,
                faux_tx_unconfirmed_callback_called: 0,
                direct_send_callback_called: 0,
                store_and_forward_send_callback_called: 0,
                transaction_queued_for_retry_callback_called: 0,
                callback_txo_validation_completed: false,
                callback_txo_validation_communication_failure: false,
                callback_txo_validation_internal_failure: false,
                callback_txo_validation_already_busy: false,
                callback_contacts_liveness_data_updated: 0,
                callback_balance_updated: 0,
                callback_transaction_validation_complete: 0,
                tx_cancellation_callback_called_completed: false,
                tx_cancellation_callback_called_inbound: false,
                tx_cancellation_callback_called_outbound: false,
                saf_messages_received: false,
                connectivity_status_callback_called: 0,
            }
        }
    }

    lazy_static! {
        static ref CALLBACK_STATE: Mutex<CallbackState> = Mutex::new(CallbackState::new());
    }

    unsafe extern "C" fn received_tx_callback(tx: *mut InboundTransaction) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.received_tx_callback_called = true;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn received_tx_reply_callback(tx: *mut CompletedTransaction) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.received_tx_reply_callback_called = true;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn received_tx_finalized_callback(tx: *mut CompletedTransaction) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.received_finalized_tx_callback_called = true;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn broadcast_callback(tx: *mut CompletedTransaction) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.broadcast_tx_callback_called = true;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn mined_callback(tx: *mut CompletedTransaction) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.mined_tx_callback_called = true;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn mined_unconfirmed_callback(tx: *mut CompletedTransaction, confirmations: u64) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.mined_tx_unconfirmed_callback_called = confirmations;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn faux_confirmed_callback(tx: *mut CompletedTransaction) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.faux_tx_confirmed_callback_called = true;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn faux_unconfirmed_callback(tx: *mut CompletedTransaction, confirmations: u64) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.faux_tx_unconfirmed_callback_called = confirmations;
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn transaction_send_result_callback(_tx_id: u64, status: *mut TransactionSendStatus) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        if (*status).direct_send_result {
            lock.direct_send_callback_called += 1;
        };
        if (*status).store_and_forward_send_result {
            lock.store_and_forward_send_callback_called += 1;
        };
        if (*status).queued_for_retry {
            lock.transaction_queued_for_retry_callback_called += 1;
        };
        drop(lock);
    }

    unsafe extern "C" fn saf_messages_received_callback() {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.saf_messages_received = true;
        drop(lock);
    }

    unsafe extern "C" fn tx_cancellation_callback(tx: *mut CompletedTransaction, _reason: u64) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        match (*tx).tx_id.as_u64() {
            3 => lock.tx_cancellation_callback_called_inbound = true,
            4 => lock.tx_cancellation_callback_called_completed = true,
            5 => lock.tx_cancellation_callback_called_outbound = true,
            _ => (),
        }
        drop(lock);
        Box::from_raw(tx);
    }

    unsafe extern "C" fn txo_validation_complete_callback(_tx_id: u64, result: u64) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        match result {
            0 => lock.callback_txo_validation_completed = true,
            1 => lock.callback_txo_validation_already_busy = true,
            2 => lock.callback_txo_validation_communication_failure = true,
            3 => lock.callback_txo_validation_internal_failure = true,
            _ => (),
        }
        drop(lock);
    }

    unsafe extern "C" fn contacts_liveness_data_updated_callback(_data: *mut ContactsLivenessData) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.callback_contacts_liveness_data_updated += 1;
        drop(lock);
    }

    unsafe extern "C" fn balance_updated_callback(balance: *mut Balance) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.callback_balance_updated += 1;
        drop(lock);
        Box::from_raw(balance);
    }

    unsafe extern "C" fn transaction_validation_complete_callback(request_key: u64, _result: bool) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.callback_transaction_validation_complete += request_key as u32;
        drop(lock);
    }

    unsafe extern "C" fn connectivity_status_callback(status: u64) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.connectivity_status_callback_called += status + 1;
        drop(lock);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_callback_handler() {
        let runtime = Runtime::new().unwrap();

        let (connection, _tempdir) = make_wallet_database_connection(None);
        let db = TransactionDatabase::new(TransactionServiceSqliteDatabase::new(connection, None));

        let rtp = ReceiverTransactionProtocol::new_placeholder();
        let inbound_tx = InboundTransaction::new(
            1u64.into(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            22 * uT,
            rtp,
            TransactionStatus::Pending,
            "1".to_string(),
            Utc::now().naive_utc(),
        );
        db.add_pending_inbound_transaction(1u64.into(), inbound_tx.clone())
            .unwrap();

        let completed_tx = CompletedTransaction::new(
            2u64.into(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            MicroTari::from(100),
            MicroTari::from(2000),
            Transaction::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                BlindingFactor::default(),
                BlindingFactor::default(),
            ),
            TransactionStatus::Completed,
            "2".to_string(),
            Utc::now().naive_utc(),
            TransactionDirection::Inbound,
            None,
            None,
            None,
        );
        db.insert_completed_transaction(2u64.into(), completed_tx.clone())
            .unwrap();

        let stp = SenderTransactionProtocol::new_placeholder();
        let outbound_tx = OutboundTransaction::new(
            3u64.into(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            22 * uT,
            23 * uT,
            stp,
            TransactionStatus::Pending,
            "3".to_string(),
            Utc::now().naive_utc(),
            false,
        );
        db.add_pending_outbound_transaction(3u64.into(), outbound_tx.clone())
            .unwrap();
        db.cancel_pending_transaction(3u64.into()).unwrap();

        let inbound_tx_cancelled = InboundTransaction {
            tx_id: 4u64.into(),
            ..inbound_tx.clone()
        };
        db.add_pending_inbound_transaction(4u64.into(), inbound_tx_cancelled)
            .unwrap();
        db.cancel_pending_transaction(4u64.into()).unwrap();

        let completed_tx_cancelled = CompletedTransaction {
            tx_id: 5u64.into(),
            ..completed_tx.clone()
        };
        db.insert_completed_transaction(5u64.into(), completed_tx_cancelled.clone())
            .unwrap();
        db.reject_completed_transaction(5u64.into(), TxCancellationReason::Unknown)
            .unwrap();

        let faux_unconfirmed_tx = CompletedTransaction::new(
            6u64.into(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            MicroTari::from(100),
            MicroTari::from(2000),
            Transaction::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                BlindingFactor::default(),
                BlindingFactor::default(),
            ),
            TransactionStatus::FauxUnconfirmed,
            "6".to_string(),
            Utc::now().naive_utc(),
            TransactionDirection::Inbound,
            None,
            Some(2),
            Some(NaiveDateTime::from_timestamp(0, 0)),
        );
        db.insert_completed_transaction(6u64.into(), faux_unconfirmed_tx.clone())
            .unwrap();

        let faux_confirmed_tx = CompletedTransaction::new(
            7u64.into(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            MicroTari::from(100),
            MicroTari::from(2000),
            Transaction::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                BlindingFactor::default(),
                BlindingFactor::default(),
            ),
            TransactionStatus::FauxConfirmed,
            "7".to_string(),
            Utc::now().naive_utc(),
            TransactionDirection::Inbound,
            None,
            Some(5),
            Some(NaiveDateTime::from_timestamp(0, 0)),
        );
        db.insert_completed_transaction(7u64.into(), faux_confirmed_tx.clone())
            .unwrap();

        let (transaction_event_sender, transaction_event_receiver) = broadcast::channel(20);
        let (oms_event_sender, oms_event_receiver) = broadcast::channel(20);
        let (dht_event_sender, dht_event_receiver) = broadcast::channel(20);

        let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
        let mut oms_handle = OutputManagerHandle::new(oms_request_sender, oms_event_sender.clone());

        let shutdown_signal = Shutdown::new();
        let mut mock_output_manager_service =
            MockOutputManagerService::new(oms_request_receiver, shutdown_signal.to_signal());
        let mut balance = Balance {
            available_balance: completed_tx.amount +
                completed_tx.fee +
                completed_tx_cancelled.amount +
                completed_tx_cancelled.fee,
            time_locked_balance: None,
            pending_incoming_balance: inbound_tx.amount,
            pending_outgoing_balance: outbound_tx.amount + outbound_tx.fee,
        };
        let mut mock_output_manager_service_state = mock_output_manager_service.get_response_state();
        mock_output_manager_service_state.set_balance(balance.clone());
        runtime.spawn(mock_output_manager_service.run());
        assert_eq!(balance, runtime.block_on(oms_handle.get_balance()).unwrap());

        let (connectivity_tx, connectivity_rx) = watch::channel(OnlineStatus::Offline);
        let (contacts_liveness_events_sender, _) = broadcast::channel(250);
        let contacts_liveness_events = contacts_liveness_events_sender.subscribe();

        let callback_handler = CallbackHandler::new(
            db,
            transaction_event_receiver,
            oms_event_receiver,
            oms_handle,
            dht_event_receiver,
            shutdown_signal.to_signal(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            connectivity_rx,
            contacts_liveness_events,
            received_tx_callback,
            received_tx_reply_callback,
            received_tx_finalized_callback,
            broadcast_callback,
            mined_callback,
            mined_unconfirmed_callback,
            faux_confirmed_callback,
            faux_unconfirmed_callback,
            transaction_send_result_callback,
            tx_cancellation_callback,
            txo_validation_complete_callback,
            contacts_liveness_data_updated_callback,
            balance_updated_callback,
            transaction_validation_complete_callback,
            saf_messages_received_callback,
            connectivity_status_callback,
        );

        runtime.spawn(callback_handler.start());
        let mut callback_balance_updated = 0;

        // The balance updated callback is bundled with other callbacks and will only fire if the balance actually
        // changed from an initial zero balance.
        // Balance updated should be detected with following event, total = 1 times
        transaction_event_sender
            .send(Arc::new(TransactionEvent::ReceivedTransaction(1u64.into())))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 1 {
                    callback_balance_updated = 1;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 1);

        balance.time_locked_balance = Some(completed_tx_cancelled.amount);
        mock_output_manager_service_state.set_balance(balance.clone());
        // Balance updated should be detected with following event, total = 2 times
        transaction_event_sender
            .send(Arc::new(TransactionEvent::ReceivedTransactionReply(2u64.into())))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 2 {
                    callback_balance_updated = 2;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 2);

        balance.pending_incoming_balance += inbound_tx.amount;
        mock_output_manager_service_state.set_balance(balance.clone());
        // Balance updated should be detected with following event, total = 3 times
        transaction_event_sender
            .send(Arc::new(TransactionEvent::ReceivedFinalizedTransaction(2u64.into())))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 3 {
                    callback_balance_updated = 3;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 3);

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionBroadcast(2u64.into())))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionMined {
                tx_id: 2u64.into(),
                is_valid: true,
            }))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionMinedUnconfirmed {
                tx_id: 2u64.into(),
                num_confirmations: 22u64,
                is_valid: true,
            }))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionSendResult(
                2u64.into(),
                TransactionSendStatus {
                    direct_send_result: true,
                    store_and_forward_send_result: true,
                    queued_for_retry: false,
                },
            )))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionSendResult(
                2u64.into(),
                TransactionSendStatus {
                    direct_send_result: false,
                    store_and_forward_send_result: false,
                    queued_for_retry: true,
                },
            )))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionSendResult(
                2u64.into(),
                TransactionSendStatus {
                    direct_send_result: false,
                    store_and_forward_send_result: true,
                    queued_for_retry: false,
                },
            )))
            .unwrap();

        balance.pending_outgoing_balance += outbound_tx.amount;
        mock_output_manager_service_state.set_balance(balance.clone());
        // Balance updated should be detected with following event, total = 4 times
        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                3u64.into(),
                TxCancellationReason::UserCancelled,
            )))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 4 {
                    callback_balance_updated = 4;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 4);

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                4u64.into(),
                TxCancellationReason::UserCancelled,
            )))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionCancelled(
                5u64.into(),
                TxCancellationReason::UserCancelled,
            )))
            .unwrap();

        oms_event_sender
            .send(Arc::new(OutputManagerEvent::TxoValidationSuccess(1u64)))
            .unwrap();

        oms_event_sender
            .send(Arc::new(OutputManagerEvent::TxoValidationSuccess(1u64)))
            .unwrap();

        balance.available_balance -= completed_tx_cancelled.amount;
        mock_output_manager_service_state.set_balance(balance.clone());
        // Balance updated should be detected with following event, total = 5 times
        oms_event_sender
            .send(Arc::new(OutputManagerEvent::TxoValidationSuccess(1u64)))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 5 {
                    callback_balance_updated = 5;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 5);

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionValidationStateChanged(
                1u64.into(),
            )))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionValidationStateChanged(
                2u64.into(),
            )))
            .unwrap();

        oms_event_sender
            .send(Arc::new(OutputManagerEvent::TxoValidationCommunicationFailure(1u64)))
            .unwrap();
        oms_event_sender
            .send(Arc::new(OutputManagerEvent::TxoValidationInternalFailure(1u64)))
            .unwrap();
        oms_event_sender
            .send(Arc::new(OutputManagerEvent::TxoValidationAlreadyBusy(1u64)))
            .unwrap();
        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionValidationCompleted(3u64.into())))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionValidationCompleted(4u64.into())))
            .unwrap();

        balance.pending_incoming_balance += faux_unconfirmed_tx.amount;
        mock_output_manager_service_state.set_balance(balance.clone());
        // Balance updated should be detected with following event, total = 6 times
        transaction_event_sender
            .send(Arc::new(TransactionEvent::FauxTransactionUnconfirmed {
                tx_id: 6u64.into(),
                num_confirmations: 2,
                is_valid: true,
            }))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 6 {
                    callback_balance_updated = 6;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 6);

        balance.available_balance += faux_confirmed_tx.amount;
        mock_output_manager_service_state.set_balance(balance.clone());
        // Balance updated should be detected with following event, total = 7 times
        transaction_event_sender
            .send(Arc::new(TransactionEvent::FauxTransactionConfirmed {
                tx_id: 7u64.into(),
                is_valid: true,
            }))
            .unwrap();
        let start = Instant::now();
        while start.elapsed().as_secs() < 10 {
            {
                let lock = CALLBACK_STATE.lock().unwrap();
                if lock.callback_balance_updated == 7 {
                    callback_balance_updated = 7;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(callback_balance_updated, 7);

        let contact = Contact::new(
            "My friend".to_string(),
            faux_unconfirmed_tx.destination_public_key,
            None,
            None,
        );
        let data = ContactsLivenessData::new(
            contact.public_key.clone(),
            contact.node_id.clone(),
            contact.latency,
            contact.last_seen,
            ContactMessageType::NoMessage,
            ContactOnlineStatus::NeverSeen,
        );
        contacts_liveness_events_sender
            .send(Arc::new(ContactsLivenessEvent::StatusUpdated(Box::new(data))))
            .unwrap();
        let data = ContactsLivenessData::new(
            contact.public_key.clone(),
            contact.node_id,
            Some(1234),
            Some(Utc::now().naive_utc()),
            ContactMessageType::Ping,
            ContactOnlineStatus::Online,
        );
        contacts_liveness_events_sender
            .send(Arc::new(ContactsLivenessEvent::StatusUpdated(Box::new(data))))
            .unwrap();

        dht_event_sender
            .send(Arc::new(DhtEvent::StoreAndForwardMessagesReceived))
            .unwrap();
        thread::sleep(Duration::from_secs(2));
        connectivity_tx.send(OnlineStatus::Offline).unwrap();
        thread::sleep(Duration::from_secs(2));
        connectivity_tx.send(OnlineStatus::Connecting).unwrap();
        thread::sleep(Duration::from_secs(2));
        connectivity_tx.send(OnlineStatus::Online).unwrap();
        thread::sleep(Duration::from_secs(2));
        connectivity_tx.send(OnlineStatus::Connecting).unwrap();

        thread::sleep(Duration::from_secs(10));

        let lock = CALLBACK_STATE.lock().unwrap();
        assert!(lock.received_tx_callback_called);
        assert!(lock.received_tx_reply_callback_called);
        assert!(lock.received_finalized_tx_callback_called);
        assert!(lock.broadcast_tx_callback_called);
        assert!(lock.mined_tx_callback_called);
        assert_eq!(lock.mined_tx_unconfirmed_callback_called, 22u64);
        assert!(lock.faux_tx_confirmed_callback_called);
        assert_eq!(lock.faux_tx_unconfirmed_callback_called, 2u64);
        assert_eq!(lock.direct_send_callback_called, 1);
        assert_eq!(lock.store_and_forward_send_callback_called, 2);
        assert_eq!(lock.transaction_queued_for_retry_callback_called, 1);
        assert!(lock.tx_cancellation_callback_called_inbound);
        assert!(lock.tx_cancellation_callback_called_completed);
        assert!(lock.tx_cancellation_callback_called_outbound);
        assert!(lock.saf_messages_received);
        assert!(lock.callback_txo_validation_completed);
        assert!(lock.callback_txo_validation_communication_failure);
        assert!(lock.callback_txo_validation_already_busy);
        assert!(lock.callback_txo_validation_internal_failure);
        assert_eq!(lock.callback_contacts_liveness_data_updated, 2);
        assert_eq!(lock.callback_balance_updated, 7);
        assert_eq!(lock.callback_transaction_validation_complete, 7);
        assert_eq!(lock.connectivity_status_callback_called, 7);

        drop(lock);
    }
}
