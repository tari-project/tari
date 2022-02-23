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

use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    path::Path,
    sync::Arc,
    time::Duration,
};

use chrono::{Duration as ChronoDuration, Utc};
use futures::{
    channel::{mpsc, mpsc::Sender},
    FutureExt,
    SinkExt,
};
use prost::Message;
use rand::rngs::OsRng;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    transaction::{ImportStatus, TransactionDirection, TransactionStatus, TxId},
    types::{PrivateKey, PublicKey, Signature},
};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::rpc::{mock::MockRpcServer, NamedProtocolService},
    test_utils::node_identity::build_node_identity,
    CommsNode,
    PeerConnection,
};
use tari_comms_dht::outbound::mock::{
    create_outbound_service_mock,
    MockBehaviour,
    OutboundServiceMockState,
    ResponseType,
};
use tari_core::{
    base_node::{
        proto::wallet_rpc::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::BaseNodeWalletRpcServer,
    },
    blocks::BlockHeader,
    consensus::ConsensusConstantsBuilder,
    covenants::Covenant,
    proto::{
        base_node as base_node_proto,
        base_node::{
            TxLocation as TxLocationProto,
            TxQueryBatchResponse as TxQueryBatchResponseProto,
            TxQueryBatchResponses as TxQueryBatchResponsesProto,
        },
        types::Signature as SignatureProto,
    },
    transactions::{
        fee::Fee,
        tari_amount::*,
        test_helpers::{create_unblinded_output, TestParams as TestParamsHelpers},
        transaction_components::{KernelBuilder, KernelFeatures, OutputFeatures, Transaction},
        transaction_protocol::{
            proto::protocol as proto,
            recipient::RecipientSignedMessage,
            sender::TransactionSenderMessage,
        },
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    common::Blake256,
    inputs,
    keys::{PublicKey as PK, SecretKey as SK},
    script,
    script::{ExecutionStack, TariScript},
};
use tari_key_manager::cipher_seed::CipherSeed;
use tari_p2p::{comms_connector::pubsub_connector, domain_message::DomainMessage, Network};
use tari_service_framework::{reply_channel, RegisterHandle, StackBuilder};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::random;
use tari_utilities::Hashable;
use tari_wallet::{
    base_node_service::{
        config::BaseNodeServiceConfig,
        handle::BaseNodeServiceHandle,
        mock_base_node_service::MockBaseNodeService,
        BaseNodeServiceInitializer,
    },
    connectivity_service::{
        create_wallet_connectivity_mock,
        WalletConnectivityHandle,
        WalletConnectivityInitializer,
        WalletConnectivityInterface,
        WalletConnectivityMock,
    },
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::{Balance, OutputManagerService},
        storage::{
            database::OutputManagerDatabase,
            models::KnownOneSidedPaymentScript,
            sqlite_db::OutputManagerSqliteDatabase,
        },
        OutputManagerServiceInitializer,
    },
    storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::{run_migration_and_create_sqlite_connection, WalletDbConnection},
    },
    test_utils::{create_consensus_constants, make_wallet_database_connection},
    transaction_service::{
        config::TransactionServiceConfig,
        error::TransactionServiceError,
        handle::{TransactionEvent, TransactionServiceHandle},
        service::TransactionService,
        storage::{
            database::{DbKeyValuePair, TransactionBackend, TransactionDatabase, WriteOperation},
            models::{CompletedTransaction, InboundTransaction, OutboundTransaction, WalletTransaction},
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
    time::sleep,
};

use crate::support::{
    comms_and_services::{create_dummy_message, get_next_memory_address, setup_comms_services},
    comms_rpc::{connect_rpc_client, BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    utils::{make_input, TestParams},
};

fn create_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .worker_threads(8)
        .build()
        .unwrap()
}

pub fn setup_transaction_service<P: AsRef<Path>>(
    runtime: &mut Runtime,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    factories: CryptoFactories,
    db_connection: WalletDbConnection,
    database_path: P,
    discovery_request_timeout: Duration,
    shutdown_signal: ShutdownSignal,
) -> (
    TransactionServiceHandle,
    OutputManagerHandle,
    CommsNode,
    WalletConnectivityHandle,
) {
    let _enter = runtime.enter();
    let (publisher, subscription_factory) = pubsub_connector(100, 20);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = runtime.block_on(setup_comms_services(
        node_identity,
        peers,
        publisher,
        database_path.as_ref().to_str().unwrap().to_owned(),
        discovery_request_timeout,
        shutdown_signal.clone(),
    ));

    let db = WalletDatabase::new(WalletSqliteDatabase::new(db_connection.clone(), None).unwrap());
    let metadata = ChainMetadata::new(std::i64::MAX as u64, Vec::new(), 0, 0, 0);

    runtime.block_on(db.set_chain_metadata(metadata)).unwrap();

    let ts_backend = TransactionServiceSqliteDatabase::new(db_connection.clone(), None);
    let oms_backend = OutputManagerSqliteDatabase::new(db_connection, None);

    let fut = StackBuilder::new(shutdown_signal)
        .add_initializer(RegisterHandle::new(dht))
        .add_initializer(RegisterHandle::new(comms.connectivity()))
        .add_initializer(OutputManagerServiceInitializer::new(
            OutputManagerServiceConfig::default(),
            oms_backend,
            factories.clone(),
            Network::Weatherwax.into(),
            CipherSeed::new(),
            comms.node_identity(),
        ))
        .add_initializer(TransactionServiceInitializer::new(
            TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(5),
                chain_monitoring_timeout: Duration::from_secs(5),
                low_power_polling_timeout: Duration::from_secs(20),
                num_confirmations_required: 0,
                ..Default::default()
            },
            subscription_factory,
            ts_backend,
            comms.node_identity(),
            factories,
            db.clone(),
        ))
        .add_initializer(BaseNodeServiceInitializer::new(BaseNodeServiceConfig::default(), db))
        .add_initializer(WalletConnectivityInitializer::new(BaseNodeServiceConfig::default()))
        .build();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let output_manager_handle = handles.expect_handle::<OutputManagerHandle>();
    let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();
    let connectivity_service_handle = handles.expect_handle::<WalletConnectivityHandle>();

    (
        transaction_service_handle,
        output_manager_handle,
        comms,
        connectivity_service_handle,
    )
}

/// This struct holds a collection of interfaces that can be used in tests to interact with a Transaction Service that
/// is constructed without a comms layer, base node etc
pub struct TransactionServiceNoCommsInterface {
    transaction_service_handle: TransactionServiceHandle,
    output_manager_service_handle: OutputManagerHandle,
    outbound_service_mock_state: OutboundServiceMockState,
    transaction_send_message_channel: Sender<DomainMessage<proto::TransactionSenderMessage>>,
    transaction_ack_message_channel: Sender<DomainMessage<proto::RecipientSignedMessage>>,
    transaction_finalize_message_channel: Sender<DomainMessage<proto::TransactionFinalizedMessage>>,
    _base_node_response_message_channel: Sender<DomainMessage<base_node_proto::BaseNodeServiceResponse>>,
    transaction_cancelled_message_channel: Sender<DomainMessage<proto::TransactionCancelledMessage>>,
    _shutdown: Shutdown,
    _mock_rpc_server: MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    base_node_identity: Arc<NodeIdentity>,
    base_node_rpc_mock_state: BaseNodeWalletRpcMockState,
    wallet_connectivity_service_mock: WalletConnectivityMock,
    _rpc_server_connection: PeerConnection,
    output_manager_service_event_publisher: broadcast::Sender<Arc<OutputManagerEvent>>,
}

/// This utility function creates a Transaction service without using the Service Framework Stack and exposes all the
/// streams for testing purposes.
#[allow(clippy::type_complexity)]
pub fn setup_transaction_service_no_comms(
    runtime: &mut Runtime,
    factories: CryptoFactories,
    db_connection: WalletDbConnection,
    config: Option<TransactionServiceConfig>,
) -> TransactionServiceNoCommsInterface {
    let (oms_request_sender, oms_request_receiver) = reply_channel::unbounded();

    let (output_manager_service_event_publisher, _) = broadcast::channel(200);
    let (outbound_message_requester, mock_outbound_service) = create_outbound_service_mock(100);

    let (ts_request_sender, ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let transaction_service_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher.clone());
    let (transaction_send_message_channel, tx_receiver) = mpsc::channel(20);
    let (transaction_ack_message_channel, tx_ack_receiver) = mpsc::channel(20);
    let (transaction_finalize_message_channel, tx_finalized_receiver) = mpsc::channel(20);
    let (base_node_response_message_channel, base_node_response_receiver) = mpsc::channel(20);
    let (transaction_cancelled_message_channel, tx_cancelled_receiver) = mpsc::channel(20);

    let outbound_service_mock_state = mock_outbound_service.get_state();
    runtime.spawn(mock_outbound_service.run());

    let service = BaseNodeWalletRpcMockService::new();
    let base_node_rpc_mock_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();

    let base_node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let mut mock_rpc_server = {
        let _enter = runtime.handle().enter();
        MockRpcServer::new(server, base_node_identity.clone())
    };

    {
        let _enter = runtime.handle().enter();
        mock_rpc_server.serve();
    }

    let wallet_connectivity_service_mock = create_wallet_connectivity_mock();

    let mut rpc_server_connection = runtime.block_on(async {
        mock_rpc_server
            .create_connection(base_node_identity.to_peer(), protocol_name.into())
            .await
    });

    runtime.block_on(async {
        wallet_connectivity_service_mock
            .set_base_node_wallet_rpc_client(connect_rpc_client(&mut rpc_server_connection).await)
    });

    let constants = ConsensusConstantsBuilder::new(Network::Weatherwax).build();

    let shutdown = Shutdown::new();

    let (sender, receiver_bns) = reply_channel::unbounded();
    let (base_node_service_event_publisher, _) = broadcast::channel(100);

    let base_node_service_handle = BaseNodeServiceHandle::new(sender, base_node_service_event_publisher);
    let mut mock_base_node_service = MockBaseNodeService::new(receiver_bns, shutdown.to_signal());
    mock_base_node_service.set_default_base_node_state();
    runtime.spawn(mock_base_node_service.run());

    let wallet_db = WalletDatabase::new(
        WalletSqliteDatabase::new(db_connection.clone(), None).expect("Should be able to create wallet database"),
    );
    let ts_db = TransactionDatabase::new(TransactionServiceSqliteDatabase::new(db_connection.clone(), None));
    let oms_db = OutputManagerDatabase::new(OutputManagerSqliteDatabase::new(db_connection, None));
    let output_manager_service = runtime
        .block_on(OutputManagerService::new(
            OutputManagerServiceConfig::default(),
            oms_request_receiver,
            oms_db,
            output_manager_service_event_publisher.clone(),
            factories.clone(),
            constants,
            shutdown.to_signal(),
            base_node_service_handle.clone(),
            wallet_connectivity_service_mock.clone(),
            CipherSeed::new(),
            base_node_identity.clone(),
        ))
        .unwrap();

    let output_manager_service_handle =
        OutputManagerHandle::new(oms_request_sender, output_manager_service_event_publisher.clone());

    let test_config = config.unwrap_or(TransactionServiceConfig {
        broadcast_monitoring_timeout: Duration::from_secs(5),
        chain_monitoring_timeout: Duration::from_secs(5),
        direct_send_timeout: Duration::from_secs(5),
        broadcast_send_timeout: Duration::from_secs(5),
        low_power_polling_timeout: Duration::from_secs(6),
        transaction_resend_period: Duration::from_secs(200),
        resend_response_cooldown: Duration::from_secs(200),
        pending_transaction_cancellation_timeout: Duration::from_secs(300),
        transaction_mempool_resubmission_window: Duration::from_secs(2),
        max_tx_query_batch_size: 2,
        ..Default::default()
    });

    let ts_service = TransactionService::new(
        test_config,
        ts_db,
        wallet_db,
        ts_request_receiver,
        tx_receiver,
        tx_ack_receiver,
        tx_finalized_receiver,
        base_node_response_receiver,
        tx_cancelled_receiver,
        output_manager_service_handle.clone(),
        outbound_message_requester,
        wallet_connectivity_service_mock.clone(),
        event_publisher,
        base_node_identity.clone(),
        factories,
        shutdown.to_signal(),
        base_node_service_handle,
    );
    runtime.spawn(async move { output_manager_service.start().await.unwrap() });
    runtime.spawn(async move { ts_service.start().await.unwrap() });
    TransactionServiceNoCommsInterface {
        transaction_service_handle,
        output_manager_service_handle,
        outbound_service_mock_state,
        transaction_send_message_channel,
        transaction_ack_message_channel,
        transaction_finalize_message_channel,
        _base_node_response_message_channel: base_node_response_message_channel,
        transaction_cancelled_message_channel,
        _shutdown: shutdown,
        _mock_rpc_server: mock_rpc_server,
        base_node_identity,
        base_node_rpc_mock_state,
        wallet_connectivity_service_mock,
        _rpc_server_connection: rpc_server_connection,
        output_manager_service_event_publisher,
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

#[test]
#[ignore = "broken since validator node merge"]
fn manage_single_transaction() {
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );
    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let (alice_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));
    let (bob_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, mut alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![],
        factories.clone(),
        alice_connection,
        database_path.clone(),
        Duration::from_secs(0),
        shutdown.to_signal(),
    );

    alice_connectivity.set_base_node(base_node_identity.to_peer());

    let mut alice_event_stream = alice_ts.get_event_stream();

    runtime.block_on(async { sleep(Duration::from_secs(2)).await });

    let (mut bob_ts, mut bob_oms, bob_comms, mut bob_connectivity) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_connection,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );
    bob_connectivity.set_base_node(base_node_identity.to_peer());

    let mut bob_event_stream = bob_ts.get_event_stream();

    let _ = runtime.block_on(
        bob_comms
            .connectivity()
            .dial_peer(alice_node_identity.node_id().clone()),
    );

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

    runtime.block_on(alice_oms.add_output(uo1, None)).unwrap();
    let message = "TAKE MAH MONEYS!".to_string();
    runtime
        .block_on(alice_ts.send_transaction(
            bob_node_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            message,
        ))
        .expect("Alice sending tx");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(90));
        tokio::pin!(delay);
        let mut count = 0;
        loop {
            tokio::select! {
                _event = alice_event_stream.recv() => {
                    println!("alice: {:?}", &*_event.as_ref().unwrap());
                    count+=1;
                    if count>=2 {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });

    let mut tx_id = TxId::from(0);
    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(90));
        tokio::pin!(delay);
        let mut finalized = 0;
        loop {
            tokio::select! {
                event = bob_event_stream.recv() => {
                    if let TransactionEvent::ReceivedFinalizedTransaction(id) = &*event.unwrap() {
                        tx_id = *id;
                        finalized+=1;
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(finalized, 1);
    });

    assert!(runtime.block_on(bob_ts.get_completed_transaction(999.into())).is_err());

    let _bob_completed_tx = runtime
        .block_on(bob_ts.get_completed_transaction(tx_id))
        .expect("Could not find tx");

    assert_eq!(
        runtime
            .block_on(bob_oms.get_balance())
            .unwrap()
            .pending_incoming_balance,
        value
    );
}

#[test]
fn single_transaction_to_self() {
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();

    let (db_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, mut alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![],
        factories.clone(),
        db_connection,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );

    alice_connectivity.set_base_node(base_node_identity.to_peer());

    runtime.block_on(async move {
        let initial_wallet_value = 2500.into();
        let (_utxo, uo1) = make_input(&mut OsRng, initial_wallet_value, &factories.commitment);

        alice_oms.add_output(uo1, None).await.unwrap();
        let message = "TAKE MAH _OWN_ MONEYS!".to_string();
        let value = 1000.into();
        let tx_id = alice_ts
            .send_transaction(
                alice_node_identity.public_key().clone(),
                value,
                20.into(),
                message.clone(),
            )
            .await
            .expect("Alice sending tx");

        let completed_tx = alice_ts
            .get_completed_transaction(tx_id)
            .await
            .expect("Could not find tx");

        let fees = completed_tx.fee;

        assert_eq!(
            alice_oms.get_balance().await.unwrap().pending_incoming_balance,
            initial_wallet_value - fees
        );
    });
}

#[test]
fn send_one_sided_transaction_to_other() {
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();

    let (db_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, mut alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity,
        vec![],
        factories.clone(),
        db_connection,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );

    let mut alice_event_stream = alice_ts.get_event_stream();

    alice_connectivity.set_base_node(base_node_identity.to_peer());

    let initial_wallet_value = 2500.into();
    let (_utxo, uo1) = make_input(&mut OsRng, initial_wallet_value, &factories.commitment);
    let mut alice_oms_clone = alice_oms.clone();
    runtime.block_on(async move { alice_oms_clone.add_output(uo1, None).await.unwrap() });

    let message = "SEE IF YOU CAN CATCH THIS ONE..... SIDED TX!".to_string();
    let value = 1000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let tx_id = runtime.block_on(async move {
        alice_ts_clone
            .send_one_sided_transaction(
                bob_node_identity.public_key().clone(),
                value,
                20.into(),
                message.clone(),
            )
            .await
            .expect("Alice sending one-sided tx to Bob")
    });

    runtime.block_on(async move {
        let completed_tx = alice_ts
            .get_completed_transaction(tx_id)
            .await
            .expect("Could not find completed one-sided tx");

        let fees = completed_tx.fee;

        assert_eq!(
            alice_oms.get_balance().await.unwrap().pending_incoming_balance,
            initial_wallet_value - value - fees
        );
    });

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);
        let mut found = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionCompletedImmediately(id) = &*event.unwrap() {
                        if id == &tx_id {
                            found = true;
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(found, "'TransactionCompletedImmediately(_)' event not found");
    });
}

#[test]
fn recover_one_sided_transaction() {
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let temp_dir2 = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let database_path2 = temp_dir2.path().to_str().unwrap().to_string();

    let (alice_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));
    let (bob_connection, _tempdir) = make_wallet_database_connection(Some(database_path2.clone()));

    let shutdown = Shutdown::new();
    let (mut alice_ts, alice_oms, _alice_comms, mut alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity,
        vec![],
        factories.clone(),
        alice_connection,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );

    let (_bob_ts, mut bob_oms, _bob_comms, _bob_connectivity) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![],
        factories.clone(),
        bob_connection,
        database_path2,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );
    let script = script!(PushPubKey(Box::new(bob_node_identity.public_key().clone())));
    let known_script = KnownOneSidedPaymentScript {
        script_hash: script.as_hash::<Blake256>().unwrap().to_vec(),
        private_key: bob_node_identity.secret_key().clone(),
        script,
        input: ExecutionStack::default(),
        script_lock_height: 0,
    };
    let mut cloned_bob_oms = bob_oms.clone();
    runtime.block_on(async move {
        cloned_bob_oms.add_known_script(known_script).await.unwrap();
    });

    alice_connectivity.set_base_node(base_node_identity.to_peer());

    let initial_wallet_value = 2500.into();
    let (_utxo, uo1) = make_input(&mut OsRng, initial_wallet_value, &factories.commitment);
    let mut alice_oms_clone = alice_oms;
    runtime.block_on(async move { alice_oms_clone.add_output(uo1, None).await.unwrap() });

    let message = "".to_string();
    let value = 1000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let tx_id = runtime.block_on(async move {
        alice_ts_clone
            .send_one_sided_transaction(
                bob_node_identity.public_key().clone(),
                value,
                20.into(),
                message.clone(),
            )
            .await
            .expect("Alice sending one-sided tx to Bob")
    });

    runtime.block_on(async move {
        let completed_tx = alice_ts
            .get_completed_transaction(tx_id)
            .await
            .expect("Could not find completed one-sided tx");
        let outputs = completed_tx.transaction.body.outputs().clone();

        let unblinded = bob_oms
            .scan_outputs_for_one_sided_payments(outputs.clone(), TxId::new_random())
            .await
            .unwrap();
        // Bob should be able to claim 1 output.
        assert_eq!(1, unblinded.len());
        assert_eq!(value, unblinded[0].value);

        // Should ignore already existing outputs
        let unblinded = bob_oms
            .scan_outputs_for_one_sided_payments(outputs, TxId::new_random())
            .await
            .unwrap();
        assert!(unblinded.is_empty());
    });
}

#[test]
fn test_htlc_send_and_claim() {
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let temp_dir_bob = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let path_string = temp_dir_bob.path().to_str().unwrap().to_string();
    let bob_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let bob_db_path = format!("{}/{}", path_string, bob_db_name);

    let (db_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));
    let bob_connection = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, mut alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity,
        vec![],
        factories.clone(),
        db_connection,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );

    let mut bob_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_connection, None);

    log::info!(
        "manage_single_transaction: Bob: '{}'",
        bob_ts_interface.base_node_identity.node_id().short_str(),
    );

    let mut alice_event_stream = alice_ts.get_event_stream();

    alice_connectivity.set_base_node(base_node_identity.to_peer());

    let initial_wallet_value = 2500.into();
    let (_utxo, uo1) = make_input(&mut OsRng, initial_wallet_value, &factories.commitment);
    let mut alice_oms_clone = alice_oms.clone();
    runtime.block_on(async move { alice_oms_clone.add_output(uo1, None).await.unwrap() });

    let message = "".to_string();
    let value = 1000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let bob_pubkey = bob_ts_interface.base_node_identity.public_key().clone();
    let (tx_id, pre_image, output) = runtime.block_on(async move {
        alice_ts_clone
            .send_sha_atomic_swap_transaction(bob_pubkey, value, 20.into(), message.clone())
            .await
            .expect("Alice sending HTLC transaction")
    });

    runtime.block_on(async move {
        let completed_tx = alice_ts
            .get_completed_transaction(tx_id)
            .await
            .expect("Could not find completed HTLC tx");

        let fees = completed_tx.fee;

        assert_eq!(
            alice_oms.get_balance().await.unwrap().pending_incoming_balance,
            initial_wallet_value - value - fees
        );
    });

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionCompletedImmediately(id) = &*event.unwrap() {
                        if id == &tx_id {
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });
    let hash = output.hash();
    bob_ts_interface.base_node_rpc_mock_state.set_utxos(vec![output]);
    runtime.block_on(async move {
        let (tx_id_htlc, _htlc_fee, htlc_amount, tx) = bob_ts_interface
            .output_manager_service_handle
            .create_claim_sha_atomic_swap_transaction(hash, pre_image, 20.into())
            .await
            .unwrap();

        bob_ts_interface
            .transaction_service_handle
            .submit_transaction(tx_id_htlc, tx, htlc_amount, "".to_string())
            .await
            .unwrap();
        assert_eq!(
            bob_ts_interface
                .output_manager_service_handle
                .get_balance()
                .await
                .unwrap()
                .pending_incoming_balance,
            htlc_amount
        );
    });
}

#[test]
fn send_one_sided_transaction_to_self() {
    let mut runtime = create_runtime();

    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();

    let (alice_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));

    let shutdown = Shutdown::new();
    let (alice_ts, alice_oms, _alice_comms, mut alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![],
        factories.clone(),
        alice_connection,
        database_path,
        Duration::from_secs(0),
        shutdown.to_signal(),
    );

    alice_connectivity.set_base_node(base_node_identity.to_peer());

    let initial_wallet_value = 2500.into();
    let (_utxo, uo1) = make_input(&mut OsRng, initial_wallet_value, &factories.commitment);
    let mut alice_oms_clone = alice_oms;
    runtime.block_on(async move { alice_oms_clone.add_output(uo1, None).await.unwrap() });

    let message = "SEE IF YOU CAN CATCH THIS ONE..... SIDED TX!".to_string();
    let value = 1000.into();
    let mut alice_ts_clone = alice_ts;
    let _tx_id = runtime.block_on(async move {
        match alice_ts_clone
            .send_one_sided_transaction(
                alice_node_identity.public_key().clone(),
                value,
                20.into(),
                message.clone(),
            )
            .await
        {
            Err(TransactionServiceError::OneSidedTransactionError(e)) => {
                assert_eq!(e.as_str(), "One-sided spend-to-self transactions not supported");
            },
            _ => {
                panic!("Expected: OneSidedTransactionError(\"One-sided spend-to-self transactions not supported\")");
            },
        };
    });
}

#[test]
fn manage_multiple_transactions() {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Carols's parameters
    let carol_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "wallet::manage_multiple_transactions: Alice: '{}', Bob: '{}', carol: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        carol_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();

    let database_path = temp_dir.path().to_str().unwrap().to_string();

    let (alice_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));
    let (bob_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));
    let (carol_connection, _tempdir) = make_wallet_database_connection(Some(database_path.clone()));

    let mut shutdown = Shutdown::new();

    let (mut alice_ts, mut alice_oms, alice_comms, _alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        alice_connection,
        database_path.clone(),
        Duration::from_secs(60),
        shutdown.to_signal(),
    );
    let mut alice_event_stream = alice_ts.get_event_stream();

    runtime.block_on(async { sleep(Duration::from_secs(5)).await });

    // Spin up Bob and Carol
    let (mut bob_ts, mut bob_oms, bob_comms, _bob_connectivity) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        bob_connection,
        database_path.clone(),
        Duration::from_secs(1),
        shutdown.to_signal(),
    );
    let mut bob_event_stream = bob_ts.get_event_stream();
    runtime.block_on(async { sleep(Duration::from_secs(5)).await });

    let (mut carol_ts, mut carol_oms, carol_comms, _carol_connectivity) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone()],
        factories.clone(),
        carol_connection,
        database_path,
        Duration::from_secs(1),
        shutdown.to_signal(),
    );
    let mut carol_event_stream = carol_ts.get_event_stream();

    // Establish some connections beforehand, to reduce the amount of work done concurrently in tests
    // Connect Bob and Alice
    runtime.block_on(async { sleep(Duration::from_secs(3)).await });

    let _ = runtime.block_on(
        bob_comms
            .connectivity()
            .dial_peer(alice_node_identity.node_id().clone()),
    );
    runtime.block_on(async { sleep(Duration::from_secs(3)).await });

    // Connect alice to carol
    let _ = runtime.block_on(
        alice_comms
            .connectivity()
            .dial_peer(carol_node_identity.node_id().clone()),
    );

    let (_utxo, uo2) = make_input(&mut OsRng, MicroTari(3500), &factories.commitment);
    runtime.block_on(bob_oms.add_output(uo2, None)).unwrap();
    let (_utxo, uo3) = make_input(&mut OsRng, MicroTari(4500), &factories.commitment);
    runtime.block_on(carol_oms.add_output(uo3, None)).unwrap();

    // Add some funds to Alices wallet
    let (_utxo, uo1a) = make_input(&mut OsRng, MicroTari(5500), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1a, None)).unwrap();
    let (_utxo, uo1b) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1b, None)).unwrap();
    let (_utxo, uo1c) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1c, None)).unwrap();

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
        let delay = sleep(Duration::from_secs(90));
        tokio::pin!(delay);
        let mut tx_reply = 0;
        let mut finalized = 0;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => tx_reply+=1,
                        TransactionEvent::ReceivedFinalizedTransaction(_) => finalized+=1,
                        _ => (),
                    }

                    if tx_reply == 3 && finalized ==1 {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(tx_reply, 3, "Need 3 replies");
        assert_eq!(finalized, 1);
    });

    log::trace!("Alice received all Tx messages");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(90));

        tokio::pin!(delay);
        let mut tx_reply = 0;
        let mut finalized = 0;
        loop {
            tokio::select! {
                event = bob_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => tx_reply+=1,
                        TransactionEvent::ReceivedFinalizedTransaction(_) => finalized+=1,
                        _ => (),
                    }
                    if tx_reply == 1 && finalized == 2 {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(tx_reply, 1);
        assert_eq!(finalized, 2);
    });

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(90));
        tokio::pin!(delay);

        tokio::pin!(delay);
        let mut finalized = 0;
        loop {
            tokio::select! {
                event = carol_event_stream.recv() => {
                    if let TransactionEvent::ReceivedFinalizedTransaction(_) = &*event.unwrap() { finalized+=1 }
                },
                () = &mut delay => {
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

    shutdown.trigger();
    runtime.block_on(async move {
        alice_comms.wait_until_shutdown().await;
        bob_comms.wait_until_shutdown().await;
        carol_comms.wait_until_shutdown().await;
    });
}

#[test]
fn test_accepting_unknown_tx_id_and_malformed_reply() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let temp_dir = tempdir().unwrap();
    let path_string = temp_dir.path().to_str().unwrap().to_string();
    let alice_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path, 16).unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let mut alice_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection_alice, None);

    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);

    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            MicroTari::from(5000),
            MicroTari::from(20),
            "".to_string(),
        ))
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

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
        &factories,
    );

    let mut tx_reply = rtp.get_signed_data().unwrap().clone();
    let mut wrong_tx_id = tx_reply.clone();
    wrong_tx_id.tx_id = 2.into();
    let (_p, pub_key) = PublicKey::random_keypair(&mut OsRng);
    tx_reply.public_spend_key = pub_key;
    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(wrong_tx_id.into(), bob_node_identity.public_key())),
        )
        .unwrap();

    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(tx_reply.into(), bob_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);

        let mut errors = 0;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::Error(s) = &*event.unwrap() {
                        if s == &"TransactionProtocolError(TransactionBuildError(InvalidSignatureError(\"Verifying kernel signature\")))".to_string()                         {
                            errors+=1;
                        }
                        if errors >= 1 {
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(errors >= 1);
    });
}

#[test]
fn finalize_tx_with_incorrect_pubkey() {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let temp_dir = tempdir().unwrap();
    let path_string = temp_dir.path().to_str().unwrap().to_string();

    let alice_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let bob_db_path = format!("{}/{}", path_string, bob_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path, 16).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();

    let mut alice_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection_alice, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let mut bob_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection_bob, None);

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);
    runtime
        .block_on(bob_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();
    let mut stp = runtime
        .block_on(
            bob_ts_interface
                .output_manager_service_handle
                .prepare_transaction_to_send(
                    TxId::new_random(),
                    MicroTari::from(5000),
                    None,
                    None,
                    MicroTari::from(25),
                    None,
                    "".to_string(),
                    script!(Nop),
                    Covenant::default(),
                ),
        )
        .unwrap();
    let msg = stp.build_single_round_message().unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg)).into(),
        bob_node_identity.public_key(),
    );

    runtime
        .block_on(alice_ts_interface.transaction_send_message_channel.send(tx_message))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let recipient_reply: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    stp.add_single_recipient_info(recipient_reply.clone(), &factories.range_proof)
        .unwrap();
    stp.finalize(KernelFeatures::empty(), &factories, None, u64::MAX)
        .unwrap();
    let tx = stp.get_transaction().unwrap();

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: recipient_reply.tx_id.as_u64(),
        transaction: Some(tx.clone().try_into().unwrap()),
    };

    runtime
        .block_on(
            alice_ts_interface
                .transaction_finalize_message_channel
                .send(create_dummy_message(
                    finalized_transaction_message,
                    &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                )),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(15));
        tokio::pin!(delay);

        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = (*event.unwrap()).clone() {
                         panic!("Should not have received finalized event!");
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transaction(recipient_reply.tx_id)
        )
        .is_err());
}

#[test]
fn finalize_tx_with_missing_output() {
    let mut runtime = create_runtime();
    let factories = CryptoFactories::default();

    let temp_dir = tempdir().unwrap();
    let path_string = temp_dir.path().to_str().unwrap().to_string();

    let alice_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let bob_db_path = format!("{}/{}", path_string, bob_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path, 16).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();

    let mut alice_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection_alice, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let mut bob_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection_bob, None);

    let (_utxo, uo) = make_input(&mut OsRng, MicroTari(250000), &factories.commitment);

    runtime
        .block_on(bob_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let mut stp = runtime
        .block_on(
            bob_ts_interface
                .output_manager_service_handle
                .prepare_transaction_to_send(
                    TxId::new_random(),
                    MicroTari::from(5000),
                    None,
                    None,
                    MicroTari::from(20),
                    None,
                    "".to_string(),
                    script!(Nop),
                    Covenant::default(),
                ),
        )
        .unwrap();
    let msg = stp.build_single_round_message().unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg)).into(),
        bob_node_identity.public_key(),
    );

    runtime
        .block_on(alice_ts_interface.transaction_send_message_channel.send(tx_message))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(10))
        .unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let recipient_reply: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    stp.add_single_recipient_info(recipient_reply.clone(), &factories.range_proof)
        .unwrap();
    stp.finalize(KernelFeatures::empty(), &factories, None, u64::MAX)
        .unwrap();

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: recipient_reply.tx_id.as_u64(),
        transaction: Some(
            Transaction::new(
                vec![],
                vec![],
                vec![],
                PrivateKey::random(&mut OsRng),
                PrivateKey::random(&mut OsRng),
            )
            .try_into()
            .unwrap(),
        ),
    };

    runtime
        .block_on(
            alice_ts_interface
                .transaction_finalize_message_channel
                .send(create_dummy_message(
                    finalized_transaction_message,
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(15));
        tokio::pin!(delay);

        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(_) = (*event.unwrap()).clone() {
                        panic!("Should not have received finalized event");
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transaction(recipient_reply.tx_id)
        )
        .is_err());
}

#[test]
fn discovery_async_return_test() {
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path();

    let mut runtime = runtime::Builder::new_current_thread()
        .enable_time()
        .thread_name("discovery_async_return_test")
        .build()
        .unwrap();
    let factories = CryptoFactories::default();

    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Carols's parameters
    let carol_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "discovery_async_return_test: Alice: '{}', Bob: '{}', Carol: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        carol_node_identity.node_id().short_str(),
    );
    let mut shutdown = Shutdown::new();

    let (carol_connection, _temp_dir1) = make_wallet_database_connection(None);

    let (_carol_ts, _carol_oms, carol_comms, _carol_connectivity) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![],
        factories.clone(),
        carol_connection,
        db_folder.join("carol"),
        Duration::from_secs(1),
        shutdown.to_signal(),
    );

    let (alice_connection, _temp_dir2) = make_wallet_database_connection(None);

    let (mut alice_ts, mut alice_oms, alice_comms, _alice_connectivity) = setup_transaction_service(
        &mut runtime,
        alice_node_identity,
        vec![carol_node_identity.clone()],
        factories.clone(),
        alice_connection,
        db_folder.join("alice"),
        Duration::from_secs(20),
        shutdown.to_signal(),
    );
    let mut alice_event_stream = alice_ts.get_event_stream();

    let (_utxo, uo1a) = make_input(&mut OsRng, MicroTari(5500), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1a, None)).unwrap();
    let (_utxo, uo1b) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1b, None)).unwrap();
    let (_utxo, uo1c) = make_input(&mut OsRng, MicroTari(3000), &factories.commitment);
    runtime.block_on(alice_oms.add_output(uo1c, None)).unwrap();

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

    let mut txid = TxId::from(0);
    let mut is_success = true;
    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);

        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionDirectSendResult(tx_id, result) = (*event.unwrap()).clone() {
                        txid = tx_id;
                        is_success = result;
                        break;
                    }
                },
                () = &mut delay => {
                    panic!("Timeout while waiting for transaction to fail sending");
                },
            }
        }
    });
    assert_eq!(txid, tx_id);
    assert!(!is_success);

    let tx_id2 = runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_a_to_c_1,
            MicroTari::from(20),
            "Discovery Tx2!".to_string(),
        ))
        .unwrap();

    let mut success_result = false;
    let mut success_tx_id = TxId::from(0);
    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);

        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionDirectSendResult(tx_id, success) = &*event.unwrap() {
                        success_result = *success;
                        success_tx_id = *tx_id;
                        break;
                    }
                },
                () = &mut delay => {
                    panic!("Timeout while waiting for transaction to successfully be sent");
                },
            }
        }
    });

    assert_eq!(success_tx_id, tx_id2);
    assert!(success_result);

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap() {
                        if tx_id == &tx_id2 {
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    panic!("Timeout while Alice was waiting for a transaction reply");
                },
            }
        }
    });

    shutdown.trigger();
    runtime.block_on(async move {
        alice_comms.wait_until_shutdown().await;
        carol_comms.wait_until_shutdown().await;
    });
}

#[test]
fn test_power_mode_updates() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let (connection, _temp_dir) = make_wallet_database_connection(None);
    let tx_backend = TransactionServiceSqliteDatabase::new(connection.clone(), None);

    let kernel = KernelBuilder::new()
        .with_excess(&factories.commitment.zero())
        .with_signature(&Signature::default())
        .build()
        .unwrap();
    let tx = Transaction::new(
        vec![],
        vec![],
        vec![kernel],
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
    );
    let completed_tx1 = CompletedTransaction {
        tx_id: 1.into(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: 5000 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
        confirmations: None,
        mined_height: None,
        mined_in_block: None,
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2.into(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: 6000 * uT,
        fee: MicroTari::from(200),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
        confirmations: None,
        mined_height: None,
        mined_in_block: None,
    };

    tx_backend
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            1.into(),
            Box::new(completed_tx1),
        )))
        .unwrap();
    tx_backend
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            2.into(),
            Box::new(completed_tx2),
        )))
        .unwrap();

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, connection, None);

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_ts_interface.base_node_identity.to_peer());

    alice_ts_interface
        .wallet_connectivity_service_mock
        .notify_base_node_set(alice_ts_interface.base_node_identity.to_peer());

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::NotStored,
            block_hash: None,
            confirmations: 0,
            is_synced: true,
            height_of_longest_chain: 10,
        });

    let result = runtime.block_on(
        alice_ts_interface
            .transaction_service_handle
            .restart_broadcast_protocols(),
    );

    assert!(result.is_ok());

    // Wait for first 4 messages
    let _ = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_transaction_query_calls(4, Duration::from_secs(20)),
        )
        .unwrap();

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.set_low_power_mode())
        .unwrap();
    // expect 4 messages more
    let _ = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_transaction_query_calls(4, Duration::from_secs(60)),
        )
        .unwrap();

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.set_normal_power_mode())
        .unwrap();
    // and 4 more
    let _ = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_transaction_query_calls(4, Duration::from_secs(60)),
        )
        .unwrap();
}

#[test]
fn test_set_num_confirmations() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories,
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );

    let num_confirmations_required = runtime
        .block_on(ts_interface.transaction_service_handle.get_num_confirmations_required())
        .unwrap();
    assert_eq!(
        num_confirmations_required,
        TransactionServiceConfig::default().num_confirmations_required
    );

    for number in 1..10 {
        runtime
            .block_on(
                ts_interface
                    .transaction_service_handle
                    .set_num_confirmations_required(number),
            )
            .unwrap();

        let num_confirmations_required = runtime
            .block_on(ts_interface.transaction_service_handle.get_num_confirmations_required())
            .unwrap();
        assert_eq!(num_confirmations_required, number);
    }
}

#[test]
#[ignore = "test is flaky"]
fn test_transaction_cancellation() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionStoreForwardSendResult(_,_) = &*event.unwrap() {
                       break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });

    for i in 0..=12 {
        match runtime
            .block_on(
                alice_ts_interface
                    .transaction_service_handle
                    .get_pending_outbound_transactions(),
            )
            .unwrap()
            .remove(&tx_id)
        {
            None => (),
            Some(_) => break,
        }
        runtime.block_on(async { sleep(Duration::from_secs(5)).await });
        if i >= 12 {
            panic!("Pending outbound transaction should have been added by now");
        }
    }

    let _ = alice_ts_interface.outbound_service_mock_state.take_calls();

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.cancel_transaction(tx_id))
        .unwrap();

    // Wait for cancellation event, in an effort to nail down where the issue is for the flakey CI test
    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut cancelled = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionCancelled(..) = &*event.unwrap() {
                       cancelled = true;
                       break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Cancelled event should have occurred");
    });
    // We expect 1 sent direct and via SAF
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("alice call wait 1");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let alice_cancel_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_cancel_message.tx_id, tx_id.as_u64(), "DIRECT");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let alice_cancel_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_cancel_message.tx_id, tx_id.as_u64(), "SAF");

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_pending_outbound_transactions()
        )
        .unwrap()
        .remove(&tx_id)
        .is_none());

    let input = create_unblinded_output(
        TariScript::default(),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(100_000),
    );

    let constants = create_consensus_constants(0);
    let mut builder = SenderTransactionProtocol::builder(1, constants);
    let amount = MicroTari::from(10_000);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input
                .as_transaction_input(&factories.commitment)
                .expect("Should be able to make transaction input"),
            input,
        )
        .with_change_secret(PrivateKey::random(&mut OsRng))
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            Default::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(script!(Nop), ExecutionStack::default(), PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories, None, u64::MAX).unwrap();
    let tx_sender_msg = stp.build_single_round_message().unwrap();
    let tx_id2 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.into());
    runtime
        .block_on(
            alice_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(proto_message, bob_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::ReceivedTransaction(_) = &*event.unwrap() {
                       break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_pending_inbound_transactions(),
        )
        .unwrap()
        .remove(&tx_id2)
        .expect("Pending Transaction 2 should be in list");

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.cancel_transaction(tx_id2))
        .unwrap();

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_pending_inbound_transactions()
        )
        .unwrap()
        .remove(&tx_id2)
        .is_none());

    // Lets cancel the last one using a Comms stack message
    let input = create_unblinded_output(
        TariScript::default(),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(100_000),
    );
    let constants = create_consensus_constants(0);
    let mut builder = SenderTransactionProtocol::builder(1, constants);
    let amount = MicroTari::from(10_000);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input
                .as_transaction_input(&factories.commitment)
                .expect("Should be able to make transaction input"),
            input,
        )
        .with_change_secret(PrivateKey::random(&mut OsRng))
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            Default::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(script!(Nop), ExecutionStack::default(), PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories, None, u64::MAX).unwrap();
    let tx_sender_msg = stp.build_single_round_message().unwrap();
    let tx_id3 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.into());
    runtime
        .block_on(
            alice_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(proto_message, bob_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::ReceivedTransaction(_) = &*event.unwrap() {
                       break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
    });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_pending_inbound_transactions(),
        )
        .unwrap()
        .remove(&tx_id3)
        .expect("Pending Transaction 3 should be in list");

    let proto_message = proto::TransactionCancelledMessage { tx_id: tx_id3.as_u64() };
    // Sent from the wrong source address so should not cancel
    runtime
        .block_on(
            alice_ts_interface
                .transaction_cancelled_message_channel
                .send(create_dummy_message(
                    proto_message,
                    &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
                )),
        )
        .unwrap();

    runtime.block_on(async { sleep(Duration::from_secs(5)).await });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_pending_inbound_transactions(),
        )
        .unwrap()
        .remove(&tx_id3)
        .expect("Pending Transaction 3 should be in list");

    let proto_message = proto::TransactionCancelledMessage { tx_id: tx_id3.as_u64() };
    runtime
        .block_on(
            alice_ts_interface
                .transaction_cancelled_message_channel
                .send(create_dummy_message(proto_message, bob_node_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30)).fuse();
        tokio::pin!(delay);
        let mut cancelled = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::TransactionCancelled(..) = &*event.unwrap() {
                       cancelled = true;
                       break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Should received cancelled event");
    });

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_pending_inbound_transactions()
        )
        .unwrap()
        .remove(&tx_id3)
        .is_none());
}
#[test]
fn test_direct_vs_saf_send_of_tx_reply_and_finalize() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection, None);

    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();

    let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

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
            panic!("Transaction is the not a single rounder sender variant");
        },
    };
    assert_eq!(tx_id, msg_tx_id);
    let (connection, _temp_dir) = make_wallet_database_connection(None);

    // Test sending the Reply to a receiver with Direct and then with SAF and never both
    let mut bob_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );

    bob_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Queued,
            broadcast: ResponseType::Failed,
        });

    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    tx_sender_msg.clone().into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let _: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime.block_on(async { sleep(Duration::from_secs(5)).await });
    assert_eq!(
        bob_ts_interface.outbound_service_mock_state.call_count(),
        0,
        "Should be no more calls"
    );
    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut bob2_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );
    bob2_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Failed,
            broadcast: ResponseType::Queued,
        });

    runtime
        .block_on(
            bob2_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    tx_sender_msg.into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();

    bob2_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    let (_, body) = bob2_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime.block_on(async { sleep(Duration::from_secs(5)).await });
    assert_eq!(
        bob2_ts_interface.outbound_service_mock_state.call_count(),
        0,
        "Should be no more calls"
    );

    // Test finalize is sent Direct Only.
    // UPDATE: both direct and SAF will be sent
    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Queued,
            broadcast: ResponseType::Queued,
        });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    tx_reply_msg.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    let _ = alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60));
    let _ = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let _ = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    runtime.block_on(async { sleep(Duration::from_secs(5)).await });
    assert_eq!(
        alice_ts_interface.outbound_service_mock_state.call_count(),
        0,
        "Should be no more calls"
    );

    // Now to repeat sending so we can test the SAF send of the finalize message
    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 20000 * uT;

    let _tx_id2 = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .unwrap();

    let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    tx_sender_msg.into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();

    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Failed,
            broadcast: ResponseType::Queued,
        });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    tx_reply_msg.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    let _ = alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60));

    assert_eq!(alice_ts_interface.outbound_service_mock_state.call_count(), 1);
    let _ = alice_ts_interface.outbound_service_mock_state.pop_call();
    runtime.block_on(async { sleep(Duration::from_secs(5)).await });
    assert_eq!(
        alice_ts_interface.outbound_service_mock_state.call_count(),
        0,
        "Should be no more calls2"
    );
}

#[test]
fn test_tx_direct_send_behaviour() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let (_utxo, uo) = make_input(&mut OsRng, 1000000 * uT, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();
    let (_utxo, uo) = make_input(&mut OsRng, 1000000 * uT, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();
    let (_utxo, uo) = make_input(&mut OsRng, 1000000 * uT, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();
    let (_utxo, uo) = make_input(&mut OsRng, 1000000 * uT, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 10000 * uT;

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Failed,
            broadcast: ResponseType::Failed,
        });

    let _tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message1".to_string(),
        ))
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut direct_count = 0;
        let mut saf_count = 0;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionDirectSendResult(_, result) => if !result { direct_count+=1 },
                        TransactionEvent::TransactionStoreForwardSendResult(_, result) => if !result { saf_count+=1},
                        _ => (),
                    }

                    if direct_count == 1 && saf_count == 1 {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(direct_count, 1, "Should be 1 failed direct");
        assert_eq!(saf_count, 1, "Should be 1 failed saf");
    });

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::QueuedFail,
            broadcast: ResponseType::Queued,
        });

    let _tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message2".to_string(),
        ))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut direct_count = 0;
        let mut saf_count = 0;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionDirectSendResult(_, result) => if !result { direct_count+=1 },
                        TransactionEvent::TransactionStoreForwardSendResult(_, result) => if *result { saf_count+=1 },
                        _ => (),
                    }

                    if direct_count == 1 && saf_count == 1 {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(direct_count, 1, "Should be 1 failed direct");
        assert_eq!(saf_count, 1, "Should be 1 succeeded saf");
    });

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(1)),
            broadcast: ResponseType::Queued,
        });

    let _tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message3".to_string(),
        ))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut direct_count = 0;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionDirectSendResult(_, result) => if *result { direct_count+=1 },
                        TransactionEvent::TransactionStoreForwardSendResult(_, _) => panic!("Should be no SAF messages"),
                        _ => (),
                    }

                    if direct_count >= 1  {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(direct_count, 1, "Should be 1 succeeded direct");
    });

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(30)),
            broadcast: ResponseType::Queued,
        });

    let _tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message4".to_string(),
        ))
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
tokio::pin!(delay);
        let mut saf_count = 0;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionStoreForwardSendResult(_, result) => if *result { saf_count+=1},
                        TransactionEvent::TransactionDirectSendResult(_, result) => if *result { panic!("Should be no direct messages") },                         _ => (),
                    }

                    if saf_count >= 1  {
                        break;
                    }
                },
                () = &mut delay => {
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
    let (alice_connection, _temp_dir) = make_wallet_database_connection(None);
    let alice_backend = TransactionServiceSqliteDatabase::new(alice_connection.clone(), None);

    let (bob_connection, _temp_dir2) = make_wallet_database_connection(None);
    let bob_backend = TransactionServiceSqliteDatabase::new(bob_connection.clone(), None);

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let alice_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let bob_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob is going to send a transaction to Alice
    let alice = TestParams::new(&mut OsRng);
    let bob = TestParams::new(&mut OsRng);
    let (utxo, input) = make_input(&mut OsRng, MicroTari(2000), &factories.commitment);
    let constants = create_consensus_constants(0);
    let fee_calc = Fee::new(*constants.transaction_weight());
    let mut builder = SenderTransactionProtocol::builder(1, constants);
    let fee = fee_calc.calculate(MicroTari(4), 1, 1, 1, 0);
    let script_private_key = PrivateKey::random(&mut OsRng);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari(4))
        .with_offset(bob.offset.clone())
        .with_private_nonce(bob.nonce)
        .with_input(utxo, input)
        .with_amount(0, MicroTari(2000) - fee - MicroTari(10))
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            Default::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(
            script!(Nop),
            inputs!(PublicKey::from_secret_key(&script_private_key)),
            script_private_key,
        );
    let mut bob_stp = builder.build::<Blake256>(&factories, None, u64::MAX).unwrap();
    let msg = bob_stp.build_single_round_message().unwrap();
    let bob_pre_finalize = bob_stp.clone();

    let tx_id = msg.tx_id;

    let sender_info = TransactionSenderMessage::Single(Box::new(msg.clone()));
    let receiver_protocol =
        ReceiverTransactionProtocol::new(sender_info, alice.nonce.clone(), alice.spend_key, &factories);

    let alice_reply = receiver_protocol.get_signed_data().unwrap().clone();

    bob_stp
        .add_single_recipient_info(alice_reply.clone(), &factories.range_proof)
        .unwrap();

    match bob_stp.finalize(KernelFeatures::empty(), &factories, None, u64::MAX) {
        Ok(_) => (),
        Err(e) => panic!("Should be able to finalize tx: {}", e),
    };
    let tx = bob_stp.get_transaction().unwrap().clone();

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
    let mut bob_ts_interface =
        setup_transaction_service_no_comms(&mut runtime, factories.clone(), bob_connection, None);
    let mut bob_event_stream = bob_ts_interface.transaction_service_handle.get_event_stream();

    bob_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(base_node_identity.to_peer());
    assert!(runtime
        .block_on(
            bob_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());

    runtime
        .block_on(
            bob_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(alice_reply.into(), alice_identity.public_key())),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(15));
        tokio::pin!(delay);
        let mut received_reply = false;
        loop {
            tokio::select! {
                event = bob_event_stream.recv() => {
                     if let TransactionEvent::ReceivedTransactionReply(id) = (*event.unwrap()).clone() {
                        assert_eq!(id, tx_id);
                        received_reply = true;
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(received_reply, "Should have received tx reply");
    });

    // Test Alice's node restarts the receive protocol
    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, alice_connection, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(base_node_identity.to_peer());

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: tx_id.as_u64(),
        transaction: Some(tx.try_into().unwrap()),
    };

    runtime
        .block_on(
            alice_ts_interface
                .transaction_finalize_message_channel
                .send(create_dummy_message(
                    finalized_transaction_message,
                    bob_identity.public_key(),
                )),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(15));
        tokio::pin!(delay);
        let mut received_finalized = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                     if let TransactionEvent::ReceivedFinalizedTransaction(id) = (*event.unwrap()).clone() {
                        assert_eq!(id, tx_id);
                        received_finalized = true;
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(received_finalized, "Should have received finalized tx");
    });
}

#[test]
fn test_coinbase_transactions_rejection_same_height() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, connection, None);

    let block_height_a = 10;
    let block_height_b = block_height_a + 1;

    let fees1 = 1000 * uT;
    let reward1 = 1_000_000 * uT;

    let fees2 = 2000 * uT;
    let reward2 = 2_000_000 * uT;

    let fees3 = 4000 * uT;
    let reward3 = 4_000_000 * uT;

    // Create a coinbase Txn at the first block height
    let _tx1 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward1, fees1, block_height_a),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 1);
    let _tx_id1 = transactions
        .values()
        .find(|tx| tx.amount == fees1 + reward1)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1
    );

    // Create another coinbase Txn at the same block height; the previous one will be cancelled
    let _tx2 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward2, fees2, block_height_a),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap(); // Only one valid coinbase txn remains
    assert_eq!(transactions.len(), 1);
    let _tx_id2 = transactions
        .values()
        .find(|tx| tx.amount == fees2 + reward2)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees2 + reward2
    );

    // Create a third coinbase Txn at the second block height; only the last two will be valid
    let _tx3 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward3, fees3, block_height_b),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 2);
    let _tx_id3 = transactions
        .values()
        .find(|tx| tx.amount == fees3 + reward3)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees2 + reward2 + fees3 + reward3
    );

    assert!(!transactions.values().any(|tx| tx.amount == fees1 + reward1));
    assert!(transactions.values().any(|tx| tx.amount == fees2 + reward2));
    assert!(transactions.values().any(|tx| tx.amount == fees3 + reward3));
}

#[test]
fn test_coinbase_generation_and_monitoring() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (connection, _temp_dir) = make_wallet_database_connection(None);
    let tx_backend = TransactionServiceSqliteDatabase::new(connection.clone(), None);
    let db = TransactionDatabase::new(tx_backend);
    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, connection, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();
    alice_ts_interface
        .base_node_rpc_mock_state
        .set_response_delay(Some(Duration::from_secs(1)));

    let block_height_a = 10;
    let block_height_b = block_height_a + 1;

    let fees1 = 1000 * uT;
    let reward1 = 1_000_000 * uT;

    let fees2 = 2000 * uT;
    let fees2b = 5000 * uT;
    let reward2 = 2_000_000 * uT;

    // Create a coinbase Txn at the first block height
    let _tx1 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward1, fees1, block_height_a),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 1);
    let tx_id1 = transactions
        .values()
        .find(|tx| tx.amount == fees1 + reward1)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1
    );

    // Create another coinbase Txn at the next block height
    let _tx2 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward2, fees2, block_height_b),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 2);
    let tx_id2 = transactions
        .values()
        .find(|tx| tx.amount == fees2 + reward2)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1 + fees2 + reward2
    );

    // Take out a second one at the second height which should overwrite the initial one
    let _tx2b = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward2, fees2b, block_height_b),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 2);
    let tx_id2b = transactions
        .values()
        .find(|tx| tx.amount == fees2b + reward2)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1 + fees2b + reward2
    );

    assert!(transactions.values().any(|tx| tx.amount == fees1 + reward1));
    assert!(transactions.values().any(|tx| tx.amount == fees2b + reward2));

    // Start the transaction protocols
    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_ts_interface.base_node_identity.to_peer());

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);
        let mut count = 0usize;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    if let TransactionEvent::ReceivedFinalizedTransaction(tx_id) = &*event.unwrap() {
                        if tx_id == &tx_id1 || tx_id == &tx_id2 || tx_id == &tx_id2b {
                            count += 1;
                        }
                        if count == 3 {
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert_eq!(
            count, 3,
            "Expected exactly two 'ReceivedFinalizedTransaction(_)' events"
        );
    });

    // Now we will test validation where tx1 will not be found but tx2b will be unconfirmed, then confirmed.
    let tx1 = runtime.block_on(db.get_completed_transaction(tx_id1)).unwrap();
    let tx2b = runtime.block_on(db.get_completed_transaction(tx_id2b)).unwrap();

    let mut block_headers = HashMap::new();
    for i in 0..=4 {
        let mut block_header = BlockHeader::new(1);
        block_header.height = i;
        block_headers.insert(i, block_header.clone());
    }
    alice_ts_interface
        .base_node_rpc_mock_state
        .set_blocks(block_headers.clone());
    let mut transaction_query_batch_responses = vec![
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx1.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(
                tx2b.transaction.first_kernel_excess_sig().unwrap().clone(),
            )),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&1).unwrap().hash()),
            confirmations: 0,
            block_height: 1,
        },
    ];
    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses.clone(),
        is_synced: true,
        tip_hash: Some(block_headers.get(&1).unwrap().hash()),
        height_of_longest_chain: 1,
    };

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_batch_responses(batch_query_response);

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_ts_interface.base_node_identity.to_peer());

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.validate_transactions())
        .expect("Validation should start");

    let _tx_batch_query_calls = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(30)),
        )
        .unwrap();

    let completed_txs = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();

    assert_eq!(completed_txs.len(), 2);

    let tx = completed_txs.get(&tx_id1).unwrap();
    assert_eq!(tx.status, TransactionStatus::Coinbase);

    let tx = completed_txs.get(&tx_id2b).unwrap();
    assert_eq!(tx.status, TransactionStatus::MinedUnconfirmed);

    // Now we will have tx_id2b becoming confirmed
    let _ = transaction_query_batch_responses.pop();
    transaction_query_batch_responses.push(TxQueryBatchResponseProto {
        signature: Some(SignatureProto::from(
            tx2b.transaction.first_kernel_excess_sig().unwrap().clone(),
        )),
        location: TxLocationProto::from(TxLocation::Mined) as i32,
        block_hash: Some(block_headers.get(&4).unwrap().hash()),
        confirmations: 3,
        block_height: 4,
    });

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses,
        is_synced: true,
        tip_hash: Some(block_headers.get(&4).unwrap().hash()),
        height_of_longest_chain: 4,
    };
    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_batch_responses(batch_query_response);

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.validate_transactions())
        .expect("Validation should start");

    let _tx_batch_query_calls = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_transaction_batch_query_calls(1, Duration::from_secs(30)),
        )
        .unwrap();

    let completed_txs = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();

    let tx = completed_txs.get(&tx_id2b).unwrap();
    assert_eq!(tx.status, TransactionStatus::MinedConfirmed);
}

#[test]
fn test_coinbase_abandoned() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, connection, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let block_height_a = 10;

    // First we create un unmined coinbase and then abandon it
    let fees1 = 1000 * uT;
    let reward1 = 1_000_000 * uT;

    let tx1 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward1, fees1, block_height_a),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 1);
    let tx_id1 = transactions
        .values()
        .find(|tx| tx.amount == fees1 + reward1)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1
    );

    let transaction_query_batch_responses = vec![TxQueryBatchResponseProto {
        signature: Some(SignatureProto::from(tx1.first_kernel_excess_sig().unwrap().clone())),
        location: TxLocationProto::from(TxLocation::InMempool) as i32,
        block_hash: None,
        confirmations: 0,
        block_height: 0,
    }];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses,
        is_synced: true,
        tip_hash: Some([5u8; 16].to_vec()),
        height_of_longest_chain: block_height_a + TransactionServiceConfig::default().num_confirmations_required + 1,
    };

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_batch_responses(batch_query_response);

    // Start the transaction protocols
    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_ts_interface.base_node_identity.to_peer());

    let balance = runtime
        .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
        .unwrap();
    assert_eq!(balance.pending_incoming_balance, fees1 + reward1);

    let validation_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.validate_transactions())
        .expect("Validation should start");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);
        let mut cancelled = false;
        let mut completed = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionValidationCompleted(id) => {
                            if id == &validation_id  {
                                completed = true;
                            }
                        },
                        TransactionEvent::TransactionCancelled(tx_id, _) => {
                             if tx_id == &tx_id1  {
                                cancelled = true;
                            }
                        },
                        _ => (),
                    }

                    if cancelled && completed {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Expected a TransactionCancelled event");
        assert!(completed, "Expected a TransactionValidationCompleted event");
    });

    let txs = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_cancelled_completed_transactions(),
        )
        .unwrap();
    assert!(txs.get(&tx_id1).is_some());

    let balance = runtime
        .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
        .unwrap();
    assert_eq!(balance, Balance {
        available_balance: MicroTari(0),
        time_locked_balance: Some(MicroTari(0)),
        pending_incoming_balance: MicroTari(0),
        pending_outgoing_balance: MicroTari(0)
    });

    let invalid_txs = runtime
        .block_on(alice_ts_interface.output_manager_service_handle.get_invalid_outputs())
        .unwrap();
    assert!(invalid_txs.is_empty());

    // Now we will make a coinbase that will be mined, reorged out and then reorged back in
    let fees2 = 2000 * uT;
    let reward2 = 2_000_000 * uT;
    let block_height_b = 11;

    let tx2 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward2, fees2, block_height_b),
        )
        .unwrap();
    let transactions = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap();
    assert_eq!(transactions.len(), 1);
    let tx_id2 = transactions
        .values()
        .find(|tx| tx.amount == fees2 + reward2)
        .unwrap()
        .tx_id;
    assert_eq!(
        runtime
            .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees2 + reward2
    );

    let transaction_query_batch_responses = vec![
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(tx1.first_kernel_excess_sig().unwrap().clone())),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(tx2.first_kernel_excess_sig().unwrap().clone())),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some([11u8; 16].to_vec()),
            confirmations: 2,
            block_height: block_height_b,
        },
    ];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses,
        is_synced: true,
        tip_hash: Some([13u8; 16].to_vec()),
        height_of_longest_chain: block_height_b + 2,
    };

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_batch_responses(batch_query_response);

    let mut block_headers = HashMap::new();
    for i in 0..=(block_height_b + 2) {
        let mut block_header = BlockHeader::new(1);
        block_header.height = i;
        block_headers.insert(i, block_header.clone());
    }
    alice_ts_interface.base_node_rpc_mock_state.set_blocks(block_headers);

    let validation_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.validate_transactions())
        .expect("Validation should start");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);
        let mut completed = false;
        let mut mined_unconfirmed = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionValidationCompleted(id) => {
                            if id == &validation_id  {
                                completed = true;
                            }
                        },
                        TransactionEvent::TransactionMinedUnconfirmed{tx_id, num_confirmations:_, is_valid: _} => {
                             if tx_id == &tx_id2  {
                                mined_unconfirmed = true;
                            }
                        },
                        _ => (),
                    }

                    if mined_unconfirmed && completed {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(mined_unconfirmed, "Expected a TransactionMinedUnconfirmed event");
        assert!(completed, "Expected a TransactionValidationCompleted event");
    });

    let tx = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transaction(tx_id2),
        )
        .unwrap();
    assert_eq!(tx.status, TransactionStatus::MinedUnconfirmed);

    // Now we create a reorg
    let transaction_query_batch_responses = vec![
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(tx1.first_kernel_excess_sig().unwrap().clone())),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(tx2.first_kernel_excess_sig().unwrap().clone())),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
    ];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses,
        is_synced: true,
        tip_hash: Some([12u8; 16].to_vec()),
        height_of_longest_chain: block_height_b + TransactionServiceConfig::default().num_confirmations_required + 1,
    };

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_batch_responses(batch_query_response);

    let mut block_headers = HashMap::new();
    for i in 0..=(block_height_b + TransactionServiceConfig::default().num_confirmations_required + 1) {
        let mut block_header = BlockHeader::new(2);
        block_header.height = i;
        block_headers.insert(i, block_header.clone());
    }
    alice_ts_interface.base_node_rpc_mock_state.set_blocks(block_headers);

    let validation_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.validate_transactions())
        .expect("Validation should start");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(30));
        tokio::pin!(delay);
        let mut completed = false;
        let mut broadcast = false;
        let mut cancelled = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionBroadcast(tx_id) => {
                            if tx_id == &tx_id2  {
                               broadcast = true;
                            }
                        },
                        TransactionEvent::TransactionCancelled(tx_id, _) => {
                             if tx_id == &tx_id2  {
                                cancelled = true;
                            }
                        },
                        TransactionEvent::TransactionValidationCompleted(id) => {
                            if id == &validation_id  {
                                completed = true;
                            }
                        },
                        _ => (),
                    }

                    if cancelled && broadcast && completed {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(cancelled, "Expected a TransactionCancelled event");
        assert!(broadcast, "Expected a TransactionBroadcast event");
        assert!(completed, "Expected a TransactionValidationCompleted event");
    });

    let txs = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_cancelled_completed_transactions(),
        )
        .unwrap();

    assert!(txs.get(&tx_id1).is_some());
    assert!(txs.get(&tx_id2).is_some());

    let balance = runtime
        .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
        .unwrap();
    assert_eq!(balance, Balance {
        available_balance: MicroTari(0),
        time_locked_balance: Some(MicroTari(0)),
        pending_incoming_balance: MicroTari(0),
        pending_outgoing_balance: MicroTari(0)
    });

    // Now reorg again and have tx2 be mined
    let mut block_headers = HashMap::new();
    for i in 0..=15 {
        let mut block_header = BlockHeader::new(1);
        block_header.height = i;
        block_headers.insert(i, block_header.clone());
    }
    alice_ts_interface
        .base_node_rpc_mock_state
        .set_blocks(block_headers.clone());

    let transaction_query_batch_responses = vec![
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(tx1.first_kernel_excess_sig().unwrap().clone())),
            location: TxLocationProto::from(TxLocation::NotStored) as i32,
            block_hash: None,
            confirmations: 0,
            block_height: 0,
        },
        TxQueryBatchResponseProto {
            signature: Some(SignatureProto::from(tx2.first_kernel_excess_sig().unwrap().clone())),
            location: TxLocationProto::from(TxLocation::Mined) as i32,
            block_hash: Some(block_headers.get(&10).unwrap().hash()),
            confirmations: 5,
            block_height: 10,
        },
    ];

    let batch_query_response = TxQueryBatchResponsesProto {
        responses: transaction_query_batch_responses,
        is_synced: true,
        tip_hash: Some([20u8; 16].to_vec()),
        height_of_longest_chain: 20,
    };

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_batch_responses(batch_query_response);

    let validation_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.validate_transactions())
        .expect("Validation should start");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut mined = false;
        let mut cancelled = false;
        let mut completed = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::TransactionMined { tx_id, is_valid: _ }  => {
                            if tx_id == &tx_id2  {
                                mined = true;
                            }
                        },
                        TransactionEvent::TransactionCancelled(tx_id, _) => {
                             if tx_id == &tx_id1  {
                                cancelled = true;
                            }
                        },
                        TransactionEvent::TransactionValidationCompleted(id) => {
                            if id == &validation_id  {
                                completed = true;
                            }
                        },
                        _ => (),
                    }

                    if mined && cancelled && completed {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(mined, "Expected to received TransactionMined event");
        assert!(cancelled, "Expected to received TransactionCancelled event");
        assert!(completed, "Expected a TransactionValidationCompleted event");
    });
}

#[test]
fn test_coinbase_transaction_reused_for_same_height() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, connection, None);

    let blockheight1 = 10;
    let fees1 = 2000 * uT;
    let reward1 = 1_000_000 * uT;

    let blockheight2 = 11;
    let fees2 = 3000 * uT;
    let reward2 = 2_000_000 * uT;

    // a requested coinbase transaction for the same height and amount should be the same
    let tx1 = runtime
        .block_on(
            ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward1, fees1, blockheight1),
        )
        .unwrap();

    let tx2 = runtime
        .block_on(
            ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward1, fees1, blockheight1),
        )
        .unwrap();

    assert_eq!(tx1, tx2);
    let transactions = runtime
        .block_on(ts_interface.transaction_service_handle.get_completed_transactions())
        .unwrap();

    assert_eq!(transactions.len(), 1);
    for tx in transactions.values() {
        assert_eq!(tx.amount, fees1 + reward1);
    }
    assert_eq!(
        runtime
            .block_on(ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees1 + reward1
    );

    // a requested coinbase transaction for the same height but new amount should be different
    let tx3 = runtime
        .block_on(
            ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward2, fees2, blockheight1),
        )
        .unwrap();

    assert_ne!(tx3, tx1);
    let transactions = runtime
        .block_on(ts_interface.transaction_service_handle.get_completed_transactions())
        .unwrap();
    assert_eq!(transactions.len(), 1); // tx1 and tx2 should be cancelled
    for tx in transactions.values() {
        assert_eq!(tx.amount, fees2 + reward2);
    }
    assert_eq!(
        runtime
            .block_on(ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        fees2 + reward2
    );

    // a requested coinbase transaction for a new height should be different
    let tx_height2 = runtime
        .block_on(
            ts_interface
                .transaction_service_handle
                .generate_coinbase_transaction(reward2, fees2, blockheight2),
        )
        .unwrap();

    assert_ne!(tx1, tx_height2);
    let transactions = runtime
        .block_on(ts_interface.transaction_service_handle.get_completed_transactions())
        .unwrap();
    assert_eq!(transactions.len(), 2);
    for tx in transactions.values() {
        assert_eq!(tx.amount, fees2 + reward2);
    }
    assert_eq!(
        runtime
            .block_on(ts_interface.output_manager_service_handle.get_balance())
            .unwrap()
            .pending_incoming_balance,
        2 * (fees2 + reward2)
    );
}

#[test]
fn test_transaction_resending() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // Setup Alice wallet with no comms stack
    let (connection, _tempdir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(20),
            resend_response_cooldown: Duration::from_secs(10),
            ..Default::default()
        }),
    );

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    // Check that there were repeats
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Alice call wait 1");

    let mut alice_sender_message = TransactionSenderMessage::None;
    for _ in 0..2 {
        let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
        alice_sender_message = try_decode_sender_message(call.1.to_vec().clone()).unwrap();
        if let TransactionSenderMessage::Single(data) = alice_sender_message.clone() {
            assert_eq!(data.tx_id, tx_id);
        } else {
            panic!("Should be a Single Transaction Sender Message")
        }
    }

    // Setup Bob's wallet with no comms stack
    let (connection, _tempdir) = make_wallet_database_connection(None);

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories,
        connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(20),
            resend_response_cooldown: Duration::from_secs(10),
            ..Default::default()
        }),
    );

    // Pass sender message to Bob's wallet
    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    alice_sender_message.clone().into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();

    // Check that the reply was repeated
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Bob call wait 1");

    let mut bob_reply_message;
    for _ in 0..2 {
        let call = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
        bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec().clone()).unwrap();
        assert_eq!(bob_reply_message.tx_id, tx_id);
    }

    runtime.block_on(async { sleep(Duration::from_secs(2)).await });
    // See if sending a second message too soon is ignored
    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    alice_sender_message.clone().into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();

    assert!(bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(2))
        .is_err());

    // Wait for the cooldown to expire but before the resend period has elapsed see if a repeat illicits a response.
    runtime.block_on(async { sleep(Duration::from_secs(8)).await });
    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    alice_sender_message.into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Bob call wait 2");
    let _ = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let call = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_reply_message.tx_id, tx_id);

    let _ = alice_ts_interface.outbound_service_mock_state.take_calls();

    // Send the reply to Alice
    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    bob_reply_message.clone().into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Alice call wait 2");

    let _ = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let alice_finalize_message = try_decode_finalized_transaction_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_finalize_message.tx_id, tx_id.as_u64());

    // See if sending a second message before cooldown and see if it is ignored
    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    bob_reply_message.clone().into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    assert!(alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(5))
        .is_err());

    // Wait for the cooldown to expire but before the resend period has elapsed see if a repeat illicts a response.
    runtime.block_on(async { sleep(Duration::from_secs(6)).await });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    bob_reply_message.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 3");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let alice_finalize_message = try_decode_finalized_transaction_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_finalize_message.tx_id, tx_id);
}

#[test]
fn test_resend_on_startup() {
    // Test that messages are resent on startup if enough time has passed
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    // First we will check the Send Tranasction message
    let input = create_unblinded_output(
        script!(Nop),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(100_000),
    );
    let constants = create_consensus_constants(0);
    let mut builder = SenderTransactionProtocol::builder(1, constants);
    let amount = MicroTari::from(10_000);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177 / 5))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input
                .as_transaction_input(&factories.commitment)
                .expect("Should be able to make transaction input"),
            input,
        )
        .with_change_secret(PrivateKey::random(&mut OsRng))
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            Default::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(script!(Nop), ExecutionStack::default(), PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories, None, u64::MAX).unwrap();
    let stp_msg = stp.build_single_round_message().unwrap();
    let tx_sender_msg = TransactionSenderMessage::Single(Box::new(stp_msg));

    let tx_id = stp.get_tx_id().unwrap();
    let mut outbound_tx = OutboundTransaction {
        tx_id,
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount,
        fee: stp.get_fee_amount().unwrap(),
        sender_protocol: stp,
        status: TransactionStatus::Pending,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: false,
        direct_send_success: false,
        send_count: 1,
        last_send_timestamp: Some(Utc::now().naive_utc()),
    };
    let (connection, _temp_dir) = make_wallet_database_connection(None);
    let alice_backend = TransactionServiceSqliteDatabase::new(connection.clone(), None);
    alice_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx.clone()),
        )))
        .unwrap();

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for alices base node, doesn't matter what
    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_node_identity.to_peer());

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_ok());
    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());

    // Check that if the cooldown is not done that a message will not be sent.
    assert!(alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(5))
        .is_err());
    drop(alice_ts_interface);

    // Now we do it again with the timestamp prior to the cooldown and see that a message is sent
    outbound_tx.send_count = 1;
    outbound_tx.last_send_timestamp = Utc::now().naive_utc().checked_sub_signed(ChronoDuration::seconds(20));

    let (connection2, _temp_dir2) = make_wallet_database_connection(None);
    let alice_backend2 = TransactionServiceSqliteDatabase::new(connection2.clone(), None);

    alice_backend2
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx),
        )))
        .unwrap();

    let mut alice2_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        connection2,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for alices base node, doesn't matter what
    alice2_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_node_identity.to_peer());

    assert!(runtime
        .block_on(
            alice2_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_ok());
    assert!(runtime
        .block_on(
            alice2_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());

    // Check for resend on startup
    alice2_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Carol call wait 1");

    let call = alice2_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    if let TransactionSenderMessage::Single(data) = try_decode_sender_message(call.1.to_vec()).unwrap() {
        assert_eq!(data.tx_id, tx_id);
    } else {
        panic!("Should be a Single Transaction Sender Message")
    }

    // Now we do this for the Transaction Reply

    let rtp = ReceiverTransactionProtocol::new(
        tx_sender_msg,
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
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
    let (bob_connection, _temp_dir) = make_wallet_database_connection(None);
    let bob_backend = TransactionServiceSqliteDatabase::new(bob_connection.clone(), None);

    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx.clone()),
        )))
        .unwrap();

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        bob_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for bobs base node, doesn't matter what
    bob_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_node_identity.to_peer());

    assert!(runtime
        .block_on(
            bob_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_ok());
    assert!(runtime
        .block_on(
            bob_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());

    // Check that if the cooldown is not done that a message will not be sent.
    assert!(bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(5))
        .is_err());

    drop(bob_ts_interface);

    // Now we do it again with the timestamp prior to the cooldown and see that a message is sent
    inbound_tx.send_count = 1;
    inbound_tx.last_send_timestamp = Utc::now().naive_utc().checked_sub_signed(ChronoDuration::seconds(20));
    let (bob_connection2, _temp_dir2) = make_wallet_database_connection(None);
    let bob_backend2 = TransactionServiceSqliteDatabase::new(bob_connection2.clone(), None);
    bob_backend2
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx),
        )))
        .unwrap();

    let mut bob2_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories,
        bob_connection2,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    );

    // Need to set something for bobs base node, doesn't matter what
    bob2_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_node_identity.to_peer());

    assert!(runtime
        .block_on(
            bob2_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_ok());
    assert!(runtime
        .block_on(
            bob2_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());
    // Check for resend on startup

    bob2_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Dave call wait 1");

    let call = bob2_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let reply = try_decode_transaction_reply_message(call.1.to_vec()).unwrap();
    assert_eq!(reply.tx_id, tx_id);
}

#[test]
fn test_replying_to_cancelled_tx() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // Testing if a Tx Reply is received for a Cancelled Outbound Tx that a Cancelled message is sent back:
    let (alice_connection, _tempdir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        alice_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    );

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 1");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let alice_sender_message = try_decode_sender_message(call.1.to_vec()).unwrap();
    if let TransactionSenderMessage::Single(data) = alice_sender_message.clone() {
        assert_eq!(data.tx_id, tx_id);
    }
    // Need a moment for Alice's wallet to finish writing to its database before cancelling
    runtime.block_on(async { sleep(Duration::from_secs(5)).await });

    runtime
        .block_on(alice_ts_interface.transaction_service_handle.cancel_transaction(tx_id))
        .unwrap();

    // Setup Bob's wallet with no comms stack
    let (bob_connection, _tempdir) = make_wallet_database_connection(None);

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories,
        bob_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );

    // Pass sender message to Bob's wallet
    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    alice_sender_message.into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Bob call wait 1");

    let call = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_reply_message.tx_id, tx_id);

    // Wait for cooldown to expire
    runtime.block_on(async { sleep(Duration::from_secs(5)).await });

    let _ = alice_ts_interface.outbound_service_mock_state.take_calls();

    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    bob_reply_message.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .expect("Alice call wait 2");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let alice_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_cancelled_message.tx_id, tx_id.as_u64());
}

#[test]
fn test_transaction_timeout_cancellation() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // Testing if a Tx Reply is received for a Cancelled Outbound Tx that a Cancelled message is sent back:
    let (alice_connection, _tempdir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        alice_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let (_utxo, uo) = make_input(&mut OsRng, alice_total_available, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let amount_sent = 10000 * uT;

    let tx_id = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent,
            20 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();

    // For testing the resend period is set to 10 seconds and the timeout period is set to 15 seconds so we are going
    // to wait for 3 messages The intial send, the resend and then the cancellation
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(5, Duration::from_secs(60))
        .expect("Alice call wait 1");

    let calls = alice_ts_interface.outbound_service_mock_state.take_calls();

    // First call

    let sender_message = try_decode_sender_message(calls[0].1.to_vec()).unwrap();
    if let TransactionSenderMessage::Single(data) = sender_message {
        assert_eq!(data.tx_id, tx_id);
    } else {
        panic!("Should be a Single Transaction Sender Message")
    }
    // Resend
    let sender_message = try_decode_sender_message(calls[2].1.to_vec()).unwrap();
    if let TransactionSenderMessage::Single(data) = sender_message {
        assert_eq!(data.tx_id, tx_id);
    } else {
        panic!("Should be a Single Transaction Sender Message")
    }

    // Timeout Cancellation
    let alice_cancelled_message = try_decode_transaction_cancelled_message(calls[4].1.to_vec()).unwrap();
    assert_eq!(alice_cancelled_message.tx_id, tx_id.as_u64());

    // Now to test if the timeout has elapsed during downtime and that it is honoured on startup
    // First we will check the Send Transction message
    let input = create_unblinded_output(
        TariScript::default(),
        OutputFeatures::default(),
        TestParamsHelpers::new(),
        MicroTari::from(100_000),
    );
    let constants = create_consensus_constants(0);
    let mut builder = SenderTransactionProtocol::builder(1, constants);
    let amount = MicroTari::from(10_000);
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroTari::from(177 / 5))
        .with_offset(PrivateKey::random(&mut OsRng))
        .with_private_nonce(PrivateKey::random(&mut OsRng))
        .with_amount(0, amount)
        .with_message("Yo!".to_string())
        .with_input(
            input
                .as_transaction_input(&factories.commitment)
                .expect("Should be able to make transaction input"),
            input,
        )
        .with_change_secret(PrivateKey::random(&mut OsRng))
        .with_recipient_data(
            0,
            script!(Nop),
            PrivateKey::random(&mut OsRng),
            Default::default(),
            PrivateKey::random(&mut OsRng),
            Covenant::default(),
        )
        .with_change_script(script!(Nop), ExecutionStack::default(), PrivateKey::random(&mut OsRng));

    let mut stp = builder.build::<HashDigest>(&factories, None, u64::MAX).unwrap();
    let stp_msg = stp.build_single_round_message().unwrap();
    let tx_sender_msg = TransactionSenderMessage::Single(Box::new(stp_msg));

    let tx_id = stp.get_tx_id().unwrap();
    let outbound_tx = OutboundTransaction {
        tx_id,
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount,
        fee: stp.get_fee_amount().unwrap(),
        sender_protocol: stp,
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
    let (bob_connection, _temp_dir) = make_wallet_database_connection(None);
    let bob_backend = TransactionServiceSqliteDatabase::new(bob_connection.clone(), None);
    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx),
        )))
        .unwrap();

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories.clone(),
        bob_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );

    // Need to set something for bobs base node, doesn't matter what
    bob_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(bob_node_identity.to_peer());
    assert!(runtime
        .block_on(
            bob_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_ok());
    assert!(runtime
        .block_on(
            bob_ts_interface
                .transaction_service_handle
                .restart_transaction_protocols()
        )
        .is_ok());

    // Make sure we receive this before the timeout as it should be sent immediately on startup
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(14))
        .expect("Bob call wait 1");
    let call = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let bob_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_cancelled_message.tx_id, tx_id.as_u64());

    let call = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let bob_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_cancelled_message.tx_id, tx_id.as_u64());
    let (carol_connection, _temp) = make_wallet_database_connection(None);

    // Now to do this for the Receiver
    let mut carol_ts_interface = setup_transaction_service_no_comms(
        &mut runtime,
        factories,
        carol_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    );
    let mut carol_event_stream = carol_ts_interface.transaction_service_handle.get_event_stream();

    runtime
        .block_on(
            carol_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    tx_sender_msg.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    // Then we should get 2 reply messages and 1 cancellation event
    carol_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Carol call wait 1");

    let calls = carol_ts_interface.outbound_service_mock_state.take_calls();

    // Initial Reply
    let carol_reply_message = try_decode_transaction_reply_message(calls[0].1.to_vec()).unwrap();
    assert_eq!(carol_reply_message.tx_id, tx_id);

    // Resend
    let carol_reply_message = try_decode_transaction_reply_message(calls[1].1.to_vec()).unwrap();
    assert_eq!(carol_reply_message.tx_id, tx_id);

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut transaction_cancelled = false;
        loop {
            tokio::select! {
                event = carol_event_stream.recv() => {
                     if let TransactionEvent::TransactionCancelled(t, _) = &*event.unwrap() {
                        if t == &tx_id {
                            transaction_cancelled = true;
                            break;
                        }
                     }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(transaction_cancelled, "Transaction must be cancelled");
    });
}

/// This test will check that the Transaction Service starts the tx broadcast protocol correctly and reacts correctly
/// to a tx being broadcast and to a tx being rejected.
#[test]
fn transaction_service_tx_broadcast() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection, None);
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_ts_interface.base_node_identity.to_peer());

    let (connection2, _temp_dir2) = make_wallet_database_connection(None);
    let mut bob_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection2, None);

    let alice_output_value = MicroTari(250000);

    let (_utxo, uo) = make_input(&mut OsRng, alice_output_value, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo, None))
        .unwrap();

    let (_utxo, uo2) = make_input(&mut OsRng, alice_output_value, &factories.commitment);
    runtime
        .block_on(alice_ts_interface.output_manager_service_handle.add_output(uo2, None))
        .unwrap();

    let amount_sent1 = 10000 * uT;

    // Send Tx1
    let tx_id1 = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent1,
            100 * uT,
            "Testing Message".to_string(),
        ))
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Alice call wait 1");
    let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    match tx_sender_msg {
        TransactionSenderMessage::Single(_) => (),
        _ => {
            panic!("Transaction is the not a single rounder sender variant");
        },
    };

    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    tx_sender_msg.into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("bob call wait 1");

    let _ = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let call = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(&mut call.1.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg1: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    // Send Tx2
    let amount_sent2 = 100001 * uT;
    let tx_id2 = runtime
        .block_on(alice_ts_interface.transaction_service_handle.send_transaction(
            bob_node_identity.public_key().clone(),
            amount_sent2,
            20 * uT,
            "Testing Message2".to_string(),
        ))
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Alice call wait 2");

    let _ = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let call = alice_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let tx_sender_msg = try_decode_sender_message(call.1.to_vec()).unwrap();

    match tx_sender_msg {
        TransactionSenderMessage::Single(_) => (),
        _ => {
            panic!("Transaction is the not a single rounder sender variant");
        },
    };

    runtime
        .block_on(
            bob_ts_interface
                .transaction_send_message_channel
                .send(create_dummy_message(
                    tx_sender_msg.into(),
                    alice_node_identity.public_key(),
                )),
        )
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .expect("Bob call wait 2");

    let (_, _body) = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();
    let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg2: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    let balance = runtime
        .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
        .unwrap();
    assert_eq!(balance.available_balance, MicroTari(0));

    // Give Alice the first of tx reply to start the broadcast process.
    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    bob_tx_reply_msg1.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut tx1_received = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                     if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap(){
                        if tx_id == &tx_id1 {
                            tx1_received = true;
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(tx1_received);
    });

    let alice_completed_tx1 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap()
        .remove(&tx_id1)
        .expect("Transaction must be in collection");

    let tx1_fee = alice_completed_tx1.fee;

    assert_eq!(alice_completed_tx1.status, TransactionStatus::Completed);

    let _ = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_submit_transaction_calls(1, Duration::from_secs(30)),
        )
        .expect("Should receive a tx submission");
    let _ = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_transaction_query_calls(1, Duration::from_secs(30)),
        )
        .expect("Should receive a tx query");

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::Mined,
            block_hash: None,
            confirmations: TransactionServiceConfig::default().num_confirmations_required,
            is_synced: true,
            height_of_longest_chain: 0,
        });

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut tx1_broadcast = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                    println!("Event: {:?}", event);
                     if let TransactionEvent::TransactionBroadcast(tx_id) = &*event.unwrap(){
                        if tx_id == &tx_id1 {
                            tx1_broadcast = true;
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(tx1_broadcast);
    });

    runtime
        .block_on(
            alice_ts_interface
                .transaction_ack_message_channel
                .send(create_dummy_message(
                    bob_tx_reply_msg2.into(),
                    bob_node_identity.public_key(),
                )),
        )
        .unwrap();

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut tx2_received = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                     if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap(){
                        if tx_id == &tx_id2 {
                            tx2_received = true;
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(tx2_received);
    });

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_submit_transaction_response(TxSubmissionResponse {
            accepted: false,
            rejection_reason: TxSubmissionRejectionReason::Orphan,
            is_synced: true,
        });

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::NotStored,
            block_hash: None,
            confirmations: TransactionServiceConfig::default().num_confirmations_required,
            is_synced: true,
            height_of_longest_chain: 0,
        });

    let alice_completed_tx2 = runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .get_completed_transactions(),
        )
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction must be in collection");

    assert_eq!(alice_completed_tx2.status, TransactionStatus::Completed);

    let _ = runtime
        .block_on(
            alice_ts_interface
                .base_node_rpc_mock_state
                .wait_pop_submit_transaction_calls(1, Duration::from_secs(30)),
        )
        .expect("Should receive a tx submission");

    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut tx2_cancelled = false;
        loop {
            tokio::select! {
                event = alice_event_stream.recv() => {
                     if let TransactionEvent::TransactionCancelled(tx_id, _) = &*event.unwrap(){
                        if tx_id == &tx_id2 {
                            tx2_cancelled = true;
                            break;
                        }
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(tx2_cancelled);
    });

    // Check that the cancelled Tx value + change from tx1 is available
    let balance = runtime
        .block_on(alice_ts_interface.output_manager_service_handle.get_balance())
        .unwrap();

    assert_eq!(
        balance.pending_incoming_balance,
        alice_output_value - amount_sent1 - tx1_fee
    );
    assert_eq!(balance.available_balance, alice_output_value);
}

#[test]
fn broadcast_all_completed_transactions_on_startup() {
    let mut runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let (connection, _temp_dir) = make_wallet_database_connection(None);
    let db = TransactionServiceSqliteDatabase::new(connection.clone(), None);
    let kernel = KernelBuilder::new()
        .with_excess(&factories.commitment.zero())
        .with_signature(&Signature::default())
        .build()
        .unwrap();

    let tx = Transaction::new(
        vec![],
        vec![],
        vec![kernel],
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
    );

    let completed_tx1 = CompletedTransaction {
        tx_id: 1.into(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: 5000 * uT,
        fee: MicroTari::from(20),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
        confirmations: None,
        mined_height: None,
        mined_in_block: None,
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2.into(),
        status: TransactionStatus::MinedConfirmed,
        ..completed_tx1.clone()
    };

    let completed_tx3 = CompletedTransaction {
        tx_id: 3.into(),
        status: TransactionStatus::Completed,
        ..completed_tx1.clone()
    };

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx1.tx_id,
        Box::new(completed_tx1),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx2.tx_id,
        Box::new(completed_tx2),
    )))
    .unwrap();

    db.write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
        completed_tx3.tx_id,
        Box::new(completed_tx3),
    )))
    .unwrap();

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories, connection, None);

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::Mined,
            block_hash: None,
            confirmations: TransactionServiceConfig::default().num_confirmations_required,
            is_synced: true,
            height_of_longest_chain: 0,
        });

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_err());

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(alice_ts_interface.base_node_identity.to_peer());

    assert!(runtime
        .block_on(
            alice_ts_interface
                .transaction_service_handle
                .restart_broadcast_protocols()
        )
        .is_ok());

    let mut event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();
    runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);
        let mut found1 = false;
        let mut found2 = false;
        let mut found3 = false;
        loop {
            tokio::select! {
                event = event_stream.recv() => {
                    if let TransactionEvent::TransactionBroadcast(tx_id) = (*event.unwrap()).clone() {
                        if tx_id == TxId::from(1) {
                            found1 = true
                        }
                        if tx_id == TxId::from(2) {
                            found2 = true
                        }
                        if tx_id == TxId::from(3) {
                            found3 = true
                        }
                        if found1 && found3 {
                            break;
                        }

                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(found1);
        assert!(!found2);
        assert!(found3);
    });
}

#[test]
fn test_update_faux_tx_on_oms_validation() {
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();

    let (connection, _temp_dir) = make_wallet_database_connection(None);

    let mut alice_ts_interface = setup_transaction_service_no_comms(&mut runtime, factories.clone(), connection, None);

    let tx_id_1 = runtime
        .block_on(alice_ts_interface.transaction_service_handle.import_utxo_with_status(
            MicroTari::from(10000),
            alice_ts_interface.base_node_identity.public_key().clone(),
            "blah".to_string(),
            None,
            ImportStatus::Imported,
            None,
            None,
        ))
        .unwrap();
    let tx_id_2 = runtime
        .block_on(alice_ts_interface.transaction_service_handle.import_utxo_with_status(
            MicroTari::from(20000),
            alice_ts_interface.base_node_identity.public_key().clone(),
            "one-sided 1".to_string(),
            None,
            ImportStatus::FauxUnconfirmed,
            None,
            None,
        ))
        .unwrap();
    let tx_id_3 = runtime
        .block_on(alice_ts_interface.transaction_service_handle.import_utxo_with_status(
            MicroTari::from(30000),
            alice_ts_interface.base_node_identity.public_key().clone(),
            "one-sided 2".to_string(),
            None,
            ImportStatus::FauxConfirmed,
            None,
            None,
        ))
        .unwrap();

    let (_ti, uo_1) = make_input(&mut OsRng.clone(), MicroTari::from(10000), &factories.commitment);
    let (_ti, uo_2) = make_input(&mut OsRng.clone(), MicroTari::from(20000), &factories.commitment);
    let (_ti, uo_3) = make_input(&mut OsRng.clone(), MicroTari::from(30000), &factories.commitment);
    for (tx_id, uo) in [(tx_id_1, uo_1), (tx_id_2, uo_2), (tx_id_3, uo_3)] {
        runtime
            .block_on(
                alice_ts_interface
                    .output_manager_service_handle
                    .add_output_with_tx_id(tx_id, uo, None),
            )
            .unwrap();
    }

    for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
        let transaction = runtime
            .block_on(alice_ts_interface.transaction_service_handle.get_any_transaction(tx_id))
            .unwrap()
            .unwrap();
        if tx_id == tx_id_1 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::Imported);
            } else {
                panic!("Should find a complete Imported transaction");
            }
        }
        if tx_id == tx_id_2 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::FauxUnconfirmed);
            } else {
                panic!("Should find a complete FauxUnconfirmed transaction");
            }
        }
        if tx_id == tx_id_3 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::FauxConfirmed);
            } else {
                panic!("Should find a complete FauxConfirmed transaction");
            }
        }
    }

    // This will change the status of the imported transaction
    alice_ts_interface
        .output_manager_service_event_publisher
        .send(Arc::new(OutputManagerEvent::TxoValidationSuccess(1u64)))
        .unwrap();

    let mut found_imported = false;
    let mut found_faux_unconfirmed = false;
    let mut found_faux_confirmed = false;
    for _ in 0..20 {
        runtime.block_on(async { sleep(Duration::from_secs(1)).await });
        for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
            let transaction = runtime
                .block_on(alice_ts_interface.transaction_service_handle.get_any_transaction(tx_id))
                .unwrap()
                .unwrap();
            if let WalletTransaction::Completed(tx) = transaction {
                if tx_id == tx_id_1 && tx.status == TransactionStatus::FauxUnconfirmed && !found_imported {
                    found_imported = true;
                }
                if tx_id == tx_id_2 && tx.status == TransactionStatus::FauxUnconfirmed && !found_faux_unconfirmed {
                    found_faux_unconfirmed = true;
                }
                if tx_id == tx_id_3 && tx.status == TransactionStatus::FauxConfirmed && !found_faux_confirmed {
                    found_faux_confirmed = true;
                }
            }
        }
        if found_imported && found_faux_unconfirmed && found_faux_confirmed {
            break;
        }
    }
    assert!(
        found_imported && found_faux_unconfirmed && found_faux_confirmed,
        "Should have found the updated statuses"
    );
}
