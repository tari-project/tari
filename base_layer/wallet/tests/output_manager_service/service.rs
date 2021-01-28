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
    FutureExt,
    SinkExt,
    StreamExt,
};
use prost::Message;
use rand::{rngs::OsRng, RngCore};
use std::{collections::HashSet, thread, time::Duration};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::{
    consensus::{ConsensusConstantsBuilder, Network},
    proto::generated::{
        base_node as base_node_proto,
        base_node::{
            base_node_service_request::Request,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    transactions::{
        fee::Fee,
        tari_amount::{uT, MicroTari},
        transaction::{KernelFeatures, OutputFeatures, Transaction, UnblindedOutput},
        transaction_protocol::{
            recipient::RecipientState,
            sender::TransactionSenderMessage,
            single_receiver::SingleReceiverTransactionProtocol,
        },
        types::{CryptoFactories, PrivateKey},
        SenderTransactionProtocol,
    },
};
use tari_crypto::{hash::blake2::Blake256, keys::SecretKey, tari_utilities::hash::Hashable};
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
        TxId,
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
    Sender<DomainMessage<base_node_proto::BaseNodeServiceResponse>>,
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

    let constants = ConsensusConstantsBuilder::new(Network::Ridcully).build();

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
            constants,
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
                .value,
            stp.get_amount_to_self().unwrap()
        );
    }
    let msg = stp.build_single_round_message().unwrap();
    let b = TestParams::new(&mut OsRng);
    let recv_info = SingleReceiverTransactionProtocol::create(
        &msg,
        b.nonce,
        b.spend_key,
        OutputFeatures::default(),
        &factories,
        None,
    )
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

    let stp = runtime
        .block_on(oms.prepare_transaction_to_send(MicroTari::from(1000), MicroTari::from(20), None, "".to_string()))
        .unwrap();

    let sender_tx_id = stp.get_tx_id().unwrap();

    let tx = runtime.block_on(complete_transaction(stp, oms.clone()));

    let rewind_public_keys = runtime.block_on(oms.get_rewind_public_keys()).unwrap();

    // 1 of the 2 outputs should be rewindable, there should be 2 outputs due to change but if we get unlucky enough
    // that there is no change we will skip this aspect of the test
    if tx.body.outputs().len() > 1 {
        let mut num_rewound = 0;

        let output = tx.body.outputs()[0].clone();
        match output.rewind_range_proof_value_only(
            &factories.range_proof,
            &rewind_public_keys.rewind_public_key,
            &rewind_public_keys.rewind_blinding_public_key,
        ) {
            Ok(_) => {
                num_rewound += 1;
            },
            Err(_) => {},
        }

        let output = tx.body.outputs()[1].clone();
        match output.rewind_range_proof_value_only(
            &factories.range_proof,
            &rewind_public_keys.rewind_public_key,
            &rewind_public_keys.rewind_blinding_public_key,
        ) {
            Ok(_) => {
                num_rewound += 1;
            },
            Err(_) => {},
        }
        assert_eq!(num_rewound, 1, "Should only be 1 rewindable output");
    }

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

fn fee_estimate<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let (_, uo) = make_input(&mut OsRng.clone(), MicroTari::from(3000), &factories.commitment);
    runtime.block_on(oms.add_output(uo.clone())).unwrap();

    // minimum fee
    let fee_per_gram = MicroTari::from(1);
    let fee = runtime
        .block_on(oms.fee_estimate(MicroTari::from(100), fee_per_gram, 1, 1))
        .unwrap();
    assert_eq!(fee, MicroTari::from(100));

    let fee_per_gram = MicroTari::from(25);
    for outputs in 1..5 {
        let fee = runtime
            .block_on(oms.fee_estimate(MicroTari::from(100), fee_per_gram, 1, outputs))
            .unwrap();
        assert_eq!(fee, Fee::calculate(fee_per_gram, 1, 1, outputs as usize));
    }

    // not enough funds
    let err = runtime
        .block_on(oms.fee_estimate(MicroTari::from(2750), fee_per_gram, 1, 1))
        .unwrap_err();
    assert!(matches!(err, OutputManagerError::NotEnoughFunds));
}

#[test]
fn fee_estimate_memory_db() {
    fee_estimate(OutputManagerMemoryDatabase::new());
}

#[test]
fn fee_estimate_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    fee_estimate(OutputManagerSqliteDatabase::new(connection, None));
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

    let recv_info = SingleReceiverTransactionProtocol::create(
        &msg,
        b.nonce,
        b.spend_key,
        OutputFeatures::default(),
        &factories,
        None,
    )
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
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();

    send_not_enough_for_change(OutputManagerSqliteDatabase::new(connection, None));
}

fn generate_sender_transaction_message(amount: MicroTari) -> (TxId, TransactionSenderMessage) {
    let factories = CryptoFactories::default();

    let alice = TestParams::new(&mut OsRng);

    let (utxo, input) = make_input(&mut OsRng, 2 * amount, &factories.commitment);
    let mut builder = SenderTransactionProtocol::builder(1);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari(20))
        .with_offset(alice.offset.clone())
        .with_private_nonce(alice.nonce.clone())
        .with_change_secret(alice.change_key.clone())
        .with_input(utxo.clone(), input)
        .with_amount(0, amount);
    let mut stp = builder.build::<Blake256>(&factories).unwrap();
    let tx_id = stp.get_tx_id().unwrap();
    (
        tx_id,
        TransactionSenderMessage::new_single_round_message(stp.build_single_round_message().unwrap()),
    )
}

fn receiving_and_confirmation<T: OutputManagerBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let value = MicroTari::from(5000);
    let (tx_id, sender_message) = generate_sender_transaction_message(value);
    let rtp = runtime.block_on(oms.get_recipient_transaction(sender_message)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);

    let output = match rtp.state {
        RecipientState::Finalized(s) => s.output,
        RecipientState::Failed(_) => panic!("Should not be in Failed state"),
    };

    runtime
        .block_on(oms.confirm_transaction(tx_id, vec![], vec![output]))
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
    let (_tx_id, sender_message) = generate_sender_transaction_message(recv_value);
    let _rtp = runtime.block_on(oms.get_recipient_transaction(sender_message)).unwrap();

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
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let value = MicroTari::from(5000);
    let (tx_id, sender_message) = generate_sender_transaction_message(value);
    let rtp = runtime.block_on(oms.get_recipient_transaction(sender_message)).unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);

    let output = match rtp.state {
        RecipientState::Finalized(s) => s.output,
        RecipientState::Failed(_) => panic!("Should not be in Failed state"),
    };
    runtime
        .block_on(oms.confirm_transaction(tx_id, vec![], vec![output.clone()]))
        .unwrap();
    assert_eq!(runtime.block_on(oms.get_balance()).unwrap().available_balance, value);

    let factories = CryptoFactories::default();
    let rewind_public_keys = runtime.block_on(oms.get_rewind_public_keys()).unwrap();
    let rewind_result = output
        .rewind_range_proof_value_only(
            &factories.range_proof,
            &rewind_public_keys.rewind_public_key,
            &rewind_public_keys.rewind_blinding_public_key,
        )
        .unwrap();
    assert_eq!(rewind_result.committed_value, value);
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
    let invalid_output = UnblindedOutput::new(MicroTari::from(invalid_value), invalid_key.clone(), None);
    let invalid_hash = invalid_output.as_transaction_output(&factories).unwrap().hash();

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
            invalid_output.spending_key.clone(),
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
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 500;
    let output1 = UnblindedOutput::new(MicroTari::from(value1), key1.clone(), None);
    let tx_output1 = output1.as_transaction_output(&factories).unwrap();
    let output1_hash = tx_output1.hash();
    hashes.push(output1_hash.clone());
    runtime.block_on(oms.add_output(output1.clone())).unwrap();

    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    let output2 = UnblindedOutput::new(MicroTari::from(value2), key2.clone(), None);
    let tx_output2 = output2.as_transaction_output(&factories).unwrap();
    hashes.push(tx_output2.hash());

    runtime.block_on(oms.add_output(output2.clone())).unwrap();

    let key3 = PrivateKey::random(&mut OsRng);
    let value3 = 900;
    let output3 = UnblindedOutput::new(MicroTari::from(value3), key3.clone(), None);
    let tx_output3 = output3.as_transaction_output(&factories).unwrap();
    hashes.push(tx_output3.hash());

    runtime.block_on(oms.add_output(output3.clone())).unwrap();

    let key4 = PrivateKey::random(&mut OsRng);
    let value4 = 901;
    let output4 = UnblindedOutput::new(MicroTari::from(value4), key4.clone(), None);
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
    let bn_request1: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
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
    let bn_request2: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
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
    let bn_request3: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
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
        let bn_request: base_node_proto::BaseNodeServiceRequest = envelope_body
            .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
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
    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: invalid_request_key,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs {
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

    let key5 = PrivateKey::random(&mut OsRng);
    let value5 = 1000;
    let output5 = UnblindedOutput::new(MicroTari::from(value5), key5, None);
    runtime.block_on(oms.add_output(output5.clone())).unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 0);

    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: 1,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs {
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

    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: unspent_request_key_with_output1,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs {
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

    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: unspent_request_key2,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs { outputs: vec![].into() },
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
    let bn_request: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: bn_request.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs { outputs: vec![].into() },
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
    let bn_request: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let (_, body) = outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bn_request2: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
        .unwrap()
        .unwrap();

    let invalid_txs = runtime.block_on(oms.get_invalid_outputs()).unwrap();
    assert_eq!(invalid_txs.len(), 3);

    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: bn_request.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs { outputs: vec![].into() },
        )),
        is_synced: true,
    };

    runtime
        .block_on(base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let base_node_response2 = base_node_proto::BaseNodeServiceResponse {
        request_key: bn_request2.request_key.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs { outputs: vec![].into() },
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
    let output1 = UnblindedOutput::new(MicroTari::from(value1), key1.clone(), None);
    let tx_output1 = output1.as_transaction_output(&factories).unwrap();
    let output1_hash = tx_output1.hash();
    hashes.push(output1_hash.clone());
    backend
        .write(WriteOperation::Insert(DbKeyValuePair::SpentOutput(
            output1.spending_key.clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(output1.clone(), &factories).unwrap()),
        )))
        .unwrap();

    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    let output2 = UnblindedOutput::new(MicroTari::from(value2), key2.clone(), None);
    let tx_output2 = output2.as_transaction_output(&factories).unwrap();
    let output2_hash = tx_output2.hash();
    hashes.push(output2_hash.clone());

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::SpentOutput(
            output2.spending_key.clone(),
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
    let bn_request1: base_node_proto::BaseNodeServiceRequest = envelope_body
        .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
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

    let base_node_response = base_node_proto::BaseNodeServiceResponse {
        request_key: bn_request1.request_key,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            base_node_proto::TransactionOutputs {
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
    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, MicroTari::from(value1));
}

fn sending_transaction_with_short_term_clear<T: Clone + OutputManagerBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend.clone());

    let available_balance = 10_000 * uT;
    let (_ti, uo) = make_input(&mut OsRng.clone(), available_balance, &factories.commitment);
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
    let (_ti, uo1) = make_input(&mut OsRng.clone(), val1, &factories.commitment);
    let (_ti, uo2) = make_input(&mut OsRng.clone(), val2, &factories.commitment);
    let (_ti, uo3) = make_input(&mut OsRng.clone(), val3, &factories.commitment);
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
    let (_ti, uo1) = make_input(&mut OsRng.clone(), val1, &factories.commitment);
    let (_ti, uo2) = make_input(&mut OsRng.clone(), val2, &factories.commitment);
    let (_ti, uo3) = make_input(&mut OsRng.clone(), val3, &factories.commitment);
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
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let (mut oms, _, _shutdown, _, _) = setup_output_manager_service(&mut runtime, backend);

    let reward1 = MicroTari::from(1000);
    let fees1 = MicroTari::from(500);
    let value1 = reward1 + fees1;
    let reward2 = MicroTari::from(2000);
    let fees2 = MicroTari::from(500);
    let value2 = reward2 + fees2;
    let reward3 = MicroTari::from(3000);
    let fees3 = MicroTari::from(500);
    let value3 = reward3 + fees3;

    let _ = runtime
        .block_on(oms.get_coinbase_transaction(1, reward1, fees1, 1))
        .unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        value1
    );
    let _tx2 = runtime
        .block_on(oms.get_coinbase_transaction(2, reward2, fees2, 1))
        .unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 1);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        value2
    );
    let tx3 = runtime
        .block_on(oms.get_coinbase_transaction(3, reward3, fees3, 2))
        .unwrap();
    assert_eq!(runtime.block_on(oms.get_unspent_outputs()).unwrap().len(), 0);
    assert_eq!(runtime.block_on(oms.get_pending_transactions()).unwrap().len(), 2);
    assert_eq!(
        runtime.block_on(oms.get_balance()).unwrap().pending_incoming_balance,
        value2 + value3
    );

    let output = tx3.body.outputs()[0].clone();

    let rewind_public_keys = runtime.block_on(oms.get_rewind_public_keys()).unwrap();
    let rewind_result = output
        .rewind_range_proof_value_only(
            &factories.range_proof,
            &rewind_public_keys.rewind_public_key,
            &rewind_public_keys.rewind_blinding_public_key,
        )
        .unwrap();
    assert_eq!(rewind_result.committed_value, value3);

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

#[test]
fn test_base_node_switching_triggered_txo_validation() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();
    let backend = OutputManagerSqliteDatabase::new(connection, None);

    let mut hashes = HashSet::new();

    // This output will be created and then invalidated
    let key1 = PrivateKey::random(&mut OsRng);
    let value1 = 5000;
    let output1 = UnblindedOutput::new(MicroTari::from(value1), key1.clone(), None);
    let tx_output1 = output1.as_transaction_output(&factories).unwrap();
    let output1_hash = tx_output1.hash();
    hashes.insert(output1_hash.clone());
    backend
        .write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
            output1.spending_key.clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(output1.clone(), &factories).unwrap()),
        )))
        .unwrap();

    backend
        .invalidate_unspent_output(&DbUnblindedOutput::from_unblinded_output(output1.clone(), &factories).unwrap())
        .unwrap();

    let key2 = PrivateKey::random(&mut OsRng);
    let value2 = 800;
    let output2 = UnblindedOutput::new(MicroTari::from(value2), key2.clone(), None);
    let tx_output2 = output2.as_transaction_output(&factories).unwrap();
    let output2_hash = tx_output2.hash();
    hashes.insert(output2_hash.clone());

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::UnspentOutput(
            output2.spending_key.clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(output2.clone(), &factories).unwrap()),
        )))
        .unwrap();

    let key3 = PrivateKey::random(&mut OsRng);
    let value3 = 1300;
    let output3 = UnblindedOutput::new(MicroTari::from(value3), key3.clone(), None);
    let tx_output3 = output3.as_transaction_output(&factories).unwrap();
    let output3_hash = tx_output3.hash();
    hashes.insert(output3_hash.clone());

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::SpentOutput(
            output3.spending_key.clone(),
            Box::new(DbUnblindedOutput::from_unblinded_output(output3.clone(), &factories).unwrap()),
        )))
        .unwrap();

    let (mut oms, outbound_service, _shutdown, _base_node_response_sender, _) =
        setup_output_manager_service(&mut runtime, backend);
    let mut event_stream = oms.get_event_stream_fused();

    let balance = runtime.block_on(oms.get_balance()).unwrap();
    assert_eq!(balance.available_balance, MicroTari::from(value2));
    let base_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/58217".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    runtime
        .block_on(oms.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    // Make sure that setting the base_node the first time does not trigger any validation automatically
    assert!(outbound_service.wait_call_count(1, Duration::from_secs(5)).is_err());

    // Trigger 3 validations:
    runtime
        .block_on(oms.validate_txos(TxoValidationType::Unspent, TxoValidationRetry::UntilSuccess))
        .unwrap();
    runtime
        .block_on(oms.validate_txos(TxoValidationType::Spent, TxoValidationRetry::UntilSuccess))
        .unwrap();
    runtime
        .block_on(oms.validate_txos(TxoValidationType::Invalid, TxoValidationRetry::UntilSuccess))
        .unwrap();

    outbound_service
        .wait_call_count(3, Duration::from_secs(60))
        .expect("call wait 3");

    // Setting a base node a second time will cancel the previous protocols and trigger 3 more.
    runtime
        .block_on(oms.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    // Check we get 3 protocol abort events
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let OutputManagerEvent::TxoValidationAborted(_) = event.unwrap() {
                        acc += 1;
                        if acc >= 3 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(acc >= 3, "Did not receive enough abort events");
    });

    outbound_service
        .wait_call_count(3, Duration::from_secs(60))
        .expect("call wait 3");

    let mut hashes_found = 0;
    for _ in 0..3 {
        let (_, body) = outbound_service.pop_call().unwrap();
        let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
        let bn_request: base_node_proto::BaseNodeServiceRequest = envelope_body
            .decode_part::<base_node_proto::BaseNodeServiceRequest>(1)
            .unwrap()
            .unwrap();

        match bn_request.request {
            None => assert!(false, "Invalid request"),
            Some(request) => match request {
                Request::FetchMatchingUtxos(hash_outputs) => {
                    for h in hash_outputs.outputs {
                        if hashes.remove(h.as_slice()) {
                            hashes_found += 1;
                            break;
                        }
                    }
                },
                _ => assert!(false, "invalid request"),
            },
        }
    }

    assert_eq!(
        hashes_found, 3,
        "Should have found all three hashes in three separate requests."
    );
}
