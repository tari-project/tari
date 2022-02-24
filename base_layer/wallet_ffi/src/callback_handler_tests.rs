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

#[cfg(test)]
mod test {
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use chrono::Utc;
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
        output_manager_service::{
            handle::{OutputManagerEvent, OutputManagerHandle},
            service::Balance,
        },
        test_utils::make_wallet_database_connection,
        transaction_service::{
            handle::TransactionEvent,
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
    struct CallbackState {
        pub received_tx_callback_called: bool,
        pub received_tx_reply_callback_called: bool,
        pub received_finalized_tx_callback_called: bool,
        pub broadcast_tx_callback_called: bool,
        pub mined_tx_callback_called: bool,
        pub mined_tx_unconfirmed_callback_called: u64,
        pub faux_tx_confirmed_callback_called: bool,
        pub faux_tx_unconfirmed_callback_called: u64,
        pub direct_send_callback_called: bool,
        pub store_and_forward_send_callback_called: bool,
        pub tx_cancellation_callback_called_completed: bool,
        pub tx_cancellation_callback_called_inbound: bool,
        pub tx_cancellation_callback_called_outbound: bool,
        pub callback_txo_validation_complete: u32,
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
                direct_send_callback_called: false,
                store_and_forward_send_callback_called: false,
                callback_txo_validation_complete: 0,
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

    unsafe extern "C" fn direct_send_callback(_tx_id: u64, _result: bool) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.direct_send_callback_called = true;
        drop(lock);
    }

    unsafe extern "C" fn store_and_forward_send_callback(_tx_id: u64, _result: bool) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.store_and_forward_send_callback_called = true;
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

    unsafe extern "C" fn txo_validation_complete_callback(_tx_id: u64, result: bool) {
        let mut lock = CALLBACK_STATE.lock().unwrap();
        lock.callback_txo_validation_complete += result as u32;
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
        runtime
            .block_on(db.add_pending_inbound_transaction(1u64.into(), inbound_tx.clone()))
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
        );
        runtime
            .block_on(db.insert_completed_transaction(2u64.into(), completed_tx.clone()))
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
        runtime
            .block_on(db.add_pending_outbound_transaction(3u64.into(), outbound_tx.clone()))
            .unwrap();
        runtime.block_on(db.cancel_pending_transaction(3u64.into())).unwrap();

        let inbound_tx_cancelled = InboundTransaction {
            tx_id: 4u64.into(),
            ..inbound_tx.clone()
        };
        runtime
            .block_on(db.add_pending_inbound_transaction(4u64.into(), inbound_tx_cancelled))
            .unwrap();
        runtime.block_on(db.cancel_pending_transaction(4u64.into())).unwrap();

        let completed_tx_cancelled = CompletedTransaction {
            tx_id: 5u64.into(),
            ..completed_tx.clone()
        };
        runtime
            .block_on(db.insert_completed_transaction(5u64.into(), completed_tx_cancelled.clone()))
            .unwrap();
        runtime
            .block_on(db.reject_completed_transaction(5u64.into(), TxCancellationReason::Unknown))
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
        );
        runtime
            .block_on(db.insert_completed_transaction(6u64.into(), faux_unconfirmed_tx.clone()))
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
        );
        runtime
            .block_on(db.insert_completed_transaction(7u64.into(), faux_confirmed_tx.clone()))
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

        let callback_handler = CallbackHandler::new(
            db,
            transaction_event_receiver,
            oms_event_receiver,
            oms_handle,
            dht_event_receiver,
            shutdown_signal.to_signal(),
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            connectivity_rx,
            received_tx_callback,
            received_tx_reply_callback,
            received_tx_finalized_callback,
            broadcast_callback,
            mined_callback,
            mined_unconfirmed_callback,
            faux_confirmed_callback,
            faux_unconfirmed_callback,
            direct_send_callback,
            store_and_forward_send_callback,
            tx_cancellation_callback,
            txo_validation_complete_callback,
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
            .send(Arc::new(TransactionEvent::TransactionDirectSendResult(
                2u64.into(),
                true,
            )))
            .unwrap();

        transaction_event_sender
            .send(Arc::new(TransactionEvent::TransactionStoreForwardSendResult(
                2u64.into(),
                true,
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
            .send(Arc::new(OutputManagerEvent::TxoValidationFailure(1u64)))
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
        assert!(lock.direct_send_callback_called);
        assert!(lock.store_and_forward_send_callback_called);
        assert!(lock.tx_cancellation_callback_called_inbound);
        assert!(lock.tx_cancellation_callback_called_completed);
        assert!(lock.tx_cancellation_callback_called_outbound);
        assert!(lock.saf_messages_received);
        assert_eq!(lock.callback_txo_validation_complete, 3);
        assert_eq!(lock.callback_balance_updated, 7);
        assert_eq!(lock.callback_transaction_validation_complete, 7);
        assert_eq!(lock.connectivity_status_callback_called, 7);

        drop(lock);
    }
}
