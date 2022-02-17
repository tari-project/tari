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

use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use chrono::Utc;
use rand::rngs::OsRng;
use tari_common_types::{
    transaction::{TransactionDirection, TransactionStatus, TxId},
    types::{HashDigest, PrivateKey, PublicKey, Signature},
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::{uT, MicroTari},
        test_helpers::{create_unblinded_output, TestParams},
        transaction_components::{OutputFeatures, Transaction},
        transaction_protocol::sender::TransactionSenderMessage,
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
    script,
    script::{ExecutionStack, TariScript},
};
use tari_test_utils::random;
use tari_wallet::{
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    test_utils::create_consensus_constants,
    transaction_service::storage::{
        database::{DbKeyValuePair, TransactionBackend, TransactionDatabase, WriteOperation},
        models::{
            CompletedTransaction,
            InboundTransaction,
            OutboundTransaction,
            TxCancellationReason,
            WalletTransaction,
        },
        sqlite_db::TransactionServiceSqliteDatabase,
    },
};
use tempfile::tempdir;
use tokio::runtime::Runtime;

pub fn test_db_backend<T: TransactionBackend + 'static>(backend: T) {
    let runtime = Runtime::new().unwrap();
    let mut db = TransactionDatabase::new(backend);
    let factories = CryptoFactories::default();
    let input = create_unblinded_output(
        TariScript::default(),
        OutputFeatures::default(),
        TestParams::new(),
        MicroTari::from(100_000),
    );
    let constants = create_consensus_constants(0);
    let mut builder = SenderTransactionProtocol::builder(1, constants);
    let amount = MicroTari::from(10_000);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177 / 5))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input
                .as_transaction_input(&factories.commitment)
                .expect("Should be able to make transaction input"),
            input,
        )
        .with_change_secret(PrivateKey::random(&mut OsRng))
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            Default::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(script!(Nop), ExecutionStack::default(), PrivateKey::random(&mut OsRng));

    let stp = builder.build::<HashDigest>(&factories, None, u64::MAX).unwrap();

    let messages = vec!["Hey!".to_string(), "Yo!".to_string(), "Sup!".to_string()];
    let amounts = vec![MicroTari::from(10_000), MicroTari::from(23_000), MicroTari::from(5_000)];

    let mut outbound_txs = Vec::new();

    for i in 0..messages.len() {
        let tx_id = TxId::from(i + 10);
        outbound_txs.push(OutboundTransaction {
            tx_id,
            destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: amounts[i],
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
            !runtime.block_on(db.transaction_exists(tx_id)).unwrap(),
            "TxId should not exist"
        );

        runtime
            .block_on(db.add_pending_outbound_transaction(outbound_txs[i].tx_id, outbound_txs[i].clone()))
            .unwrap();

        assert!(
            runtime.block_on(db.transaction_exists(tx_id)).unwrap(),
            "TxId should exist"
        );
    }

    let retrieved_outbound_txs = runtime.block_on(db.get_pending_outbound_transactions()).unwrap();
    assert_eq!(outbound_txs.len(), messages.len());
    for i in outbound_txs.iter().take(messages.len()) {
        let retrieved_outbound_tx = runtime.block_on(db.get_pending_outbound_transaction(i.tx_id)).unwrap();
        assert_eq!(&retrieved_outbound_tx, i);
        assert_eq!(retrieved_outbound_tx.send_count, 0);
        assert!(retrieved_outbound_tx.last_send_timestamp.is_none());

        assert_eq!(&retrieved_outbound_txs.get(&i.tx_id).unwrap(), &i);
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
        panic!("Should have found outbound tx");
    }

    let rtp = ReceiverTransactionProtocol::new(
        TransactionSenderMessage::Single(Box::new(stp.clone().build_single_round_message().unwrap())),
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
        &factories,
    );

    let mut inbound_txs = Vec::new();

    for i in 0..messages.len() {
        let tx_id = TxId::from(i);
        inbound_txs.push(InboundTransaction {
            tx_id,
            source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            amount: amounts[i],
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
            !runtime.block_on(db.transaction_exists(tx_id)).unwrap(),
            "TxId should not exist"
        );
        runtime
            .block_on(db.add_pending_inbound_transaction(tx_id, inbound_txs[i].clone()))
            .unwrap();
        assert!(
            runtime.block_on(db.transaction_exists(tx_id)).unwrap(),
            "TxId should exist"
        );
    }

    let retrieved_inbound_txs = runtime.block_on(db.get_pending_inbound_transactions()).unwrap();
    assert_eq!(inbound_txs.len(), messages.len());
    for i in inbound_txs.iter().take(messages.len()) {
        let retrieved_tx = retrieved_inbound_txs.get(&i.tx_id).unwrap();
        assert_eq!(&retrieved_tx, &i);
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
        panic!("Should have found inbound tx");
    }

    let inbound_pub_key = runtime
        .block_on(db.get_pending_transaction_counterparty_pub_key_by_tx_id(inbound_txs[0].tx_id))
        .unwrap();
    assert_eq!(inbound_pub_key, inbound_txs[0].source_public_key);

    assert!(runtime
        .block_on(db.get_pending_transaction_counterparty_pub_key_by_tx_id(100u64.into()))
        .is_err());

    let outbound_pub_key = runtime
        .block_on(db.get_pending_transaction_counterparty_pub_key_by_tx_id(outbound_txs[0].tx_id))
        .unwrap();
    assert_eq!(outbound_pub_key, outbound_txs[0].destination_public_key);

    let mut completed_txs = Vec::new();
    let tx = Transaction::new(
        vec![],
        vec![],
        vec![],
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
    );

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
                _ => TransactionStatus::MinedUnconfirmed,
            },
            message: messages[i].clone(),
            timestamp: Utc::now().naive_utc(),
            cancelled: None,
            direction: TransactionDirection::Outbound,
            coinbase_block_height: None,
            send_count: 0,
            last_send_timestamp: None,

            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
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
    assert!(retrieved_completed_tx.confirmations.is_none());

    assert!(runtime.block_on(db.fetch_last_mined_transaction()).unwrap().is_none());

    runtime
        .block_on(db.set_transaction_mined_height(completed_txs[0].tx_id, 10, [0u8; 16].to_vec(), 5, true, false))
        .unwrap();

    assert_eq!(
        runtime
            .block_on(db.fetch_last_mined_transaction())
            .unwrap()
            .unwrap()
            .tx_id,
        completed_txs[0].tx_id
    );

    let retrieved_completed_tx = runtime
        .block_on(db.get_completed_transaction(completed_txs[0].tx_id))
        .unwrap();
    assert_eq!(retrieved_completed_tx.confirmations, Some(5));

    let any_completed_tx = runtime
        .block_on(db.get_any_transaction(completed_txs[0].tx_id))
        .unwrap()
        .unwrap();
    if let WalletTransaction::Completed(tx) = any_completed_tx {
        assert_eq!(tx, retrieved_completed_tx);
    } else {
        panic!("Should have found completed tx");
    }

    let completed_txs_map = runtime.block_on(db.get_completed_transactions()).unwrap();
    let num_completed_txs = completed_txs_map.len();
    assert_eq!(
        runtime
            .block_on(db.get_cancelled_completed_transactions())
            .unwrap()
            .len(),
        0
    );

    let cancelled_tx_id = completed_txs_map[&1u64.into()].tx_id;
    assert!(runtime
        .block_on(db.get_cancelled_completed_transaction(cancelled_tx_id))
        .is_err());
    runtime
        .block_on(db.reject_completed_transaction(cancelled_tx_id, TxCancellationReason::Unknown))
        .unwrap();
    let completed_txs_map = runtime.block_on(db.get_completed_transactions()).unwrap();
    assert_eq!(completed_txs_map.len(), num_completed_txs - 1);

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
        panic!("Should have found cancelled completed tx");
    }

    runtime
        .block_on(db.add_pending_inbound_transaction(
            999u64.into(),
            InboundTransaction::new(
                999u64.into(),
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                22 * uT,
                rtp,
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
            .block_on(db.get_pending_inbound_transaction(999u64.into()))
            .unwrap()
            .direct_send_success
    );
    runtime.block_on(db.mark_direct_send_success(999u64.into())).unwrap();
    assert!(
        runtime
            .block_on(db.get_pending_inbound_transaction(999u64.into()))
            .unwrap()
            .direct_send_success
    );
    assert!(runtime
        .block_on(db.get_cancelled_pending_inbound_transaction(999u64.into()))
        .is_err());
    runtime.block_on(db.cancel_pending_transaction(999u64.into())).unwrap();
    runtime
        .block_on(db.get_cancelled_pending_inbound_transaction(999u64.into()))
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

    let any_cancelled_inbound_tx = runtime
        .block_on(db.get_any_transaction(999u64.into()))
        .unwrap()
        .unwrap();
    if let WalletTransaction::PendingInbound(tx) = any_cancelled_inbound_tx {
        assert_eq!(tx.tx_id, TxId::from(999u64));
    } else {
        panic!("Should have found cancelled inbound tx");
    }

    let mut cancelled_txs = runtime
        .block_on(db.get_cancelled_pending_inbound_transactions())
        .unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&999u64.into()).is_some());

    runtime
        .block_on(db.add_pending_outbound_transaction(
            998u64.into(),
            OutboundTransaction::new(
                998u64.into(),
                PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                22 * uT,
                stp.get_fee_amount().unwrap(),
                stp,
                TransactionStatus::Pending,
                "To be cancelled".to_string(),
                Utc::now().naive_utc(),
                false,
            ),
        ))
        .unwrap();

    assert!(
        !runtime
            .block_on(db.get_pending_outbound_transaction(998u64.into()))
            .unwrap()
            .direct_send_success
    );
    runtime.block_on(db.mark_direct_send_success(998u64.into())).unwrap();
    assert!(
        runtime
            .block_on(db.get_pending_outbound_transaction(998u64.into()))
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
        .block_on(db.get_cancelled_pending_outbound_transaction(998u64.into()))
        .is_err());

    runtime.block_on(db.cancel_pending_transaction(998u64.into())).unwrap();
    runtime
        .block_on(db.get_cancelled_pending_outbound_transaction(998u64.into()))
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
    assert!(cancelled_txs.remove(&998u64.into()).is_some());

    let any_cancelled_outbound_tx = runtime
        .block_on(db.get_any_transaction(998u64.into()))
        .unwrap()
        .unwrap();
    if let WalletTransaction::PendingOutbound(tx) = any_cancelled_outbound_tx {
        assert_eq!(tx.tx_id, TxId::from(998u64));
    } else {
        panic!("Should have found cancelled outbound tx");
    }

    let unmined_txs = runtime.block_on(db.fetch_unconfirmed_transactions_info()).unwrap();

    assert_eq!(unmined_txs.len(), 4);

    runtime
        .block_on(db.set_transaction_as_unmined(completed_txs[0].tx_id))
        .unwrap();

    let unmined_txs = runtime.block_on(db.fetch_unconfirmed_transactions_info()).unwrap();
    assert_eq!(unmined_txs.len(), 5);
}

#[test]
pub fn test_transaction_service_sqlite_db() {
    let db_name = format!("{}.sqlite3", random::string(8));
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path, 16).unwrap();

    test_db_backend(TransactionServiceSqliteDatabase::new(connection, None));
}

#[test]
pub fn test_transaction_service_sqlite_db_encrypted() {
    let db_name = format!("{}.sqlite3", random::string(8));
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path, 16).unwrap();

    let key = GenericArray::from_slice(b"an example very very secret key.");
    let cipher = Aes256Gcm::new(key);

    test_db_backend(TransactionServiceSqliteDatabase::new(connection, Some(cipher)));
}

#[tokio::test]
async fn import_tx_and_read_it_from_db() {
    let db_name = format!("{}.sqlite3", random::string(8));
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path, 16).unwrap();

    let key = GenericArray::from_slice(b"an example very very secret key.");
    let cipher = Aes256Gcm::new(key);
    let sqlite_db = TransactionServiceSqliteDatabase::new(connection, Some(cipher));

    let transaction = CompletedTransaction::new(
        TxId::from(1u64),
        PublicKey::default(),
        PublicKey::default(),
        MicroTari::from(100000),
        MicroTari::from(0),
        Transaction::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        ),
        TransactionStatus::Imported,
        "message".to_string(),
        Utc::now().naive_utc(),
        TransactionDirection::Inbound,
        Some(0),
        Some(5),
    );

    sqlite_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            TxId::from(1u64),
            Box::new(transaction),
        )))
        .unwrap();

    let transaction = CompletedTransaction::new(
        TxId::from(2u64),
        PublicKey::default(),
        PublicKey::default(),
        MicroTari::from(100000),
        MicroTari::from(0),
        Transaction::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        ),
        TransactionStatus::FauxUnconfirmed,
        "message".to_string(),
        Utc::now().naive_utc(),
        TransactionDirection::Inbound,
        Some(0),
        Some(6),
    );

    sqlite_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            TxId::from(2u64),
            Box::new(transaction),
        )))
        .unwrap();

    let transaction = CompletedTransaction::new(
        TxId::from(3u64),
        PublicKey::default(),
        PublicKey::default(),
        MicroTari::from(100000),
        MicroTari::from(0),
        Transaction::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        ),
        TransactionStatus::FauxConfirmed,
        "message".to_string(),
        Utc::now().naive_utc(),
        TransactionDirection::Inbound,
        Some(0),
        Some(7),
    );

    sqlite_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            TxId::from(3u64),
            Box::new(transaction),
        )))
        .unwrap();

    let db_tx = sqlite_db.fetch_imported_transactions().unwrap();
    assert_eq!(db_tx.len(), 1);
    assert_eq!(db_tx.first().unwrap().tx_id, TxId::from(1));
    assert_eq!(db_tx.first().unwrap().mined_height, Some(5));

    let db_tx = sqlite_db.fetch_unconfirmed_faux_transactions().unwrap();
    assert_eq!(db_tx.len(), 1);
    assert_eq!(db_tx.first().unwrap().tx_id, TxId::from(2));
    assert_eq!(db_tx.first().unwrap().mined_height, Some(6));

    let db_tx = sqlite_db.fetch_confirmed_faux_transactions_from_height(10).unwrap();
    assert_eq!(db_tx.len(), 0);
    let db_tx = sqlite_db.fetch_confirmed_faux_transactions_from_height(4).unwrap();
    assert_eq!(db_tx.len(), 1);
    assert_eq!(db_tx.first().unwrap().tx_id, TxId::from(3));
    assert_eq!(db_tx.first().unwrap().mined_height, Some(7));
}
