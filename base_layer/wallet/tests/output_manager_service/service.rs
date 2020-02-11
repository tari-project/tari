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

use crate::support::utils::{make_input, random_string, TestParams};
use rand::{rngs::OsRng, RngCore};
use std::{thread, time::Duration};
use tari_core::transactions::{
    fee::Fee,
    tari_amount::MicroTari,
    transaction::{KernelFeatures, OutputFeatures, TransactionOutput, UnblindedOutput},
    transaction_protocol::single_receiver::SingleReceiverTransactionProtocol,
    types::{CryptoFactories, PrivateKey, RangeProof},
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey, range_proof::RangeProofService};
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_utilities::ByteArray;
use tari_wallet::{
    output_manager_service::{
        error::{OutputManagerError, OutputManagerStorageError},
        handle::OutputManagerHandle,
        storage::{
            database::{DbKey, DbValue, OutputManagerBackend},
            memory_db::OutputManagerMemoryDatabase,
            sqlite_db::OutputManagerSqliteDatabase,
        },
        OutputManagerServiceInitializer,
    },
    storage::connection_manager::run_migration_and_create_connection_pool,
};
use tempdir::TempDir;
use tokio::runtime::Runtime;

pub fn setup_output_manager_service<T: OutputManagerBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (OutputManagerHandle, Shutdown)
{
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();
    let fut = StackBuilder::new(runtime.handle().clone(), shutdown.to_signal())
        .add_initializer(OutputManagerServiceInitializer::new(backend, factories))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let oms_api = handles.get_handle::<OutputManagerHandle>().unwrap();

    (oms_api, shutdown)
}

fn sending_transaction_and_confirmation<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend.clone());

    let (_ti, uo) = make_input(
        &mut OsRng.clone(),
        MicroTari::from(100 + OsRng.next_u64() % 1000),
        &factories.commitment,
    );
    runtime.block_on(oms.add_output(uo.clone())).unwrap();
    assert_eq!(
        runtime.block_on(oms.add_output(uo)),
        Err(OutputManagerError::OutputManagerStorageError(
            OutputManagerStorageError::DuplicateOutput
        ))
    );
    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        runtime.block_on(oms.add_output(uo)).unwrap();
    }

    let mut stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    let sender_tx_id = stp.get_tx_id().unwrap();
    let mut num_change = 0;
    // Is there change? Unlikely not to be but the random amounts MIGHT produce a no change output situation
    if stp.get_amount_to_self().unwrap() > MicroTari::from(0) {
        let pt = runtime.block_on(oms.get_pending_transactions()).unwrap();
        assert_eq!(pt.len(), 1);
        assert_eq!(
            pt.get(&sender_tx_id).unwrap().outputs_to_be_received[0].value,
            stp.get_amount_to_self().unwrap()
        );
        num_change = 1;
    }

    let msg = stp.build_single_round_message().unwrap();

    let b = TestParams::new(&mut OsRng);

    let recv_info =
        SingleReceiverTransactionProtocol::create(&msg, b.nonce, b.spend_key, OutputFeatures::default(), &factories)
            .unwrap();

    stp.add_single_recipient_info(recv_info.clone(), &factories.range_proof)
        .unwrap();

    stp.finalize(KernelFeatures::empty(), &factories).unwrap();

    let tx = stp.get_transaction().unwrap();

    runtime
        .block_on(oms.confirm_transaction(sender_tx_id, tx.body.inputs().clone(), tx.body.outputs().clone()))
        .unwrap();

    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 0);
    assert_eq!(
        runtime.block_on(oms.get_spent_outputs()).unwrap().len(),
        tx.body.inputs().len()
    );
    assert_eq!(
        runtime.block_on(oms.get_unspent_outputs()).unwrap().len(),
        num_outputs + 1 - runtime.block_on(oms.get_spent_outputs()).unwrap().len() + num_change
    );

    if let DbValue::KeyManagerState(km) = backend.fetch(&DbKey::KeyManagerState).unwrap().unwrap() {
        assert_eq!(km.primary_key_index, 1);
    } else {
        assert!(false, "No Key Manager set");
    }
}

#[test]
fn sending_transaction_and_confirmation_memory_db() {
    sending_transaction_and_confirmation(OutputManagerMemoryDatabase::new());
}

#[test]
fn sending_transaction_and_confirmation_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();

    sending_transaction_and_confirmation(OutputManagerSqliteDatabase::new(connection_pool));
}

fn send_not_enough_funds<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);
    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        runtime.block_on(oms.add_output(uo)).unwrap();
    }

    match runtime.block_on(oms.prepare_transaction_to_send(
        MicroTari::from(num_outputs * 2000),
        MicroTari::from(20),
        None,
        "".to_string(),
    )) {
        Err(OutputManagerError::NotEnoughFunds) => assert!(true),
        _ => assert!(false),
    }
}

#[test]
fn send_not_enough_funds_memory_db() {
    send_not_enough_funds(OutputManagerMemoryDatabase::new());
}

#[test]
fn send_not_enough_funds_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();

    send_not_enough_funds(OutputManagerSqliteDatabase::new(connection_pool));
}

fn send_no_change<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let fee_per_gram = MicroTari::from(20);
    let fee_without_change = Fee::calculate(fee_per_gram, 2, 1);
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 500;
    runtime
        .block_on(oms.add_output(UnblindedOutput::new(MicroTari::from(value1), key1, None)))
        .unwrap();
    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    runtime
        .block_on(oms.add_output(UnblindedOutput::new(MicroTari::from(value2), key2, None)))
        .unwrap();

    let mut stp = runtime
        .block_on(oms.prepare_transaction_to_send(
            MicroTari::from(value1 + value2) - fee_without_change,
            MicroTari::from(20),
            None,
            "".to_string(),
        ))
        .unwrap();

    let sender_tx_id = stp.get_tx_id().unwrap();
    assert_eq!(stp.get_amount_to_self().unwrap(), MicroTari::from(0));
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);

    let msg = stp.build_single_round_message().unwrap();

    let b = TestParams::new(&mut OsRng);

    let recv_info =
        SingleReceiverTransactionProtocol::create(&msg, b.nonce, b.spend_key, OutputFeatures::default(), &factories)
            .unwrap();

    stp.add_single_recipient_info(recv_info.clone(), &factories.range_proof)
        .unwrap();

    stp.finalize(KernelFeatures::empty(), &factories).unwrap();

    let tx = stp.get_transaction().unwrap();

    runtime
        .block_on(oms.confirm_transaction(sender_tx_id, tx.body.inputs().clone(), tx.body.outputs().clone()))
        .unwrap();

    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 0);
    assert_eq!(
        runtime.block_on(oms.get_spent_outputs()).unwrap().len(),
        tx.body.inputs().len()
    );
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
}

#[test]
fn send_no_change_memory_db() {
    send_no_change(OutputManagerMemoryDatabase::new());
}

#[test]
fn send_no_change_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();

    send_no_change(OutputManagerSqliteDatabase::new(connection_pool));
}

fn send_not_enough_for_change<T: OutputManagerBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let fee_per_gram = MicroTari::from(20);
    let fee_without_change = Fee::calculate(fee_per_gram, 2, 1);
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 500;
    runtime
        .block_on(oms.add_output(UnblindedOutput::new(MicroTari::from(value1), key1, None)))
        .unwrap();
    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    runtime
        .block_on(oms.add_output(UnblindedOutput::new(MicroTari::from(value2), key2, None)))
        .unwrap();

    match runtime.block_on(oms.prepare_transaction_to_send(
        MicroTari::from(value1 + value2 + 1) - fee_without_change,
        MicroTari::from(20),
        None,
        "".to_string(),
    )) {
        Err(OutputManagerError::NotEnoughFunds) => assert!(true),
        _ => assert!(false),
    }
}

#[test]
fn send_not_enough_for_change_memory_db() {
    send_not_enough_for_change(OutputManagerMemoryDatabase::new());
}

#[test]
fn send_not_enough_for_change_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();

    send_not_enough_for_change(OutputManagerSqliteDatabase::new(connection_pool));
}

fn receiving_and_confirmation<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let value = MicroTari::from(5000);
    let recv_key = runtime.block_on(oms.get_recipient_spending_key(1, value)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);

    let commitment = factories.commitment.commit(&recv_key, &value.into());
    let rr = factories.range_proof.construct_proof(&recv_key, value.into()).unwrap();
    let output = TransactionOutput::new(
        OutputFeatures::create_coinbase(1),
        commitment,
        RangeProof::from_bytes(&rr).unwrap(),
    );

    runtime
        .block_on(oms.confirm_transaction(1, vec![], vec![output]))
        .unwrap();

    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 1);
}

#[test]
fn receiving_and_confirmation_memory_db() {
    receiving_and_confirmation(OutputManagerMemoryDatabase::new());
}

#[test]
fn receiving_and_confirmation_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();

    receiving_and_confirmation(OutputManagerSqliteDatabase::new(connection_pool));
}

fn cancel_transaction<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        runtime.block_on(oms.add_output(uo)).unwrap();
    }
    let stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    match runtime.block_on(oms.cancel_transaction(1)) {
        Err(OutputManagerError::OutputManagerStorageError(OutputManagerStorageError::ValueNotFound(_))) => {
            assert!(true)
        },
        _ => assert!(false, "Value should not exist"),
    }

    runtime
        .block_on(oms.cancel_transaction(stp.get_tx_id().unwrap()))
        .unwrap();

    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), num_outputs);
}

#[test]
fn cancel_transaction_memory_db() {
    cancel_transaction(OutputManagerMemoryDatabase::new());
}

#[test]
fn cancel_transaction_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();
    cancel_transaction(OutputManagerSqliteDatabase::new(connection_pool));
}

fn timeout_transaction<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();
    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(100 + OsRng.next_u64() % 1000),
            &factories.commitment,
        );
        runtime.block_on(oms.add_output(uo)).unwrap();
    }
    let _stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    let remaining_outputs = runtime.block_on(oms.get_unspent_outputs()).unwrap().len();

    thread::sleep(Duration::from_millis(2));

    runtime
        .block_on(oms.timeout_transactions(Duration::from_millis(1000)))
        .unwrap();

    assert_eq!(
        runtime.block_on(oms.get_unspent_outputs()).unwrap().len(),
        remaining_outputs
    );

    runtime
        .block_on(oms.timeout_transactions(Duration::from_millis(1)))
        .unwrap();

    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), num_outputs);
}

#[test]
fn timeout_transaction_memory_db() {
    timeout_transaction(OutputManagerMemoryDatabase::new());
}

#[test]
fn timeout_transaction_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();
    timeout_transaction(OutputManagerSqliteDatabase::new(connection_pool));
}

fn test_get_balance<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let balance = runtime.block_on(oms.get_balance()).unwrap();

    assert_eq!(MicroTari::from(0), balance.available_balance);

    let mut total = MicroTari::from(0);
    let output_val = MicroTari::from(2000);
    let (_ti, uo) = make_input(&mut OsRng.clone(), output_val.clone(), &factories.commitment);
    total += uo.value.clone();
    runtime.block_on(oms.add_output(uo)).unwrap();

    let (_ti, uo) = make_input(&mut OsRng.clone(), output_val.clone(), &factories.commitment);
    total += uo.value.clone();
    runtime.block_on(oms.add_output(uo)).unwrap();

    let send_value = MicroTari::from(1000);
    let stp = runtime
        .block_on(oms.prepare_transaction_to_send(send_value.clone(), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    let change_val = stp.get_change_amount().unwrap();

    let recv_value = MicroTari::from(1500);
    let _recv_key = runtime.block_on(oms.get_recipient_spending_key(1, recv_value)).unwrap();

    let balance = runtime.block_on(oms.get_balance()).unwrap();

    assert_eq!(output_val, balance.available_balance);
    assert_eq!(recv_value + change_val, balance.pending_incoming_balance);
    assert_eq!(output_val, balance.pending_outgoing_balance);
}

#[test]
fn test_get_balance_memory_db() {
    test_get_balance(OutputManagerMemoryDatabase::new());
}

#[test]
fn test_get_balance_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();
    test_get_balance(OutputManagerSqliteDatabase::new(connection_pool));
}

fn test_confirming_received_output<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _shutdown) = setup_output_manager_service(&mut runtime, backend);

    let value = MicroTari::from(5000);
    let recv_key = runtime.block_on(oms.get_recipient_spending_key(1, value)).unwrap();
    let commitment = factories.commitment.commit(&recv_key, &value.into());

    let rr = factories.range_proof.construct_proof(&recv_key, value.into()).unwrap();
    let output = TransactionOutput::new(
        OutputFeatures::create_coinbase(1),
        commitment,
        RangeProof::from_bytes(&rr).unwrap(),
    );
    runtime
        .block_on(oms.confirm_transaction(1, vec![], vec![output]))
        .unwrap();
    assert_eq!(runtime.block_on(oms.get_balance()).unwrap().available_balance, value);
}

#[test]
fn test_confirming_received_output_memory_db() {
    test_confirming_received_output(OutputManagerMemoryDatabase::new());
}

#[test]
fn test_confirming_received_output_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection_pool = run_migration_and_create_connection_pool(db_path).unwrap();
    test_confirming_received_output(OutputManagerSqliteDatabase::new(connection_pool));
}
