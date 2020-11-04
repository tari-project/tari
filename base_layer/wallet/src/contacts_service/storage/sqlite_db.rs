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

use crate::{
    contacts_service::{
        error::ContactsServiceStorageError,
        storage::database::{Contact, ContactsBackend, DbKey, DbKeyValuePair, DbValue, WriteOperation},
    },
    schema::contacts,
    storage::sqlite_utilities::WalletDbConnection,
};
use diesel::{prelude::*, result::Error as DieselError, SqliteConnection};
use std::convert::TryFrom;
use tari_core::transactions::types::PublicKey;
use tari_crypto::tari_utilities::ByteArray;

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
        let conn = self.database_connection.acquire_lock();

        let result = match key {
            DbKey::Contact(pk) => match ContactSql::find(&pk.to_vec(), &(*conn)) {
                Ok(c) => Some(DbValue::Contact(Box::new(Contact::try_from(c)?))),
                Err(ContactsServiceStorageError::DieselError(DieselError::NotFound)) => None,
                Err(e) => return Err(e),
            },
            DbKey::Contacts => Some(DbValue::Contacts(
                ContactSql::index(&conn)?
                    .iter()
                    .map(|c| Contact::try_from(c.clone()))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        };

        Ok(result)
    }

    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, ContactsServiceStorageError> {
        let conn = self.database_connection.acquire_lock();

        match op {
            WriteOperation::Upsert(kvp) => match kvp {
                DbKeyValuePair::Contact(k, c) => match ContactSql::find(&k.to_vec(), &(*conn)) {
                    Ok(found_c) => {
                        let _ = found_c.update(UpdateContact { alias: Some(c.alias) }, &(*conn))?;
                    },
                    Err(_) => {
                        ContactSql::from(c).commit(&conn)?;
                    },
                },
            },
            WriteOperation::Remove(k) => match k {
                DbKey::Contact(k) => match ContactSql::find(&k.to_vec(), &(*conn)) {
                    Ok(c) => {
                        c.delete(&conn)?;
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
#[derive(Clone, Debug, Queryable, Insertable, PartialEq)]
#[table_name = "contacts"]
struct ContactSql {
    public_key: Vec<u8>,
    alias: String,
}

impl ContactSql {
    /// Write this struct to the database
    pub fn commit(&self, conn: &SqliteConnection) -> Result<(), ContactsServiceStorageError> {
        diesel::insert_into(contacts::table)
            .values(self.clone())
            .execute(conn)?;
        Ok(())
    }

    /// Return all contacts
    pub fn index(conn: &SqliteConnection) -> Result<Vec<ContactSql>, ContactsServiceStorageError> {
        Ok(contacts::table.load::<ContactSql>(conn)?)
    }

    /// Find a particular Contact, if it exists
    pub fn find(public_key: &[u8], conn: &SqliteConnection) -> Result<ContactSql, ContactsServiceStorageError> {
        Ok(contacts::table
            .filter(contacts::public_key.eq(public_key))
            .first::<ContactSql>(conn)?)
    }

    pub fn delete(&self, conn: &SqliteConnection) -> Result<(), ContactsServiceStorageError> {
        let num_deleted =
            diesel::delete(contacts::table.filter(contacts::public_key.eq(&self.public_key))).execute(conn)?;

        if num_deleted == 0 {
            return Err(ContactsServiceStorageError::ValuesNotFound);
        }

        Ok(())
    }

    pub fn update(
        &self,
        updated_contact: UpdateContact,
        conn: &SqliteConnection,
    ) -> Result<ContactSql, ContactsServiceStorageError>
    {
        let num_updated = diesel::update(contacts::table.filter(contacts::public_key.eq(&self.public_key)))
            .set(updated_contact)
            .execute(conn)?;

        if num_updated == 0 {
            return Err(ContactsServiceStorageError::UnexpectedResult(
                "Database update error".to_string(),
            ));
        }

        Ok(ContactSql::find(&self.public_key, conn)?)
    }
}

/// Conversion from an Contact to the Sql datatype form
impl TryFrom<ContactSql> for Contact {
    type Error = ContactsServiceStorageError;

    fn try_from(o: ContactSql) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: PublicKey::from_vec(&o.public_key).map_err(|_| ContactsServiceStorageError::ConversionError)?,
            alias: o.alias,
        })
    }
}

/// Conversion from an Contact to the Sql datatype form
impl From<Contact> for ContactSql {
    fn from(o: Contact) -> Self {
        Self {
            public_key: o.public_key.to_vec(),
            alias: o.alias,
        }
    }
}

#[derive(AsChangeset)]
#[table_name = "contacts"]
pub struct UpdateContact {
    alias: Option<String>,
}

#[cfg(test)]
mod test {
    use crate::contacts_service::storage::{
        database::Contact,
        sqlite_db::{ContactSql, UpdateContact},
    };
    use diesel::{Connection, SqliteConnection};
    use rand::rngs::OsRng;
    use std::convert::TryFrom;
    use tari_core::transactions::types::{PrivateKey, PublicKey};
    use tari_crypto::{
        keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
        tari_utilities::ByteArray,
    };
    use tari_test_utils::{paths::with_temp_dir, random::string};

    #[test]
    fn test_crud() {
        with_temp_dir(|dir_path| {
            let db_name = format!("{}.sqlite3", string(8).as_str());
            let db_path = format!("{}/{}", dir_path.to_str().unwrap(), db_name);

            embed_migrations!("./migrations");
            let conn =
                SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

            embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

            conn.execute("PRAGMA foreign_keys = ON").unwrap();

            let names = ["Alice".to_string(), "Bob".to_string(), "Carol".to_string()];

            let mut contacts = Vec::new();
            for i in 0..names.len() {
                let pub_key = PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng));
                contacts.push(Contact {
                    alias: names[i].clone(),
                    public_key: pub_key,
                });
                ContactSql::from(contacts[i].clone()).commit(&conn).unwrap();
            }

            let retrieved_contacts = ContactSql::index(&conn).unwrap();

            for i in 0..contacts.len() {
                assert!(retrieved_contacts
                    .iter()
                    .find(|v| v == &&ContactSql::from(contacts[i].clone()))
                    .is_some());
            }

            assert_eq!(
                contacts[1],
                Contact::try_from(ContactSql::find(&contacts[1].public_key.to_vec(), &conn).unwrap()).unwrap()
            );

            ContactSql::from(contacts[0].clone()).delete(&conn).unwrap();

            let retrieved_contacts = ContactSql::index(&conn).unwrap();
            assert_eq!(retrieved_contacts.len(), 2);

            assert!(retrieved_contacts
                .iter()
                .find(|v| v == &&ContactSql::from(contacts[0].clone()))
                .is_none());

            let c = ContactSql::find(&contacts[1].public_key.to_vec(), &conn).unwrap();
            c.update(
                UpdateContact {
                    alias: Some("Fred".to_string()),
                },
                &conn,
            )
            .unwrap();

            let c_updated = ContactSql::find(&contacts[1].public_key.to_vec(), &conn).unwrap();
            assert_eq!(c_updated.alias, "Fred".to_string());
        });
    }
}
