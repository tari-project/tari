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

#![allow(clippy::indexing_slicing)]
use std::{mem::size_of, sync::Arc, time::Duration};

use blake2::Blake2b;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use chrono::{Days, Utc};
use digest::consts::U32;
use minotari_wallet::{
    base_node_service::{BaseNodeServiceInitializer, handle::BaseNodeServiceHandle},
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInitializer},
    output_manager_service::{
        OutputManagerServiceInitializer,
        UtxoSelectionCriteria,
        config::OutputManagerServiceConfig,
        handle::{OutputManagerEvent, OutputManagerHandle},
        service::OutputManagerService,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::KnownOneSidedPaymentScript,
            sqlite_db::{OutputManagerSqliteDatabase, ReceivedOutputInfoForBatch},
        },
    },
    storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::{WalletDbConnection, run_migration_and_create_sqlite_connection},
    },
    test_utils::{make_wallet_database_memory_connection, random_string},
    transaction_service::{
        TransactionServiceInitializer,
        config::TransactionServiceConfig,
        handle::{TransactionEvent, TransactionServiceHandle},
        service::TransactionService,
        storage::{
            database::{DbKeyValuePair, TransactionBackend, TransactionDatabase, WriteOperation},
            models::{CompletedTransaction, WalletTransaction},
            sqlite_db::TransactionServiceSqliteDatabase,
        },
    },
    util::watch::Watch,
    utxo_scanner_service::{
        handle::UtxoScannerHandle,
        initializer::UtxoScannerServiceInitializer,
        service::ScannedBlock,
    },
};
use rand::Rng;
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_common_types::{
    chain_metadata::ChainMetadata,
    seeds::cipher_seed::CipherSeed,
    tari_address::TariAddress,
    transaction::{LegacyImportStatus, LegacyTransactionStatus, TransactionDirection, TxId},
    types::{CompressedCommitment, CompressedPublicKey, CompressedSignature, FixedHash, HashOutput, PrivateKey},
};
use tari_comms::{
    PeerConnection,
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::rpc::{NamedProtocolService, mock::MockRpcServer},
    test_utils::node_identity::build_node_identity,
    transports::MemoryTransport,
};
use tari_core::base_node::{
    proto::wallet_rpc::{TxLocation, TxQueryResponse},
    rpc::BaseNodeWalletRpcServer,
};
use tari_crypto::{commitment::HomomorphicCommitmentFactory, keys::SecretKey as SK};
use tari_p2p::Network;
use tari_script::{ExecutionStack, push_pubkey_script};
use tari_service_framework::{RegisterHandle, StackBuilder, reply_channel};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::random;
use tari_transaction_components::{
    consensus::{ConsensusConstantsBuilder, ConsensusManager},
    crypto_factories::CryptoFactories,
    key_manager::{ConfidentialOutputHasher, TransactionKeyManagerInterface},
    rpc::models::TipInfoResponse,
    tari_amount::*,
    transaction_builder::TransactionBuilder,
    transaction_components::{
        EncryptedData,
        KernelBuilder,
        OutputFeatures,
        RangeProofType,
        Transaction,
        memo_field::{MemoField, TxType},
        one_sided::{diffie_hellman_stealth_domain_hasher, public_key_to_output_encryption_key},
    },
};
use tari_transaction_key_manager::{
    legacy_key_manager::{
        LegacyTransactionKeyManagerInitializer,
        LegacyTransactionKeyManagerInterface,
        LegacyTransactionKeyManagerWrapper,
        MemoryKeyManager,
        create_new_random_key_manager,
        wallet_types::{LegacyWalletType, ProvidedKeysWallet},
    },
    storage::sqlite_db::TransactionKeyManagerSqliteDatabase,
};
use tari_utilities::{ByteArray, SafePassword, epoch_time::EpochTime};
use tempfile::tempdir;
use tokio::{
    sync::{broadcast, broadcast::channel},
    task,
    time::sleep,
};
use url::Url;

use crate::support::{
    base_node_http_service_mock::{HttpBaseNodeMock, MockHttpClientFactory},
    comms_rpc::{BaseNodeWalletRpcMockService, BaseNodeWalletRpcMockState},
    utils::make_input,
};

pub fn get_next_memory_address() -> Multiaddr {
    let port = MemoryTransport::acquire_next_memsocket_port();
    format!("/memory/{port}").parse().unwrap()
}

pub type MemoryDBKeyManager = LegacyTransactionKeyManagerWrapper<TransactionKeyManagerSqliteDatabase<DbConnection>>;

async fn setup_transaction_service(
    node_identity: Arc<NodeIdentity>,
    consensus_manager: ConsensusManager,
    factories: CryptoFactories,
    db_connection: WalletDbConnection,
    shutdown_signal: ShutdownSignal,
) -> (
    TransactionServiceHandle,
    OutputManagerHandle<MemoryDBKeyManager>,
    WalletConnectivityHandle<MockHttpClientFactory>,
    MemoryDBKeyManager,
    OutputManagerSqliteDatabase,
) {
    let passphrase = SafePassword::from("My lovely secret passphrase");
    let db = WalletDatabase::new(WalletSqliteDatabase::new(db_connection.clone(), passphrase).unwrap());

    let mut key = [0u8; size_of::<Key>()];
    rand::rng().fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let cipher = XChaCha20Poly1305::new(key_ga);

    let ts_backend = TransactionServiceSqliteDatabase::new(db_connection.clone(), cipher.clone());
    let oms_backend = OutputManagerSqliteDatabase::new(db_connection.clone());

    let connection = DbConnection::connect_url(&DbConnectionUrl::MemoryShared(random_string(8)), Some(5)).unwrap();
    let cipher = CipherSeed::random();
    let mut key = [0u8; size_of::<Key>()];
    rand::rng().fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);
    let kms_backend = TransactionKeyManagerSqliteDatabase::init(connection, db_cipher);
    let wallet_type = Arc::new(LegacyWalletType::ProvidedKeys(ProvidedKeysWallet {
        public_spend_key: CompressedPublicKey::from_secret_key(node_identity.secret_key()),
        private_spend_key: Some(node_identity.secret_key().clone()),
        view_key: SK::random(&mut rand::rng()),
        private_comms_key: Some(node_identity.secret_key().clone()),
        birthday: None,
    }));
    let http_node_url = Url::parse("http://127.0.0.1:5434").unwrap();
    let wallet_connectivity_service_mock = WalletConnectivityHandle::new(MockHttpClientFactory::default());
    let handles = StackBuilder::new(shutdown_signal)
        .add_initializer(RegisterHandle::new(wallet_connectivity_service_mock))
        .add_initializer(OutputManagerServiceInitializer::<
            OutputManagerSqliteDatabase,
            MemoryDBKeyManager,
            MockHttpClientFactory,
        >::new(
            OutputManagerServiceConfig::default(),
            oms_backend.clone(),
            factories.clone(),
            Network::LocalNet.into(),
        ))
        .add_initializer(LegacyTransactionKeyManagerInitializer::<
            TransactionKeyManagerSqliteDatabase<_>,
        >::new_with_legacy_storage(
            kms_backend, cipher, factories.clone(), wallet_type.clone()
        ))
        .add_initializer(TransactionServiceInitializer::<
            _,
            MemoryDBKeyManager,
            MockHttpClientFactory,
        >::new(
            TransactionServiceConfig {
                broadcast_monitoring_timeout: Duration::from_secs(5),
                chain_monitoring_timeout: Duration::from_secs(5),
                num_confirmations_required: 0,
                ..Default::default()
            },
            ts_backend,
            node_identity.clone(),
            Network::LocalNet,
            consensus_manager,
            factories.clone(),
            wallet_type,
        ))
        .add_initializer(BaseNodeServiceInitializer::<MockHttpClientFactory>::new())
        .add_initializer(WalletConnectivityInitializer::<MockHttpClientFactory>::new(
            "http://localhost:9001".parse().unwrap(),
            "http://localhost:9001".parse().unwrap(),
        ))
        .add_initializer(UtxoScannerServiceInitializer::<_, MemoryDBKeyManager>::new(
            db,
            Network::LocalNet,
            14,
            http_node_url.clone(),
            http_node_url,
            1,
        ))
        .build()
        .await
        .unwrap();

    let output_manager_handle = handles.expect_handle::<OutputManagerHandle<MemoryDBKeyManager>>();
    let key_manager_handle = handles.expect_handle::<MemoryDBKeyManager>();
    let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();
    let connectivity_service_handle = handles.expect_handle::<WalletConnectivityHandle<MockHttpClientFactory>>();

    (
        transaction_service_handle,
        output_manager_handle,
        connectivity_service_handle,
        key_manager_handle,
        oms_backend,
    )
}

/// This struct holds a collection of interfaces that can be used in tests to interact with a Transaction Service that
/// is constructed without a comms layer, base node etc
pub struct TransactionServiceNoCommsInterface {
    transaction_service_handle: TransactionServiceHandle,
    output_manager_service_handle: OutputManagerHandle<MemoryKeyManager>,
    key_manager_handle: MemoryKeyManager,
    _shutdown: Shutdown,
    _mock_rpc_server: MockRpcServer<BaseNodeWalletRpcServer<BaseNodeWalletRpcMockService>>,
    base_node_identity: Arc<NodeIdentity>,
    base_node_rpc_mock_state: BaseNodeWalletRpcMockState,
    _rpc_server_connection: PeerConnection,
    output_manager_service_event_publisher: broadcast::Sender<Arc<OutputManagerEvent>>,
    ts_db: TransactionServiceSqliteDatabase,
    oms_db: OutputManagerDatabase<OutputManagerSqliteDatabase>,
    wallet_db: WalletDatabase<WalletSqliteDatabase>,
    base_node_mock: HttpBaseNodeMock,
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

    let (ts_request_sender, ts_request_receiver) = reply_channel::unbounded();
    let (event_publisher, _) = channel(100);
    let transaction_service_handle = TransactionServiceHandle::new(ts_request_sender, event_publisher.clone());

    let service = BaseNodeWalletRpcMockService::new();
    let base_node_rpc_mock_state = service.get_state();

    let server = BaseNodeWalletRpcServer::new(service);
    let protocol_name = server.as_protocol_name();

    let node_identity = build_node_identity(PeerFeatures::COMMUNICATION_NODE);

    let mut mock_rpc_server = MockRpcServer::new(server, node_identity.clone());

    mock_rpc_server.serve();

    let rpc_server_connection = mock_rpc_server
        .create_connection(node_identity.to_peer(), protocol_name.into())
        .await;

    let consensus_manager = ConsensusManager::builder(Network::LocalNet).build();
    let constants = ConsensusConstantsBuilder::new(Network::LocalNet).build();

    let shutdown = Shutdown::new();

    let (sender, _receiver_bns) = reply_channel::unbounded();
    let (base_node_service_event_publisher, _) = broadcast::channel(100);

    let base_node_service_handle = BaseNodeServiceHandle::new(sender, base_node_service_event_publisher);

    let passphrase = SafePassword::from("My lovely secret passphrase");
    let wallet =
        WalletSqliteDatabase::new(db_connection.clone(), passphrase).expect("Should be able to create wallet database");
    let cipher = wallet.cipher();
    let wallet_db = WalletDatabase::new(wallet);

    let ts_service_db = TransactionServiceSqliteDatabase::new(db_connection.clone(), cipher.clone());
    let ts_db = TransactionDatabase::new(ts_service_db.clone());
    let key_manager = create_new_random_key_manager().await.unwrap();
    let oms_db = OutputManagerDatabase::new(OutputManagerSqliteDatabase::new(db_connection));
    let (event_sender, _) = broadcast::channel(200);
    let recovery_message_watch = Watch::new("unset".to_string());
    let one_sided_message_watch = Watch::new("unset".to_string());

    let scanner_handle = UtxoScannerHandle::new(event_sender.clone(), one_sided_message_watch, recovery_message_watch);
    let mock_http = MockHttpClientFactory::default();
    let mock_https_server = mock_http.get_client();

    let wallet_connectivity_service_mock = WalletConnectivityHandle::new(mock_http);
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
        scanner_handle,
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
        transaction_resend_period: Duration::from_secs(200),
        resend_response_cooldown: Duration::from_secs(200),
        pending_transaction_cancellation_timeout: Duration::from_secs(300),
        transaction_mempool_resubmission_window: Duration::from_secs(2),
        max_tx_query_batch_size: 2,
        ..Default::default()
    });
    let (event_sender, _) = broadcast::channel(200);
    let recovery_message_watch = Watch::new("unset".to_string());
    let one_sided_message_watch = Watch::new("unset".to_string());

    let scanner_handle = UtxoScannerHandle::new(event_sender.clone(), one_sided_message_watch, recovery_message_watch);
    let ts_service = TransactionService::new(
        test_config,
        ts_db.clone(),
        ts_request_receiver,
        output_manager_service_handle.clone(),
        key_manager.clone(),
        wallet_connectivity_service_mock.clone(),
        event_publisher,
        node_identity.clone(),
        Network::LocalNet,
        consensus_manager,
        factories,
        shutdown.to_signal(),
        base_node_service_handle,
        key_manager.get_legacy_wallet_type(),
        scanner_handle,
    )
    .await
    .unwrap();
    task::spawn(async move { output_manager_service.start().await.unwrap() });
    task::spawn(async move { ts_service.start().await.unwrap() });
    TransactionServiceNoCommsInterface {
        transaction_service_handle,
        output_manager_service_handle,
        key_manager_handle: key_manager,
        _shutdown: shutdown,
        _mock_rpc_server: mock_rpc_server,
        base_node_identity: node_identity,
        base_node_rpc_mock_state,
        _rpc_server_connection: rpc_server_connection,
        output_manager_service_event_publisher,
        ts_db: ts_service_db,
        oms_db,
        wallet_db,
        base_node_mock: mock_https_server,
    }
}

#[tokio::test]
async fn large_coin_split_transaction() {
    // env_logger::builder().filter_level(log::LevelFilter::Trace).init();  //  > ./target/output.log 2>&1

    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "large_coin_split_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_connectivity, key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity.clone(),
        consensus_manager,
        factories.clone(),
        db_connection,
        shutdown.to_signal(),
    )
    .await;

    let initial_wallet_value = 20 * T;
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        key_manager_handle.key_manager(),
    );

    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
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
        .submit_transaction(
            tx_id,
            coin_split_tx,
            amount,
            MemoField::new_open_from_string("large coin-split", TxType::CoinSplit).unwrap(),
        )
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

    // The payment id should match the finalized and recovered tx fee
    let mut payment_id_verified = false;
    for output in completed_tx.transaction.body.outputs() {
        if let Ok(payment_id) = key_manager_handle.extract_payment_id_from_encrypted_data(
            output.encrypted_data(),
            output.commitment(),
            None,
        ) {
            assert_eq!(completed_tx.fee, payment_id.get_fee().unwrap());
            payment_id_verified = true;
            break;
        }
    }
    assert!(payment_id_verified);
}

#[tokio::test]
async fn single_transaction_burn_tari() {
    // let _ = env_logger::builder().filter_level(log::LevelFilter::Debug).is_test(true).try_init();
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "single_transaction_burn_tari: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_connectivity, key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity.clone(),
        consensus_manager,
        factories.clone(),
        db_connection,
        shutdown.to_signal(),
    )
    .await;
    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        key_manager_handle.key_manager(),
    );

    // Burn output
    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();
    let burn_value = 10000.into();
    let (claim_private_key, claim_public_key) = CompressedPublicKey::random_keypair(&mut rand::rng());
    let (tx_id, burn_proof) = alice_ts
        .burn_tari(
            burn_value,
            UtxoSelectionCriteria::default(),
            20.into(),
            MemoField::new_empty(),
            Some(claim_public_key.clone()),
            None,
        )
        .await
        .expect("Alice sending burn tx");
    let burn_proof = burn_proof.expect("Burn proof should be present");

    // Verify final balance

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find tx");

    let fees = completed_tx.fee;

    let balance = alice_oms.get_balance().await.unwrap();

    eprintln!("Balance after burn: {:#?}", balance);

    assert_eq!(
        balance.pending_incoming_balance,
        initial_wallet_value - burn_value - fees
    );

    // The payment id should match the finalized and recovered tx fee
    let mut payment_id_verified = false;
    for output in completed_tx.transaction.body.outputs() {
        if let Ok(payment_id) = key_manager_handle.extract_payment_id_from_encrypted_data(
            output.encrypted_data(),
            output.commitment(),
            None,
        ) {
            assert_eq!(completed_tx.fee, payment_id.get_fee().unwrap());
            payment_id_verified = true;
            break;
        }
    }
    assert!(payment_id_verified);

    // The claim_public_key field of the proof echoes the user-supplied L2 account pubkey.
    assert_eq!(burn_proof.claim_public_key, claim_public_key);

    // The ownership proof commits to the stealth claim key C = H(R·p)·G + P, not P. Recompute
    // C here from R (= sender_offset_public_key on the proof) and p (= claim_private_key) to
    // verify the signature.
    let r_pub = burn_proof.sender_offset_public_key.to_public_key().unwrap();
    let dh_shared = CompressedPublicKey::new_from_pk(&r_pub * &claim_private_key);
    let stealth_hash = diffie_hellman_stealth_domain_hasher(&dh_shared);
    let scalar = PrivateKey::from_uniform_bytes(stealth_hash.as_ref()).unwrap();
    let stealth_claim_public_key = CompressedPublicKey::new_from_pk(
        CompressedPublicKey::from_secret_key(&scalar).to_public_key().unwrap() +
            &claim_public_key.to_public_key().unwrap(),
    );
    let challenge_bytes = ConfidentialOutputHasher::new("commitment_signature")
        .chain(&burn_proof.commitment)
        .chain(&stealth_claim_public_key)
        .finalize();
    let ownership_proof = burn_proof.ownership_proof.to_schnorr_signature().unwrap();
    let commit_value = factories
        .commitment
        .commit_value(&PrivateKey::default(), burn_value.as_u64());
    let signer_pk = burn_proof.commitment.to_commitment().unwrap().as_public_key() - commit_value.as_public_key();
    assert!(ownership_proof.verify(&signer_pk, challenge_bytes));
}

#[tokio::test]
async fn send_one_sided_transaction_to_other() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let db_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_connectivity, key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity,
        consensus_manager,
        factories.clone(),
        db_connection,
        shutdown.to_signal(),
    )
    .await;

    let mut alice_event_stream = alice_ts.get_event_stream();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        key_manager_handle.key_manager(),
    );
    let mut alice_oms_clone = alice_oms.clone();
    alice_oms_clone.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();

    let value = 10000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let random_pvt_key = PrivateKey::random(&mut rand::rng());
    let bob_view_key = CompressedPublicKey::from_secret_key(&random_pvt_key);
    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key,
        bob_node_identity.public_key().clone(),
        network,
    )
    .unwrap();
    let tx_id = alice_ts_clone
        .send_one_sided_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20.into(),
            MemoField::new_open_from_string("SEE IF YOU CAN CATCH THIS ONE..... SIDED TX!", TxType::PaymentToOther)
                .unwrap(),
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
                if let TransactionEvent::TransactionCompletedImmediately(id) = &*event.unwrap()
                    && id == &tx_id
                {
                    found = true;
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(found, "'TransactionCompletedImmediately(_)' event not found");

    // The payment id should match the finalized and recovered tx fee
    let mut payment_id_verified = false;
    let bob_view_key_id = key_manager_handle
        .create_encrypted_key(random_pvt_key.clone(), None)
        .unwrap();
    for output in completed_tx.transaction.body.outputs() {
        let shared_secret = key_manager_handle
            .get_diffie_hellman_shared_secret(&bob_view_key_id, &output.sender_offset_public_key)
            .unwrap();
        let encryption_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
        if let Ok((_, _, payment_id)) =
            EncryptedData::decrypt_data(&encryption_key, output.commitment(), output.encrypted_data())
        {
            assert_eq!(completed_tx.fee, payment_id.get_fee().unwrap());
            payment_id_verified = true;
            break;
        }
    }
    assert!(payment_id_verified);
}

#[tokio::test]
async fn recover_one_sided_transaction() {
    // env_logger::builder().filter_level(log::LevelFilter::Trace).init(); //  > ./target/output.log 2>&1
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let alice_connection = make_wallet_database_memory_connection();
    let shutdown = Shutdown::new();
    let (mut alice_ts, alice_oms, _alice_connectivity, alice_key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity,
        consensus_manager.clone(),
        factories.clone(),
        alice_connection,
        shutdown.to_signal(),
    )
    .await;

    let bob_connection = make_wallet_database_memory_connection();
    let (_bob_ts, mut bob_oms, _bob_connectivity, bob_key_manager_handle, _bob_db) = setup_transaction_service(
        bob_node_identity.clone(),
        consensus_manager,
        factories.clone(),
        bob_connection,
        shutdown.to_signal(),
    )
    .await;
    let script = push_pubkey_script(bob_node_identity.public_key());
    let known_script = KnownOneSidedPaymentScript {
        script_hash: script.as_hash::<Blake2b<U32>>().unwrap().to_vec(),
        script_key_id: bob_key_manager_handle
            .create_encrypted_key(bob_node_identity.secret_key().clone(), None)
            .unwrap(),
        script,
        input: ExecutionStack::default(),
        script_lock_height: 0,
    };
    let mut cloned_bob_oms = bob_oms.clone();
    cloned_bob_oms.add_known_script(known_script).await.unwrap();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        alice_key_manager_handle.key_manager(),
    );
    let mut alice_oms_clone = alice_oms;
    alice_oms_clone.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();

    log::info!("Starting one-sided transaction");

    let value = 10000.into();
    let mut alice_ts_clone = alice_ts.clone();
    let bob_view_key = bob_key_manager_handle.get_view_key();
    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key.pub_key,
        bob_node_identity.public_key().clone(),
        network,
    )
    .unwrap();
    let tx_id = alice_ts_clone
        .send_one_sided_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20.into(),
            MemoField::new_empty(),
        )
        .await
        .expect("Alice sending one-sided tx to Bob");

    log::info!("One-sided transaction sent");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find completed one-sided tx");
    let outputs = completed_tx.transaction.body.outputs().clone();

    let recovered_outputs_1 = bob_oms
        .scan_outputs_for_one_sided_payments(outputs.clone())
        .await
        .unwrap();
    // Bob should be able to claim 1 output.
    assert_eq!(1, recovered_outputs_1.len());
    assert_eq!(value, recovered_outputs_1[0].output.value());

    // The payment id should match the finalized and recovered tx fee
    let shared_secret = bob_key_manager_handle
        .get_diffie_hellman_shared_secret(
            &bob_view_key.key_id,
            &recovered_outputs_1[0]
                .output
                .to_transaction_output()
                .unwrap()
                .sender_offset_public_key,
        )
        .unwrap();
    let encryption_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
    let (_, _, payment_id) = EncryptedData::decrypt_data(
        &encryption_key,
        recovered_outputs_1[0].output.commitment(),
        recovered_outputs_1[0].output.encrypted_data(),
    )
    .unwrap();
    assert_eq!(completed_tx.fee, payment_id.get_fee().unwrap());

    // Should ignore already existing outputs
    let recovered_outputs_2 = bob_oms.scan_outputs_for_one_sided_payments(outputs).await.unwrap();
    assert!(recovered_outputs_2.is_empty());
}

#[tokio::test]
async fn recover_stealth_one_sided_transaction() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    // Bob's parameters
    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    log::info!(
        "manage_single_transaction: Alice: '{}', Bob: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let alice_connection = make_wallet_database_memory_connection();
    let shutdown = Shutdown::new();
    let (mut alice_ts, alice_oms, _alice_connectivity, alice_key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity,
        consensus_manager.clone(),
        factories.clone(),
        alice_connection,
        shutdown.to_signal(),
    )
    .await;

    let bob_connection = make_wallet_database_memory_connection();
    let (_bob_ts, mut bob_oms, _bob_connectivity, bob_key_manager_handle, _bob_db) = setup_transaction_service(
        bob_node_identity.clone(),
        consensus_manager,
        factories.clone(),
        bob_connection,
        shutdown.to_signal(),
    )
    .await;

    let bob_view_key = bob_key_manager_handle.get_view_key();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        alice_key_manager_handle.key_manager(),
    );
    let mut alice_oms_clone = alice_oms;
    alice_oms_clone.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();

    let value = 10000.into();
    let mut alice_ts_clone = alice_ts.clone();

    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key.pub_key,
        bob_node_identity.public_key().clone(),
        network,
    )
    .unwrap();
    let tx_id = alice_ts_clone
        .send_one_sided_to_stealth_address_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            OutputFeatures::default(),
            20.into(),
            MemoField::new_empty(),
        )
        .await
        .expect("Alice sending one-sided tx to Bob");

    let completed_tx = alice_ts
        .get_completed_transaction(tx_id)
        .await
        .expect("Could not find completed one-sided tx");
    let outputs = completed_tx.transaction.body.outputs().clone();

    let recovered_outputs_1 = bob_oms
        .scan_outputs_for_one_sided_payments(outputs.clone())
        .await
        .unwrap();
    // Bob should be able to claim 1 output.
    assert_eq!(1, recovered_outputs_1.len());
    assert_eq!(value, recovered_outputs_1[0].output.value());

    // The payment id should match the finalized and recovered tx fee
    let shared_secret = bob_key_manager_handle
        .get_diffie_hellman_shared_secret(
            &bob_view_key.key_id,
            &recovered_outputs_1[0]
                .output
                .to_transaction_output()
                .unwrap()
                .sender_offset_public_key,
        )
        .unwrap();
    let encryption_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
    let (_, _, payment_id) = EncryptedData::decrypt_data(
        &encryption_key,
        recovered_outputs_1[0].output.commitment(),
        recovered_outputs_1[0].output.encrypted_data(),
    )
    .unwrap();
    assert_eq!(completed_tx.fee, payment_id.get_fee().unwrap());

    // Should ignore already existing outputs
    let recovered_outputs_2 = bob_oms.scan_outputs_for_one_sided_payments(outputs).await.unwrap();
    assert!(recovered_outputs_2.is_empty());
}

// test is broken
#[ignore]
#[tokio::test]
async fn test_htlc_send_and_claim() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));
    log::info!(
        "manage_single_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        base_node_identity.node_id().short_str()
    );

    let alice_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_connectivity, key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity,
        consensus_manager,
        factories.clone(),
        alice_connection,
        shutdown.to_signal(),
    )
    .await;

    let bob_temp_dir = tempdir().unwrap();
    let bob_db_path_string = bob_temp_dir.path().to_str().unwrap().to_string();
    let bob_db_name = format!("{}.sqlite3", random::string(8).as_str());
    let bob_db_path = format!("{bob_db_path_string}/{bob_db_name}");
    let bob_connection = run_migration_and_create_sqlite_connection(&bob_db_path, 16).unwrap();
    let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), bob_connection, None).await;

    log::info!(
        "manage_single_transaction: Bob: '{}'",
        bob_ts_interface.base_node_identity.node_id().short_str(),
    );

    let mut alice_event_stream = alice_ts.get_event_stream();

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        key_manager_handle.key_manager(),
    );
    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();

    let value = 10000.into();
    let bob_pubkey = bob_ts_interface.base_node_identity.public_key().clone();
    let bob_view_key = bob_ts_interface.key_manager_handle.get_view_key();
    let bob_address =
        TariAddress::new_dual_address_with_default_features(bob_view_key.pub_key, bob_pubkey.clone(), network).unwrap();
    let (tx_id, pre_image, output) = alice_ts
        .send_sha_atomic_swap_transaction(
            bob_address,
            value,
            UtxoSelectionCriteria::default(),
            20.into(),
            MemoField::new_empty(),
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
                if let TransactionEvent::TransactionCompletedImmediately(id) = &*event.unwrap()
                    && id == &tx_id
                {
                    break;
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
        .submit_transaction(tx_id_htlc, tx, htlc_amount, MemoField::new_empty())
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

#[tokio::test]
async fn test_htlc_send_and_claim_payment_id_fee() {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManager::builder(network).build();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));

    let bob_node_identity = Arc::new(NodeIdentity::random(
        &mut rand::rng(),
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));
    log::info!(
        "manage_single_transaction: Alice: '{}', Base: '{}'",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str()
    );

    let alice_connection = make_wallet_database_memory_connection();

    let shutdown = Shutdown::new();
    let (mut alice_ts, mut alice_oms, _alice_connectivity, key_manager_handle, alice_db) = setup_transaction_service(
        alice_node_identity,
        consensus_manager.clone(),
        factories.clone(),
        alice_connection,
        shutdown.to_signal(),
    )
    .await;

    let bob_connection = make_wallet_database_memory_connection();
    let (_bob_ts_interface, _bob_oms, _bob_connectivity, bob_key_manager_handle, _bob_db) = setup_transaction_service(
        bob_node_identity.clone(),
        consensus_manager,
        factories.clone(),
        bob_connection.clone(),
        shutdown.to_signal(),
    )
    .await;

    log::info!(
        "manage_single_transaction: Bob: '{}'",
        bob_node_identity.node_id().short_str(),
    );

    let initial_wallet_value = 25000.into();
    let uo1 = make_input(
        &mut rand::rng(),
        initial_wallet_value,
        &OutputFeatures::default(),
        key_manager_handle.key_manager(),
    );
    alice_oms.add_output(uo1.clone(), None).await.unwrap();
    alice_db
        .mark_outputs_as_unspent(vec![(uo1.output_hash(), true)])
        .unwrap();

    let value = 10000.into();

    let bob_view_key = bob_key_manager_handle.get_view_key();
    let bob_address = TariAddress::new_dual_address_with_default_features(
        bob_view_key.pub_key,
        bob_node_identity.public_key().clone(),
        network,
    )
    .unwrap();

    let (tx_id, _pre_image, _output) = alice_ts
        .send_sha_atomic_swap_transaction(
            bob_address.clone(),
            value,
            UtxoSelectionCriteria::default(),
            20.into(),
            MemoField::new_empty(),
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

    // The payment id should match the finalized and recovered tx fee
    let mut payment_id_verified = false;
    for output in completed_tx.transaction.body.outputs() {
        let shared_secret = bob_key_manager_handle
            .get_diffie_hellman_shared_secret(&bob_view_key.key_id, &output.sender_offset_public_key)
            .unwrap();
        let encryption_key = public_key_to_output_encryption_key(&shared_secret).unwrap();
        if let Ok((_, _, payment_id)) =
            EncryptedData::decrypt_data(&encryption_key, output.commitment(), output.encrypted_data())
        {
            assert_eq!(completed_tx.fee, payment_id.get_fee().unwrap());
            payment_id_verified = true;
            break;
        }
    }
    assert!(payment_id_verified);
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

/// This test will check that the Transaction Service starts the tx broadcast protocol correctly and reacts correctly
/// to a tx being broadcast and to a tx being rejected.
#[tokio::test]
async fn transaction_service_tx_broadcast() {
    // let factories = CryptoFactories::default();

    // let alice_node_identity =
    //     NodeIdentity::random(&mut rand::rng(), get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    // let bob_node_identity =
    //     NodeIdentity::random(&mut rand::rng(), get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    // let connection = make_wallet_database_memory_connection();

    // let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    // let mut alice_event_stream = alice_ts_interface.transaction_service_handle.get_event_stream();

    // alice_ts_interface
    //     .wallet_connectivity_service_mock
    //     .set_base_node(BaseNodePeerManager::new(0, vec![alice_ts_interface.base_node_identity.to_peer()]).unwrap());

    // let connection2 = make_wallet_database_memory_connection();
    // let mut bob_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection2, None).await;

    // let alice_output_value = MicroMinotari(250000);

    // let uo = make_input(
    //     &mut rand::rng(),
    //     alice_output_value,
    //     &OutputFeatures::default(),
    //     &alice_ts_interface.key_manager_handle,
    // )
    // .await;
    // alice_ts_interface
    //     .output_manager_service_handle
    //     .add_output(uo.clone(), None)
    //     .await
    //     .unwrap();
    // alice_ts_interface
    //     .oms_db
    //     .mark_outputs_as_unspent(vec![(
    //         uo.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
    //         true,
    //     )])
    //     .unwrap();

    // let uo2 = make_input(
    //     &mut rand::rng(),
    //     alice_output_value,
    //     &OutputFeatures::default(),
    //     &alice_ts_interface.key_manager_handle,
    // )
    // .await;
    // alice_ts_interface
    //     .output_manager_service_handle
    //     .add_output(uo2.clone(), None)
    //     .await
    //     .unwrap();
    // alice_ts_interface
    //     .oms_db
    //     .mark_outputs_as_unspent(vec![(
    //         uo2.hash(&alice_ts_interface.key_manager_handle).await.unwrap(),
    //         true,
    //     )])
    //     .unwrap();

    // let amount_sent1 = 100000 * uT;

    // let bob_address = TariAddress::new_single_address_with_interactive_only(
    //     bob_node_identity.public_key().clone(),
    //     Network::LocalNet,
    // )
    // .unwrap();
    // // Send Tx1
    // let tx_id1 = alice_ts_interface
    //     .transaction_service_handle
    //     .send_transaction(
    //         bob_address.clone(),
    //         amount_sent1,
    //         UtxoSelectionCriteria::default(),
    //         OutputFeatures::default(),
    //         100 * uT,
    //         PaymentId::open_from_string("Testing Message", TxType::PaymentToOther),
    //     )
    //     .await
    //     .unwrap();
    // alice_ts_interface
    //     .outbound_service_mock_state
    //     .wait_call_count(2, Duration::from_secs(60))
    //     .await
    //     .expect("Alice call wait 1");
    // let (_, _body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    // let (_, body) = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    // let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    // let tx_sender_msg: TransactionSenderMessage = envelope_body
    //     .decode_part::<proto::TransactionSenderMessage>(1)
    //     .unwrap()
    //     .unwrap()
    //     .try_into()
    //     .unwrap();
    // match tx_sender_msg {
    //     TransactionSenderMessage::Single(_) => (),
    //     _ => {
    //         panic!("Transaction is the not a single rounder sender variant");
    //     },
    // };

    // bob_ts_interface
    //     .transaction_send_message_channel
    //     .send(create_dummy_message(
    //         tx_sender_msg.try_into().unwrap(),
    //         alice_node_identity.public_key(),
    //     ))
    //     .await
    //     .unwrap();
    // bob_ts_interface
    //     .outbound_service_mock_state
    //     .wait_call_count(2, Duration::from_secs(60))
    //     .await
    //     .expect("bob call wait 1");

    // let _result = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    // let call = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    // let envelope_body = EnvelopeBody::decode(&mut call.1.to_vec().as_slice()).unwrap();
    // let bob_tx_reply_msg1: RecipientSignedMessage = envelope_body
    //     .decode_part::<proto::RecipientSignedMessage>(1)
    //     .unwrap()
    //     .unwrap()
    //     .try_into()
    //     .unwrap();

    // // Send Tx2
    // let amount_sent2 = 100001 * uT;
    // let tx_id2 = alice_ts_interface
    //     .transaction_service_handle
    //     .send_transaction(
    //         bob_address,
    //         amount_sent2,
    //         UtxoSelectionCriteria::default(),
    //         OutputFeatures::default(),
    //         20 * uT,
    //         PaymentId::open_from_string("Testing Message2", TxType::PaymentToOther),
    //     )
    //     .await
    //     .unwrap();
    // alice_ts_interface
    //     .outbound_service_mock_state
    //     .wait_call_count(2, Duration::from_secs(60))
    //     .await
    //     .expect("Alice call wait 2");

    // let _result = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    // let call = alice_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    // let tx_sender_msg = try_decode_sender_message(call.1.to_vec()).unwrap();

    // match tx_sender_msg {
    //     TransactionSenderMessage::Single(_) => (),
    //     _ => {
    //         panic!("Transaction is the not a single rounder sender variant");
    //     },
    // };

    // bob_ts_interface
    //     .transaction_send_message_channel
    //     .send(create_dummy_message(
    //         tx_sender_msg.try_into().unwrap(),
    //         alice_node_identity.public_key(),
    //     ))
    //     .await
    //     .unwrap();
    // bob_ts_interface
    //     .outbound_service_mock_state
    //     .wait_call_count(2, Duration::from_secs(60))
    //     .await
    //     .expect("Bob call wait 2");

    // let (_, _body) = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();
    // let (_, body) = bob_ts_interface.outbound_service_mock_state.pop_call().await.unwrap();

    // let envelope_body = EnvelopeBody::decode(body.to_vec().as_slice()).unwrap();
    // let bob_tx_reply_msg2: RecipientSignedMessage = envelope_body
    //     .decode_part::<proto::RecipientSignedMessage>(1)
    //     .unwrap()
    //     .unwrap()
    //     .try_into()
    //     .unwrap();

    // let balance = alice_ts_interface
    //     .output_manager_service_handle
    //     .get_balance()
    //     .await
    //     .unwrap();
    // assert_eq!(balance.available_balance, MicroMinotari(0));

    // // Give Alice the first of tx reply to start the broadcast process.
    // alice_ts_interface
    //     .transaction_ack_message_channel
    //     .send(create_dummy_message(
    //         bob_tx_reply_msg1.try_into().unwrap(),
    //         bob_node_identity.public_key(),
    //     ))
    //     .await
    //     .unwrap();

    // let delay = sleep(Duration::from_secs(60));
    // tokio::pin!(delay);
    // let mut tx1_received = false;
    // loop {
    //     tokio::select! {
    //         event = alice_event_stream.recv() => {
    //              if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap(){
    //                 if tx_id == &tx_id1 {
    //                     tx1_received = true;
    //                     break;
    //                 }
    //             }
    //         },
    //         () = &mut delay => {
    //             break;
    //         },
    //     }
    // }
    // assert!(tx1_received);

    // let alice_completed_txs = alice_ts_interface
    //     .transaction_service_handle
    //     .get_completed_transactions(None, None, None, 0)
    //     .await
    //     .unwrap();
    // let alice_completed_tx1 = alice_completed_txs
    //     .iter()
    //     .find(|tx| tx.tx_id == tx_id1)
    //     .expect("Transaction must be in collection");

    // let tx1_fee = alice_completed_tx1.fee;

    // assert!(
    //     alice_completed_tx1.status == TransactionStatus::Completed ||
    //         alice_completed_tx1.status == TransactionStatus::Broadcast
    // );

    // let _transactions = alice_ts_interface
    //     .base_node_rpc_mock_state
    //     .wait_pop_submit_transaction_calls(1, Duration::from_secs(30))
    //     .await
    //     .expect("Should receive a tx submission");
    // let _schnorr_signatures = alice_ts_interface
    //     .base_node_rpc_mock_state
    //     .wait_pop_transaction_query_calls(1, Duration::from_secs(30))
    //     .await
    //     .expect("Should receive a tx query");

    // alice_ts_interface
    //     .base_node_rpc_mock_state
    //     .set_transaction_query_response(TxQueryResponse {
    //         location: TxLocation::Mined,
    //         best_block_hash: None,
    //         confirmations: TransactionServiceConfig::default().num_confirmations_required,
    //         is_synced: true,
    //         best_block_height: 0,
    //         mined_timestamp: None,
    //     });

    // let delay = sleep(Duration::from_secs(60));
    // tokio::pin!(delay);
    // let mut tx1_broadcast = false;
    // loop {
    //     tokio::select! {
    //         event = alice_event_stream.recv() => {
    //             println!("Event: {:?}", event);
    //              if let TransactionEvent::TransactionBroadcast(tx_id) = &*event.unwrap(){
    //                 if tx_id == &tx_id1 {
    //                     tx1_broadcast = true;
    //                     break;
    //                 }
    //             }
    //         },
    //         () = &mut delay => {
    //             break;
    //         },
    //     }
    // }
    // assert!(tx1_broadcast);

    // alice_ts_interface
    //     .transaction_ack_message_channel
    //     .send(create_dummy_message(
    //         bob_tx_reply_msg2.try_into().unwrap(),
    //         bob_node_identity.public_key(),
    //     ))
    //     .await
    //     .unwrap();

    // let delay = sleep(Duration::from_secs(60));
    // tokio::pin!(delay);
    // let mut tx2_received = false;
    // loop {
    //     tokio::select! {
    //         event = alice_event_stream.recv() => {
    //              if let TransactionEvent::ReceivedTransactionReply(tx_id) = &*event.unwrap(){
    //                 if tx_id == &tx_id2 {
    //                     tx2_received = true;
    //                     break;
    //                 }
    //             }
    //         },
    //         () = &mut delay => {
    //             break;
    //         },
    //     }
    // }
    // assert!(tx2_received);

    // alice_ts_interface
    //     .base_node_rpc_mock_state
    //     .set_submit_transaction_response(TxSubmissionResponse {
    //         accepted: false,
    //         rejection_reason: TxSubmissionRejectionReason::Orphan,
    //         is_synced: true,
    //     });

    // alice_ts_interface
    //     .base_node_rpc_mock_state
    //     .set_transaction_query_response(TxQueryResponse {
    //         location: TxLocation::NotStored,
    //         best_block_hash: None,
    //         confirmations: TransactionServiceConfig::default().num_confirmations_required,
    //         is_synced: true,
    //         best_block_height: 0,
    //         mined_timestamp: None,
    //     });

    // let alice_completed_txs = alice_ts_interface
    //     .transaction_service_handle
    //     .get_completed_transactions(None, None, None, 0)
    //     .await
    //     .unwrap();
    // let alice_completed_tx2 = alice_completed_txs
    //     .iter()
    //     .find(|tx| tx.tx_id == tx_id2)
    //     .expect("Transaction must be in collection");

    // assert!(
    //     alice_completed_tx2.status == TransactionStatus::Completed ||
    //         alice_completed_tx2.status == TransactionStatus::Broadcast
    // );

    // let _transactions = alice_ts_interface
    //     .base_node_rpc_mock_state
    //     .wait_pop_submit_transaction_calls(1, Duration::from_secs(30))
    //     .await
    //     .expect("Should receive a tx submission");

    // let delay = sleep(Duration::from_secs(60));
    // tokio::pin!(delay);
    // let mut tx2_cancelled = false;
    // loop {
    //     tokio::select! {
    //         event = alice_event_stream.recv() => {
    //              if let TransactionEvent::TransactionCancelled(tx_id, _) = &*event.unwrap(){
    //                 if tx_id == &tx_id2 {
    //                     tx2_cancelled = true;
    //                     break;
    //                 }
    //             }
    //         },
    //         () = &mut delay => {
    //             break;
    //         },
    //     }
    // }
    // assert!(tx2_cancelled);

    // // Check that the cancelled Tx value + change from tx1 is available
    // let balance = alice_ts_interface
    //     .output_manager_service_handle
    //     .get_balance()
    //     .await
    //     .unwrap();

    // assert_eq!(
    //     balance.pending_incoming_balance,
    //     alice_output_value - amount_sent1 - tx1_fee
    // );
    // assert_eq!(balance.available_balance, alice_output_value);
}

#[tokio::test]
async fn broadcast_all_completed_transactions_on_startup() {
    let factories = CryptoFactories::default();
    let connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let db = alice_ts_interface.ts_db.clone();

    let kernel = KernelBuilder::new()
        .with_excess(&CompressedCommitment::from_commitment(factories.commitment.zero()))
        .with_signature(CompressedSignature::default())
        .build()
        .unwrap();

    let tx = Transaction::new(
        vec![],
        vec![],
        vec![kernel],
        PrivateKey::random(&mut rand::rng()),
        PrivateKey::random(&mut rand::rng()),
    );
    let source_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();
    let destination_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();
    let completed_tx1 = CompletedTransaction {
        tx_id: 1u64.into(),
        source_address,
        destination_address,
        amount: 5000 * uT,
        fee: MicroMinotari::from(20),
        transaction: tx.clone(),
        status: LegacyTransactionStatus::Completed,
        timestamp: Utc::now(),
        cancelled: None,
        direction: TransactionDirection::Outbound,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: tx
            .first_kernel_excess_sig()
            .unwrap_or(&CompressedSignature::default())
            .clone(),
        mined_height: None,
        mined_in_block: None,
        mined_timestamp: None,
        payment_id: MemoField::new_open_from_string("Yo!", TxType::PaymentToOther).unwrap(),
        change_output_hashes: vec![],
        received_output_hashes: vec![],
        sent_output_hashes: vec![],
        lock_height: 0,
        rejection_reason: None,
    };

    let completed_tx2 = CompletedTransaction {
        tx_id: 2u64.into(),
        status: LegacyTransactionStatus::MinedConfirmed,
        ..completed_tx1.clone()
    };

    let completed_tx3 = CompletedTransaction {
        tx_id: 3u64.into(),
        status: LegacyTransactionStatus::Completed,
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
    assert!(
        alice_ts_interface
            .transaction_service_handle
            .restart_broadcast_protocols()
            .await
            .is_ok()
    );

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
    )
    .unwrap();

    let uo_1 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::default(),
        alice_ts_interface.key_manager_handle.key_manager(),
    );
    let uo_2 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(20000),
        &OutputFeatures::default(),
        alice_ts_interface.key_manager_handle.key_manager(),
    );
    let uo_3 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(30000),
        &OutputFeatures::default(),
        alice_ts_interface.key_manager_handle.key_manager(),
    );

    let tx_id_1 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(10000),
            alice_address.clone(),
            LegacyImportStatus::Imported,
            None,
            None,
            uo_1.to_transaction_output().unwrap(),
            MemoField::new_open_from_string("blah", TxType::PaymentToOther).unwrap(),
            None,
            0,
        )
        .await
        .unwrap();
    let tx_id_2 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(20000),
            alice_address.clone(),
            LegacyImportStatus::OneSidedUnconfirmed,
            None,
            None,
            uo_2.to_transaction_output().unwrap(),
            MemoField::new_open_from_string("one-sided 1", TxType::PaymentToOther).unwrap(),
            None,
            0,
        )
        .await
        .unwrap();
    let tx_id_3 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(30000),
            alice_address,
            LegacyImportStatus::OneSidedConfirmed,
            None,
            None,
            uo_3.to_transaction_output().unwrap(),
            MemoField::new_open_from_string("one-sided 2", TxType::PaymentToOther).unwrap(),
            None,
            0,
        )
        .await
        .unwrap();

    for (tx_id, uo, height) in [(tx_id_1, uo_1, 10), (tx_id_2, uo_2, 10), (tx_id_3, uo_3, 5)] {
        alice_ts_interface
            .output_manager_service_handle
            .add_output_with_tx_id(tx_id, uo.clone(), None)
            .await
            .unwrap();
        alice_ts_interface
            .oms_db
            .mark_outputs_as_unspent(vec![(uo.output_hash(), true)])
            .unwrap();
        alice_ts_interface
            .oms_db
            .set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
                commitment: uo.commitment().clone(),
                mined_height: height,
                mined_in_block: FixedHash::zero(),
                confirmed: false,
                mined_timestamp: 0,
            }])
            .unwrap();
    }
    // set height to mined height
    let scanned_block = ScannedBlock {
        header_hash: HashOutput::zero(),
        height: 10,
        timestamp: Utc::now().naive_utc(),
    };
    let chain_metadata = ChainMetadata::new(10, HashOutput::zero(), 0, 0, 1.into(), EpochTime::now().as_u64()).unwrap();
    alice_ts_interface
        .base_node_mock
        .set_tip_info(TipInfoResponse {
            metadata: Some(chain_metadata),
            is_synced: false,
        })
        .await
        .unwrap();
    alice_ts_interface.wallet_db.save_scanned_block(scanned_block).unwrap();

    for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
        let transaction = alice_ts_interface
            .transaction_service_handle
            .get_any_transaction(tx_id)
            .await
            .unwrap()
            .unwrap();
        if tx_id == tx_id_1 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, LegacyTransactionStatus::Imported);
            } else {
                panic!("Should find a complete Imported transaction");
            }
        }
        if tx_id == tx_id_2 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, LegacyTransactionStatus::OneSidedUnconfirmed);
            } else {
                panic!("Should find a complete FauxUnconfirmed transaction");
            }
        }
        if tx_id == tx_id_3 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, LegacyTransactionStatus::OneSidedConfirmed);
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
                if tx_id == tx_id_1 && tx.status == LegacyTransactionStatus::OneSidedUnconfirmed && !found_imported {
                    found_imported = true;
                }
                if tx_id == tx_id_2 &&
                    tx.status == LegacyTransactionStatus::OneSidedUnconfirmed &&
                    !found_faux_unconfirmed
                {
                    found_faux_unconfirmed = true;
                }
                if tx_id == tx_id_3 && tx.status == LegacyTransactionStatus::OneSidedConfirmed && !found_faux_confirmed
                {
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
    )
    .unwrap();

    let uo_1 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(10000),
        &OutputFeatures::create_coinbase(5, None, RangeProofType::BulletProofPlus),
        alice_ts_interface.key_manager_handle.key_manager(),
    );
    let uo_2 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(20000),
        &OutputFeatures::create_coinbase(5, None, RangeProofType::BulletProofPlus),
        alice_ts_interface.key_manager_handle.key_manager(),
    );
    let uo_3 = make_input(
        &mut rand::rng().clone(),
        MicroMinotari::from(30000),
        &OutputFeatures::create_coinbase(5, None, RangeProofType::BulletProofPlus),
        alice_ts_interface.key_manager_handle.key_manager(),
    );

    let tx_id_1 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(10000),
            alice_address.clone(),
            LegacyImportStatus::CoinbaseConfirmed,
            None,
            None,
            uo_1.to_transaction_output().unwrap(),
            MemoField::new_open_from_string("coinbase_confirmed", TxType::PaymentToOther).unwrap(),
            None,
            0,
        )
        .await
        .unwrap();
    let tx_id_2 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(20000),
            alice_address.clone(),
            LegacyImportStatus::CoinbaseUnconfirmed,
            None,
            None,
            uo_2.to_transaction_output().unwrap(),
            MemoField::new_open_from_string("one-coinbase_unconfirmed 1", TxType::PaymentToOther).unwrap(),
            None,
            0,
        )
        .await
        .unwrap();
    let tx_id_3 = alice_ts_interface
        .transaction_service_handle
        .import_utxo_with_status(
            MicroMinotari::from(30000),
            alice_address,
            LegacyImportStatus::CoinbaseUnconfirmed,
            None,
            None,
            uo_3.to_transaction_output().unwrap(),
            MemoField::new_open_from_string("Coinbase_not_mined", TxType::PaymentToOther).unwrap(),
            None,
            0,
        )
        .await
        .unwrap();

    for (tx_id, uo) in [(tx_id_1, uo_1), (tx_id_2, uo_2), (tx_id_3, uo_3)] {
        alice_ts_interface
            .output_manager_service_handle
            .add_output_with_tx_id(tx_id, uo.clone(), None)
            .await
            .unwrap();
        if uo.value() == MicroMinotari::from(10000) {
            alice_ts_interface
                .oms_db
                .set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
                    commitment: uo.commitment().clone(),
                    mined_height: 5,
                    mined_in_block: FixedHash::zero(),
                    confirmed: false,
                    mined_timestamp: 0,
                }])
                .unwrap();
        }
        if uo.value() == MicroMinotari::from(20000) {
            alice_ts_interface
                .oms_db
                .set_received_outputs_mined_height_and_statuses(vec![ReceivedOutputInfoForBatch {
                    commitment: uo.commitment().clone(),
                    mined_height: 10,
                    mined_in_block: FixedHash::zero(),
                    confirmed: false,
                    mined_timestamp: 0,
                }])
                .unwrap();
        }
    }
    // set height to 10
    let scanned_block = ScannedBlock {
        header_hash: HashOutput::zero(),
        height: 10,
        timestamp: Utc::now().naive_utc(),
    };
    let chain_metadata = ChainMetadata::new(10, HashOutput::zero(), 0, 0, 1.into(), EpochTime::now().as_u64()).unwrap();
    alice_ts_interface
        .base_node_mock
        .set_tip_info(TipInfoResponse {
            metadata: Some(chain_metadata),
            is_synced: false,
        })
        .await
        .unwrap();
    alice_ts_interface.wallet_db.save_scanned_block(scanned_block).unwrap();

    for tx_id in [tx_id_1, tx_id_2, tx_id_3] {
        let transaction = alice_ts_interface
            .transaction_service_handle
            .get_any_transaction(tx_id)
            .await
            .unwrap()
            .unwrap();
        if tx_id == tx_id_1 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, LegacyTransactionStatus::CoinbaseConfirmed);
            } else {
                panic!("Should find a complete Imported transaction");
            }
        }
        if tx_id == tx_id_2 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, LegacyTransactionStatus::CoinbaseUnconfirmed);
            } else {
                panic!("Should find a complete FauxUnconfirmed transaction");
            }
        }
        if tx_id == tx_id_3 {
            if let WalletTransaction::Completed(tx) = &transaction {
                assert_eq!(tx.status, LegacyTransactionStatus::CoinbaseUnconfirmed);
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
                if tx_id == tx_id_1 && tx.status == LegacyTransactionStatus::CoinbaseConfirmed && !coinbase_confirmed {
                    coinbase_confirmed = true;
                }
                if tx_id == tx_id_2 &&
                    tx.status == LegacyTransactionStatus::CoinbaseUnconfirmed &&
                    !coinbase_unconfirmed
                {
                    coinbase_unconfirmed = true;
                }
                if tx_id == tx_id_3 &&
                    tx.status == LegacyTransactionStatus::CoinbaseNotInBlockChain &&
                    !coinbase_unmined
                {
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

fn create_mock_completed_transaction(
    source_address: TariAddress,
    destination_address: TariAddress,
    amount: MicroMinotari,
    direction: TransactionDirection,
    description: &str,
) -> CompletedTransaction {
    CompletedTransaction {
        tx_id: TxId::new_random(),
        source_address,
        destination_address,
        amount,
        fee: MicroMinotari::from(123),
        transaction: Transaction::new(
            vec![],
            vec![],
            vec![],
            PrivateKey::random(&mut rand::rng()),
            PrivateKey::random(&mut rand::rng()),
        ),
        status: LegacyTransactionStatus::Completed,
        timestamp: Utc::now(),
        cancelled: None,
        direction,
        send_count: 0,
        last_send_timestamp: None,
        transaction_signature: CompressedSignature::default(),
        mined_height: None,
        mined_in_block: None,
        mined_timestamp: Utc::now().checked_add_days(Days::new(1)),
        payment_id: MemoField::new_open_from_string(description, TxType::PaymentToOther).unwrap(),
        change_output_hashes: vec![],
        received_output_hashes: vec![],
        sent_output_hashes: vec![],
        lock_height: 0,
        rejection_reason: None,
    }
}

#[tokio::test]
async fn test_completed_transactions_ordering() {
    let factories = CryptoFactories::default();
    let connection = make_wallet_database_memory_connection();
    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;
    let tx_backend = alice_ts_interface.ts_db;

    let source_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();
    let destination_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();

    for i in 1u32..5u32 {
        let completed_tx = create_mock_completed_transaction(
            source_address.clone(),
            destination_address.clone(),
            MicroMinotari::from(1000),
            TransactionDirection::Outbound,
            "Yo!",
        );

        tx_backend
            .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
                u64::from(i).into(),
                Box::new(completed_tx),
            )))
            .unwrap();
    }

    let alice_completed_transactions = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions(None, None, None, 0)
        .await
        .unwrap();

    let mut mined_timestamps: Vec<_> = alice_completed_transactions
        .iter()
        .map(|tx| tx.mined_timestamp.unwrap_or_default())
        .collect();
    mined_timestamps.sort_by(|a, b| b.cmp(a));

    assert_eq!(alice_completed_transactions.len(), 4);
    assert_eq!(
        alice_completed_transactions
            .iter()
            .map(|tx| tx.mined_timestamp.unwrap_or_default())
            .collect::<Vec<_>>(),
        mined_timestamps
    );
}

#[tokio::test]
async fn test_get_completed_transactions_by_addresses() {
    let factories = CryptoFactories::default();
    let connection = make_wallet_database_memory_connection();
    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), connection, None).await;

    let alice_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();
    let bob_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();
    let carol_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();

    let alice_to_bob_1 = create_mock_completed_transaction(
        alice_address.clone(),
        bob_address.clone(),
        MicroMinotari::from(100),
        TransactionDirection::Outbound,
        "Alice to Bob 1",
    );
    alice_ts_interface
        .ts_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            alice_to_bob_1.tx_id,
            Box::new(alice_to_bob_1.clone()),
        )))
        .unwrap();
    let bob_to_alice_1 = create_mock_completed_transaction(
        bob_address.clone(),
        alice_address.clone(),
        MicroMinotari::from(200),
        TransactionDirection::Inbound,
        "Bob to Alice 1",
    );
    alice_ts_interface
        .ts_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            bob_to_alice_1.tx_id,
            Box::new(bob_to_alice_1.clone()),
        )))
        .unwrap();
    let alice_to_bob_2 = create_mock_completed_transaction(
        alice_address.clone(),
        bob_address.clone(),
        MicroMinotari::from(300),
        TransactionDirection::Outbound,
        "Alice to Bob 2",
    );
    alice_ts_interface
        .ts_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            alice_to_bob_2.tx_id,
            Box::new(alice_to_bob_2.clone()),
        )))
        .unwrap();
    let alice_to_carol_1 = create_mock_completed_transaction(
        alice_address.clone(),
        carol_address.clone(),
        MicroMinotari::from(400),
        TransactionDirection::Outbound,
        "Alice to Carol 1",
    );
    alice_ts_interface
        .ts_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            alice_to_carol_1.tx_id,
            Box::new(alice_to_carol_1.clone()),
        )))
        .unwrap();

    let alice_to_bob_txs = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions_by_addresses(Some(alice_address.clone()), Some(bob_address.clone()))
        .await
        .unwrap();
    assert_eq!(alice_to_bob_txs.len(), 2);
    assert!(alice_to_bob_txs.iter().any(|tx| tx.tx_id == alice_to_bob_1.tx_id));
    assert!(alice_to_bob_txs.iter().any(|tx| tx.tx_id == alice_to_bob_2.tx_id));

    let from_alice_txs = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions_by_addresses(Some(alice_address.clone()), None)
        .await
        .unwrap();
    assert_eq!(from_alice_txs.len(), 3);
    assert!(from_alice_txs.iter().any(|tx| tx.tx_id == alice_to_bob_1.tx_id));
    assert!(from_alice_txs.iter().any(|tx| tx.tx_id == alice_to_bob_2.tx_id));
    assert!(from_alice_txs.iter().any(|tx| tx.tx_id == alice_to_carol_1.tx_id));

    let to_bob_txs = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions_by_addresses(None, Some(bob_address.clone()))
        .await
        .unwrap();
    assert_eq!(to_bob_txs.len(), 2);
    assert!(to_bob_txs.iter().any(|tx| tx.tx_id == alice_to_bob_1.tx_id));
    assert!(to_bob_txs.iter().any(|tx| tx.tx_id == alice_to_bob_2.tx_id));

    let all_txs = alice_ts_interface
        .transaction_service_handle
        .get_completed_transactions_by_addresses(None, None)
        .await
        .unwrap();
    assert_eq!(all_txs.len(), 4);
}

/// Test that verifies ReplaceByFee fails when the must_include UTXOs from the original
/// transaction are not found in the OutputManagerHandle. This simulates scenarios where
/// the original transaction's inputs have been spent or are no longer available.
#[tokio::test]
async fn replace_by_fee_fails_when_must_include_utxos_not_found() {
    let factories = CryptoFactories::default();
    let db_connection = make_wallet_database_memory_connection();

    let mut alice_ts_interface = setup_transaction_service_no_comms(factories.clone(), db_connection, None).await;

    // Create a completed transaction that references inputs that don't exist in the output manager
    // This simulates a transaction where the original inputs have been spent/removed

    let alice_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();

    let bob_address = TariAddress::new_dual_address_with_default_features(
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut rand::rng())),
        Network::LocalNet,
    )
    .unwrap();

    // Create a mock completed transaction with fake input commitments that don't exist
    let tx_id = TxId::new_random();
    let amount = MicroMinotari::from(1000);

    // Create a fake transaction with inputs that won't be found in output manager
    let key_manager = &alice_ts_interface.key_manager_handle;

    // Create a fake input that doesn't exist in the output manager
    let fake_input = make_input(
        &mut rand::rng(),
        MicroMinotari::from(5000),
        &OutputFeatures::default(),
        key_manager.key_manager(),
    );

    // Build a transaction with this fake input
    let constants = ConsensusConstantsBuilder::new(Network::LocalNet).build();
    let mut builder = TransactionBuilder::new(constants, key_manager.clone(), Network::LocalNet).unwrap();

    builder.with_input(fake_input.clone()).unwrap();
    builder
        .with_fee_per_gram(MicroMinotari::from(5))
        .with_prevent_fee_gt_amount(false);

    // Add a recipient output
    builder
        .add_stealth_recipient(
            bob_address.clone(),
            amount,
            OutputFeatures::default(),
            MemoField::new_empty(),
        )
        .unwrap();

    let finalized = builder.build().unwrap();
    let fee = finalized.transaction.body.get_total_fee().unwrap();
    let tx = finalized.transaction;

    // Create a completed transaction record
    let completed_tx = CompletedTransaction::new(
        tx_id,
        alice_address.clone(),
        bob_address.clone(),
        amount,
        fee,
        tx,
        LegacyTransactionStatus::Broadcast,
        Utc::now(),
        TransactionDirection::Outbound,
        None,
        None,
        MemoField::new_empty(),
        0,
    )
    .unwrap();

    // Insert the completed transaction into the database
    alice_ts_interface
        .ts_db
        .write(WriteOperation::Insert(DbKeyValuePair::CompletedTransaction(
            tx_id,
            Box::new(completed_tx),
        )))
        .unwrap();

    // Now try to replace by fee - this should fail because the original inputs
    // are not in the output manager (they were never added)
    let fee_increase = MicroMinotari::from(100);
    let result = alice_ts_interface
        .transaction_service_handle
        .replace_by_fee(tx_id, fee_increase)
        .await;

    // The replace_by_fee should fail because the must_include UTXOs are not found
    assert!(
        result.is_err(),
        "ReplaceByFee should fail when must_include UTXOs are not found in output manager"
    );

    // Verify the error is related to UTXO selection failure
    let err = result.unwrap_err();
    // The error should be an OutputManagerError indicating no UTXOs were selected
    // because the must_include commitments don't exist
    assert!(
        matches!(
            err,
            minotari_wallet::transaction_service::error::TransactionServiceError::OutputManagerError(_)
        ),
        "Expected OutputManagerError, got: {:?}",
        err
    );
}
