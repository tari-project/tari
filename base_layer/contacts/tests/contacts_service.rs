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

use std::{convert::TryInto, sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_common::configuration::{MultiaddrList, Network, StringList};
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_common_types::{tari_address::TariAddress, types::PublicKey};
use tari_comms::{peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::{store_forward::SafConfig, DhtConfig};
use tari_contacts::contacts_service::{
    error::{ContactsServiceError, ContactsServiceStorageError},
    handle::{ContactsServiceHandle, DEFAULT_MESSAGE_LIMIT, MAX_MESSAGE_LIMIT},
    storage::{
        database::{ContactsBackend, ContactsDatabase, DbKey},
        sqlite_db::ContactsServiceSqliteDatabase,
    },
    types::{Contact, MessageBuilder},
    ContactsServiceInitializer,
};
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::P2pInitializer,
    services::liveness::{LivenessConfig, LivenessInitializer},
    transport::MemoryTransportConfig,
    P2pConfig,
    PeerSeedsConfig,
    TransportConfig,
    TransportType,
};
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_test_utils::{comms_and_services::get_next_memory_address, paths::with_temp_dir, random, random::string};
use tempfile::tempdir;
use tokio::{runtime::Runtime, sync::broadcast::error::TryRecvError};

pub fn setup_contacts_service<T: ContactsBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (ContactsServiceHandle, Arc<NodeIdentity>, Shutdown) {
    let _enter = runtime.enter();
    let (publisher, subscription_factory) = pubsub_connector(100);
    let node_identity = Arc::new(NodeIdentity::random(
        &mut OsRng,
        get_next_memory_address(),
        PeerFeatures::COMMUNICATION_NODE,
    ));
    let comms_config = P2pConfig {
        override_from: None,
        public_addresses: MultiaddrList::default(),
        transport: TransportConfig {
            transport_type: TransportType::Memory,
            memory: MemoryTransportConfig {
                listener_address: node_identity.first_public_address().unwrap(),
            },
            ..Default::default()
        },
        auxiliary_tcp_listener_address: None,
        datastore_path: tempdir().unwrap().into_path(),
        peer_database_name: random::string(8),
        max_concurrent_inbound_tasks: 10,
        max_concurrent_outbound_tasks: 10,
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
        rpc_max_simultaneous_sessions: 0,
        rpc_max_sessions_per_peer: 0,
        listener_self_liveness_check_interval: None,
    };
    let peer_message_subscription_factory = Arc::new(subscription_factory);
    let shutdown = Shutdown::new();
    let user_agent = format!("tari/tests/{}", env!("CARGO_PKG_VERSION"));
    let fut = StackBuilder::new(shutdown.to_signal())
        .add_initializer(P2pInitializer::new(
            comms_config,
            user_agent,
            PeerSeedsConfig::default(),
            Network::LocalNet,
            node_identity.clone(),
            publisher,
        ))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_secs(1)),
                num_peers_per_round: 0,       // No random peers
                max_allowed_ping_failures: 0, // Peer with failed ping-pong will never be removed
                ..Default::default()
            },
            peer_message_subscription_factory.clone(),
        ))
        .add_initializer(ContactsServiceInitializer::new(
            backend,
            peer_message_subscription_factory,
            Duration::from_secs(5),
            2,
        ))
        .build();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let contacts_api = handles.expect_handle::<ContactsServiceHandle>();

    (contacts_api, node_identity, shutdown)
}

#[test]
pub fn test_contacts_service() {
    with_temp_dir(|dir_path| {
        let mut runtime = Runtime::new().unwrap();

        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_path = format!("{}/{}", dir_path.to_str().unwrap(), db_name);
        let url: DbConnectionUrl = db_path.try_into().unwrap();

        let db = DbConnection::connect_url(&url).unwrap();
        let backend = ContactsServiceSqliteDatabase::init(db);

        let (mut contacts_service, _node_identity, _shutdown) = setup_contacts_service(&mut runtime, backend);
        let mut liveness_event_stream = contacts_service.get_contacts_liveness_event_stream();

        let mut contacts = Vec::new();
        for i in 0..5 {
            let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);
            let address = TariAddress::new_single_address_with_default_features(public_key, Network::default());

            contacts.push(Contact::new(random::string(8), address, None, None, false));

            runtime
                .block_on(contacts_service.upsert_contact(contacts[i].clone()))
                .unwrap();
        }

        let got_contacts = runtime.block_on(contacts_service.get_contacts()).unwrap();
        assert_eq!(contacts, got_contacts);

        let contact = runtime
            .block_on(contacts_service.get_contact(contacts[0].address.clone()))
            .unwrap();
        assert_eq!(contact, contacts[0]);

        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);
        let address = TariAddress::new_single_address_with_default_features(public_key, Network::default());

        let contact = runtime.block_on(contacts_service.get_contact(address.clone()));
        match contact {
            Ok(_) => panic!("There should be an error here"),
            Err(ContactsServiceError::ContactsServiceStorageError(ContactsServiceStorageError::ValueNotFound(val))) => {
                assert_eq!(val, DbKey::Contact(address.clone()))
            },
            _ => panic!("There should be a specific error here"),
        }
        let result = runtime.block_on(contacts_service.remove_contact(address.clone()));
        match result {
            Ok(_) => panic!("There should be an error here"),
            Err(ContactsServiceError::ContactsServiceStorageError(ContactsServiceStorageError::ValueNotFound(val))) => {
                assert_eq!(val, DbKey::Contact(address))
            },
            _ => panic!("There should be a specific error here"),
        }

        let _contact = runtime
            .block_on(contacts_service.remove_contact(contacts[0].address.clone()))
            .unwrap();
        contacts.remove(0);
        let got_contacts = runtime.block_on(contacts_service.get_contacts()).unwrap();

        assert_eq!(contacts, got_contacts);

        let mut updated_contact = contacts[1].clone();
        updated_contact.alias = "Fred".to_string();
        updated_contact.favourite = true;

        runtime
            .block_on(contacts_service.upsert_contact(updated_contact.clone()))
            .unwrap();
        let new_contact = runtime
            .block_on(contacts_service.get_contact(updated_contact.address))
            .unwrap();

        assert_eq!(new_contact.alias, updated_contact.alias);

        #[allow(clippy::match_wild_err_arm)]
        match liveness_event_stream.try_recv() {
            Ok(_) => panic!("Should not receive any event here"),
            Err(TryRecvError::Empty) => {},
            Err(_) => panic!("Should not receive any other type of error here"),
        };
    });
}

#[test]
pub fn test_message_pagination() {
    with_temp_dir(|dir_path| {
        let mut runtime = Runtime::new().unwrap();

        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_path = format!("{}/{}", dir_path.to_str().unwrap(), db_name);
        let url: DbConnectionUrl = db_path.try_into().unwrap();

        let db = DbConnection::connect_url(&url).unwrap();
        let backend = ContactsServiceSqliteDatabase::init(db);
        let contacts_db = ContactsDatabase::new(backend.clone());

        let (mut contacts_service, _node_identity, _shutdown) = setup_contacts_service(&mut runtime, backend);

        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);
        let address = TariAddress::new_single_address_with_default_features(public_key, Network::default());

        let contact = Contact::new(random::string(8), address.clone(), None, None, false);
        runtime.block_on(contacts_service.upsert_contact(contact)).unwrap();

        // Test lower bounds
        for num in 0..8 {
            let message = MessageBuilder::new()
                .message(format!("Test {:?}", num))
                .receiver_address(address.clone())
                .sender_address(address.clone())
                .build();

            contacts_db.save_message(message.clone()).expect("Message to be saved");
        }

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), 5, 0))
            .unwrap();
        assert_eq!(5, messages.len());

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), 5, 1))
            .unwrap();
        assert_eq!(3, messages.len());

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), 0, 0))
            .unwrap();
        assert_eq!(8, messages.len());

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), 0, 1))
            .unwrap();
        assert_eq!(0, messages.len());

        // Test upper bounds
        for num in 0..3000 {
            let message = MessageBuilder::new()
                .message(format!("Test {:?}", num))
                .receiver_address(address.clone())
                .sender_address(address.clone())
                .build();

            contacts_db.save_message(message.clone()).expect("Message to be saved");
        }

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), u64::MAX, 0))
            .unwrap();
        assert_eq!(DEFAULT_MESSAGE_LIMIT, messages.len() as u64);

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), MAX_MESSAGE_LIMIT, 0))
            .unwrap();
        assert_eq!(2500, messages.len());

        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), MAX_MESSAGE_LIMIT, 1))
            .unwrap();
        assert_eq!(508, messages.len());

        // Would cause overflows, defaults to page = 0
        let messages = runtime
            .block_on(contacts_service.get_messages(address.clone(), MAX_MESSAGE_LIMIT, u64::MAX))
            .unwrap();
        assert_eq!(2500, messages.len());

        let messages = runtime
            .block_on(contacts_service.get_messages(address, 1, i64::MAX as u64))
            .unwrap();
        assert_eq!(0, messages.len());
    });
}
