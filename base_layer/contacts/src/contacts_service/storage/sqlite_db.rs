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

use std::{convert::TryFrom, sync::Arc};

use diesel::result::Error as DieselError;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tari_common_sqlite::{error::SqliteStorageError, sqlite_connection_pool::PooledDbConnection};
use tari_common_types::tari_address::TariAddress;
use tari_utilities::ByteArray;

use crate::contacts_service::{
    error::ContactsServiceStorageError,
    storage::{
        database::{ContactsBackend, DbKey, DbKeyValuePair, DbValue, WriteOperation},
        types::{
            contacts::{ContactSql, UpdateContact},
            messages::{MessageUpdate, MessagesSql, MessagesSqlInsert},
        },
    },
    types::{Contact, Message},
};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct ContactsServiceSqliteDatabase<TContactServiceDbConnection> {
    database_connection: Arc<TContactServiceDbConnection>,
}

impl<TContactServiceDbConnection: PooledDbConnection<Error = SqliteStorageError>>
    ContactsServiceSqliteDatabase<TContactServiceDbConnection>
{
    pub fn new(database_connection: TContactServiceDbConnection) -> Self {
        Self {
            database_connection: Arc::new(database_connection),
        }
    }

    pub fn init(database_connection: TContactServiceDbConnection) -> Self {
        let db = Self::new(database_connection);
        db.run_migrations().expect("Migrations to run");
        db
    }

    fn run_migrations(&self) -> Result<Vec<String>, SqliteStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;
        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {}", b);
                        // std::io::stdout()
                        //     .write_all(m.as_ref())
                        //     .expect("Couldn't write migration number to stdout");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .map_err(|e| SqliteStorageError::DieselR2d2Error(e.to_string()))
    }
}

impl<TContactServiceDbConnection> ContactsBackend for ContactsServiceSqliteDatabase<TContactServiceDbConnection>
where TContactServiceDbConnection: PooledDbConnection<Error = SqliteStorageError>
{
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, ContactsServiceStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;

        let result = match key {
            DbKey::Contact(address) => match ContactSql::find_by_address(&address.to_bytes(), &mut conn) {
                Ok(c) => Some(DbValue::Contact(Box::new(Contact::try_from(c)?))),
                Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::ContactId(id) => match ContactSql::find_by_node_id(&id.to_vec(), &mut conn) {
                Ok(c) => Some(DbValue::Contact(Box::new(Contact::try_from(c)?))),
                Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::Contacts => Some(DbValue::Contacts(
                ContactSql::index(&mut conn)?
                    .iter()
                    .map(|c| Contact::try_from(c.clone()))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            DbKey::Messages(address, limit, page) => {
                match MessagesSql::find_by_address(&address.to_bytes(), *limit, *page, &mut conn) {
                    Ok(messages) => Some(DbValue::Messages(
                        messages
                            .iter()
                            .map(|m| Message::try_from(m.clone()).expect("Couldn't cast MessageSql to Message"))
                            .collect::<Vec<Message>>(),
                    )),
                    Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => None,
                    Err(e) => return Err(e),
                }
            },
            DbKey::Message(id) => match MessagesSql::find_by_message_id(&id.to_vec(), &mut conn) {
                Ok(c) => Some(DbValue::Message(Box::new(Message::try_from(c)?))),
                Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, ContactsServiceStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;

        match op {
            WriteOperation::Upsert(kvp) => match *kvp {
                DbKeyValuePair::MessageConfirmations(k, d, r) => {
                    if MessagesSql::find_by_message_id_and_update(&mut conn, &k, MessageUpdate {
                        delivery_confirmation_at: d,
                        read_confirmation_at: r,
                    })
                    .is_err()
                    {
                        MessagesSql::find_by_message_id(&k, &mut conn)?;
                    }
                },
                DbKeyValuePair::Contact(k, c) => {
                    if ContactSql::find_by_address_and_update(&mut conn, &k.to_bytes(), UpdateContact {
                        alias: Some(c.clone().alias),
                        last_seen: None,
                        latency: None,
                        favourite: Some(i32::from(c.favourite)),
                    })
                    .is_err()
                    {
                        ContactSql::from(c).commit(&mut conn)?;
                    }
                },
                DbKeyValuePair::LastSeen(..) => return Err(ContactsServiceStorageError::OperationNotSupported),
            },
            WriteOperation::UpdateLastSeen(kvp) => match *kvp {
                DbKeyValuePair::LastSeen(node_id, date_time, latency) => {
                    let contact =
                        ContactSql::find_by_node_id_and_update(&mut conn, &node_id.to_vec(), UpdateContact {
                            alias: None,
                            last_seen: Some(Some(date_time)),
                            latency: Some(latency),
                            favourite: None,
                        })?;
                    return Ok(Some(DbValue::TariAddress(Box::new(
                        TariAddress::from_bytes(&contact.address)
                            .map_err(|_| ContactsServiceStorageError::ConversionError)?,
                    ))));
                },
                DbKeyValuePair::Contact(..) => return Err(ContactsServiceStorageError::OperationNotSupported),
                DbKeyValuePair::MessageConfirmations(..) => {
                    return Err(ContactsServiceStorageError::OperationNotSupported)
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::Contact(k) => match ContactSql::find_by_address_and_delete(&mut conn, &k.to_bytes()) {
                    Ok(c) => {
                        return Ok(Some(DbValue::Contact(Box::new(Contact::try_from(c)?))));
                    },
                    Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                },
                DbKey::ContactId(id) => match ContactSql::find_by_node_id_and_delete(&mut conn, &id.to_vec()) {
                    Ok(c) => {
                        return Ok(Some(DbValue::Contact(Box::new(Contact::try_from(c)?))));
                    },
                    Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => (),
                    Err(e) => return Err(e),
                },
                DbKey::Contacts => return Err(ContactsServiceStorageError::OperationNotSupported),
                DbKey::Messages(_pk, _l, _p) => return Err(ContactsServiceStorageError::OperationNotSupported),
                DbKey::Message(_id) => return Err(ContactsServiceStorageError::OperationNotSupported),
            },
            WriteOperation::Insert(i) => {
                if let DbValue::Message(m) = *i {
                    MessagesSqlInsert::try_from(*m)?.commit(&mut conn)?;
                }
            },
        }

        Ok(None)
    }
}

#[cfg(test)]
mod test {
    use std::convert::{TryFrom, TryInto};

    use rand::rngs::OsRng;
    use tari_common::configuration::Network;
    use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
    use tari_common_types::{
        tari_address::TariAddress,
        types::{PrivateKey, PublicKey},
    };
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
    use tari_test_utils::{paths::with_temp_dir, random::string};

    use super::*;
    use crate::contacts_service::{
        storage::types::contacts::{ContactSql, UpdateContact},
        types::Contact,
    };

    #[test]
    fn test_crud() {
        with_temp_dir(|dir_path| {
            let db_name = format!("{}.sqlite3", string(8).as_str());
            let db_path = format!("{}/{}", dir_path.to_str().unwrap(), db_name);
            let url: DbConnectionUrl = db_path.try_into().unwrap();

            let db = DbConnection::connect_url(&url).unwrap();
            let _service = ContactsServiceSqliteDatabase::init(db.clone());
            let mut conn = db.get_pooled_connection().unwrap();

            let names = ["Alice".to_string(), "Bob".to_string(), "Carol".to_string()];

            let mut contacts = Vec::new();
            for i in 0..names.len() {
                let pub_key = PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng));
                let address = TariAddress::new(pub_key, Network::default());
                contacts.push(Contact::new(names[i].clone(), address, None, None, false));
                ContactSql::from(contacts[i].clone()).commit(&mut conn).unwrap();
            }

            let retrieved_contacts = ContactSql::index(&mut conn).unwrap();

            for i in &contacts {
                assert!(retrieved_contacts.iter().any(|v| v == &ContactSql::from(i.clone())));
            }

            assert_eq!(
                contacts[1],
                Contact::try_from(ContactSql::find_by_address(&contacts[1].address.to_bytes(), &mut conn).unwrap())
                    .unwrap()
            );

            ContactSql::find_by_address_and_delete(&mut conn, &contacts[0].address.clone().to_bytes()).unwrap();

            let retrieved_contacts = ContactSql::index(&mut conn).unwrap();
            assert_eq!(retrieved_contacts.len(), 2);

            assert!(!retrieved_contacts
                .iter()
                .any(|v| v == &ContactSql::from(contacts[0].clone())));

            let _c =
                ContactSql::find_by_address_and_update(&mut conn, &contacts[1].address.to_bytes(), UpdateContact {
                    alias: Some("Fred".to_string()),
                    last_seen: None,
                    latency: None,
                    favourite: Some(i32::from(true)),
                })
                .unwrap();

            let c_updated = ContactSql::find_by_address(&contacts[1].address.to_bytes(), &mut conn).unwrap();
            assert_eq!(c_updated.alias, "Fred".to_string());
            assert_eq!(c_updated.favourite, i32::from(true));
        });
    }
}
