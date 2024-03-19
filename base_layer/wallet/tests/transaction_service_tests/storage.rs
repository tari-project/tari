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

use core::default::Default;
use std::mem::size_of;

use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use chrono::{NaiveDateTime, Utc};
use minotari_wallet::{
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
    transaction::{TransactionDirection, TransactionStatus, TxId},
    types::{FixedHash, PrivateKey, PublicKey, Signature},
};
use tari_core::{
    covenants::Covenant,
    transactions::{
        key_manager::{create_memory_db_key_manager, TransactionKeyManagerBranch, TransactionKeyManagerInterface},
        tari_amount::{uT, MicroMinotari},
        test_helpers::{create_wallet_output_with_data, TestParams},
        transaction_components::{
            OutputFeatures,
            RangeProofType,
            Transaction,
            TransactionOutput,
            TransactionOutputVersion,
            WalletOutput,
        },
        transaction_protocol::sender::TransactionSenderMessage,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
use tari_key_manager::key_manager_service::KeyManagerInterface;
use tari_script::{inputs, script};
use tari_test_utils::random;
use tempfile::tempdir;

pub async fn test_db_backend<T: TransactionBackend + 'static>(backend: T) {
    let mut db = TransactionDatabase::new(backend);
    let key_manager = create_memory_db_key_manager();
    let input = create_wallet_output_with_data(
        script!(Nop),
        OutputFeatures::default(),
        &TestParams::new(&key_manager).await,
        MicroMinotari::from(100_000),
        &key_manager,
    )
    .await
    .unwrap();
    let constants = create_consensus_constants(0);
    let key_manager = create_memory_db_key_manager();
    let mut builder = SenderTransactionProtocol::builder(constants.clone(), key_manager.clone());
    let amount = MicroMinotari::from(10_000);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari::from(177 / 5))
        .with_message("Yo!".to_string())
        .with_input(input)
        .await
        .unwrap()
        .with_recipient_data(
            script!(Nop),
            Default::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            amount,
        )
        .await
        .unwrap();
    let change = TestParams::new(&key_manager).await;
    builder.with_change_data(
        script!(Nop),
        inputs!(change.script_key_pk),
        change.script_key_id.clone(),
        change.spend_key_id.clone(),
        Covenant::default(),
    );

    let stp = builder.build().await.unwrap();

    let messages = vec!["Hey!".to_string(), "Yo!".to_string(), "Sup!".to_string()];
    let amounts = [
        MicroMinotari::from(10_000),
        MicroMinotari::from(23_000),
        MicroMinotari::from(5_000),
    ];

    let mut outbound_txs = Vec::new();

    for i in 0..messages.len() {
        let tx_id = TxId::from(i + 10);
        let address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        outbound_txs.push(OutboundTransaction {
            tx_id,
            destination_address: address,
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

        assert_eq!(&retrieved_outbound_txs.get(&i.tx_id).unwrap(), &i);
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
    let sender = stp.clone().build_single_round_message(&key_manager).await.unwrap();
    let (spending_key_id, _) = key_manager
        .get_next_key(TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
        .await
        .unwrap();
    let (script_key_id, public_script_key) = key_manager
        .get_next_key(TransactionKeyManagerBranch::ScriptKey.get_branch_key())
        .await
        .unwrap();
    let encrypted_data = key_manager
        .encrypt_data_for_recovery(&spending_key_id, None, sender.amount.as_u64())
        .await
        .unwrap();
    let mut output = WalletOutput::new(
        TransactionOutputVersion::get_current_version(),
        sender.amount,
        spending_key_id.clone(),
        sender.features.clone(),
        sender.script.clone(),
        inputs!(public_script_key),
        script_key_id,
        sender.sender_offset_public_key.clone(),
        Default::default(),
        0,
        Covenant::default(),
        encrypted_data,
        MicroMinotari::zero(),
        &key_manager,
    )
    .await
    .unwrap();
    let output_message = TransactionOutput::metadata_signature_message(&output);
    output.metadata_signature = key_manager
        .get_receiver_partial_metadata_signature(
            &spending_key_id,
            &sender.amount.into(),
            &sender.sender_offset_public_key,
            &sender.ephemeral_public_nonce,
            &TransactionOutputVersion::get_current_version(),
            &output_message,
            RangeProofType::BulletProofPlus,
        )
        .await
        .unwrap();

    let rtp = ReceiverTransactionProtocol::new(
        TransactionSenderMessage::Single(Box::new(sender)),
        output,
        &key_manager,
        &constants,
    )
    .await;

    let mut inbound_txs = Vec::new();

    for i in 0..messages.len() {
        let address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let tx_id = TxId::from(i);
        inbound_txs.push(InboundTransaction {
            tx_id,
            source_address: address,
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
        assert!(!db.transaction_exists(tx_id).unwrap(), "TxId should not exist");
        db.add_pending_inbound_transaction(tx_id, inbound_txs[i].clone())
            .unwrap();
        assert!(db.transaction_exists(tx_id).unwrap(), "TxId should exist");
    }

    let retrieved_inbound_txs = db.get_pending_inbound_transactions().unwrap();
    assert_eq!(inbound_txs.len(), messages.len());
    for i in inbound_txs.iter().take(messages.len()) {
        let retrieved_tx = retrieved_inbound_txs.get(&i.tx_id).unwrap();
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
        let source_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        let dest_address = TariAddress::new(
            PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            Network::LocalNet,
        );
        completed_txs.push(CompletedTransaction {
            tx_id: outbound_txs[i].tx_id,
            source_address,
            destination_address: dest_address,
            amount: outbound_txs[i].amount,
            fee: MicroMinotari::from(200),
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
            send_count: 0,
            last_send_timestamp: None,

            transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
            confirmations: None,
            mined_height: None,
            mined_in_block: None,
            mined_timestamp: None,
        });
        db.complete_outbound_transaction(outbound_txs[i].tx_id, completed_txs[i].clone())
            .unwrap();
        db.complete_inbound_transaction(inbound_txs[i].tx_id, CompletedTransaction {
            tx_id: inbound_txs[i].tx_id,
            ..completed_txs[i].clone()
        })
        .unwrap();
    }

    let retrieved_completed_txs = db.get_completed_transactions().unwrap();
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

    db.increment_send_count(completed_txs[0].tx_id).unwrap();
    db.increment_send_count(completed_txs[0].tx_id).unwrap();
    let retrieved_completed_tx = db.get_completed_transaction(completed_txs[0].tx_id).unwrap();
    assert_eq!(retrieved_completed_tx.send_count, 2);
    assert!(retrieved_completed_tx.last_send_timestamp.is_some());
    assert!(retrieved_completed_tx.confirmations.is_none());

    assert!(db.fetch_last_mined_transaction().unwrap().is_none());

    db.set_transaction_mined_height(
        completed_txs[0].tx_id,
        10,
        FixedHash::zero(),
        0,
        5,
        true,
        &completed_txs[0].status,
    )
    .unwrap();

    assert_eq!(
        db.fetch_last_mined_transaction().unwrap().unwrap().tx_id,
        completed_txs[0].tx_id
    );

    let retrieved_completed_tx = db.get_completed_transaction(completed_txs[0].tx_id).unwrap();
    assert_eq!(retrieved_completed_tx.confirmations, Some(5));

    let any_completed_tx = db.get_any_transaction(completed_txs[0].tx_id).unwrap().unwrap();
    if let WalletTransaction::Completed(tx) = any_completed_tx {
        assert_eq!(tx, retrieved_completed_tx);
    } else {
        panic!("Should have found completed tx");
    }

    let completed_txs_map = db.get_completed_transactions().unwrap();
    let num_completed_txs = completed_txs_map.len();
    assert_eq!(db.get_cancelled_completed_transactions().unwrap().len(), 0);

    let cancelled_tx_id = completed_txs_map[&1u64.into()].tx_id;
    assert!(db.get_cancelled_completed_transaction(cancelled_tx_id).is_err());
    db.reject_completed_transaction(cancelled_tx_id, TxCancellationReason::Unknown)
        .unwrap();
    let completed_txs_map = db.get_completed_transactions().unwrap();
    assert_eq!(completed_txs_map.len(), num_completed_txs - 1);

    db.get_cancelled_completed_transaction(cancelled_tx_id)
        .expect("Should find cancelled transaction");

    let mut cancelled_txs = db.get_cancelled_completed_transactions().unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&cancelled_tx_id).is_some());

    let any_cancelled_completed_tx = db.get_any_transaction(cancelled_tx_id).unwrap().unwrap();
    if let WalletTransaction::Completed(tx) = any_cancelled_completed_tx {
        assert_eq!(tx.tx_id, cancelled_tx_id);
    } else {
        panic!("Should have found cancelled completed tx");
    }
    let address = TariAddress::new(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    db.add_pending_inbound_transaction(
        999u64.into(),
        InboundTransaction::new(
            999u64.into(),
            address,
            22 * uT,
            rtp,
            TransactionStatus::Pending,
            "To be cancelled".to_string(),
            Utc::now().naive_utc(),
        ),
    )
    .unwrap();

    assert_eq!(db.get_cancelled_pending_inbound_transactions().unwrap().len(), 0);

    assert_eq!(db.get_pending_inbound_transactions().unwrap().len(), 1);
    assert!(
        !db.get_pending_inbound_transaction(999u64.into())
            .unwrap()
            .direct_send_success
    );
    db.mark_direct_send_success(999u64.into()).unwrap();
    assert!(
        db.get_pending_inbound_transaction(999u64.into())
            .unwrap()
            .direct_send_success
    );
    assert!(db.get_cancelled_pending_inbound_transaction(999u64.into()).is_err());
    db.cancel_pending_transaction(999u64.into()).unwrap();
    db.get_cancelled_pending_inbound_transaction(999u64.into())
        .expect("Should find cancelled inbound tx");

    assert_eq!(db.get_cancelled_pending_inbound_transactions().unwrap().len(), 1);

    assert_eq!(db.get_pending_inbound_transactions().unwrap().len(), 0);

    let any_cancelled_inbound_tx = db.get_any_transaction(999u64.into()).unwrap().unwrap();
    if let WalletTransaction::PendingInbound(tx) = any_cancelled_inbound_tx {
        assert_eq!(tx.tx_id, TxId::from(999u64));
    } else {
        panic!("Should have found cancelled inbound tx");
    }

    let mut cancelled_txs = db.get_cancelled_pending_inbound_transactions().unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&999u64.into()).is_some());
    let address = TariAddress::new(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    db.add_pending_outbound_transaction(
        998u64.into(),
        OutboundTransaction::new(
            998u64.into(),
            address,
            22 * uT,
            stp.get_fee_amount().unwrap(),
            stp,
            TransactionStatus::Pending,
            "To be cancelled".to_string(),
            Utc::now().naive_utc(),
            false,
        ),
    )
    .unwrap();

    assert!(
        !db.get_pending_outbound_transaction(998u64.into())
            .unwrap()
            .direct_send_success
    );
    db.mark_direct_send_success(998u64.into()).unwrap();
    assert!(
        db.get_pending_outbound_transaction(998u64.into())
            .unwrap()
            .direct_send_success
    );

    assert_eq!(db.get_cancelled_pending_outbound_transactions().unwrap().len(), 0);

    assert_eq!(db.get_pending_outbound_transactions().unwrap().len(), 1);

    assert!(db.get_cancelled_pending_outbound_transaction(998u64.into()).is_err());

    db.cancel_pending_transaction(998u64.into()).unwrap();
    db.get_cancelled_pending_outbound_transaction(998u64.into())
        .expect("Should find cancelled outbound tx");
    assert_eq!(db.get_cancelled_pending_outbound_transactions().unwrap().len(), 1);

    assert_eq!(db.get_pending_outbound_transactions().unwrap().len(), 0);

    let mut cancelled_txs = db.get_cancelled_pending_outbound_transactions().unwrap();
    assert_eq!(cancelled_txs.len(), 1);
    assert!(cancelled_txs.remove(&998u64.into()).is_some());

    let any_cancelled_outbound_tx = db.get_any_transaction(998u64.into()).unwrap().unwrap();
    if let WalletTransaction::PendingOutbound(tx) = any_cancelled_outbound_tx {
        assert_eq!(tx.tx_id, TxId::from(998u64));
    } else {
        panic!("Should have found cancelled outbound tx");
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
    let db_path = format!("{}/{}", db_folder, db_name);
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
    let db_path = format!("{}/{}", db_folder, db_name);
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
        TransactionStatus::Imported,
        "message".to_string(),
        Utc::now().naive_utc(),
        TransactionDirection::Inbound,
        Some(5),
        Some(NaiveDateTime::from_timestamp_opt(0, 0).unwrap()),
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
        TransactionStatus::OneSidedUnconfirmed,
        "message".to_string(),
        Utc::now().naive_utc(),
        TransactionDirection::Inbound,
        Some(6),
        Some(NaiveDateTime::from_timestamp_opt(0, 0).unwrap()),
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
        TransactionStatus::OneSidedConfirmed,
        "message".to_string(),
        Utc::now().naive_utc(),
        TransactionDirection::Inbound,
        Some(7),
        Some(NaiveDateTime::from_timestamp_opt(0, 0).unwrap()),
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
