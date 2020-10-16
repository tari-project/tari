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

use crate::support::utils::{make_input, random_string};
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use tari_comms_dht::DhtConfig;
use tari_core::transactions::{tari_amount::MicroTari, types::CryptoFactories};
use tari_crypto::keys::PublicKey;
use tari_p2p::initialization::CommsConfig;
use tari_shutdown::{Shutdown, ShutdownSignal};

use crate::support::comms_and_services::get_next_memory_address;
use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use digest::Digest;
use futures::{FutureExt, StreamExt};
use std::path::Path;
use tari_core::{
    consensus::Network,
    transactions::{tari_amount::uT, transaction::UnblindedOutput, types::PrivateKey},
};
use tari_crypto::common::Blake256;
use tari_p2p::transport::TransportType;
use tari_wallet::{
    contacts_service::storage::{
        database::Contact,
        memory_db::ContactsServiceMemoryDatabase,
        sqlite_db::ContactsServiceSqliteDatabase,
    },
    error::{WalletError, WalletStorageError},
    output_manager_service::storage::{memory_db::OutputManagerMemoryDatabase, sqlite_db::OutputManagerSqliteDatabase},
    storage::{
        database::WalletDatabase,
        memory_db::WalletMemoryDatabase,
        sqlite_db::WalletSqliteDatabase,
        sqlite_utilities::{partial_wallet_backup, run_migration_and_create_sqlite_connection},
    },
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionEvent,
        storage::{memory_db::TransactionMemoryDatabase, sqlite_db::TransactionServiceSqliteDatabase},
    },
    wallet::WalletConfig,
    Wallet,
    WalletSqlite,
};
use tempfile::tempdir;
use tokio::{runtime::Runtime, time::delay_for};

fn create_peer(public_key: CommsPublicKey, net_address: Multiaddr) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        net_address.into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
        &[],
        Default::default(),
    )
}

async fn create_wallet(
    node_identity: NodeIdentity,
    data_path: &Path,
    database_name: &str,
    factories: CryptoFactories,
    shutdown_signal: ShutdownSignal,
) -> WalletSqlite
{
    let comms_config = CommsConfig {
        node_identity: Arc::new(node_identity.clone()),
        transport_type: TransportType::Memory {
            listener_address: node_identity.public_address(),
        },
        datastore_path: data_path.to_path_buf(),
        peer_database_name: random_string(8),
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
    };

    let sql_database_path = comms_config
        .datastore_path
        .clone()
        .join(database_name)
        .with_extension("sqlite3");
    let connection = run_migration_and_create_sqlite_connection(&sql_database_path).expect("Could not open Sqlite db");

    let wallet_backend = WalletSqliteDatabase::new(connection.clone(), None).unwrap();
    let transaction_backend = TransactionServiceSqliteDatabase::new(connection.clone(), None);
    let output_manager_backend = OutputManagerSqliteDatabase::new(connection.clone(), None);
    let contacts_backend = ContactsServiceSqliteDatabase::new(connection);

    let transaction_service_config = TransactionServiceConfig {
        resend_response_cooldown: Duration::from_secs(1),
        ..Default::default()
    };

    let config = WalletConfig::new(
        comms_config,
        factories,
        Some(transaction_service_config),
        Network::Rincewind,
    );

    let wallet = Wallet::new(
        config,
        wallet_backend,
        transaction_backend,
        output_manager_backend,
        contacts_backend,
        shutdown_signal,
    )
    .await
    .unwrap();
    wallet
}

#[tokio_macros::test]
async fn test_wallet() {
    let shutdown_a = Shutdown::new();
    let shutdown_b = Shutdown::new();
    let db_tempdir = tempdir().unwrap();

    let factories = CryptoFactories::default();
    let alice_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let bob_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let base_node_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

    let mut alice_wallet = create_wallet(
        alice_identity.clone(),
        &db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
    )
    .await;

    let bob_wallet = create_wallet(
        bob_identity.clone(),
        &db_tempdir.path(),
        "bob_db",
        factories.clone(),
        shutdown_b.to_signal(),
    )
    .await;

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

    let mut alice_event_stream = alice_wallet.transaction_service.get_event_stream_fused();

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

    let mut delay = delay_for(Duration::from_secs(60)).fuse();
    let mut reply_count = false;
    loop {
        futures::select! {
            event = alice_event_stream.select_next_some() => match &*event.unwrap() {
                    TransactionEvent::ReceivedTransactionReply(_) => {
                        reply_count = true;
                        break;
                    },
                    _ => (),
                },

            () = delay => {
                break;
            },
        }
    }
    assert!(reply_count);

    let mut contacts = Vec::new();
    for i in 0..2 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: random_string(8),
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
    let current_wallet_path = db_tempdir.path().join("alice_db").with_extension("sqlite3");

    alice_wallet
        .apply_encryption("It's turtles all the way down".to_string())
        .await
        .unwrap();

    // Second encryption should fail
    match alice_wallet
        .apply_encryption("It's turtles all the way down".to_string())
        .await
    {
        Ok(_) => assert!(false, "Should not be able to encrypt twice"),
        Err(WalletError::WalletStorageError(WalletStorageError::AlreadyEncrypted)) => assert!(true),
        Err(_) => assert!(false, "Should be the Already Encrypted error"),
    }

    let connection =
        run_migration_and_create_sqlite_connection(&current_wallet_path).expect("Could not open Sqlite db");
    if let Err(WalletStorageError::InvalidEncryptionCipher) = WalletSqliteDatabase::new(connection.clone(), None) {
        assert!(true);
    } else {
        assert!(
            false,
            "Should not be able to instantiate encrypted wallet without cipher"
        );
    }
    let passphrase_hash = Blake256::new()
        .chain("wrong passphrase".to_string().as_bytes())
        .result()
        .to_vec();
    let key = GenericArray::from_slice(passphrase_hash.as_slice());
    let cipher = Aes256Gcm::new(key);
    let result = WalletSqliteDatabase::new(connection.clone(), Some(cipher));

    if let Err(WalletStorageError::AeadError(s)) = result {
        assert_eq!(s, "Decryption Error".to_string());
    } else {
        assert!(
            false,
            "Should not be able to instantiate encrypted wallet without cipher"
        );
    }

    let passphrase_hash = Blake256::new()
        .chain("It's turtles all the way down".to_string().as_bytes())
        .result()
        .to_vec();
    let key = GenericArray::from_slice(passphrase_hash.as_slice());
    let cipher = Aes256Gcm::new(key);
    let _ = WalletSqliteDatabase::new(connection.clone(), Some(cipher))
        .expect("Should be able to instantiate db with cipher");

    alice_wallet.remove_encryption().await.unwrap();

    let _ = WalletSqliteDatabase::new(connection.clone(), None).expect("Should be able to instantiate db with cipher");

    // Test the partial db backup in this test so that we can work with the data generated during the test
    let backup_wallet_path = db_tempdir.path().join("alice_db_backup").with_extension("sqlite3");

    alice_wallet
        .db
        .set_comms_secret_key(alice_identity.secret_key().clone())
        .await
        .unwrap();

    partial_wallet_backup(current_wallet_path.clone(), backup_wallet_path.clone())
        .await
        .unwrap();

    let connection =
        run_migration_and_create_sqlite_connection(&current_wallet_path).expect("Could not open Sqlite db");
    let wallet_db = WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap());
    let comms_private_key = wallet_db.get_comms_secret_key().await.unwrap();
    assert!(comms_private_key.is_some());
    // Checking that the backup has had its Comms Private Key is cleared.
    let connection = run_migration_and_create_sqlite_connection(&backup_wallet_path).expect("Could not open Sqlite db");
    let backup_wallet_db = WalletDatabase::new(WalletSqliteDatabase::new(connection.clone(), None).unwrap());
    let comms_private_key = backup_wallet_db.get_comms_secret_key().await.unwrap();
    assert!(comms_private_key.is_none());

    alice_wallet.wait_until_shutdown().await;
    bob_wallet.wait_until_shutdown().await;
}

#[test]
fn test_store_and_forward_send_tx() {
    let shutdown_a = Shutdown::new();
    let shutdown_b = Shutdown::new();
    let shutdown_c = Shutdown::new();
    let factories = CryptoFactories::default();
    let db_tempdir = tempdir().unwrap();

    let mut alice_runtime = Runtime::new().expect("Failed to initialize tokio runtime");
    let mut bob_runtime = Runtime::new().expect("Failed to initialize tokio runtime");
    let mut carol_runtime = Runtime::new().expect("Failed to initialize tokio runtime");

    let alice_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let bob_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let carol_identity =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    log::info!(
        "Alice = {}, Bob = {}, Carol = {}",
        alice_identity.node_id(),
        bob_identity.node_id(),
        carol_identity.node_id()
    );

    let mut alice_wallet = alice_runtime.block_on(create_wallet(
        alice_identity.clone(),
        &db_tempdir.path(),
        "alice_db",
        factories.clone(),
        shutdown_a.to_signal(),
    ));
    let bob_wallet = bob_runtime.block_on(create_wallet(
        bob_identity.clone(),
        &db_tempdir.path(),
        "bob_db",
        factories.clone(),
        shutdown_b.to_signal(),
    ));

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
    alice_runtime.block_on(async { delay_for(Duration::from_secs(10)).await });

    alice_runtime
        .block_on(alice_wallet.transaction_service.cancel_transaction(tx_id))
        .unwrap();

    alice_runtime.block_on(async { delay_for(Duration::from_secs(10)).await });

    let carol_wallet = carol_runtime.block_on(create_wallet(
        carol_identity.clone(),
        &db_tempdir.path(),
        "carol_db",
        factories.clone(),
        shutdown_c.to_signal(),
    ));

    let mut carol_event_stream = carol_wallet.transaction_service.get_event_stream_fused();
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
        let mut delay = delay_for(Duration::from_secs(60)).fuse();

        let mut tx_recv = false;
        let mut tx_cancelled = false;
        loop {
            futures::select! {
                event = carol_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransaction(_) => tx_recv = true,
                            TransactionEvent::TransactionCancelled(_) => tx_cancelled = true,
                        _ => (),
                    }
                    if tx_recv && tx_cancelled {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert!(tx_recv, "Must have received a tx from alice");
        assert!(tx_cancelled, "Must have received a cancel tx from alice");
    });
    alice_runtime.block_on(alice_wallet.wait_until_shutdown());
    bob_runtime.block_on(bob_wallet.wait_until_shutdown());
    carol_runtime.block_on(carol_wallet.wait_until_shutdown());
}

#[tokio_macros::test]
async fn test_import_utxo() {
    let shutdown = Shutdown::new();
    let factories = CryptoFactories::default();
    let alice_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/24521".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let base_node_identity = NodeIdentity::random(
        &mut OsRng,
        "/ip4/127.0.0.1/tcp/24522".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let temp_dir = tempdir().unwrap();
    let comms_config = CommsConfig {
        node_identity: Arc::new(alice_identity.clone()),
        transport_type: TransportType::Tcp {
            listener_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            tor_socks_config: None,
        },
        datastore_path: temp_dir.path().to_path_buf(),
        peer_database_name: random_string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
    };
    let config = WalletConfig::new(comms_config, factories.clone(), None, Network::Rincewind);
    let mut alice_wallet = Wallet::new(
        config,
        WalletMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
        shutdown.to_signal(),
    )
    .await
    .unwrap();

    let utxo = UnblindedOutput::new(20000 * uT, PrivateKey::default(), None);

    let tx_id = alice_wallet
        .import_utxo(
            utxo.value,
            &utxo.spending_key,
            base_node_identity.public_key(),
            "Testing".to_string(),
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
}

#[cfg(feature = "test_harness")]
#[tokio_macros::test]
async fn test_data_generation() {
    let shutdown = Shutdown::new();
    use tari_wallet::testnet_utils::generate_wallet_test_data;
    let factories = CryptoFactories::default();
    let node_id =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let temp_dir = tempdir().unwrap();
    let comms_config = CommsConfig {
        node_identity: Arc::new(node_id.clone()),
        transport_type: TransportType::Memory {
            listener_address: "/memory/0".parse().unwrap(),
        },
        datastore_path: temp_dir.path().to_path_buf(),
        peer_database_name: random_string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_millis(500),
            ..Default::default()
        },
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        user_agent: "tari/test-wallet".to_string(),
    };

    let config = WalletConfig::new(comms_config, factories, None, Network::Rincewind);

    let transaction_backend = TransactionMemoryDatabase::new();

    let mut wallet = Wallet::new(
        config,
        WalletMemoryDatabase::new(),
        transaction_backend.clone(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
        shutdown.to_signal(),
    )
    .await
    .unwrap();

    generate_wallet_test_data(&mut wallet, temp_dir.path(), transaction_backend)
        .await
        .unwrap();

    let contacts = wallet.contacts_service.get_contacts().await.unwrap();
    assert!(contacts.len() > 0);

    let balance = wallet.output_manager_service.get_balance().await.unwrap();
    assert!(balance.available_balance > MicroTari::from(0));

    // TODO Put this back when the new comms goes in and we use the new Event bus
    //    let outbound_tx = wallet
    //        .runtime
    //        .block_on(wallet.transaction_service.get_pending_outbound_transactions())
    //        .unwrap();
    //    assert!(outbound_tx.len() > 0);

    let completed_tx = wallet.transaction_service.get_completed_transactions().await.unwrap();
    assert!(completed_tx.len() > 0);

    wallet.wait_until_shutdown().await;
}
