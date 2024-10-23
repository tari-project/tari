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
    convert::{TryFrom, TryInto},
    mem::size_of,
    path::Path,
    sync::Arc,
    time::Duration,
};

use blake2::Blake2b;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use chrono::{Duration as ChronoDuration, Utc};
use digest::consts::U32;
use futures::{
    channel::{mpsc, mpsc::Sender},
    FutureExt,
    SinkExt,
};
use minotari_wallet::{
    base_node_service::{config::BaseNodeServiceConfig, handle::BaseNodeServiceHandle, BaseNodeServiceInitializer},
    connectivity_service::{
        create_wallet_connectivity_mock,
        BaseNodePeerManager,
        WalletConnectivityHandle,
        WalletConnectivityInitializer,
        WalletConnectivityInterface,
        WalletConnectivityMock,
    },
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::OutputManagerService,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::KnownOneSidedPaymentScript,
            sqlite_db::{OutputManagerSqliteDatabase, ReceivedOutputInfoForBatch},
        },
        OutputManagerServiceInitializer,
        UtxoSelectionCriteria,
    },
    storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::{run_migration_and_create_sqlite_connection, WalletDbConnection},
    },
    test_utils::{
        create_consensus_constants,
        make_wallet_database_connection,
        make_wallet_database_memory_connection,
        random_string,
    },
    transaction_service::{
        config::TransactionServiceConfig,
        handle::{TransactionEvent, TransactionSendStatus, TransactionServiceHandle},
        service::TransactionService,
        storage::{
            database::{DbKeyValuePair, TransactionBackend, TransactionDatabase, WriteOperation},
            models::{CompletedTransaction, InboundTransaction, OutboundTransaction, WalletTransaction},
            sqlite_db::TransactionServiceSqliteDatabase,
        },
        TransactionServiceInitializer,
    },
};
use prost::Message;
use rand::{rngs::OsRng, RngCore};
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    tari_address::TariAddress,
    transaction::{ImportStatus, TransactionDirection, TransactionStatus, TxId},
    types::{FixedHash, PrivateKey, PublicKey, Signature},
    wallet_types::{ProvidedKeysWallet, WalletType},
};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::{
        rpc,
        rpc::{mock::MockRpcServer, NamedProtocolService},
    },
    test_utils::node_identity::build_node_identity,
    types::CommsDHKE,
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
    consensus::{ConsensusConstantsBuilder, ConsensusManager},
    covenants::Covenant,
    one_sided::shared_secret_to_output_encryption_key,
    proto::base_node as base_node_proto,
    transactions::{
        fee::Fee,
        key_manager::{
            create_memory_db_key_manager,
            MemoryDbKeyManager,
            TransactionKeyManagerInitializer,
            TransactionKeyManagerInterface,
        },
        tari_amount::*,
        test_helpers::{create_wallet_output_with_data, TestParams},
        transaction_components::{
            encrypted_data::PaymentId,
            KernelBuilder,
            OutputFeatures,
            RangeProofType,
            Transaction,
        },
        transaction_protocol::{
            proto::protocol as proto,
            recipient::RecipientSignedMessage,
            sender::TransactionSenderMessage,
            TransactionMetadata,
        },
        CryptoFactories,
        ReceiverTransactionProtocol,
        SenderTransactionProtocol,
    },
    ConfidentialOutputHasher,
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    extended_range_proof::{ExtendedRangeProofService, Statement},
    keys::{PublicKey as PK, SecretKey as SK},
    ristretto::bulletproofs_plus::RistrettoAggregatedPublicStatement,
};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager_service::{storage::sqlite_db::KeyManagerSqliteDatabase, KeyId, KeyManagerInterface},
};
use tari_p2p::{comms_connector::pubsub_connector, domain_message::DomainMessage, Network};
use tari_script::{inputs, push_pubkey_script, script, ExecutionStack};
use tari_service_framework::{reply_channel, RegisterHandle, StackBuilder};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::{comms_and_services::get_next_memory_address, random};
use tari_utilities::{ByteArray, SafePassword};
use tempfile::tempdir;
use tokio::{
    sync::{broadcast, broadcast::channel},
    task,
    time::sleep,
};

use crate::support::{
    base_node_service_mock::MockBaseNodeService,
    comms_and_services::{create_dummy_message, setup_comms_services},
    comms_rpc::{connect_rpc_client, BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    utils::{create_wallet_output_from_sender_data, make_fake_input_from_copy, make_input},
};

async fn setup_transaction_service<P: AsRef<Path>>(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    consensus_manager: ConsensusManager,
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
    MemoryDbKeyManager,
    OutputManagerSqliteDatabase,
) {
    let (publisher, subscription_factory) = pubsub_connector(100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(
        node_identity.clone(),
        peers,
        publisher,
        database_path.as_ref().to_str().unwrap().to_owned(),
        discovery_request_timeout,
        shutdown_signal.clone(),
    )
    .await;

    let passphrase = SafePassword::from("My lovely secret passphrase");
    let db = WalletDatabase::new(WalletSqliteDatabase::new(db_connection.clone(), passphrase).unwrap());
    let metadata = ChainMetadata::new(i64::MAX as u64, FixedHash::zero(), 0, 0, 1.into(), 0).unwrap();

    db.set_chain_metadata(metadata).unwrap();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let cipher = XChaCha20Poly1305::new(key_ga);

    let ts_backend = TransactionServiceSqliteDatabase::new(db_connection.clone(), cipher.clone());
    let oms_backend = OutputManagerSqliteDatabase::new(db_connection.clone());

    let connection = DbConnection::connect_url(&DbConnectionUrl::MemoryShared(random_string(8))).unwrap();
    let cipher = CipherSeed::new();
    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);
    let kms_backend = KeyManagerSqliteDatabase::init(connection, db_cipher);
    let wallet_type = Arc::new(WalletType::ProvidedKeys(ProvidedKeysWallet {
        public_spend_key: PublicKey::from_secret_key(node_identity.secret_key()),
        private_spend_key: Some(node_identity.secret_key().clone()),
        view_key: SK::random(&mut OsRng),
        private_comms_key: Some(node_identity.secret_key().clone()),
    }));
    let handles = StackBuilder::new(shutdown_signal)
        .add_initializer(RegisterHandle::new(dht))
        .add_initializer(RegisterHandle::new(comms.connectivity()))
        .add_initializer(OutputManagerServiceInitializer::<
            OutputManagerSqliteDatabase,
            MemoryDbKeyManager,
        >::new(
            OutputManagerServiceConfig::default(),
            oms_backend.clone(),
            factories.clone(),
            Network::LocalNet.into(),
        ))
        .add_initializer(TransactionKeyManagerInitializer::<KeyManagerSqliteDatabase<_>>::new(
            kms_backend,
            cipher,
            factories.clone(),
            wallet_type.clone(),
        ))
        .add_initializer(TransactionServiceInitializer::<_, _, MemoryDbKeyManager>::new(
            TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(5),
                chain_monitoring_timeout: Duration::from_secs(5),
                low_power_polling_timeout: Duration::from_secs(20),
                num_confirmations_required: 0,
                ..Default::default()
            },
            subscription_factory,
            ts_backend,
            node_identity.clone(),
            Network::LocalNet,
            consensus_manager,
            factories,
            db.clone(),
            wallet_type,
        ))
        .add_initializer(BaseNodeServiceInitializer::new(BaseNodeServiceConfig::default(), db))
        .add_initializer(WalletConnectivityInitializer::new(BaseNodeServiceConfig::default()))
        .build()
        .await
        .unwrap();

    let output_manager_handle = handles.expect_handle::<OutputManagerHandle>();
    let key_manager_handle = handles.expect_handle::<MemoryDbKeyManager>();
    let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();
    let connectivity_service_handle = handles.expect_handle::<WalletConnectivityHandle>();

    (
        transaction_service_handle,
        output_manager_handle,
        comms,
        connectivity_service_handle,
        key_manager_handle,
        oms_backend,
    )
}

/// This struct holds a collection of interfaces that can be used in tests to interact with a Transaction Service that
/// is constructed without a comms layer, base node etc
pub struct TransactionServiceNoCommsInterface {
    transaction_service_handle: TransactionServiceHandle,
    output_manager_service_handle: OutputManagerHandle,
    key_manager_handle: MemoryDbKeyManager,
    outbound_service_mock_state: OutboundServiceMockState,
    transaction_send_message_channel:
        Sender<DomainMessage<Result<proto::TransactionSenderMessage, prost::DecodeError>>>,
    transaction_ack_message_channel: Sender<DomainMessage<Result<proto::RecipientSignedMessage, prost::DecodeError>>>,
    transaction_finalize_message_channel:
        Sender<DomainMessage<Result<proto::TransactionFinalizedMessage, prost::DecodeError>>>,
    _base_node_response_message_channel:
        Sender<DomainMessage<Result<base_node_proto::BaseNodeServiceResponse, prost::DecodeError>>>,
    transaction_cancelled_message_channel:
        Sender<DomainMessage<Result<proto::TransactionCancelledMessage, prost::DecodeError>>>,
    _shutdown: Shutdown,
    _mock_rpc_server: MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    base_node_identity: Arc<NodeIdentity>,
    base_node_rpc_mock_state: BaseNodeWalletRpcMockState,
    wallet_connectivity_service_mock: WalletConnectivityMock,
    _rpc_server_connection: PeerConnection,
    output_manager_service_event_publisher: broadcast::Sender<Arc<OutputManagerEvent>>,
    ts_db: TransactionServiceSqliteDatabase,
    oms_db: OutputManagerDatabase<OutputManagerSqliteDatabase>,
}

/// This utility function creates a Transaction service without using the Service Framework Stack and exposes all the
/// streams for testing purposes.
#[allow(clippy::type_complexity)]
async fn setup_transaction_service_no_comms(
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
    task::spawn(mock_outbound_service.run());

    let service = BaseNodeWalletRpcMockService::new();
    let base_node_rpc_mock_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let mut mock_rpc_server = MockRpcServer::new(server, node_identity.clone());

    mock_rpc_server.serve();

    let mut wallet_connectivity_service_mock = create_wallet_connectivity_mock();

    let mut rpc_server_connection = mock_rpc_server
        .create_connection(node_identity.to_peer(), protocol_name.into())
        .await;

    wallet_connectivity_service_mock
        .set_base_node_wallet_rpc_client(connect_rpc_client(&mut rpc_server_connection).await);
    wallet_connectivity_service_mock.set_base_node(BaseNodePeerManager::new(0, vec![node_identity.to_peer()]).unwrap());
    wallet_connectivity_service_mock.base_node_changed().await;

    let consensus_manager = ConsensusManager::builder(Network::LocalNet).build().unwrap();
    let constants = ConsensusConstantsBuilder::new(Network::LocalNet).build();

    let shutdown = Shutdown::new();

    let (sender, receiver_bns) = reply_channel::unbounded();
    let (base_node_service_event_publisher, _) = broadcast::channel(100);

    let base_node_service_handle = BaseNodeServiceHandle::new(sender, base_node_service_event_publisher);
    let mut mock_base_node_service = MockBaseNodeService::new(receiver_bns, shutdown.to_signal());
    mock_base_node_service.set_default_base_node_state();
    task::spawn(mock_base_node_service.run());

    let passphrase = SafePassword::from("My lovely secret passphrase");
    let wallet =
        WalletSqliteDatabase::new(db_connection.clone(), passphrase).expect("Should be able to create wallet database");
    let cipher = wallet.cipher();
    let wallet_db = WalletDatabase::new(wallet);

    let ts_service_db = TransactionServiceSqliteDatabase::new(db_connection.clone(), cipher.clone());
    let ts_db = TransactionDatabase::new(ts_service_db.clone());
    let key_manager = create_memory_db_key_manager().unwrap();
    let oms_db = OutputManagerDatabase::new(OutputManagerSqliteDatabase::new(db_connection));
    let output_manager_service = OutputManagerService::new(
        OutputManagerServiceConfig::default(),
        oms_request_receiver,
        oms_db.clone(),
        output_manager_service_event_publisher.clone(),
        factories.clone(),
        constants,
        shutdown.to_signal(),
        base_node_service_handle.clone(),
        Network::LocalNet,
        wallet_connectivity_service_mock.clone(),
        key_manager.clone(),
    )
    .await
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
        ts_db.clone(),
        wallet_db.clone(),
        ts_request_receiver,
        tx_receiver,
        tx_ack_receiver,
        tx_finalized_receiver,
        base_node_response_receiver,
        tx_cancelled_receiver,
        output_manager_service_handle.clone(),
        key_manager.clone(),
        outbound_message_requester,
        wallet_connectivity_service_mock.clone(),
        event_publisher,
        node_identity.clone(),
        Network::LocalNet,
        consensus_manager,
        factories,
        shutdown.to_signal(),
        base_node_service_handle,
        key_manager.get_wallet_type().await,
    )
    .await
    .unwrap();
    task::spawn(async move { output_manager_service.start().await.unwrap() });
    task::spawn(async move { ts_service.start().await.unwrap() });
    TransactionServiceNoCommsInterface {
        transaction_service_handle,
        output_manager_service_handle,
        key_manager_handle: key_manager,
        outbound_service_mock_state,
        transaction_send_message_channel,
        transaction_ack_message_channel,
        transaction_finalize_message_channel,
        _base_node_response_message_channel: base_node_response_message_channel,
        transaction_cancelled_message_channel,
        _shutdown: shutdown,
        _mock_rpc_server: mock_rpc_server,
        base_node_identity: node_identity,
        base_node_rpc_mock_state,
        wallet_connectivity_service_mock,
        _rpc_server_connection: rpc_server_connection,
        output_manager_service_event_publisher,
        ts_db: ts_service_db,
        oms_db,
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
    envelope_body
        .decode_part::<proto::TransactionFinalizedMessage>(1)
        .unwrap_or_default()
}

fn try_decode_transaction_cancelled_message(bytes: Vec<u8>) -> Option<proto::TransactionCancelledMessage> {
    let envelope_body = EnvelopeBody::decode(&mut bytes.as_slice()).unwrap();
    envelope_body
        .decode_part::<proto::TransactionCancelledMessage>(1)
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn manage_single_transaction() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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
    let alice_connection = make_wallet_database_memory_connection();
    let bob_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path.clone(),
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let mut alice_event_stream = alice_ts.get_event_stream();

    sleep(Duration::from_secs(2)).await;

    let (mut bob_ts, mut bob_oms, bob_comms, _bob_connectivity, _bob_key_manager_handle, _bob_db) =
        setup_transaction_service(
            bob_node_identity.clone(),
            vec![alice_node_identity.clone()],
            consensus_manager,
            factories.clone(),
            bob_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let mut bob_event_stream = bob_ts.get_event_stream();

    let _peer_connection = bob_comms
        .connectivity()
        .dial_peer(alice_node_identity.node_id().clone(), false)
        .await
        .unwrap();

    let value = MicroMinotari::from(1000);
    let uo1 = make_input(
        &mut OsRng,
        MicroMinotari(2500),
        &OutputFeatures::default(),
        &alice_key_manager_handle,
    )
    .await;
    let bob_address =
        TariAddress::new_single_address_with_interactive_only(bob_node_identity.public_key().clone(), network);
    assert!(alice_ts
        .send_transaction(
            bob_address.clone(),
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            "".to_string()
        )
        .await
        .is_err());

    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let message = "TAKE MAH MONEYS!".to_string();
    alice_ts
        .send_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(4),
            message,
        )
        .await
        .expect("Alice sending tx");

    let delay = sleep(Duration::from_secs(90));
    tokio::pin!(delay);
    let mut count = 0;
    loop {
        tokio::select! {
            _event = alice_event_stream.recv() => {
                println!("alice: {:?}", _event.as_ref().unwrap());
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

    let mut tx_id = TxId::from(0u64);
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

    assert!(bob_ts.get_completed_transaction(999u64.into()).await.is_err());

    let _bob_completed_tx = bob_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find tx");

    assert_eq!(bob_oms.get_balance().await.unwrap().pending_incoming_balance, value);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn large_interactive_transaction() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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
        "large_interactive_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );
    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let alice_connection = make_wallet_database_memory_connection();
    let bob_connection = make_wallet_database_memory_connection();

    // Alice sets up her Transaction Service
    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path.clone(),
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;
    let mut alice_event_stream = alice_ts.get_event_stream();

    sleep(Duration::from_secs(2)).await;

    // Bob sets up his Transaction Service
    let (mut bob_ts, mut bob_oms, bob_comms, _bob_connectivity, _bob_key_manager_handle, _bob_db) =
        setup_transaction_service(
            bob_node_identity.clone(),
            vec![alice_node_identity.clone()],
            consensus_manager,
            factories.clone(),
            bob_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;
    let mut bob_event_stream = bob_ts.get_event_stream();

    // Verify that Alice and Bob are connected
    let _peer_connection = bob_comms
        .connectivity()
        .dial_peer(alice_node_identity.node_id().clone(), false)
        .await
        .unwrap();

    // Alice prepares her large transaction
    let outputs_count = 1250usize;
    let output_value = MicroMinotari(20000);
    let mut unspent: Vec<(FixedHash, bool)> = Vec::with_capacity(outputs_count);
    for _ in 0..outputs_count {
        let uo = make_input(
            &mut OsRng,
            output_value,
            &OutputFeatures::default(),
            &alice_key_manager_handle,
        )
        .await;
        alice_oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&alice_key_manager_handle).await.unwrap(), true));
    }
    alice_db.mark_outputs_as_unspent(unspent).unwrap();
    let transaction_value = output_value * (outputs_count as u64 - 1);
    let bob_address =
        TariAddress::new_single_address_with_interactive_only(bob_node_identity.public_key().clone(), network);

    let message = "TAKE MAH MONEYS!".to_string();
    alice_ts
        .send_transaction(
            bob_address,
            transaction_value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(1),
            message,
        )
        .await
        .expect("Alice sending large tx");

    // Monitor Alice's and Bob's event streams for the transaction send results
    let delay = sleep(Duration::from_secs(90));
    tokio::pin!(delay);
    let mut bob_finalized = false;
    let mut alice_finalized = false;
    let mut tx_id = TxId::from(0u64);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                // println!("alice: {:?}", event.as_ref().unwrap());
                match &*event.clone().unwrap() {
                    TransactionEvent::TransactionSendResult(id, _) => {
                        // We want to ensure that we can get the pending outbound transaction from the database,
                        // and excercise the sender_protocol
                        let pending_outbound = alice_ts.get_pending_outbound_transactions().await.unwrap();
                        pending_outbound.get(id).unwrap().sender_protocol.get_amount_to_recipient().unwrap();
                        assert_eq!(
                            pending_outbound.get(id).unwrap().sender_protocol.get_amount_to_recipient().unwrap(),
                            transaction_value
                        );
                    },
                    TransactionEvent::ReceivedTransactionReply(_) => {
                        alice_finalized = true;
                        if alice_finalized && bob_finalized {
                            break;
                        }
                    },
                    _ => (),
                }
            },
           event = bob_event_stream.recv() => {
                // println!("bob: {:?}", event.as_ref().unwrap());
                match &*event.clone().unwrap() {
                    TransactionEvent::ReceivedTransaction(id) => {
                        // We want to ensure that we can get the pending inbound transaction from the database,
                        // and excercise the receiver_protocol
                        let pending_inbound = bob_ts.get_pending_inbound_transactions().await.unwrap();
                        assert!(pending_inbound.get(id).unwrap().receiver_protocol.get_signed_data().is_ok());
                        assert_eq!(pending_inbound.get(id).unwrap().amount, transaction_value);
                    },
                    TransactionEvent::ReceivedFinalizedTransaction(id) => {
                        tx_id = *id;
                        bob_finalized = true;
                        if alice_finalized && bob_finalized {
                            break;
                        }
                    },
                    _ => (),
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(bob_finalized && alice_finalized);

    let bob_completed_tx = bob_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find tx");
    assert_eq!(bob_completed_tx.transaction.body.inputs().len(), outputs_count);
    assert_eq!(
        bob_oms.get_balance().await.unwrap().pending_incoming_balance,
        transaction_value
    );
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_spend_dust_to_self_in_oversized_transaction() {
    //` cargo test --release --test  wallet_integration_tests
    //` transaction_service_tests::service::test_spend_dust_to_self_in_oversized_transaction > .\target\output.txt
    //` 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
    let factories = CryptoFactories::default();
    let shutdown = Shutdown::new();

    // Alice's wallet parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
    );
    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let alice_connection = make_wallet_database_memory_connection();

    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path.clone(),
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    // Alice create dust

    let amount_per_output = 10_000 * uT;
    // This value was determined by running the test and evaluating the error message,
    // e.g. `TransactionTooLarge { got: 3379097, expected: 3135488 }`
    let max_number_of_outputs_in_frame = (rpc::RPC_MAX_FRAME_SIZE as f64 / 700.0f64).ceil() as usize;
    let number_of_outputs = max_number_of_outputs_in_frame + 100;
    let mut uo_reference = make_input(
        &mut OsRng,
        amount_per_output,
        &OutputFeatures::default(),
        &alice_key_manager_handle,
    )
    .await;
    let mut unspent: Vec<(FixedHash, bool)> = Vec::with_capacity(number_of_outputs);
    for _ in 0..number_of_outputs {
        let uo = make_fake_input_from_copy(&mut uo_reference, &alice_key_manager_handle).await;

        alice_oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&alice_key_manager_handle).await.unwrap(), true));
    }
    alice_db.mark_outputs_as_unspent(unspent).unwrap();

    let balance = alice_oms.get_balance().await.unwrap();
    let initial_available_balance = balance.available_balance;

    // Alice try to spend too much dust to self

    let fee_per_gram = MicroMinotari::from(1);
    let message = "TAKE MAH _OWN_ MONEYS!".to_string();
    let value = balance.available_balance - amount_per_output * 10;
    let alice_address =
        TariAddress::new_single_address_with_interactive_only(alice_node_identity.public_key().clone(), network);
    assert!(alice_ts
        .send_transaction(
            alice_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            message.clone(),
        )
        .await
        .is_err());
    let balance = alice_oms.get_balance().await.unwrap();
    // Encumbered outputs are re-instated
    assert_eq!(balance.available_balance, initial_available_balance);
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_spend_dust_to_other_in_oversized_transaction() {
    //` cargo test --release --test  wallet_integration_tests
    //` transaction_service_tests::service::test_spend_dust_to_other_in_oversized_transaction > .\target\output.txt
    //` 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
    let factories = CryptoFactories::default();
    let shutdown = Shutdown::new();

    // Alice's wallet parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
    );
    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let alice_connection = make_wallet_database_memory_connection();

    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path.clone(),
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    // Alice create dust

    let amount_per_output = 10_000 * uT;
    // This value was determined by running the test and evaluating the error message,
    // e.g. `TransactionTooLarge { got: 3205068, expected: 3135488 }`
    let max_number_of_outputs_in_frame = (rpc::RPC_MAX_FRAME_SIZE as f64 / 1175.0f64).ceil() as usize;
    let number_of_outputs = max_number_of_outputs_in_frame + 100;
    let mut uo_reference = make_input(
        &mut OsRng,
        amount_per_output,
        &OutputFeatures::default(),
        &alice_key_manager_handle,
    )
    .await;
    let mut unspent: Vec<(FixedHash, bool)> = Vec::with_capacity(number_of_outputs);
    for _ in 0..number_of_outputs {
        let uo = make_fake_input_from_copy(&mut uo_reference, &alice_key_manager_handle).await;

        alice_oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&alice_key_manager_handle).await.unwrap(), true));
    }
    alice_db.mark_outputs_as_unspent(unspent).unwrap();

    let balance = alice_oms.get_balance().await.unwrap();
    let initial_available_balance = balance.available_balance;

    // Alice try to spend too much dust to Bob

    let fee_per_gram = MicroMinotari::from(1);
    let message = "GIVE MAH _OWN_ MONEYS AWAY!".to_string();
    let value = balance.available_balance - amount_per_output * 10;
    let bob_address =
        TariAddress::new_single_address_with_interactive_only(bob_node_identity.public_key().clone(), network);
    let tx_id = alice_ts
        .send_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            message.clone(),
        )
        .await
        .unwrap();
    println!("tx_id: {}", tx_id);

    let mut count = 0;
    loop {
        match alice_ts.get_any_transaction(tx_id).await {
            Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
            Ok(Some(WalletTransaction::PendingOutbound(_))) => {
                println!("waited {}ms to detect the transaction", count * 100);
                break;
            },
            _ => {
                panic!(
                    "waited {}ms to detect the transaction, unexpected error/inbound/completed!",
                    count * 100
                );
            },
        }
        count += 1;
        if count > 20 * 10 {
            panic!("waited {}ms but could not detect the transaction!", count * 100);
        }
    }
    // Encumbered outputs are re-instated
    assert_eq!(balance.available_balance, initial_available_balance);
}

#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::too_many_lines)]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_spend_dust_happy_path() {
    //` cargo test --release --test  wallet_integration_tests
    //` transaction_service_tests::service::test_spend_dust_happy_path > .\target\output.txt 2>&1
    // env_logger::init(); // Set `$env:RUST_LOG = "trace"`

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
    let factories = CryptoFactories::default();
    let shutdown = Shutdown::new();

    // Alice's wallet parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
    );
    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();
    let alice_connection = make_wallet_database_memory_connection();

    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path.clone(),
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    // Alice create dust

    let amount_per_output = 10_000 * uT;
    let number_of_outputs = 1000;
    let fee_per_gram = MicroMinotari::from(1);
    let mut uo_reference = make_input(
        &mut OsRng,
        amount_per_output,
        &OutputFeatures::default(),
        &alice_key_manager_handle,
    )
    .await;
    let mut unspent: Vec<(FixedHash, bool)> = Vec::with_capacity(number_of_outputs as usize);
    for _ in 0..number_of_outputs {
        let uo = make_fake_input_from_copy(&mut uo_reference, &alice_key_manager_handle).await;

        alice_oms.add_output(uo.clone(), None).await.unwrap();
        unspent.push((uo.hash(&alice_key_manager_handle).await.unwrap(), true));
    }
    alice_db.mark_outputs_as_unspent(unspent).unwrap();

    let balance = alice_oms.get_balance().await.unwrap();
    let initial_available_balance = balance.available_balance;

    // Alice try to spend a fair amount of dust to self [should succeed] (we just need to verify that the
    // transaction is created and that the available balance is correct)

    let message = "TAKE MAH _OWN_ MONEYS!".to_string();
    let value_self = (number_of_outputs / 3) * amount_per_output;
    let alice_address =
        TariAddress::new_single_address_with_interactive_only(alice_node_identity.public_key().clone(), network);
    let tx_id = alice_ts
        .send_transaction(
            alice_address,
            value_self,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            message.clone(),
        )
        .await
        .unwrap();
    let mut count = 0;
    let mut fees_self = loop {
        match alice_ts.get_any_transaction(tx_id).await {
            Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
            Ok(Some(WalletTransaction::Completed(tx))) => {
                println!("waited {}ms to detect the transaction", count * 100);
                break tx.fee;
            },
            _ => {
                panic!(
                    "waited {}ms to detect the transaction, unexpected error/inbound/outboubd!",
                    count * 100
                );
            },
        }
        count += 1;
        if count > 20 * 10 {
            panic!("waited {}ms but could not detect the transaction!", count * 100);
        }
    };
    fees_self = (fees_self.0 as f64 / amount_per_output.0 as f64).ceil() as u64 * amount_per_output;
    let balance = alice_oms.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        initial_available_balance - value_self - fees_self
    );

    // Alice try to spend a fair amount of dust to Bob [should succeed] (We do not need Bob to be present,
    // we just need to verify that the transaction is created and that the available balance is correct)

    let message = "GIVE MAH _OWN_ MONEYS AWAY!".to_string();
    let value_bob = (number_of_outputs / 3) * amount_per_output;
    let bob_address =
        TariAddress::new_single_address_with_interactive_only(bob_node_identity.public_key().clone(), network);
    let tx_id = alice_ts
        .send_transaction(
            bob_address,
            value_bob,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            fee_per_gram,
            message.clone(),
        )
        .await
        .unwrap();
    println!("tx_id: {}", tx_id);

    let mut count = 0;
    let mut fees_bob = loop {
        match alice_ts.get_any_transaction(tx_id).await {
            Ok(None) => tokio::time::sleep(Duration::from_millis(100)).await,
            Ok(Some(WalletTransaction::PendingOutbound(tx))) => {
                println!("waited {}ms to detect the transaction", count * 100);
                break tx.fee;
            },
            _ => {
                panic!(
                    "waited {}ms to detect the transaction, unexpected error/inbound/completed!",
                    count * 100
                );
            },
        }
        count += 1;
        if count > 20 * 10 {
            panic!("waited {}ms but could not detect the transaction!", count * 100);
        }
    };
    fees_bob = (fees_bob.0 as f64 / amount_per_output.0 as f64).ceil() as u64 * amount_per_output;
    let balance = alice_oms.get_balance().await.unwrap();
    assert_eq!(
        balance.available_balance,
        initial_available_balance - value_self - fees_self - value_bob - fees_bob
    );
}

#[tokio::test]
async fn single_transaction_to_self() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager,
            factories.clone(),
            db_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;

    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let message = "TAKE MAH _OWN_ MONEYS!".to_string();
    let value = 10000.into();
    let alice_address =
        TariAddress::new_single_address_with_interactive_only(alice_node_identity.public_key().clone(), network);
    let tx_id = alice_ts
        .send_transaction(
            alice_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
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
}

#[tokio::test]
async fn large_coin_split_transaction() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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
        "large_coin_split_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager,
            factories.clone(),
            db_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let initial_wallet_value = 20 * T;
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;

    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let fee_per_gram = MicroMinotari::from(1);
    let split_count = 499;
    let (tx_id, coin_split_tx, amount) = alice_oms
        .create_coin_split(vec![], 10000.into(), split_count, fee_per_gram)
        .await
        .unwrap();
    assert_eq!(coin_split_tx.body.inputs().len(), 1);
    assert_eq!(coin_split_tx.body.outputs().len(), split_count + 1);

    alice_ts
        .submit_transaction(tx_id, coin_split_tx, amount, "large coin-split".to_string())
        .await
        .expect("Alice sending coin-split tx");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find tx");

    let fees = completed_tx.fee;

    assert_eq!(
        alice_oms.get_balance().await.unwrap().pending_incoming_balance,
        initial_wallet_value - fees
    );
}

#[tokio::test]
async fn single_transaction_burn_tari() {
    // let _ = env_logger::builder().is_test(true).try_init(); // Need `$env:RUST_LOG = "trace"` for this to work
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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
        "single_transaction_burn_tari: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let temp_dir = tempdir().unwrap();
    let database_path = temp_dir.path().to_str().unwrap().to_string();

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![],
            consensus_manager,
            factories.clone(),
            db_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;

    // Burn output

    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let message = "BURN MAH _OWN_ MONEYS!".to_string();
    let burn_value = 10000.into();
    let (claim_private_key, claim_public_key) = PublicKey::random_keypair(&mut OsRng);
    let (tx_id, burn_proof) = alice_ts
        .burn_tari(
            burn_value,
            UtxoSelectionCriteria::default(),
            20.into(),
            message.clone(),
            Some(claim_public_key.clone()),
        )
        .await
        .expect("Alice sending burn tx");

    // Verify final balance

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find tx");

    let fees = completed_tx.fee;

    assert_eq!(
        alice_oms.get_balance().await.unwrap().pending_incoming_balance,
        initial_wallet_value - burn_value - fees
    );

    // Verify burn proof

    let challenge_bytes = ConfidentialOutputHasher::new("commitment_signature")
        .chain(&burn_proof.ownership_proof.as_ref().unwrap().public_nonce())
        .chain(&burn_proof.commitment)
        .chain(&claim_public_key)
        .finalize();
    let challenge = PrivateKey::from_uniform_bytes(&challenge_bytes).unwrap();
    assert!(burn_proof.ownership_proof.unwrap().verify(
        &burn_proof.commitment,
        &challenge,
        factories.commitment.as_ref()
    ));
    let statement = RistrettoAggregatedPublicStatement {
        statements: vec![Statement {
            commitment: burn_proof.commitment.clone(),
            minimum_value_promise: MicroMinotari::zero().as_u64(),
        }],
    };
    assert!(factories
        .range_proof
        .verify_batch(vec![&burn_proof.range_proof.to_vec()], vec![&statement])
        .is_ok());
    let spending_key_id_from_reciprocal_claim_public_key = key_manager_handle
        .get_spending_key_id(&burn_proof.reciprocal_claim_public_key.clone())
        .await
        .unwrap();

    // Verify recovery of burned output

    let shared_secret = CommsDHKE::new(&claim_private_key, &burn_proof.reciprocal_claim_public_key);
    let encryption_key = shared_secret_to_output_encryption_key(&shared_secret).unwrap();
    let recovery_key_id = KeyId::Imported {
        key: PublicKey::from_secret_key(&encryption_key),
    };
    let mut found_burned_output = false;
    for output in completed_tx.transaction.body.outputs() {
        if output.is_burned() {
            found_burned_output = true;
            match key_manager_handle
                .try_output_key_recovery(output, Some(&recovery_key_id))
                .await
            {
                Ok((spending_key_id, value, _)) => {
                    assert_eq!(value, burn_value);
                    assert_eq!(spending_key_id, spending_key_id_from_reciprocal_claim_public_key)
                },
                Err(e) => panic!("{}", e),
            }
        }
    }
    assert!(found_burned_output);
}

#[tokio::test]
async fn send_one_sided_transaction_to_other() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity,
            vec![],
            consensus_manager,
            factories.clone(),
            db_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let mut alice_event_stream = alice_ts.get_event_stream();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    let mut alice_oms_clone = alice_oms.clone();
    alice_oms_clone.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let message = "SEE IF YOU CAN CATCH THIS ONE..... SIDED TX!".to_string();
    let value = 10000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let random_pvt_key = PrivateKey::random(&mut OsRng);
    let bob_view_key = PublicKey::from_secret_key(&random_pvt_key);
    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key,
        bob_node_identity.public_key().clone(),
        network,
    );
    let tx_id = alice_ts_clone
        .send_one_sided_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20.into(),
            message.clone(),
            PaymentId::Empty,
        )
        .await
        .expect("Alice sending one-sided tx to Bob");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find completed one-sided tx");

    let fees = completed_tx.fee;

    assert_eq!(
        alice_oms.get_balance().await.unwrap().pending_incoming_balance,
        initial_wallet_value - value - fees
    );

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
}

#[tokio::test]
async fn recover_one_sided_transaction() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let alice_connection = make_wallet_database_memory_connection();
    let bob_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity,
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let (_bob_ts, mut bob_oms, _bob_comms, _bob_connectivity, bob_key_manager_handle, _bob_db) =
        setup_transaction_service(
            bob_node_identity.clone(),
            vec![],
            consensus_manager,
            factories.clone(),
            bob_connection,
            database_path2,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;
    let script = push_pubkey_script(bob_node_identity.public_key());
    let known_script = KnownOneSidedPaymentScript {
        script_hash: script.as_hash::<Blake2b<U32>>().unwrap().to_vec(),
        script_key_id: bob_key_manager_handle
            .import_key(bob_node_identity.secret_key().clone())
            .await
            .unwrap(),
        script,
        input: ExecutionStack::default(),
        script_lock_height: 0,
    };
    let mut cloned_bob_oms = bob_oms.clone();
    cloned_bob_oms.add_known_script(known_script).await.unwrap();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &alice_key_manager_handle,
    )
    .await;
    let mut alice_oms_clone = alice_oms;
    alice_oms_clone.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let message = "".to_string();
    let value = 10000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let bob_view_key = bob_key_manager_handle.get_view_key().await.unwrap();
    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key.pub_key,
        bob_node_identity.public_key().clone(),
        network,
    );
    let tx_id = alice_ts_clone
        .send_one_sided_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20.into(),
            message.clone(),
            PaymentId::Empty,
        )
        .await
        .expect("Alice sending one-sided tx to Bob");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find completed one-sided tx");
    let outputs = completed_tx.transaction.body.outputs().clone();

    let recovered_outputs_1 = bob_oms
        .scan_outputs_for_one_sided_payments(outputs.iter().map(|o| (o.clone(), None)).collect())
        .await
        .unwrap();
    // Bob should be able to claim 1 output.
    assert_eq!(1, recovered_outputs_1.len());
    assert_eq!(value, recovered_outputs_1[0].output.value);

    // Should ignore already existing outputs
    let recovered_outputs_2 = bob_oms
        .scan_outputs_for_one_sided_payments(outputs.into_iter().map(|o| (o, None)).collect())
        .await
        .unwrap();
    assert!(recovered_outputs_2.is_empty());
}

#[tokio::test]
async fn recover_stealth_one_sided_transaction() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let alice_connection = make_wallet_database_memory_connection();
    let bob_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, alice_oms, _alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity,
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let (_bob_ts, mut bob_oms, _bob_comms, _bob_connectivity, bob_key_manager_handle, _bob_db) =
        setup_transaction_service(
            bob_node_identity.clone(),
            vec![],
            consensus_manager,
            factories.clone(),
            bob_connection,
            database_path2,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let bob_view_key = bob_key_manager_handle.get_view_key().await.unwrap();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &alice_key_manager_handle,
    )
    .await;
    let mut alice_oms_clone = alice_oms;
    alice_oms_clone.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let message = "".to_string();
    let value = 10000.into();
    let mut alice_ts_clone = alice_ts.clone();

    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key.pub_key,
        bob_node_identity.public_key().clone(),
        network,
    );
    let tx_id = alice_ts_clone
        .send_one_sided_to_stealth_address_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20.into(),
            message.clone(),
            PaymentId::Empty,
        )
        .await
        .expect("Alice sending one-sided tx to Bob");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find completed one-sided tx");
    let outputs = completed_tx.transaction.body.outputs().clone();

    let recovered_outputs_1 = bob_oms
        .scan_outputs_for_one_sided_payments(outputs.iter().map(|o| (o.clone(), None)).collect())
        .await
        .unwrap();
    // Bob should be able to claim 1 output.
    assert_eq!(1, recovered_outputs_1.len());
    assert_eq!(value, recovered_outputs_1[0].output.value);

    // Should ignore already existing outputs
    let recovered_outputs_2 = bob_oms
        .scan_outputs_for_one_sided_payments(outputs.into_iter().map(|o| (o, None)).collect())
        .await
        .unwrap();
    assert!(recovered_outputs_2.is_empty());
}

#[tokio::test]
async fn test_htlc_send_and_claim() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let db_connection = make_wallet_database_memory_connection();
    let bob_connection = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_comms, _alice_connectivity, key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity,
            vec![],
            consensus_manager,
            factories.clone(),
            db_connection,
            database_path,
            Duration::from_secs(0),
            shutdown.to_signal(),
        )
        .await;

    let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), bob_connection, None).await;

    log::info!(
        "manage_single_transaction: Bob: '{}'",
        bob_ts_interface.base_node_identity.node_id().short_str(),
    );

    let mut alice_event_stream = alice_ts.get_event_stream();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut OsRng,
        initial_wallet_value,
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.hash(&key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let message = "".to_string();
    let value = 10000.into();
    let bob_pubkey = bob_ts_interface.base_node_identity.public_key().clone();
    let bob_view_key = bob_ts_interface.key_manager_handle.get_view_key().await.unwrap();
    let bob_address =
        TariAddress::new_dual_address_with_default_features(bob_view_key.pub_key, bob_pubkey.clone(), network);
    let (tx_id, pre_image, output) = alice_ts
        .send_sha_atomic_swap_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            20.into(),
            message.clone(),
        )
        .await
        .expect("Alice sending HTLC transaction");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find completed HTLC tx");

    let fees = completed_tx.fee;

    assert_eq!(
        alice_oms.get_balance().await.unwrap().pending_incoming_balance,
        initial_wallet_value - fees
    );

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
    let hash = output.hash();
    bob_ts_interface.base_node_rpc_mock_state.set_utxos(vec![output]);
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn manage_multiple_transactions() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let (mut alice_ts, mut alice_oms, alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity.clone(),
            vec![bob_node_identity.clone(), carol_node_identity.clone()],
            consensus_manager.clone(),
            factories.clone(),
            alice_connection,
            database_path.clone(),
            Duration::from_secs(1),
            shutdown.to_signal(),
        )
        .await;
    let mut alice_event_stream = alice_ts.get_event_stream();

    sleep(Duration::from_secs(5)).await;

    // Spin up Bob and Carol
    let (mut bob_ts, mut bob_oms, bob_comms, _bob_connectivity, bob_key_manager_handle, bob_db) =
        setup_transaction_service(
            bob_node_identity.clone(),
            vec![alice_node_identity.clone()],
            consensus_manager.clone(),
            factories.clone(),
            bob_connection,
            database_path.clone(),
            Duration::from_secs(1),
            shutdown.to_signal(),
        )
        .await;
    let mut bob_event_stream = bob_ts.get_event_stream();
    sleep(Duration::from_secs(5)).await;

    let (mut carol_ts, mut carol_oms, carol_comms, _carol_connectivity, key_manager_handle, carol_db) =
        setup_transaction_service(
            carol_node_identity.clone(),
            vec![alice_node_identity.clone()],
            consensus_manager,
            factories.clone(),
            carol_connection,
            database_path,
            Duration::from_secs(1),
            shutdown.to_signal(),
        )
        .await;
    let mut carol_event_stream = carol_ts.get_event_stream();

    // Establish some connections beforehand, to reduce the amount of work done concurrently in tests
    // Connect Bob and Alice
    sleep(Duration::from_secs(3)).await;

    let _peer_connection = bob_comms
        .connectivity()
        .dial_peer(alice_node_identity.node_id().clone(), false)
        .await
        .unwrap();
    sleep(Duration::from_secs(3)).await;

    // Connect alice to carol
    let _peer_connection = alice_comms
        .connectivity()
        .dial_peer(carol_node_identity.node_id().clone(), false)
        .await
        .unwrap();

    let uo2 = make_input(
        &mut OsRng,
        MicroMinotari(35000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    bob_oms.add_output(uo2.clone(), None).await.unwrap();
    bob_db
        .mark_outputs_as_unspent(vec![(uo2.hash(&bob_key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let uo3 = make_input(
        &mut OsRng,
        MicroMinotari(45000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    carol_oms.add_output(uo3.clone(), None).await.unwrap();
    carol_db
        .mark_outputs_as_unspent(vec![(uo3.hash(&key_manager_handle).await.unwrap(), true)])
        .unwrap();

    // Add some funds to Alices wallet
    let uo1a = make_input(
        &mut OsRng,
        MicroMinotari(55000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1a.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1a.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let uo1b = make_input(
        &mut OsRng,
        MicroMinotari(30000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1b.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1b.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let uo1c = make_input(
        &mut OsRng,
        MicroMinotari(30000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1c.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1c.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();

    // A series of interleaved transactions. First with Bob and Carol offline and then two with them online
    let value_a_to_b_1 = MicroMinotari::from(10000);
    let value_a_to_b_2 = MicroMinotari::from(8000);
    let value_b_to_a_1 = MicroMinotari::from(11000);
    let value_a_to_c_1 = MicroMinotari::from(14000);
    log::trace!("Sending A to B 1");
    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id_a_to_b_1 = alice_ts
        .send_transaction(
            bob_address.clone(),
            value_a_to_b_1,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "a to b 1".to_string(),
        )
        .await
        .unwrap();
    log::trace!("A to B 1 TxID: {}", tx_id_a_to_b_1);
    log::trace!("Sending A to C 1");
    let carol_address = TariAddress::new_single_address_with_interactive_only(
        carol_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id_a_to_c_1 = alice_ts
        .send_transaction(
            carol_address,
            value_a_to_c_1,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "a to c 1".to_string(),
        )
        .await
        .unwrap();
    let alice_completed_tx = alice_ts.get_completed_transactions().await.unwrap();
    assert_eq!(alice_completed_tx.len(), 0);
    log::trace!("A to C 1 TxID: {}", tx_id_a_to_c_1);

    let alice_address = TariAddress::new_single_address_with_interactive_only(
        alice_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    bob_ts
        .send_transaction(
            alice_address,
            value_b_to_a_1,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "b to a 1".to_string(),
        )
        .await
        .unwrap();
    alice_ts
        .send_transaction(
            bob_address,
            value_a_to_b_2,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "a to b 2".to_string(),
        )
        .await
        .unwrap();

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

    log::trace!("Alice received all Tx messages");

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

    let delay = sleep(Duration::from_secs(90));
    tokio::pin!(delay);

    tokio::pin!(delay);
    let mut finalized = 0;
    loop {
        tokio::select! {
            event = carol_event_stream.recv() => {
                if let TransactionEvent::ReceivedFinalizedTransaction(_) = &*event.unwrap() {
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

    let alice_pending_outbound = alice_ts.get_pending_outbound_transactions().await.unwrap();
    let alice_completed_tx = alice_ts.get_completed_transactions().await.unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 4, "Not enough transactions for Alice");
    let bob_pending_outbound = bob_ts.get_pending_outbound_transactions().await.unwrap();
    let bob_completed_tx = bob_ts.get_completed_transactions().await.unwrap();
    assert_eq!(bob_pending_outbound.len(), 0);
    assert_eq!(bob_completed_tx.len(), 3, "Not enough transactions for Bob");

    let carol_pending_inbound = carol_ts.get_pending_inbound_transactions().await.unwrap();
    let carol_completed_tx = carol_ts.get_completed_transactions().await.unwrap();
    assert_eq!(carol_pending_inbound.len(), 0);
    assert_eq!(carol_completed_tx.len(), 1);

    shutdown.trigger();
    alice_comms.wait_until_shutdown().await;
    bob_comms.wait_until_shutdown().await;
    carol_comms.wait_until_shutdown().await;
}

#[tokio::test]
async fn test_accepting_unknown_tx_id_and_malformed_reply() {
    let factories = CryptoFactories::default();
    let consensus_constants = create_consensus_constants(0);
    let temp_dir = tempdir().unwrap();
    let path_string = temp_dir.path().to_str().unwrap().to_string();
    let alice_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path, 16).unwrap();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection_alice, None).await;

    let uo = make_input(
        &mut OsRng,
        MicroMinotari(250000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;

    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            MicroMinotari::from(5000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "".to_string(),
        )
        .await
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let sender_message = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap();

    let sender = sender_message.try_into().unwrap();
    let output = create_wallet_output_from_sender_data(&sender, &alice_ts_interface.key_manager_handle).await;
    let rtp = ReceiverTransactionProtocol::new(
        sender,
        output,
        &alice_ts_interface.key_manager_handle,
        &consensus_constants,
    )
    .await;

    let mut tx_reply = rtp.get_signed_data().unwrap().clone();
    let mut wrong_tx_id = tx_reply.clone();
    wrong_tx_id.tx_id = 2u64.into();
    let (_p, pub_key) = PublicKey::random_keypair(&mut OsRng);
    tx_reply.public_spend_key = pub_key;
    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            wrong_tx_id.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            tx_reply.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn finalize_tx_with_incorrect_pubkey() {
    let factories = CryptoFactories::default();
    let key_manager = create_memory_db_key_manager().unwrap();

    let temp_dir = tempdir().unwrap();
    let path_string = temp_dir.path().to_str().unwrap().to_string();

    let alice_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let bob_db_path = format!("{}/{}", path_string, bob_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path, 16).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection_alice, None).await;
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection_bob, None).await;

    let uo = make_input(
        &mut OsRng,
        MicroMinotari(250000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    bob_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    bob_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&bob_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();
    let mut stp = bob_ts_interface
        .output_manager_service_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(5000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(25),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop).unwrap(),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    let msg = stp.build_single_round_message(&key_manager).await.unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg)).try_into().unwrap(),
        bob_node_identity.public_key(),
    );

    alice_ts_interface
        .transaction_send_message_channel
        .send(tx_message)
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(10))
        .await
        .unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let recipient_reply: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    stp.add_single_recipient_info(recipient_reply.clone(), &key_manager)
        .await
        .unwrap();
    stp.finalize(&key_manager).await.unwrap();
    let tx = stp.get_transaction().unwrap();

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: recipient_reply.tx_id.as_u64(),
        transaction: Some(tx.clone().try_into().unwrap()),
    };

    alice_ts_interface
        .transaction_finalize_message_channel
        .send(create_dummy_message(
            finalized_transaction_message,
            &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        ))
        .await
        .unwrap();

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

    assert!(alice_ts_interface
        .transaction_service_handle
        .get_completed_transaction(recipient_reply.tx_id)
        .await
        .is_err());
}

#[tokio::test]
async fn finalize_tx_with_missing_output() {
    let factories = CryptoFactories::default();
    let key_manager = create_memory_db_key_manager().unwrap();
    let temp_dir = tempdir().unwrap();
    let path_string = temp_dir.path().to_str().unwrap().to_string();

    let alice_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let alice_db_path = format!("{}/{}", path_string, alice_db_name);
    let bob_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let bob_db_path = format!("{}/{}", path_string, bob_db_name);
    let connection_alice = run_migration_and_create_sqlite_connection(&alice_db_path, 16).unwrap();
    let connection_bob = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection_alice, None).await;
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection_bob, None).await;

    let uo = make_input(
        &mut OsRng,
        MicroMinotari(250000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;

    bob_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    bob_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&bob_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let mut stp = bob_ts_interface
        .output_manager_service_handle
        .prepare_transaction_to_send(
            TxId::new_random(),
            MicroMinotari::from(5000),
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            TransactionMetadata::default(),
            "".to_string(),
            script!(Nop).unwrap(),
            Covenant::default(),
            MicroMinotari::zero(),
        )
        .await
        .unwrap();
    let msg = stp.build_single_round_message(&key_manager).await.unwrap();
    let tx_message = create_dummy_message(
        TransactionSenderMessage::Single(Box::new(msg)).try_into().unwrap(),
        bob_node_identity.public_key(),
    );

    alice_ts_interface
        .transaction_send_message_channel
        .send(tx_message)
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(10))
        .await
        .unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let recipient_reply: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    stp.add_single_recipient_info(recipient_reply.clone(), &key_manager)
        .await
        .unwrap();
    stp.finalize(&key_manager).await.unwrap();

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

    alice_ts_interface
        .transaction_finalize_message_channel
        .send(create_dummy_message(
            finalized_transaction_message,
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

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

    assert!(alice_ts_interface
        .transaction_service_handle
        .get_completed_transaction(recipient_reply.tx_id)
        .await
        .is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn discovery_async_return_test() {
    let db_tempdir = tempdir().unwrap();
    let db_folder = db_tempdir.path();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build().unwrap();
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

    let carol_connection = make_wallet_database_memory_connection();

    let (_carol_ts, _carol_oms, carol_comms, _carol_connectivity, key_manager_handle, _carol_db) =
        setup_transaction_service(
            carol_node_identity.clone(),
            vec![],
            consensus_manager.clone(),
            factories.clone(),
            carol_connection,
            db_folder.join("carol"),
            Duration::from_secs(1),
            shutdown.to_signal(),
        )
        .await;

    let alice_connection = make_wallet_database_memory_connection();

    let (mut alice_ts, mut alice_oms, alice_comms, _alice_connectivity, alice_key_manager_handle, alice_db) =
        setup_transaction_service(
            alice_node_identity,
            vec![carol_node_identity.clone()],
            consensus_manager,
            factories.clone(),
            alice_connection,
            db_folder.join("alice"),
            Duration::from_secs(20),
            shutdown.to_signal(),
        )
        .await;
    let mut alice_event_stream = alice_ts.get_event_stream();

    let uo1a = make_input(
        &mut OsRng,
        MicroMinotari(55000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1a.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1a.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let uo1b = make_input(
        &mut OsRng,
        MicroMinotari(30000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1b.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1b.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();
    let uo1c = make_input(
        &mut OsRng,
        MicroMinotari(30000),
        &OutputFeatures::default(),
        &key_manager_handle,
    )
    .await;
    alice_oms.add_output(uo1c.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1c.hash(&alice_key_manager_handle).await.unwrap(), true)])
        .unwrap();

    let initial_balance = alice_oms.get_balance().await.unwrap();

    let value_a_to_c_1 = MicroMinotari::from(14000);
    let bob_address =
        TariAddress::new_single_address_with_interactive_only(bob_node_identity.public_key().clone(), network);
    let tx_id = alice_ts
        .send_transaction(
            bob_address,
            value_a_to_c_1,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "Discovery Tx!".to_string(),
        )
        .await
        .unwrap();

    assert_ne!(initial_balance, alice_oms.get_balance().await.unwrap());

    #[allow(unused_assignments)]
    let mut found_txid = TxId::from(0u64);
    #[allow(unused_assignments)]
    let mut is_direct_send = true;
    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(tx_id, status) = (*event.unwrap()).clone() {
                    found_txid = tx_id;
                    is_direct_send = status.direct_send_result;
                    break;
                }
            },
            () = &mut delay => {
                panic!("Timeout while waiting for transaction to fail sending");
            },
        }
    }
    assert_eq!(found_txid, tx_id);
    assert!(!is_direct_send);

    let carol_address =
        TariAddress::new_single_address_with_interactive_only(carol_node_identity.public_key().clone(), network);
    let tx_id2 = alice_ts
        .send_transaction(
            carol_address,
            value_a_to_c_1,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            MicroMinotari::from(20),
            "Discovery Tx2!".to_string(),
        )
        .await
        .unwrap();

    #[allow(unused_assignments)]
    let mut success_result = false;
    #[allow(unused_assignments)]
    let mut success_tx_id = TxId::from(0u64);
    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);

    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(tx_id, status) = &*event.unwrap() {
                    success_result = status.direct_send_result;
                    success_tx_id = *tx_id;
                    break;
                }
            },
            () = &mut delay => {
                panic!("Timeout while waiting for transaction to successfully be sent");
            },
        }
    }

    assert_eq!(success_tx_id, tx_id2);
    assert!(success_result);

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

    shutdown.trigger();
    alice_comms.wait_until_shutdown().await;
    carol_comms.wait_until_shutdown().await;
}

#[tokio::test]
async fn test_power_mode_updates() {
    let factories = CryptoFactories::default();
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let tx_backend = alice_ts_interface.ts_db;

    let kernel = KernelBuilder::new()
        .with_excess(&factories.commitment.zero())
        .with_signature(Signature::default())
        .build()
        .unwrap();
    let tx = Transaction::new(
        vec![],
        vec![],
        vec![kernel],
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
    );
    let source_address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let destination_address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let completed_tx1 = CompletedTransaction {
        tx_id: 1u64.into(),
        source_address,
        destination_address,
        amount: 5000 * uT,
        fee: MicroMinotari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
        confirmations: None,
        mined_height: None,
        mined_in_block: None,
        mined_timestamp: None,
        payment_id: None,
    };

    let source_address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let destination_address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let completed_tx2 = CompletedTransaction {
        tx_id: 2u64.into(),
        source_address,
        destination_address,
        amount: 6000 * uT,
        fee: MicroMinotari::from(200),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
        confirmations: None,
        mined_height: None,
        mined_in_block: None,
        mined_timestamp: None,
        payment_id: None,
    };

    tx_backend
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            1u64.into(),
            Box::new(completed_tx1),
        )))
        .unwrap();
    tx_backend
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            2u64.into(),
            Box::new(completed_tx2),
        )))
        .unwrap();

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![alice_ts_interface.base_node_identity.to_peer()]).unwrap());

    alice_ts_interface
        .wallet_connectivity_service_mock
        .notify_base_node_set(
            BaseNodePeerManager::new(0, vec![alice_ts_interface.base_node_identity.to_peer()]).unwrap(),
        );

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::NotStored,
            best_block_hash: None,
            confirmations: 0,
            is_synced: true,
            best_block_height: 10,
            mined_timestamp: None,
        });

    let result = alice_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await;

    assert!(result.is_ok());

    // Wait for first 4 messages
    let _schnorr_signatures = alice_ts_interface
        .base_node_rpc_mock_state
        .wait_pop_transaction_query_calls(4, Duration::from_secs(20))
        .await
        .unwrap();

    alice_ts_interface
        .transaction_service_handle
        .set_low_power_mode()
        .await
        .unwrap();
    // expect 4 messages more
    let _schnorr_signatures = alice_ts_interface
        .base_node_rpc_mock_state
        .wait_pop_transaction_query_calls(4, Duration::from_secs(60))
        .await
        .unwrap();

    alice_ts_interface
        .transaction_service_handle
        .set_normal_power_mode()
        .await
        .unwrap();
    // and 4 more
    let _schnorr_signatures = alice_ts_interface
        .base_node_rpc_mock_state
        .wait_pop_transaction_query_calls(4, Duration::from_secs(60))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_set_num_confirmations() {
    let factories = CryptoFactories::default();

    let connection = make_wallet_database_memory_connection();

    let mut ts_interface = setup_transaction_service_no_comms(
        factories,
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    )
    .await;

    let num_confirmations_required = ts_interface
        .transaction_service_handle
        .get_num_confirmations_required()
        .await
        .unwrap();
    assert_eq!(
        num_confirmations_required,
        TransactionServiceConfig::default().num_confirmations_required
    );

    for number in 1..10 {
        ts_interface
            .transaction_service_handle
            .set_num_confirmations_required(number)
            .await
            .unwrap();

        let num_confirmations_required = ts_interface
            .transaction_service_handle
            .get_num_confirmations_required()
            .await
            .unwrap();
        assert_eq!(num_confirmations_required, number);
    }
}

#[tokio::test]
async fn test_transaction_cancellation() {
    let factories = CryptoFactories::default();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    )
    .await;
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let alice_total_available = 2500000 * uT;
    let uo = make_input(
        &mut OsRng,
        alice_total_available,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent = 100000 * uT;
    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(_,_) = &*event.unwrap() {
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }

    for i in 0..=12 {
        match alice_ts_interface
            .transaction_service_handle
            .get_pending_outbound_transactions()
            .await
            .unwrap()
            .remove(&tx_id)
        {
            None => (),
            Some(_) => break,
        }
        sleep(Duration::from_secs(5)).await;
        if i >= 12 {
            panic!("Pending outbound transaction should have been added by now");
        }
    }

    let _result = alice_ts_interface.outbound_service_mock_state.take_calls().await;

    alice_ts_interface
        .transaction_service_handle
        .cancel_transaction(tx_id)
        .await
        .unwrap();

    // Wait for cancellation event, in an effort to nail down where the issue is for the flakey CI test
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
    // We expect 1 sent direct and via SAF
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("alice call wait 1");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let alice_cancel_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_cancel_message.tx_id, tx_id.as_u64(), "DIRECT");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let alice_cancel_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_cancel_message.tx_id, tx_id.as_u64(), "SAF");

    assert!(alice_ts_interface
        .transaction_service_handle
        .get_pending_outbound_transactions()
        .await
        .unwrap()
        .remove(&tx_id)
        .is_none());

    let key_manager = create_memory_db_key_manager().unwrap();
    let input = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&key_manager).await,
        MicroMinotari::from(100_000),
        &key_manager,
    )
    .await
    .unwrap();

    let constants = create_consensus_constants(0);
    let key_manager = create_memory_db_key_manager().unwrap();
    let mut builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
    let amount = MicroMinotari::from(10_000);
    let change = TestParams::new(&key_manager).await;
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari::from(5))
        .with_message("Yo!".to_string())
        .with_input(input)
        .await
        .unwrap()
        .with_change_data(
            script!(Nop).unwrap(),
            inputs!(change.script_key_pk),
            change.script_key_id.clone(),
            change.commitment_mask_key_id.clone(),
            Covenant::default(),
            TariAddress::default(),
        )
        .with_recipient_data(
            script!(Nop).unwrap(),
            Default::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            amount,
        )
        .await
        .unwrap();

    let mut stp = builder.build().await.unwrap();
    let tx_sender_msg = stp.build_single_round_message(&key_manager).await.unwrap();
    let tx_id2 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.try_into().unwrap());
    alice_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(proto_message, bob_node_identity.public_key()))
        .await
        .unwrap();

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

    alice_ts_interface
        .transaction_service_handle
        .get_pending_inbound_transactions()
        .await
        .unwrap()
        .remove(&tx_id2)
        .expect("Pending Transaction 2 should be in list");

    alice_ts_interface
        .transaction_service_handle
        .cancel_transaction(tx_id2)
        .await
        .unwrap();

    assert!(alice_ts_interface
        .transaction_service_handle
        .get_pending_inbound_transactions()
        .await
        .unwrap()
        .remove(&tx_id2)
        .is_none());

    // Lets cancel the last one using a Comms stack message
    let input = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&key_manager.clone()).await,
        MicroMinotari::from(100_000),
        &key_manager.clone(),
    )
    .await
    .unwrap();
    let constants = create_consensus_constants(0);
    let mut builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
    let amount = MicroMinotari::from(10_000);
    let change = TestParams::new(&key_manager).await;
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari::from(5))
        .with_message("Yo!".to_string())
        .with_input(input)
        .await
        .unwrap()
        .with_change_data(
            script!(Nop).unwrap(),
            inputs!(change.script_key_pk),
            change.script_key_id.clone(),
            change.commitment_mask_key_id.clone(),
            Covenant::default(),
            TariAddress::default(),
        )
        .with_recipient_data(
            script!(Nop).unwrap(),
            Default::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            amount,
        )
        .await
        .unwrap();

    let mut stp = builder.build().await.unwrap();
    let tx_sender_msg = stp.build_single_round_message(&key_manager).await.unwrap();
    let tx_id3 = tx_sender_msg.tx_id;
    let proto_message = proto::TransactionSenderMessage::single(tx_sender_msg.try_into().unwrap());
    alice_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(proto_message, bob_node_identity.public_key()))
        .await
        .unwrap();

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

    alice_ts_interface
        .transaction_service_handle
        .get_pending_inbound_transactions()
        .await
        .unwrap()
        .remove(&tx_id3)
        .expect("Pending Transaction 3 should be in list");

    let proto_message = proto::TransactionCancelledMessage { tx_id: tx_id3.as_u64() };
    // Sent from the wrong source address so should not cancel
    alice_ts_interface
        .transaction_cancelled_message_channel
        .send(create_dummy_message(
            proto_message,
            &PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        ))
        .await
        .unwrap();

    sleep(Duration::from_secs(5)).await;

    alice_ts_interface
        .transaction_service_handle
        .get_pending_inbound_transactions()
        .await
        .unwrap()
        .remove(&tx_id3)
        .expect("Pending Transaction 3 should be in list");

    let proto_message = proto::TransactionCancelledMessage { tx_id: tx_id3.as_u64() };
    alice_ts_interface
        .transaction_cancelled_message_channel
        .send(create_dummy_message(proto_message, bob_node_identity.public_key()))
        .await
        .unwrap();

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

    assert!(alice_ts_interface
        .transaction_service_handle
        .get_pending_inbound_transactions()
        .await
        .unwrap()
        .remove(&tx_id3)
        .is_none());
}
#[tokio::test]
async fn test_direct_vs_saf_send_of_tx_reply_and_finalize() {
    let factories = CryptoFactories::default();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;

    let alice_total_available = 2500000 * uT;
    let uo = make_input(
        &mut OsRng,
        alice_total_available,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent = 100000 * uT;
    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .unwrap();

    let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

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
    let connection = make_wallet_database_memory_connection();

    // Test sending the Reply to a receiver with Direct and then with SAF and never both
    let mut bob_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    )
    .await;

    bob_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Queued,
            broadcast: ResponseType::Failed,
        })
        .await;

    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            tx_sender_msg.clone().try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();

    let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let _recipient_signed_message: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    sleep(Duration::from_secs(5)).await;
    assert_eq!(
        bob_ts_interface.outbound_service_mock_state.call_count().await,
        0,
        "Should be no more calls"
    );
    let connection = make_wallet_database_memory_connection();

    let mut bob2_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            broadcast_monitoring_timeout: Duration::from_secs(20),
            chain_monitoring_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    )
    .await;
    bob2_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Failed,
            broadcast: ResponseType::Queued,
        })
        .await;

    bob2_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            tx_sender_msg.try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();

    bob2_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();

    let (_, body) = bob2_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_reply_msg: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    sleep(Duration::from_secs(5)).await;
    assert_eq!(
        bob2_ts_interface.outbound_service_mock_state.call_count().await,
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
        })
        .await;

    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            tx_reply_msg.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    let _size = alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .unwrap();
    let _result = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let _result = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    sleep(Duration::from_secs(5)).await;
    assert_eq!(
        alice_ts_interface.outbound_service_mock_state.call_count().await,
        0,
        "Should be no more calls"
    );

    // Now to repeat sending so we can test the SAF send of the finalize message
    let alice_total_available = 250000 * uT;
    let uo = make_input(
        &mut OsRng,
        alice_total_available,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent = 20000 * uT;

    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let _tx_id2 = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .unwrap();

    let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let tx_sender_msg: TransactionSenderMessage = envelope_body
        .decode_part::<proto::TransactionSenderMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            tx_sender_msg.try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();

    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();

    let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

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
        })
        .await;

    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            tx_reply_msg.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    let _size = alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await;

    assert_eq!(alice_ts_interface.outbound_service_mock_state.call_count().await, 1);
    let _result = alice_ts_interface.outbound_service_mock_state.pop_call().await;
    sleep(Duration::from_secs(5)).await;
    assert_eq!(
        alice_ts_interface.outbound_service_mock_state.call_count().await,
        0,
        "Should be no more calls2"
    );
}

#[tokio::test]
async fn test_tx_direct_send_behaviour() {
    let factories = CryptoFactories::default();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    let uo = make_input(
        &mut OsRng,
        1000000 * uT,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();
    let uo = make_input(
        &mut OsRng,
        1000000 * uT,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();
    let uo = make_input(
        &mut OsRng,
        1000000 * uT,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();
    let uo = make_input(
        &mut OsRng,
        1000000 * uT,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent = 100000 * uT;

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::Failed,
            broadcast: ResponseType::Failed,
        })
        .await;

    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let _tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address.clone(),
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message1".to_string(),
        )
        .await
        .unwrap();
    let mut transaction_send_status = TransactionSendStatus::default();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(_, status) = &*event.unwrap() {
                    transaction_send_status = status.clone();
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(!transaction_send_status.direct_send_result, "Should be 1 failed direct");
    assert!(
        !transaction_send_status.store_and_forward_send_result,
        "Should be 1 failed saf"
    );
    assert!(transaction_send_status.queued_for_retry, "Should be 1 queued");

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::QueuedFail,
            broadcast: ResponseType::Queued,
        })
        .await;

    let _tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address.clone(),
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message2".to_string(),
        )
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(_, status) = &*event.unwrap() {
                    transaction_send_status = status.clone();
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(!transaction_send_status.direct_send_result, "Should be 1 failed direct");
    assert!(
        transaction_send_status.store_and_forward_send_result,
        "Should be 1 succeed saf"
    );
    assert!(!transaction_send_status.queued_for_retry, "Should be 0 queued");

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(1)),
            broadcast: ResponseType::QueuedFail,
        })
        .await;

    let _tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address.clone(),
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message3".to_string(),
        )
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(_, status) = &*event.unwrap() {
                    transaction_send_status = status.clone();
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(transaction_send_status.direct_send_result, "Should be 1 succeed direct");
    assert!(
        !transaction_send_status.store_and_forward_send_result,
        "Should be 1 failed saf"
    );
    assert!(!transaction_send_status.queued_for_retry, "Should be 0 queued");

    alice_ts_interface
        .outbound_service_mock_state
        .set_behaviour(MockBehaviour {
            direct: ResponseType::QueuedSuccessDelay(Duration::from_secs(30)),
            broadcast: ResponseType::Queued,
        })
        .await;

    let _tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message4".to_string(),
        )
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(60))
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => {
                if let TransactionEvent::TransactionSendResult(_, status) = &*event.unwrap() {
                    transaction_send_status = status.clone();
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(!transaction_send_status.direct_send_result, "Should be 1 failed direct");
    assert!(
        transaction_send_status.store_and_forward_send_result,
        "Should be 1 succeed saf"
    );
    assert!(!transaction_send_status.queued_for_retry, "Should be 0 queued");
}

#[tokio::test]
async fn test_restarting_transaction_protocols() {
    let network = Network::LocalNet;
    let factories = CryptoFactories::default();
    let alice_connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), alice_connection, None).await;

    let alice_backend = alice_ts_interface.ts_db;

    let bob_connection = make_wallet_database_memory_connection();
    let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), bob_connection, None).await;

    let bob_backend = bob_ts_interface.ts_db;

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
    let input = make_input(
        &mut OsRng,
        MicroMinotari(2000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    let constants = create_consensus_constants(0);
    let fee_calc = Fee::new(*constants.transaction_weight_params());
    let key_manager = create_memory_db_key_manager().unwrap();
    let mut builder = SenderTransactionProtocol::builder(constants.clone(), key_manager.clone());
    let fee = fee_calc.calculate(MicroMinotari(4), 1, 1, 1, 0);
    let change = TestParams::new(&key_manager).await;
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari(4))
        .with_input(input)
        .await
        .unwrap()
        .with_recipient_data(
            script!(Nop).unwrap(),
            Default::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            MicroMinotari(2000) - fee - MicroMinotari(10),
        )
        .await
        .unwrap()
        .with_change_data(
            script!(Nop).unwrap(),
            inputs!(change.script_key_pk),
            change.script_key_id.clone(),
            change.commitment_mask_key_id.clone(),
            Covenant::default(),
            TariAddress::default(),
        );
    let mut bob_stp = builder.build().await.unwrap();
    let msg = bob_stp.build_single_round_message(&key_manager).await.unwrap();
    let bob_pre_finalize = bob_stp.clone();

    let tx_id = msg.tx_id;

    let sender_info = TransactionSenderMessage::Single(Box::new(msg.clone()));

    let output = create_wallet_output_from_sender_data(&sender_info, &key_manager).await;
    let receiver_protocol = ReceiverTransactionProtocol::new(sender_info, output, &key_manager, &constants).await;

    let alice_reply = receiver_protocol.get_signed_data().unwrap().clone();

    bob_stp
        .add_single_recipient_info(alice_reply.clone(), &key_manager)
        .await
        .unwrap();

    match bob_stp.finalize(&key_manager).await {
        Ok(_) => (),
        Err(e) => panic!("Should be able to finalize tx: {}", e),
    };
    let tx = bob_stp.get_transaction().unwrap().clone();

    let bob_view_key = bob_ts_interface.key_manager_handle.get_view_key().await.unwrap();
    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key.pub_key,
        bob_identity.public_key().clone(),
        network,
    );
    let inbound_tx = InboundTransaction {
        tx_id,
        source_address: bob_address,
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
    let alice_view_key = alice_ts_interface.key_manager_handle.get_view_key().await.unwrap();
    let alice_address = TariAddress::new_dual_address_with_default_features(
        alice_view_key.pub_key,
        alice_identity.public_key().clone(),
        network,
    );
    let outbound_tx = OutboundTransaction {
        tx_id,
        destination_address: alice_address,
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
    let mut bob_event_stream = bob_ts_interface.transaction_service_handle.get_event_stream();

    bob_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![base_node_identity.to_peer()]).unwrap());
    assert!(bob_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .is_ok());

    bob_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            alice_reply.try_into().unwrap(),
            alice_identity.public_key(),
        ))
        .await
        .unwrap();

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

    // Test Alice's node restarts the receive protocol
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![base_node_identity.to_peer()]).unwrap());

    assert!(alice_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .is_ok());

    let finalized_transaction_message = proto::TransactionFinalizedMessage {
        tx_id: tx_id.as_u64(),
        transaction: Some(tx.try_into().unwrap()),
    };

    alice_ts_interface
        .transaction_finalize_message_channel
        .send(create_dummy_message(
            finalized_transaction_message,
            bob_identity.public_key(),
        ))
        .await
        .unwrap();

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
}

#[tokio::test]
async fn test_transaction_resending() {
    let factories = CryptoFactories::default();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // Setup Alice wallet with no comms stack
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(20),
            resend_response_cooldown: Duration::from_secs(10),
            ..Default::default()
        }),
    )
    .await;

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let uo = make_input(
        &mut OsRng,
        alice_total_available,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent = 100000 * uT;

    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();

    // Check that there were repeats
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Alice call wait 1");

    let mut alice_sender_message = TransactionSenderMessage::None;
    for _ in 0..2 {
        let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
        alice_sender_message = try_decode_sender_message(call.1.to_vec().clone()).unwrap();
        if let TransactionSenderMessage::Single(data) = alice_sender_message.clone() {
            assert_eq!(data.tx_id, tx_id);
        } else {
            panic!("Should be a Single Transaction Sender Message")
        }
    }

    // Setup Bob's wallet with no comms stack
    let connection = make_wallet_database_memory_connection();

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        factories,
        connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(20),
            resend_response_cooldown: Duration::from_secs(10),
            ..Default::default()
        }),
    )
    .await;

    // Pass sender message to Bob's wallet
    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            alice_sender_message.clone().try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();

    // Check that the reply was repeated
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Bob call wait 1");

    let mut bob_reply_message;
    for _ in 0..2 {
        let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
        bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec().clone()).unwrap();
        assert_eq!(bob_reply_message.tx_id, tx_id);
    }

    sleep(Duration::from_secs(2)).await;
    // See if sending a second message too soon is ignored
    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            alice_sender_message.clone().try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();

    assert!(bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(2))
        .await
        .is_err());

    // Wait for the cooldown to expire but before the resend period has elapsed see if a repeat illicits a response.
    sleep(Duration::from_secs(8)).await;
    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            alice_sender_message.try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Bob call wait 2");
    let _result = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_reply_message.tx_id, tx_id);

    let _result = alice_ts_interface.outbound_service_mock_state.take_calls().await;

    // Send the reply to Alice
    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            bob_reply_message.clone().try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Alice call wait 2");

    let _result = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let alice_finalize_message = try_decode_finalized_transaction_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_finalize_message.tx_id, tx_id.as_u64());

    // See if sending a second message before cooldown and see if it is ignored
    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            bob_reply_message.clone().try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    assert!(alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(8))
        .await
        .is_err());

    // Wait for the cooldown to expire but before the resend period has elapsed see if a repeat illicts a response.
    sleep(Duration::from_secs(6)).await;

    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            bob_reply_message.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .await
        .expect("Alice call wait 3");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let alice_finalize_message = try_decode_finalized_transaction_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_finalize_message.tx_id, tx_id);
}
// This test fails on the code coverage, so disabling.
#[ignore]
#[tokio::test]
async fn test_resend_on_startup() {
    // Test that messages are resent on startup if enough time has passed
    let factories = CryptoFactories::default();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    // First we will check the Send Tranasction message
    let key_manager = create_memory_db_key_manager().unwrap();
    let input = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&key_manager).await,
        MicroMinotari::from(100_000),
        &key_manager,
    )
    .await
    .unwrap();
    let constants = create_consensus_constants(0);
    let key_manager = create_memory_db_key_manager().unwrap();
    let mut builder = SenderTransactionProtocol::builder(constants.clone(), key_manager.clone());
    let amount = MicroMinotari::from(10_000);
    let change = TestParams::new(&key_manager).await;
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari::from(177 / 5))
        .with_message("Yo!".to_string())
        .with_input(input)
        .await
        .unwrap()
        .with_change_data(
            script!(Nop).unwrap(),
            inputs!(change.script_key_pk),
            change.script_key_id.clone(),
            change.commitment_mask_key_id.clone(),
            Covenant::default(),
            TariAddress::default(),
        )
        .with_recipient_data(
            script!(Nop).unwrap(),
            Default::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            amount,
        )
        .await
        .unwrap();

    let mut stp = builder.build().await.unwrap();
    let stp_msg = stp.build_single_round_message(&key_manager).await.unwrap();
    let tx_sender_msg = TransactionSenderMessage::Single(Box::new(stp_msg));

    let tx_id = stp.get_tx_id().unwrap();
    let address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let mut outbound_tx = OutboundTransaction {
        tx_id,
        destination_address: address,
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
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    )
    .await;

    let alice_backend = alice_ts_interface.ts_db;
    alice_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx.clone()),
        )))
        .unwrap();

    // Need to set something for alices base node, doesn't matter what
    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![alice_node_identity.to_peer()]).unwrap());

    assert!(alice_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .is_ok());

    alice_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .unwrap();

    // Check that if the cooldown is not done that a message will not be sent.
    assert!(alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(5))
        .await
        .is_err());

    // Now we do it again with the timestamp prior to the cooldown and see that a message is sent
    outbound_tx.send_count = 1;
    outbound_tx.last_send_timestamp = Utc::now().naive_utc().checked_sub_signed(ChronoDuration::seconds(20));

    let connection2 = make_wallet_database_memory_connection();

    let mut alice2_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        connection2,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    )
    .await;

    let alice_backend2 = alice2_ts_interface.ts_db;

    alice_backend2
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx),
        )))
        .unwrap();

    // Need to set something for alices base node, doesn't matter what
    alice2_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![alice_node_identity.to_peer()]).unwrap());

    assert!(alice2_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .is_ok());
    assert!(alice2_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .is_ok());

    // Check for resend on startup
    alice2_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .await
        .expect("Carol call wait 1");

    let call = alice2_ts_interface
        .outbound_service_mock_state
        .pop_call()
        .await
        .unwrap();

    if let TransactionSenderMessage::Single(data) = try_decode_sender_message(call.1.to_vec()).unwrap() {
        assert_eq!(data.tx_id, tx_id);
    } else {
        panic!("Should be a Single Transaction Sender Message")
    }

    // Now we do this for the Transaction Reply
    let output = create_wallet_output_from_sender_data(&tx_sender_msg, &alice2_ts_interface.key_manager_handle).await;
    let rtp = ReceiverTransactionProtocol::new(
        tx_sender_msg,
        output,
        &alice2_ts_interface.key_manager_handle,
        &constants,
    )
    .await;
    let address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let mut inbound_tx = InboundTransaction {
        tx_id,
        source_address: address,
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
    let bob_connection = make_wallet_database_memory_connection();

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        bob_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    )
    .await;

    let bob_backend = bob_ts_interface.ts_db;

    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx.clone()),
        )))
        .unwrap();

    // Need to set something for bobs base node, doesn't matter what
    bob_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![alice_node_identity.to_peer()]).unwrap());

    assert!(bob_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .is_ok());
    assert!(bob_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .is_ok());

    // Check that if the cooldown is not done that a message will not be sent.
    assert!(bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(5))
        .await
        .is_err());

    // Now we do it again with the timestamp prior to the cooldown and see that a message is sent
    inbound_tx.send_count = 1;
    inbound_tx.last_send_timestamp = Utc::now().naive_utc().checked_sub_signed(ChronoDuration::seconds(20));
    let bob_connection2 = make_wallet_database_memory_connection();

    let mut bob2_ts_interface = setup_transaction_service_no_comms(
        factories,
        bob_connection2,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            ..Default::default()
        }),
    )
    .await;

    let bob_backend2 = bob2_ts_interface.ts_db;
    bob_backend2
        .write(WriteOperation::Insert(DbKeyValuePair::PendingInboundTransaction(
            tx_id,
            Box::new(inbound_tx),
        )))
        .unwrap();

    // Need to set something for bobs base node, doesn't matter what
    bob2_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![alice_node_identity.to_peer()]).unwrap());

    assert!(bob2_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .is_ok());
    assert!(bob2_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .is_ok());
    // Check for resend on startup

    bob2_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .await
        .expect("Dave call wait 1");

    let call = bob2_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let reply = try_decode_transaction_reply_message(call.1.to_vec()).unwrap();
    assert_eq!(reply.tx_id, tx_id);
}

#[tokio::test]
async fn test_replying_to_cancelled_tx() {
    let factories = CryptoFactories::default();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // Testing if a Tx Reply is received for a Cancelled Outbound Tx that a Cancelled message is sent back:
    let alice_connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        alice_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(20),
            ..Default::default()
        }),
    )
    .await;

    // Send a transaction to Bob
    let alice_total_available = 2500000 * uT;
    let uo = make_input(
        &mut OsRng,
        alice_total_available,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();
    let amount_sent = 100000 * uT;
    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .await
        .expect("Alice call wait 1");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let alice_sender_message = try_decode_sender_message(call.1.to_vec()).unwrap();
    if let TransactionSenderMessage::Single(data) = alice_sender_message.clone() {
        assert_eq!(data.tx_id, tx_id);
    }
    // Need a moment for Alice's wallet to finish writing to its database before cancelling
    sleep(Duration::from_secs(5)).await;

    alice_ts_interface
        .transaction_service_handle
        .cancel_transaction(tx_id)
        .await
        .unwrap();

    // Setup Bob's wallet with no comms stack
    let bob_connection = make_wallet_database_memory_connection();

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        factories,
        bob_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    )
    .await;

    // Pass sender message to Bob's wallet
    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            alice_sender_message.try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .await
        .expect("Bob call wait 1");

    let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let bob_reply_message = try_decode_transaction_reply_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_reply_message.tx_id, tx_id);

    // Wait for cooldown to expire
    sleep(Duration::from_secs(5)).await;

    let _result = alice_ts_interface.outbound_service_mock_state.take_calls().await;

    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            bob_reply_message.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(1, Duration::from_secs(30))
        .await
        .expect("Alice call wait 2");

    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let alice_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(alice_cancelled_message.tx_id, tx_id.as_u64());
}

#[tokio::test]
async fn test_transaction_timeout_cancellation() {
    let factories = CryptoFactories::default();

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // Testing if a Tx Reply is received for a Cancelled Outbound Tx that a Cancelled message is sent back:
    let alice_connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        alice_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    )
    .await;

    // Send a transaction to Bob
    let alice_total_available = 250000 * uT;
    let uo = make_input(
        &mut OsRng,
        alice_total_available,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent = 10000 * uT;

    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    let tx_id = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();

    // For testing the resend period is set to 10 seconds and the timeout period is set to 15 seconds so we are going
    // to wait for 3 messages The intial send, the resend and then the cancellation
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(5, Duration::from_secs(60))
        .await
        .expect("Alice call wait 1");

    let calls = alice_ts_interface.outbound_service_mock_state.take_calls().await;

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
    let key_manager = create_memory_db_key_manager().unwrap();
    let input = create_wallet_output_with_data(
        script!(Nop).unwrap(),
        OutputFeatures::default(),
        &TestParams::new(&key_manager).await,
        MicroMinotari::from(100_000),
        &key_manager,
    )
    .await
    .unwrap();
    let constants = create_consensus_constants(0);
    let key_manager = create_memory_db_key_manager().unwrap();
    let mut builder = SenderTransactionProtocol::builder(constants, key_manager.clone());
    let amount = MicroMinotari::from(10_000);
    let change = TestParams::new(&key_manager).await;
    builder
        .with_lock_height(0)
        .with_fee_per_gram(MicroMinotari::from(177 / 5))
        .with_message("Yo!".to_string())
        .with_input(input)
        .await
        .unwrap()
        .with_change_data(
            script!(Nop).unwrap(),
            inputs!(change.script_key_pk),
            change.script_key_id.clone(),
            change.commitment_mask_key_id.clone(),
            Covenant::default(),
            TariAddress::default(),
        )
        .with_recipient_data(
            script!(Nop).unwrap(),
            Default::default(),
            Covenant::default(),
            MicroMinotari::zero(),
            amount,
        )
        .await
        .unwrap();

    let mut stp = builder.build().await.unwrap();
    let stp_msg = stp.build_single_round_message(&key_manager).await.unwrap();
    let tx_sender_msg = TransactionSenderMessage::Single(Box::new(stp_msg));

    let tx_id = stp.get_tx_id().unwrap();
    let address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let outbound_tx = OutboundTransaction {
        tx_id,
        destination_address: address,
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
    let bob_connection = make_wallet_database_memory_connection();

    let mut bob_ts_interface = setup_transaction_service_no_comms(
        factories.clone(),
        bob_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    )
    .await;

    let bob_backend = bob_ts_interface.ts_db;
    bob_backend
        .write(WriteOperation::Insert(DbKeyValuePair::PendingOutboundTransaction(
            tx_id,
            Box::new(outbound_tx),
        )))
        .unwrap();

    // Need to set something for bobs base node, doesn't matter what
    bob_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![bob_node_identity.to_peer()]).unwrap());
    assert!(bob_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .is_ok());
    assert!(bob_ts_interface
        .transaction_service_handle
        .restart_transaction_protocols()
        .await
        .is_ok());

    // Make sure we receive this before the timeout as it should be sent immediately on startup
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(14))
        .await
        .expect("Bob call wait 1");
    let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let bob_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_cancelled_message.tx_id, tx_id.as_u64());

    let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let bob_cancelled_message = try_decode_transaction_cancelled_message(call.1.to_vec()).unwrap();
    assert_eq!(bob_cancelled_message.tx_id, tx_id.as_u64());
    let carol_connection = make_wallet_database_memory_connection();

    // Now to do this for the Receiver
    let mut carol_ts_interface = setup_transaction_service_no_comms(
        factories,
        carol_connection,
        Some(TransactionServiceConfig {
            transaction_resend_period: Duration::from_secs(10),
            resend_response_cooldown: Duration::from_secs(5),
            pending_transaction_cancellation_timeout: Duration::from_secs(15),
            ..Default::default()
        }),
    )
    .await;
    let mut carol_event_stream = carol_ts_interface.transaction_service_handle.get_event_stream();

    carol_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            tx_sender_msg.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

    // Then we should get 2 reply messages and 1 cancellation event
    carol_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Carol call wait 1");

    let calls = carol_ts_interface.outbound_service_mock_state.take_calls().await;

    // Initial Reply
    let carol_reply_message = try_decode_transaction_reply_message(calls[0].1.to_vec()).unwrap();
    assert_eq!(carol_reply_message.tx_id, tx_id);

    // Resend
    let carol_reply_message = try_decode_transaction_reply_message(calls[1].1.to_vec()).unwrap();
    assert_eq!(carol_reply_message.tx_id, tx_id);

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
}

/// This test will check that the Transaction Service starts the tx broadcast protocol correctly and reacts correctly
/// to a tx being broadcast and to a tx being rejected.
#[tokio::test]
async fn transaction_service_tx_broadcast() {
    let factories = CryptoFactories::default();

    let alice_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let bob_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    alice_ts_interface
        .wallet_connectivity_service_mock
        .set_base_node(BaseNodePeerManager::new(0, vec![alice_ts_interface.base_node_identity.to_peer()]).unwrap());

    let connection2 = make_wallet_database_memory_connection();
    let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection2, None).await;

    let alice_output_value = MicroMinotari(250000);

    let uo = make_input(
        &mut OsRng,
        alice_output_value,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let uo2 = make_input(
        &mut OsRng,
        alice_output_value,
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    alice_ts_interface
        .output_manager_service_handle
        .add_output(uo2.clone(), None)
        .await
        .unwrap();
    alice_ts_interface
        .oms_db
        .mark_outputs_as_unspent(vec![(
            uo2.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )])
        .unwrap();

    let amount_sent1 = 100000 * uT;

    let bob_address = TariAddress::new_single_address_with_interactive_only(
        bob_node_identity.public_key().clone(),
        Network::LocalNet,
    );
    // Send Tx1
    let tx_id1 = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address.clone(),
            amount_sent1,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            100 * uT,
            "Testing Message".to_string(),
        )
        .await
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Alice call wait 1");
    let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

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

    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            tx_sender_msg.try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("bob call wait 1");

    let _result = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(&mut call.1.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg1: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    // Send Tx2
    let amount_sent2 = 100001 * uT;
    let tx_id2 = alice_ts_interface
        .transaction_service_handle
        .send_transaction(
            bob_address,
            amount_sent2,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20 * uT,
            "Testing Message2".to_string(),
        )
        .await
        .unwrap();
    alice_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Alice call wait 2");

    let _result = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let tx_sender_msg = try_decode_sender_message(call.1.to_vec()).unwrap();

    match tx_sender_msg {
        TransactionSenderMessage::Single(_) => (),
        _ => {
            panic!("Transaction is the not a single rounder sender variant");
        },
    };

    bob_ts_interface
        .transaction_send_message_channel
        .send(create_dummy_message(
            tx_sender_msg.try_into().unwrap(),
            alice_node_identity.public_key(),
        ))
        .await
        .unwrap();
    bob_ts_interface
        .outbound_service_mock_state
        .wait_call_count(2, Duration::from_secs(60))
        .await
        .expect("Bob call wait 2");

    let (_, _body) = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    let bob_tx_reply_msg2: RecipientSignedMessage = envelope_body
        .decode_part::<proto::RecipientSignedMessage>(1)
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();

    let balance = alice_ts_interface
        .output_manager_service_handle
        .get_balance()
        .await
        .unwrap();
    assert_eq!(balance.available_balance, MicroMinotari(0));

    // Give Alice the first of tx reply to start the broadcast process.
    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            bob_tx_reply_msg1.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

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

    let alice_completed_tx1 = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions()
        .await
        .unwrap()
        .remove(&tx_id1)
        .expect("Transaction must be in collection");

    let tx1_fee = alice_completed_tx1.fee;

    assert!(
        alice_completed_tx1.status == TransactionStatus::Completed ||
            alice_completed_tx1.status == TransactionStatus::Broadcast
    );

    let _transactions = alice_ts_interface
        .base_node_rpc_mock_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(30))
        .await
        .expect("Should receive a tx submission");
    let _schnorr_signatures = alice_ts_interface
        .base_node_rpc_mock_state
        .wait_pop_transaction_query_calls(1, Duration::from_secs(30))
        .await
        .expect("Should receive a tx query");

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::Mined,
            best_block_hash: None,
            confirmations: TransactionServiceConfig::default().num_confirmations_required,
            is_synced: true,
            best_block_height: 0,
            mined_timestamp: None,
        });

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

    alice_ts_interface
        .transaction_ack_message_channel
        .send(create_dummy_message(
            bob_tx_reply_msg2.try_into().unwrap(),
            bob_node_identity.public_key(),
        ))
        .await
        .unwrap();

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
            best_block_hash: None,
            confirmations: TransactionServiceConfig::default().num_confirmations_required,
            is_synced: true,
            best_block_height: 0,
            mined_timestamp: None,
        });

    let alice_completed_tx2 = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions()
        .await
        .unwrap()
        .remove(&tx_id2)
        .expect("Transaction must be in collection");

    assert!(
        alice_completed_tx2.status == TransactionStatus::Completed ||
            alice_completed_tx2.status == TransactionStatus::Broadcast
    );

    let _transactions = alice_ts_interface
        .base_node_rpc_mock_state
        .wait_pop_submit_transaction_calls(1, Duration::from_secs(30))
        .await
        .expect("Should receive a tx submission");

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

    // Check that the cancelled Tx value + change from tx1 is available
    let balance = alice_ts_interface
        .output_manager_service_handle
        .get_balance()
        .await
        .unwrap();

    assert_eq!(
        balance.pending_incoming_balance,
        alice_output_value - amount_sent1 - tx1_fee
    );
    assert_eq!(balance.available_balance, alice_output_value);
}

#[tokio::test]
async fn broadcast_all_completed_transactions_on_startup() {
    let factories = CryptoFactories::default();
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let db = alice_ts_interface.ts_db.clone();

    let kernel = KernelBuilder::new()
        .with_excess(&factories.commitment.zero())
        .with_signature(Signature::default())
        .build()
        .unwrap();

    let tx = Transaction::new(
        vec![],
        vec![],
        vec![kernel],
        PrivateKey::random(&mut OsRng),
        PrivateKey::random(&mut OsRng),
    );
    let source_address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let destination_address = TariAddress::new_dual_address_with_default_features(
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        Network::LocalNet,
    );
    let completed_tx1 = CompletedTransaction {
        tx_id: 1u64.into(),
        source_address,
        destination_address,
        amount: 5000 * uT,
        fee: MicroMinotari::from(20),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Yo!".to_string(),
        timestamp: Utc::now().naive_utc(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx.first_kernel_excess_sig().unwrap_or(&Signature::default()).clone(),
        confirmations: None,
        mined_height: None,
        mined_in_block: None,
        mined_timestamp: None,
        payment_id: None,
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2u64.into(),
        status: TransactionStatus::MinedConfirmed,
        ..completed_tx1.clone()
    };

    let completed_tx3 = CompletedTransaction {
        tx_id: 3u64.into(),
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

    alice_ts_interface
        .base_node_rpc_mock_state
        .set_transaction_query_response(TxQueryResponse {
            location: TxLocation::Mined,
            best_block_hash: None,
            confirmations: TransactionServiceConfig::default().num_confirmations_required,
            is_synced: true,
            best_block_height: 0,
            mined_timestamp: None,
        });

    // Note: The event stream has to be assigned before the broadcast protocol is restarted otherwise the events will be
    // dropped
    let mut event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();
    alice_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .unwrap();
    assert!(alice_ts_interface
        .transaction_service_handle
        .restart_broadcast_protocols()
        .await
        .is_ok());

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    let mut found1 = false;
    let mut found2 = false;
    let mut found3 = false;
    loop {
        tokio::select! {
            event = event_stream.recv() => {
                if let TransactionEvent::TransactionBroadcast(tx_id) = (*event.unwrap()).clone() {
                    if tx_id == 1u64 {
                        found1 = true
                    }
                    if tx_id == 2u64 {
                        found2 = true
                    }
                    if tx_id == 3u64 {
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
}

#[tokio::test]
async fn test_update_faux_tx_on_oms_validation() {
    let factories = CryptoFactories::default();

    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let alice_address = TariAddress::new_single_address_with_interactive_only(
        alice_ts_interface.base_node_identity.public_key().clone(),
        Network::LocalNet,
    );

    let uo_1 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    let uo_2 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(20000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    let uo_3 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(30000),
        &OutputFeatures::default(),
        &alice_ts_interface.key_manager_handle,
    )
    .await;

    let tx_id_1 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(10000),
            alice_address.clone(),
            "blah".to_string(),
            ImportStatus::Imported,
            None,
            None,
            None,
            uo_1.to_transaction_output(&alice_ts_interface.key_manager_handle)
                .await
                .unwrap(),
            PaymentId::Empty,
        )
        .await
        .unwrap();
    let tx_id_2 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(20000),
            alice_address.clone(),
            "one-sided 1".to_string(),
            ImportStatus::OneSidedUnconfirmed,
            None,
            None,
            None,
            uo_2.to_transaction_output(&alice_ts_interface.key_manager_handle)
                .await
                .unwrap(),
            PaymentId::Empty,
        )
        .await
        .unwrap();
    let tx_id_3 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(30000),
            alice_address,
            "one-sided 2".to_string(),
            ImportStatus::OneSidedConfirmed,
            None,
            None,
            None,
            uo_3.to_transaction_output(&alice_ts_interface.key_manager_handle)
                .await
                .unwrap(),
            PaymentId::Empty,
        )
        .await
        .unwrap();

    for (tx_id, uo) in [(tx_id_1, uo_1), (tx_id_2, uo_2), (tx_id_3, uo_3)] {
        alice_ts_interface
            .output_manager_service_handle
            .add_output_with_tx_id(tx_id, uo.clone(), None)
            .await
            .unwrap();
        let _result = alice_ts_interface.oms_db.mark_outputs_as_unspent(vec![(
            uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
            true,
        )]);
        alice_ts_interface
            .oms_db
            .set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
                commitment: uo.commitment(&alice_ts_interface.key_manager_handle).await.unwrap(),
                mined_height: 5,
                mined_in_block: FixedHash::zero(),
                confirmed: false,
                mined_timestamp: 0,
            }])
            .unwrap();
    }

    for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
        let transaction = alice_ts_interface
            .transaction_service_handle
            .get_any_transaction(tx_id)
            .await
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
                assert_eq!(tx.status, TransactionStatus::OneSidedUnconfirmed);
            } else {
                panic!("Should find a complete FauxUnconfirmed transaction");
            }
        }
        if tx_id == tx_id_3 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::OneSidedConfirmed);
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
        sleep(Duration::from_secs(1)).await;
        for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
            let transaction = alice_ts_interface
                .transaction_service_handle
                .get_any_transaction(tx_id)
                .await
                .unwrap()
                .unwrap();
            if let WalletTransaction::Completed(tx) = transaction {
                if tx_id == tx_id_1 && tx.status == TransactionStatus::OneSidedUnconfirmed && !found_imported {
                    found_imported = true;
                }
                if tx_id == tx_id_2 && tx.status == TransactionStatus::OneSidedUnconfirmed && !found_faux_unconfirmed {
                    found_faux_unconfirmed = true;
                }
                if tx_id == tx_id_3 && tx.status == TransactionStatus::OneSidedConfirmed && !found_faux_confirmed {
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

#[tokio::test]
async fn test_update_coinbase_tx_on_oms_validation() {
    let factories = CryptoFactories::default();

    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let alice_address = TariAddress::new_single_address_with_interactive_only(
        alice_ts_interface.base_node_identity.public_key().clone(),
        Network::LocalNet,
    );

    let uo_1 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::create_coinbase(5, None, RangeProofType::BulletProofPlus),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    let uo_2 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(20000),
        &OutputFeatures::create_coinbase(5, None, RangeProofType::BulletProofPlus),
        &alice_ts_interface.key_manager_handle,
    )
    .await;
    let uo_3 = make_input(
        &mut OsRng.clone(),
        MicroMinotari::from(30000),
        &OutputFeatures::create_coinbase(5, None, RangeProofType::BulletProofPlus),
        &alice_ts_interface.key_manager_handle,
    )
    .await;

    let tx_id_1 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(10000),
            alice_address.clone(),
            "coinbase_confirmed".to_string(),
            ImportStatus::CoinbaseConfirmed,
            None,
            None,
            None,
            uo_1.to_transaction_output(&alice_ts_interface.key_manager_handle)
                .await
                .unwrap(),
            PaymentId::Empty,
        )
        .await
        .unwrap();
    let tx_id_2 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(20000),
            alice_address.clone(),
            "one-coinbase_unconfirmed 1".to_string(),
            ImportStatus::CoinbaseUnconfirmed,
            None,
            None,
            None,
            uo_2.to_transaction_output(&alice_ts_interface.key_manager_handle)
                .await
                .unwrap(),
            PaymentId::Empty,
        )
        .await
        .unwrap();
    let tx_id_3 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(30000),
            alice_address,
            "Coinbase_not_mined".to_string(),
            ImportStatus::CoinbaseUnconfirmed,
            None,
            None,
            None,
            uo_3.to_transaction_output(&alice_ts_interface.key_manager_handle)
                .await
                .unwrap(),
            PaymentId::Empty,
        )
        .await
        .unwrap();

    for (tx_id, uo) in [(tx_id_1, uo_1), (tx_id_2, uo_2), (tx_id_3, uo_3)] {
        alice_ts_interface
            .output_manager_service_handle
            .add_output_with_tx_id(tx_id, uo.clone(), None)
            .await
            .unwrap();
        if uo.value != MicroMinotari::from(30000) {
            alice_ts_interface
                .oms_db
                .set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
                    commitment: uo.commitment(&alice_ts_interface.key_manager_handle).await.unwrap(),
                    mined_height: 5,
                    mined_in_block: FixedHash::zero(),
                    confirmed: false,
                    mined_timestamp: 0,
                }])
                .unwrap();
        }
    }

    for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
        let transaction = alice_ts_interface
            .transaction_service_handle
            .get_any_transaction(tx_id)
            .await
            .unwrap()
            .unwrap();
        if tx_id == tx_id_1 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::CoinbaseConfirmed);
            } else {
                panic!("Should find a complete Imported transaction");
            }
        }
        if tx_id == tx_id_2 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::CoinbaseUnconfirmed);
            } else {
                panic!("Should find a complete FauxUnconfirmed transaction");
            }
        }
        if tx_id == tx_id_3 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, TransactionStatus::CoinbaseUnconfirmed);
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

    let mut coinbase_confirmed = false;
    let mut coinbase_unconfirmed = false;
    let mut coinbase_unmined = false;
    for _ in 0..20 {
        sleep(Duration::from_secs(1)).await;
        for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
            let transaction = alice_ts_interface
                .transaction_service_handle
                .get_any_transaction(tx_id)
                .await
                .unwrap()
                .unwrap();
            if let WalletTransaction::Completed(tx) = transaction {
                if tx_id == tx_id_1 && tx.status == TransactionStatus::CoinbaseConfirmed && !coinbase_confirmed {
                    coinbase_confirmed = true;
                }
                if tx_id == tx_id_2 && tx.status == TransactionStatus::CoinbaseUnconfirmed && !coinbase_unconfirmed {
                    coinbase_unconfirmed = true;
                }
                if tx_id == tx_id_3 && tx.status == TransactionStatus::CoinbaseNotInBlockChain && !coinbase_unmined {
                    coinbase_unmined = true;
                }
            }
        }
        if coinbase_confirmed && coinbase_unconfirmed && coinbase_unmined {
            break;
        }
    }
    assert!(
        coinbase_confirmed && coinbase_unconfirmed && coinbase_unmined,
        "Should have found the updated statuses"
    );
}

#[tokio::test]
async fn test_get_fee_per_gram_per_block_basic() {
    let factories = CryptoFactories::default();
    let connection = make_wallet_database_memory_connection();
    let mut alice_ts_interface = setup_transaction_service_no_comms(factories, connection, None).await;
    let stats = vec![base_node_proto::MempoolFeePerGramStat {
        order: 0,
        min_fee_per_gram: 1,
        avg_fee_per_gram: 2,
        max_fee_per_gram: 3,
    }];
    alice_ts_interface
        .base_node_rpc_mock_state
        .set_fee_per_gram_stats_response(base_node_proto::GetMempoolFeePerGramStatsResponse { stats: stats.clone() });

    let estimates = alice_ts_interface
        .transaction_service_handle
        .get_fee_per_gram_stats_per_block(10)
        .await
        .unwrap();
    assert_eq!(estimates.stats, stats.into_iter().map(Into::into).collect::<Vec<_>>());
    assert_eq!(estimates.stats.len(), 1)
}
