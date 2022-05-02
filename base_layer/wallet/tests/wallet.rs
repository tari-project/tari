// Copyright 2021. The Tari Project
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

//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{panic, path::Path, sync::Arc, time::Duration};

use rand::{rngs::OsRng, Rng};
use support::{comms_and_services::get_next_memory_address, utils::make_input};
use tari_common::configuration::StringList;
use tari_common_types::{
    chain_metadata::ChainMetadata,
    transaction::TransactionStatus,
    types::{PrivateKey, PublicKey},
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use tari_comms_dht::{store_forward::SafConfig, DhtConfig};
use tari_core::{
    covenants::Covenant,
    transactions::{
        tari_amount::{uT, MicroTari},
        test_helpers::{create_unblinded_output, TestParams},
        transaction_components::OutputFeatures,
        CryptoFactories,
    },
};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey};
use tari_key_manager::{cipher_seed::CipherSeed, mnemonic::Mnemonic};
use tari_p2p::{
    comms_connector::InboundDomainConnector,
    initialization::initialize_local_test_comms,
    transport::MemoryTransportConfig,
    Network,
    P2pConfig,
    PeerSeedsConfig,
    TcpTransportConfig,
    TransportConfig,
};
use tari_script::{inputs, script};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::{collect_recv, random};
use tari_utilities::Hashable;
use tari_wallet::{
    contacts_service::{
        handle::ContactsLivenessEvent,
        service::ContactMessageType,
        storage::{database::Contact, sqlite_db::ContactsServiceSqliteDatabase},
    },
    error::{WalletError, WalletStorageError},
    key_manager_service::storage::sqlite_db::KeyManagerSqliteDatabase,
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::{
        database::{DbKeyValuePair, WalletBackend, WalletDatabase, WriteOperation},
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::{
            initialize_sqlite_database_backends,
            partial_wallet_backup,
            run_migration_and_create_sqlite_connection,
        },
    },
    test_utils::make_wallet_database_connection,
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionEvent,
        storage::sqlite_db::TransactionServiceSqliteDatabase,
    },
    wallet::read_or_create_master_seed,
    Wallet,
    WalletConfig,
    WalletSqlite,
};
use tempfile::tempdir;
use tokio::{sync::mpsc, time::sleep};

pub mod support;
use tari_wallet::output_manager_service::storage::database::OutputManagerDatabase;

fn create_peer(public_key: CommsPublicKey, net_address: Multiaddr) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key),
        net_address.into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
        Default::default(),
        Default::default(),
    )
}

async fn create_wallet(
    data_path: &Path,
    database_name: &str,
    factories: CryptoFactories,
    shutdown_signal: ShutdownSignal,
    passphrase: Option<String>,
    recovery_seed: Option<CipherSeed>,
) -> Result<WalletSqlite, WalletError> {
    const NETWORK: Network = Network::LocalNet;
    let node_identity = NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let comms_config = P2pConfig {
        override_from: None,
        public_address: None,
        transport: TransportConfig::new_memory(MemoryTransportConfig {
            listener_address: node_identity.public_address(),
        }),
        datastore_path: data_path.to_path_buf(),
        peer_database_name: random::string(8),
        max_concurrent_inbound_tasks: 10,
        max_concurrent_outbound_tasks: 10,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_secs(1),
            auto_join: true,
            saf: SafConfig {
                auto_request: true,
                ..Default::default()
            },
            ..Default::default()
        },
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: StringList::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
        auxiliary_tcp_listener_address: None,
        rpc_max_simultaneous_sessions: 0,
    };

    let sql_database_path = comms_config
        .datastore_path
        .clone()
        .join(database_name)
        .with_extension("sqlite3");

    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend, key_manager_backend) =
        initialize_sqlite_database_backends(sql_database_path, passphrase, 16).unwrap();

    let transaction_service_config = TransactionServiceConfig {
        resend_response_cooldown: Duration::from_secs(1),
        ..Default::default()
    };

    let config = WalletConfig {
        p2p: comms_config,
        transaction_service_config,
        network: NETWORK,
        contacts_auto_ping_interval: Duration::from_secs(5),
        ..Default::default()
    };

    let metadata = ChainMetadata::new(std::i64::MAX as u64, Vec::new(), 0, 0, 0);

    let _db_value = wallet_backend.write(WriteOperation::Insert(DbKeyValuePair::BaseNodeChainMetadata(metadata)));

    let wallet_db = WalletDatabase::new(wallet_backend);
    let master_seed = read_or_create_master_seed(recovery_seed, &wallet_db).await?;

    Wallet::start(
        config,
        PeerSeedsConfig::default(),
        Arc::new(node_identity.clone()),
        factories,
        wallet_db,
        transaction_backend,
        output_manager_backend,
        contacts_backend,
        key_manager_backend,
        shutdown_signal,
        master_seed,
    )
    .await
}

#[tokio::test]
async fn test_wallet() {
    let mut shutdown_a = Shutdown::new();
    let mut shutdown_b = Shutdown::new();
    let alice_db_tempdir = tempdir().unwrap();
    let bob_db_tempdir = tempdir().unwrap();

    let factories = CryptoFactories::default();

    let base_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);

    let mut alice_wallet = create_wallet(
        alice_db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();
    let alice_identity = (*alice_wallet.comms.node_identity()).clone();

    let bob_wallet = create_wallet(
        bob_db_tempdir.path(),
        "bob_db",
        factories.clone(),
        shutdown_b.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();
    let bob_identity = (*bob_wallet.comms.node_identity()).clone();

    alice_wallet
        .comms
        .peer_manager()
        .add_peer(create_peer(
            bob_identity.public_key().clone(),
            bob_identity.public_address(),
        ))
        .await
        .unwrap();

    bob_wallet
        .comms
        .peer_manager()
        .add_peer(create_peer(
            alice_identity.public_key().clone(),
            alice_identity.public_address(),
        ))
        .await
        .unwrap();

    alice_wallet
        .set_base_node_peer(
            (*base_node_identity.public_key()).clone(),
            base_node_identity.public_address().clone(),
        )
        .await
        .unwrap();

    let mut alice_event_stream = alice_wallet.transaction_service.get_event_stream();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment, None).await;

    alice_wallet.output_manager_service.add_output(uo1, None).await.unwrap();

    alice_wallet
        .transaction_service
        .send_transaction(
            bob_identity.public_key().clone(),
            value,
            MicroTari::from(5),
            "".to_string(),
        )
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    let mut received_reply = false;
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => if let TransactionEvent::ReceivedTransactionReply(_) = &*event.unwrap() {
                received_reply = true;
                break;
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(received_reply);

    let mut contacts = Vec::new();
    for i in 0..2 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact::new(random::string(8), public_key, None, None));

        alice_wallet
            .contacts_service
            .upsert_contact(contacts[i].clone())
            .await
            .unwrap();
    }

    let got_contacts = alice_wallet.contacts_service.get_contacts().await.unwrap();
    assert_eq!(contacts, got_contacts);

    // Test applying and removing encryption
    let current_wallet_path = alice_db_tempdir.path().join("alice_db").with_extension("sqlite3");

    alice_wallet
        .apply_encryption("It's turtles all the way down".to_string())
        .await
        .unwrap();

    // Second encryption should fail
    #[allow(clippy::match_wild_err_arm)]
    match alice_wallet
        .apply_encryption("It's turtles all the way down".to_string())
        .await
    {
        Ok(_) => panic!("Should not be able to encrypt twice"),
        Err(WalletError::WalletStorageError(WalletStorageError::AlreadyEncrypted)) => {},
        Err(_) => panic!("Should be the Already Encrypted error"),
    }

    drop(alice_event_stream);
    shutdown_a.trigger();
    alice_wallet.wait_until_shutdown().await;

    let connection =
        run_migration_and_create_sqlite_connection(&current_wallet_path, 16).expect("Could not open Sqlite db");

    if WalletSqliteDatabase::new(connection.clone(), None).is_ok() {
        panic!("Should not be able to instantiate encrypted wallet without cipher");
    }

    let result = WalletSqliteDatabase::new(connection.clone(), Some("wrong passphrase".to_string()));

    if let Err(err) = result {
        assert!(matches!(err, WalletStorageError::InvalidPassphrase));
    } else {
        panic!("Should not be able to instantiate encrypted wallet without cipher");
    }

    let db = WalletSqliteDatabase::new(connection, Some("It's turtles all the way down".to_string()))
        .expect("Should be able to instantiate db with cipher");
    drop(db);

    let mut shutdown_a = Shutdown::new();
    let mut alice_wallet = create_wallet(
        alice_db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
        Some("It's turtles all the way down".to_string()),
        None,
    )
    .await
    .unwrap();

    alice_wallet.remove_encryption().await.unwrap();

    shutdown_a.trigger();
    alice_wallet.wait_until_shutdown().await;

    let connection =
        run_migration_and_create_sqlite_connection(&current_wallet_path, 16).expect("Could not open Sqlite db");
    let db = WalletSqliteDatabase::new(connection, None).expect(
        "Should be able to instantiate db with
    cipher",
    );
    drop(db);

    // Test the partial db backup in this test so that we can work with the data generated during the test
    let mut shutdown_a = Shutdown::new();
    let alice_wallet = create_wallet(
        alice_db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();

    let backup_db_tempdir = tempdir().unwrap();
    let backup_wallet_path = backup_db_tempdir
        .path()
        .join("alice_db_backup")
        .with_extension("sqlite3");

    let alice_seed = CipherSeed::new();

    alice_wallet.db.set_master_seed(alice_seed).await.unwrap();

    shutdown_a.trigger();
    alice_wallet.wait_until_shutdown().await;

    partial_wallet_backup(current_wallet_path.clone(), backup_wallet_path.clone())
        .await
        .unwrap();

    let connection =
        run_migration_and_create_sqlite_connection(&current_wallet_path, 16).expect("Could not open Sqlite db");
    let wallet_db = WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap());
    let master_seed = wallet_db.get_master_seed().await.unwrap();
    assert!(master_seed.is_some());
    // Checking that the backup has had its Comms Private Key is cleared.
    let connection = run_migration_and_create_sqlite_connection(&backup_wallet_path, 16).expect(
        "Could not open Sqlite
    db",
    );
    let backup_wallet_db = WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap());
    let master_seed = backup_wallet_db.get_master_seed().await.unwrap();
    assert!(master_seed.is_none());

    shutdown_b.trigger();

    bob_wallet.wait_until_shutdown().await;
}

#[tokio::test]
async fn test_do_not_overwrite_master_key() {
    let factories = CryptoFactories::default();
    let dir = tempdir().unwrap();

    // create a wallet and shut it down
    let mut shutdown = Shutdown::new();
    let recovery_seed = CipherSeed::new();
    let wallet = create_wallet(
        dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_seed),
    )
    .await
    .unwrap();
    shutdown.trigger();
    wallet.wait_until_shutdown().await;

    // try to use a new master key to create a wallet using the existing wallet database
    let shutdown = Shutdown::new();
    let recovery_seed = CipherSeed::new();
    match create_wallet(
        dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_seed.clone()),
    )
    .await
    {
        Ok(_) => panic!("Should not be able to overwrite wallet master secret key!"),
        Err(e) => assert!(matches!(e, WalletError::WalletRecoveryError(_))),
    }

    // make sure we can create a new wallet with recovery key if the db file does not exist
    let dir = tempdir().unwrap();
    let _wallet = create_wallet(
        dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_seed),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn test_sign_message() {
    let factories = CryptoFactories::default();
    let dir = tempdir().unwrap();

    let shutdown = Shutdown::new();
    let mut wallet = create_wallet(
        dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();

    let (secret, public_key) = PublicKey::random_keypair(&mut OsRng);
    let (nonce, public_nonce) = PublicKey::random_keypair(&mut OsRng);
    let message = "Tragedy will find us.";
    let schnorr = wallet.sign_message(secret, nonce, message).unwrap();
    let signature = schnorr.get_signature().clone();

    assert!(wallet.verify_message_signature(public_key, public_nonce, signature, message.into()));
}

#[test]
fn test_many_iterations_store_and_forward_send_tx() {
    for _n in 1..=10 {
        test_store_and_forward_send_tx();
    }
}

#[tokio::test]
async fn test_store_and_forward_send_tx() {
    let shutdown_a = Shutdown::new();
    let shutdown_c = Shutdown::new();
    let factories = CryptoFactories::default();
    let alice_db_tempdir = tempdir().unwrap();
    let carol_db_tempdir = tempdir().unwrap();
    let base_node_tempdir = tempdir().unwrap();

    let mut alice_wallet = create_wallet(
        alice_db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();

    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        "/memory/0".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    ));
    let (tx, _rx) = mpsc::channel(100);
    let (base_node, _dht, _msg_sender) = initialize_local_test_comms(
        base_node_identity,
        InboundDomainConnector::new(tx),
        &base_node_tempdir,
        Duration::from_secs(5),
        vec![],
        shutdown_a.to_signal(),
    )
    .await
    .unwrap();

    let carol_wallet = create_wallet(
        carol_db_tempdir.path(),
        "carol_db",
        factories.clone(),
        shutdown_c.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();

    let carol_identity = carol_wallet.comms.node_identity();
    let mut carol_event_stream = carol_wallet.transaction_service.get_event_stream();

    alice_wallet
        .comms
        .peer_manager()
        .add_peer(base_node.node_identity_ref().to_peer())
        .await
        .unwrap();

    alice_wallet
        .comms
        .connectivity()
        .dial_peer(base_node.node_identity_ref().node_id().clone())
        .await
        .unwrap();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment, None).await;

    alice_wallet.output_manager_service.add_output(uo1, None).await.unwrap();

    let mut alice_events = alice_wallet.transaction_service.get_event_stream();
    alice_wallet
        .transaction_service
        .send_transaction(
            carol_identity.public_key().clone(),
            value,
            MicroTari::from(3),
            "Store and Forward!".to_string(),
        )
        .await
        .unwrap();

    let events = collect_recv!(alice_events, take = 2, timeout = Duration::from_secs(10));
    for evt in events {
        match &*evt {
            TransactionEvent::TransactionSendResult(_, result) => {
                assert!(result.store_and_forward_send_result);
            },
            _ => {},
        }
    }

    // Carol makes herself known to the network after discovery/the transaction has been sent
    carol_wallet
        .comms
        .peer_manager()
        .add_peer(base_node.node_identity_ref().to_peer())
        .await
        .unwrap();
    carol_wallet
        .comms
        .connectivity()
        .dial_peer(base_node.node_identity_ref().node_id().clone())
        .await
        .unwrap();

    carol_wallet.dht_service.dht_requester().send_join().await.unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);

    let mut tx_recv = false;
    loop {
        tokio::select! {
            event = carol_event_stream.recv() => {
                match &*event.unwrap() {
                    TransactionEvent::ReceivedTransaction(_) => tx_recv = true,
                    _ => (),
                }
                if tx_recv {
                    break;
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(tx_recv, "Must have received a tx from alice");
}

#[tokio::test]
async fn test_import_utxo() {
    let factories = CryptoFactories::default();
    let shutdown = Shutdown::new();
    let alice_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/24521".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    ));
    let base_node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/24522".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    ));
    let temp_dir = tempdir().unwrap();
    let (connection, _temp_dir) = make_wallet_database_connection(None);
    let comms_config = P2pConfig {
        override_from: None,
        public_address: None,
        transport: TransportConfig::new_tcp(TcpTransportConfig {
            listener_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            tor_socks_address: None,
            tor_socks_auth: Default::default(),
        }),
        datastore_path: temp_dir.path().to_path_buf(),
        peer_database_name: random::string(8),
        max_concurrent_inbound_tasks: 10,
        max_concurrent_outbound_tasks: 10,
        outbound_buffer_size: 10,
        dht: Default::default(),
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: StringList::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
        auxiliary_tcp_listener_address: None,
        rpc_max_simultaneous_sessions: 0,
    };
    let config = WalletConfig {
        p2p: comms_config,
        network: Network::Weatherwax,
        ..Default::default()
    };

    let mut alice_wallet = Wallet::start(
        config,
        PeerSeedsConfig::default(),
        alice_identity.clone(),
        factories.clone(),
        WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap()),
        TransactionServiceSqliteDatabase::new(connection.clone(), None),
        OutputManagerSqliteDatabase::new(connection.clone(), None),
        ContactsServiceSqliteDatabase::new(connection.clone()),
        KeyManagerSqliteDatabase::new(connection.clone(), None).unwrap(),
        shutdown.to_signal(),
        CipherSeed::new(),
    )
    .await
    .unwrap();
    let key = PrivateKey::random(&mut OsRng);
    let claim = PublicKey::from_secret_key(&key);
    let script = script!(Nop);
    let input = inputs!(claim);
    let temp_features = OutputFeatures::create_coinbase(50, rand::thread_rng().gen::<u8>());

    let p = TestParams::new();
    let utxo = create_unblinded_output(script.clone(), temp_features, &p, 20000 * uT);
    let output = utxo.as_transaction_output(&factories).unwrap();
    let expected_output_hash = output.hash();

    let tx_id = alice_wallet
        .import_external_utxo_as_non_rewindable(
            utxo.value,
            &utxo.spending_key,
            script.clone(),
            input.clone(),
            base_node_identity.public_key(),
            utxo.features.clone(),
            "Testing".to_string(),
            utxo.metadata_signature.clone(),
            &p.script_private_key,
            &p.sender_offset_public_key,
            0,
            Covenant::default(),
        )
        .await
        .unwrap();

    let balance = alice_wallet.output_manager_service.get_balance().await.unwrap();

    assert_eq!(balance.pending_incoming_balance, 20000 * uT);

    let completed_tx = alice_wallet
        .transaction_service
        .get_completed_transactions()
        .await
        .unwrap()
        .remove(&tx_id)
        .expect("Tx should be in collection");

    assert_eq!(completed_tx.amount, 20000 * uT);
    assert_eq!(completed_tx.status, TransactionStatus::Imported);
    let db = OutputManagerDatabase::new(OutputManagerSqliteDatabase::new(connection, None));
    let outputs = db.fetch_outputs_by_tx_id(tx_id).unwrap();
    assert!(outputs.iter().any(|o| { o.hash == expected_output_hash }));
}

#[test]
fn test_db_file_locking() {
    let db_tempdir = tempdir().unwrap();
    let wallet_path = db_tempdir.path().join("alice_db").with_extension("sqlite3");

    let connection = run_migration_and_create_sqlite_connection(&wallet_path, 16).expect("Could not open Sqlite db");

    match run_migration_and_create_sqlite_connection(&wallet_path, 16) {
        Err(WalletStorageError::CannotAcquireFileLock) => {},
        _ => panic!("Should not be able to acquire file lock"),
    }

    drop(connection);

    assert!(run_migration_and_create_sqlite_connection(&wallet_path, 16).is_ok());
}

#[tokio::test]
async fn test_recovery_birthday() {
    let dir = tempdir().unwrap();
    let factories = CryptoFactories::default();
    let shutdown = Shutdown::new();

    let seed_words: Vec<String> = [
        "cactus", "pool", "fuel", "skull", "chair", "casino", "season", "disorder", "flat", "crash", "wrist",
        "whisper", "decorate", "narrow", "oxygen", "remember", "minor", "among", "happy", "cricket", "embark", "blue",
        "ship", "sick",
    ]
    .iter()
    .map(|w| w.to_string())
    .collect();

    let recovery_seed = CipherSeed::from_mnemonic(seed_words.as_slice(), None).unwrap();
    let birthday = recovery_seed.birthday();

    let wallet = create_wallet(
        dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_seed),
    )
    .await
    .unwrap();

    let db_birthday = wallet.db.get_wallet_birthday().await.unwrap();
    assert_eq!(birthday, db_birthday);
}

#[tokio::test]
async fn test_contacts_service_liveness() {
    let mut shutdown_a = Shutdown::new();
    let mut shutdown_b = Shutdown::new();
    let factories = CryptoFactories::default();
    let alice_db_tempdir = tempdir().unwrap();
    let bob_db_tempdir = tempdir().unwrap();

    let mut alice_wallet = create_wallet(
        alice_db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();
    let alice_identity = alice_wallet.comms.node_identity();

    let mut bob_wallet = create_wallet(
        bob_db_tempdir.path(),
        "bob_db",
        factories,
        shutdown_b.to_signal(),
        None,
        None,
    )
    .await
    .unwrap();
    let bob_identity = (*bob_wallet.comms.node_identity()).clone();

    alice_wallet
        .comms
        .peer_manager()
        .add_peer(bob_identity.to_peer())
        .await
        .unwrap();
    let contact_bob = Contact::new(random::string(8), bob_identity.public_key().clone(), None, None);
    alice_wallet.contacts_service.upsert_contact(contact_bob).await.unwrap();

    bob_wallet
        .comms
        .peer_manager()
        .add_peer(alice_identity.to_peer())
        .await
        .unwrap();
    let contact_alice = Contact::new(random::string(8), alice_identity.public_key().clone(), None, None);
    bob_wallet.contacts_service.upsert_contact(contact_alice).await.unwrap();

    alice_wallet
        .comms
        .connectivity()
        .dial_peer(bob_identity.node_id().clone())
        .await
        .unwrap();

    let mut liveness_event_stream_alice = alice_wallet.contacts_service.get_contacts_liveness_event_stream();
    let delay = sleep(Duration::from_secs(15));
    tokio::pin!(delay);
    let mut ping_count = 0;
    let mut pong_count = 0;
    loop {
        tokio::select! {
            event = liveness_event_stream_alice.recv() => {
                if let ContactsLivenessEvent::StatusUpdated(data) = &*event.unwrap() {
                    if data.public_key() == bob_identity.public_key(){
                        assert_eq!(data.node_id(), bob_identity.node_id());
                        match data.message_type()  {
                            ContactMessageType::Ping  => {
                                ping_count += 1;
                            }
                            ContactMessageType::Pong => {
                                pong_count += 1;
                            }
                            _ => {}
                        }
                    }
                    if ping_count > 1 && pong_count > 1 {
                        break;
                    }
                }
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(ping_count > 1);
    assert!(pong_count > 1);

    let mut liveness_event_stream_bob = bob_wallet.contacts_service.get_contacts_liveness_event_stream();
    let timeout = sleep(Duration::from_secs(50));
    tokio::pin!(timeout);
    let mut ping_count = 0;
    let mut pong_count = 0;
    loop {
        tokio::select! {
            event = liveness_event_stream_bob.recv() => {
                if let ContactsLivenessEvent::StatusUpdated(data) = &*event.unwrap() {
                    if data.public_key() == alice_identity.public_key(){
                        assert_eq!(data.node_id(), alice_identity.node_id());
                        if data.message_type() == ContactMessageType::Ping {
                            ping_count += 1;
                        } else if data.message_type() == ContactMessageType::Pong {
                            pong_count += 1;
                        } else {}
                    }
                    if ping_count > 1 && pong_count > 1 {
                        break;
                    }
                }
            },
            () = &mut timeout => {
                break;
            },
        }
    }
    assert!(ping_count > 1);
    assert!(pong_count > 1);

    shutdown_a.trigger();
    shutdown_b.trigger();
    alice_wallet.wait_until_shutdown().await;
    bob_wallet.wait_until_shutdown().await;
}
