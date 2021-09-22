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

use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use digest::Digest;
use rand::rngs::OsRng;
use tari_crypto::{
    common::Blake256,
    inputs,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    script,
};
use tempfile::tempdir;
use tokio::runtime::Runtime;

use tari_common_types::{
    chain_metadata::ChainMetadata,
    types::{PrivateKey, PublicKey},
};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_comms_dht::DhtConfig;
use tari_core::transactions::{
    helpers::{create_unblinded_output, TestParams},
    tari_amount::{uT, MicroTari},
    transaction::OutputFeatures,
    CryptoFactories,
};
use tari_p2p::{initialization::P2pConfig, transport::TransportType, Network, DEFAULT_DNS_NAME_SERVER};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::random;
use tari_wallet::{
    contacts_service::storage::{database::Contact, sqlite_db::ContactsServiceSqliteDatabase},
    error::{WalletError, WalletStorageError},
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::{
        database::{DbKeyValuePair, WalletBackend, WalletDatabase, WriteOperation},
        sqlite_db::WalletSqliteDatabase,
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
    Wallet,
    WalletConfig,
    WalletSqlite,
};
use tokio::time::sleep;

use crate::support::{comms_and_services::get_next_memory_address, utils::make_input};

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
    recovery_master_key: Option<CommsSecretKey>,
) -> Result<WalletSqlite, WalletError> {
    const NETWORK: Network = Network::Weatherwax;
    let node_identity = NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let comms_config = P2pConfig {
        network: NETWORK,
        node_identity: Arc::new(node_identity.clone()),
        transport_type: TransportType::Memory {
            listener_address: node_identity.public_address(),
        },
        auxilary_tcp_listener_address: None,
        datastore_path: data_path.to_path_buf(),
        peer_database_name: random::string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_secs(1),
            auto_join: true,
            saf_auto_request: true,
            ..Default::default()
        },
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
        dns_seeds_name_server: DEFAULT_DNS_NAME_SERVER.parse().unwrap(),
        peer_seeds: Default::default(),
        dns_seeds: Default::default(),
        dns_seeds_use_dnssec: false,
    };

    let sql_database_path = comms_config
        .datastore_path
        .clone()
        .join(database_name)
        .with_extension("sqlite3");

    let (wallet_backend, transaction_backend, output_manager_backend, contacts_backend) =
        initialize_sqlite_database_backends(sql_database_path, passphrase).unwrap();

    let transaction_service_config = TransactionServiceConfig {
        resend_response_cooldown: Duration::from_secs(1),
        ..Default::default()
    };

    let config = WalletConfig::new(
        comms_config,
        factories,
        Some(transaction_service_config),
        None,
        NETWORK.into(),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let metadata = ChainMetadata::new(std::u64::MAX, Vec::new(), 0, 0, 0);

    let _ = wallet_backend.write(WriteOperation::Insert(DbKeyValuePair::BaseNodeChainMetadata(metadata)));

    Wallet::start(
        config,
        WalletDatabase::new(wallet_backend),
        transaction_backend,
        output_manager_backend,
        contacts_backend,
        shutdown_signal,
        recovery_master_key,
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
        &alice_db_tempdir.path(),
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
        &bob_db_tempdir.path(),
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
            get_next_memory_address().to_string(),
        )
        .await
        .unwrap();

    let mut alice_event_stream = alice_wallet.transaction_service.get_event_stream();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment);

    alice_wallet.output_manager_service.add_output(uo1).await.unwrap();

    alice_wallet
        .transaction_service
        .send_transaction(
            bob_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            "".to_string(),
        )
        .await
        .unwrap();

    let delay = sleep(Duration::from_secs(60));
    tokio::pin!(delay);
    let mut reply_count = false;
    loop {
        tokio::select! {
            event = alice_event_stream.recv() => if let TransactionEvent::ReceivedTransactionReply(_) = &*event.unwrap() {
                reply_count = true;
                break;
            },
            () = &mut delay => {
                break;
            },
        }
    }
    assert!(reply_count);

    let mut contacts = Vec::new();
    for i in 0..2 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: random::string(8),
            public_key,
        });

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
        run_migration_and_create_sqlite_connection(&current_wallet_path).expect("Could not open Sqlite db");

    if WalletSqliteDatabase::new(connection.clone(), None).is_ok() {
        panic!("Should not be able to instantiate encrypted wallet without cipher");
    }

    let passphrase_hash = Blake256::new()
        .chain("wrong passphrase".to_string().as_bytes())
        .finalize();
    let key = GenericArray::from_slice(passphrase_hash.as_slice());
    let cipher = Aes256Gcm::new(key);
    let result = WalletSqliteDatabase::new(connection.clone(), Some(cipher));

    if let Err(err) = result {
        assert!(matches!(err, WalletStorageError::IncorrectPassword));
    } else {
        panic!("Should not be able to instantiate encrypted wallet without cipher");
    }

    let passphrase_hash = Blake256::new()
        .chain("It's turtles all the way down".to_string().as_bytes())
        .finalize();
    let key = GenericArray::from_slice(passphrase_hash.as_slice());
    let cipher = Aes256Gcm::new(key);
    let db = WalletSqliteDatabase::new(connection, Some(cipher)).expect("Should be able to instantiate db with cipher");
    drop(db);

    let mut shutdown_a = Shutdown::new();
    let mut alice_wallet = create_wallet(
        &alice_db_tempdir.path(),
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
        run_migration_and_create_sqlite_connection(&current_wallet_path).expect("Could not open Sqlite db");
    let db = WalletSqliteDatabase::new(connection, None).expect(
        "Should be able to instantiate db with
    cipher",
    );
    drop(db);

    // Test the partial db backup in this test so that we can work with the data generated during the test
    let mut shutdown_a = Shutdown::new();
    let alice_wallet = create_wallet(
        &alice_db_tempdir.path(),
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

    alice_wallet
        .db
        .set_master_secret_key(alice_identity.secret_key().clone())
        .await
        .unwrap();

    shutdown_a.trigger();
    alice_wallet.wait_until_shutdown().await;

    partial_wallet_backup(current_wallet_path.clone(), backup_wallet_path.clone())
        .await
        .unwrap();

    let connection =
        run_migration_and_create_sqlite_connection(&current_wallet_path).expect("Could not open Sqlite db");
    let wallet_db = WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap());
    let master_private_key = wallet_db.get_master_secret_key().await.unwrap();
    assert!(master_private_key.is_some());
    // Checking that the backup has had its Comms Private Key is cleared.
    let connection = run_migration_and_create_sqlite_connection(&backup_wallet_path).expect(
        "Could not open Sqlite
    db",
    );
    let backup_wallet_db = WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap());
    let master_secret_key = backup_wallet_db.get_master_secret_key().await.unwrap();
    assert!(master_secret_key.is_none());

    shutdown_b.trigger();

    bob_wallet.wait_until_shutdown().await;
}

#[tokio::test]
async fn test_do_not_overwrite_master_key() {
    let factories = CryptoFactories::default();
    let dir = tempdir().unwrap();

    // create a wallet and shut it down
    let mut shutdown = Shutdown::new();
    let (recovery_master_key, _) = PublicKey::random_keypair(&mut OsRng);
    let wallet = create_wallet(
        &dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_master_key),
    )
    .await
    .unwrap();
    shutdown.trigger();
    wallet.wait_until_shutdown().await;

    // try to use a new master key to create a wallet using the existing wallet database
    let shutdown = Shutdown::new();
    let (recovery_master_key, _) = PublicKey::random_keypair(&mut OsRng);
    match create_wallet(
        &dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_master_key.clone()),
    )
    .await
    {
        Ok(_) => panic!("Should not be able to overwrite wallet master secret key!"),
        Err(e) => assert!(matches!(e, WalletError::WalletRecoveryError(_))),
    }

    // make sure we can create a new wallet with recovery key if the db file does not exist
    let dir = tempdir().unwrap();
    let _wallet = create_wallet(
        &dir.path(),
        "wallet_db",
        factories.clone(),
        shutdown.to_signal(),
        None,
        Some(recovery_master_key),
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
        &dir.path(),
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
#[ignore = "Useful for debugging, ignored because it takes over 30 minutes to run"]
#[allow(clippy::redundant_closure)]
fn test_20_store_and_forward_send_tx() {
    let mut fails = 0;
    for _n in 1..=20 {
        let hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let result = panic::catch_unwind(move || test_store_and_forward_send_tx());
        panic::set_hook(hook);
        match result {
            Ok(_) => {},
            Err(_) => {
                fails += 1;
            },
        }
    }
    assert_eq!(fails, 0);
}

#[test]
#[ignore = "Flakey on CI, theory is that it is due to SAF neighbourhoods. Retry after Kademlia style neighbourhoods \
            are included"]
fn test_store_and_forward_send_tx() {
    let mut shutdown_a = Shutdown::new();
    let mut shutdown_b = Shutdown::new();
    let mut shutdown_c = Shutdown::new();
    let mut shutdown_c2 = Shutdown::new();
    let factories = CryptoFactories::default();
    let alice_db_tempdir = tempdir().unwrap();
    let bob_db_tempdir = tempdir().unwrap();
    let carol_db_tempdir = tempdir().unwrap();

    let alice_runtime = Runtime::new().expect("Failed to initialize tokio runtime");
    let bob_runtime = Runtime::new().expect("Failed to initialize tokio runtime");
    let carol_runtime = Runtime::new().expect("Failed to initialize tokio runtime");

    let mut alice_wallet = alice_runtime
        .block_on(create_wallet(
            &alice_db_tempdir.path(),
            "alice_db",
            factories.clone(),
            shutdown_a.to_signal(),
            None,
            None,
        ))
        .unwrap();

    let bob_wallet = bob_runtime
        .block_on(create_wallet(
            &bob_db_tempdir.path(),
            "bob_db",
            factories.clone(),
            shutdown_b.to_signal(),
            None,
            None,
        ))
        .unwrap();
    let bob_identity = (*bob_wallet.comms.node_identity()).clone();

    let carol_wallet = carol_runtime
        .block_on(create_wallet(
            &carol_db_tempdir.path(),
            "carol_db",
            factories.clone(),
            shutdown_c.to_signal(),
            None,
            None,
        ))
        .unwrap();
    let carol_identity = (*carol_wallet.comms.node_identity()).clone();
    shutdown_c.trigger();
    carol_runtime.block_on(carol_wallet.wait_until_shutdown());

    alice_runtime
        .block_on(alice_wallet.comms.peer_manager().add_peer(bob_identity.to_peer()))
        .unwrap();

    bob_runtime
        .block_on(bob_wallet.comms.peer_manager().add_peer(carol_identity.to_peer()))
        .unwrap();

    alice_runtime
        .block_on(
            alice_wallet
                .comms
                .connectivity()
                .dial_peer(bob_identity.node_id().clone()),
        )
        .unwrap();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment);

    alice_runtime
        .block_on(alice_wallet.output_manager_service.add_output(uo1))
        .unwrap();

    let tx_id = alice_runtime
        .block_on(alice_wallet.transaction_service.send_transaction(
            carol_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            "Store and Forward!".to_string(),
        ))
        .unwrap();

    // Waiting here for a while to make sure the discovery retry is over
    alice_runtime.block_on(async { sleep(Duration::from_secs(60)).await });

    alice_runtime
        .block_on(alice_wallet.transaction_service.cancel_transaction(tx_id))
        .unwrap();

    alice_runtime.block_on(async { sleep(Duration::from_secs(60)).await });

    let carol_wallet = carol_runtime
        .block_on(create_wallet(
            &carol_db_tempdir.path(),
            "carol_db",
            factories,
            shutdown_c2.to_signal(),
            None,
            None,
        ))
        .unwrap();

    let mut carol_event_stream = carol_wallet.transaction_service.get_event_stream();

    carol_runtime
        .block_on(carol_wallet.comms.peer_manager().add_peer(create_peer(
            bob_identity.public_key().clone(),
            bob_identity.public_address(),
        )))
        .unwrap();
    carol_runtime
        .block_on(carol_wallet.dht_service.dht_requester().send_join())
        .unwrap();

    carol_runtime.block_on(async {
        let delay = sleep(Duration::from_secs(60));
        tokio::pin!(delay);

        let mut tx_recv = false;
        let mut tx_cancelled = false;
        loop {
            tokio::select! {
                event = carol_event_stream.recv() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransaction(_) => tx_recv = true,
                        TransactionEvent::TransactionCancelled(_) => tx_cancelled = true,
                        _ => (),
                    }
                    if tx_recv && tx_cancelled {
                        break;
                    }
                },
                () = &mut delay => {
                    break;
                },
            }
        }
        assert!(tx_recv, "Must have received a tx from alice");
        assert!(tx_cancelled, "Must have received a cancel tx from alice");
    });
    shutdown_a.trigger();
    shutdown_b.trigger();
    shutdown_c2.trigger();
    alice_runtime.block_on(alice_wallet.wait_until_shutdown());
    bob_runtime.block_on(bob_wallet.wait_until_shutdown());
    carol_runtime.block_on(carol_wallet.wait_until_shutdown());
}

#[tokio::test]
async fn test_import_utxo() {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();
    let alice_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/24521".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    );
    let base_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/24522".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    );
    let temp_dir = tempdir().unwrap();
    let (connection, _temp_dir) = make_wallet_database_connection(None);
    let comms_config = P2pConfig {
        network: Network::Weatherwax,
        node_identity: Arc::new(alice_identity.clone()),
        transport_type: TransportType::Tcp {
            listener_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            tor_socks_config: None,
        },
        auxilary_tcp_listener_address: None,
        datastore_path: temp_dir.path().to_path_buf(),
        peer_database_name: random::string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
        dns_seeds_name_server: DEFAULT_DNS_NAME_SERVER.parse().unwrap(),
        peer_seeds: Default::default(),
        dns_seeds: Default::default(),
        dns_seeds_use_dnssec: false,
    };
    let config = WalletConfig::new(
        comms_config,
        factories.clone(),
        None,
        None,
        Network::Weatherwax.into(),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let mut alice_wallet = Wallet::start(
        config,
        WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap()),
        TransactionServiceSqliteDatabase::new(connection.clone(), None),
        OutputManagerSqliteDatabase::new(connection.clone(), None),
        ContactsServiceSqliteDatabase::new(connection),
        shutdown.to_signal(),
        None,
    )
    .await
    .unwrap();
    let key = PrivateKey::random(&mut OsRng);
    let claim = PublicKey::from_secret_key(&key);
    let script = script!(Nop);
    let input = inputs!(claim);
    let features = OutputFeatures::create_coinbase(50);

    let p = TestParams::new();
    let utxo = create_unblinded_output(script.clone(), features.clone(), p.clone(), 20000 * uT);

    let tx_id = alice_wallet
        .import_utxo(
            utxo.value,
            &utxo.spending_key,
            script,
            input,
            base_node_identity.public_key(),
            features,
            "Testing".to_string(),
            utxo.metadata_signature.clone(),
            &p.script_private_key,
            &p.sender_offset_public_key,
        )
        .await
        .unwrap();

    let balance = alice_wallet.output_manager_service.get_balance().await.unwrap();

    assert_eq!(balance.available_balance, 20000 * uT);

    let completed_tx = alice_wallet
        .transaction_service
        .get_completed_transactions()
        .await
        .unwrap()
        .remove(&tx_id)
        .expect("Tx should be in collection");

    assert_eq!(completed_tx.amount, 20000 * uT);
    let stored_utxo = alice_wallet.output_manager_service.get_unspent_outputs().await.unwrap()[0].clone();
    assert_eq!(stored_utxo, utxo);
}

#[test]
fn test_db_file_locking() {
    let db_tempdir = tempdir().unwrap();
    let wallet_path = db_tempdir.path().join("alice_db").with_extension("sqlite3");

    let connection = run_migration_and_create_sqlite_connection(&wallet_path).expect("Could not open Sqlite db");

    match run_migration_and_create_sqlite_connection(&wallet_path) {
        Err(WalletStorageError::CannotAcquireFileLock) => {},
        _ => panic!("Should not be able to acquire file lock"),
    }

    drop(connection);

    assert!(run_migration_and_create_sqlite_connection(&wallet_path).is_ok());
}
