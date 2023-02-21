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

use std::convert::TryFrom;

use chrono::NaiveDateTime;
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};
use tari_common_types::tari_address::TariAddress;
use tari_comms::peer_manager::NodeId;
use tari_utilities::ByteArray;

use crate::{
    contacts_service::{
        error::ContactsServiceStorageError,
        storage::database::{Contact, ContactsBackend, DbKey, DbKeyValuePair, DbValue, WriteOperation},
    },
    schema::contacts,
    storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
    util::diesel_ext::ExpectedRowsExtension,
};

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct ContactsServiceSqliteDatabase {
    database_connection: WalletDbConnection,
}
impl ContactsServiceSqliteDatabase {
    pub fn new(database_connection: WalletDbConnection) -> Self {
        Self { database_connection }
    }
}

impl ContactsBackend for ContactsServiceSqliteDatabase {
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
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, ContactsServiceStorageError> {
        let mut conn = self.database_connection.get_pooled_connection()?;

        match op {
            WriteOperation::Upsert(kvp) => match *kvp {
                DbKeyValuePair::Contact(k, c) => {
                    if ContactSql::find_by_address_and_update(&mut conn, &k.to_bytes(), UpdateContact {
                        alias: Some(c.clone().alias),
                        last_seen: None,
                        latency: None,
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
                        })?;
                    return Ok(Some(DbValue::TariAddress(Box::new(
                        TariAddress::from_bytes(&contact.address)
                            .map_err(|_| ContactsServiceStorageError::ConversionError)?,
                    ))));
                },
                DbKeyValuePair::Contact(..) => return Err(ContactsServiceStorageError::OperationNotSupported),
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
            },
        }

        Ok(None)
    }
}

/// A Sql version of the Contact struct
#[derive(Clone, Debug, Queryable, Insertable, PartialEq, Eq)]
#[diesel(table_name = contacts)]
struct ContactSql {
    address: Vec<u8>,
    node_id: Vec<u8>,
    alias: String,
    last_seen: Option<NaiveDateTime>,
    latency: Option<i32>,
}

impl ContactSql {
    /// Write this struct to the database
    pub fn commit(&self, conn: &mut SqliteConnection) -> Result<(), ContactsServiceStorageError> {
        diesel::insert_into(contacts::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    /// Return all contacts
    pub fn index(conn: &mut SqliteConnection) -> Result<Vec<ContactSql>, ContactsServiceStorageError> {
        Ok(contacts::table.load::<ContactSql>(conn)?)
    }

    /// Find a particular Contact by their address, if it exists
    pub fn find_by_address(
        address: &[u8],
        conn: &mut SqliteConnection,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        Ok(contacts::table
            .filter(contacts::address.eq(address))
            .first::<ContactSql>(conn)?)
    }

    /// Find a particular Contact by their node ID, if it exists
    pub fn find_by_node_id(
        node_id: &[u8],
        conn: &mut SqliteConnection,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        Ok(contacts::table
            .filter(contacts::node_id.eq(node_id))
            .first::<ContactSql>(conn)?)
    }

    /// Find a particular Contact by their address, and update it if it exists, returning the affected record
    pub fn find_by_address_and_update(
        conn: &mut SqliteConnection,
        address: &[u8],
        updated_contact: UpdateContact,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        diesel::update(contacts::table.filter(contacts::address.eq(address)))
            .set(updated_contact)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        ContactSql::find_by_address(address, conn)
    }

    /// Find a particular Contact by their address, and delete it if it exists, returning the affected record
    pub fn find_by_address_and_delete(
        conn: &mut SqliteConnection,
        address: &[u8],
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        let contact = ContactSql::find_by_address(address, conn)?;
        if diesel::delete(contacts::table.filter(contacts::address.eq(address))).execute(conn)? == 0 {
            return Err(ContactsServiceStorageError::ValuesNotFound);
        }
        Ok(contact)
    }

    /// Find a particular Contact by their node ID, and update it if it exists, returning the affected record
    pub fn find_by_node_id_and_update(
        conn: &mut SqliteConnection,
        node_id: &[u8],
        updated_contact: UpdateContact,
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        diesel::update(contacts::table.filter(contacts::node_id.eq(node_id)))
            .set(updated_contact)
            .execute(conn)
            .num_rows_affected_or_not_found(1)?;
        ContactSql::find_by_node_id(node_id, conn)
    }

    /// Find a particular Contact by their node ID, and delete it if it exists, returning the affected record
    pub fn find_by_node_id_and_delete(
        conn: &mut SqliteConnection,
        node_id: &[u8],
    ) -> Result<ContactSql, ContactsServiceStorageError> {
        // Note: `get_result` not implemented for SQLite
        let contact = ContactSql::find_by_node_id(node_id, conn)?;
        if diesel::delete(contacts::table.filter(contacts::node_id.eq(node_id))).execute(conn)? == 0 {
            return Err(ContactsServiceStorageError::ValuesNotFound);
        }
        Ok(contact)
    }
}

/// Conversion from an Contact to the Sql datatype form
impl TryFrom<ContactSql> for Contact {
    type Error = ContactsServiceStorageError;

    #[allow(clippy::cast_sign_loss)]
    fn try_from(o: ContactSql) -> Result<Self, Self::Error> {
        let address = TariAddress::from_bytes(&o.address).map_err(|_| ContactsServiceStorageError::ConversionError)?;
        Ok(Self {
            // Public key must always be the master data source for node ID here
            node_id: NodeId::from_key(address.public_key()),
            address,
            alias: o.alias,
            last_seen: o.last_seen,
            latency: o.latency.map(|val| val as u32),
        })
    }
}

/// Conversion from a Contact to the Sql datatype form
#[allow(clippy::cast_possible_wrap)]
impl From<Contact> for ContactSql {
    fn from(o: Contact) -> Self {
        Self {
            // Public key must always be the master data source for node ID here
            node_id: NodeId::from_key(o.address.public_key()).to_vec(),
            address: o.address.to_bytes().to_vec(),
            alias: o.alias,
            last_seen: o.last_seen,
            latency: o.latency.map(|val| val as i32),
        }
    }
}

#[derive(AsChangeset)]
#[diesel(table_name = contacts)]
pub struct UpdateContact {
    alias: Option<String>,
    last_seen: Option<Option<NaiveDateTime>>,
    latency: Option<Option<i32>>,
}

#[cfg(test)]
mod test {
    use std::{convert::TryFrom, io::Write};

    use diesel::{sql_query, Connection, RunQueryDsl, SqliteConnection};
    use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
    use rand::rngs::OsRng;
    use tari_common::configuration::Network;
    use tari_common_types::{
        tari_address::TariAddress,
        types::{PrivateKey, PublicKey},
    };
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
    use tari_test_utils::{paths::with_temp_dir, random::string};

    use crate::contacts_service::storage::{
        database::Contact,
        sqlite_db::{ContactSql, UpdateContact},
    };

    #[test]
    fn test_crud() {
        with_temp_dir(|dir_path| {
            let db_name = format!("{}.sqlite3", string(8).as_str());
            let db_path = format!("{}/{}", dir_path.to_str().unwrap(), db_name);

            const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
            let mut conn =
                SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

            conn.run_pending_migrations(MIGRATIONS)
                .map(|v| {
                    v.into_iter()
                        .map(|b| {
                            let m = format!("Running migration {}", b);
                            std::io::stdout()
                                .write_all(m.as_ref())
                                .expect("Couldn't write migration number to stdout");
                            m
                        })
                        .collect::<Vec<String>>()
                })
                .expect("Migrations failed");

            sql_query("PRAGMA foreign_keys = ON").execute(&mut conn).unwrap();

            let names = ["Alice".to_string(), "Bob".to_string(), "Carol".to_string()];

            let mut contacts = Vec::new();
            for i in 0..names.len() {
                let pub_key = PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng));
                let address = TariAddress::new(pub_key, Network::default());
                contacts.push(Contact::new(names[i].clone(), address, None, None));
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
                })
                .unwrap();

            let c_updated = ContactSql::find_by_address(&contacts[1].address.to_bytes(), &mut conn).unwrap();
            assert_eq!(c_updated.alias, "Fred".to_string());
        });
    }
}
