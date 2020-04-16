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
    protocol::messaging::{MessagingEvent, MessagingEventSender},
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
        transaction::{KernelBuilder, KernelFeatures, OutputFeatures, Transaction, TransactionOutput, UnblindedOutput},
        transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::TransactionSenderMessage},
        types::{CryptoFactories, PrivateKey, PublicKey, RangeProof, Signature},
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
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
use tari_test_utils::{collect_stream, paths::with_temp_dir};
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
    types::HashDigest,
};
use tempdir::TempDir;
use tokio::{
    runtime,
    runtime::{Builder, Runtime},
    sync::{broadcast, broadcast::channel},
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
            comms.message_event_sender(),
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
    mined_request_timeout: Option<Duration>,
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
    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(100);

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
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher.clone());
    let (tx_sender, tx_receiver) = mpsc::channel(20);
    let (tx_ack_sender, tx_ack_receiver) = mpsc::channel(20);
    let (tx_finalized_sender, tx_finalized_receiver) = mpsc::channel(20);
    let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(20);
    let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(20);

    let outbound_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());

    let (message_event_publisher, _message_event_subscriber) = broadcast::channel(30);

    let ts_service = TransactionService::new(
        TransactionServiceConfig {
            mempool_broadcast_timeout: Duration::from_secs(5),
            base_node_mined_timeout: mined_request_timeout.unwrap_or(Duration::from_secs(5)),
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
        message_event_publisher.clone(),
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

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

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

    let mut bob_event_stream = bob_ts.get_event_stream_fused();

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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    count+=1;
                    if count>=2 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });

    let mut tx_id = 0u64;
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut finalized = 0;
        loop {
            futures::select! {
                event = bob_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedFinalizedTransaction(id) => {
                            tx_id = *id;
                            finalized+=1;
                         },
                        _ => (),
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

    log::info!(
        "wallet::manage_multiple_transactions: Alice: '{}', Bob: '{}', carol: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        carol_node_identity.node_id().short_str()
    );

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(60),
    );
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

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
    let mut bob_event_stream = bob_ts.get_event_stream_fused();
    let (mut carol_ts, mut carol_oms, carol_comms) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        carol_backend,
        database_path,
        Duration::from_secs(1),
    );
    let mut carol_event_stream = carol_ts.get_event_stream_fused();
    let (_utxo, uo2) = make_input(&mut OsRng, MicroTari(3500), &factories.commitment);
    runtime.block_on(bob_oms.add_output(uo2)).unwrap();
    let (_utxo, uo3) = make_input(&mut OsRng, MicroTari(4500), &factories.commitment);
    runtime.block_on(carol_oms.add_output(uo3)).unwrap();

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
    log::trace!("Sending A to B 1");
    let tx_id_a_to_b_1 = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value_a_to_b_1,
            MicroTari::from(20),
            "a to b 1".to_string(),
        ))
        .unwrap();
    log::trace!("A to B 1 TxID: {}", tx_id_a_to_b_1);
    log::trace!("Sending A to C 1");
    let tx_id_a_to_c_1 = runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "a to c 1".to_string(),
        ))
        .unwrap();
    let alice_completed_tx = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(alice_completed_tx.len(), 0);
    log::trace!("A to C 1 TxID: {}", tx_id_a_to_c_1);

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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut tx_reply = 0;
        let mut finalized = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => tx_reply+=1,
                        TransactionEvent::ReceivedFinalizedTransaction(_) => finalized+=1,
                        _ => (),
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
        assert_eq!(tx_reply, 3, "Need 3 replies");
        assert_eq!(finalized, 1);
    });

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut tx_reply = 0;
        let mut finalized = 0;
        loop {
            futures::select! {
                event = bob_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => tx_reply+=1,
                        TransactionEvent::ReceivedFinalizedTransaction(_) => finalized+=1,
                        _ => (),
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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(90)).fuse();
        let mut finalized = 0;
        loop {
            futures::select! {
                event = carol_event_stream.select_next_some() => {
                     match &*event.unwrap() {
                        TransactionEvent::ReceivedFinalizedTransaction(_) => finalized+=1,
                        _ => (),
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
    assert_eq!(alice_completed_tx.len(), 4, "Not enough transactions for Alice");
    let bob_pending_outbound = runtime.block_on(bob_ts.get_pending_outbound_transactions()).unwrap();
    let bob_completed_tx = runtime.block_on(bob_ts.get_completed_transactions()).unwrap();
    assert_eq!(bob_pending_outbound.len(), 0);
    assert_eq!(bob_completed_tx.len(), 3, "Not enough transactions for Bob");

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

fn test_accepting_unknown_tx_id_and_malformed_reply<T: TransactionBackend + Clone + 'static>(alice_backend: T) {
    let mut runtime = Runtime::new().unwrap();
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
        alice_messaging_event_sender,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

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
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();
    let (_, body, tag) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message
    alice_messaging_event_sender
        .send(Arc::new(MessagingEvent::MessageSent(tag.clone())))
        .unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut errors = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::Error(s) = &*event.unwrap() {
                        if s == &"Error handling Transaction Recipient Reply message".to_string() {
                            errors+=1;
                        }
                        if s == &"TransactionError(ValidationError(\"Transaction could not be finalized\"))".to_string() {
                            errors+=1;
                        }
                        if errors >= 2 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(errors >= 2);
    });
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
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);
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
        .find(|i| if let TransactionEvent::Error(s) = &**(**i).as_ref().unwrap() {
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
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend, None);

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
        .wait_call_count(2, Duration::from_secs(10))
        .unwrap();
    let (_, body, _) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
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
        .find(|i| if let TransactionEvent::Error(s) = &**(**i).as_ref().unwrap() {
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
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);
    let alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (_bob_ts, mut bob_output_manager, _bob_outbound_service, _bob_tx_sender, _bob_tx_ack_sender, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend, None);

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
        .wait_call_count(2, Duration::from_secs(10))
        .unwrap();
    let (_, body, _) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
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
        .find(|i| if let TransactionEvent::Error(s) = &**(**i).as_ref().unwrap() {
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

    let tx_id = runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "Discovery Tx!".to_string(),
        ))
        .unwrap();

    assert_ne!(initial_balance, runtime.block_on(alice_oms.get_balance()).unwrap());

    let mut txid = 0;
    let mut is_success = true;
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionDirectSendResult(tx_id, result) = (*event.unwrap()).clone() {
                        txid = tx_id;
                        is_success = result;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });
    assert_eq!(txid, tx_id);
    assert_eq!(is_success, false);

    let tx_id2 = runtime
        .block_on(alice_ts.send_transaction(
            dave_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "Discovery Tx2!".to_string(),
        ))
        .unwrap();

    let mut success_result = false;
    let mut success_tx_id = 0u64;
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut success_count = 0;

        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionDirectSendResult(tx_id, success) = &*event.unwrap() {
                        success_count+=1;
                        success_result = success.clone();
                        success_tx_id = tx_id.clone();
                        if success_count >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(success_count >= 1);
    });

    assert_eq!(success_tx_id, tx_id2);
    assert!(success_result);

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut tx_reply = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap() {
                        if tx_id == &tx_id2 {
                            tx_reply +=1;
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(tx_reply >= 1);
    });

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
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), backend, None);

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
        mut alice_base_node_response_sender,
        alice_messaging_event_sender,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let (_bob_ts, _bob_output_manager, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let (_utxo, uo2) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo2)).unwrap();

    // Send Tx1
    let tx_id1 = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            10000 * uT,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();
    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Alice call wait 1");
    let (_, body, tag) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // Burn the SAF version of the message
    alice_messaging_event_sender
        .send(Arc::new(MessagingEvent::MessageSent(tag.clone())))
        .unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    match tx_sender_msg.clone() {
        TransactionSenderMessage::Single(_) => (),
        _ => {
            assert!(false, "Transaction is the not a single rounder sender variant");
        },
    };

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();
    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .expect("bob call wait 1");

    let call = bob_outbound_service.pop_call().unwrap();
    let _ = bob_outbound_service.pop_call().unwrap(); // Burn the SAF version of the message
    let envelope_body = EnvelopeBody::decode(&mut call.1.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg1: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    // Send Tx2
    let tx_id2 = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            10001 * uT,
            100 * uT,
            "Testing Message2".to_string(),
        ))
        .unwrap();
    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Alice call wait 2");

    let call = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // Burn the SAF version of the message
    let tx_sender_msg = try_decode_sender_message(call.1.to_vec().clone()).unwrap();

    alice_messaging_event_sender
        .send(Arc::new(MessagingEvent::MessageSent(call.2.clone())))
        .unwrap();

    match tx_sender_msg.clone() {
        TransactionSenderMessage::Single(_) => (),
        _ => {
            assert!(false, "Transaction is the not a single rounder sender variant");
        },
    };

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();
    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Bob call wait 2");

    let (_, body, _) = bob_outbound_service.pop_call().unwrap();
    let _ = bob_outbound_service.pop_call().unwrap(); // Burn the SAF version of the message

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg2: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    // Give Alice both of Bobs replies
    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            bob_tx_reply_msg1.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            bob_tx_reply_msg2.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut tx1_timeout = false;
        let mut tx2_timeout = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::MempoolBroadcastTimedOut(tx_id) = &*event.unwrap(){
                        if tx_id == &tx_id1 {
                            tx1_timeout = true;
                        }
                        if tx_id == &tx_id2 {
                            tx2_timeout = true;
                        }
                        if tx1_timeout && tx2_timeout {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(tx1_timeout && tx2_timeout);
    });

    let alice_completed_tx1 = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id1)
        .expect("Transaction must be in collection");

    let alice_completed_tx2 = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx1.status, TransactionStatus::Completed);
    assert_eq!(alice_completed_tx2.status, TransactionStatus::Completed);

    alice_outbound_service
        .wait_call_count(4, Duration::from_secs(60))
        .expect("Alice call wait 3");

    let mut msr_tx1_found = false;
    let mut bsr_tx1_found = false;
    let mut msr_tx2_found = false;
    let mut bsr_tx2_found = false;
    log::info!("Starting to look for MSR and BSR requests");
    for _ in 0..4 {
        let call = alice_outbound_service.pop_call().unwrap();
        match try_decode_mempool_request(call.1.to_vec().clone()) {
            Some(m) => {
                if m.request_key == tx_id1 {
                    msr_tx1_found = true;
                }
                if m.request_key == tx_id2 {
                    msr_tx2_found = true;
                }
                match m.request {
                    MempoolRequest::GetStats => assert!(false, "Invalid Mempool Service Request variant"),
                    MempoolRequest::GetState => assert!(false, "Invalid Mempool Service Request variant"),
                    MempoolRequest::GetTxStateWithExcessSig(_) => {
                        assert!(false, "Invalid Mempool Service Request variant")
                    },
                    MempoolRequest::SubmitTransaction(t) => {
                        if m.request_key == tx_id1 {
                            assert_eq!(t, alice_completed_tx1.transaction);
                        }
                        if m.request_key == tx_id2 {
                            assert_eq!(t, alice_completed_tx2.transaction);
                        }
                    },
                }
            },
            None => {
                if let Some(bsr) = try_decode_base_node_request(call.1.to_vec().clone()) {
                    if bsr.request_key == tx_id1 {
                        bsr_tx1_found = true;
                    }
                    if bsr.request_key == tx_id2 {
                        bsr_tx2_found = true;
                    }
                }
            },
        }
    }
    assert!(msr_tx1_found);
    assert!(msr_tx2_found);
    assert!(bsr_tx1_found);
    assert!(bsr_tx2_found);

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut broadcast_timeout_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::MempoolBroadcastTimedOut(_) = &*event.unwrap(){
                        broadcast_timeout_count +=1;
                        if broadcast_timeout_count >= 1 {
                            break;
                        }

                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(broadcast_timeout_count >= 1);
    });

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: tx_id1,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::UnconfirmedPool).into()),
    };
    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    let completed_tx_outputs: Vec<TransactionOutputProto> = alice_completed_tx2
        .transaction
        .body
        .outputs()
        .iter()
        .map(|o| TransactionOutputProto::from(o.clone()))
        .collect();

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: tx_id2.clone(),
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: completed_tx_outputs.into(),
            },
        )),
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut broadcast = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::TransactionBroadcast(id) = &*event.unwrap(){
                        broadcast = &tx_id1 == id;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(broadcast);
    });

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut mined = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::TransactionMined(id) = &*event.unwrap(){
                        mined = &tx_id2 == id;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(mined);
    });

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id1)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Broadcast);

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Mined);
}

fn try_decode_mempool_request(bytes: Vec<u8>) -> Option<MempoolServiceRequest> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    let msr = match envelope_body.decode_part::<MempoolProto::MempoolServiceRequest>(1) {
        Err(_) => return None,
        Ok(d) => match d {
            None => return None,
            Some(r) => r,
        },
    };

    match MempoolServiceRequest::try_from(msr) {
        Ok(msr) => Some(msr),
        Err(_) => None,
    }
}

fn try_decode_sender_message(bytes: Vec<u8>) -> Option<TransactionSenderMessage> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    let tx_sender_msg = match envelope_body.decode_part::<proto::TransactionSenderMessage>(1) {
        Err(_) => return None,
        Ok(d) => match d {
            None => return None,
            Some(r) => r,
        },
    };

    match TransactionSenderMessage::try_from(tx_sender_msg) {
        Ok(msr) => Some(msr),
        Err(_) => None,
    }
}

fn try_decode_base_node_request(bytes: Vec<u8>) -> Option<BaseNodeProto::BaseNodeServiceRequest> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    match envelope_body.decode_part::<BaseNodeProto::BaseNodeServiceRequest>(1) {
        Err(_) => return None,
        Ok(d) => match d {
            None => return None,
            Some(r) => return Some(r),
        },
    };
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
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), db, None);

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
                    if let TransactionEvent::MempoolBroadcastTimedOut(tx_id) = (*event.unwrap()).clone() {
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
        alice_messaging_event_sender,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let (_, _, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _bob_messaging_event_sender) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let mut alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let alice_total_available2 = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available2, &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();
    alice_total_available += alice_total_available2;

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();

    let (_, body, tag) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message
    alice_messaging_event_sender
        .send(Arc::new(MessagingEvent::MessageSent(tag.clone())))
        .unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    match tx_sender_msg.clone() {
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

    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();
    let (_, body, _) = bob_outbound_service.pop_call().unwrap();
    let _ = bob_outbound_service.pop_call().unwrap(); // burn SAF message

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg1: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    // Send another transaction
    let amount_sent2 = 20000 * uT;

    let tx_id2 = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent2,
            100 * uT,
            "Testing Message1".to_string(),
        ))
        .unwrap();

    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();
    let (_, body, tag) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message

    alice_messaging_event_sender
        .send(Arc::new(MessagingEvent::MessageSent(tag.clone())))
        .unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();
    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();
    let (_, body, _) = bob_outbound_service.pop_call().unwrap();
    let _ = bob_outbound_service.pop_call().unwrap(); // burn SAF message

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg2: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            bob_tx_reply_msg1.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();
    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            bob_tx_reply_msg2.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut reply_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => {
                            reply_count+=1;
                            if reply_count >= 2 {
                                break;
                            }
                        },
                        _ => (),
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Completed);

    let alice_completed_tx2 = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction2 must be in collection");

    assert_eq!(alice_completed_tx2.status, TransactionStatus::Completed);

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let _ = alice_outbound_service.wait_call_count(6, Duration::from_secs(60));
    for _ in 0..6 {
        let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message
    }

    let broadcast_tx_id = tx_id;
    let completed_tx_id = tx_id2;

    let broadcast_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&broadcast_tx_id)
        .expect("Broadcast Transaction must be in collection");
    let broadcast_tx_outputs: Vec<TransactionOutputProto> = broadcast_tx
        .transaction
        .body
        .outputs()
        .iter()
        .map(|o| TransactionOutputProto::from(o.clone()))
        .collect();

    let completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&completed_tx_id)
        .expect("Completed Transaction must be in collection");
    let completed_tx_outputs: Vec<TransactionOutputProto> = completed_tx
        .transaction
        .body
        .outputs()
        .iter()
        .map(|o| TransactionOutputProto::from(o.clone()))
        .collect();

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: broadcast_tx_id,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::UnconfirmedPool).into()),
    };

    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut mined_request_timeout_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::TransactionMinedRequestTimedOut(_) = &*event.unwrap(){
                        mined_request_timeout_count +=1;
                        if mined_request_timeout_count >= 2 {
                            break;
                        }

                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(mined_request_timeout_count >= 2);
    });

    // Test that receiving a base node response with the wrong outputs does not result in a TX being mined
    let wrong_outputs = vec![completed_tx_outputs[0].clone(), TransactionOutput::default().into()];

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: completed_tx_id.clone(),
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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut mined_request_timeout_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::TransactionMinedRequestTimedOut(_) = &*event.unwrap(){
                        mined_request_timeout_count +=1;
                        if mined_request_timeout_count >= 2 {
                            break;
                        }

                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(mined_request_timeout_count >= 2);
    });

    let broadcast_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&broadcast_tx_id)
        .expect("Broadcast Transaction2 must be in collection");

    let completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&completed_tx_id)
        .expect("Completed Transaction must be in collection");

    assert_eq!(broadcast_tx.status, TransactionStatus::Broadcast);
    assert_eq!(completed_tx.status, TransactionStatus::Completed);

    let mut chain_monitoring_id = 0u64;
    // We need to get the Protocol ID that is not the completed_tx_id so we might need to pop one or pop up to 3
    for _ in 0..4 {
        let call = alice_outbound_service.pop_call().unwrap();
        let envelope_body = EnvelopeBody::decode(&mut call.1.to_vec().as_slice()).unwrap();
        let msr = envelope_body
            .clone()
            .decode_part::<MempoolProto::MempoolServiceRequest>(1)
            .unwrap()
            .unwrap();

        chain_monitoring_id = msr.request_key;
        if chain_monitoring_id != completed_tx_id {
            break;
        }
    }

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: chain_monitoring_id,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: broadcast_tx_outputs.into(),
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
                outputs: completed_tx_outputs.into(),
            },
        )),
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response2,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut acc = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionMined(_) = &*event.unwrap() {
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
        .expect("Completed Transaction3 must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Mined);

    let alice_completed_tx2 = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Completed Transaction4 must be in collection");

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
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), db, None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime
        .block_on(alice_ts.set_base_node_public_key(PublicKey::default()))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut found_tx_mined_1 = false;
        let mut found_tx_mined_2 = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::TransactionMinedRequestTimedOut(tx_id) = &*event.unwrap(){
                        match tx_id {
                            1u64 => found_tx_mined_1 = true,
                            2u64 => found_tx_mined_2 = true,
                            _ => assert!(false, "Should be no other transactions being broadcast!"),
                        }
                        if found_tx_mined_1 && found_tx_mined_2 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(found_tx_mined_1);
        assert!(found_tx_mined_2);
    });
}

#[test]
#[ignore]
fn test_failed_tx_send_timeout() {
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
                    if let TransactionEvent::TransactionDirectSendResult(_, success) = (*event.unwrap()).clone() {
                        returned = true;
                        result = success;
                        break;
                    }
                },
                () = delay => {
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

#[test]
fn transaction_cancellation_when_not_in_mempool() {
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
        alice_messaging_event_sender,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionMemoryDatabase::new(),
        Some(Duration::from_secs(5)),
    );
    let mut alice_event_stream = alice_ts.get_event_stream_fused();
    let (mut bob_ts, _, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionMemoryDatabase::new(),
        Some(Duration::from_secs(20)),
    );
    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let amount_sent = 10000 * uT;

    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();
    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();
    let (_, body, tag) = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message
    alice_messaging_event_sender
        .send(Arc::new(MessagingEvent::MessageSent(tag.clone())))
        .unwrap();
    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
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
    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();
    let (_, body, _) = bob_outbound_service.pop_call().unwrap();
    let _ = bob_outbound_service.pop_call().unwrap(); // burn SAF message

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
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

    let _ = alice_outbound_service.wait_call_count(2, Duration::from_secs(60));
    let _ = alice_outbound_service.pop_call().unwrap(); // Burn finalize message
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => break,
                        _ => (),
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });
    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Completed);

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: tx_id,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::UnconfirmedPool).into()),
    };

    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut timeouts = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionMinedRequestTimedOut(_e) = &*event.unwrap() {
                        timeouts+=1;
                        if timeouts >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(timeouts >= 1);
    });

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx.status, TransactionStatus::Broadcast);

    let _ = alice_outbound_service.wait_call_count(2, Duration::from_secs(60));
    let call = alice_outbound_service.pop_call().unwrap();
    let _ = alice_outbound_service.pop_call().unwrap(); // burn SAF message

    let envelope_body = EnvelopeBody::decode(&mut call.1.to_vec().as_slice()).unwrap();
    let msr = envelope_body
        .decode_part::<MempoolProto::MempoolServiceRequest>(1)
        .unwrap()
        .unwrap();
    let chain_monitoring_id = msr.request_key;

    let mempool_response = MempoolProto::MempoolServiceResponse {
        request_key: chain_monitoring_id,
        response: Some(MempoolResponse::TxStorage(TxStorageResponse::NotStored).into()),
    };

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut timeouts = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionMinedRequestTimedOut(_e) = &*event.unwrap() {
                        timeouts+=1;
                        if timeouts >= 1 {
                            break;
                        }
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(timeouts >= 1);
    });

    let balance = runtime.block_on(alice_output_manager.get_balance()).unwrap();
    assert_eq!(balance.available_balance, MicroTari(0));

    runtime
        .block_on(
            alice_mempool_response_sender.send(create_dummy_message(mempool_response, base_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(20)).fuse();
        let mut cancelled = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionCancelled(_) = &*event.unwrap() {
                        cancelled = true;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Tx should have been cancelled");
    });

    let alice_completed_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&tx_id);
    assert!(alice_completed_tx.is_none(), "Transaction must not be in collection");

    let balance = runtime.block_on(alice_output_manager.get_balance()).unwrap();

    assert_eq!(balance.available_balance, alice_total_available);
}

fn test_transaction_cancellation<T: TransactionBackend + Clone + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (mut alice_ts, mut alice_output_manager, _alice_outbound_service, mut alice_tx_sender, _, _, _, _, _) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), backend, Some(Duration::from_secs(20)));
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionStoreForwardSendResult(_,_) = &*event.unwrap() {
                       break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });

    runtime
        .block_on(alice_ts.get_pending_outbound_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Pending Transaction should be in list");

    runtime.block_on(alice_ts.cancel_transaction(tx_id)).unwrap();

    assert!(runtime
        .block_on(alice_ts.get_pending_outbound_transactions())
        .unwrap()
        .remove(&tx_id)
        .is_none());

    let mut builder = SenderTransactionProtocol::builder(1);
    let amount = MicroTari::from(10_000);
    let input = UnblindedOutput::new(MicroTari::from(100_000), PrivateKey::random(&mut OsRng), None);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input.as_transaction_input(&factories.commitment, OutputFeatures::default()),
            input.clone(),
        )
        .with_change_secret(PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories).unwrap();
    let tx_sender_msg = stp.build_single_round_message().unwrap();
    let tx_id2 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.into());
    runtime
        .block_on(alice_tx_sender.send(create_dummy_message(
            proto_message,
            &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::ReceivedTransaction(_) = &*event.unwrap() {
                       break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });

    runtime
        .block_on(alice_ts.get_pending_inbound_transactions())
        .unwrap()
        .remove(&tx_id2)
        .expect("Pending Transaction should be in list");

    runtime.block_on(alice_ts.cancel_transaction(tx_id2)).unwrap();

    assert!(runtime
        .block_on(alice_ts.get_pending_inbound_transactions())
        .unwrap()
        .remove(&tx_id2)
        .is_none());
}

#[test]
fn test_transaction_cancellation_memory_db() {
    test_transaction_cancellation(TransactionMemoryDatabase::new());
}

#[test]
fn test_transaction_cancellation_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    test_transaction_cancellation(TransactionServiceSqliteDatabase::new(connection));
}
