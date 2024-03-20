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

use std::convert::TryFrom;

use minotari_wallet::output_manager_service::{
    error::OutputManagerStorageError,
    service::Balance,
    storage::{
        database::{OutputManagerBackend, OutputManagerDatabase},
        models::DbWalletOutput,
        sqlite_db::{OutputManagerSqliteDatabase, ReceivedOutputInfoForBatch, SpentOutputInfoForBatch},
        OutputSource,
        OutputStatus,
    },
};
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{
    transaction::TxId,
    types::{FixedHash, HashOutput, PrivateKey},
};
use tari_core::transactions::{
    key_manager::create_memory_db_key_manager,
    tari_amount::MicroMinotari,
    transaction_components::OutputFeatures,
};
use tari_crypto::keys::SecretKey;
use tari_utilities::{hex::Hex, ByteArray};

use crate::support::{data::get_temp_sqlite_database_connection, utils::make_input};

#[allow(clippy::too_many_lines)]
pub async fn test_db_backend<T: OutputManagerBackend + 'static>(backend: T) {
    let db = OutputManagerDatabase::new(backend);

    // Add some unspent outputs
    let mut unspent_outputs = Vec::new();
    let key_manager = create_memory_db_key_manager();
    let mut unspent = Vec::with_capacity(5);
    for i in 0..5 {
        let uo = make_input(
            &mut OsRng,
            MicroMinotari::from(100 + OsRng.next_u64() % 1000),
            &OutputFeatures::default(),
            &key_manager,
        )
        .await;
        let mut kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
            .await
            .unwrap();
        kmo.wallet_output.features.maturity = i;
        db.add_unspent_output(kmo.clone()).unwrap();
        unspent.push((kmo.hash, true));
        unspent_outputs.push(kmo);
    }
    db.mark_outputs_as_unspent(unspent).unwrap();

    let time_locked_outputs = db.get_timelocked_outputs(3).unwrap();
    assert_eq!(time_locked_outputs.len(), 1);
    assert_eq!(unspent_outputs[4], time_locked_outputs[0]);
    let time_locked_outputs = db.get_timelocked_outputs(4).unwrap();
    assert_eq!(time_locked_outputs.len(), 0);
    let time_locked_balance = unspent_outputs[4].wallet_output.value;

    for i in 0..4usize {
        let balance = db.get_balance(Some(i as u64)).unwrap();
        let mut sum = MicroMinotari::from(0);
        for output in unspent_outputs.iter().take(5).skip(i + 1) {
            sum += output.wallet_output.value;
        }
        assert_eq!(balance.time_locked_balance.unwrap(), sum);
    }

    unspent_outputs.sort();

    let outputs = db.fetch_sorted_unspent_outputs().unwrap();
    assert_eq!(unspent_outputs, outputs);

    // Add some sent transactions with outputs to be spent and received
    struct PendingTransactionOutputs {
        tx_id: TxId,
        outputs_to_be_spent: Vec<DbWalletOutput>,
        outputs_to_be_received: Vec<DbWalletOutput>,
    }

    let mut pending_txs = Vec::new();
    for _ in 0..3 {
        let mut pending_tx = PendingTransactionOutputs {
            tx_id: TxId::new_random(),
            outputs_to_be_spent: vec![],
            outputs_to_be_received: vec![],
        };
        for _ in 0..4 {
            let kmo = make_input(
                &mut OsRng,
                MicroMinotari::from(100 + OsRng.next_u64() % 1000),
                &OutputFeatures::default(),
                &key_manager,
            )
            .await;
            let kmo = DbWalletOutput::from_wallet_output(kmo, &key_manager, None, OutputSource::Standard, None, None)
                .await
                .unwrap();
            db.add_unspent_output(kmo.clone()).unwrap();
            db.mark_outputs_as_unspent(vec![(kmo.hash, true)]).unwrap();
            pending_tx.outputs_to_be_spent.push(kmo);
        }
        for _ in 0..2 {
            let uo = make_input(
                &mut OsRng,
                MicroMinotari::from(100 + OsRng.next_u64() % 1000),
                &OutputFeatures::default(),
                &key_manager,
            )
            .await;
            let kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
                .await
                .unwrap();
            pending_tx.outputs_to_be_received.push(kmo);
        }
        db.encumber_outputs(
            pending_tx.tx_id,
            pending_tx.outputs_to_be_spent.clone(),
            pending_tx.outputs_to_be_received.clone(),
        )
        .unwrap();
        pending_txs.push(pending_tx);
    }

    // Test balance calc
    let available_balance = unspent_outputs
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
    let mut pending_incoming_balance = MicroMinotari(0);
    let mut pending_outgoing_balance = MicroMinotari(0);
    for v in &pending_txs {
        pending_outgoing_balance += v
            .outputs_to_be_spent
            .iter()
            .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
        pending_incoming_balance += v
            .outputs_to_be_received
            .iter()
            .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
    }

    let balance = db.get_balance(None).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    let balance = db.get_balance(Some(3)).unwrap();
    assert_eq!(balance, Balance {
        available_balance: available_balance - time_locked_balance,
        time_locked_balance: Some(time_locked_balance),
        pending_incoming_balance,
        pending_outgoing_balance
    });

    for v in &pending_txs {
        db.confirm_encumbered_outputs(v.tx_id).unwrap();
    }

    let balance = db.get_balance(None).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    // Set first pending tx to mined but unconfirmed
    let mut updates = Vec::new();
    for o in &pending_txs[0].outputs_to_be_received {
        updates.push(ReceivedOutputInfoForBatch {
            commitment: o.commitment.clone(),
            mined_height: 2,
            mined_in_block: FixedHash::zero(),
            confirmed: false,
            mined_timestamp: 0,
        });
    }
    db.set_received_outputs_mined_height_and_statuses(updates).unwrap();
    let mut spent = Vec::new();
    for o in &pending_txs[0].outputs_to_be_spent {
        spent.push(SpentOutputInfoForBatch {
            commitment: o.commitment.clone(),
            confirmed: false,
            mark_deleted_at_height: 3,
            mark_deleted_in_block: FixedHash::zero(),
        });
    }
    db.mark_outputs_as_spent(spent).unwrap();

    // Balance shouldn't change
    let balance = db.get_balance(None).unwrap();

    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    // Set second pending tx to mined and confirmed
    let mut updates = Vec::new();
    for o in &pending_txs[1].outputs_to_be_received {
        updates.push(ReceivedOutputInfoForBatch {
            commitment: o.commitment.clone(),
            mined_height: 4,
            mined_in_block: FixedHash::zero(),
            confirmed: true,
            mined_timestamp: 0,
        });
    }
    db.set_received_outputs_mined_height_and_statuses(updates).unwrap();
    let mut spent = Vec::new();
    for o in &pending_txs[1].outputs_to_be_spent {
        spent.push(SpentOutputInfoForBatch {
            commitment: o.commitment.clone(),
            confirmed: true,
            mark_deleted_at_height: 5,
            mark_deleted_in_block: FixedHash::zero(),
        });
    }
    db.mark_outputs_as_spent(spent).unwrap();

    // Balance with confirmed second pending tx
    let mut available_balance = unspent_outputs
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
    let mut pending_incoming_balance = MicroMinotari(0);
    let mut pending_outgoing_balance = MicroMinotari(0);

    pending_outgoing_balance += pending_txs[0]
        .outputs_to_be_spent
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
    pending_outgoing_balance += pending_txs[2]
        .outputs_to_be_spent
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
    pending_incoming_balance += pending_txs[0]
        .outputs_to_be_received
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);
    pending_incoming_balance += pending_txs[2]
        .outputs_to_be_received
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);

    available_balance += pending_txs[1]
        .outputs_to_be_received
        .iter()
        .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value);

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance,
        Balance {
            available_balance,
            time_locked_balance: None,
            pending_incoming_balance,
            pending_outgoing_balance
        },
        "Balance should change"
    );

    // Add output to be received
    let uo = make_input(
        &mut OsRng,
        MicroMinotari::from(100 + OsRng.next_u64() % 1000),
        &OutputFeatures::default(),
        &key_manager,
    )
    .await;
    let output_to_be_received =
        DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
            .await
            .unwrap();
    db.add_output_to_be_received(TxId::from(11u64), output_to_be_received.clone())
        .unwrap();
    pending_incoming_balance += output_to_be_received.wallet_output.value;

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance,
        Balance {
            available_balance,
            time_locked_balance: None,
            pending_incoming_balance,
            pending_outgoing_balance
        },
        "Balance should reflect new output to be received"
    );

    let spent_outputs = db.fetch_spent_outputs().unwrap();
    assert_eq!(spent_outputs.len(), 4);

    let unconfirmed_outputs = db.fetch_unconfirmed_outputs().unwrap();
    assert_eq!(unconfirmed_outputs.len(), 22);

    let mined_unspent_outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(mined_unspent_outputs.len(), 4);

    // Spend a received and confirmed output
    db.mark_outputs_as_spent(vec![SpentOutputInfoForBatch {
        commitment: pending_txs[1].outputs_to_be_received[0].commitment.clone(),
        confirmed: true,
        mark_deleted_at_height: 6,
        mark_deleted_in_block: FixedHash::zero(),
    }])
    .unwrap();

    let mined_unspent_outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(mined_unspent_outputs.len(), 3);

    let unspent_outputs = db.fetch_sorted_unspent_outputs().unwrap();
    assert_eq!(unspent_outputs.len(), 6);

    let last_mined_output = db.get_last_mined_output().unwrap().unwrap();
    assert!(pending_txs[1]
        .outputs_to_be_received
        .iter()
        .any(|o| o.commitment == last_mined_output.commitment));

    let last_spent_output = db.get_last_spent_output().unwrap().unwrap();
    assert_eq!(
        last_spent_output.commitment,
        pending_txs[1].outputs_to_be_received[0].commitment
    );

    db.remove_output_by_commitment(last_spent_output.commitment).unwrap();
    let last_spent_output = db.get_last_spent_output().unwrap().unwrap();
    assert_ne!(
        last_spent_output.commitment,
        pending_txs[1].outputs_to_be_received[0].commitment
    );

    // Test cancelling a pending transaction
    db.cancel_pending_transaction_outputs(pending_txs[2].tx_id).unwrap();

    let unspent_outputs = db.fetch_sorted_unspent_outputs().unwrap();
    assert_eq!(unspent_outputs.len(), 10);
}

#[tokio::test]
pub async fn test_output_manager_sqlite_db() {
    //` cargo test --release --test
    //` wallet_integration_tests output_manager_service_tests::storage::test_output_manager_sqlite_db
    //` > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    test_db_backend(OutputManagerSqliteDatabase::new(connection)).await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
pub async fn test_raw_custom_queries_regression() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection);
    let db = OutputManagerDatabase::new(backend);

    // Add some unspent outputs
    let mut unspent_outputs = Vec::new();
    let key_manager = create_memory_db_key_manager();
    let mut unspent = Vec::with_capacity(5);
    for i in 0..5 {
        let uo = make_input(
            &mut OsRng,
            MicroMinotari::from(100 + OsRng.next_u64() % 1000),
            &OutputFeatures::default(),
            &key_manager,
        )
        .await;
        let mut kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
            .await
            .unwrap();
        kmo.wallet_output.features.maturity = i;
        db.add_unspent_output(kmo.clone()).unwrap();
        unspent.push((kmo.hash, true));
        unspent_outputs.push(kmo);
    }

    let unknown = HashOutput::try_from(PrivateKey::random(&mut rand::thread_rng()).as_bytes()).unwrap();
    let mut unspent_with_unknown = unspent.clone();
    unspent_with_unknown.push((unknown, true));
    assert!(db.mark_outputs_as_unspent(unspent_with_unknown).is_err());

    db.mark_outputs_as_unspent(unspent).unwrap();

    // Add some sent transactions with outputs to be spent and received
    struct PendingTransactionOutputs {
        tx_id: TxId,
        outputs_to_be_spent: Vec<DbWalletOutput>,
        outputs_to_be_received: Vec<DbWalletOutput>,
    }

    let mut pending_txs = Vec::new();
    for _ in 0..3 {
        let mut pending_tx = PendingTransactionOutputs {
            tx_id: TxId::new_random(),
            outputs_to_be_spent: vec![],
            outputs_to_be_received: vec![],
        };
        for _ in 0..4 {
            let kmo = make_input(
                &mut OsRng,
                MicroMinotari::from(100 + OsRng.next_u64() % 1000),
                &OutputFeatures::default(),
                &key_manager,
            )
            .await;
            let kmo = DbWalletOutput::from_wallet_output(kmo, &key_manager, None, OutputSource::Standard, None, None)
                .await
                .unwrap();
            db.add_unspent_output(kmo.clone()).unwrap();
            db.mark_outputs_as_unspent(vec![(kmo.hash, true)]).unwrap();
            pending_tx.outputs_to_be_spent.push(kmo);
        }
        for _ in 0..2 {
            let uo = make_input(
                &mut OsRng,
                MicroMinotari::from(100 + OsRng.next_u64() % 1000),
                &OutputFeatures::default(),
                &key_manager,
            )
            .await;
            let kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
                .await
                .unwrap();
            pending_tx.outputs_to_be_received.push(kmo);
        }
        db.encumber_outputs(
            pending_tx.tx_id,
            pending_tx.outputs_to_be_spent.clone(),
            pending_tx.outputs_to_be_received.clone(),
        )
        .unwrap();
        pending_txs.push(pending_tx);
    }
    for v in &pending_txs {
        db.confirm_encumbered_outputs(v.tx_id).unwrap();
    }

    // Custom query test section
    // - `set_received_outputs_mined_height_and_statuses`

    let mut updates_info = Vec::new();
    let mut block_hashes = Vec::new();
    for (i, to_be_received) in pending_txs[0].outputs_to_be_received.iter().enumerate() {
        let k = PrivateKey::random(&mut OsRng);
        let mined_in_block = FixedHash::from_hex(&k.to_hex()).unwrap();
        block_hashes.push(mined_in_block);
        updates_info.push(ReceivedOutputInfoForBatch {
            commitment: to_be_received.commitment.clone(),
            mined_height: (i + 2) as u64,
            mined_in_block,
            confirmed: i % 2 == 0,
            mined_timestamp: 0,
        });
    }

    let uo = make_input(
        &mut OsRng,
        MicroMinotari::from(100 + OsRng.next_u64() % 1000),
        &OutputFeatures::default(),
        &key_manager,
    )
    .await;
    let unknown = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
        .await
        .unwrap();
    let mut updates_info_with_unknown = updates_info.clone();
    updates_info_with_unknown.push(ReceivedOutputInfoForBatch {
        commitment: unknown.commitment.clone(),
        mined_height: 2,
        mined_in_block: block_hashes[0],
        confirmed: true,
        mined_timestamp: 0,
    });
    assert!(db
        .set_received_outputs_mined_height_and_statuses(updates_info_with_unknown)
        .is_err());

    db.set_received_outputs_mined_height_and_statuses(updates_info).unwrap();

    for (i, to_be_received) in pending_txs[0].outputs_to_be_received.iter().enumerate() {
        let unspent_output = db.fetch_by_commitment(to_be_received.commitment.clone()).unwrap();
        assert_eq!(unspent_output.mined_height.unwrap(), (i + 2) as u64);
        assert_eq!(unspent_output.mined_in_block.unwrap(), block_hashes[i]);
        assert_eq!(
            unspent_output.status,
            if i % 2 == 0 {
                OutputStatus::Unspent
            } else {
                OutputStatus::UnspentMinedUnconfirmed
            }
        );
    }

    // - `mark_outputs_as_spent`

    let mut updates_info = Vec::new();
    let mut block_hashes = Vec::new();
    for (i, to_be_spent) in pending_txs[0].outputs_to_be_spent.iter().enumerate() {
        let k = PrivateKey::random(&mut OsRng);
        let mark_deleted_in_block = FixedHash::from_hex(&k.to_hex()).unwrap();
        block_hashes.push(mark_deleted_in_block);
        updates_info.push(SpentOutputInfoForBatch {
            commitment: to_be_spent.commitment.clone(),
            confirmed: i % 2 == 0,
            mark_deleted_at_height: (i + 3) as u64,
            mark_deleted_in_block,
        });
    }

    let mut updates_info_with_unknown = updates_info.clone();
    updates_info_with_unknown.push(SpentOutputInfoForBatch {
        commitment: unknown.commitment,
        confirmed: true,
        mark_deleted_at_height: 4,
        mark_deleted_in_block: block_hashes[0],
    });
    assert!(db.mark_outputs_as_spent(updates_info_with_unknown).is_err());

    db.mark_outputs_as_spent(updates_info).unwrap();

    for (i, to_be_spent) in pending_txs[0].outputs_to_be_spent.iter().enumerate() {
        let spent_output = db.fetch_by_commitment(to_be_spent.commitment.clone()).unwrap();
        assert_eq!(spent_output.marked_deleted_at_height.unwrap(), (i + 3) as u64);
        assert_eq!(spent_output.marked_deleted_in_block.unwrap(), block_hashes[i]);
        assert_eq!(
            spent_output.status,
            if i % 2 == 0 {
                OutputStatus::Spent
            } else {
                OutputStatus::SpentMinedUnconfirmed
            }
        );
    }
}

#[tokio::test]
pub async fn test_short_term_encumberance() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection);
    let db = OutputManagerDatabase::new(backend);

    let mut unspent_outputs = Vec::new();
    let key_manager = create_memory_db_key_manager();
    for i in 0..5 {
        let kmo = make_input(
            &mut OsRng,
            MicroMinotari::from(100 + OsRng.next_u64() % 1000),
            &OutputFeatures::default(),
            &key_manager,
        )
        .await;
        let mut kmo = DbWalletOutput::from_wallet_output(kmo, &key_manager, None, OutputSource::Standard, None, None)
            .await
            .unwrap();
        kmo.wallet_output.features.maturity = i;
        db.add_unspent_output(kmo.clone()).unwrap();
        db.mark_outputs_as_unspent(vec![(kmo.hash, true)]).unwrap();
        unspent_outputs.push(kmo);
    }

    db.encumber_outputs(1u64.into(), unspent_outputs[0..=2].to_vec(), vec![])
        .unwrap();

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance.available_balance,
        unspent_outputs[3..5]
            .iter()
            .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value)
    );

    db.clear_short_term_encumberances().unwrap();

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance.available_balance,
        unspent_outputs
            .iter()
            .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value)
    );

    db.encumber_outputs(2u64.into(), unspent_outputs[0..=2].to_vec(), vec![])
        .unwrap();

    db.confirm_encumbered_outputs(TxId::from(2u64)).unwrap();
    db.clear_short_term_encumberances().unwrap();

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance.available_balance,
        unspent_outputs[3..5]
            .iter()
            .fold(MicroMinotari::from(0), |acc, x| acc + x.wallet_output.value)
    );
}

#[tokio::test]
pub async fn test_no_duplicate_outputs() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection);
    let db = OutputManagerDatabase::new(backend);

    // create an output
    let key_manager = create_memory_db_key_manager();
    let uo = make_input(
        &mut OsRng,
        MicroMinotari::from(1000),
        &OutputFeatures::default(),
        &key_manager,
    )
    .await;
    let kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
        .await
        .unwrap();

    // add it to the database
    let result = db.add_unspent_output(kmo.clone());
    assert!(result.is_ok());
    let result = db.set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
        commitment: kmo.commitment.clone(),
        mined_height: 1,
        mined_in_block: FixedHash::zero(),
        confirmed: true,
        mined_timestamp: 0,
    }]);
    assert!(result.is_ok());
    let outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(outputs.len(), 1);

    // adding it again should be an error
    let err = db.add_unspent_output(kmo.clone()).unwrap_err();
    assert!(matches!(err, OutputManagerStorageError::DuplicateOutput));
    let outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(outputs.len(), 1);

    // add a pending transaction with the same duplicate output

    assert!(db.encumber_outputs(2u64.into(), vec![], vec![kmo]).is_err());

    // we should still only have 1 unspent output
    let outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(outputs.len(), 1);
}

#[tokio::test]
pub async fn test_mark_as_unmined() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection);
    let db = OutputManagerDatabase::new(backend);

    // create an output
    let key_manager = create_memory_db_key_manager();
    let uo = make_input(
        &mut OsRng,
        MicroMinotari::from(1000),
        &OutputFeatures::default(),
        &key_manager,
    )
    .await;
    let kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
        .await
        .unwrap();

    // add it to the database
    db.add_unspent_output(kmo.clone()).unwrap();
    db.set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
        commitment: kmo.commitment.clone(),
        mined_height: 1,
        mined_in_block: FixedHash::zero(),
        confirmed: true,
        mined_timestamp: 0,
    }])
    .unwrap();
    let o = db.get_last_mined_output().unwrap().unwrap();
    assert_eq!(o.hash, kmo.hash);
    db.set_outputs_to_unmined_and_invalid(vec![kmo.hash]).unwrap();
    assert!(db.get_last_mined_output().unwrap().is_none());
    let o = db.get_invalid_outputs().unwrap().pop().unwrap();
    assert_eq!(o.hash, kmo.hash);
    assert!(o.mined_height.is_none());
    assert!(o.mined_in_block.is_none());

    // Test batch mode operations
    // - Add 5 outputs and remember the hashes
    let batch_count = 7usize;
    let mut batch_hashes = Vec::with_capacity(batch_count);
    let mut batch_outputs = Vec::with_capacity(batch_count);
    let mut batch_info = Vec::with_capacity(batch_count);
    for i in 0..batch_count {
        let uo = make_input(
            &mut OsRng,
            MicroMinotari::from(1000),
            &OutputFeatures::default(),
            &key_manager,
        )
        .await;
        let kmo = DbWalletOutput::from_wallet_output(uo, &key_manager, None, OutputSource::Standard, None, None)
            .await
            .unwrap();
        db.add_unspent_output(kmo.clone()).unwrap();
        batch_hashes.push(kmo.hash);
        batch_info.push(ReceivedOutputInfoForBatch {
            commitment: kmo.commitment.clone(),
            mined_height: i as u64 + 1,
            mined_in_block: FixedHash::zero(),
            confirmed: true,
            mined_timestamp: i as u64,
        });
        batch_outputs.push(kmo);
    }

    // - Perform batch mode operations
    db.set_received_outputs_mined_height_and_statuses(batch_info).unwrap();

    let last = db.get_last_mined_output().unwrap().unwrap();
    assert_eq!(last.hash, batch_outputs.last().unwrap().hash);

    db.set_outputs_to_unmined_and_invalid(batch_hashes).unwrap();
    assert!(db.get_last_mined_output().unwrap().is_none());

    let invalid_outputs = db.get_invalid_outputs().unwrap();
    let mut batch_invalid_count = 0;
    for invalid in invalid_outputs {
        if let Some(kmo) = batch_outputs.iter().find(|wo| wo.hash == invalid.hash) {
            assert_eq!(invalid.hash, kmo.hash);
            assert!(invalid.mined_height.is_none());
            assert!(invalid.mined_in_block.is_none());
            batch_invalid_count += 1;
        }
    }
    assert_eq!(batch_invalid_count, batch_count);
}
