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

use crate::support::utils::{make_input, random_string};
use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use chrono::{Duration as ChronoDuration, Utc};
use diesel::result::{DatabaseErrorKind, Error::DatabaseError};
use rand::{rngs::OsRng, RngCore};
use std::time::Duration;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::OutputFeatures,
    types::{CryptoFactories, PrivateKey},
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey};
use tari_wallet::{
    output_manager_service::{
        error::OutputManagerStorageError,
        service::Balance,
        storage::{
            database::{KeyManagerState, OutputManagerBackend, OutputManagerDatabase, PendingTransactionOutputs},
            memory_db::OutputManagerMemoryDatabase,
            models::DbUnblindedOutput,
            sqlite_db::OutputManagerSqliteDatabase,
        },
    },
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
};
use tempfile::tempdir;
use tokio::runtime::Runtime;

pub fn test_db_backend<T: OutputManagerBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();
    let db = OutputManagerDatabase::new(backend);
    let factories = CryptoFactories::default();

    // Add some unspent outputs
    let mut unspent_outputs = Vec::new();
    for i in 0..5 {
        let (_ti, uo) = make_input(
            &mut OsRng,
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        let mut uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
        uo.unblinded_output.features.maturity = i;
        runtime.block_on(db.add_unspent_output(uo.clone())).unwrap();
        unspent_outputs.push(uo);
    }

    let time_locked_outputs = runtime.block_on(db.get_timelocked_outputs(3)).unwrap();
    assert_eq!(time_locked_outputs.len(), 1);
    assert_eq!(unspent_outputs[4], time_locked_outputs[0]);
    let time_locked_outputs = runtime.block_on(db.get_timelocked_outputs(4)).unwrap();
    assert_eq!(time_locked_outputs.len(), 0);
    let time_locked_balance = unspent_outputs[4].unblinded_output.value;

    unspent_outputs.sort();
    // Add some pending transactions
    let mut pending_txs = Vec::new();
    for i in 0..3 {
        let mut pending_tx = PendingTransactionOutputs {
            tx_id: OsRng.next_u64(),
            outputs_to_be_spent: vec![],
            outputs_to_be_received: vec![],
            timestamp: Utc::now().naive_utc() -
                ChronoDuration::from_std(Duration::from_millis(120_000_000 * i)).unwrap(),
            coinbase_block_height: None,
        };
        for _ in 0..(OsRng.next_u64() % 5 + 1) {
            let (_ti, uo) = make_input(
                &mut OsRng,
                MicroTari::from(100 + OsRng.next_u64() % 1000),
                &factories.commitment,
            );
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
            pending_tx.outputs_to_be_spent.push(uo);
        }
        for _ in 0..(OsRng.next_u64() % 5 + 1) {
            let (_ti, uo) = make_input(
                &mut OsRng,
                MicroTari::from(100 + OsRng.next_u64() % 1000),
                &factories.commitment,
            );
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
            pending_tx.outputs_to_be_received.push(uo);
        }
        runtime
            .block_on(db.add_pending_transaction_outputs(pending_tx.clone()))
            .unwrap();
        pending_txs.push(pending_tx);
    }

    let outputs = runtime.block_on(db.fetch_sorted_unspent_outputs()).unwrap();
    assert_eq!(unspent_outputs, outputs);

    let p_tx = runtime.block_on(db.fetch_all_pending_transaction_outputs()).unwrap();

    for (k, v) in p_tx.iter() {
        assert_eq!(v, pending_txs.iter().find(|i| &i.tx_id == k).unwrap());
    }

    assert_eq!(
        runtime
            .block_on(db.fetch_pending_transaction_outputs(pending_txs[0].tx_id))
            .unwrap(),
        pending_txs[0]
    );

    // Test balance calc
    let mut available_balance = unspent_outputs
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    let mut pending_incoming_balance = MicroTari(0);
    let mut pending_outgoing_balance = MicroTari(0);
    for v in pending_txs.iter() {
        pending_outgoing_balance += v
            .outputs_to_be_spent
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
        pending_incoming_balance += v
            .outputs_to_be_received
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    }

    let balance = runtime.block_on(db.get_balance(None)).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    let balance = runtime.block_on(db.get_balance(Some(3))).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: Some(time_locked_balance),
        pending_incoming_balance,
        pending_outgoing_balance
    });

    runtime
        .block_on(db.confirm_pending_transaction_outputs(pending_txs[0].tx_id))
        .unwrap();

    available_balance += pending_txs[0]
        .outputs_to_be_received
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

    pending_incoming_balance -= pending_txs[0]
        .outputs_to_be_received
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

    pending_outgoing_balance -= pending_txs[0]
        .outputs_to_be_spent
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

    let balance = runtime.block_on(db.get_balance(None)).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    let spent_outputs = runtime.block_on(db.fetch_spent_outputs()).unwrap();

    assert!(spent_outputs.len() > 0);
    assert_eq!(
        spent_outputs
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value),
        pending_txs[0]
            .outputs_to_be_spent
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value)
    );

    let (_ti, uo_change) = make_input(
        &mut OsRng.clone(),
        MicroTari::from(100 + OsRng.next_u64() % 1000),
        &factories.commitment,
    );
    let uo_change = DbUnblindedOutput::from_unblinded_output(uo_change, &factories).unwrap();
    let outputs_to_encumber = vec![outputs[0].clone(), outputs[1].clone()];
    let total_encumbered = outputs[0].clone().unblinded_output.value + outputs[1].clone().unblinded_output.value;
    runtime
        .block_on(db.encumber_outputs(2, outputs_to_encumber, vec![uo_change.clone()]))
        .unwrap();
    runtime.block_on(db.confirm_encumbered_outputs(2)).unwrap();

    available_balance -= total_encumbered;
    pending_incoming_balance += uo_change.clone().unblinded_output.value;
    pending_outgoing_balance += total_encumbered;

    let balance = runtime.block_on(db.get_balance(None)).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    let (_ti, uo_incoming) = make_input(
        &mut OsRng.clone(),
        MicroTari::from(100 + OsRng.next_u64() % 1000),
        &factories.commitment,
    );
    runtime
        .block_on(db.accept_incoming_pending_transaction(
            5,
            uo_incoming.value,
            uo_incoming.spending_key.clone(),
            OutputFeatures::default(),
            &factories,
            None,
        ))
        .unwrap();

    pending_incoming_balance += uo_incoming.clone().value;

    let balance = runtime.block_on(db.get_balance(None)).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    runtime
        .block_on(db.cancel_pending_transaction_outputs(pending_txs[1].tx_id))
        .unwrap();

    let mut cancelled_incoming = MicroTari(0);
    let mut cancelled_outgoing = MicroTari(0);

    cancelled_outgoing += pending_txs[1]
        .outputs_to_be_spent
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    cancelled_incoming += pending_txs[1]
        .outputs_to_be_received
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

    available_balance += cancelled_outgoing;
    pending_incoming_balance -= cancelled_incoming;
    pending_outgoing_balance -= cancelled_outgoing;

    let balance = runtime.block_on(db.get_balance(None)).unwrap();
    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    let remaining_p_tx = runtime.block_on(db.fetch_all_pending_transaction_outputs()).unwrap();

    runtime
        .block_on(db.timeout_pending_transaction_outputs(Duration::from_millis(120_000_000_000)))
        .unwrap();

    assert_eq!(
        runtime
            .block_on(db.fetch_all_pending_transaction_outputs())
            .unwrap()
            .len(),
        remaining_p_tx.len()
    );

    runtime
        .block_on(db.timeout_pending_transaction_outputs(Duration::from_millis(6_000_000)))
        .unwrap();

    assert_eq!(
        runtime
            .block_on(db.fetch_all_pending_transaction_outputs())
            .unwrap()
            .len(),
        remaining_p_tx.len() - 1
    );

    assert!(!runtime
        .block_on(db.fetch_all_pending_transaction_outputs())
        .unwrap()
        .contains_key(&pending_txs[2].tx_id));

    // Test invalidating an output
    let invalid_outputs = runtime.block_on(db.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_outputs.len(), 0);
    let unspent_outputs = runtime.block_on(db.get_unspent_outputs()).unwrap();
    let _ = runtime
        .block_on(db.invalidate_output(unspent_outputs[0].clone()))
        .unwrap();
    let invalid_outputs = runtime.block_on(db.get_invalid_outputs()).unwrap();

    assert_eq!(invalid_outputs.len(), 1);
    assert_eq!(invalid_outputs[0], unspent_outputs[0]);

    // test revalidating output
    let unspent_outputs = runtime.block_on(db.get_unspent_outputs()).unwrap();
    assert!(
        unspent_outputs
            .iter()
            .find(|o| o.unblinded_output == invalid_outputs[0].unblinded_output)
            .is_none(),
        "Should not find output"
    );

    assert!(runtime
        .block_on(db.revalidate_output(factories.commitment.commit(
            &pending_txs[2].outputs_to_be_spent[0].unblinded_output.spending_key,
            &pending_txs[2].outputs_to_be_spent[0].unblinded_output.value.into()
        )))
        .is_err());
    runtime
        .block_on(db.revalidate_output(factories.commitment.commit(
            &invalid_outputs[0].unblinded_output.spending_key,
            &invalid_outputs[0].unblinded_output.value.into(),
        )))
        .unwrap();
    let new_invalid_outputs = runtime.block_on(db.get_invalid_outputs()).unwrap();
    assert_eq!(new_invalid_outputs.len(), 0);
    let unspent_outputs = runtime.block_on(db.get_unspent_outputs()).unwrap();
    assert!(
        unspent_outputs
            .iter()
            .find(|o| o.unblinded_output == invalid_outputs[0].unblinded_output)
            .is_some(),
        "Should find revalidated output"
    );
    let result = runtime.block_on(db.update_spent_output_to_unspent(unspent_outputs[0].commitment.clone()));
    assert!(result.is_err());

    let spent_outputs = runtime.block_on(db.get_spent_outputs()).unwrap();
    let updated_output = runtime
        .block_on(db.update_spent_output_to_unspent(spent_outputs[0].commitment.clone()))
        .unwrap();

    let unspent_outputs = runtime.block_on(db.get_unspent_outputs()).unwrap();
    assert!(
        unspent_outputs
            .iter()
            .find(|o| o.unblinded_output == updated_output.unblinded_output)
            .is_some(),
        "Should find updated spent output"
    );
}

#[test]
pub fn test_output_manager_memory_db() {
    test_db_backend(OutputManagerMemoryDatabase::new());
}

#[test]
pub fn test_output_manager_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    test_db_backend(OutputManagerSqliteDatabase::new(connection, None));
}

#[test]
pub fn test_output_manager_sqlite_db_encrypted() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    let key = GenericArray::from_slice(b"an example very very secret key.");
    let cipher = Aes256Gcm::new(key);

    test_db_backend(OutputManagerSqliteDatabase::new(connection, Some(cipher)));
}

pub fn test_key_manager_crud<T: OutputManagerBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();

    let db = OutputManagerDatabase::new(backend);

    assert_eq!(runtime.block_on(db.get_key_manager_state()).unwrap(), None);
    assert!(runtime.block_on(db.increment_key_index()).is_err());

    let state1 = KeyManagerState {
        master_key: PrivateKey::random(&mut OsRng),
        branch_seed: "blah".to_string(),
        primary_key_index: 0,
    };

    runtime.block_on(db.set_key_manager_state(state1.clone())).unwrap();

    let read_state1 = runtime.block_on(db.get_key_manager_state()).unwrap().unwrap();
    assert_eq!(state1, read_state1);

    let state2 = KeyManagerState {
        master_key: PrivateKey::random(&mut OsRng),
        branch_seed: "blah2".to_string(),
        primary_key_index: 0,
    };

    runtime.block_on(db.set_key_manager_state(state2.clone())).unwrap();

    let read_state2 = runtime.block_on(db.get_key_manager_state()).unwrap().unwrap();
    assert_eq!(state2, read_state2);

    runtime.block_on(db.increment_key_index()).unwrap();
    runtime.block_on(db.increment_key_index()).unwrap();

    let read_state3 = runtime.block_on(db.get_key_manager_state()).unwrap().unwrap();
    assert_eq!(read_state3.primary_key_index, 2);
}
#[test]
pub fn test_key_manager_crud_memory_db() {
    test_key_manager_crud(OutputManagerMemoryDatabase::new());
}

#[test]
pub fn test_key_manager_crud_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    test_key_manager_crud(OutputManagerSqliteDatabase::new(connection, None));
}

pub async fn test_short_term_encumberance<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let db = OutputManagerDatabase::new(backend);

    // Add a pending tx
    let mut available_balance = MicroTari(0);
    let mut pending_tx = PendingTransactionOutputs {
        tx_id: OsRng.next_u64(),
        outputs_to_be_spent: vec![],
        outputs_to_be_received: vec![],
        timestamp: Utc::now().naive_utc() - ChronoDuration::from_std(Duration::from_millis(120_000_000)).unwrap(),
        coinbase_block_height: None,
    };
    for i in 1..4 {
        let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(1000 * i), &factories.commitment);
        available_balance += uo.value.clone();
        let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
        db.add_unspent_output(uo.clone()).await.unwrap();
        pending_tx.outputs_to_be_spent.push(uo);
    }

    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(50), &factories.commitment);
    let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
    pending_tx.outputs_to_be_received.push(uo);

    db.encumber_outputs(pending_tx.tx_id, pending_tx.outputs_to_be_spent.clone(), vec![
        pending_tx.outputs_to_be_received[0].clone(),
    ])
    .await
    .unwrap();

    let balance = db.get_balance(None).await.unwrap();
    assert_eq!(balance.available_balance, MicroTari(0));

    db.clear_short_term_encumberances().await.unwrap();

    let balance = db.get_balance(None).await.unwrap();
    assert_eq!(available_balance, balance.available_balance);

    pending_tx.outputs_to_be_received.clear();
    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(50), &factories.commitment);
    let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
    pending_tx.outputs_to_be_received.push(uo);

    db.encumber_outputs(pending_tx.tx_id, pending_tx.outputs_to_be_spent.clone(), vec![
        pending_tx.outputs_to_be_received[0].clone(),
    ])
    .await
    .unwrap();

    db.confirm_encumbered_outputs(pending_tx.tx_id).await.unwrap();
    db.clear_short_term_encumberances().await.unwrap();

    let balance = db.get_balance(None).await.unwrap();
    assert_eq!(balance.available_balance, MicroTari(0));

    pending_tx.outputs_to_be_received.clear();
    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(50), &factories.commitment);
    let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();
    pending_tx.outputs_to_be_received.push(uo);

    db.cancel_pending_transaction_outputs(pending_tx.tx_id).await.unwrap();

    db.encumber_outputs(pending_tx.tx_id, pending_tx.outputs_to_be_spent.clone(), vec![
        pending_tx.outputs_to_be_received[0].clone(),
    ])
    .await
    .unwrap();

    db.confirm_pending_transaction_outputs(pending_tx.tx_id).await.unwrap();

    let balance = db.get_balance(None).await.unwrap();
    assert_eq!(
        balance.available_balance,
        pending_tx.outputs_to_be_received[0].unblinded_output.value
    );
}

#[tokio_macros::test]
pub async fn test_short_term_encumberance_memory_db() {
    test_short_term_encumberance(OutputManagerMemoryDatabase::new()).await;
}

#[tokio_macros::test]
pub async fn test_short_term_encumberance_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    test_short_term_encumberance(OutputManagerSqliteDatabase::new(connection, None)).await;
}

#[tokio_macros::test]
pub async fn test_no_duplicate_outputs_memory_db() {
    test_no_duplicate_outputs(OutputManagerMemoryDatabase::new()).await;
}

#[tokio_macros::test]
pub async fn test_no_duplicate_outputs_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    test_no_duplicate_outputs(OutputManagerSqliteDatabase::new(connection, None)).await;
}

pub async fn test_no_duplicate_outputs<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let db = OutputManagerDatabase::new(backend);

    // create an output
    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(1000), &factories.commitment);
    let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories).unwrap();

    // add it to the database
    let result = db.add_unspent_output(uo.clone()).await;
    assert!(result.is_ok());
    let outputs = db.get_unspent_outputs().await.unwrap();
    assert_eq!(outputs.len(), 1);

    // adding it again should be an error
    let err = db.add_unspent_output(uo.clone()).await.unwrap_err();
    assert!(matches!(err, OutputManagerStorageError::DuplicateOutput));
    let outputs = db.get_unspent_outputs().await.unwrap();
    assert_eq!(outputs.len(), 1);

    // add a pending transaction with the same duplicate output
    let pending_tx = PendingTransactionOutputs {
        tx_id: OsRng.next_u64(),
        outputs_to_be_spent: vec![],
        outputs_to_be_received: vec![uo],
        timestamp: Utc::now().naive_utc() - ChronoDuration::from_std(Duration::from_millis(120_000_000)).unwrap(),
        coinbase_block_height: None,
    };

    match db.add_pending_transaction_outputs(pending_tx.clone()).await {
        Ok(()) => {
            // memory db storage allows the pending tx but trying to confirm the transaction should be an error
            let err = db
                .confirm_pending_transaction_outputs(pending_tx.tx_id)
                .await
                .unwrap_err();
            assert!(matches!(err, OutputManagerStorageError::DuplicateOutput));
        },
        Err(e) => {
            // sqlite db storage should not even allow the pending tx, since it adds a duplicate in the outputs table
            if let OutputManagerStorageError::DieselError(e) = e {
                if let DatabaseError(db_err, _) = e {
                    assert!(matches!(db_err, DatabaseErrorKind::UniqueViolation));
                } else {
                    assert!(false, "Unexpected database error type: {}", e);
                }
            } else {
                assert!(false, "Unexpected output manager storage error type: {}", e);
            }
        },
    }

    // we should still only have 1 unspent output
    let outputs = db.get_unspent_outputs().await.unwrap();
    assert_eq!(outputs.len(), 1);
}
