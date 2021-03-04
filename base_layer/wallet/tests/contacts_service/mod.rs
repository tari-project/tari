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

use crate::support::utils::random_string;
use rand::rngs::OsRng;
use tari_core::transactions::types::PublicKey;
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_wallet::{
    contacts_service::{
        error::{ContactsServiceError, ContactsServiceStorageError},
        handle::ContactsServiceHandle,
        storage::{
            database::{Contact, ContactsBackend, ContactsDatabase, DbKey},
            memory_db::ContactsServiceMemoryDatabase,
            sqlite_db::ContactsServiceSqliteDatabase,
        },
        ContactsServiceInitializer,
    },
    storage::sqlite_utilities::run_migration_and_create_sqlite_connection,
};
use tempfile::tempdir;
use tokio::runtime::Runtime;

pub fn setup_contacts_service<T: ContactsBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (ContactsServiceHandle, Shutdown)
{
    let shutdown = Shutdown::new();
    let fut = StackBuilder::new(shutdown.to_signal())
        .add_initializer(ContactsServiceInitializer::new(backend))
        .build();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let contacts_api = handles.expect_handle::<ContactsServiceHandle>();

    (contacts_api, shutdown)
}

#[test]
pub fn test_memory_database_crud() {
    let mut runtime = Runtime::new().unwrap();

    let db = ContactsDatabase::new(ContactsServiceMemoryDatabase::new());
    let mut contacts = Vec::new();
    for i in 0..5 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: random_string(8),
            public_key,
        });

        runtime.block_on(db.upsert_contact(contacts[i].clone())).unwrap();
    }

    let got_contacts = runtime.block_on(db.get_contacts()).unwrap();
    assert_eq!(contacts, got_contacts);

    let contact = runtime
        .block_on(db.get_contact(contacts[0].public_key.clone()))
        .unwrap();
    assert_eq!(contact, contacts[0]);

    let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

    let contact = runtime.block_on(db.get_contact(public_key.clone()));
    assert_eq!(
        contact,
        Err(ContactsServiceStorageError::ValueNotFound(DbKey::Contact(
            public_key.clone()
        )))
    );
    assert_eq!(
        runtime.block_on(db.remove_contact(public_key.clone())),
        Err(ContactsServiceStorageError::ValueNotFound(DbKey::Contact(public_key)))
    );

    let _ = runtime
        .block_on(db.remove_contact(contacts[0].public_key.clone()))
        .unwrap();
    contacts.remove(0);
    let got_contacts = runtime.block_on(db.get_contacts()).unwrap();

    assert_eq!(contacts, got_contacts);
}

pub fn test_contacts_service<T: ContactsBackend + 'static>(backend: T) {
    let mut runtime = Runtime::new().unwrap();
    let (mut contacts_service, _shutdown) = setup_contacts_service(&mut runtime, backend);

    let mut contacts = Vec::new();
    for i in 0..5 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: random_string(8),
            public_key,
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

#[test]
fn contacts_service_memory_db() {
    test_contacts_service(ContactsServiceMemoryDatabase::new());
}

#[test]
fn contacts_service_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = tempdir().unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    let db_path = format!("{}/{}", db_folder, db_name);
    let connection = run_migration_and_create_sqlite_connection(&db_path).unwrap();
    test_contacts_service(ContactsServiceSqliteDatabase::new(connection));
}
