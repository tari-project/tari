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
use tari_crypto::keys::PublicKey as PublicKeyTrait;
use tari_service_framework::StackBuilder;
use tari_shutdown::Shutdown;
use tari_transactions::types::PublicKey;
use tari_wallet::contacts_service::{
    error::{ContactsServiceError, ContactsServiceStorageError},
    handle::ContactsServiceHandle,
    storage::{
        database::{Contact, ContactsBackend, ContactsDatabase, DbKey},
        memory_db::ContactsServiceMemoryDatabase,
        sqlite_db::ContactsServiceSqliteDatabase,
    },
    ContactsServiceInitializer,
};
use tempdir::TempDir;
use tokio::runtime::Runtime;

pub fn setup_contacts_service<T: ContactsBackend + 'static>(
    runtime: &mut Runtime,
    backend: T,
) -> (ContactsServiceHandle, Shutdown)
{
    let shutdown = Shutdown::new();
    let fut = StackBuilder::new(runtime.handle().clone(), shutdown.to_signal())
        .add_initializer(ContactsServiceInitializer::new(backend))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let contacts_api = handles.get_handle::<ContactsServiceHandle>().unwrap();

    (contacts_api, shutdown)
}

#[test]
pub fn test_memory_database_crud() {
    let mut rng = rand::OsRng::new().unwrap();

    let mut db = ContactsDatabase::new(ContactsServiceMemoryDatabase::new());
    let mut contacts = Vec::new();
    for i in 0..5 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut rng);

        contacts.push(Contact {
            alias: random_string(8),
            public_key,
        });

        db.save_contact(contacts[i].clone()).unwrap();
        assert_eq!(
            db.save_contact(contacts[i].clone()),
            Err(ContactsServiceStorageError::DuplicateContact)
        );
    }

    let got_contacts = db.get_contacts().unwrap();
    assert_eq!(contacts, got_contacts);

    let contact = db.get_contact(&contacts[0].public_key).unwrap();
    assert_eq!(contact, contacts[0]);

    let (_secret_key, public_key) = PublicKey::random_keypair(&mut rng);

    let contact = db.get_contact(&public_key);
    assert_eq!(
        contact,
        Err(ContactsServiceStorageError::ValueNotFound(DbKey::Contact(
            public_key.clone()
        )))
    );
    assert_eq!(
        db.remove_contact(&public_key),
        Err(ContactsServiceStorageError::ValueNotFound(DbKey::Contact(
            public_key.clone()
        )))
    );

    let _ = db.remove_contact(&contacts[0].public_key).unwrap();
    contacts.remove(0);
    let got_contacts = db.get_contacts().unwrap();

    assert_eq!(contacts, got_contacts);
}

pub fn test_contacts_service<T: ContactsBackend + 'static>(backend: T) {
    let mut rng = rand::OsRng::new().unwrap();

    let mut runtime = Runtime::new().unwrap();
    let (mut contacts_service, _shutdown) = setup_contacts_service(&mut runtime, backend);

    let mut contacts = Vec::new();
    for i in 0..5 {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut rng);

        contacts.push(Contact {
            alias: random_string(8),
            public_key,
        });

        runtime
            .block_on(contacts_service.save_contact(contacts[i].clone()))
            .unwrap();
    }

    let got_contacts = runtime.block_on(contacts_service.get_contacts()).unwrap();
    assert_eq!(contacts, got_contacts);

    let contact = runtime
        .block_on(contacts_service.get_contact(contacts[0].public_key.clone()))
        .unwrap();
    assert_eq!(contact, contacts[0]);

    let (_secret_key, public_key) = PublicKey::random_keypair(&mut rng);

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
            ContactsServiceStorageError::ValueNotFound(DbKey::Contact(public_key.clone()))
        ))
    );

    let _ = runtime
        .block_on(contacts_service.remove_contact(contacts[0].public_key.clone()))
        .unwrap();
    contacts.remove(0);
    let got_contacts = runtime.block_on(contacts_service.get_contacts()).unwrap();

    assert_eq!(contacts, got_contacts);
}

#[test]
fn contacts_service_memory_db() {
    test_contacts_service(ContactsServiceMemoryDatabase::new());
}

#[test]
fn contacts_service_sqlite_db() {
    let db_name = format!("{}.sqlite3", random_string(8).as_str());
    let temp_dir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = temp_dir.path().to_str().unwrap().to_string();
    test_contacts_service(
        ContactsServiceSqliteDatabase::new(format!("{}/{}", db_folder, db_name).to_string()).unwrap(),
    );
}
