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

use crate::support::data::get_temp_sqlite_database_connection;
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
use tokio::runtime::Runtime;

pub fn setup_contacts_service<T: ContactsBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (ContactsServiceHandle, Shutdown) {
    let shutdown = Shutdown::new();
    let fut = StackBuilder::new(shutdown.to_signal())
        .add_initializer(ContactsServiceInitializer::new(backend))
        .build();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let contacts_api = handles.expect_handle::<ContactsServiceHandle>();

    (contacts_api, shutdown)
}

#[test]
pub fn test_contacts_service() {
    let mut runtime = Runtime::new().unwrap();
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let backend = ContactsServiceSqliteDatabase::new(connection);

    let (mut contacts_service, _shutdown) = setup_contacts_service(&mut runtime, backend);

    let mut contacts = Vec::new();
    for i in 0..5 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: random::string(8),
            public_key: public_key.compress(),
        });

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
    let public_key = public_key.compress();
    let contact = runtime.block_on(contacts_service.get_contact(public_key.clone()));
    assert_eq!(
        contact,
        Err(ContactsServiceError::ContactsServiceStorageError(
            ContactsServiceStorageError::ValueNotFound(DbKey::Contact(public_key.clone()))
        ))
    );
    assert_eq!(
        runtime.block_on(contacts_service.remove_contact(public_key.clone())),
        Err(ContactsServiceError::ContactsServiceStorageError(
            ContactsServiceStorageError::ValueNotFound(DbKey::Contact(public_key))
        ))
    );

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
}
