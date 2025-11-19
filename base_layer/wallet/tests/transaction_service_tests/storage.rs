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

#![allow(clippy::indexing_slicing)]
use std::mem::size_of;

use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use chrono::{DateTime, Utc};
use minotari_wallet::{
    legacy_transaction_protocol::{ReceiverTransactionProtocol, SenderTransactionProtocol},
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
use rand::{rngs::OsRng, RngCore};
use tari_common::configuration::Network;
use tari_common_types::{
    tari_address::TariAddress,
    transaction::{LegacyTransactionStatus, TransactionDirection, TxId},
    types::{CompressedPublicKey, CompressedSignature, FixedHash, PrivateKey},
};
use tari_crypto::keys::SecretKey as SecretKeyTrait;
use tari_script::{inputs, script};
use tari_test_utils::random;
use tari_transaction_components::{
    key_manager::{KeyManager, TariKeyId, TransactionKeyManagerInterface},
    test_helpers::{create_wallet_output_with_data, TestParams},
    transaction_builder::TransactionBuilder,
    transaction_components::{
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        OutputFeatures,
        Transaction,
        TransactionOutputVersion,
        WalletOutput,
    },
    MicroMinotari,
};
use tempfile::tempdir;

pub async fn test_db_backend<T: TransactionBackend + 'static>(backend: T) {
    let mut db = TransactionDatabase::new(backend);
    let key_manager = KeyManager::new_random().unwrap();
    let input = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&key_manager),
        MicroMinotari::from(100_000),
        &key_manager,
    )
    .unwrap();
    let constants = create_consensus_constants(0);
    let mut builder = TransactionBuilder::new(constants.clone(), key_manager.clone(), Network::LocalNet).unwrap();
    let amount = MicroMinotari::from(10_000);
    builder
        .with_fee_per_gram(MicroMinotari::from(177 / 5))
        .with_memo(MemoField::new_open_from_string("Yo!", TxType::PaymentToOther).unwrap())
        .with_input(input)
        .unwrap();

    let commitment_mask_key = key_manager.get_random_key(None, false).unwrap();
    let script_key_id = TariKeyId::Derived {
        key: (&commitment_mask_key.key_id).into(),
    };
    let public_script_key = key_manager.get_public_key_at_key_id(&script_key_id).unwrap();

    let sender_offset = key_manager.get_random_key(None, false).unwrap();
    let encrypted_data = key_manager
        .encrypt_data_for_recovery(
            &commitment_mask_key.key_id,
            None,
            amount.as_u64(),
            MemoField::new_empty(),
        )
        .unwrap();
    let output = WalletOutput::new(
        TransactionOutputVersion::get_current_version(),
        amount,
        commitment_mask_key.key_id.clone(),
        Default::default(),
        script!(Nop).unwrap(),
        inputs!(public_script_key),
        script_key_id,
        sender_offset.pub_key.clone(),
        Default::default(),
        0,
        Covenant::default(),
        encrypted_data,
        MicroMinotari::zero(),
        MemoField::new_empty(),
        &key_manager,
    )
    .unwrap();
    builder.with_output(output.clone(), sender_offset.key_id, None).unwrap();
    let finalized = builder.build().unwrap();

    let messages = ["Hey!", "Yo!", "Sup!"];
    let amounts = [
        MicroMinotari::from(10_000),
        MicroMinotari::from(23_000),
        MicroMinotari::from(5_000),
    ];

    let mut outbound_txs = Vec::new();

    for i in 0..messages.len() {
        let tx_id = TxId::from(i + 10);
        let address = TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        )
        .unwrap();
        outbound_txs.push(OutboundTransaction {
            tx_id,
            destination_address: address,
            amount: amounts[i],
            fee: finalized.fee,
            sender_protocol: SenderTransactionProtocol::new_placeholder(),
            status: LegacyTransactionStatus::Pending,
            payment_id: MemoField::new_open_from_string(messages[i], TxType::PaymentToOther).unwrap(),
            timestamp: Utc::now(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
            sent_output_hashes: vec![],
        });
        assert!(!db.transaction_exists(tx_id).unwrap(), "TxId should not exist");

        db.add_pending_outbound_transaction(outbound_txs[i].tx_id, outbound_txs[i].clone())
            .unwrap();

        assert!(db.transaction_exists(tx_id).unwrap(), "TxId should exist");
    }

    let retrieved_outbound_txs = db.get_pending_outbound_transactions().unwrap();
    assert_eq!(outbound_txs.len(), messages.len());
    for i in outbound_txs.iter().take(messages.len()) {
        let retrieved_outbound_tx = db.get_pending_outbound_transaction(i.tx_id).unwrap();
        assert_eq!(&retrieved_outbound_tx, i);
        assert_eq!(retrieved_outbound_tx.send_count, 0);
        assert!(retrieved_outbound_tx.last_send_timestamp.is_none());
        assert!(retrieved_outbound_txs.iter().any(|tx| tx == i));
    }

    db.increment_send_count(outbound_txs[0].tx_id).unwrap();
    let retrieved_outbound_tx = db.get_pending_outbound_transaction(outbound_txs[0].tx_id).unwrap();
    assert_eq!(retrieved_outbound_tx.send_count, 1);
    assert!(retrieved_outbound_tx.last_send_timestamp.is_some());

    let any_outbound_tx = db.get_any_transaction(outbound_txs[0].tx_id).unwrap().unwrap();
    if let WalletTransaction::PendingOutbound(tx) = any_outbound_tx {
        assert_eq!(tx, retrieved_outbound_tx);
    } else {
        panic!("Should have found outbound tx");
    }

    let messages = ["Hey!", "Yo!", "Sup!"];
    let amounts = [
        MicroMinotari::from(10_000),
        MicroMinotari::from(23_000),
        MicroMinotari::from(5_000),
    ];
    let mut inbound_txs = Vec::new();

    for i in 0..messages.len() {
        let address = TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        )
        .unwrap();
        let tx_id = TxId::from(i);
        inbound_txs.push(InboundTransaction {
            tx_id,
            source_address: address,
            amount: amounts[i],
            receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
            status: LegacyTransactionStatus::Pending,
            payment_id: MemoField::new_open_from_string(messages[i], TxType::PaymentToOther).unwrap(),
            timestamp: Utc::now(),
            cancelled: false,
            direct_send_success: false,
            send_count: 0,
            last_send_timestamp: None,
            received_output_hashes: vec![],
        });
        assert!(!db.transaction_exists(tx_id).unwrap(), "TxId should not exist");
        db.add_pending_inbound_transaction(tx_id, inbound_txs[i].clone())
            .unwrap();
        assert!(db.transaction_exists(tx_id).unwrap(), "TxId should exist");
    }

    let retrieved_inbound_txs = db.get_pending_inbound_transactions().unwrap();
    assert_eq!(inbound_txs.len(), messages.len());
    for i in inbound_txs.iter().take(messages.len()) {
        let retrieved_tx = retrieved_inbound_txs.iter().find(|tx| tx.tx_id == i.tx_id).unwrap();
        assert_eq!(&retrieved_tx, &i);
        assert_eq!(retrieved_tx.send_count, 0);
        assert!(retrieved_tx.last_send_timestamp.is_none());
    }

    db.increment_send_count(inbound_txs[0].tx_id).unwrap();
    let retrieved_inbound_tx = db.get_pending_inbound_transaction(inbound_txs[0].tx_id).unwrap();
    assert_eq!(retrieved_inbound_tx.send_count, 1);
    assert!(retrieved_inbound_tx.last_send_timestamp.is_some());

    let any_inbound_tx = db.get_any_transaction(inbound_txs[0].tx_id).unwrap().unwrap();
    if let WalletTransaction::PendingInbound(tx) = any_inbound_tx {
        assert_eq!(tx, retrieved_inbound_tx);
    } else {
        panic!("Should have found inbound tx");
    }

    let inbound_address = db
        .get_pending_transaction_counterparty_address_by_tx_id(inbound_txs[0].tx_id)
        .unwrap();
    assert_eq!(inbound_address, inbound_txs[0].source_address);

    assert!(db
        .get_pending_transaction_counterparty_address_by_tx_id(100u64.into())
        .is_err());

    let outbound_address = db
        .get_pending_transaction_counterparty_address_by_tx_id(outbound_txs[0].tx_id)
        .unwrap();
    assert_eq!(outbound_address, outbound_txs[0].destination_address);

    let mut completed_txs = Vec::new();
    let tx = Transaction::new(
        vec![],
        vec![],
        vec![],
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
    );

    for i in 0..messages.len() {
        let source_address = TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        )
        .unwrap();
        let dest_address = TariAddress::new_dual_address_with_default_features(
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        )
        .unwrap();
        completed_txs.push(CompletedTransaction {
            tx_id: outbound_txs[i].tx_id,
            source_address,
            destination_address: dest_address,
            amount: outbound_txs[i].amount,
            fee: MicroMinotari::from(200),
            transaction: tx.clone(),
            status: match i {
                0 => LegacyTransactionStatus::Completed,
                1 => LegacyTransactionStatus::Broadcast,
                _ => LegacyTransactionStatus::MinedUnconfirmed,
            },
            timestamp: Utc::now(),
            cancelled: None,
            direction: TransactionDirection::Outbound,
            send_count: 0,
            last_send_timestamp: None,

            transaction_signature: tx
                .first_kernel_excess_sig()
                .unwrap_or(&CompressedSignature::default())
                .clone(),
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
            payment_id: MemoField::new_open_from_string(messages[i], TxType::PaymentToOther).unwrap(),
            sent_output_hashes: vec![],
            change_output_hashes: vec![],
            received_output_hashes: vec![],
        });
        db.complete_outbound_transaction(outbound_txs[i].tx_id, completed_txs[i].clone())
            .unwrap();
        db.complete_inbound_transaction(inbound_txs[i].tx_id, CompletedTransaction {
            tx_id: inbound_txs[i].tx_id,
            ..completed_txs[i].clone()
        })
        .unwrap();
    }

    let retrieved_completed_txs = db.get_completed_transactions(None, None, None, 0).unwrap();
    assert_eq!(retrieved_completed_txs.len(), 2 * messages.len());

    for completed_tx in completed_txs.iter().take(messages.len()) {
        assert_eq!(
            retrieved_completed_txs
                .iter()
                .find(|tx| tx.tx_id == completed_tx.tx_id)
                .unwrap(),
            &CompletedTransaction {
                tx_id: completed_tx.tx_id,
                ..completed_tx.clone()
            }
        );
        assert_eq!(
            retrieved_completed_txs
                .iter()
                .find(|tx| tx.tx_id == completed_tx.tx_id)
                .unwrap(),
            completed_tx
        );
    }

    db.increment_send_count(completed_txs[0].tx_id).unwrap();
    db.increment_send_count(completed_txs[0].tx_id).unwrap();
    let retrieved_completed_tx = db.get_completed_transaction(completed_txs[0].tx_id).unwrap();
    assert_eq!(retrieved_completed_tx.send_count, 2);
    assert!(retrieved_completed_tx.last_send_timestamp.is_some());

    assert!(db.fetch_last_mined_transaction().unwrap().is_none());

    db.set_transaction_mined_height(
        completed_txs[0].tx_id,
        10,
        FixedHash::zero(),
        0,
        true,
        completed_txs[0].status,
    )
    .unwrap();

    assert_eq!(
        db.fetch_last_mined_transaction().unwrap().unwrap().tx_id,
        completed_txs[0].tx_id
    );

    let retrieved_completed_tx = db.get_completed_transaction(completed_txs[0].tx_id).unwrap();

    let any_completed_tx = db.get_any_transaction(completed_txs[0].tx_id).unwrap().unwrap();
    if let WalletTransaction::Completed(tx) = any_completed_tx {
        assert_eq!(tx, retrieved_completed_tx);
    } else {
        panic!("Should have found completed tx");
    }

    let completed_txs = db.get_completed_transactions(None, None, None, 0).unwrap();
    let num_completed_txs = completed_txs.len();
    assert_eq!(db.get_cancelled_completed_transactions(0).unwrap().len(), 0);

    let cancelled_tx_id = completed_txs[1].tx_id;
    assert!(db.get_cancelled_completed_transaction(cancelled_tx_id).is_err());
    db.reject_completed_transaction(cancelled_tx_id, TxCancellationReason::Unknown)
        .unwrap();
    let completed_txs = db.get_completed_transactions(None, None, None, 0).unwrap();
    assert_eq!(completed_txs.len(), num_completed_txs - 1);

    db.get_cancelled_completed_transaction(cancelled_tx_id)
        .expect("Should find cancelled transaction");

    let cancelled_txs = db.get_cancelled_completed_transactions(0).unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.iter().any(|c_tx| c_tx.tx_id == cancelled_tx_id));

    let any_cancelled_completed_tx = db.get_any_transaction(cancelled_tx_id).unwrap().unwrap();
    if let WalletTransaction::Completed(tx) = any_cancelled_completed_tx {
        assert_eq!(tx.tx_id, cancelled_tx_id);
    } else {
        panic!("Should have found cancelled completed tx");
    }

    // Transactions with empty kernel signatures should not be returned with this method, as those will be considered
    // as faux transactions (imported or one-sided)
    let unmined_txs = db.fetch_unconfirmed_transactions_info().unwrap();
    assert_eq!(unmined_txs.len(), 0);
}

#[tokio::test]
pub async fn test_transaction_service_sqlite_db() {
    let db_name = format!("{}.sqlite3", random::string(8));
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{db_folder}/{db_name}");
    let connection = run_migration_and_create_sqlite_connection(db_path, 16).unwrap();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let cipher = XChaCha20Poly1305::new(key_ga);

    test_db_backend(TransactionServiceSqliteDatabase::new(connection, cipher)).await;
}

#[tokio::test]
async fn import_tx_and_read_it_from_db() {
    let db_name = format!("{}.sqlite3", random::string(8));
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{db_folder}/{db_name}");
    let connection = run_migration_and_create_sqlite_connection(db_path, 16).unwrap();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let cipher = XChaCha20Poly1305::new(key_ga);
    let sqlite_db = TransactionServiceSqliteDatabase::new(connection, cipher);

    let transaction = CompletedTransaction::new(
        TxId::from(1u64),
        TariAddress::default(),
        TariAddress::default(),
        MicroMinotari::from(100000),
        MicroMinotari::from(0),
        Transaction::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        ),
        LegacyTransactionStatus::Imported,
        Utc::now(),
        TransactionDirection::Inbound,
        Some(5),
        Some(DateTime::from_timestamp(0, 0).unwrap()),
        MemoField::new_open_from_string("message", TxType::PaymentToOther).unwrap(),
    )
    .unwrap();

    sqlite_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            TxId::from(1u64),
            Box::new(transaction),
        )))
        .unwrap();

    let transaction = CompletedTransaction::new(
        TxId::from(2u64),
        TariAddress::default(),
        TariAddress::default(),
        MicroMinotari::from(100000),
        MicroMinotari::from(0),
        Transaction::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        ),
        LegacyTransactionStatus::OneSidedUnconfirmed,
        Utc::now(),
        TransactionDirection::Inbound,
        Some(6),
        Some(DateTime::from_timestamp(0, 0).unwrap()),
        MemoField::new_open_from_string("message", TxType::PaymentToOther).unwrap(),
    )
    .unwrap();

    sqlite_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            TxId::from(2u64),
            Box::new(transaction),
        )))
        .unwrap();

    let transaction = CompletedTransaction::new(
        TxId::from(3u64),
        TariAddress::default(),
        TariAddress::default(),
        MicroMinotari::from(100000),
        MicroMinotari::from(0),
        Transaction::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            PrivateKey::random(&mut OsRng),
            PrivateKey::random(&mut OsRng),
        ),
        LegacyTransactionStatus::OneSidedConfirmed,
        Utc::now(),
        TransactionDirection::Inbound,
        Some(7),
        Some(DateTime::from_timestamp(0, 0).unwrap()),
        MemoField::new_open_from_string("message", TxType::PaymentToOther).unwrap(),
    )
    .unwrap();

    sqlite_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            TxId::from(3u64),
            Box::new(transaction),
        )))
        .unwrap();

    let db_tx = sqlite_db.fetch_imported_transactions().unwrap();
    assert_eq!(db_tx.len(), 1);
    assert_eq!(db_tx.first().unwrap().tx_id, TxId::from(1u64));
    assert_eq!(db_tx.first().unwrap().mined_height, Some(5));

    let db_tx = sqlite_db.fetch_unconfirmed_detected_transactions().unwrap();
    assert_eq!(db_tx.len(), 1);
    assert_eq!(db_tx.first().unwrap().tx_id, TxId::from(2u64));
    assert_eq!(db_tx.first().unwrap().mined_height, Some(6));

    let db_tx = sqlite_db.fetch_confirmed_detected_transactions_from_height(10).unwrap();
    assert_eq!(db_tx.len(), 0);
    let db_tx = sqlite_db.fetch_confirmed_detected_transactions_from_height(4).unwrap();
    assert_eq!(db_tx.len(), 1);
    assert_eq!(db_tx.first().unwrap().tx_id, TxId::from(3u64));
    assert_eq!(db_tx.first().unwrap().mined_height, Some(7));
}
