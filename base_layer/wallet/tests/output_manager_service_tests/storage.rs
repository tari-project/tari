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

use std::mem::size_of;

use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use rand::{rngs::OsRng, RngCore};
use tari_common_types::{transaction::TxId, types::FixedHash};
use tari_core::transactions::{tari_amount::MicroTari, CryptoFactories};
use tari_wallet::output_manager_service::{
    error::OutputManagerStorageError,
    service::Balance,
    storage::{
        database::{OutputManagerBackend, OutputManagerDatabase},
        models::DbUnblindedOutput,
        sqlite_db::OutputManagerSqliteDatabase,
        OutputSource,
    },
};
use tokio::runtime::Runtime;

use crate::support::{data::get_temp_sqlite_database_connection, utils::make_input};

#[allow(clippy::too_many_lines)]
pub fn test_db_backend<T: OutputManagerBackend + 'static>(backend: T) {
    let runtime = Runtime::new().unwrap();

    let db = OutputManagerDatabase::new(backend);
    let factories = CryptoFactories::default();

    // Add some unspent outputs
    let mut unspent_outputs = Vec::new();
    for i in 0..5 {
        let (_ti, uo) = runtime.block_on(make_input(
            &mut OsRng,
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        ));
        let mut uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
        uo.unblinded_output.features.maturity = i;
        db.add_unspent_output(uo.clone()).unwrap();
        unspent_outputs.push(uo);
    }

    let time_locked_outputs = db.get_timelocked_outputs(3).unwrap();
    assert_eq!(time_locked_outputs.len(), 1);
    assert_eq!(unspent_outputs[4], time_locked_outputs[0]);
    let time_locked_outputs = db.get_timelocked_outputs(4).unwrap();
    assert_eq!(time_locked_outputs.len(), 0);
    let time_locked_balance = unspent_outputs[4].unblinded_output.value;

    for i in 0..4usize {
        let balance = db.get_balance(Some(i as u64)).unwrap();
        let mut sum = MicroTari::from(0);
        for output in unspent_outputs.iter().take(5).skip(i + 1) {
            sum += output.unblinded_output.value;
        }
        assert_eq!(balance.time_locked_balance.unwrap(), sum);
    }

    unspent_outputs.sort();

    let outputs = db.fetch_sorted_unspent_outputs().unwrap();
    assert_eq!(unspent_outputs, outputs);

    // Add some sent transactions with outputs to be spent and received
    struct PendingTransactionOutputs {
        tx_id: TxId,
        outputs_to_be_spent: Vec<DbUnblindedOutput>,
        outputs_to_be_received: Vec<DbUnblindedOutput>,
    }

    let mut pending_txs = Vec::new();
    for _ in 0..3 {
        let mut pending_tx = PendingTransactionOutputs {
            tx_id: TxId::new_random(),
            outputs_to_be_spent: vec![],
            outputs_to_be_received: vec![],
        };
        for _ in 0..4 {
            let (_ti, uo) = runtime.block_on(make_input(
                &mut OsRng,
                MicroTari::from(100 + OsRng.next_u64() % 1000),
                &factories.commitment,
            ));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
            db.add_unspent_output(uo.clone()).unwrap();
            pending_tx.outputs_to_be_spent.push(uo);
        }
        for _ in 0..2 {
            let (_ti, uo) = runtime.block_on(make_input(
                &mut OsRng,
                MicroTari::from(100 + OsRng.next_u64() % 1000),
                &factories.commitment,
            ));
            let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
            pending_tx.outputs_to_be_received.push(uo);
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
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    let mut pending_incoming_balance = MicroTari(0);
    let mut pending_outgoing_balance = MicroTari(0);
    for v in &pending_txs {
        pending_outgoing_balance += v
            .outputs_to_be_spent
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
        pending_incoming_balance += v
            .outputs_to_be_received
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
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
        available_balance,
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
    let mut mmr_pos = 0;
    for o in &pending_txs[0].outputs_to_be_received {
        db.set_received_output_mined_height_and_status(o.hash, 2, FixedHash::zero(), mmr_pos, false, 0)
            .unwrap();
        mmr_pos += 1;
    }
    for o in &pending_txs[0].outputs_to_be_spent {
        db.mark_output_as_spent(o.hash, 3, FixedHash::zero(), false).unwrap();
    }

    // Balance shouldn't change
    let balance = db.get_balance(None).unwrap();

    assert_eq!(balance, Balance {
        available_balance,
        time_locked_balance: None,
        pending_incoming_balance,
        pending_outgoing_balance
    });

    // Set second pending tx to mined and confirmed
    for o in &pending_txs[1].outputs_to_be_received {
        db.set_received_output_mined_height_and_status(o.hash, 4, FixedHash::zero(), mmr_pos, true, 0)
            .unwrap();
        mmr_pos += 1;
    }
    for o in &pending_txs[1].outputs_to_be_spent {
        db.mark_output_as_spent(o.hash, 5, FixedHash::zero(), true).unwrap();
    }

    // Balance with confirmed second pending tx
    let mut available_balance = unspent_outputs
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    let mut pending_incoming_balance = MicroTari(0);
    let mut pending_outgoing_balance = MicroTari(0);

    pending_outgoing_balance += pending_txs[0]
        .outputs_to_be_spent
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    pending_outgoing_balance += pending_txs[2]
        .outputs_to_be_spent
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    pending_incoming_balance += pending_txs[0]
        .outputs_to_be_received
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);
    pending_incoming_balance += pending_txs[2]
        .outputs_to_be_received
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

    available_balance += pending_txs[1]
        .outputs_to_be_received
        .iter()
        .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value);

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
    let (_ti, uo) = runtime.block_on(make_input(
        &mut OsRng,
        MicroTari::from(100 + OsRng.next_u64() % 1000),
        &factories.commitment,
    ));
    let output_to_be_received =
        DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
    db.add_output_to_be_received(TxId::from(11u64), output_to_be_received.clone(), None)
        .unwrap();
    pending_incoming_balance += output_to_be_received.unblinded_output.value;

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
    db.mark_output_as_spent(
        pending_txs[1].outputs_to_be_received[0].hash,
        6,
        FixedHash::zero(),
        true,
    )
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

#[test]
pub fn test_output_manager_sqlite_db() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    test_db_backend(OutputManagerSqliteDatabase::new(connection, None));
}

#[test]
pub fn test_output_manager_sqlite_db_encrypted() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let cipher = XChaCha20Poly1305::new(key_ga);

    test_db_backend(OutputManagerSqliteDatabase::new(connection, Some(cipher)));
}

#[tokio::test]
pub async fn test_short_term_encumberance() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let db = OutputManagerDatabase::new(backend);

    let mut unspent_outputs = Vec::new();
    for i in 0..5 {
        let (_ti, uo) = make_input(
            &mut OsRng,
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        )
        .await;
        let mut uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();
        uo.unblinded_output.features.maturity = i;
        db.add_unspent_output(uo.clone()).unwrap();
        unspent_outputs.push(uo);
    }

    db.encumber_outputs(1u64.into(), unspent_outputs[0..=2].to_vec(), vec![])
        .unwrap();

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance.available_balance,
        unspent_outputs[3..5]
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value)
    );

    db.clear_short_term_encumberances().unwrap();

    let balance = db.get_balance(None).unwrap();
    assert_eq!(
        balance.available_balance,
        unspent_outputs
            .iter()
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value)
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
            .fold(MicroTari::from(0), |acc, x| acc + x.unblinded_output.value)
    );
}

#[tokio::test]
pub async fn test_no_duplicate_outputs() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let db = OutputManagerDatabase::new(backend);

    // create an output
    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(1000), &factories.commitment).await;
    let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();

    // add it to the database
    let result = db.add_unspent_output(uo.clone());
    assert!(result.is_ok());
    let result = db.set_received_output_mined_height_and_status(uo.hash, 1, FixedHash::zero(), 1, true, 0);
    assert!(result.is_ok());
    let outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(outputs.len(), 1);

    // adding it again should be an error
    let err = db.add_unspent_output(uo.clone()).unwrap_err();
    assert!(matches!(err, OutputManagerStorageError::DuplicateOutput));
    let outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(outputs.len(), 1);

    // add a pending transaction with the same duplicate output

    assert!(db.encumber_outputs(2u64.into(), vec![], vec![uo]).is_err());

    // we should still only have 1 unspent output
    let outputs = db.fetch_mined_unspent_outputs().unwrap();
    assert_eq!(outputs.len(), 1);
}

#[tokio::test]
pub async fn test_mark_as_unmined() {
    let factories = CryptoFactories::default();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = OutputManagerSqliteDatabase::new(connection, None);
    let db = OutputManagerDatabase::new(backend);

    // create an output
    let (_ti, uo) = make_input(&mut OsRng, MicroTari::from(1000), &factories.commitment).await;
    let uo = DbUnblindedOutput::from_unblinded_output(uo, &factories, None, OutputSource::Unknown).unwrap();

    // add it to the database
    db.add_unspent_output(uo.clone()).unwrap();
    db.set_received_output_mined_height_and_status(uo.hash, 1, FixedHash::zero(), 1, true, 0)
        .unwrap();
    let o = db.get_last_mined_output().unwrap().unwrap();
    assert_eq!(o.hash, uo.hash);
    db.set_output_to_unmined_and_invalid(uo.hash).unwrap();
    assert!(db.get_last_mined_output().unwrap().is_none());
    let o = db.get_invalid_outputs().unwrap().pop().unwrap();
    assert_eq!(o.hash, uo.hash);
    assert!(o.mined_height.is_none());
    assert!(o.mined_in_block.is_none());
}
