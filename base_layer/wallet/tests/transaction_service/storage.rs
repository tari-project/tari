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

use crate::support::utils::random_string;
use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use chrono::Utc;
use rand::rngs::OsRng;
use tari_core::transactions::{
    tari_amount::{uT, MicroTari},
    transaction::{OutputFeatures, Transaction, UnblindedOutput},
    transaction_protocol::sender::TransactionSenderMessage,
    types::{CryptoFactories, HashDigest, PrivateKey, PublicKey},
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
use tari_wallet::{
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    transaction_service::storage::{
        database::{TransactionBackend, TransactionDatabase},
        memory_db::TransactionMemoryDatabase,
        models::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            TransactionDirection,
            TransactionStatus,
            WalletTransaction,
        },
        sqlite_db::TransactionServiceSqliteDatabase,
    },
};
use tempfile::tempdir;
use tokio::runtime::Runtime;

pub fn test_db_backend<T: TransactionBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();
    let mut db = TransactionDatabase::new(backend);
    let factories = CryptoFactories::default();
    let mut builder = SenderTransactionProtocol::builder(1);
    let amount = MicroTari::from(10_000);
    let input = UnblindedOutput::new(MicroTari::from(100_000), PrivateKey::random(&mut OsRng), None);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input.as_transaction_input(&factories.commitment, OutputFeatures::default()),
            input.clone(),
        )
        .with_change_secret(PrivateKey::random(&mut OsRng));

    let stp = builder.build::<HashDigest>(&factories).unwrap();

    let messages = vec!["Hey!".to_string(), "Yo!".to_string(), "Sup!".to_string()];
    let amounts = vec![MicroTari::from(10_000), MicroTari::from(23_000), MicroTari::from(5_000)];

    let mut outbound_txs = Vec::new();

    for i in 0..messages.len() {
        outbound_txs.push(OutboundTransaction {
            tx_id: (i + 10) as u64,
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: amounts[i].clone(),
            fee: stp.clone().get_fee_amount().unwrap(),
            sender_protocol: stp.clone(),
            status: TransactionStatus::Pending,
            message: messages[i].clone(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        });
        assert!(
            !runtime.block_on(db.transaction_exists((i + 10) as u64)).unwrap(),
            "TxId should not exist"
        );

        runtime
            .block_on(db.add_pending_outbound_transaction(outbound_txs[i].tx_id, outbound_txs[i].clone()))
            .unwrap();
        assert!(
            runtime.block_on(db.transaction_exists((i + 10) as u64)).unwrap(),
            "TxId should exist"
        );
    }

    let retrieved_outbound_txs = runtime.block_on(db.get_pending_outbound_transactions()).unwrap();
    assert_eq!(outbound_txs.len(), messages.len());
    for i in 0..messages.len() {
        let retrieved_outbound_tx = runtime
            .block_on(db.get_pending_outbound_transaction(outbound_txs[i].tx_id))
            .unwrap();
        assert_eq!(retrieved_outbound_tx, outbound_txs[i]);
        assert_eq!(retrieved_outbound_tx.send_count, 0);
        assert!(retrieved_outbound_tx.last_send_timestamp.is_none());

        assert_eq!(
            retrieved_outbound_txs.get(&outbound_txs[i].tx_id).unwrap(),
            &outbound_txs[i]
        );
    }

    runtime
        .block_on(db.increment_send_count(outbound_txs[0].tx_id))
        .unwrap();
    let retrieved_outbound_tx = runtime
        .block_on(db.get_pending_outbound_transaction(outbound_txs[0].tx_id))
        .unwrap();
    assert_eq!(retrieved_outbound_tx.send_count, 1);
    assert!(retrieved_outbound_tx.last_send_timestamp.is_some());

    let any_outbound_tx = runtime
        .block_on(db.get_any_transaction(outbound_txs[0].tx_id))
        .unwrap()
        .unwrap();
    if let WalletTransaction::PendingOutbound(tx) = any_outbound_tx {
        assert_eq!(tx, retrieved_outbound_tx);
    } else {
        assert!(false, "Should have found outbound tx");
    }

    let rtp = ReceiverTransactionProtocol::new(
        TransactionSenderMessage::Single(Box::new(stp.clone().build_single_round_message().unwrap())),
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
        OutputFeatures::default(),
        &factories,
    );

    let mut inbound_txs = Vec::new();

    for i in 0..messages.len() {
        inbound_txs.push(InboundTransaction {
            tx_id: i as u64,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: amounts[i].clone(),
            receiver_protocol: rtp.clone(),
            status: TransactionStatus::Pending,
            message: messages[i].clone(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
        });
        assert!(
            !runtime.block_on(db.transaction_exists(i as u64)).unwrap(),
            "TxId should not exist"
        );
        runtime
            .block_on(db.add_pending_inbound_transaction(i as u64, inbound_txs[i].clone()))
            .unwrap();
        assert!(
            runtime.block_on(db.transaction_exists(i as u64)).unwrap(),
            "TxId should exist"
        );
    }

    let retrieved_inbound_txs = runtime.block_on(db.get_pending_inbound_transactions()).unwrap();
    assert_eq!(inbound_txs.len(), messages.len());
    for i in 0..messages.len() {
        let retrieved_tx = retrieved_inbound_txs.get(&inbound_txs[i].tx_id).unwrap();
        assert_eq!(retrieved_tx, &inbound_txs[i]);
        assert_eq!(retrieved_tx.send_count, 0);
        assert!(retrieved_tx.last_send_timestamp.is_none());
    }

    runtime.block_on(db.increment_send_count(inbound_txs[0].tx_id)).unwrap();
    let retrieved_inbound_tx = runtime
        .block_on(db.get_pending_inbound_transaction(inbound_txs[0].tx_id))
        .unwrap();
    assert_eq!(retrieved_inbound_tx.send_count, 1);
    assert!(retrieved_inbound_tx.last_send_timestamp.is_some());

    let any_inbound_tx = runtime
        .block_on(db.get_any_transaction(inbound_txs[0].tx_id))
        .unwrap()
        .unwrap();
    if let WalletTransaction::PendingInbound(tx) = any_inbound_tx {
        assert_eq!(tx, retrieved_inbound_tx);
    } else {
        assert!(false, "Should have found inbound tx");
    }

    let inbound_pub_key = runtime
        .block_on(db.get_pending_transaction_counterparty_pub_key_by_tx_id(inbound_txs[0].tx_id))
        .unwrap();
    assert_eq!(inbound_pub_key, inbound_txs[0].source_public_key);

    assert!(runtime
        .block_on(db.get_pending_transaction_counterparty_pub_key_by_tx_id(100))
        .is_err());

    let outbound_pub_key = runtime
        .block_on(db.get_pending_transaction_counterparty_pub_key_by_tx_id(outbound_txs[0].tx_id))
        .unwrap();
    assert_eq!(outbound_pub_key, outbound_txs[0].destination_public_key);

    let mut completed_txs = Vec::new();
    let tx = Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut OsRng));

    for i in 0..messages.len() {
        completed_txs.push(CompletedTransaction {
            tx_id: outbound_txs[i].tx_id,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: outbound_txs[i].amount,
            fee: MicroTari::from(200),
            transaction: tx.clone(),
            status: match i {
                0 => TransactionStatus::Completed,
                1 => TransactionStatus::Broadcast,
                _ => TransactionStatus::Mined,
            },
            message: messages[i].clone(),
            timestamp: Utc::now().naive_utc(),
            cancelled: false,
            direction: TransactionDirection::Outbound,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,
        });
        runtime
            .block_on(db.complete_outbound_transaction(outbound_txs[i].tx_id, completed_txs[i].clone()))
            .unwrap();
        runtime
            .block_on(
                db.complete_inbound_transaction(inbound_txs[i].tx_id, CompletedTransaction {
                    tx_id: inbound_txs[i].tx_id,
                    ..completed_txs[i].clone()
                }),
            )
            .unwrap();
    }

    let retrieved_completed_txs = runtime.block_on(db.get_completed_transactions()).unwrap();
    assert_eq!(retrieved_completed_txs.len(), 2 * messages.len());

    for i in 0..messages.len() {
        assert_eq!(
            retrieved_completed_txs.get(&inbound_txs[i].tx_id).unwrap(),
            &CompletedTransaction {
                tx_id: inbound_txs[i].tx_id,
                ..completed_txs[i].clone()
            }
        );
        assert_eq!(
            retrieved_completed_txs.get(&outbound_txs[i].tx_id).unwrap(),
            &completed_txs[i]
        );
    }

    runtime
        .block_on(db.increment_send_count(completed_txs[0].tx_id))
        .unwrap();
    runtime
        .block_on(db.increment_send_count(completed_txs[0].tx_id))
        .unwrap();
    let retrieved_completed_tx = runtime
        .block_on(db.get_completed_transaction(completed_txs[0].tx_id))
        .unwrap();
    assert_eq!(retrieved_completed_tx.send_count, 2);
    assert!(retrieved_completed_tx.last_send_timestamp.is_some());

    let any_completed_tx = runtime
        .block_on(db.get_any_transaction(completed_txs[0].tx_id))
        .unwrap()
        .unwrap();
    if let WalletTransaction::Completed(tx) = any_completed_tx {
        assert_eq!(tx, retrieved_completed_tx);
    } else {
        assert!(false, "Should have found completed tx");
    }

    if cfg!(feature = "test_harness") {
        let retrieved_completed_txs = runtime.block_on(db.get_completed_transactions()).unwrap();
        assert!(retrieved_completed_txs.contains_key(&completed_txs[0].tx_id));
        assert_eq!(
            retrieved_completed_txs.get(&completed_txs[0].tx_id).unwrap().status,
            TransactionStatus::Completed
        );
        #[cfg(feature = "test_harness")]
        runtime
            .block_on(db.broadcast_completed_transaction(completed_txs[0].tx_id))
            .unwrap();
        let retrieved_completed_txs = runtime.block_on(db.get_completed_transactions()).unwrap();

        assert!(retrieved_completed_txs.contains_key(&completed_txs[0].tx_id));
        assert_eq!(
            retrieved_completed_txs.get(&completed_txs[0].tx_id).unwrap().status,
            TransactionStatus::Broadcast
        );

        #[cfg(feature = "test_harness")]
        runtime
            .block_on(db.mine_completed_transaction(completed_txs[0].tx_id))
            .unwrap();
        let retrieved_completed_txs = runtime.block_on(db.get_completed_transactions()).unwrap();

        assert!(retrieved_completed_txs.contains_key(&completed_txs[0].tx_id));
        assert_eq!(
            retrieved_completed_txs.get(&completed_txs[0].tx_id).unwrap().status,
            TransactionStatus::Mined
        );
    }

    let completed_txs = runtime.block_on(db.get_completed_transactions()).unwrap();
    let num_completed_txs = completed_txs.len();
    assert_eq!(
        runtime
            .block_on(db.get_cancelled_completed_transactions())
            .unwrap()
            .len(),
        0
    );

    let cancelled_tx_id = completed_txs[&1].tx_id;
    assert!(runtime
        .block_on(db.get_cancelled_completed_transaction(cancelled_tx_id))
        .is_err());
    runtime
        .block_on(db.cancel_completed_transaction(cancelled_tx_id))
        .unwrap();
    let completed_txs = runtime.block_on(db.get_completed_transactions()).unwrap();
    assert_eq!(completed_txs.len(), num_completed_txs - 1);

    runtime
        .block_on(db.get_cancelled_completed_transaction(cancelled_tx_id))
        .expect("Should find cancelled transaction");

    let mut cancelled_txs = runtime.block_on(db.get_cancelled_completed_transactions()).unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&cancelled_tx_id).is_some());

    let any_cancelled_completed_tx = runtime
        .block_on(db.get_any_transaction(cancelled_tx_id))
        .unwrap()
        .unwrap();
    if let WalletTransaction::Completed(tx) = any_cancelled_completed_tx {
        assert_eq!(tx.tx_id, cancelled_tx_id);
    } else {
        assert!(false, "Should have found cancelled completed tx");
    }

    runtime
        .block_on(db.add_pending_inbound_transaction(
            999,
            InboundTransaction::new(
                999u64,
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                22 * uT,
                rtp.clone(),
                TransactionStatus::Pending,
                "To be cancelled".to_string(),
                Utc::now().naive_utc(),
            ),
        ))
        .unwrap();

    assert_eq!(
        runtime
            .block_on(db.get_cancelled_pending_inbound_transactions())
            .unwrap()
            .len(),
        0
    );

    assert_eq!(
        runtime.block_on(db.get_pending_inbound_transactions()).unwrap().len(),
        1
    );
    assert!(
        !runtime
            .block_on(db.get_pending_inbound_transaction(999))
            .unwrap()
            .direct_send_success
    );
    runtime.block_on(db.mark_direct_send_success(999)).unwrap();
    assert!(
        runtime
            .block_on(db.get_pending_inbound_transaction(999))
            .unwrap()
            .direct_send_success
    );
    assert!(runtime
        .block_on(db.get_cancelled_pending_inbound_transaction(999))
        .is_err());
    runtime.block_on(db.cancel_pending_transaction(999)).unwrap();
    runtime
        .block_on(db.get_cancelled_pending_inbound_transaction(999))
        .expect("Should find cancelled inbound tx");

    assert_eq!(
        runtime
            .block_on(db.get_cancelled_pending_inbound_transactions())
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        runtime.block_on(db.get_pending_inbound_transactions()).unwrap().len(),
        0
    );

    let any_cancelled_inbound_tx = runtime.block_on(db.get_any_transaction(999)).unwrap().unwrap();
    if let WalletTransaction::PendingInbound(tx) = any_cancelled_inbound_tx {
        assert_eq!(tx.tx_id, 999);
    } else {
        assert!(false, "Should have found cancelled inbound tx");
    }

    let mut cancelled_txs = runtime
        .block_on(db.get_cancelled_pending_inbound_transactions())
        .unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&999).is_some());

    runtime
        .block_on(db.add_pending_outbound_transaction(
            998,
            OutboundTransaction::new(
                998u64,
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                22 * uT,
                stp.clone().get_fee_amount().unwrap(),
                stp.clone(),
                TransactionStatus::Pending,
                "To be cancelled".to_string(),
                Utc::now().naive_utc(),
                false,
            ),
        ))
        .unwrap();

    assert!(
        !runtime
            .block_on(db.get_pending_outbound_transaction(998))
            .unwrap()
            .direct_send_success
    );
    runtime.block_on(db.mark_direct_send_success(998)).unwrap();
    assert!(
        runtime
            .block_on(db.get_pending_outbound_transaction(998))
            .unwrap()
            .direct_send_success
    );

    assert_eq!(
        runtime
            .block_on(db.get_cancelled_pending_outbound_transactions())
            .unwrap()
            .len(),
        0
    );

    assert_eq!(
        runtime.block_on(db.get_pending_outbound_transactions()).unwrap().len(),
        1
    );

    assert!(runtime
        .block_on(db.get_cancelled_pending_outbound_transaction(998))
        .is_err());

    runtime.block_on(db.cancel_pending_transaction(998)).unwrap();
    runtime
        .block_on(db.get_cancelled_pending_outbound_transaction(998))
        .expect("Should find cancelled outbound tx");
    assert_eq!(
        runtime
            .block_on(db.get_cancelled_pending_outbound_transactions())
            .unwrap()
            .len(),
        1
    );

    assert_eq!(
        runtime.block_on(db.get_pending_outbound_transactions()).unwrap().len(),
        0
    );

    let mut cancelled_txs = runtime
        .block_on(db.get_cancelled_pending_outbound_transactions())
        .unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&998).is_some());

    let any_cancelled_outbound_tx = runtime.block_on(db.get_any_transaction(998)).unwrap().unwrap();
    if let WalletTransaction::PendingOutbound(tx) = any_cancelled_outbound_tx {
        assert_eq!(tx.tx_id, 998);
    } else {
        assert!(false, "Should have found cancelled outbound tx");
    }
}

#[test]
pub fn test_transaction_service_memory_db() {
    test_db_backend(TransactionMemoryDatabase::new());
}

#[test]
pub fn test_transaction_service_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    test_db_backend(TransactionServiceSqliteDatabase::new(connection, None));
}

#[test]
pub fn test_transaction_service_sqlite_db_encrypted() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    let key = GenericArray::from_slice(b"an example very very secret key.");
    let cipher = Aes256Gcm::new(key);

    test_db_backend(TransactionServiceSqliteDatabase::new(connection, Some(cipher)));
}
