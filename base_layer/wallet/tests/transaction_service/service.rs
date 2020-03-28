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
    comms_and_services::{create_dummy_message, get_next_memory_address, setup_comms_services},
    utils::{make_input, random_string, TestParams},
};
use chrono::Utc;
use futures::{
    channel::{mpsc, mpsc::Sender},
    stream,
    FutureExt,
    SinkExt,
    StreamExt,
};
use prost::Message;
use rand::rngs::OsRng;
use std::{
    convert::{TryFrom, TryInto},
    sync::Arc,
    time::Duration,
};
use tari_broadcast_channel::bounded;
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::messaging::MessagingEventSender,
    CommsNode,
};
use tari_comms_dht::outbound::mock::{create_outbound_service_mock, OutboundServiceMockState};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::base_node_service_response::Response as BaseNodeResponseProto,
    },
    mempool::{
        proto::mempool as MempoolProto,
        service::{MempoolRequest, MempoolResponse, MempoolServiceRequest},
        TxStorageResponse,
    },
    transactions::{
        proto::types::TransactionOutput as TransactionOutputProto,
        tari_amount::*,
        transaction::{KernelBuilder, KernelFeatures, OutputFeatures, Transaction, TransactionOutput},
        transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::TransactionSenderMessage},
        types::{CryptoFactories, PrivateKey, PublicKey, RangeProof, Signature},
        ReceiverTransactionProtocol,
    },
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
use tari_test_utils::{collect_stream, paths::with_temp_dir, unpack_enum};
use tari_wallet::{
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::OutputManagerHandle,
        service::OutputManagerService,
        storage::{database::OutputManagerDatabase, memory_db::OutputManagerMemoryDatabase},
        OutputManagerServiceInitializer,
    },
    storage::connection_manager::run_migration_and_create_sqlite_connection,
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceHandle},
        service::TransactionService,
        storage::{
            database::{
                CompletedTransaction,
                DbKeyValuePair,
                TransactionBackend,
                TransactionDatabase,
                TransactionStatus,
                WriteOperation,
            },
            memory_db::TransactionMemoryDatabase,
            sqlite_db::TransactionServiceSqliteDatabase,
        },
        TransactionServiceInitializer,
    },
};
use tempdir::TempDir;
use tokio::{
    runtime,
    runtime::{Builder, Runtime},
    sync::broadcast,
    time::delay_for,
};

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
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    factories: CryptoFactories,
    backend: T,
    database_path: String,
    discovery_request_timeout: Duration,
) -> (TransactionServiceHandle, OutputManagerHandle, CommsNode)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.handle().clone(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = runtime.block_on(setup_comms_services(
        node_identity,
        peers,
        publisher,
        database_path,
        discovery_request_timeout,
    ));

    let fut = StackBuilder::new(runtime.handle().clone(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(OutputManagerServiceInitializer::new(
            OutputManagerServiceConfig::default(),
            subscription_factory.clone(),
            OutputManagerMemoryDatabase::new(),
            factories.clone(),
        ))
        .add_initializer(TransactionServiceInitializer::new(
            TransactionServiceConfig {
                mempool_broadcast_timeout: Duration::from_secs(5),
                base_node_mined_timeout: Duration::from_secs(5),
                ..Default::default()
            },
            subscription_factory,
            comms.subscribe_messaging_events(),
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
    Sender<DomainMessage<MempoolProto::MempoolServiceResponse>>,
    Sender<DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
    MessagingEventSender,
)
{
    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();

    let (oms_event_publisher, oms_event_subscriber) = bounded(100);
    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(20);

    let output_manager_service = runtime
        .block_on(OutputManagerService::new(
            OutputManagerServiceConfig::default(),
            outbound_message_requester.clone(),
            oms_request_receiver,
            stream::empty(),
            OutputManagerDatabase::new(OutputManagerMemoryDatabase::new()),
            oms_event_publisher,
            factories.clone(),
        ))
        .unwrap();

    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_subscriber);

    let (ts_request_sender, ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, event_subscriber) = bounded(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_subscriber);
    let (tx_sender, tx_receiver) = mpsc::channel(20);
    let (tx_ack_sender, tx_ack_receiver) = mpsc::channel(20);
    let (tx_finalized_sender, tx_finalized_receiver) = mpsc::channel(20);
    let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(20);
    let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(20);

    let outbound_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());

    let (message_event_publisher, message_event_subscriber) = broadcast::channel(30);

    let ts_service = TransactionService::new(
        TransactionServiceConfig {
            mempool_broadcast_timeout: Duration::from_secs(5),
            base_node_mined_timeout: Duration::from_secs(5),
            ..Default::default()
        },
        TransactionDatabase::new(backend),
        ts_request_receiver,
        tx_receiver,
        tx_ack_receiver,
        tx_finalized_receiver,
        mempool_response_receiver,
        base_node_response_receiver,
        output_manager_service_handle.clone(),
        outbound_message_requester.clone(),
        message_event_subscriber,
        event_publisher,
        Arc::new(
            NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
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
        mempool_response_sender,
        base_node_response_sender,
        message_event_publisher,
    )
}

fn manage_single_transaction<T: TransactionBackend + Clone + 'static>(
    alice_backend: T,
    bob_backend: T,
    database_path: String,
)
{
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Bob's parameters
    let bob_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    let base_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(0),
    );
    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let alice_event_stream = alice_ts.get_event_stream_fused();

    let (mut bob_ts, mut bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path,
        Duration::from_secs(0),
    );
    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let bob_event_stream = bob_ts.get_event_stream_fused();

    runtime
        .block_on(
            bob_comms
                .connection_manager()
                .dial_peer(alice_node_identity.node_id().clone()),
        )
        .unwrap();

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

    let _alice_events =
        runtime.block_on(async { collect_stream!(alice_event_stream, take = 2, timeout = Duration::from_secs(20)) });

    let bob_events =
        runtime.block_on(async { collect_stream!(bob_event_stream, take = 2, timeout = Duration::from_secs(20)) });

    let tx_id = bob_events
        .iter()
        .find_map(|e| {
            if let TransactionEvent::ReceivedFinalizedTransaction(tx_id) = &**e {
                Some(tx_id.clone())
            } else {
                None
            }
        })
        .unwrap();

    let mut bob_completed_tx = runtime.block_on(bob_ts.get_completed_transactions()).unwrap();

    match bob_completed_tx.remove(&tx_id) {
        None => assert!(false, "Completed transaction could not be found"),
        Some(tx) => {
            runtime
                .block_on(bob_oms.confirm_transaction(tx_id, vec![], tx.transaction.body.outputs().clone()))
                .unwrap();
        },
    }

    assert_eq!(
        runtime.block_on(bob_oms.get_balance()).unwrap().available_balance,
        value
    );

    runtime.block_on(async move {
        alice_comms.shutdown().await;
        bob_comms.shutdown().await;
    });
}

#[test]
fn manage_single_transaction_memory_db() {
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    manage_single_transaction(
        TransactionMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        temp_dir.path().to_str().unwrap().to_string(),
    );
}

#[test]
fn manage_single_transaction_sqlite_db() {
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let alice_db_path = format!("{}/{}", temp_dir.path().to_str().unwrap(), alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let bob_db_path = format!("{}/{}", temp_dir.path().to_str().unwrap(), bob_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path).unwrap();

    manage_single_transaction(
        TransactionServiceSqliteDatabase::new(connection_alice),
        TransactionServiceSqliteDatabase::new(connection_bob),
        temp_dir.path().to_str().unwrap().to_string(),
    );
}

fn manage_multiple_transactions<T: TransactionBackend + Clone + 'static>(
    alice_backend: T,
    bob_backend: T,
    carol_backend: T,
    database_path: String,
)
{
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Bob's parameters
    let bob_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Carols's parameters
    let carol_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(1),
    );

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
            "a to b 1".to_string(),
        ))
        .unwrap();
    runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "a to c 1".to_string(),
        ))
        .unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
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

    let (_utxo, uo2) = make_input(&mut OsRng, MicroTari(3500), &factories.commitment);
    runtime.block_on(bob_oms.add_output(uo2)).unwrap();
    let (_utxo, uo3) = make_input(&mut OsRng, MicroTari(4500), &factories.commitment);
    runtime.block_on(carol_oms.add_output(uo3)).unwrap();

    runtime
        .block_on(bob_ts.send_transaction(
            alice_node_identity.public_key().clone(),
            value_b_to_a_1,
            MicroTari::from(20),
            "b to a 1".to_string(),
        ))
        .unwrap();
    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value_a_to_b_2,
            MicroTari::from(20),
            "a to b 2".to_string(),
        ))
        .unwrap();

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut tx_reply = 0;
        let mut finalized = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::ReceivedTransactionReply(_) = &*event{
                        tx_reply+=1;
                    }
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = &*event{
                        finalized+=1;
                    }
                    if tx_reply == 3 && finalized ==1 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(tx_reply, 3);
        assert_eq!(finalized, 1);
    });

    let mut bob_event_stream = bob_ts.get_event_stream_fused();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut tx_reply = 0;
        let mut finalized = 0;
        loop {
            futures::select! {
                event = bob_event_stream.select_next_some() => {
                    if let TransactionEvent::ReceivedTransactionReply(_) = &*event{
                        tx_reply+=1;
                    }
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = &*event{
                        finalized+=1;
                    }
                    if tx_reply == 1 && finalized == 2 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(tx_reply, 1);
        assert_eq!(finalized, 2);
    });

    let mut carol_event_stream = carol_ts.get_event_stream_fused();
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut finalized = 0;
        loop {
            futures::select! {
                event = carol_event_stream.select_next_some() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = &*event{
                        finalized+=1;
                    }
                    if finalized == 1 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(finalized, 1);
    });

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

    runtime.block_on(async move {
        alice_comms.shutdown().await;
        bob_comms.shutdown().await;
        carol_comms.shutdown().await;
    });
}

#[test]
fn manage_multiple_transactions_memory_db() {
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();

    manage_multiple_transactions(
        TransactionMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        temp_dir.path().to_str().unwrap().to_string(),
    );
}

#[test]
fn manage_multiple_transactions_sqlite_db() {
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();

    let path_string = temp_dir.path().to_str().unwrap().to_string();
    let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let bob_db_path = format!("{}/{}", path_string, bob_db_name);
    let carol_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let carol_db_path = format!("{}/{}", path_string, carol_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path).unwrap();
    let connection_carol = run_migration_and_create_sqlite_connection(&carol_db_path).unwrap();
    manage_multiple_transactions(
        TransactionServiceSqliteDatabase::new(connection_alice),
        TransactionServiceSqliteDatabase::new(connection_bob),
        TransactionServiceSqliteDatabase::new(connection_carol),
        path_string,
    );
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

    let (
        alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        _alice_mempool_response_sender,
        _,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _, _, _, _) =
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

    let result =
        runtime.block_on(async { collect_stream!(alice_event_stream, take = 2, timeout = Duration::from_secs(10)) });

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();

    assert_eq!(result.len(), 2);
    assert!(result
        .iter()
        .find(|i| if let TransactionEvent::ReceivedTransaction(_) = &***i {
            true
        } else {
            false
        })
        .is_some());
    assert!(result
        .iter()
        .find(|i| if let TransactionEvent::Error(s) = &***i {
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
        let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
        let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path).unwrap();
        test_sending_repeated_tx_ids(
            TransactionServiceSqliteDatabase::new(connection_alice),
            TransactionServiceSqliteDatabase::new(connection_bob),
        );
    });
}

fn test_accepting_unknown_tx_id_and_malformed_reply<T: TransactionBackend + Clone + 'static>(alice_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        _alice_tx_sender,
        mut alice_tx_ack_sender,
        _,
        _,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);

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

    assert!(runtime
        .block_on(async { collect_stream!(alice_event_stream, take = 2, timeout = Duration::from_secs(10)) })
        .iter()
        .find(|i| if let TransactionEvent::Error(s) = &***i {
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
        let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
        test_accepting_unknown_tx_id_and_malformed_reply(TransactionServiceSqliteDatabase::new(connection_alice));
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
        _,
        _,
        _,
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

    assert!(runtime
        .block_on(async { collect_stream!(alice_event_stream, take = 1, timeout = Duration::from_secs(10)) })
        .iter()
        .find(|i| if let TransactionEvent::Error(s) = &***i {
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
        let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();

        finalize_tx_with_nonexistent_txid(TransactionServiceSqliteDatabase::new(connection_alice));
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
        _,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _, _, _, _) =
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

    assert!(runtime
        .block_on(async { collect_stream!(alice_event_stream, take = 2, timeout = Duration::from_secs(10)) })
        .iter()
        .find(|i| if let TransactionEvent::Error(s) = &***i {
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
        let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
        let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path).unwrap();
        finalize_tx_with_incorrect_pubkey(
            TransactionServiceSqliteDatabase::new(connection_alice),
            TransactionServiceSqliteDatabase::new(connection_bob),
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
        _,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _, _, _, _) =
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

    assert!(runtime
        .block_on(async { collect_stream!(alice_event_stream, take = 2, timeout = Duration::from_secs(10)) })
        .iter()
        .find(|i| if let TransactionEvent::Error(s) = &***i {
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
        let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
        let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path).unwrap();
        finalize_tx_with_missing_output(
            TransactionServiceSqliteDatabase::new(connection_alice),
            TransactionServiceSqliteDatabase::new(connection_bob),
        );
    });
}

#[test]
fn discovery_async_return_test() {
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();

    let mut runtime = runtime::Builder::new()
        .basic_scheduler()
        .enable_time()
        .thread_name("discovery_async_return_test")
        .build()
        .unwrap();
    let factories = CryptoFactories::default();

    let alice_backend = TransactionMemoryDatabase::new();
    let bob_backend = TransactionMemoryDatabase::new();
    let dave_backend = TransactionMemoryDatabase::new();

    // Alice's parameters
    let alice_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Bob's parameters
    let bob_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Carols's parameters
    let carol_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Dave's parameters
    let dave_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    log::info!(
        "discovery_async_return_test: Alice: '{}', Bob: '{}', Carol: '{}', Dave: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        carol_node_identity.node_id().short_str(),
        dave_node_identity.node_id().short_str()
    );

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        alice_backend,
        db_folder.clone(),
        Duration::from_secs(5),
    );
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let (_bob_ts, _bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone(), dave_node_identity.clone()],
        factories.clone(),
        bob_backend,
        db_folder.clone(),
        Duration::from_secs(1),
    );

    let (_dave_ts, _dave_oms, dave_comms) = setup_transaction_service(
        &mut runtime,
        dave_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        dave_backend,
        db_folder,
        Duration::from_secs(1),
    );

    // Establish some connections beforehand, to reduce the amount of work done concurrently in tests
    // Connect Bob and Alice
    runtime
        .block_on(
            bob_comms
                .connection_manager()
                .dial_peer(alice_node_identity.node_id().clone()),
        )
        .unwrap();

    // Connect Dave to Bob
    runtime
        .block_on(
            dave_comms
                .connection_manager()
                .dial_peer(bob_node_identity.node_id().clone()),
        )
        .unwrap();
    log::error!("Finished Dials");

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

    let event = runtime.block_on(alice_event_stream.next()).unwrap();
    unpack_enum!(TransactionEvent::TransactionSendDiscoveryComplete(txid, is_success) = &*event);
    assert_eq!(txid, &tx_id);
    assert_eq!(*is_success, false);

    assert_eq!(initial_balance, runtime.block_on(alice_oms.get_balance()).unwrap());

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

    let event = runtime.block_on(alice_event_stream.next()).unwrap();
    unpack_enum!(TransactionEvent::TransactionSendResult(txid, is_success) = &*event);
    assert_eq!(txid, &tx_id2);
    assert!(is_success);

    let event = runtime.block_on(alice_event_stream.next()).unwrap();
    unpack_enum!(TransactionEvent::ReceivedTransactionReply(txid) = &*event);
    assert_eq!(txid, &tx_id2);

    runtime.block_on(async move {
        alice_comms.shutdown().await;
        bob_comms.shutdown().await;
        dave_comms.shutdown().await;
    });
}

fn test_coinbase<T: TransactionBackend + Clone + 'static>(backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (
        mut alice_ts,
        mut alice_output_manager,
        _alice_outbound_service,
        _alice_tx_sender,
        _alice_tx_ack_sender,
        _,
        _,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), backend);

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
        let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();

        test_coinbase(TransactionServiceSqliteDatabase::new(connection_alice));
    });
}

#[test]
fn transaction_mempool_broadcast() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let base_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        mut _alice_tx_sender,
        mut alice_tx_ack_sender,
        _,
        mut alice_mempool_response_sender,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new());

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let (
        mut bob_ts,
        _bob_output_manager,
        bob_outbound_service,
        mut bob_tx_sender,
        _,
        mut bob_tx_finalized_sender,
        _,
        _,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new());

    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            10000 * uT,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    let call = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let tx_id = match tx_sender_msg.clone() {
        TransactionSenderMessage::Single(s) => s.tx_id,
        _ => {
            assert!(false, "Transaction is the not a single rounder sender variant");
            0
        },
    };

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();

    let _result_stream = runtime.block_on(async {
        collect_stream!(
            bob_ts.get_event_stream_fused(),
            take = 1,
            timeout = Duration::from_secs(20)
        )
    });
    let call = bob_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            tx_reply_msg.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 3,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        2,
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::MempoolBroadcastTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Completed);

    let call = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let msr = envelope_body
        .decode_part::<MempoolProto::MempoolServiceRequest>(1)
        .unwrap()
        .unwrap();

    let mempool_service_request = MempoolServiceRequest::try_from(msr.clone()).unwrap();

    let _ = alice_outbound_service.pop_call().unwrap(); // burn a tx broadcast
    let _ = alice_outbound_service.pop_call().unwrap(); // burn a mempool request
    let _ = alice_outbound_service.pop_call().unwrap(); // burn a tx broadcast
    let _ = alice_outbound_service.pop_call().unwrap(); // burn a mempool request
    let _ = alice_outbound_service.pop_call().unwrap(); // burn a tx broadcast
    let call = alice_outbound_service.pop_call().unwrap(); // this should be the sending of the finalized tx to the receiver

    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_finalized = envelope_body
        .decode_part::<proto::TransactionFinalizedMessage>(1)
        .unwrap()
        .unwrap();

    runtime
        .block_on(bob_tx_finalized_sender.send(create_dummy_message(tx_finalized, alice_node_identity.public_key())))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            bob_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 3,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        1,
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::MempoolBroadcastTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let kernel_sig = alice_completed_tx.transaction.body.kernels()[0].clone().excess_sig;
    assert_eq!(mempool_service_request.request_key, tx_id);

    match mempool_service_request.request {
        MempoolRequest::GetStats => assert!(false, "Invalid Mempool Service Request variant"),
        MempoolRequest::GetTxStateWithExcessSig(excess_sig) => assert_eq!(excess_sig, kernel_sig),
        MempoolRequest::SubmitTransaction(_) => assert!(false, "Invalid Mempool Service Request variant"),
    }

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: tx_id,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::NotStored).into()),
    };

    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 4,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        3,
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::MempoolBroadcastTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: tx_id,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::UnconfirmedPool).into()),
    };

    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 5,
            timeout = Duration::from_secs(60)
        )
    });

    assert_eq!(
        3,
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::MempoolBroadcastTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    assert_eq!(
        1,
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::TransactionBroadcast(_) = item {
                acc + 1
            } else {
                acc
            }
        })
    );

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Broadcast);
}

#[test]
fn broadcast_all_completed_transactions_on_startup() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let db = TransactionMemoryDatabase::new();

    let kernel = KernelBuilder::new()
        .with_excess(&factories.commitment.zero())
        .with_signature(&Signature::default())
        .build()
        .unwrap();

    let tx = Transaction::new(vec![], vec![], vec![kernel], PrivateKey::random(&mut OsRng));

    let completed_tx1 = CompletedTransaction {
        tx_id: 1,
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: 5000 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2,
        status: TransactionStatus::Broadcast,
        ..completed_tx1.clone()
    };

    let completed_tx3 = CompletedTransaction {
        tx_id: 3,
        status: TransactionStatus::Completed,
        ..completed_tx1.clone()
    };

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx1.tx_id.clone(),
        Box::new(completed_tx1.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx2.tx_id.clone(),
        Box::new(completed_tx2.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx3.tx_id.clone(),
        Box::new(completed_tx3.clone()),
    )))
    .unwrap();

    let (mut alice_ts, _, _, _, _, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), db);

    runtime
        .block_on(alice_ts.set_base_node_public_key(PublicKey::default()))
        .unwrap();

    let mut event_stream = alice_ts.get_event_stream_fused();
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut found1 = false;
        let mut found3 = false;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let TransactionEvent::MempoolBroadcastTimedOut(tx_id) = (*event).clone() {
                        if tx_id == 1u64 {
                            found1 = true
                        }
                        if tx_id == 3u64 {
                            found3 = true
                        }
                        if found1 && found3 {
                            break;
                        }

                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(found1);
        assert!(found3);
    });
}

#[test]
fn transaction_base_node_monitoring() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let base_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        mut _alice_tx_sender,
        mut alice_tx_ack_sender,
        _,
        mut alice_mempool_response_sender,
        mut alice_base_node_response_sender,
        _,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new());

    let (mut bob_ts, _, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new());

    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let mut alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let alice_total_available2 = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available2, &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();
    alice_total_available += alice_total_available2;

    let amount_sent = 10000 * uT;

    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    let call = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let tx_id = match tx_sender_msg.clone() {
        TransactionSenderMessage::Single(s) => s.tx_id,
        _ => {
            assert!(false, "Transaction is the not a single rounder sender variant");
            0
        },
    };

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();

    let _result_stream = runtime.block_on(async {
        collect_stream!(
            bob_ts.get_event_stream_fused(),
            take = 1,
            timeout = Duration::from_secs(20)
        )
    });
    let call = bob_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            tx_reply_msg.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    let _result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 1,
            timeout = Duration::from_secs(60)
        )
    });
    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Completed);

    // Send another transaction
    let amount_sent2 = 20000 * uT;

    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent2,
            100 * uT,
            "Testing Message1".to_string(),
        ))
        .unwrap();

    let call = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let tx_id2 = match tx_sender_msg.clone() {
        TransactionSenderMessage::Single(s) => s.tx_id,
        _ => {
            assert!(false, "Transaction is the not a single rounder sender variant");
            0
        },
    };

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();

    let _result_stream = runtime.block_on(async {
        collect_stream!(
            bob_ts.get_event_stream_fused(),
            take = 2,
            timeout = Duration::from_secs(20)
        )
    });
    let call = bob_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            tx_reply_msg.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    let _result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 2,
            timeout = Duration::from_secs(60)
        )
    });
    let alice_completed_tx2 = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx2.status, TransactionStatus::Completed);

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let _ = alice_outbound_service.pop_call().unwrap(); // burn a Tx Mined? request
    let _ = alice_outbound_service.pop_call().unwrap(); // burn a Tx Mined? request

    let call = alice_outbound_service.pop_call().unwrap();
    let envelope_body = EnvelopeBody::decode(&mut call.1.as_slice()).unwrap();
    let msr = envelope_body
        .decode_part::<MempoolProto::MempoolServiceRequest>(1)
        .unwrap()
        .unwrap();

    let mempool_service_request = MempoolServiceRequest::try_from(msr.clone()).unwrap();

    let _ = alice_outbound_service.pop_call().unwrap(); // burn a tx broadcast
    let _ = alice_outbound_service.pop_call().unwrap(); // burn a mempool request

    let broadcast_tx_id = mempool_service_request.request_key;
    let completed_tx_id = if tx_id == broadcast_tx_id { tx_id2 } else { tx_id };

    let broadcast_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&broadcast_tx_id)
        .expect("Transaction must be in collection");
    let kernel_sig = broadcast_tx.transaction.body.kernels()[0].clone().excess_sig;
    let tx_outputs: Vec<TransactionOutputProto> = broadcast_tx
        .transaction
        .body
        .outputs()
        .iter()
        .map(|o| TransactionOutputProto::from(o.clone()))
        .collect();

    match mempool_service_request.request {
        MempoolRequest::GetStats => assert!(false, "Invalid Mempool Service Request variant"),
        MempoolRequest::GetTxStateWithExcessSig(excess_sig) => assert_eq!(excess_sig, kernel_sig),
        MempoolRequest::SubmitTransaction(_) => assert!(false, "Invalid Mempool Service Request variant"),
    }

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: broadcast_tx_id,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::UnconfirmedPool).into()),
    };

    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 6,
            timeout = Duration::from_secs(60)
        )
    });
    assert!(
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::TransactionMinedRequestTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        }) >= 2
    );

    let wrong_outputs = vec![tx_outputs[0].clone(), TransactionOutput::default().into()];

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: tx_id.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: wrong_outputs.into(),
            },
        )),
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 10,
            timeout = Duration::from_secs(60)
        )
    });

    assert!(
        result_stream.iter().fold(0, |acc, item| {
            if let TransactionEvent::TransactionMinedRequestTimedOut(_) = item {
                acc + 1
            } else {
                acc
            }
        }) >= 3
    );

    let broadcast_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&broadcast_tx_id)
        .expect("Transaction must be in collection");

    let completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&completed_tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(broadcast_tx.status, TransactionStatus::Broadcast);
    assert_eq!(completed_tx.status, TransactionStatus::Completed);

    let tx_outputs2: Vec<TransactionOutputProto> = completed_tx
        .transaction
        .body
        .outputs()
        .iter()
        .map(|o| TransactionOutputProto::from(o.clone()))
        .collect();

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: broadcast_tx_id.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: tx_outputs.into(),
            },
        )),
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let base_node_response2 = BaseNodeProto::BaseNodeServiceResponse {
        request_key: completed_tx_id.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: tx_outputs2.into(),
            },
        )),
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response2,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let mut event_stream = alice_ts.get_event_stream_fused();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionMined(_) = &*event {
                        acc += 1;
                        if acc >= 2 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(acc >= 2, "Did not receive enough mined transactions");
    });

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Mined);

    let alice_completed_tx2 = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx2.status, TransactionStatus::Mined);

    let balance = runtime.block_on(alice_output_manager.get_balance()).unwrap();

    assert_eq!(
        balance.available_balance,
        alice_total_available - amount_sent - alice_completed_tx.fee - amount_sent2 - alice_completed_tx2.fee
    );
}

#[test]
fn query_all_completed_transactions_on_startup() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let db = TransactionMemoryDatabase::new();

    let kernel = KernelBuilder::new()
        .with_excess(&factories.commitment.zero())
        .with_signature(&Signature::default())
        .build()
        .unwrap();

    let tx = Transaction::new(vec![], vec![], vec![kernel], PrivateKey::random(&mut OsRng));

    let completed_tx1 = CompletedTransaction {
        tx_id: 1,
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: 5000 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Broadcast,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2,
        status: TransactionStatus::Broadcast,
        ..completed_tx1.clone()
    };

    let completed_tx3 = CompletedTransaction {
        tx_id: 3,
        status: TransactionStatus::Mined,
        ..completed_tx1.clone()
    };

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx1.tx_id.clone(),
        Box::new(completed_tx1.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx2.tx_id.clone(),
        Box::new(completed_tx2.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx3.tx_id.clone(),
        Box::new(completed_tx3.clone()),
    )))
    .unwrap();

    let (mut alice_ts, _, _, _, _, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), db);

    runtime
        .block_on(alice_ts.set_base_node_public_key(PublicKey::default()))
        .unwrap();

    let result_stream = runtime.block_on(async {
        collect_stream!(
            alice_ts.get_event_stream_fused().map(|i| (*i).clone()),
            take = 2,
            timeout = Duration::from_secs(20)
        )
    });

    assert!(result_stream
        .iter()
        .find(|v| match v {
            TransactionEvent::TransactionMinedRequestTimedOut(tx_id) => *tx_id == 1u64,
            _ => false,
        })
        .is_some());
    assert!(result_stream
        .iter()
        .find(|v| match v {
            TransactionEvent::TransactionMinedRequestTimedOut(tx_id) => *tx_id == 2u64,
            _ => false,
        })
        .is_some());
}

#[test]
#[ignore]
fn test_failed_tx_send_timeout() {
    let _ = env_logger::try_init();
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Bob's parameters
    let bob_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    let base_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let (mut alice_ts, mut alice_oms, _alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        TransactionMemoryDatabase::new(),
        temp_dir.path().to_str().unwrap().to_string().clone(),
        Duration::from_secs(0),
    );
    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let (mut bob_ts, _bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        TransactionMemoryDatabase::new(),
        temp_dir.path().to_str().unwrap().to_string(),
        Duration::from_secs(0),
    );
    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    runtime
        .block_on(
            bob_comms
                .connection_manager()
                .dial_peer(alice_node_identity.node_id().clone()),
        )
        .unwrap();

    runtime.block_on(bob_comms.shutdown());
    runtime.block_on(async { delay_for(Duration::from_secs(10)).await });

    let balance = 2500 * uT;
    let value_sent = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, balance, &factories.commitment);

    runtime.block_on(alice_oms.add_output(uo1)).unwrap();
    let message = "TAKE MAH MONEYS!".to_string();
    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value_sent,
            MicroTari::from(20),
            message.clone(),
        ))
        .unwrap();

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(180)).fuse();
        let mut returned = false;
        let mut result = true;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionSendResult(_, success) = (*event).clone() {
                        returned = true;
                        result = success;
                        break;
                    }
                },
                () = delay => {
                log::error!("This select loop timed out");
                    break;
                },
            }
        }
        assert!(returned, "Did not receive event");
        assert!(!result, "Send should have failed");
    });

    let current_balance = runtime.block_on(alice_oms.get_balance()).unwrap();
    assert_eq!(current_balance.available_balance, balance);
}
