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
use tari_test_utils::paths::with_temp_dir;

use crate::support::comms_and_services::get_next_memory_address;
use futures::{FutureExt, StreamExt};
use std::path::Path;
use tari_core::transactions::{tari_amount::uT, transaction::UnblindedOutput, types::PrivateKey};
use tari_p2p::transport::TransportType;
use tari_wallet::{
    contacts_service::storage::{database::Contact, memory_db::ContactsServiceMemoryDatabase},
    output_manager_service::storage::memory_db::OutputManagerMemoryDatabase,
    storage::memory_db::WalletMemoryDatabase,
    transaction_service::{handle::TransactionEvent, storage::memory_db::TransactionMemoryDatabase},
    wallet::WalletConfig,
    Wallet,
};
use tempdir::TempDir;
use tokio::{runtime::Runtime, time::delay_for};

fn create_peer(public_key: CommsPublicKey, net_address: Multiaddr) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        net_address.into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
        &[],
    )
}

fn create_wallet(
    node_identity: NodeIdentity,
    data_path: &Path,
    factories: CryptoFactories,
) -> Wallet<WalletMemoryDatabase, TransactionMemoryDatabase, OutputManagerMemoryDatabase, ContactsServiceMemoryDatabase>
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
        listener_liveness_whitelist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
    };
    let config = WalletConfig {
        comms_config,
        factories,
        transaction_service_config: None,
    };
    let runtime_node = Runtime::new().unwrap();
    let wallet = Wallet::new(
        config,
        runtime_node,
        WalletMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
    )
    .unwrap();
    wallet
}

#[test]
fn test_wallet() {
    with_temp_dir(|dir_path| {
        let mut runtime = Runtime::new().unwrap();
        let factories = CryptoFactories::default();
        let alice_identity =
            NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
        let bob_identity =
            NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

        let base_node_identity =
            NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();

        let mut alice_wallet = create_wallet(alice_identity.clone(), dir_path, factories.clone());
        let mut bob_wallet = create_wallet(bob_identity.clone(), dir_path, factories.clone());

        alice_wallet
            .runtime
            .block_on(alice_wallet.comms.peer_manager().add_peer(create_peer(
                bob_identity.public_key().clone(),
                bob_identity.public_address(),
            )))
            .unwrap();

        bob_wallet
            .runtime
            .block_on(bob_wallet.comms.peer_manager().add_peer(create_peer(
                alice_identity.public_key().clone(),
                alice_identity.public_address(),
            )))
            .unwrap();

        alice_wallet
            .set_base_node_peer(
                (*base_node_identity.public_key()).clone(),
                get_next_memory_address().to_string(),
            )
            .unwrap();

        let mut alice_event_stream = alice_wallet.transaction_service.get_event_stream_fused();

        let value = MicroTari::from(1000);
        let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment);

        runtime
            .block_on(alice_wallet.output_manager_service.add_output(uo1))
            .unwrap();

        runtime
            .block_on(alice_wallet.transaction_service.send_transaction(
                bob_identity.public_key().clone(),
                value,
                MicroTari::from(20),
                "".to_string(),
            ))
            .unwrap();

        runtime.block_on(async {
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
        });

        let mut contacts = Vec::new();
        for i in 0..2 {
            let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

            contacts.push(Contact {
                alias: random_string(8),
                public_key,
            });

            runtime
                .block_on(alice_wallet.contacts_service.upsert_contact(contacts[i].clone()))
                .unwrap();
        }

        let got_contacts = runtime.block_on(alice_wallet.contacts_service.get_contacts()).unwrap();
        assert_eq!(contacts, got_contacts);
    });
}

#[test]
fn test_store_and_forward_send_tx() {
    let factories = CryptoFactories::default();
    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();

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

    let mut alice_wallet = create_wallet(alice_identity.clone(), &db_tempdir.path(), factories.clone());
    let mut bob_wallet = create_wallet(bob_identity.clone(), &db_tempdir.path(), factories.clone());
    let mut alice_event_stream = alice_wallet.transaction_service.get_event_stream_fused();

    alice_wallet
        .runtime
        .block_on(alice_wallet.comms.peer_manager().add_peer(bob_identity.to_peer()))
        .unwrap();

    bob_wallet
        .runtime
        .block_on(bob_wallet.comms.peer_manager().add_peer(carol_identity.to_peer()))
        .unwrap();

    alice_wallet
        .runtime
        .block_on(
            alice_wallet
                .comms
                .connectivity()
                .dial_peer(bob_identity.node_id().clone()),
        )
        .unwrap();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut OsRng, MicroTari(2500), &factories.commitment);

    alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.add_output(uo1))
        .unwrap();

    alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.send_transaction(
            carol_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            "Store and Forward!".to_string(),
        ))
        .unwrap();

    // Waiting here for a while to make sure the discovery retry is over
    alice_wallet
        .runtime
        .block_on(async { delay_for(Duration::from_secs(10)).await });

    let mut carol_wallet = create_wallet(carol_identity.clone(), &db_tempdir.path(), factories.clone());

    carol_wallet
        .runtime
        .block_on(carol_wallet.comms.peer_manager().add_peer(create_peer(
            bob_identity.public_key().clone(),
            bob_identity.public_address(),
        )))
        .unwrap();
    carol_wallet
        .runtime
        .block_on(carol_wallet.dht_service.dht_requester().send_join())
        .unwrap();

    alice_wallet.runtime.block_on(async {
        let mut delay = delay_for(Duration::from_secs(60)).fuse();
        let mut tx_reply = 0;
        loop {
            futures::select! {
                event = alice_event_stream.select_next_some() => {
                    match &*event.unwrap() {
                        TransactionEvent::ReceivedTransactionReply(_) => tx_reply+=1,
                        _ => (),
                    }
                    if tx_reply == 1 {
                        break;
                    }
                },
                () = delay => {
                    break;
                },
            }
        }
        assert_eq!(tx_reply, 1, "Must have received a reply from Carol");
    });

    alice_wallet.shutdown();
    bob_wallet.shutdown();
    carol_wallet.shutdown();
}

#[test]
fn test_import_utxo() {
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
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
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
        listener_liveness_whitelist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
    };
    let config = WalletConfig {
        comms_config,
        factories: factories.clone(),
        transaction_service_config: None,
    };
    let runtime_node = Runtime::new().unwrap();
    let mut alice_wallet = Wallet::new(
        config,
        runtime_node,
        WalletMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
    )
    .unwrap();

    let utxo = UnblindedOutput::new(20000 * uT, PrivateKey::default(), None);

    let tx_id = alice_wallet
        .import_utxo(
            utxo.value,
            &utxo.spending_key,
            base_node_identity.public_key(),
            "Testing".to_string(),
        )
        .unwrap();

    let balance = alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.get_balance())
        .unwrap();

    assert_eq!(balance.available_balance, 20000 * uT);

    let completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap()
        .remove(&tx_id)
        .expect("Tx should be in collection");

    assert_eq!(completed_tx.amount, 20000 * uT);
}

#[cfg(feature = "test_harness")]
#[test]
fn test_data_generation() {
    use tari_wallet::testnet_utils::generate_wallet_test_data;
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();
    let node_id =
        NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE).unwrap();
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
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
        listener_liveness_whitelist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
    };

    let config = WalletConfig {
        comms_config,
        factories,
        transaction_service_config: None,
    };

    let transaction_backend = TransactionMemoryDatabase::new();

    let mut wallet = Wallet::new(
        config,
        runtime,
        WalletMemoryDatabase::new(),
        transaction_backend.clone(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
    )
    .unwrap();

    generate_wallet_test_data(&mut wallet, temp_dir.path(), transaction_backend).unwrap();

    let contacts = wallet.runtime.block_on(wallet.contacts_service.get_contacts()).unwrap();
    assert!(contacts.len() > 0);

    let balance = wallet
        .runtime
        .block_on(wallet.output_manager_service.get_balance())
        .unwrap();
    assert!(balance.available_balance > MicroTari::from(0));

    // TODO Put this back when the new comms goes in and we use the new Event bus
    //    let outbound_tx = wallet
    //        .runtime
    //        .block_on(wallet.transaction_service.get_pending_outbound_transactions())
    //        .unwrap();
    //    assert!(outbound_tx.len() > 0);

    let completed_tx = wallet
        .runtime
        .block_on(wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert!(completed_tx.len() > 0);

    wallet.shutdown();
}
