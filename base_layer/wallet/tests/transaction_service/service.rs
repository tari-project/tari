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
    utils::{event_stream_count, make_input, random_string, TestParams},
};
use futures::{
    channel::{mpsc, mpsc::Sender},
    SinkExt,
};
use prost::Message;
use rand::OsRng;
use std::{convert::TryInto, sync::Arc, time::Duration};
use tari_broadcast_channel::bounded;
use tari_comms::{
    builder::CommsNode,
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_crypto::keys::{PublicKey as PK, SecretKey as SK};
use tari_p2p::{
    comms_connector::pubsub_connector,
    domain_message::DomainMessage,
    services::comms_outbound::CommsOutboundServiceInitializer,
};
use tari_service_framework::{reply_channel, StackBuilder};
use tari_test_utils::paths::with_temp_dir;
use tari_transactions::{
    tari_amount::*,
    transaction::{KernelFeatures, OutputFeatures, Transaction},
    transaction_protocol::{
        proto,
        recipient::{RecipientSignedMessage, RecipientState},
        sender::TransactionSenderMessage,
    },
    types::{CryptoFactories, PrivateKey, PublicKey},
    ReceiverTransactionProtocol,
};
use tari_wallet::{
    output_manager_service::{
        handle::OutputManagerHandle,
        service::OutputManagerService,
        storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
        OutputManagerConfig,
        OutputManagerServiceInitializer,
    },
    transaction_service::{
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
use tokio::runtime::Runtime;

pub fn setup_transaction_service<T: TransactionBackend + 'static>(
    runtime: &Runtime,
    master_key: PrivateKey,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    factories: CryptoFactories,
    backend: T,
    database_path: String,
) -> (TransactionServiceHandle, OutputManagerHandle, CommsNode)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(
        runtime.executor(),
        Arc::new(node_identity.clone()),
        "127.0.0.1:0".parse().unwrap(),
        peers,
        publisher,
        database_path,
    );

    let fut = StackBuilder::new(runtime.executor(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(OutputManagerServiceInitializer::new(
            OutputManagerConfig {
                master_seed: master_key,
                branch_seed: "".to_string(),
                primary_key_index: 0,
            },
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
pub fn setup_transaction_service_no_comms<T: TransactionBackend + 'static>(
    runtime: &Runtime,
    master_key: PrivateKey,
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
    let output_manager_service = OutputManagerService::new(
        oms_request_receiver,
        OutputManagerConfig {
            master_seed: master_key,
            branch_seed: "".to_string(),
            primary_key_index: 0,
        },
        OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
        factories.clone(),
    )
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
                &mut OsRng::new().unwrap(),
                "0.0.0.0:41239".parse().unwrap(),
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

fn manage_single_transaction<T: TransactionBackend + 'static>(
    alice_backend: T,
    bob_backend: T,
    port_offset: i32,
    database_path: String,
)
{
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_seed = PrivateKey::random(&mut rng);
    let alice_port = 31501 + port_offset;
    let alice_node_identity = NodeIdentity::random(
        &mut rng,
        format!("127.0.0.1:{}", alice_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Bob's parameters
    let bob_seed = PrivateKey::random(&mut rng);
    let bob_port = 32713 + port_offset;
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        format!("127.0.0.1:{}", bob_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &runtime,
        alice_seed,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
    );
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut rng, MicroTari(2500), &factories.commitment);

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
        &runtime,
        bob_seed,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path,
    );

    let mut result =
        runtime.block_on(async { event_stream_count(alice_event_stream, 1, Duration::from_secs(10)).await });
    assert_eq!(result.remove(&TransactionEvent::ReceivedTransactionReply), Some(1));

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

fn manage_multiple_transactions<T: TransactionBackend + 'static>(
    alice_backend: T,
    bob_backend: T,
    carol_backend: T,
    port_offset: i32,
    database_path: String,
)
{
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_seed = PrivateKey::random(&mut rng);
    let alice_port = 31484 + port_offset;
    let alice_node_identity = NodeIdentity::random(
        &mut rng,
        format!("127.0.0.1:{}", alice_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Bob's parameters
    let bob_seed = PrivateKey::random(&mut rng);
    let bob_port = 31475 + port_offset;
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        format!("127.0.0.1:{}", bob_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Carols's parameters
    let carol_seed = PrivateKey::random(&mut rng);
    let carol_port = 31488 + port_offset;
    let carol_node_identity = NodeIdentity::random(
        &mut rng,
        format!("127.0.0.1:{}", carol_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &runtime,
        alice_seed,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
    );
    let alice_event_stream = alice_ts.get_event_stream_fused();

    // Add some funds to Alices wallet
    let (_utxo, uo1a) = make_input(&mut rng, MicroTari(5500), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1a)).unwrap();
    let (_utxo, uo1b) = make_input(&mut rng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1b)).unwrap();
    let (_utxo, uo1c) = make_input(&mut rng, MicroTari(3000), &factories.commitment);
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
        &runtime,
        bob_seed,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path.clone(),
    );
    let (mut carol_ts, mut carol_oms, carol_comms) = setup_transaction_service(
        &runtime,
        carol_seed,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        carol_backend,
        database_path,
    );

    let bob_event_stream = bob_ts.get_event_stream_fused();

    let (_utxo, uo2) = make_input(&mut rng, MicroTari(3500), &factories.commitment);
    runtime.block_on(bob_oms.add_output(uo2)).unwrap();
    let (_utxo, uo3) = make_input(&mut rng, MicroTari(4500), &factories.commitment);
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

    let mut result =
        runtime.block_on(async { event_stream_count(alice_event_stream, 5, Duration::from_secs(10)).await });

    assert_eq!(result.remove(&TransactionEvent::ReceivedTransactionReply), Some(3));

    let _ = runtime.block_on(async { event_stream_count(bob_event_stream, 5, Duration::from_secs(10)).await });
    let alice_pending_outbound = runtime.block_on(alice_ts.get_pending_outbound_transactions()).unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 4);
    let bob_pending_outbound = runtime.block_on(bob_ts.get_pending_outbound_transactions()).unwrap();
    let bob_completed_tx = runtime.block_on(bob_ts.get_completed_transactions()).unwrap();
    assert_eq!(bob_pending_outbound.len(), 0);
    assert_eq!(bob_completed_tx.len(), 3);
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

fn test_sending_repeated_tx_ids<T: TransactionBackend + 'static>(alice_backend: T, bob_backend: T) {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_seed = PrivateKey::random(&mut rng);
    let bob_seed = PrivateKey::random(&mut rng);
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:55741".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let (alice_ts, _alice_output_manager, alice_outbound_service, mut alice_tx_sender, _alice_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&runtime, alice_seed, factories.clone(), alice_backend);
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&runtime, bob_seed, factories.clone(), bob_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let (_utxo, uo) = make_input(&mut rng, MicroTari(250000), &factories.commitment);

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

    let mut result = runtime.block_on(event_stream_count(alice_event_stream, 2, Duration::from_secs(10)));

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result.remove(&TransactionEvent::ReceivedTransaction), Some(1));
    assert_eq!(
        result.remove(&TransactionEvent::Error(
            "Error handling Transaction Sender message".to_string()
        )),
        Some(1)
    );
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

fn test_accepting_unknown_tx_id_and_malformed_reply<T: TransactionBackend + 'static>(alice_backend: T) {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_seed = PrivateKey::random(&mut rng);
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:31585".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let (mut alice_ts, mut alice_output_manager, alice_outbound_service, _alice_tx_sender, mut alice_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&runtime, alice_seed, factories.clone(), alice_backend);

    let alice_event_stream = alice_ts.get_event_stream_fused();

    let (_utxo, uo) = make_input(&mut rng, MicroTari(250000), &factories.commitment);

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
    let envelope_body = EnvelopeBody::decode(&body).unwrap();
    let sender_message = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap();

    let params = TestParams::new(&mut rng);

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
    let (_p, pub_key) = PublicKey::random_keypair(&mut rng);
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

    let mut result =
        runtime.block_on(async { event_stream_count(alice_event_stream, 2, Duration::from_secs(10)).await });
    assert_eq!(
        result.remove(&TransactionEvent::Error(
            "Error handling Transaction Recipient Reply message".to_string()
        )),
        Some(2)
    );
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

fn finalize_tx_with_nonexistent_txid<T: TransactionBackend + 'static>(alice_backend: T) {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_seed = PrivateKey::random(&mut rng);

    let (
        alice_ts,
        _alice_output_manager,
        _alice_outbound_service,
        _alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
    ) = setup_transaction_service_no_comms(&runtime, alice_seed, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let tx = Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut rng));
    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: 88u64,
        transaction: Some(tx.clone().into()),
    };

    runtime
        .block_on(alice_tx_finalized.send(create_dummy_message(
            finalized_transaction_message.clone(),
            &PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
        )))
        .unwrap();

    let mut result = runtime.block_on(event_stream_count(alice_event_stream, 1, Duration::from_secs(10)));

    assert_eq!(
        result.remove(&TransactionEvent::Error(
            "Error handling Transaction Finalized message".to_string()
        )),
        Some(1)
    );
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

fn finalize_tx_with_incorrect_pubkey<T: TransactionBackend + 'static>(alice_backend: T, bob_backend: T) {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_seed = PrivateKey::random(&mut rng);

    let (
        alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
    ) = setup_transaction_service_no_comms(&runtime, alice_seed, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_seed = PrivateKey::random(&mut rng);
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:55741".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&runtime, bob_seed, factories.clone(), bob_backend);

    let (_utxo, uo) = make_input(&mut rng, MicroTari(250000), &factories.commitment);

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
    let envelope_body = EnvelopeBody::decode(&body).unwrap();
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
            &PublicKey::from_secret_key(&PrivateKey::random(&mut rng)),
        )))
        .unwrap();

    let mut result = runtime.block_on(event_stream_count(alice_event_stream, 2, Duration::from_secs(10)));

    assert_eq!(
        result.remove(&TransactionEvent::Error(
            "Error handling Transaction Finalized message".to_string()
        )),
        Some(1)
    );
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

fn finalize_tx_with_missing_output<T: TransactionBackend + 'static>(alice_backend: T, bob_backend: T) {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_seed = PrivateKey::random(&mut rng);

    let (
        alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
    ) = setup_transaction_service_no_comms(&runtime, alice_seed, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_seed = PrivateKey::random(&mut rng);
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:55714".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _) =
        setup_transaction_service_no_comms(&runtime, bob_seed, factories.clone(), bob_backend);

    let (_utxo, uo) = make_input(&mut rng, MicroTari(250000), &factories.commitment);

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
    let envelope_body = EnvelopeBody::decode(&body).unwrap();
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
        transaction: Some(Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut rng)).into()),
    };

    runtime
        .block_on(alice_tx_finalized.send(create_dummy_message(
            finalized_transaction_message.clone(),
            &bob_node_identity.public_key(),
        )))
        .unwrap();

    let mut result = runtime.block_on(event_stream_count(alice_event_stream, 2, Duration::from_secs(10)));

    assert_eq!(
        result.remove(&TransactionEvent::Error(
            "Error handling Transaction Finalized message".to_string()
        )),
        Some(1)
    );
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
