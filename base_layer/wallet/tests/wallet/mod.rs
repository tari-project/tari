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
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tari_comms::{
    control_service::ControlServiceConfig,
    multiaddr::Multiaddr,
    peer_manager::{peer::PeerFlags, NodeId, NodeIdentity, Peer, PeerFeatures},
    types::CommsPublicKey,
};
#[cfg(feature = "test_harness")]
use tari_comms_dht::DhtConfig;
use tari_crypto::keys::PublicKey;
use tari_p2p::initialization::CommsConfig;
use tari_test_utils::{collect_stream, paths::with_temp_dir};
use tari_transactions::{tari_amount::MicroTari, types::CryptoFactories};
#[cfg(feature = "test_harness")]
use tari_wallet::testnet_utils::broadcast_transaction;
#[cfg(feature = "test_harness")]
use tari_wallet::testnet_utils::finalize_received_transaction;
#[cfg(feature = "test_harness")]
use tari_wallet::transaction_service::storage::database::{CompletedTransaction, InboundTransaction};
use tari_wallet::{
    contacts_service::storage::{database::Contact, memory_db::ContactsServiceMemoryDatabase},
    output_manager_service::storage::memory_db::OutputManagerMemoryDatabase,
    storage::memory_db::WalletMemoryDatabase,
    transaction_service::{handle::TransactionEvent, storage::memory_db::TransactionMemoryDatabase},
    wallet::WalletConfig,
    Wallet,
};
#[cfg(feature = "test_harness")]
use tempdir::TempDir;
use tokio::runtime::Runtime;

fn create_peer(public_key: CommsPublicKey, net_address: Multiaddr) -> Peer {
    Peer::new(
        public_key.clone(),
        NodeId::from_key(&public_key).unwrap(),
        net_address.into(),
        PeerFlags::empty(),
        PeerFeatures::COMMUNICATION_NODE,
    )
}

#[test]
fn test_wallet() {
    with_temp_dir(|dir_path| {
        let runtime = Runtime::new().unwrap();

        let mut rng = rand::OsRng::new().unwrap();
        let factories = CryptoFactories::default();

        let alice_identity = NodeIdentity::random(
            &mut rng,
            "/ip4/127.0.0.1/tcp/22523".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap();
        let bob_identity = NodeIdentity::random(
            &mut rng,
            "/ip4/127.0.0.1/tcp/22145".parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap();
        let comms_config1 = CommsConfig {
            node_identity: Arc::new(alice_identity.clone()),
            peer_connection_listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            socks_proxy_address: None,
            control_service: ControlServiceConfig {
                listening_address: alice_identity.control_service_address(),
                socks_proxy_address: None,
                public_peer_address: None,
                requested_connection_timeout: Duration::from_millis(2000),
            },
            datastore_path: dir_path.to_str().unwrap().to_string(),
            establish_connection_timeout: Duration::from_secs(10),
            peer_database_name: random_string(8),
            inbound_buffer_size: 100,
            outbound_buffer_size: 100,
            dht: Default::default(),
        };
        let comms_config2 = CommsConfig {
            node_identity: Arc::new(bob_identity.clone()),
            peer_connection_listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            socks_proxy_address: None,
            control_service: ControlServiceConfig {
                listening_address: bob_identity.control_service_address(),
                socks_proxy_address: None,
                public_peer_address: None,
                requested_connection_timeout: Duration::from_millis(2000),
            },
            datastore_path: dir_path.to_str().unwrap().to_string(),
            establish_connection_timeout: Duration::from_secs(10),
            peer_database_name: random_string(8),
            inbound_buffer_size: 100,
            outbound_buffer_size: 100,
            dht: Default::default(),
        };
        let config1 = WalletConfig {
            comms_config: comms_config1,
            logging_path: None,
            factories: factories.clone(),
        };
        let config2 = WalletConfig {
            comms_config: comms_config2,
            logging_path: None,
            factories: factories.clone(),
        };
        let runtime_node1 = Runtime::new().unwrap();
        let runtime_node2 = Runtime::new().unwrap();
        let mut alice_wallet = Wallet::new(
            config1,
            runtime_node1,
            WalletMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            OutputManagerMemoryDatabase::new(),
            ContactsServiceMemoryDatabase::new(),
        )
        .unwrap();
        let bob_wallet = Wallet::new(
            config2,
            runtime_node2,
            WalletMemoryDatabase::new(),
            TransactionMemoryDatabase::new(),
            OutputManagerMemoryDatabase::new(),
            ContactsServiceMemoryDatabase::new(),
        )
        .unwrap();

        alice_wallet
            .comms
            .peer_manager()
            .add_peer(create_peer(
                bob_identity.public_key().clone(),
                bob_identity.control_service_address(),
            ))
            .unwrap();

        bob_wallet
            .comms
            .peer_manager()
            .add_peer(create_peer(
                alice_identity.public_key().clone(),
                alice_identity.control_service_address(),
            ))
            .unwrap();

        let alice_event_stream = alice_wallet.transaction_service.get_event_stream_fused();

        let value = MicroTari::from(1000);
        let (_utxo, uo1) = make_input(&mut rng, MicroTari(2500), &factories.commitment);

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

        assert_eq!(
            collect_stream!(
                runtime,
                alice_event_stream.map(|i| (*i).clone()),
                take = 1,
                timeout = Duration::from_secs(10)
            )
            .iter()
            .fold(0, |acc, x| match x {
                TransactionEvent::ReceivedTransactionReply(_) => acc + 1,
                _ => acc,
            }),
            1
        );

        let mut contacts = Vec::new();
        for i in 0..2 {
            let (_secret_key, public_key) = PublicKey::random_keypair(&mut rng);

            contacts.push(Contact {
                alias: random_string(8),
                public_key,
            });

            runtime
                .block_on(alice_wallet.contacts_service.save_contact(contacts[i].clone()))
                .unwrap();
        }

        let got_contacts = runtime.block_on(alice_wallet.contacts_service.get_contacts()).unwrap();
        assert_eq!(contacts, got_contacts);

        runtime.shutdown_on_idle();
    });
}

#[cfg(feature = "test_harness")]
#[test]
fn test_data_generation() {
    use tari_wallet::testnet_utils::generate_wallet_test_data;
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let mut rng = rand::OsRng::new().unwrap();

    let node_id = NodeIdentity::random(
        &mut rng,
        "/ip4/127.0.0.1/tcp/22712".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let comms_config = CommsConfig {
        node_identity: Arc::new(node_id.clone()),
        peer_connection_listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listening_address: node_id.control_service_address(),
            socks_proxy_address: None,
            public_peer_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        establish_connection_timeout: Duration::from_secs(10),
        datastore_path: temp_dir.path().to_str().unwrap().to_string(),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_millis(500),
            ..Default::default()
        },
    };

    let config = WalletConfig {
        comms_config,
        factories,
        logging_path: None,
    };

    let mut wallet = Wallet::new(
        config,
        runtime,
        WalletMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
    )
    .unwrap();

    generate_wallet_test_data(&mut wallet, temp_dir.path().to_str().unwrap()).unwrap();

    let contacts = wallet.runtime.block_on(wallet.contacts_service.get_contacts()).unwrap();
    assert!(contacts.len() > 0);

    let balance = wallet
        .runtime
        .block_on(wallet.output_manager_service.get_balance())
        .unwrap();
    assert!(balance.available_balance > MicroTari::from(0));

    let outbound_tx = wallet
        .runtime
        .block_on(wallet.transaction_service.get_pending_outbound_transactions())
        .unwrap();
    assert!(outbound_tx.len() > 0);

    let completed_tx = wallet
        .runtime
        .block_on(wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert!(completed_tx.len() > 0);

    wallet.shutdown().unwrap();
}

#[derive(Debug)]
struct CallbackState {
    pub received_tx_callback_called: bool,
    pub received_tx_reply_callback_called: bool,
    pub received_finalized_tx_callback_called: bool,
    pub broadcast_tx_callback_called: bool,
    pub mined_tx_callback_called: bool,
    pub discovery_send_callback_called: bool,
}

impl CallbackState {
    fn new() -> Self {
        Self {
            received_tx_callback_called: false,
            received_tx_reply_callback_called: false,
            received_finalized_tx_callback_called: false,
            broadcast_tx_callback_called: false,
            mined_tx_callback_called: false,
            discovery_send_callback_called: false,
        }
    }

    fn reset(&mut self) {
        self.received_tx_callback_called = false;
        self.received_tx_reply_callback_called = false;
        self.received_finalized_tx_callback_called = false;
        self.broadcast_tx_callback_called = false;
        self.mined_tx_callback_called = false;
        self.discovery_send_callback_called = false;
    }
}
lazy_static! {
    static ref CALLBACK_STATE_HARNESS: Mutex<CallbackState> = {
        let c = Mutex::new(CallbackState::new());
        c
    };
}
#[cfg(feature = "test_harness")]
unsafe extern "C" fn received_tx_callback(_tx: *mut InboundTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE_HARNESS.lock().unwrap().received_tx_callback_called = true;
    Box::from_raw(_tx);
}
#[cfg(feature = "test_harness")]
unsafe extern "C" fn received_tx_reply_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE_HARNESS.lock().unwrap().received_tx_reply_callback_called = true;
    Box::from_raw(_tx);
}
#[cfg(feature = "test_harness")]
unsafe extern "C" fn received_finalized_tx_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE_HARNESS
        .lock()
        .unwrap()
        .received_finalized_tx_callback_called = true;
    Box::from_raw(_tx);
}
#[cfg(feature = "test_harness")]
unsafe extern "C" fn broadcast_tx_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE_HARNESS.lock().unwrap().broadcast_tx_callback_called = true;
    Box::from_raw(_tx);
}
#[cfg(feature = "test_harness")]
unsafe extern "C" fn mined_tx_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE_HARNESS.lock().unwrap().mined_tx_callback_called = true;
    Box::from_raw(_tx);
}
#[cfg(feature = "test_harness")]
unsafe extern "C" fn discovery_send_callback(_tx_id: u64, _result: bool) {
    CALLBACK_STATE_HARNESS.lock().unwrap().discovery_send_callback_called = true;
    assert!(true);
}

#[cfg(feature = "test_harness")]
#[test]
fn test_test_harness() {
    CALLBACK_STATE_HARNESS.lock().unwrap().reset();

    use rand::OsRng;
    use std::thread;
    use tari_wallet::{
        testnet_utils::{complete_sent_transaction, mine_transaction, receive_test_transaction},
        transaction_service::storage::database::TransactionStatus,
    };

    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();
    // Alice's parameters
    let alice_identity = NodeIdentity::random(
        &mut rng,
        "/ip4/127.0.0.1/tcp/21525".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let bob_identity = NodeIdentity::random(
        &mut rng,
        "/ip4/127.0.0.1/tcp/21144".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let alice_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let comms_config1 = CommsConfig {
        node_identity: Arc::new(alice_identity.clone()),
        peer_connection_listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listening_address: alice_identity.control_service_address(),
            socks_proxy_address: None,
            public_peer_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: alice_dir.path().to_str().unwrap().to_string(),
        establish_connection_timeout: Duration::from_secs(10),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_millis(500),
            ..Default::default()
        },
    };
    let config1 = WalletConfig {
        comms_config: comms_config1,
        factories: factories.clone(),
        logging_path: None,
    };

    let runtime = Runtime::new().unwrap();
    let tx_backend = TransactionMemoryDatabase::new();
    let mut alice_wallet = Wallet::new(
        config1,
        runtime,
        WalletMemoryDatabase::new(),
        tx_backend.clone(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
    )
    .unwrap();

    alice_wallet.set_callbacks(
        tx_backend,
        received_tx_callback,
        received_tx_reply_callback,
        received_finalized_tx_callback,
        broadcast_tx_callback,
        mined_tx_callback,
        discovery_send_callback,
    );

    alice_wallet
        .comms
        .peer_manager()
        .add_peer(create_peer(
            bob_identity.public_key().clone(),
            bob_identity.control_service_address(),
        ))
        .unwrap();

    alice_wallet
        .comms
        .peer_manager()
        .add_peer(create_peer(
            bob_identity.public_key().clone(),
            bob_identity.control_service_address(),
        ))
        .unwrap();

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut rng, MicroTari(2500), &factories.commitment);

    alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.add_output(uo1))
        .unwrap();

    alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.send_transaction(
            bob_identity.public_key().clone(),
            value,
            MicroTari::from(20),
            "".to_string(),
        ))
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    let alice_pending_outbound = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_pending_outbound_transactions())
        .unwrap();
    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert_eq!(alice_pending_outbound.len(), 1);
    assert_eq!(alice_completed_tx.len(), 0);

    let mut tx_id = 0u64;
    for k in alice_pending_outbound.keys() {
        tx_id = k.clone();
    }

    complete_sent_transaction(&mut alice_wallet, tx_id.clone()).unwrap();

    let alice_pending_outbound = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_pending_outbound_transactions())
        .unwrap();
    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 1);
    for (_k, v) in alice_completed_tx.clone().drain().take(1) {
        assert_eq!(v.status, TransactionStatus::Completed);
    }

    broadcast_transaction(&mut alice_wallet, tx_id.clone()).unwrap();

    let alice_pending_outbound = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_pending_outbound_transactions())
        .unwrap();
    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 1);
    for (_k, v) in alice_completed_tx.clone().drain().take(1) {
        assert_eq!(v.status, TransactionStatus::Broadcast);
    }

    let pre_mined_balance = alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.get_balance())
        .unwrap();

    mine_transaction(&mut alice_wallet, tx_id.clone()).unwrap();

    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert_eq!(alice_completed_tx.len(), 1);
    for (_k, v) in alice_completed_tx.clone().drain().take(1) {
        assert_eq!(v.status, TransactionStatus::Mined);
    }

    let post_mined_balance = alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.get_balance())
        .unwrap();

    assert_eq!(
        pre_mined_balance.pending_incoming_balance,
        post_mined_balance.available_balance
    );

    receive_test_transaction(&mut alice_wallet).unwrap();

    let alice_pending_inbound = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_pending_inbound_transactions())
        .unwrap();

    assert_eq!(alice_pending_inbound.len(), 1);

    let mut inbound_tx_id = None;
    for (_k, v) in alice_pending_inbound.clone().drain().take(1) {
        inbound_tx_id = Some(v.tx_id);
    }
    assert!(inbound_tx_id.is_some());

    finalize_received_transaction(&mut alice_wallet, inbound_tx_id.clone().unwrap()).unwrap();

    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();

    assert_eq!(alice_completed_tx.len(), 2);
    let tx = alice_completed_tx.get(&inbound_tx_id.clone().take().unwrap()).unwrap();
    assert_eq!(tx.status, TransactionStatus::Completed);

    broadcast_transaction(&mut alice_wallet, inbound_tx_id.clone().unwrap()).unwrap();

    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();

    assert_eq!(alice_completed_tx.len(), 2);
    let tx = alice_completed_tx.get(&inbound_tx_id.clone().take().unwrap()).unwrap();
    assert_eq!(tx.status, TransactionStatus::Broadcast);

    let pre_mined_balance = alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.get_balance())
        .unwrap();

    mine_transaction(&mut alice_wallet, inbound_tx_id.clone().take().unwrap()).unwrap();

    let alice_completed_tx = alice_wallet
        .runtime
        .block_on(alice_wallet.transaction_service.get_completed_transactions())
        .unwrap();
    assert_eq!(alice_completed_tx.len(), 2);
    let tx = alice_completed_tx.get(&inbound_tx_id.clone().take().unwrap()).unwrap();
    assert_eq!(tx.status, TransactionStatus::Mined);

    let post_mined_balance = alice_wallet
        .runtime
        .block_on(alice_wallet.output_manager_service.get_balance())
        .unwrap();

    assert_eq!(
        pre_mined_balance.pending_incoming_balance + pre_mined_balance.available_balance,
        post_mined_balance.available_balance
    );

    let callback_state = CALLBACK_STATE_HARNESS.lock().unwrap();
    assert!(callback_state.received_tx_callback_called);
    assert!(callback_state.received_finalized_tx_callback_called);
    assert!(callback_state.broadcast_tx_callback_called);
    assert!(callback_state.mined_tx_callback_called);

    alice_wallet.shutdown().unwrap();
}
