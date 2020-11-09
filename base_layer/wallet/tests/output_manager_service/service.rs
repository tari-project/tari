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

use crate::support::{
    comms_and_services::create_dummy_message,
    utils::{random_string, TestParams},
};
use futures::{
    channel::{mpsc, mpsc::Sender},
    FutureExt,
    SinkExt,
    StreamExt,
};
use prost::Message;
use rand::{rngs::OsRng, RngCore};
use std::{thread, time::Duration};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::{
            base_node_service_request::Request,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    consensus::{ConsensusConstantsBuilder, Network},
    transactions::{
        fee::Fee,
        tari_amount::{uT, MicroTari},
        transaction::{KernelFeatures, Transaction},
        transaction_protocol::single_receiver::SingleReceiverTransactionProtocol,
        types::{CommitmentFactory, CryptoFactories, PrivateKey},
        OutputBuilder,
        OutputFeatures,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{keys::SecretKey, tari_utilities::hash::Hashable};
use tari_p2p::domain_message::DomainMessage;
use tari_service_framework::reply_channel;
use tari_shutdown::Shutdown;
use tari_wallet::{
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerStorageError},
        handle::{OutputManagerEvent, OutputManagerHandle},
        protocols::txo_validation_protocol::{TxoValidationRetry, TxoValidationType},
        service::OutputManagerService,
        storage::{
            database::{DbKey, DbKeyValuePair, DbValue, OutputManagerBackend, OutputManagerDatabase, WriteOperation},
            memory_db::OutputManagerMemoryDatabase,
            models::DbUnblindedOutput,
            sqlite_db::OutputManagerSqliteDatabase,
        },
    },
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    transaction_service::handle::TransactionServiceHandle,
};
use tempfile::tempdir;
use tokio::{
    runtime::Runtime,
    sync::{broadcast, broadcast::channel},
    time::delay_for,
};

pub fn setup_output_manager_service<T: OutputManagerBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (
    OutputManagerHandle,
    OutboundServiceMockState,
    Shutdown,
    Sender<DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
    TransactionServiceHandle,
)
{
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(20);
    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(20);
    let (oms_event_publisher, _) = broadcast::channel(200);

    let (ts_request_sender, _ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher.clone());

    let constants = ConsensusConstantsBuilder::new(Network::Rincewind).build();

    let output_manager_service = runtime
        .block_on(OutputManagerService::new(
            OutputManagerServiceConfig {
                base_node_query_timeout: Duration::from_secs(10),
                max_utxo_query_size: 2,
                ..Default::default()
            },
            outbound_message_requester.clone(),
            ts_handle.clone(),
            oms_request_receiver,
            base_node_response_receiver,
            OutputManagerDatabase::new(backend),
            oms_event_publisher.clone(),
            factories.clone(),
            constants.coinbase_lock_height(),
            shutdown.to_signal(),
        ))
        .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);

    runtime.spawn(async move { output_manager_service.start().await.unwrap() });

    let outbound_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());

    (
        output_manager_service_handle,
        outbound_mock_state,
        shutdown,
        base_node_response_sender,
        ts_handle,
    )
}

async fn complete_transaction(mut stp: SenderTransactionProtocol, mut oms: OutputManagerHandle) -> Transaction {
    let factories = CryptoFactories::default();

    let sender_tx_id = stp.get_tx_id().unwrap();
    // Is there change? Unlikely not to be but the random amounts MIGHT produce a no change output situation
    if stp.get_amount_to_self().unwrap() > MicroTari::from(0) {
        let pt = oms.get_pending_transactions().await.unwrap();
        assert_eq!(pt.len(), 1);
        assert_eq!(
            pt.get(&sender_tx_id).unwrap().outputs_to_be_received[0]
                .unblinded_output
                .value(),
            stp.get_amount_to_self().unwrap()
        );
    }
    let msg = stp.build_single_round_message().unwrap();
    let b = TestParams::new(&mut OsRng);
    let recv_info =
        SingleReceiverTransactionProtocol::create(&msg, b.nonce, b.spend_key, OutputFeatures::default(), &factories)
            .unwrap();
    stp.add_single_recipient_info(recv_info.clone(), &factories.range_proof)
        .unwrap();
    stp.finalize(KernelFeatures::empty(), &factories).unwrap();
    stp.get_transaction().unwrap().clone()
}

fn sending_transaction_and_confirmation<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let uo = OutputBuilder::new()
        .with_value(100 + OsRng.next_u64() % 1000)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(oms.add_output(uo.clone())).unwrap();
    match runtime.block_on(oms.add_output(uo)) {
        Err(OutputManagerError::OutputManagerStorageError(OutputManagerStorageError::DuplicateOutput)) => assert!(true),
        _ => assert!(false, "Incorrect error message"),
    };
    let num_outputs = 20;
    for _i in 0..num_outputs {
        let uo = OutputBuilder::new()
            .with_value(100 + OsRng.next_u64() % 1000)
            .build(&factories.commitment)
            .unwrap();
        runtime.block_on(oms.add_output(uo)).unwrap();
    }

    let stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    let sender_tx_id = stp.get_tx_id().unwrap();

    let tx = runtime.block_on(complete_transaction(stp, oms.clone()));

    runtime
        .block_on(oms.confirm_transaction(sender_tx_id, tx.body.inputs().clone(), tx.body.outputs().clone()))
        .unwrap();

    assert_eq!(
        runtime.block_on(oms.get_pending_transactions()).unwrap().len(),
        0,
        "Should have no pending tx"
    );
    assert_eq!(
        runtime.block_on(oms.get_spent_outputs()).unwrap().len(),
        tx.body.inputs().len(),
        "# Outputs should equal number of sent inputs"
    );
    assert_eq!(
        runtime.block_on(oms.get_unspent_outputs()).unwrap().len(),
        num_outputs + 1 - runtime.block_on(oms.get_spent_outputs()).unwrap().len() + tx.body.outputs().len() - 1,
        "Unspent outputs"
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    sending_transaction_and_confirmation(OutputManagerSqliteDatabase::new(connection, None));
}

fn send_not_enough_funds<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);
    let num_outputs = 20;
    for _i in 0..num_outputs {
        let uo = OutputBuilder::new()
            .with_value(100 + OsRng.next_u64() % 1000)
            .build(&factories.commitment)
            .unwrap();
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    send_not_enough_funds(OutputManagerSqliteDatabase::new(connection, None));
}

fn send_no_change<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let fee_per_gram = MicroTari::from(20);
    let fee_without_change = Fee::calculate(fee_per_gram, 1, 2, 1);
    let value1 = 500;
    runtime
        .block_on(
            oms.add_output(
                OutputBuilder::new()
                    .with_value(value1)
                    .build(&factories.commitment)
                    .unwrap(),
            ),
        )
        .unwrap();
    let value2 = 800;
    runtime
        .block_on(
            oms.add_output(
                OutputBuilder::new()
                    .with_value(value2)
                    .build(&factories.commitment)
                    .unwrap(),
            ),
        )
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    send_no_change(OutputManagerSqliteDatabase::new(connection, None));
}

fn send_not_enough_for_change<T: OutputManagerBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let fee_per_gram = MicroTari::from(20);
    let fee_without_change = Fee::calculate(fee_per_gram, 1, 2, 1);
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 500;
    let factory = CommitmentFactory::default();
    runtime
        .block_on(
            oms.add_output(
                OutputBuilder::new()
                    .with_value(MicroTari::from(value1))
                    .with_spending_key(key1)
                    .build(&factory)
                    .unwrap(),
            ),
        )
        .unwrap();
    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    runtime
        .block_on(
            oms.add_output(
                OutputBuilder::new()
                    .with_value(MicroTari::from(value2))
                    .with_spending_key(key2)
                    .build(&factory)
                    .unwrap(),
            ),
        )
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    send_not_enough_for_change(OutputManagerSqliteDatabase::new(connection, None));
}

fn receiving_and_confirmation<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let value = MicroTari::from(5000);
    let recv_key = runtime.block_on(oms.get_recipient_spending_key(1, value)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);

    let uo = OutputBuilder::new()
        .with_value(value)
        .with_spending_key(recv_key)
        .with_features(OutputFeatures::create_coinbase(1))
        .build(&factories.commitment)
        .unwrap();
    let output = uo.as_transaction_output(&factories).unwrap();

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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    receiving_and_confirmation(OutputManagerSqliteDatabase::new(connection, None));
}

fn cancel_transaction<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let uo = OutputBuilder::new()
            .with_value(100 + OsRng.next_u64() % 1000)
            .build(&factories.commitment)
            .unwrap();
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    cancel_transaction(OutputManagerSqliteDatabase::new(connection, None));
}

fn timeout_transaction<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();
    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let num_outputs = 20;
    for _i in 0..num_outputs {
        let uo = OutputBuilder::new()
            .with_value(100 + OsRng.next_u64() % 1000)
            .build(&factories.commitment)
            .unwrap();
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    timeout_transaction(OutputManagerSqliteDatabase::new(connection, None));
}

fn test_get_balance<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let balance = runtime.block_on(oms.get_balance()).unwrap();

    assert_eq!(MicroTari::from(0), balance.available_balance);

    let mut total = MicroTari::from(0);
    let output_val = MicroTari::from(2000);
    let uo = OutputBuilder::new()
        .with_value(output_val.clone())
        .build(&factories.commitment)
        .unwrap();
    total += uo.value().clone();
    runtime.block_on(oms.add_output(uo)).unwrap();

    let uo = OutputBuilder::new()
        .with_value(output_val.clone())
        .build(&factories.commitment)
        .unwrap();
    total += uo.value().clone();
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    test_get_balance(OutputManagerSqliteDatabase::new(connection, None));
}

fn test_confirming_received_output<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let value = MicroTari::from(5000);
    let recv_key = runtime.block_on(oms.get_recipient_spending_key(1, value)).unwrap();
    let output = OutputBuilder::new()
        .with_value(value)
        .with_spending_key(recv_key)
        .with_features(OutputFeatures::create_coinbase(1))
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();

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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    test_confirming_received_output(OutputManagerSqliteDatabase::new(connection, None));
}

#[test]
fn test_utxo_validation() {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();
    let backend = OutputManagerMemoryDatabase::new();

    let invalid_key = PrivateKey::random(&mut OsRng);
    let invalid_value = 666;
    let invalid_output = OutputBuilder::new()
        .with_value(invalid_value)
        .with_spending_key(invalid_key.clone())
        .build(&factories.commitment)
        .unwrap();
    let invalid_hash = invalid_output.as_transaction_output(&factories).unwrap().hash();

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
            invalid_output.blinding_factor().clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(invalid_output.clone(), &factories).unwrap()),
        )))
        .unwrap();
    backend
        .invalidate_unspent_output(
            &DbUnblindedOutput::from_unblinded_output(invalid_output.clone(), &factories).unwrap(),
        )
        .unwrap();

    let (mut oms, outbound_service, _shutdown, mut base_node_response_sender, _) =
        setup_output_manager_service(&mut runtime, backend);
    let mut event_stream = oms.get_event_stream_fused();

    let mut hashes = Vec::new();
    let value1 = 500;
    let output1 = OutputBuilder::new()
        .with_value(value1)
        .build(&factories.commitment)
        .unwrap();
    let tx_output1 = output1.as_transaction_output(&factories).unwrap();
    let output1_hash = tx_output1.hash();
    hashes.push(output1_hash.clone());
    runtime.block_on(oms.add_output(output1.clone())).unwrap();

    let value2 = 800;
    let output2 = OutputBuilder::new()
        .with_value(value2)
        .build(&factories.commitment)
        .unwrap();
    let tx_output2 = output2.as_transaction_output(&factories).unwrap();
    hashes.push(tx_output2.hash());

    runtime.block_on(oms.add_output(output2.clone())).unwrap();

    let value3 = 900;
    let output3 = OutputBuilder::new()
        .with_value(value3)
        .build(&factories.commitment)
        .unwrap();
    let tx_output3 = output3.as_transaction_output(&factories).unwrap();
    hashes.push(tx_output3.hash());

    runtime.block_on(oms.add_output(output3.clone())).unwrap();

    let value4 = 901;
    let output4 = OutputBuilder::new()
        .with_value(value4)
        .build(&factories.commitment)
        .unwrap();
    let tx_output4 = output4.as_transaction_output(&factories).unwrap();
    hashes.push(tx_output4.hash());

    runtime.block_on(oms.add_output(output4.clone())).unwrap();

    let base_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/58217".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    runtime
        .block_on(oms.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    runtime
        .block_on(oms.validate_txos(TxoValidationType::Invalid, TxoValidationRetry::Limited(5)))
        .unwrap();
    runtime
        .block_on(oms.validate_txos(TxoValidationType::Unspent, TxoValidationRetry::UntilSuccess))
        .unwrap();

    outbound_service
        .wait_call_count(3, Duration::from_secs(60))
        .expect("call wait 1");

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request1: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();
    let mut hashes_found = 0;
    match bn_request1.request {
        None => assert!(false, "Invalid request"),
        Some(request) => match request {
            Request::FetchMatchingUtxos(hash_outputs) => {
                for h in hash_outputs.outputs {
                    if hashes.iter().find(|i| **i == h).is_some() {
                        hashes_found += 1;
                    }
                }
            },
            _ => assert!(false, "invalid request"),
        },
    }

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request2: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    match bn_request2.request {
        None => assert!(false, "Invalid request"),
        Some(request) => match request {
            Request::FetchMatchingUtxos(hash_outputs) => {
                for h in hash_outputs.outputs {
                    if hashes.iter().find(|i| **i == h).is_some() {
                        hashes_found += 1;
                    }
                }
            },
            _ => assert!(false, "invalid request"),
        },
    }

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request3: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    match bn_request3.request {
        None => assert!(false, "Invalid request"),
        Some(request) => match request {
            Request::FetchMatchingUtxos(hash_outputs) => {
                for h in hash_outputs.outputs {
                    if hashes.iter().find(|i| **i == h).is_some() {
                        hashes_found += 1;
                    }
                }
            },
            _ => assert!(false, "invalid request"),
        },
    }

    assert_eq!(hashes_found, 4, "Should have found our Unspent UTXO hashes");

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut timeouts = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    match event.unwrap() {
                        OutputManagerEvent::TxoValidationTimedOut(_) => {
                            timeouts+=1;
                         },
                        _ => (),
                    }
                    if timeouts >= 2 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(timeouts, 2);
    });

    // Test the response to the revalidation call first so as not to confuse the invalidation that happens during the
    // responses to the Unspent UTXO queries
    let mut invalid_request_key = 0;
    let mut unspent_request_key_with_output1 = 0;
    let mut unspent_request_key2 = 0;

    outbound_service.wait_call_count(3, Duration::from_secs(60)).unwrap();

    for _ in 0..3 {
        let (_, body) = outbound_service.pop_call().unwrap();
        let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
        let bn_request: BaseNodeProto::BaseNodeServiceRequest = envelope_body
            .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
            .unwrap()
            .unwrap();

        let request_hashes = if let Request::FetchMatchingUtxos(outputs) = bn_request.request.unwrap() {
            outputs.outputs
        } else {
            assert!(false, "Wrong request type");
            Vec::new()
        };

        if request_hashes.iter().find(|i| **i == invalid_hash).is_some() {
            invalid_request_key = bn_request.request_key;
        } else if request_hashes.iter().find(|i| **i == output1_hash).is_some() {
            unspent_request_key_with_output1 = bn_request.request_key;
        } else {
            unspent_request_key2 = bn_request.request_key;
        }
    }
    assert_ne!(invalid_request_key, 0, "Should have found invalid request key");
    assert_ne!(
        unspent_request_key_with_output1, 0,
        "Should have found request key for request with output 1 in it"
    );
    assert_ne!(
        unspent_request_key2, 0,
        "Should have found request key for second unspent outputs request"
    );

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 1);
    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: invalid_request_key,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: vec![invalid_output.clone().as_transaction_output(&factories).unwrap().into()].into(),
            },
        )),
        is_synced: true,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let OutputManagerEvent::TxoValidationSuccess(_) = event.unwrap() {
                        acc += 1;
                        if acc >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(acc >= 1, "Did not receive enough responses");
    });

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 0);

    let value5 = 1000;
    let output5 = OutputBuilder::new()
        .with_value(value5)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(oms.add_output(output5.clone())).unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 0);

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: 1,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: vec![output1.clone().as_transaction_output(&factories).unwrap().into()].into(),
            },
        )),
        is_synced: true,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 0);

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: unspent_request_key_with_output1,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: vec![output1.clone().as_transaction_output(&factories).unwrap().into()].into(),
            },
        )),
        is_synced: true,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: unspent_request_key2,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![].into() },
        )),
        is_synced: true,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let OutputManagerEvent::TxoValidationSuccess(_) = event.unwrap() {
                        acc += 1;
                        if acc >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(acc >= 1, "Did not receive enough responses2");
    });

    let invalid_outputs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_outputs.len(), 3);

    let unspent_outputs = runtime.block_on(oms.get_unspent_outputs()).unwrap();
    assert_eq!(unspent_outputs.len(), 3);
    assert!(unspent_outputs.iter().find(|uo| uo == &&output1).is_some());
    assert!(unspent_outputs.iter().find(|uo| uo == &&output2).is_none());
    assert!(unspent_outputs.iter().find(|uo| uo == &&output3).is_none());
    assert!(unspent_outputs.iter().find(|uo| uo == &&output4).is_none());

    // test what happens if 'is_synced' is false
    runtime
        .block_on(oms.validate_txos(TxoValidationType::Unspent, TxoValidationRetry::Limited(1)))
        .unwrap();

    outbound_service.wait_call_count(2, Duration::from_secs(60)).unwrap();

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: bn_request.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![].into() },
        )),
        is_synced: false,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let OutputManagerEvent::TxoValidationAborted(_r) = event.unwrap() {
                        acc += 1;
                        if acc >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(acc, 1, "Did not receive enough responses3");
    });

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 3);
    let _ = outbound_service.take_calls();

    runtime
        .block_on(oms.validate_txos(TxoValidationType::Unspent, TxoValidationRetry::Limited(1)))
        .unwrap();

    outbound_service.wait_call_count(2, Duration::from_secs(60)).unwrap();

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request2: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 3);

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: bn_request.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![].into() },
        )),
        is_synced: true,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let base_node_response2 = BaseNodeProto::BaseNodeServiceResponse {
        request_key: bn_request2.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![].into() },
        )),
        is_synced: true,
    };
    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response2,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let OutputManagerEvent::TxoValidationSuccess(_r) = event.unwrap() {
                        acc += 1;
                        if acc >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(acc, 1, "Did not receive enough responses4");
    });

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 6);
}

#[test]
fn test_spent_txo_validation() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let mut hashes = Vec::new();
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 500;
    let output1 = OutputBuilder::new()
        .with_value(MicroTari::from(value1))
        .with_spending_key(key1.clone())
        .build(&factories.commitment)
        .unwrap();
    let tx_output1 = output1.as_transaction_output(&factories).unwrap();
    let output1_hash = tx_output1.hash();
    hashes.push(output1_hash.clone());
    backend
        .write(WriteOperation::Insert(DbKeyValuePair::SpentOutput(
            output1.spending_key().clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(output1.clone(), &factories).unwrap()),
        )))
        .unwrap();

    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    let output2 = OutputBuilder::new()
        .with_value(MicroTari::from(value2))
        .with_spending_key(key2.clone())
        .build(&factories.commitment)
        .unwrap();
    let tx_output2 = output2.as_transaction_output(&factories).unwrap();
    let output2_hash = tx_output2.hash();
    hashes.push(output2_hash.clone());

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::SpentOutput(
            output2.spending_key().clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(output2.clone(), &factories).unwrap()),
        )))
        .unwrap();

    let (mut oms, outbound_service, _shutdown, mut base_node_response_sender, _) =
        setup_output_manager_service(&mut runtime, backend);
    let mut event_stream = oms.get_event_stream_fused();

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, MicroTari::from(0));
    let base_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/58217".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    runtime
        .block_on(oms.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();
    runtime
        .block_on(oms.validate_txos(TxoValidationType::Spent, TxoValidationRetry::UntilSuccess))
        .unwrap();

    outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .expect("call wait 1");

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request1: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();
    let mut hashes_found = 0;

    match bn_request1.request {
        None => assert!(false, "Invalid request"),
        Some(request) => match request {
            Request::FetchMatchingUtxos(hash_outputs) => {
                assert_eq!(hash_outputs.outputs.len(), 2, "There should be 2 hashes in the query");
                for h in hash_outputs.outputs {
                    if hashes.iter().find(|i| **i == h).is_some() {
                        hashes_found += 1;
                    }
                }
            },
            _ => assert!(false, "invalid request"),
        },
    }
    assert_eq!(hashes_found, 2, "Should find both hashes");

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: bn_request1.request_key,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: vec![tx_output1.into()].into(),
            },
        )),
        is_synced: true,
    };
    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut acc: u64 = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let OutputManagerEvent::TxoValidationSuccess(_) = event.unwrap() {
                        acc += 1;
                        if acc >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(acc >= 1, "Did not receive enough responses");
    });
    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, MicroTari::from(value1));
}

fn sending_transaction_with_short_term_clear<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let available_balance = 10_000 * uT;
    let uo = OutputBuilder::new()
        .with_value(available_balance)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(oms.add_output(uo.clone())).unwrap();

    // Check that funds are encumbered and then unencumbered if the pending tx is not confirmed before restart
    let _stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    let expected_change = balance.pending_incoming_balance;
    assert_eq!(balance.pending_outgoing_balance, available_balance);

    drop(oms);
    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, available_balance);

    // Check that a unconfirm Pending Transaction can be cancelled
    let stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();
    let sender_tx_id = stp.get_tx_id().unwrap();

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.pending_outgoing_balance, available_balance);
    runtime.block_on(oms.cancel_transaction(sender_tx_id)).unwrap();

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, available_balance);

    // Check that is the pending tx is confirmed that the encumberance persists after restart
    let stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();
    let sender_tx_id = stp.get_tx_id().unwrap();
    runtime.block_on(oms.confirm_pending_transaction(sender_tx_id)).unwrap();

    drop(oms);
    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.pending_outgoing_balance, available_balance);

    let tx = runtime.block_on(complete_transaction(stp, oms.clone()));

    runtime
        .block_on(oms.confirm_transaction(sender_tx_id, tx.body.inputs().clone(), tx.body.outputs().clone()))
        .unwrap();

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, expected_change);
}

#[test]
fn sending_transaction_with_short_term_clear_memory_db() {
    sending_transaction_with_short_term_clear(OutputManagerMemoryDatabase::new());
}

#[test]
fn sending_transaction_with_short_term_clear_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    sending_transaction_with_short_term_clear(OutputManagerSqliteDatabase::new(connection, None));
}

fn coin_split_with_change<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let val1 = 6_000 * uT;
    let val2 = 7_000 * uT;
    let val3 = 8_000 * uT;
    let uo1 = OutputBuilder::new()
        .with_value(val1)
        .build(&factories.commitment)
        .unwrap();
    let uo2 = OutputBuilder::new()
        .with_value(val2)
        .build(&factories.commitment)
        .unwrap();
    let uo3 = OutputBuilder::new()
        .with_value(val3)
        .build(&factories.commitment)
        .unwrap();
    assert!(runtime.block_on(oms.add_output(uo1)).is_ok());
    assert!(runtime.block_on(oms.add_output(uo2)).is_ok());
    assert!(runtime.block_on(oms.add_output(uo3)).is_ok());

    let fee_per_gram = MicroTari::from(25);
    let split_count = 8;
    let (_tx_id, coin_split_tx, fee, amount) = runtime
        .block_on(oms.create_coin_split(1000.into(), split_count, fee_per_gram, None))
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 2);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count + 1);
    assert_eq!(fee, Fee::calculate(fee_per_gram, 1, 2, split_count + 1));
    assert_eq!(amount, val2 + val3);
}

#[test]
fn coin_split_with_change_memory_db() {
    coin_split_with_change(OutputManagerMemoryDatabase::new());
}

#[test]
fn coin_split_with_change_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    coin_split_with_change(OutputManagerSqliteDatabase::new(connection, None));
}

fn coin_split_no_change<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let fee_per_gram = MicroTari::from(25);
    let split_count = 15;
    let fee = Fee::calculate(fee_per_gram, 1, 3, 15);
    let val1 = 4_000 * uT;
    let val2 = 5_000 * uT;
    let val3 = 6_000 * uT + fee;
    let uo1 = OutputBuilder::new()
        .with_value(val1)
        .build(&factories.commitment)
        .unwrap();
    let uo2 = OutputBuilder::new()
        .with_value(val2)
        .build(&factories.commitment)
        .unwrap();
    let uo3 = OutputBuilder::new()
        .with_value(val3)
        .build(&factories.commitment)
        .unwrap();
    assert!(runtime.block_on(oms.add_output(uo1)).is_ok());
    assert!(runtime.block_on(oms.add_output(uo2)).is_ok());
    assert!(runtime.block_on(oms.add_output(uo3)).is_ok());

    let (_tx_id, coin_split_tx, fee, amount) = runtime
        .block_on(oms.create_coin_split(1000.into(), split_count, fee_per_gram, None))
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 3);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count);
    assert_eq!(fee, Fee::calculate(fee_per_gram, 1, 3, split_count));
    assert_eq!(amount, val1 + val2 + val3);
}

#[test]
fn coin_split_no_change_memory_db() {
    coin_split_no_change(OutputManagerMemoryDatabase::new());
}

#[test]
fn coin_split_no_change_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    coin_split_no_change(OutputManagerSqliteDatabase::new(connection, None));
}

fn handle_coinbase<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let value1 = MicroTari::from(1000);
    let value2 = MicroTari::from(2000);
    let value3 = MicroTari::from(4000);
    let recv_key1 = runtime.block_on(oms.get_coinbase_spending_key(1, value1, 1)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        value1
    );
    let recv_key2 = runtime.block_on(oms.get_coinbase_spending_key(2, value2, 1)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        value2
    );
    let recv_key3 = runtime.block_on(oms.get_coinbase_spending_key(3, value3, 2)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 2);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        value2 + value3
    );

    assert_eq!(recv_key1, recv_key2);
    assert_ne!(recv_key1, recv_key3);
    let output = OutputBuilder::new()
        .with_value(value3)
        .with_spending_key(recv_key3)
        .with_features(OutputFeatures::create_coinbase(3))
        .build(&factories.commitment)
        .and_then(|o| o.as_transaction_output(&factories))
        .unwrap();

    runtime
        .block_on(oms.confirm_transaction(3, vec![], vec![output]))
        .unwrap();

    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 1);
    assert_eq!(runtime.block_on(oms.get_balance()).unwrap().available_balance, value3);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        MicroTari::from(value2)
    );
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_outgoing_balance,
        MicroTari::from(0)
    );
}

#[test]
fn handle_coinbase_memory_db() {
    handle_coinbase(OutputManagerMemoryDatabase::new());
}

#[test]
fn handle_coinbase_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();

    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    handle_coinbase(OutputManagerSqliteDatabase::new(connection, None));
}
