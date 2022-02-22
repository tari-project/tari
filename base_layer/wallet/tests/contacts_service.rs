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

use std::{sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_common_types::types::PublicKey;
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_test_utils::random;
use tari_wallet::contacts_service::{
    error::{ContactsServiceError, ContactsServiceStorageError},
    handle::ContactsServiceHandle,
    storage::{
        database::{Contact, ContactsBackend, DbKey},
        sqlite_db::ContactsServiceSqliteDatabase,
    },
    ContactsServiceInitializer,
};
use tempfile::tempdir;
use tokio::{runtime::Runtime, sync::broadcast::error::TryRecvError};
pub mod support;
use support::data::get_temp_sqlite_database_connection;
use tari_common::configuration::Network;
use tari_comms::{peer_manager::PeerFeatures, NodeIdentity};
use tari_comms_dht::{store_forward::SafConfig, DhtConfig};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{P2pConfig, P2pInitializer},
    services::liveness::{LivenessConfig, LivenessInitializer},
    transport::TransportType,
    DEFAULT_DNS_NAME_SERVER,
};

use crate::support::comms_and_services::get_next_memory_address;

pub fn setup_contacts_service<T: ContactsBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (ContactsServiceHandle, NodeIdentity, Shutdown) {
    let _enter = runtime.enter();
    let (publisher, subscription_factory) = pubsub_connector(100, 50);
    const NETWORK: Network = Network::Weatherwax;
    let node_identity = NodeIdentity::random(&mut OsRng, get_next_memory_address(), PeerFeatures::COMMUNICATION_NODE);
    let comms_config = P2pConfig {
        network: NETWORK,
        node_identity: Arc::new(node_identity.clone()),
        transport_type: TransportType::Memory {
            listener_address: node_identity.public_address(),
        },
        auxilary_tcp_listener_address: None,
        datastore_path: tempdir().unwrap().into_path(),
        peer_database_name: random::string(8),
        max_concurrent_inbound_tasks: 10,
        max_concurrent_outbound_tasks: 10,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_secs(1),
            auto_join: true,
            saf_config: SafConfig {
                auto_request: true,
                ..Default::default()
            },
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
    let peer_message_subscription_factory = Arc::new(subscription_factory);
    let shutdown = Shutdown::new();
    let fut = StackBuilder::new(shutdown.to_signal())
        .add_initializer(P2pInitializer::new(comms_config, publisher))
        .add_initializer(LivenessInitializer::new(
            LivenessConfig {
                auto_ping_interval: Some(Duration::from_secs(1)),
                num_peers_per_round: 0,       // No random peers
                max_allowed_ping_failures: 0, // Peer with failed ping-pong will never be removed
                ..Default::default()
            },
            peer_message_subscription_factory,
        ))
        .add_initializer(ContactsServiceInitializer::new(backend))
        .build();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let contacts_api = handles.expect_handle::<ContactsServiceHandle>();

    (contacts_api, node_identity, shutdown)
}

#[test]
pub fn test_contacts_service() {
    let mut runtime = Runtime::new().unwrap();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = ContactsServiceSqliteDatabase::new(connection);

    let (mut contacts_service, _node_identity, _shutdown) = setup_contacts_service(&mut runtime, backend);
    let mut liveness_event_stream = contacts_service.get_contacts_liveness_event_stream();

    let mut contacts = Vec::new();
    for i in 0..5 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact::new(random::string(8), public_key, None, None));

        runtime
            .block_on(contacts_service.upsert_contact(contacts[i].clone()))
            .unwrap();
    }

    let got_contacts = runtime.block_on(contacts_service.get_contacts()).unwrap();
    assert_eq!(contacts, got_contacts);

    let contact = runtime
        .block_on(contacts_service.get_contact(contacts[0].public_key.clone()))
        .unwrap();
    assert_eq!(contact, contacts[0]);

    let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

    let contact = runtime.block_on(contacts_service.get_contact(public_key.clone()));
    match contact {
        Ok(_) => panic!("There should be an error here"),
        Err(ContactsServiceError::ContactsServiceStorageError(ContactsServiceStorageError::ValueNotFound(val))) => {
            assert_eq!(val, DbKey::Contact(public_key.clone()))
        },
        _ => panic!("There should be a specific error here"),
    }
    let result = runtime.block_on(contacts_service.remove_contact(public_key.clone()));
    match result {
        Ok(_) => panic!("There should be an error here"),
        Err(ContactsServiceError::ContactsServiceStorageError(ContactsServiceStorageError::ValueNotFound(val))) => {
            assert_eq!(val, DbKey::Contact(public_key))
        },
        _ => panic!("There should be a specific error here"),
    }

    let _ = runtime
        .block_on(contacts_service.remove_contact(contacts[0].public_key.clone()))
        .unwrap();
    contacts.remove(0);
    let got_contacts = runtime.block_on(contacts_service.get_contacts()).unwrap();

    assert_eq!(contacts, got_contacts);

    let mut updated_contact = contacts[1].clone();
    updated_contact.alias = "Fred".to_string();

    runtime
        .block_on(contacts_service.upsert_contact(updated_contact.clone()))
        .unwrap();
    let new_contact = runtime
        .block_on(contacts_service.get_contact(updated_contact.public_key))
        .unwrap();

    assert_eq!(new_contact.alias, updated_contact.alias);

    match liveness_event_stream.try_recv() {
        Ok(_) => panic!("Should not receive any event here"),
        Err(TryRecvError::Empty) => {},
        Err(_) => panic!("Should not receive any other type of error here"),
    };
}
