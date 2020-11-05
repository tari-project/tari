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
    utils::{random_string, TestParams},
};
use chrono::{Duration as ChronoDuration, Utc};
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
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
    CommsNode,
};
use tari_comms_dht::outbound::mock::{
    create_outbound_service_mock,
    MockBehaviour,
    OutboundServiceMockState,
    ResponseType,
};
use tari_core::{
    base_node::proto::{
        base_node as BaseNodeProto,
        base_node::{
            base_node_service_request::Request as BaseNodeRequestProto,
            base_node_service_response::Response as BaseNodeResponseProto,
        },
    },
    consensus::{ConsensusConstantsBuilder, Network},
    mempool::{
        proto::mempool as MempoolProto,
        service::{MempoolRequest, MempoolResponse, MempoolServiceRequest},
        TxStorageResponse,
    },
    transactions::{
        fee::Fee,
        helpers::make_input,
        proto::types::TransactionOutput as TransactionOutputProto,
        tari_amount::*,
        transaction::{KernelBuilder, KernelFeatures, Transaction},
        transaction_protocol::{proto, recipient::RecipientSignedMessage, sender::TransactionSenderMessage},
        types::{CryptoFactories, PrivateKey, PublicKey, Signature},
        OutputBuilder,
        OutputFeatures,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
        TransactionOutput,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    common::Blake256,
    keys::{PublicKey as PK, SecretKey as SK},
};
use tari_p2p::{comms_connector::pubsub_connector, domain_message::DomainMessage};
use tari_service_framework::{reply_channel, RegisterHandle, StackBuilder};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::paths::with_temp_dir;
use tari_wallet::{
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::OutputManagerHandle,
        service::OutputManagerService,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            memory_db::OutputManagerMemoryDatabase,
        },
        OutputManagerServiceInitializer,
    },
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::{TransactionEvent, TransactionServiceHandle},
        service::TransactionService,
        storage::{
            database::{DbKeyValuePair, TransactionBackend, TransactionDatabase, WriteOperation},
            memory_db::TransactionMemoryDatabase,
            models::{
                CompletedTransaction,
                InboundTransaction,
                OutboundTransaction,
                TransactionDirection,
                TransactionStatus,
            },
            sqlite_db::TransactionServiceSqliteDatabase,
        },
        TransactionServiceInitializer,
    },
    types::HashDigest,
};
use tempfile::tempdir;
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

pub fn setup_transaction_service<T: TransactionBackend + 'static, P: AsRef<Path>>(
    runtime: &mut Runtime,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    factories: CryptoFactories,
    backend: T,
    database_path: P,
    discovery_request_timeout: Duration,
    shutdown_signal: ShutdownSignal,
) -> (TransactionServiceHandle, OutputManagerHandle, CommsNode)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.handle().clone(), 100, 20);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = runtime.block_on(setup_comms_services(
        node_identity,
        peers,
        publisher,
        database_path.as_ref().to_str().unwrap().to_owned(),
        discovery_request_timeout,
        shutdown_signal.clone(),
    ));

    let fut = StackBuilder::new(shutdown_signal)
        .add_initializer(RegisterHandle::new(dht))
        .add_initializer(OutputManagerServiceInitializer::new(
            OutputManagerServiceConfig::default(),
            subscription_factory.clone(),
            OutputManagerMemoryDatabase::new(),
            factories.clone(),
            Network::Rincewind,
        ))
        .add_initializer(TransactionServiceInitializer::new(
            TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(5),
                chain_monitoring_timeout: Duration::from_secs(5),
                low_power_polling_timeout: Duration::from_secs(20),
                ..Default::default()
            },
            subscription_factory.clone(),
            backend,
            comms.node_identity().clone(),
            factories.clone(),
            Network::Rincewind,
        ))
        .build();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let output_manager_handle = handles.expect_handle::<OutputManagerHandle>();
    let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();

    (transaction_service_handle, output_manager_handle, comms)
}

/// This utility function creates a Transaction service without using the Service Framework Stack and exposes all the
/// streams for testing purposes.
pub fn setup_transaction_service_no_comms<T: TransactionBackend + 'static>(
    runtime: &mut Runtime,
    factories: CryptoFactories,
    tx_backend: T,
    config: Option<TransactionServiceConfig>,
) -> (
    TransactionServiceHandle,
    OutputManagerHandle,
    OutboundServiceMockState,
    Sender<DomainMessage<proto::TransactionSenderMessage>>,
    Sender<DomainMessage<proto::RecipientSignedMessage>>,
    Sender<DomainMessage<proto::TransactionFinalizedMessage>>,
    Sender<DomainMessage<MempoolProto::MempoolServiceResponse>>,
    Sender<DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
    Sender<DomainMessage<proto::TransactionCancelledMessage>>,
    Shutdown,
)
{
    setup_transaction_service_no_comms_and_oms_backend(
        runtime,
        factories,
        tx_backend,
        OutputManagerMemoryDatabase::new(),
        config,
    )
}

pub fn setup_transaction_service_no_comms_and_oms_backend<
    T: TransactionBackend + 'static,
    S: OutputManagerBackend + 'static,
>(
    runtime: &mut Runtime,
    factories: CryptoFactories,
    tx_backend: T,
    oms_backend: S,
    config: Option<TransactionServiceConfig>,
) -> (
    TransactionServiceHandle,
    OutputManagerHandle,
    OutboundServiceMockState,
    Sender<DomainMessage<proto::TransactionSenderMessage>>,
    Sender<DomainMessage<proto::RecipientSignedMessage>>,
    Sender<DomainMessage<proto::TransactionFinalizedMessage>>,
    Sender<DomainMessage<MempoolProto::MempoolServiceResponse>>,
    Sender<DomainMessage<BaseNodeProto::BaseNodeServiceResponse>>,
    Sender<DomainMessage<proto::TransactionCancelledMessage>>,
    Shutdown,
)
{
    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();

    let (oms_event_publisher, _) = broadcast::channel(200);
    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(100);

    let (ts_request_sender, ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let ts_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher.clone());
    let (tx_sender, tx_receiver) = mpsc::channel(20);
    let (tx_ack_sender, tx_ack_receiver) = mpsc::channel(20);
    let (tx_finalized_sender, tx_finalized_receiver) = mpsc::channel(20);
    let (mempool_response_sender, mempool_response_receiver) = mpsc::channel(20);
    let (base_node_response_sender, base_node_response_receiver) = mpsc::channel(20);
    let (tx_cancelled_sender, tx_cancelled_receiver) = mpsc::channel(20);

    let outbound_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());
    let constants = ConsensusConstantsBuilder::new(Network::Rincewind).build();

    let shutdown = Shutdown::new();

    let output_manager_service = runtime
        .block_on(OutputManagerService::new(
            OutputManagerServiceConfig::default(),
            outbound_message_requester.clone(),
            ts_handle.clone(),
            oms_request_receiver,
            stream::empty(),
            OutputManagerDatabase::new(oms_backend),
            oms_event_publisher.clone(),
            factories.clone(),
            constants.coinbase_lock_height(),
            shutdown.to_signal(),
        ))
        .unwrap();

    let output_manager_service_handle = OutputManagerHandle::new(oms_request_sender, oms_event_publisher);
    let constants = ConsensusConstantsBuilder::new(Network::Rincewind).build();

    let test_config = config.unwrap_or(TransactionServiceConfig {
        broadcast_monitoring_timeout: Duration::from_secs(5),
        chain_monitoring_timeout: Duration::from_secs(5),
        direct_send_timeout: Duration::from_secs(5),
        broadcast_send_timeout: Duration::from_secs(5),
        low_power_polling_timeout: Duration::from_secs(15),
        transaction_resend_period: Duration::from_secs(200),
        resend_response_cooldown: Duration::from_secs(200),
        pending_transaction_cancellation_timeout: Duration::from_secs(300),
    });

    let ts_service = TransactionService::new(
        test_config,
        TransactionDatabase::new(tx_backend),
        ts_request_receiver,
        tx_receiver,
        tx_ack_receiver,
        tx_finalized_receiver,
        mempool_response_receiver,
        base_node_response_receiver,
        tx_cancelled_receiver,
        output_manager_service_handle.clone(),
        outbound_message_requester.clone(),
        event_publisher,
        Arc::new(
            NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
        ),
        factories.clone(),
        constants,
        shutdown.to_signal(),
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
        tx_cancelled_sender,
        shutdown,
    )
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

// These are helpers functions to attempt to decode the various types of comms messages when using the Mock outbound
// service
fn try_decode_transaction_reply_message(bytes: Vec<u8>) -> Option<RecipientSignedMessage> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    let tx_reply_msg = match envelope_body.decode_part::<proto::RecipientSignedMessage>(1) {
        Err(_) => return None,
        Ok(d) => match d {
            None => return None,
            Some(r) => r,
        },
    };

    match RecipientSignedMessage::try_from(tx_reply_msg) {
        Ok(msr) => Some(msr),
        Err(_) => None,
    }
}

fn try_decode_finalized_transaction_message(bytes: Vec<u8>) -> Option<proto::TransactionFinalizedMessage> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    match envelope_body.decode_part::<proto::TransactionFinalizedMessage>(1) {
        Err(_) => None,
        Ok(d) => d,
    }
}

fn try_decode_transaction_cancelled_message(bytes: Vec<u8>) -> Option<proto::TransactionCancelledMessage> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    match envelope_body.decode_part::<proto::TransactionCancelledMessage>(1) {
        Err(_) => None,
        Ok(d) => d,
    }
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

fn manage_single_transaction<T: TransactionBackend + 'static>(alice_backend: T, bob_backend: T, database_path: String) {
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

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(0),
        shutdown.to_signal(),
    );
    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime.block_on(async { delay_for(Duration::from_secs(2)).await });

    let (mut bob_ts, mut bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );
    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let mut bob_event_stream = bob_ts.get_event_stream_fused();

    let _ = runtime.block_on(
        bob_comms
            .connection_manager()
            .dial_peer(alice_node_identity.node_id().clone()),
    );

    let value = MicroTari::from(1000);
    let uo1 = OutputBuilder::new()
        .with_value(MicroTari(2500))
        .build(&factories.commitment)
        .unwrap();

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
        .expect("Alice sending tx");

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

    assert!(runtime.block_on(bob_ts.get_completed_transaction(999)).is_err());

    let bob_completed_tx = runtime
        .block_on(bob_ts.get_completed_transaction(tx_id))
        .expect("Could not find tx");

    runtime
        .block_on(bob_oms.confirm_transaction(tx_id, vec![], bob_completed_tx.transaction.body.outputs().clone()))
        .unwrap();

    assert_eq!(
        runtime.block_on(bob_oms.get_balance()).unwrap().available_balance,
        value
    );
}

#[test]
fn manage_single_transaction_memory_db() {
    let temp_dir = tempdir().unwrap();
    manage_single_transaction(
        TransactionMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        temp_dir.path().to_str().unwrap().to_string(),
    );
}

#[test]
fn manage_single_transaction_sqlite_db() {
    let temp_dir = tempdir().unwrap();
    let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let alice_db_path = format!("{}/{}", temp_dir.path().to_str().unwrap(), alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let bob_db_path = format!("{}/{}", temp_dir.path().to_str().unwrap(), bob_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path).unwrap();

    manage_single_transaction(
        TransactionServiceSqliteDatabase::new(connection_alice, None),
        TransactionServiceSqliteDatabase::new(connection_bob, None),
        temp_dir.path().to_str().unwrap().to_string(),
    );
}

fn manage_multiple_transactions<T: TransactionBackend + 'static>(
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

    let mut shutdown = Shutdown::new();

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        alice_backend,
        database_path.clone(),
        Duration::from_secs(60),
        shutdown.to_signal(),
    );
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });

    // Spin up Bob and Carol
    let (mut bob_ts, mut bob_oms, bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_backend,
        database_path.clone(),
        Duration::from_secs(1),
        shutdown.to_signal(),
    );
    let mut bob_event_stream = bob_ts.get_event_stream_fused();
    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });

    let (mut carol_ts, mut carol_oms, carol_comms) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        carol_backend,
        database_path,
        Duration::from_secs(1),
        shutdown.to_signal(),
    );
    let mut carol_event_stream = carol_ts.get_event_stream_fused();

    // Establish some connections beforehand, to reduce the amount of work done concurrently in tests
    // Connect Bob and Alice
    runtime.block_on(async { delay_for(Duration::from_secs(3)).await });

    let _ = runtime.block_on(
        bob_comms
            .connection_manager()
            .dial_peer(alice_node_identity.node_id().clone()),
    );
    runtime.block_on(async { delay_for(Duration::from_secs(3)).await });

    // Connect alice to carol
    let _ = runtime.block_on(
        alice_comms
            .connection_manager()
            .dial_peer(carol_node_identity.node_id().clone()),
    );

    let uo2 = OutputBuilder::new()
        .with_value(MicroTari(3500))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(bob_oms.add_output(uo2)).unwrap();
    let uo3 = OutputBuilder::new()
        .with_value(MicroTari(4500))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(carol_oms.add_output(uo3)).unwrap();

    // Add some funds to Alices wallet
    let uo1a = OutputBuilder::new()
        .with_value(MicroTari(5500))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_oms.add_output(uo1a)).unwrap();
    let uo1b = OutputBuilder::new()
        .with_value(MicroTari(3000))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_oms.add_output(uo1b)).unwrap();
    let uo1c = OutputBuilder::new()
        .with_value(MicroTari(3000))
        .build(&factories.commitment)
        .unwrap();
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

    log::trace!("Alice received all Tx messages");

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

    shutdown.trigger().unwrap();
    runtime.block_on(async move {
        alice_comms.wait_until_shutdown().await;
        bob_comms.wait_until_shutdown().await;
        carol_comms.wait_until_shutdown().await;
    });
}

#[test]
fn manage_multiple_transactions_memory_db() {
    let temp_dir = tempdir().unwrap();

    manage_multiple_transactions(
        TransactionMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        temp_dir.path().to_str().unwrap().to_string(),
    );
}

#[test]
fn manage_multiple_transactions_sqlite_db() {
    let temp_dir = tempdir().unwrap();

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
        TransactionServiceSqliteDatabase::new(connection_alice, None),
        TransactionServiceSqliteDatabase::new(connection_bob, None),
        TransactionServiceSqliteDatabase::new(connection_carol, None),
        path_string,
    );
}

fn test_accepting_unknown_tx_id_and_malformed_reply<T: TransactionBackend + 'static>(alice_backend: T) {
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
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let uo = OutputBuilder::new()
        .with_value(MicroTari(250000))
        .build(&factories.commitment)
        .unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = alice_outbound_service.pop_call().unwrap();

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
    tx_reply.public_blinding_factor = pub_key;
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
                        if s == &"TransactionProtocolError(TransactionBuildError(InvalidSignatureError))".to_string() {
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
        assert!(errors >= 1);
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
        test_accepting_unknown_tx_id_and_malformed_reply(TransactionServiceSqliteDatabase::new(connection_alice, None));
    });
}

fn finalize_tx_with_incorrect_pubkey<T: TransactionBackend + 'static>(alice_backend: T, bob_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (
        mut alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (
        _bob_ts,
        mut bob_output_manager,
        _bob_outbound_service,
        _bob_tx_sender,
        _bob_tx_ack_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend, None);

    let uo = OutputBuilder::new()
        .with_value(MicroTari(250000))
        .build(&factories.commitment)
        .unwrap();

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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(15)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = (*event.unwrap()).clone() {
                        assert!(false, "Should not have received finalized event!");
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });

    assert!(runtime
        .block_on(alice_ts.get_completed_transaction(recipient_reply.tx_id))
        .is_err());
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
            TransactionServiceSqliteDatabase::new(connection_alice, None),
            TransactionServiceSqliteDatabase::new(connection_bob, None),
        );
    });
}

fn finalize_tx_with_missing_output<T: TransactionBackend + 'static>(alice_backend: T, bob_backend: T) {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let (
        mut alice_ts,
        _alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _alice_tx_ack_sender,
        mut alice_tx_finalized,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let (
        _bob_ts,
        mut bob_output_manager,
        _bob_outbound_service,
        _bob_tx_sender,
        _bob_tx_ack_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend, None);

    let uo = OutputBuilder::new()
        .with_value(MicroTari(250000))
        .build(&factories.commitment)
        .unwrap();

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

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(15)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = (*event.unwrap()).clone() {
                        assert!(false, "Should not have received finalized event");
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
    });

    assert!(runtime
        .block_on(alice_ts.get_completed_transaction(recipient_reply.tx_id))
        .is_err());
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
            TransactionServiceSqliteDatabase::new(connection_alice, None),
            TransactionServiceSqliteDatabase::new(connection_bob, None),
        );
    });
}

#[test]
fn discovery_async_return_test() {
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path();

    let mut runtime = runtime::Builder::new()
        .basic_scheduler()
        .enable_time()
        .thread_name("discovery_async_return_test")
        .build()
        .unwrap();
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
        "discovery_async_return_test: Alice: '{}', Bob: '{}', Carol: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        carol_node_identity.node_id().short_str(),
    );
    let mut shutdown = Shutdown::new();

    let (_carol_ts, _carol_oms, carol_comms) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![],
        factories.clone(),
        TransactionMemoryDatabase::new(),
        db_folder.join("carol"),
        Duration::from_secs(1),
        shutdown.to_signal(),
    );

    let (mut alice_ts, mut alice_oms, alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![carol_node_identity.clone()],
        factories.clone(),
        TransactionMemoryDatabase::new(),
        db_folder.join("alice"),
        Duration::from_secs(20),
        shutdown.to_signal(),
    );
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let uo1a = OutputBuilder::new()
        .with_value(MicroTari(5500))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_oms.add_output(uo1a)).unwrap();
    let uo1b = OutputBuilder::new()
        .with_value(MicroTari(3000))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_oms.add_output(uo1b)).unwrap();
    let uo1c = OutputBuilder::new()
        .with_value(MicroTari(3000))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_oms.add_output(uo1c)).unwrap();

    let initial_balance = runtime.block_on(alice_oms.get_balance()).unwrap();

    let value_a_to_c_1 = MicroTari::from(1400);

    let tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
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
                    panic!("Timeout while waiting for transaction to fail sending");
                },
            }
        }
    });
    assert_eq!(txid, tx_id);
    assert_eq!(is_success, false);

    let tx_id2 = runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "Discovery Tx2!".to_string(),
        ))
        .unwrap();

    let mut success_result = false;
    let mut success_tx_id = 0u64;
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();

        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionDirectSendResult(tx_id, success) = &*event.unwrap() {
                        success_result = *success;
                        success_tx_id = *tx_id;
                        break;
                    }
                },
                () = delay => {
                    panic!("Timeout while waiting for transaction to successfully be sent");
                },
            }
        }
    });

    assert_eq!(success_tx_id, tx_id2);
    assert!(success_result);

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap() {
                        if tx_id == &tx_id2 {
                            break;
                        }
                    }
                },
                () = delay => {
                    panic!("Timeout while Alice was waiting for a transaction reply");
                },
            }
        }
    });

    shutdown.trigger().unwrap();
    runtime.block_on(async move {
        alice_comms.wait_until_shutdown().await;
        carol_comms.wait_until_shutdown().await;
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
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let (_bob_ts, _bob_output_manager, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let uo = OutputBuilder::new()
        .with_value(MicroTari(250000))
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let uo2 = OutputBuilder::new()
        .with_value(MicroTari(250000))
        .build(&factories.commitment)
        .unwrap();
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
        .wait_call_count(1, Duration::from_secs(60))
        .expect("Alice call wait 1");
    let (_, body) = alice_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .expect("bob call wait 1");

    let call = bob_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .expect("Alice call wait 2");

    let call = alice_outbound_service.pop_call().unwrap();
    let tx_sender_msg = try_decode_sender_message(call.1.to_vec().clone()).unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .expect("Bob call wait 2");

    let (_, body) = bob_outbound_service.pop_call().unwrap();

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
                    MempoolRequest::GetTxStateByExcessSig(_) => {
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
                        if broadcast_timeout_count >= 2 {
                            break;
                        }

                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(broadcast_timeout_count >= 2);
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
        is_synced: false,
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

#[test]
fn test_power_mode_updates() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let backend = TransactionMemoryDatabase::new();
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
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2,
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: 6000 * uT,
        fee: MicroTari::from(200),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
    };

    backend
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            1,
            Box::new(completed_tx1),
        )))
        .unwrap();
    backend
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            2,
            Box::new(completed_tx2),
        )))
        .unwrap();

    let base_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (mut alice_ts, _, alice_outbound_service, _, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), backend, None);

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());
    // Wait for first 4 messages
    alice_outbound_service
        .wait_call_count(4, Duration::from_secs(30))
        .expect("Alice call wait 1");

    runtime.block_on(alice_ts.set_low_power_mode()).unwrap();
    // expect 4 messages more
    alice_outbound_service
        .wait_call_count(8, Duration::from_secs(30))
        .expect("Alice call wait 2");

    runtime.block_on(alice_ts.set_normal_power_mode()).unwrap();
    // and 4 more
    alice_outbound_service
        .wait_call_count(12, Duration::from_secs(30))
        .expect("Alice call wait 3");
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
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
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
        completed_tx1.tx_id,
        Box::new(completed_tx1.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx2.tx_id,
        Box::new(completed_tx2.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx3.tx_id,
        Box::new(completed_tx3.clone()),
    )))
    .unwrap();

    let (mut alice_ts, _, _, _, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), db, None);

    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_err());

    runtime
        .block_on(alice_ts.set_base_node_public_key(PublicKey::default()))
        .unwrap();

    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());

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
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let (_, _, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    runtime.block_on(alice_ts.set_low_power_mode()).unwrap();
    runtime.block_on(alice_ts.set_normal_power_mode()).unwrap();

    let mut alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let alice_total_available2 = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available2)
        .build(&factories.commitment)
        .unwrap();
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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    let (_, body) = alice_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = bob_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = alice_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = bob_outbound_service.pop_call().unwrap();

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

    let _ = alice_outbound_service.wait_call_count(2, Duration::from_secs(60));
    for _ in 0..2 {
        let _ = alice_outbound_service.pop_call().unwrap(); // burn Finalize Messages
    }

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());

    // Wait for 2 pairs of BN and Mempool requests from the two transactions and burn them
    let _ = alice_outbound_service.wait_call_count(4, Duration::from_secs(60));
    for _ in 0..4 {
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
        let mut timeout_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                   match &*event.unwrap() {
                       TransactionEvent::TransactionMinedRequestTimedOut(_) => timeout_count +=1,
                       TransactionEvent::MempoolBroadcastTimedOut(_) => timeout_count +=1,
                       _ => (),
                   }
                    if timeout_count >= 2 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(timeout_count >= 2);
    });

    // Test that receiving a base node response with the wrong outputs does not result in a TX being mined
    let wrong_outputs = vec![completed_tx_outputs[0].clone(), TransactionOutput::default().into()];

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: completed_tx_id,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: wrong_outputs.into(),
            },
        )),
        is_synced: true,
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut timeout_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     match &*event.unwrap() {
                       TransactionEvent::TransactionMinedRequestTimedOut(_) => timeout_count +=1,
                       TransactionEvent::MempoolBroadcastTimedOut(_) => timeout_count +=1,
                       _ => (),
                   }
                    if timeout_count >= 2 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(timeout_count >= 2);
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
        is_synced: true,
    };

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let base_node_response2 = BaseNodeProto::BaseNodeServiceResponse {
        request_key: completed_tx_id,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: completed_tx_outputs.into(),
            },
        )),
        is_synced: true,
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
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
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
        completed_tx1.tx_id,
        Box::new(completed_tx1.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx2.tx_id,
        Box::new(completed_tx2.clone()),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx3.tx_id,
        Box::new(completed_tx3.clone()),
    )))
    .unwrap();

    let (mut alice_ts, _, _, _, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), db, None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime
        .block_on(alice_ts.set_base_node_public_key(PublicKey::default()))
        .unwrap();

    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());

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
        mut alice_base_node_response_sender,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let mut alice_event_stream = alice_ts.get_event_stream_fused();
    let (mut bob_ts, _, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(
            &mut runtime,
            factories.clone(),
            TransactionMemoryDatabase::new(),
            Some(TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(20),
                chain_monitoring_timeout: Duration::from_secs(20),
                ..Default::default()
            }),
        );

    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = alice_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = bob_outbound_service.pop_call().unwrap();

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

    let _ = alice_outbound_service.wait_call_count(1, Duration::from_secs(60));
    let _ = alice_outbound_service.pop_call().unwrap(); // Burn finalize messageSAF message
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

    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());

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

    let base_node_response = BaseNodeProto::BaseNodeServiceResponse {
        request_key: chain_monitoring_id,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![] },
        )),
        is_synced: false,
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

    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut cancelled = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionCancelled(_) = &*event.unwrap() {
                        cancelled = true;
                        break;
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

fn test_transaction_cancellation<T: TransactionBackend + 'static>(backend: T) {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        mut alice_tx_sender,
        _,
        _,
        _,
        _,
        mut alice_tx_cancelled_sender,
        _shutdown,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        backend,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
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

    for i in 0..=12 {
        match runtime
            .block_on(alice_ts.get_pending_outbound_transactions())
            .unwrap()
            .remove(&tx_id)
        {
            None => (),
            Some(_) => break,
        }
        runtime.block_on(async { delay_for(Duration::from_secs(5)).await });
        if i >= 12 {
            assert!(false, "Pending outbound transaction should have been added by now");
        }
    }

    let _ = alice_outbound_service.take_calls();

    runtime.block_on(alice_ts.cancel_transaction(tx_id)).unwrap();

    // Wait for cancellation event, in an effort to nail down where the issue is for the flakey CI test
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut cancelled = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionCancelled(_) = &*event.unwrap() {
                       cancelled = true;
                       break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Cancelled event should have occurred");
    });
    // We expect 1 sent direct and via SAF
    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .expect("alice call wait 1");

    let call = alice_outbound_service.pop_call().unwrap();
    let alice_cancel_message = try_decode_transaction_cancelled_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(alice_cancel_message.tx_id, tx_id, "DIRECT");

    let call = alice_outbound_service.pop_call().unwrap();
    let alice_cancel_message = try_decode_transaction_cancelled_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(alice_cancel_message.tx_id, tx_id, "SAF");

    assert!(runtime
        .block_on(alice_ts.get_pending_outbound_transactions())
        .unwrap()
        .remove(&tx_id)
        .is_none());

    let mut builder = SenderTransactionProtocol::builder(1);
    let amount = MicroTari::from(10_000);
    let input = OutputBuilder::new()
        .with_value(100_000)
        .build(&factories.commitment)
        .unwrap();
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(input.as_transaction_input(), input.clone())
        .with_change_secret(PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories).unwrap();
    let tx_sender_msg = stp.build_single_round_message().unwrap();
    let tx_id2 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.into());
    runtime
        .block_on(alice_tx_sender.send(create_dummy_message(proto_message, &bob_node_identity.public_key())))
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
        .expect("Pending Transaction 2 should be in list");

    runtime.block_on(alice_ts.cancel_transaction(tx_id2)).unwrap();

    assert!(runtime
        .block_on(alice_ts.get_pending_inbound_transactions())
        .unwrap()
        .remove(&tx_id2)
        .is_none());

    // Lets cancel the last one using a Comms stack message
    let mut builder = SenderTransactionProtocol::builder(1);
    let amount = MicroTari::from(10_000);
    let input = OutputBuilder::new()
        .with_value(MicroTari::from(100_000))
        .with_spending_key(PrivateKey::random(&mut OsRng))
        .build(&factories.commitment)
        .unwrap();
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(input.as_transaction_input(), input.clone())
        .with_change_secret(PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories).unwrap();
    let tx_sender_msg = stp.build_single_round_message().unwrap();
    let tx_id3 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.into());
    runtime
        .block_on(alice_tx_sender.send(create_dummy_message(proto_message, &bob_node_identity.public_key())))
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
        .remove(&tx_id3)
        .expect("Pending Transaction 3 should be in list");

    let proto_message = proto::TransactionCancelledMessage { tx_id: tx_id3 };
    // Sent from the wrong source address so should not cancel
    runtime
        .block_on(alice_tx_cancelled_sender.send(create_dummy_message(
            proto_message,
            &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        )))
        .unwrap();

    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });

    runtime
        .block_on(alice_ts.get_pending_inbound_transactions())
        .unwrap()
        .remove(&tx_id3)
        .expect("Pending Transaction 3 should be in list");

    let proto_message = proto::TransactionCancelledMessage { tx_id: tx_id3 };
    runtime
        .block_on(alice_tx_cancelled_sender.send(create_dummy_message(proto_message, &bob_node_identity.public_key())))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(30)).fuse();
        let mut cancelled = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    if let TransactionEvent::TransactionCancelled(_) = &*event.unwrap() {
                       cancelled = true;
                       break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Should received cancelled event");
    });

    assert!(runtime
        .block_on(alice_ts.get_pending_inbound_transactions())
        .unwrap()
        .remove(&tx_id3)
        .is_none());
}

#[test]
fn test_transaction_cancellation_memory_db() {
    test_transaction_cancellation(TransactionMemoryDatabase::new());
}

#[test]
fn test_transaction_cancellation_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let connection = run_migration_and_create_sqlite_connection(&format!("{}/{}", db_folder, db_name)).unwrap();

    test_transaction_cancellation(TransactionServiceSqliteDatabase::new(connection, None));
}

#[test]
fn test_direct_vs_saf_send_of_tx_reply_and_finalize() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        mut _alice_tx_sender,
        mut alice_tx_ack_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);

    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
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

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    let (_, body) = alice_outbound_service.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let msg_tx_id = match tx_sender_msg.clone() {
        TransactionSenderMessage::Single(s) => s.tx_id,
        _ => {
            assert!(false, "Transaction is the not a single rounder sender variant");
            0
        },
    };
    assert_eq!(tx_id, msg_tx_id);

    // Test sending the Reply to a receiver with Direct and then with SAF and never both
    let (_bob_ts, _, bob_outbound_service, mut bob_tx_sender, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(
            &mut runtime,
            factories.clone(),
            TransactionMemoryDatabase::new(),
            Some(TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(20),
                chain_monitoring_timeout: Duration::from_secs(20),
                ..Default::default()
            }),
        );

    bob_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::Queued,
        broadcast: ResponseType::Failed,
    });

    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            tx_sender_msg.clone().into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();
    bob_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = bob_outbound_service.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let _: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });
    assert_eq!(bob_outbound_service.call_count(), 0, "Should be no more calls");

    let (_bob2_ts, _, bob2_outbound_service, mut bob2_tx_sender, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(
            &mut runtime,
            factories.clone(),
            TransactionMemoryDatabase::new(),
            Some(TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(20),
                chain_monitoring_timeout: Duration::from_secs(20),
                ..Default::default()
            }),
        );
    bob2_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::Failed,
        broadcast: ResponseType::Queued,
    });

    runtime
        .block_on(bob2_tx_sender.send(create_dummy_message(
            tx_sender_msg.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();

    bob2_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = bob2_outbound_service.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });
    assert_eq!(bob2_outbound_service.call_count(), 0, "Should be no more calls");

    // Test finalize is sent Direct Only.
    alice_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::Queued,
        broadcast: ResponseType::Queued,
    });

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            tx_reply_msg.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    let _ = alice_outbound_service.wait_call_count(1, Duration::from_secs(60));
    let _ = alice_outbound_service.pop_call().unwrap();

    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });
    assert_eq!(alice_outbound_service.call_count(), 0, "Should be no more calls");

    // Now to repeat sending so we can test the SAF send of the finalize message
    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let amount_sent = 20000 * uT;

    let _tx_id2 = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    let (_, body) = alice_outbound_service.pop_call().unwrap();

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
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = bob_outbound_service.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    alice_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::Failed,
        broadcast: ResponseType::Queued,
    });

    runtime
        .block_on(alice_tx_ack_sender.send(create_dummy_message(
            tx_reply_msg.into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    // Should be 1 SAF message
    let _ = alice_outbound_service.wait_call_count(1, Duration::from_secs(60));
    assert_eq!(alice_outbound_service.call_count(), 1);
    let _ = alice_outbound_service.pop_call();
    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });
    assert_eq!(alice_outbound_service.call_count(), 0, "Should be no more calls2");
}

#[test]
fn test_tx_direct_send_behaviour() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        mut _alice_tx_sender,
        mut _alice_tx_ack_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let uo = OutputBuilder::new()
        .with_value(1000000 * uT)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();
    let uo = OutputBuilder::new()
        .with_value(1000000 * uT)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();
    let uo = OutputBuilder::new()
        .with_value(1000000 * uT)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();
    let uo = OutputBuilder::new()
        .with_value(1000000 * uT)
        .build(&factories.commitment)
        .unwrap();
    runtime.block_on(alice_output_manager.add_output(uo)).unwrap();

    let amount_sent = 10000 * uT;

    alice_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::Failed,
        broadcast: ResponseType::Failed,
    });

    let _tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message1".to_string(),
        ))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut direct_count = 0;
        let mut saf_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionDirectSendResult(_, result) => if (!result) { direct_count+=1 },
                        TransactionEvent::TransactionStoreForwardSendResult(_, result) => if (!result) { saf_count+=1 },
                        _ => (),
                    }

                    if direct_count == 1 && saf_count == 1 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(direct_count, 1, "Should be 1 failed direct");
        assert_eq!(saf_count, 1, "Should be 1 failed saf");
    });

    alice_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::QueuedFail,
        broadcast: ResponseType::Queued,
    });

    let _tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message2".to_string(),
        ))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut direct_count = 0;
        let mut saf_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionDirectSendResult(_, result) => if (!result) { direct_count+=1 },
                        TransactionEvent::TransactionStoreForwardSendResult(_, result) => if *result { saf_count+=1 },
                        _ => (),
                    }

                    if direct_count == 1 && saf_count == 1 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(direct_count, 1, "Should be 1 failed direct");
        assert_eq!(saf_count, 1, "Should be 1 succeeded saf");
    });

    alice_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(1)),
        broadcast: ResponseType::Queued,
    });

    let _tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message3".to_string(),
        ))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut direct_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionDirectSendResult(_, result) => if *result { direct_count+=1 },
                        TransactionEvent::TransactionStoreForwardSendResult(_, _) => assert!(false, "Should be no SAF messages"),
                        _ => (),
                    }

                    if direct_count >= 1  {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(direct_count, 1, "Should be 1 succeeded direct");
    });

    alice_outbound_service.set_behaviour(MockBehaviour {
        direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(30)),
        broadcast: ResponseType::Queued,
    });

    let _tx_id = runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message4".to_string(),
        ))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut saf_count = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionStoreForwardSendResult(_, result) => if *result { saf_count+=1 },
                        TransactionEvent::TransactionDirectSendResult(_, result) => if *result { assert!(false, "Should be no direct messages") },
                        _ => (),
                    }

                    if saf_count >= 1  {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(saf_count, 1, "Should be 1 succeeded saf");
    });
}

#[test]
fn test_restarting_transaction_protocols() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let alice_backend = TransactionMemoryDatabase::new();
    let bob_backend = TransactionMemoryDatabase::new();

    let base_node_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    let alice_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    let bob_identity = Arc::new(
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap(),
    );

    // Bob is going to send a transaction to Alice
    let alice = TestParams::new(&mut OsRng);
    let bob = TestParams::new(&mut OsRng);
    let (utxo, input) = make_input(MicroTari(2000), &factories.commitment);
    let mut builder = SenderTransactionProtocol::builder(1);
    let fee = Fee::calculate(MicroTari(20), 1, 1, 1);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari(20))
        .with_offset(bob.offset.clone())
        .with_private_nonce(bob.nonce.clone())
        .with_input(utxo.clone(), input)
        .with_amount(0, MicroTari(2000) - fee - MicroTari(10));
    let mut bob_stp = builder.build::<Blake256>(&factories).unwrap();
    let msg = bob_stp.build_single_round_message().unwrap();
    let bob_pre_finalize = bob_stp.clone();

    let tx_id = msg.tx_id;

    let sender_info = TransactionSenderMessage::Single(Box::new(msg.clone()));
    let receiver_protocol = ReceiverTransactionProtocol::new(
        sender_info,
        alice.nonce.clone(),
        alice.spend_key.clone(),
        OutputFeatures::default(),
        &factories,
    );

    let alice_reply = receiver_protocol.get_signed_data().unwrap().clone();

    bob_stp
        .add_single_recipient_info(alice_reply.clone(), &factories.range_proof)
        .unwrap();

    match bob_stp.finalize(KernelFeatures::empty(), &factories) {
        Ok(_0) => (),
        _ => assert!(false, "Should be able to finalize tx"),
    };
    let tx = bob_stp.clone().get_transaction().unwrap().clone();

    let inbound_tx = InboundTransaction {
        tx_id,
        source_public_key: bob_identity.public_key().clone(),
        amount: msg.amount,
        receiver_protocol,
        status: TransactionStatus::Pending,
        message: msg.message.clone(),
        timestamp: Utc::now().naive_utc(),
        cancelled: false,
        direct_send_success: false,
        send_count: 0,
        last_send_timestamp: None,
    };

    alice_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx),
        )))
        .unwrap();

    let outbound_tx = OutboundTransaction {
        tx_id,
        destination_public_key: alice_identity.public_key().clone(),
        amount: msg.amount,
        fee,
        sender_protocol: bob_pre_finalize,
        status: TransactionStatus::Pending,
        message: msg.message,
        timestamp: Utc::now().naive_utc(),
        cancelled: false,
        direct_send_success: false,
        send_count: 0,
        last_send_timestamp: None,
    };
    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx),
        )))
        .unwrap();

    // Test that Bob's node restarts the send protocol
    let (mut bob_ts, _bob_oms, _bob_outbound_service, _, mut bob_tx_reply, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_backend, None);
    let mut bob_event_stream = bob_ts.get_event_stream_fused();

    runtime
        .block_on(bob_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(bob_ts.restart_transaction_protocols()).is_ok());

    runtime
        .block_on(bob_tx_reply.send(create_dummy_message(alice_reply.into(), &alice_identity.public_key())))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(15)).fuse();
        let mut received_reply = false;
        loop {
            futures::select! {
                event = bob_event_stream.select_next_some() => {
                     if let TransactionEvent::ReceivedTransactionReply(id) = (*event.unwrap()).clone() {
                        assert_eq!(id, tx_id);
                        received_reply = true;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(received_reply, "Should have received tx reply");
    });

    // Test Alice's node restarts the receive protocol
    let (mut alice_ts, _alice_oms, _alice_outbound_service, _, _, mut alice_tx_finalized, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), alice_backend, None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(alice_ts.restart_transaction_protocols()).is_ok());

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id,
        transaction: Some(tx.clone().into()),
    };

    runtime
        .block_on(alice_tx_finalized.send(create_dummy_message(
            finalized_transaction_message.clone(),
            bob_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(15)).fuse();
        let mut received_finalized = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(id) = (*event.unwrap()).clone() {
                        assert_eq!(id, tx_id);
                        received_finalized = true;
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(received_finalized, "Should have received finalized tx");
    });
}

#[test]
fn test_handling_coinbase_transactions() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let base_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        _,
        _,
        _,
        _,
        mut alice_base_node_response_sender,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(&mut runtime, factories.clone(), TransactionMemoryDatabase::new(), None);
    let mut alice_event_stream = alice_ts.get_event_stream_fused();

    let blockheight1 = 10;
    let fees1 = 2000 * uT;
    let reward1 = 1_000_000 * uT;

    let blockheight2 = 10;
    let fees2 = 3000 * uT;
    let reward2 = 2_000_000 * uT;
    let mut tx_id2 = 0;

    let blockheight3 = 11;
    let fees3 = 4000 * uT;
    let reward3 = 3_000_000 * uT;
    let tx_id3;

    let _tx1 = runtime
        .block_on(alice_ts.generate_coinbase_transaction(reward1, fees1, blockheight1))
        .unwrap();

    let transactions = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();
    assert_eq!(transactions.len(), 1);
    for tx in transactions.values() {
        assert_eq!(tx.amount, fees1 + reward1);
    }
    assert_eq!(
        runtime
            .block_on(alice_output_manager.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1
    );

    let _tx2 = runtime
        .block_on(alice_ts.generate_coinbase_transaction(reward2, fees2, blockheight2))
        .unwrap();

    let transactions = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();

    assert_eq!(transactions.len(), 1);
    for tx in transactions.values() {
        assert_eq!(tx.amount, fees2 + reward2);
        tx_id2 = tx.tx_id;
    }
    assert_eq!(
        runtime
            .block_on(alice_output_manager.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees2 + reward2
    );

    let _tx3 = runtime
        .block_on(alice_ts.generate_coinbase_transaction(reward3, fees3, blockheight3))
        .unwrap();

    let transactions = runtime.block_on(alice_ts.get_completed_transactions()).unwrap();

    assert_eq!(transactions.len(), 2);

    assert!(transactions.values().find(|tx| tx.amount == fees3 + reward3).is_some());
    tx_id3 = transactions
        .values()
        .find(|tx| tx.amount == fees3 + reward3)
        .clone()
        .unwrap()
        .tx_id;
    assert!(transactions.values().find(|tx| tx.amount == fees2 + reward2).is_some());
    assert!(transactions.values().find(|tx| tx.amount == fees1 + reward1).is_none());

    assert_eq!(
        runtime
            .block_on(alice_output_manager.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees2 + reward2 + fees3 + reward3
    );

    runtime
        .block_on(alice_ts.set_base_node_public_key(base_node_identity.public_key().clone()))
        .unwrap();

    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());

    // Two Coinbase Monitoring Protocols should be started at this stage.
    alice_outbound_service
        .wait_call_count(4, Duration::from_secs(30))
        .expect("Alice call wait 1");

    let mut chain_metadata_request = HashSet::new();
    let mut fetch_utxo_request = HashMap::new();
    for _ in 0..4 {
        let call = alice_outbound_service.pop_call().unwrap();
        if let Some(bsr) = try_decode_base_node_request(call.1.to_vec().clone()) {
            match bsr.request.unwrap() {
                BaseNodeRequestProto::GetChainMetadata(_c) => {
                    chain_metadata_request.insert(bsr.request_key);
                },
                BaseNodeRequestProto::FetchMatchingUtxos(f) => {
                    fetch_utxo_request.insert(bsr.request_key, f);
                },
                _ => (),
            }
        }
    }

    assert_eq!(chain_metadata_request.len(), 2);
    assert_eq!(fetch_utxo_request.len(), 2);

    let mut request_key1 = 0;
    for v in chain_metadata_request.iter() {
        assert!(fetch_utxo_request.keys().find(|k| k == &v).is_some());
        request_key1 = *v;
    }

    // Firstly lets respond with a higher tip than the request blockheight and see if the transaction gets cancelled as
    // it should
    let _ = chain_metadata_request.remove(&request_key1);

    let base_node_response_outputs = BaseNodeProto::BaseNodeServiceResponse {
        request_key: request_key1,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs { outputs: vec![].into() },
        )),
        is_synced: false,
    };
    let metadata_response1 = BaseNodeProto::BaseNodeServiceResponse {
        request_key: request_key1,
        response: Some(BaseNodeResponseProto::ChainMetadata(BaseNodeProto::ChainMetadata {
            height_of_longest_chain: Some(20),
            best_block: None,
            pruning_horizon: 0,
            accumulated_difficulty: Vec::new(),
            effective_pruned_height: 0,
        })),
        is_synced: false,
    };
    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response_outputs,
            base_node_identity.public_key(),
        )))
        .unwrap();
    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            metadata_response1,
            base_node_identity.public_key(),
        )))
        .unwrap();

    let mut transaction_cancelled2 = false;
    let mut transaction_cancelled3 = false;
    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     match &*event.unwrap() {
                       TransactionEvent::TransactionCancelled(tx_id) => {
                            if tx_id == &tx_id2 {
                                transaction_cancelled2 = true;
                                break;
                            }
                            if tx_id == &tx_id3 {
                                transaction_cancelled3 = true;
                                break;
                            }
                       }
                       _ => (),
                   }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(transaction_cancelled2 || transaction_cancelled3);
    });

    // Now lets send back an acceptable chain tip and affirmative output for a success
    let (target_tx_id, target_balance) = if transaction_cancelled2 {
        (tx_id3, reward3 + fees3)
    } else {
        (tx_id2, reward2 + fees2)
    };

    let target_tx = runtime
        .block_on(alice_ts.get_completed_transactions())
        .unwrap()
        .remove(&target_tx_id)
        .expect("Completed Transaction must be in collection   ");

    let mut request_key2 = 0;
    for v in chain_metadata_request.iter() {
        request_key2 = *v;
    }

    let target_tx_outputs: Vec<TransactionOutputProto> = target_tx
        .transaction
        .body
        .outputs()
        .iter()
        .map(|o| TransactionOutputProto::from(o.clone()))
        .collect();

    let base_node_response_outputs = BaseNodeProto::BaseNodeServiceResponse {
        request_key: request_key2,
        response: Some(BaseNodeResponseProto::TransactionOutputs(
            BaseNodeProto::TransactionOutputs {
                outputs: target_tx_outputs.into(),
            },
        )),
        is_synced: false,
    };
    let metadata_response1 = BaseNodeProto::BaseNodeServiceResponse {
        request_key: request_key2,
        response: Some(BaseNodeResponseProto::ChainMetadata(BaseNodeProto::ChainMetadata {
            height_of_longest_chain: Some(blockheight2),
            best_block: None,
            pruning_horizon: 0,
            accumulated_difficulty: Vec::new(),
            effective_pruned_height: 0,
        })),
        is_synced: false,
    };
    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            base_node_response_outputs,
            base_node_identity.public_key(),
        )))
        .unwrap();
    runtime
        .block_on(alice_base_node_response_sender.send(create_dummy_message(
            metadata_response1,
            base_node_identity.public_key(),
        )))
        .unwrap();

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut transaction_mined = false;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                     match &*event.unwrap() {
                       TransactionEvent::TransactionMined(tx_id) => {
                            if tx_id == &target_tx_id {
                                transaction_mined = true;
                                break;
                            }

                       }
                       _ => (),
                   }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(transaction_mined);
    });

    assert_eq!(
        runtime
            .block_on(alice_output_manager.get_balance())
            .unwrap()
            .available_balance,
        target_balance
    );

    // Finally just test that the protocol gets started after Base Node Pubkey is provided and that it repeats after the
    // timeout
    let _ = runtime
        .block_on(alice_ts.generate_coinbase_transaction(reward1, fees1, 66))
        .unwrap();

    alice_outbound_service
        .wait_call_count(4, Duration::from_secs(30))
        .expect("Alice call wait 2");
    // make sure that there are 2 sets of requests and they all have the same request key
    let mut request_key = 0;
    let mut fetch_count = 0;
    let mut metadata_count = 0;
    for _ in 0..4 {
        let call = alice_outbound_service.pop_call().unwrap();
        if let Some(bsr) = try_decode_base_node_request(call.1.to_vec().clone()) {
            match bsr.request.unwrap() {
                BaseNodeRequestProto::GetChainMetadata(_c) => {
                    if request_key == 0 {
                        request_key = bsr.request_key;
                    } else {
                        assert_eq!(request_key, bsr.request_key);
                    }
                    metadata_count += 1;
                },
                BaseNodeRequestProto::FetchMatchingUtxos(_f) => {
                    if request_key == 0 {
                        request_key = bsr.request_key;
                    } else {
                        assert_eq!(request_key, bsr.request_key);
                    }
                    fetch_count += 1;
                },
                _ => (),
            }
        }
    }
    assert_eq!(fetch_count, 2);
    assert_eq!(metadata_count, 2);
}

#[test]
fn test_transaction_resending() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    // Setup Alice wallet with no comms stack
    let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let alice_temp_dir = tempdir().unwrap();
    let alice_db_folder = alice_temp_dir.path().to_str().unwrap().to_string();
    let alice_connection =
        run_migration_and_create_sqlite_connection(&format!("{}/{}", alice_db_folder, alice_db_name)).unwrap();

    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        _alice_tx_sender,
        mut alice_tx_reply_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionServiceSqliteDatabase::new(alice_connection, None),
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
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

    // Check that there were repeats
    alice_outbound_service
        .wait_call_count(2, Duration::from_secs(30))
        .expect("Alice call wait 1");

    let mut alice_sender_message = TransactionSenderMessage::None;
    for _ in 0..2 {
        let call = alice_outbound_service.pop_call().unwrap();
        alice_sender_message = try_decode_sender_message(call.1.to_vec().clone()).unwrap();
        if let TransactionSenderMessage::Single(data) = alice_sender_message.clone() {
            assert_eq!(data.tx_id, tx_id);
        } else {
            assert!(false, "Should be a Single Transaction Sender Message")
        }
    }

    // Setup Bob's wallet with no comms stack
    let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let bob_temp_dir = tempdir().unwrap();
    let bob_db_folder = bob_temp_dir.path().to_str().unwrap().to_string();
    let bob_connection =
        run_migration_and_create_sqlite_connection(&format!("{}/{}", bob_db_folder, bob_db_name)).unwrap();

    let (
        _bob_ts,
        _bob_output_manager,
        bob_outbound_service,
        mut bob_tx_sender,
        mut _bob_tx_reply_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionServiceSqliteDatabase::new(bob_connection, None),
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Pass sender message to Bob's wallet
    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            alice_sender_message.clone().into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();

    // Check that the reply was repeated
    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(30))
        .expect("Bob call wait 1");

    let mut bob_reply_message;
    for _ in 0..2 {
        let call = bob_outbound_service.pop_call().unwrap();
        bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec().clone()).unwrap();
        assert_eq!(bob_reply_message.tx_id, tx_id);
    }

    // See if sending a second message too soon is ignored
    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            alice_sender_message.clone().into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();

    assert!(bob_outbound_service.wait_call_count(1, Duration::from_secs(4)).is_err());

    // Wait for the cooldown to expire but before the resend period has elapsed see if a repeat illicts a reponse.
    runtime.block_on(async { delay_for(Duration::from_secs(2)).await });
    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            alice_sender_message.into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();
    bob_outbound_service
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Bob call wait 2");
    let call = bob_outbound_service.pop_call().unwrap();
    bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(bob_reply_message.tx_id, tx_id);

    let _ = alice_outbound_service.take_calls();

    // Send the reply to Alice
    runtime
        .block_on(alice_tx_reply_sender.send(create_dummy_message(
            bob_reply_message.clone().into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 2");

    let call = alice_outbound_service.pop_call().unwrap();
    let alice_finalize_message = try_decode_finalized_transaction_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(alice_finalize_message.tx_id, tx_id);

    // See if sending a second message before cooldown and see if it is ignored
    runtime
        .block_on(alice_tx_reply_sender.send(create_dummy_message(
            bob_reply_message.clone().into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    assert!(alice_outbound_service
        .wait_call_count(1, Duration::from_secs(4))
        .is_err());

    // Wait for the cooldown to expire but before the resend period has elapsed see if a repeat illicts a reponse.
    runtime.block_on(async { delay_for(Duration::from_secs(2)).await });

    runtime
        .block_on(alice_tx_reply_sender.send(create_dummy_message(
            bob_reply_message.clone().into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 3");

    let call = alice_outbound_service.pop_call().unwrap();
    let alice_finalize_message = try_decode_finalized_transaction_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(alice_finalize_message.tx_id, tx_id);
}

#[test]
fn test_resend_on_startup() {
    // Test that messages are resent on startup if enough time has passed
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    // First we will check the Send Transction message
    let mut builder = SenderTransactionProtocol::builder(1);
    let amount = MicroTari::from(10_000);
    let input = OutputBuilder::new()
        .with_value(MicroTari::from(100_000))
        .with_spending_key(PrivateKey::random(&mut OsRng))
        .build(&factories.commitment)
        .unwrap();
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(input.as_transaction_input(), input.clone())
        .with_change_secret(PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories).unwrap();
    let stp_msg = stp.build_single_round_message().unwrap();
    let tx_sender_msg = TransactionSenderMessage::Single(Box::new(stp_msg.clone()));

    let tx_id = stp.get_tx_id().unwrap();
    let mut outbound_tx = OutboundTransaction {
        tx_id,
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount,
        fee: stp.clone().get_fee_amount().unwrap(),
        sender_protocol: stp.clone(),
        status: TransactionStatus::Pending,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: false,
        direct_send_success: false,
        send_count: 1,
        last_send_timestamp: Some(Utc::now().naive_utc()),
    };

    let alice_backend = TransactionMemoryDatabase::new();
    alice_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx.clone()),
        )))
        .unwrap();

    let (mut alice_ts, _, alice_outbound_service, _, _, _, _, _, _, _shutdown) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        alice_backend,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for alices base node, doesn't matter what
    runtime
        .block_on(alice_ts.set_base_node_public_key(alice_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(alice_ts.restart_broadcast_protocols()).is_ok());
    assert!(runtime.block_on(alice_ts.restart_transaction_protocols()).is_ok());

    // Check that if the cooldown is not done that a message will not be sent.
    assert!(alice_outbound_service
        .wait_call_count(1, Duration::from_secs(5))
        .is_err());
    drop(alice_ts);
    drop(alice_outbound_service);

    // Now we do it again with the timestamp prior to the cooldown and see that a message is sent
    outbound_tx.send_count = 1;
    outbound_tx.last_send_timestamp = Utc::now().naive_utc().checked_sub_signed(ChronoDuration::seconds(20));

    let alice_backend2 = TransactionMemoryDatabase::new();
    alice_backend2
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx),
        )))
        .unwrap();

    let (mut alice_ts2, _, alice_outbound_service2, _, _, _, _, _, _, _shutdown) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        alice_backend2,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for alices base node, doesn't matter what
    runtime
        .block_on(alice_ts2.set_base_node_public_key(alice_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(alice_ts2.restart_broadcast_protocols()).is_ok());
    assert!(runtime.block_on(alice_ts2.restart_transaction_protocols()).is_ok());

    // Check for resend on startup
    alice_outbound_service2
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Carol call wait 1");

    let call = alice_outbound_service2.pop_call().unwrap();

    if let TransactionSenderMessage::Single(data) = try_decode_sender_message(call.1.to_vec().clone()).unwrap() {
        assert_eq!(data.tx_id, tx_id);
    } else {
        assert!(false, "Should be a Single Transaction Sender Message")
    }

    // Now we do this for the Transaction Reply

    let rtp = ReceiverTransactionProtocol::new(
        tx_sender_msg,
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
        OutputFeatures::default(),
        &factories,
    );

    let mut inbound_tx = InboundTransaction {
        tx_id,
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount,
        receiver_protocol: rtp,
        status: TransactionStatus::Pending,
        message: "Yo2".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: false,
        direct_send_success: false,
        send_count: 0,
        last_send_timestamp: Some(Utc::now().naive_utc()),
    };

    let bob_backend = TransactionMemoryDatabase::new();
    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx.clone()),
        )))
        .unwrap();

    let (mut bob_ts, _, bob_outbound_service, _, _, _, _, _, _, _shutdown) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        bob_backend,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for bobs base node, doesn't matter what
    runtime
        .block_on(bob_ts.set_base_node_public_key(alice_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(bob_ts.restart_broadcast_protocols()).is_ok());
    assert!(runtime.block_on(bob_ts.restart_transaction_protocols()).is_ok());

    // Check that if the cooldown is not done that a message will not be sent.
    assert!(bob_outbound_service.wait_call_count(1, Duration::from_secs(5)).is_err());
    drop(bob_ts);
    drop(bob_outbound_service);

    // Now we do it again with the timestamp prior to the cooldown and see that a message is sent
    inbound_tx.send_count = 1;
    inbound_tx.last_send_timestamp = Utc::now().naive_utc().checked_sub_signed(ChronoDuration::seconds(20));

    let bob_backend2 = TransactionMemoryDatabase::new();
    bob_backend2
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx),
        )))
        .unwrap();

    let (mut bob_ts2, _, bob_outbound_service2, _, _, _, _, _, _, _shutdown) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        bob_backend2,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for bobs base node, doesn't matter what
    runtime
        .block_on(bob_ts2.set_base_node_public_key(alice_node_identity.public_key().clone()))
        .unwrap();

    assert!(runtime.block_on(bob_ts2.restart_broadcast_protocols()).is_ok());
    assert!(runtime.block_on(bob_ts2.restart_transaction_protocols()).is_ok());
    // Check for resend on startup

    bob_outbound_service2
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Dave call wait 1");

    let call = bob_outbound_service2.pop_call().unwrap();

    let reply = try_decode_transaction_reply_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(reply.tx_id, tx_id);
}

#[test]
fn test_replying_to_cancelled_tx() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    // Testing if a Tx Reply is received for a Cancelled Outbound Tx that a Cancelled message is sent back:
    let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let alice_temp_dir = tempdir().unwrap();
    let alice_db_folder = alice_temp_dir.path().to_str().unwrap().to_string();
    let alice_connection =
        run_migration_and_create_sqlite_connection(&format!("{}/{}", alice_db_folder, alice_db_name)).unwrap();
    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        _alice_tx_sender,
        mut alice_tx_reply_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionServiceSqliteDatabase::new(alice_connection, None),
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
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
    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 1");

    let call = alice_outbound_service.pop_call().unwrap();
    let alice_sender_message = try_decode_sender_message(call.1.to_vec().clone()).unwrap();
    if let TransactionSenderMessage::Single(data) = alice_sender_message.clone() {
        assert_eq!(data.tx_id, tx_id);
    }
    // Need a moment for Alice's wallet to finish writing to its database before cancelling
    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });

    runtime.block_on(alice_ts.cancel_transaction(tx_id)).unwrap();

    // Setup Bob's wallet with no comms stack
    let bob_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let bob_temp_dir = tempdir().unwrap();
    let bob_db_folder = bob_temp_dir.path().to_str().unwrap().to_string();
    let bob_connection =
        run_migration_and_create_sqlite_connection(&format!("{}/{}", bob_db_folder, bob_db_name)).unwrap();

    let (
        _bob_ts,
        _bob_output_manager,
        bob_outbound_service,
        mut bob_tx_sender,
        mut _bob_tx_reply_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionServiceSqliteDatabase::new(bob_connection, None),
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );

    // Pass sender message to Bob's wallet
    runtime
        .block_on(bob_tx_sender.send(create_dummy_message(
            alice_sender_message.clone().into(),
            alice_node_identity.public_key(),
        )))
        .unwrap();
    bob_outbound_service
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Bob call wait 1");

    let call = bob_outbound_service.pop_call().unwrap();
    let bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(bob_reply_message.tx_id, tx_id);

    // Wait for cooldown to expire
    runtime.block_on(async { delay_for(Duration::from_secs(5)).await });

    let _ = alice_outbound_service.take_calls();

    runtime
        .block_on(alice_tx_reply_sender.send(create_dummy_message(
            bob_reply_message.clone().into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    alice_outbound_service
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 2");

    let call = alice_outbound_service.pop_call().unwrap();
    let alice_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(alice_cancelled_message.tx_id, tx_id);
}

#[test]
fn test_transaction_timeout_cancellation() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    // Testing if a Tx Reply is received for a Cancelled Outbound Tx that a Cancelled message is sent back:
    let alice_db_name = format!("{}.sqlite3", random_string(8).as_str());
    let alice_temp_dir = tempdir().unwrap();
    let alice_db_folder = alice_temp_dir.path().to_str().unwrap().to_string();
    let alice_connection =
        run_migration_and_create_sqlite_connection(&format!("{}/{}", alice_db_folder, alice_db_name)).unwrap();
    let (
        mut alice_ts,
        mut alice_output_manager,
        alice_outbound_service,
        _alice_tx_sender,
        _alice_tx_reply_sender,
        _,
        _,
        _,
        _,
        _shutdown,
    ) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        TransactionServiceSqliteDatabase::new(alice_connection, None),
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let uo = OutputBuilder::new()
        .with_value(alice_total_available)
        .build(&factories.commitment)
        .unwrap();
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

    // For testing the resend period is set to 10 seconds and the timeout period is set to 15 seconds so we are going to
    // wait for 3 messages The intial send, the resend and then the cancellation
    alice_outbound_service
        .wait_call_count(3, Duration::from_secs(60))
        .expect("Alice call wait 1");

    let calls = alice_outbound_service.take_calls();

    // First call

    let sender_message = try_decode_sender_message(calls[0].1.to_vec().clone()).unwrap();
    if let TransactionSenderMessage::Single(data) = sender_message {
        assert_eq!(data.tx_id, tx_id);
    } else {
        assert!(false, "Should be a Single Transaction Sender Message")
    }
    // Resend
    let sender_message = try_decode_sender_message(calls[1].1.to_vec().clone()).unwrap();
    if let TransactionSenderMessage::Single(data) = sender_message {
        assert_eq!(data.tx_id, tx_id);
    } else {
        assert!(false, "Should be a Single Transaction Sender Message")
    }

    // Timeout Cancellation
    let alice_cancelled_message = try_decode_transaction_cancelled_message(calls[2].1.to_vec().clone()).unwrap();
    assert_eq!(alice_cancelled_message.tx_id, tx_id);

    // Now to test if the timeout has elapsed during downtime and that it is honoured on startup
    // First we will check the Send Transction message
    let mut builder = SenderTransactionProtocol::builder(1);
    let amount = MicroTari::from(10_000);
    let input = OutputBuilder::new()
        .with_value(100_00)
        .with_spending_key(PrivateKey::random(&mut OsRng))
        .build(&factories.commitment)
        .unwrap();
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input.as_transaction_input(),
            input.clone(),
        )
        .with_change_secret(PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories).unwrap();
    let stp_msg = stp.build_single_round_message().unwrap();
    let tx_sender_msg = TransactionSenderMessage::Single(Box::new(stp_msg.clone()));

    let tx_id = stp.get_tx_id().unwrap();
    let outbound_tx = OutboundTransaction {
        tx_id,
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount,
        fee: stp.clone().get_fee_amount().unwrap(),
        sender_protocol: stp.clone(),
        status: TransactionStatus::Pending,
        message: "Yo!".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::seconds(20))
            .unwrap(),
        cancelled: false,
        direct_send_success: false,
        send_count: 1,
        last_send_timestamp: Some(Utc::now().naive_utc()),
    };

    let bob_backend = TransactionMemoryDatabase::new();
    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx.clone()),
        )))
        .unwrap();

    let (mut bob_ts, _, bob_outbound_service, _, _, _, _, _, _, _shutdown) = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        bob_backend,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );

    // Need to set something for bobs base node, doesn't matter what
    runtime
        .block_on(bob_ts.set_base_node_public_key(bob_node_identity.public_key().clone()))
        .unwrap();
    assert!(runtime.block_on(bob_ts.restart_broadcast_protocols()).is_ok());
    assert!(runtime.block_on(bob_ts.restart_transaction_protocols()).is_ok());

    // Make sure we receive this before the timeout as it should be sent immideately on startup
    bob_outbound_service
        .wait_call_count(2, Duration::from_secs(14))
        .expect("Bob call wait 1");
    let call = bob_outbound_service.pop_call().unwrap();
    let bob_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(bob_cancelled_message.tx_id, tx_id);

    let call = bob_outbound_service.pop_call().unwrap();
    let bob_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec().clone()).unwrap();
    assert_eq!(bob_cancelled_message.tx_id, tx_id);

    // Now to do this for the Receiver
    let (carol_ts, _, carol_outbound_service, mut carol_tx_sender, _, _, _, _, _, _shutdown) =
        setup_transaction_service_no_comms(
            &mut runtime,
            factories.clone(),
            TransactionMemoryDatabase::new(),
            Some(TransactionServiceConfig {
                transaction_resend_period: Duration::from_secs(10),
                resend_response_cooldown: Duration::from_secs(5),
                pending_transaction_cancellation_timeout: Duration::from_secs(15),
                ..Default::default()
            }),
        );
    let mut carol_event_stream = carol_ts.get_event_stream_fused();

    runtime
        .block_on(carol_tx_sender.send(create_dummy_message(
            tx_sender_msg.clone().into(),
            bob_node_identity.public_key(),
        )))
        .unwrap();

    // Then we should get 2 reply messages and 1 cancellation event
    carol_outbound_service
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Carol call wait 1");

    let calls = carol_outbound_service.take_calls();

    // Initial Reply
    let carol_reply_message = try_decode_transaction_reply_message(calls[0].1.to_vec().clone()).unwrap();
    assert_eq!(carol_reply_message.tx_id, tx_id);

    // Resend
    let carol_reply_message = try_decode_transaction_reply_message(calls[1].1.to_vec().clone()).unwrap();
    assert_eq!(carol_reply_message.tx_id, tx_id);

    runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut transaction_cancelled = false;
        loop {
            futures::select! {
                event = carol_event_stream.select_next_some() => {
                     match &*event.unwrap() {
                       TransactionEvent::TransactionCancelled(t) => {
                            if t == &tx_id {
                                transaction_cancelled = true;
                                break;
                            }

                       }
                       _ => (),
                   }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(transaction_cancelled, "Transaction must be cancelled");
    });
}
