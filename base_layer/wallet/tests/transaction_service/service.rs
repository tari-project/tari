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
    comms_and_services::{create_dummy_message, setup_comms_services},
    utils::{make_input, random_string, TestParams},
};
use futures::{
    channel::{mpsc, mpsc::Sender},
    SinkExt,
};
use prost::Message;
use rand::rngs::OsRng;
use std::{convert::TryInto, sync::Arc, time::Duration};
use tari_broadcast_channel::bounded;
use tari_comms::{
    builder::CommsNode,
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::transactions::{
    tari_amount::*,
    transaction::{KernelBuilder, KernelFeatures, OutputFeatures, Transaction, TransactionOutput},
    transaction_protocol::{
        proto,
        recipient::{RecipientSignedMessage, RecipientState},
        sender::TransactionSenderMessage,
    },
    types::{CryptoFactories, PrivateKey, PublicKey, RangeProof, Signature},
    ReceiverTransactionProtocol,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey as SK},
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    domain_message::DomainMessage,
    services::comms_outbound::CommsOutboundServiceInitializer,
};
use tari_service_framework::{reply_channel, StackBuilder};
use tari_test_utils::{collect_stream, paths::with_temp_dir};
use tari_wallet::{
    output_manager_service::{
        handle::OutputManagerHandle,
        service::OutputManagerService,
        storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
        OutputManagerServiceInitializer,
    },
    transaction_service::{
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceHandle},
        service::TransactionService,
        storage::{
            database::{TransactionBackend, TransactionDatabase},
            memory_db::TransactionMemoryDatabase,
            sqlite_db::TransactionServiceSqliteDatabase,
        },
        TransactionServiceInitializer,
    },
};
use tempdir::TempDir;
use tokio::runtime::{Builder, Runtime};

fn create_runtime() -> Runtime {
    Builder::new()
        .threaded_scheduler()
        .enable_all()
        .core_threads(8)
        .build()
        .unwrap()
}

pub fn setup_transaction_service<T: TransactionBackend + Clone + 'static>(
    runtime: &mut Runtime,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    factories: CryptoFactories,
    backend: T,
    database_path: String,
    discovery_request_timeout: Duration,
) -> (TransactionServiceHandle, OutputManagerHandle, CommsNode)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.handle().clone(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(
        runtime.handle().clone(),
        Arc::new(node_identity.clone()),
        peers,
        publisher,
        database_path,
        discovery_request_timeout,
    );

    let fut = StackBuilder::new(runtime.handle().clone(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(OutputManagerServiceInitializer::new(
            OutputManagerMemoryDatabase::new(),
            factories.clone(),
        ))
        .add_initializer(TransactionServiceInitializer::new(
            subscription_factory,
            backend,
            comms.node_identity().clone(),
            factories.clone(),
        ))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let output_manager_handle = handles.get_handle::<OutputManagerHandle>().unwrap();
    let transaction_service_handle = handles.get_handle::<TransactionServiceHandle>().unwrap();

    (transaction_service_handle, output_manager_handle, comms)
}

/// This utility function creates a Transaction service without using the Service Framework Stack and exposes all the
/// streams for testing purposes.
pub fn setup_transaction_service_no_comms<T: TransactionBackend + Clone + 'static>(
    runtime: &mut Runtime,
    factories: CryptoFactories,
    backend: T,
) -> (
    TransactionServiceHandle,
    OutputManagerHandle,
    OutboundServiceMockState,
    Sender<DomainMessage<proto::TransactionSenderMessage>>,
    Sender<DomainMessage<proto::RecipientSignedMessage>>,
    Sender<DomainMessage<proto::TransactionFinalizedMessage>>,
)
{
    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();
    let output_manager_service = runtime
        .block_on(OutputManagerService::new(
            oms_request_receiver,
            OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
            factories.clone(),
        ))
        .unwrap();
    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender);

    let (ts_request_sender, ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, event_subscriber) = bounded(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_subscriber);
    let (tx_sender, tx_receiver) = mpsc::channel(20);
    let (tx_ack_sender, tx_ack_receiver) = mpsc::channel(20);
    let (tx_finalized_sender, tx_finalized_receiver) = mpsc::channel(20);

    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(20);
    let outbound_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());

    let ts_service = TransactionService::new(
        TransactionDatabase::new(backend),
        ts_request_receiver,
        tx_receiver,
        tx_ack_receiver,
        tx_finalized_receiver,
        output_manager_service_handle.clone(),
        outbound_message_requester.clone(),
        event_publisher,
        Arc::new(
            NodeIdentity::random(
                &mut OsRng,
                "/ip4/0.0.0.0/tcp/41239".parse().unwrap(),
                PeerFeatures::COMMUNICATION_NODE,
            )
            .unwrap(),
        ),
        factories.clone(),
    );
    runtime.spawn(async move { output_manager_service.start().await.unwrap() });
    runtime.spawn(async move { ts_service.start().await.unwrap() });
    (
        ts_handle,
        output_manager_service_handle,
        outbound_mock_state,
        tx_sender,
        tx_ack_sender,
        tx_finalized_sender,
    )
}

fn manage_single_transaction<T: TransactionBackend + Clone + 'static>(
    alice_backend: T,
    bob_backend: T,
    port_offset: i32,
    database_path: String,
)
{
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_port = 31501 + port_offset;
    let alice_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", alice_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Bob's parameters
    let bob_port = 32713 + port_offset;
    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", bob_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(1),
    );
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment);

    assert!(runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            "".to_string()
        ))
        .is_err());

    runtime.block_on(alice_oms.add_output(uo1)).unwrap();
    let message = "TAKE MAH MONEYS!".to_string();
    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            message.clone(),
        ))
        .unwrap();
    let alice_pending_outbound = runtime.block_on(alice_ts.get_pending_outbound_transactions()).unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(alice_pending_outbound.len(), 1);
    assert_eq!(alice_completed_tx.len(), 0);

    let (mut bob_ts, mut bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path,
        Duration::from_secs(1),
    );

    assert_eq!(
        collect_stream!(
            runtime,
            alice_event_stream.map(|i| (*i).clone()),
            take = 1,
            timeout = Duration::from_secs(10)
        )
        .iter()
        .fold(0, |acc, x| match x {
            TransactionEvent::ReceivedTransactionReply(_) => acc + 1,
            _ => acc,
        }),
        1
    );

    let alice_pending_outbound = runtime.block_on(alice_ts.get_pending_outbound_transactions()).unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 1);

    let bob_pending_inbound_tx = runtime.block_on(bob_ts.get_pending_inbound_transactions()).unwrap();
    assert_eq!(bob_pending_inbound_tx.len(), 1);
    for (_k, v) in bob_pending_inbound_tx.clone().drain().take(1) {
        assert_eq!(v.message, message);
    }

    let mut alice_tx_id = 0;
    for (k, _v) in alice_completed_tx.iter() {
        alice_tx_id = k.clone();
    }
    for (k, v) in bob_pending_inbound_tx.iter() {
        assert_eq!(*k, alice_tx_id);
        if let RecipientState::Finalized(rsm) = &v.receiver_protocol.state {
            runtime
                .block_on(bob_oms.confirm_received_output(alice_tx_id, rsm.output.clone()))
                .unwrap();
            assert_eq!(
                runtime.block_on(bob_oms.get_balance()).unwrap().available_balance,
                value
            );
        } else {
            assert!(false);
        }
    }

    alice_comms.shutdown().unwrap();
    bob_comms.shutdown().unwrap();
}

#[test]
fn manage_single_transaction_memory_db() {
    with_temp_dir(|dir_path| {
        manage_single_transaction(
            TransactionMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            2,
            dir_path.to_str().unwrap().to_string(),
        );
    });
}

#[test]
fn manage_single_transaction_sqlite_db() {
    with_temp_dir(|dir_path| {
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", dir_path.to_str().unwrap(), alice_db_name);
        let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let bob_db_path = format!("{}/{}", dir_path.to_str().unwrap(), bob_db_name);
        manage_single_transaction(
            TransactionServiceSqliteDatabase::new(alice_db_path).unwrap(),
            TransactionServiceSqliteDatabase::new(bob_db_path).unwrap(),
            1,
            dir_path.to_str().unwrap().to_string(),
        );
    });
}

fn manage_multiple_transactions<T: TransactionBackend + Clone + 'static>(
    alice_backend: T,
    bob_backend: T,
    carol_backend: T,
    port_offset: i32,
    database_path: String,
)
{
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_port = 31484 + port_offset;
    let alice_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", alice_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Bob's parameters
    let bob_port = 31475 + port_offset;
    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", bob_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Carols's parameters
    let carol_port = 31488 + port_offset;
    let carol_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", carol_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(1),
    );
    let alice_event_stream = alice_ts.get_event_stream_fused();

    // Add some funds to Alices wallet
    let (_utxo, uo1a) = make_input(&mut OsRng, MicroTari(5500), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1a)).unwrap();
    let (_utxo, uo1b) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1b)).unwrap();
    let (_utxo, uo1c) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1c)).unwrap();

    // A series of interleaved transactions. First with Bob and Carol offline and then two with them online
    let value_a_to_b_1 = MicroTari::from(1000);
    let value_a_to_b_2 = MicroTari::from(800);
    let value_b_to_a_1 = MicroTari::from(1100);
    let value_a_to_c_1 = MicroTari::from(1400);
    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value_a_to_b_1,
            MicroTari::from(20),
            "".to_string(),
        ))
        .unwrap();
    runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "".to_string(),
        ))
        .unwrap();
    let alice_pending_outbound = runtime.block_on(alice_ts.get_pending_outbound_transactions()).unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(alice_pending_outbound.len(), 2);
    assert_eq!(alice_completed_tx.len(), 0);

    // Spin up Bob and Carol
    let (mut bob_ts, mut bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path.clone(),
        Duration::from_secs(1),
    );
    let (mut carol_ts, mut carol_oms, carol_comms) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        carol_backend,
        database_path,
        Duration::from_secs(1),
    );

    let bob_event_stream = bob_ts.get_event_stream_fused();
    let carol_event_stream = carol_ts.get_event_stream_fused();

    let (_utxo, uo2) = make_input(&mut OsRng, MicroTari(3500), &factories.commitment);
    runtime.block_on(bob_oms.add_output(uo2)).unwrap();
    let (_utxo, uo3) = make_input(&mut OsRng, MicroTari(4500), &factories.commitment);
    runtime.block_on(carol_oms.add_output(uo3)).unwrap();

    runtime
        .block_on(bob_ts.send_transaction(
            alice_node_identity.public_key().clone(),
            value_b_to_a_1,
            MicroTari::from(20),
            "".to_string(),
        ))
        .unwrap();
    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value_a_to_b_2,
            MicroTari::from(20),
            "".to_string(),
        ))
        .unwrap();

    assert_eq!(
        collect_stream!(
            runtime,
            alice_event_stream.map(|i| (*i).clone()),
            take = 5,
            timeout = Duration::from_secs(30)
        )
        .iter()
        .fold(0, |acc, x| match x {
            TransactionEvent::ReceivedTransactionReply(_) => acc + 1,
            _ => acc,
        }),
        3
    );

    let _ = collect_stream!(runtime, bob_event_stream, take = 5, timeout = Duration::from_secs(30));

    let alice_pending_outbound = runtime.block_on(alice_ts.get_pending_outbound_transactions()).unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 4);
    let bob_pending_outbound = runtime.block_on(bob_ts.get_pending_outbound_transactions()).unwrap();
    let bob_completed_tx = runtime.block_on(bob_ts.get_completed_transactions()).unwrap();
    assert_eq!(bob_pending_outbound.len(), 0);
    assert_eq!(bob_completed_tx.len(), 3);
    let _ = collect_stream!(runtime, carol_event_stream, take = 2, timeout = Duration::from_secs(30));
    let carol_pending_inbound = runtime.block_on(carol_ts.get_pending_inbound_transactions()).unwrap();
    let carol_completed_tx = runtime.block_on(carol_ts.get_completed_transactions()).unwrap();
    assert_eq!(carol_pending_inbound.len(), 0);
    assert_eq!(carol_completed_tx.len(), 1);

    alice_comms.shutdown().unwrap();
    bob_comms.shutdown().unwrap();
    carol_comms.shutdown().unwrap();
}

#[test]
fn manage_multiple_transactions_memory_db() {
    with_temp_dir(|dir_path| {
        manage_multiple_transactions(
            TransactionMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            0,
            dir_path.to_str().unwrap().to_string(),
        );
    });
}

#[test]
fn manage_multiple_transactions_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let bob_db_path = format!("{}/{}", path_string, bob_db_name);
        let carol_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let carol_db_path = format!("{}/{}", path_string, carol_db_name);
        manage_multiple_transactions(
            TransactionServiceSqliteDatabase::new(alice_db_path).unwrap(),
            TransactionServiceSqliteDatabase::new(bob_db_path).unwrap(),
            TransactionServiceSqliteDatabase::new(carol_db_path).unwrap(),
            1,
            path_string,
        );
    });
}

fn test_sending_repeated_tx_ids<T: TransactionBackend + Clone + 'static>(alice_backend: T, bob_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/55741".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (alice_ts, _alice_output_manager, alice_outbound_service, mut alice_tx_sender, _alice_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);

    runtime.block_on(bob_output_manager.add_output(uo)).unwrap();

    let mut stp = runtime
        .block_on(bob_output_manager.prepare_transaction_to_send(
            MicroTari::from(500),
            MicroTari::from(1000),
            None,
            "".to_string(),
        ))
        .unwrap();
    let msg = stp.build_single_round_message().unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg.clone())).into(),
        &bob_node_identity.public_key(),
    );

    runtime.block_on(alice_tx_sender.send(tx_message.clone())).unwrap();
    runtime.block_on(alice_tx_sender.send(tx_message.clone())).unwrap();

    let result = collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 2,
        timeout = Duration::from_secs(10)
    );

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();

    assert_eq!(result.len(), 2);
    assert!(result
        .iter()
        .find(|i| if let TransactionEvent::ReceivedTransaction(_) = i {
            true
        } else {
            false
        })
        .is_some());
    assert!(result
        .iter()
        .find(|i| if let TransactionEvent::Error(s) = i {
            s == &"Error handling Transaction Sender message".to_string()
        } else {
            false
        })
        .is_some());
}

#[test]
fn test_sending_repeated_tx_ids_memory_db() {
    test_sending_repeated_tx_ids(TransactionMemoryDatabase::new(), TransactionMemoryDatabase::new());
}

#[test]
fn test_sending_repeated_tx_ids_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let bob_db_path = format!("{}/{}", path_string, bob_db_name);
        test_sending_repeated_tx_ids(
            TransactionServiceSqliteDatabase::new(alice_db_path).unwrap(),
            TransactionServiceSqliteDatabase::new(bob_db_path).unwrap(),
        );
    });
}

fn test_accepting_unknown_tx_id_and_malformed_reply<T: TransactionBackend + Clone + 'static>(alice_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/31585".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let (mut alice_ts, mut alice_output_manager, alice_outbound_service, _alice_tx_sender, mut alice_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);

    let alice_event_stream = alice_ts.get_event_stream_fused();

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);

    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            MicroTari::from(500),
            MicroTari::from(1000),
            "".to_string(),
        ))
        .unwrap();
    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();
    let (_, body) = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.as_slice()).unwrap();
    let sender_message = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap();

    let params = TestParams::new(&mut OsRng);

    let rtp = ReceiverTransactionProtocol::new(
        sender_message.try_into().unwrap(),
        params.nonce,
        params.spend_key,
        OutputFeatures::default(),
        &factories,
    );

    let mut tx_reply = rtp.get_signed_data().unwrap().clone();
    let mut wrong_tx_id = tx_reply.clone();
    wrong_tx_id.tx_id = 2;
    let (_p, pub_key) = PublicKey::random_keypair(&mut OsRng);
    tx_reply.public_spend_key = pub_key;
    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            wrong_tx_id.into(),
            &bob_node_identity.public_key(),
        )))
        .unwrap();

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(tx_reply.into(), &bob_node_identity.public_key())))
        .unwrap();

    assert!(collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 2,
        timeout = Duration::from_secs(10)
    )
    .iter()
    .find(|i| if let TransactionEvent::Error(s) = i {
        s == &"Error handling Transaction Recipient Reply message".to_string()
    } else {
        false
    })
    .is_some());
}

#[test]
fn test_accepting_unknown_tx_id_and_malformed_reply_memory_db() {
    test_accepting_unknown_tx_id_and_malformed_reply(TransactionMemoryDatabase::new());
}

#[test]
fn test_accepting_unknown_tx_id_and_malformed_reply_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        test_accepting_unknown_tx_id_and_malformed_reply(TransactionServiceSqliteDatabase::new(alice_db_path).unwrap());
    });
}

fn finalize_tx_with_nonexistent_txid<T: TransactionBackend + Clone + 'static>(alice_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (
        alice_ts,
        _alice_output_manager,
        _alice_outbound_service,
        _alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let tx = Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut OsRng));
    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: 88u64,
        transaction: Some(tx.clone().into()),
    };

    runtime
        .block_on(alice_tx_finalized.send(create_dummy_message(
            finalized_transaction_message.clone(),
            &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        )))
        .unwrap();

    assert!(collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 1,
        timeout = Duration::from_secs(10)
    )
    .iter()
    .find(|i| if let TransactionEvent::Error(s) = i {
        s == &"Error handling Transaction Finalized message".to_string()
    } else {
        false
    })
    .is_some());
}

#[test]
fn finalize_tx_with_nonexistent_txid_memory_db() {
    finalize_tx_with_nonexistent_txid(TransactionMemoryDatabase::new());
}

#[test]
fn finalize_tx_with_nonexistent_txid_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        finalize_tx_with_nonexistent_txid(TransactionServiceSqliteDatabase::new(alice_db_path).unwrap());
    });
}

fn finalize_tx_with_incorrect_pubkey<T: TransactionBackend + Clone + 'static>(alice_backend: T, bob_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (
        alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/55741".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend);

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);

    runtime.block_on(bob_output_manager.add_output(uo)).unwrap();

    let mut stp = runtime
        .block_on(bob_output_manager.prepare_transaction_to_send(
            MicroTari::from(500),
            MicroTari::from(1000),
            None,
            "".to_string(),
        ))
        .unwrap();
    let msg = stp.build_single_round_message().unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg.clone())).into(),
        &bob_node_identity.public_key(),
    );

    runtime.block_on(alice_tx_sender.send(tx_message.clone())).unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();
    let (_, body) = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.as_slice()).unwrap();
    let recipient_reply: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    stp.add_single_recipient_info(recipient_reply.clone(), &factories.range_proof)
        .unwrap();
    stp.finalize(KernelFeatures::empty(), &factories).unwrap();
    let tx = stp.get_transaction().unwrap();

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: recipient_reply.tx_id,
        transaction: Some(tx.clone().into()),
    };

    runtime
        .block_on(alice_tx_finalized.send(create_dummy_message(
            finalized_transaction_message.clone(),
            &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        )))
        .unwrap();

    assert!(collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 2,
        timeout = Duration::from_secs(10)
    )
    .iter()
    .find(|i| if let TransactionEvent::Error(s) = i {
        s == &"Error handling Transaction Finalized message".to_string()
    } else {
        false
    })
    .is_some());
}

#[test]
fn finalize_tx_with_incorrect_pubkey_memory_db() {
    finalize_tx_with_incorrect_pubkey(TransactionMemoryDatabase::new(), TransactionMemoryDatabase::new());
}

#[test]
fn finalize_tx_with_incorrect_pubkey_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let bob_db_path = format!("{}/{}", path_string, bob_db_name);
        finalize_tx_with_incorrect_pubkey(
            TransactionServiceSqliteDatabase::new(alice_db_path).unwrap(),
            TransactionServiceSqliteDatabase::new(bob_db_path).unwrap(),
        );
    });
}

fn finalize_tx_with_missing_output<T: TransactionBackend + Clone + 'static>(alice_backend: T, bob_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (
        alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/55714".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend);

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);

    runtime.block_on(bob_output_manager.add_output(uo)).unwrap();

    let mut stp = runtime
        .block_on(bob_output_manager.prepare_transaction_to_send(
            MicroTari::from(500),
            MicroTari::from(1000),
            None,
            "".to_string(),
        ))
        .unwrap();
    let msg = stp.build_single_round_message().unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg.clone())).into(),
        &bob_node_identity.public_key(),
    );

    runtime.block_on(alice_tx_sender.send(tx_message.clone())).unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();
    let (_, body) = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(body.as_slice()).unwrap();
    let recipient_reply: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    stp.add_single_recipient_info(recipient_reply.clone(), &factories.range_proof)
        .unwrap();
    stp.finalize(KernelFeatures::empty(), &factories).unwrap();

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: recipient_reply.tx_id,
        transaction: Some(Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut OsRng)).into()),
    };

    runtime
        .block_on(alice_tx_finalized.send(create_dummy_message(
            finalized_transaction_message.clone(),
            &bob_node_identity.public_key(),
        )))
        .unwrap();

    assert!(collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 2,
        timeout = Duration::from_secs(10)
    )
    .iter()
    .find(|i| if let TransactionEvent::Error(s) = i {
        s == &"Error handling Transaction Finalized message".to_string()
    } else {
        false
    })
    .is_some());
}

#[test]
fn finalize_tx_with_missing_output_memory_db() {
    finalize_tx_with_missing_output(TransactionMemoryDatabase::new(), TransactionMemoryDatabase::new());
}

#[test]
fn finalize_tx_with_missing_output_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let bob_db_path = format!("{}/{}", path_string, bob_db_name);
        finalize_tx_with_missing_output(
            TransactionServiceSqliteDatabase::new(alice_db_path).unwrap(),
            TransactionServiceSqliteDatabase::new(bob_db_path).unwrap(),
        );
    });
}

#[test]
fn discovery_async_return_test() {
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();

    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_backend = TransactionMemoryDatabase::new();
    let bob_backend = TransactionMemoryDatabase::new();
    let dave_backend = TransactionMemoryDatabase::new();
    let port_offset = 1;

    // Alice's parameters
    let alice_port = 30484 + port_offset;
    let alice_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", alice_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Bob's parameters
    let bob_port = 30475 + port_offset;
    let bob_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", bob_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Carols's parameters
    let carol_port = 30488 + port_offset;
    let carol_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", carol_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Dave's parameters
    let dave_port = 30498 + port_offset;
    let dave_node_identity = NodeIdentity::random(
        &mut OsRng,
        format!("/ip4/127.0.0.1/tcp/{}", dave_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (mut alice_ts, mut alice_oms, _alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        alice_backend,
        db_folder.clone(),
        Duration::from_secs(1),
    );
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let (_bob_ts, _bob_oms, _bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![
            alice_node_identity.clone(),
            carol_node_identity.clone(),
            dave_node_identity.clone(),
        ],
        factories.clone(),
        bob_backend,
        db_folder.clone(),
        Duration::from_secs(1),
    );

    let (_utxo, uo1a) = make_input(&mut OsRng, MicroTari(5500), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1a)).unwrap();
    let (_utxo, uo1b) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1b)).unwrap();
    let (_utxo, uo1c) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1c)).unwrap();

    let initial_balance = runtime.block_on(alice_oms.get_balance()).unwrap();

    let value_a_to_c_1 = MicroTari::from(1400);

    let tx_id = match runtime.block_on(alice_ts.send_transaction(
        carol_node_identity.public_key().clone(),
        value_a_to_c_1,
        MicroTari::from(20),
        "Discovery Tx!".to_string(),
    )) {
        Err(TransactionServiceError::OutboundSendDiscoveryInProgress(tx_id)) => tx_id,
        _ => {
            assert!(false, "Send should not succeed as Peer is not known");
            0u64
        },
    };
    assert_ne!(initial_balance, runtime.block_on(alice_oms.get_balance()).unwrap());

    assert!(collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 1,
        timeout = Duration::from_secs(10)
    )
    .iter()
    .find(
        |i| if let TransactionEvent::TransactionSendDiscoveryComplete(t, result) = i {
            t == &tx_id && !(*result)
        } else {
            false
        }
    )
    .is_some());

    assert_eq!(initial_balance, runtime.block_on(alice_oms.get_balance()).unwrap());

    let (_dave_ts, _dave_oms, _dave_comms) = setup_transaction_service(
        &mut runtime,
        dave_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        dave_backend,
        db_folder,
        Duration::from_secs(1),
    );

    let tx_id2 = match runtime.block_on(alice_ts.send_transaction(
        dave_node_identity.public_key().clone(),
        value_a_to_c_1,
        MicroTari::from(20),
        "Discovery Tx2!".to_string(),
    )) {
        Err(TransactionServiceError::OutboundSendDiscoveryInProgress(tx_id)) => tx_id,
        _ => {
            assert!(false, "Send should not succeed as Peer is not known");
            0u64
        },
    };
    let alice_event_stream = alice_ts.get_event_stream_fused();
    let result = collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 3,
        timeout = Duration::from_secs(10)
    );
    assert!(result
        .iter()
        .find(
            |i| if let TransactionEvent::TransactionSendDiscoveryComplete(t, result) = i {
                t == &tx_id2 && *result
            } else {
                false
            }
        )
        .is_some());
}

fn test_coinbase<T: TransactionBackend + Clone + 'static>(backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (mut alice_ts, mut alice_output_manager, _alice_outbound_service, _alice_tx_sender, _alice_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), backend);

    let balance = runtime.block_on(alice_output_manager.get_balance()).unwrap();
    assert_eq!(balance.pending_incoming_balance, MicroTari(0));

    let coinbase = runtime
        .block_on(alice_ts.request_coinbase_key(MicroTari::from(4000), 7777))
        .unwrap();

    let balance = runtime.block_on(alice_output_manager.get_balance()).unwrap();
    assert_eq!(balance.pending_incoming_balance, MicroTari(4000));

    runtime
        .block_on(alice_ts.cancel_coinbase_transaction(coinbase.tx_id))
        .unwrap();

    let balance = runtime.block_on(alice_output_manager.get_balance()).unwrap();
    assert_eq!(balance.pending_incoming_balance, MicroTari(0));

    let coinbase = runtime
        .block_on(alice_ts.request_coinbase_key(MicroTari::from(7000), 7778))
        .unwrap();

    let output = TransactionOutput::new(
        OutputFeatures::create_coinbase(7778),
        factories.commitment.commit_value(&coinbase.spending_key, 7000),
        RangeProof::default(),
    );
    let kernel = KernelBuilder::new()
        .with_features(KernelFeatures::create_coinbase())
        .with_excess(&factories.commitment.zero())
        .with_signature(&Signature::default())
        .build()
        .unwrap();
    let output_wrong_commitment = TransactionOutput::new(
        OutputFeatures::create_coinbase(1000),
        factories.commitment.commit_value(&coinbase.spending_key, 2222),
        RangeProof::default(),
    );

    let output_wrong_feature = TransactionOutput::new(
        OutputFeatures::default(),
        factories.commitment.commit_value(&coinbase.spending_key, 7000),
        RangeProof::default(),
    );

    let transaction_wrong_commitment = Transaction::new(
        Vec::new(),
        vec![output_wrong_commitment.clone()],
        vec![kernel.clone()],
        PrivateKey::default(),
    );
    let transaction_wrong_feature = Transaction::new(
        Vec::new(),
        vec![output_wrong_feature.clone()],
        vec![kernel.clone()],
        PrivateKey::default(),
    );
    let transaction = Transaction::new(
        Vec::new(),
        vec![output.clone()],
        vec![kernel.clone()],
        PrivateKey::default(),
    );

    assert!(runtime
        .block_on(alice_ts.complete_coinbase_transaction(55, transaction.clone()))
        .is_err());

    assert!(runtime
        .block_on(alice_ts.complete_coinbase_transaction(coinbase.tx_id, transaction_wrong_commitment.clone()))
        .is_err());

    assert!(runtime
        .block_on(alice_ts.complete_coinbase_transaction(coinbase.tx_id, transaction_wrong_feature.clone()))
        .is_err());

    runtime
        .block_on(alice_ts.complete_coinbase_transaction(coinbase.tx_id, transaction))
        .unwrap();

    let completed_txs = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();

    assert_eq!(completed_txs.len(), 1);
    assert!(completed_txs.get(&coinbase.tx_id).is_some());
}

#[test]
fn test_coinbase_memory_db() {
    test_coinbase(TransactionMemoryDatabase::new());
}

#[test]
fn test_coinbase_sqlite_db() {
    with_temp_dir(|dir_path| {
        let path_string = dir_path.to_str().unwrap().to_string();
        let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
        let alice_db_path = format!("{}/{}", path_string, alice_db_name);
        test_coinbase(TransactionServiceSqliteDatabase::new(alice_db_path).unwrap());
    });
}
