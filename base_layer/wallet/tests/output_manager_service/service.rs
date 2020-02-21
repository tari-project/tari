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
    utils::{make_input, random_string, TestParams},
};
use futures::{
    channel::{mpsc, mpsc::Sender},
    SinkExt,
};
use prost::Message;
use rand::{rngs::OsRng, RngCore};
use std::{thread, time::Duration};
use tari_broadcast_channel::bounded;
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::base_node_service_response::Response as BaseNodeResponseProto,
    },
    transactions::{
        fee::Fee,
        tari_amount::MicroTari,
        transaction::{KernelFeatures, OutputFeatures, TransactionOutput, UnblindedOutput},
        transaction_protocol::single_receiver::SingleReceiverTransactionProtocol,
        types::{CryptoFactories, PrivateKey, RangeProof},
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::SecretKey,
    range_proof::RangeProofService,
    tari_utilities::ByteArray,
};
use tari_p2p::domain_message::DomainMessage;
use tari_service_framework::reply_channel;
use tari_shutdown::Shutdown;
use tari_test_utils::collect_stream;
use tari_wallet::{
    output_manager_service::{
        config::OutputManagerServiceConfig,
        error::{OutputManagerError, OutputManagerStorageError},
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::OutputManagerService,
        storage::{
            database::{DbKey, DbValue, OutputManagerBackend, OutputManagerDatabase},
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
) -> (
    OutputManagerHandle,
    OutboundServiceMockState,
    Shutdown,
    Sender<DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
)
{
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();

    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(20);
    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(20);
    let (oms_event_publisher, oms_event_subscriber) = bounded(100);

    let output_manager_service = runtime
        .block_on(OutputManagerService::new(
            OutputManagerServiceConfig {
                base_node_query_timeout_in_secs: 3,
            },
            outbound_message_requester.clone(),
            oms_request_receiver,
            base_node_response_receiver,
            OutputManagerDatabase::new(backend),
            oms_event_publisher,
            factories.clone(),
        ))
        .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_subscriber);

    runtime.spawn(async move { output_manager_service.start().await.unwrap() });

    let outbound_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());

    (
        output_manager_service_handle,
        outbound_mock_state,
        shutdown,
        base_node_response_sender,
    )
}

fn sending_transaction_and_confirmation<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let (_ti, uo) = make_input(
        &mut OsRng.clone(),
        MicroTari::from(100 + OsRng.next_u64() % 1000),
        &factories.commitment,
    );
    runtime.block_on(oms.add_output(uo.clone())).unwrap();
    match runtime.block_on(oms.add_output(uo)) {
        Err(OutputManagerError::OutputManagerStorageError(OutputManagerStorageError::DuplicateOutput)) => assert!(true),
        _ => assert!(false, "Incorrect error message"),
    };
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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();

    sending_transaction_and_confirmation(OutputManagerSqliteDatabase::new(connection_pool));
}

fn send_not_enough_funds<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);
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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();

    send_not_enough_funds(OutputManagerSqliteDatabase::new(connection_pool));
}

fn send_no_change<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();

    send_no_change(OutputManagerSqliteDatabase::new(connection_pool));
}

fn send_not_enough_for_change<T: OutputManagerBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();

    send_not_enough_for_change(OutputManagerSqliteDatabase::new(connection_pool));
}

fn receiving_and_confirmation<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();

    receiving_and_confirmation(OutputManagerSqliteDatabase::new(connection_pool));
}

fn cancel_transaction<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();
    cancel_transaction(OutputManagerSqliteDatabase::new(connection_pool));
}

fn timeout_transaction<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();
    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();
    timeout_transaction(OutputManagerSqliteDatabase::new(connection_pool));
}

fn test_get_balance<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();
    test_get_balance(OutputManagerSqliteDatabase::new(connection_pool));
}

fn test_confirming_received_output<T: OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _) = setup_output_manager_service(&mut runtime, backend);

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
    let connection_pool = run_migration_and_create_connection_pool(&db_path).unwrap();
    test_confirming_received_output(OutputManagerSqliteDatabase::new(connection_pool));
}

#[test]
fn test_startup_utxo_scan() {
    let factories = CryptoFactories::default();

    let mut runtime = Runtime::new().unwrap();

    let (mut oms, outbound_service, _shutdown, mut base_node_response_sender) =
        setup_output_manager_service(&mut runtime, OutputManagerMemoryDatabase::new());
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 500;
    let output1 = UnblindedOutput::new(MicroTari::from(value1), key1, None);

    runtime.block_on(oms.add_output(output1.clone())).unwrap();
    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    let output2 = UnblindedOutput::new(MicroTari::from(value2), key2, None);
    runtime.block_on(oms.add_output(output2.clone())).unwrap();

    let base_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/58217".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    runtime
        .block_on(oms.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            oms.get_event_stream_fused().map(|i| (*i).clone()),
            take = 1,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        1,
        result_stream.iter().fold(0, |acc, item| {
            if let OutputManagerEvent::BaseNodeSyncRequestTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let call = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let bn_request: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 0);

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: 1,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: vec![output1.clone().as_transaction_output(&factories).unwrap().into()].into(),
            },
        )),
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
        request_key: bn_request.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: vec![output1.clone().as_transaction_output(&factories).unwrap().into()].into(),
            },
        )),
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            oms.get_event_stream_fused().map(|i| (*i).clone()),
            take = 2,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        1,
        result_stream.iter().fold(0, |acc, item| {
            if let OutputManagerEvent::ReceiveBaseNodeResponse(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let invalid_outputs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_outputs.len(), 1);
    assert_eq!(invalid_outputs[0], output2);

    let unspent_outputs = runtime.block_on(oms.get_unspent_outputs()).unwrap();
    assert_eq!(unspent_outputs.len(), 1);
    assert_eq!(unspent_outputs[0], output1);

    runtime.block_on(oms.sync_with_base_node()).unwrap();

    let call = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let bn_request: BaseNodeProto::BaseNodeServiceRequest = envelope_body
        .decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 1);

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: bn_request.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![].into() },
        )),
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            oms.get_event_stream_fused().map(|i| (*i).clone()),
            take = 3,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        2,
        result_stream.iter().fold(0, |acc, item| {
            if let OutputManagerEvent::ReceiveBaseNodeResponse(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 2);
}
