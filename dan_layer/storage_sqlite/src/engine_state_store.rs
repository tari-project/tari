//  Copyright 2022. The Tari Project
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

use diesel::{connection::TransactionManager, Connection, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection};
use tari_dan_engine::state_store::{AtomicDb, StateReader, StateStoreError, StateWriter};

use crate::{diesel::ExpressionMethods, error::SqliteStorageError, schema::metadata};
pub struct SqliteStateStore {
    conn: SqliteConnection,
}

impl SqliteStateStore {
    pub fn try_connect(url: &str) -> Result<Self, SqliteStorageError> {
        let conn = SqliteConnection::establish(url)?;
        conn.execute("PRAGMA foreign_keys = ON;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "set pragma".to_string(),
            })?;
        Ok(Self { conn })
    }

    pub fn migrate(&self) -> Result<(), SqliteStorageError> {
        embed_migrations!("./migrations");
        embedded_migrations::run(&self.conn)?;
        Ok(())
    }

    fn access(&self) -> Result<SqliteTransaction<'_>, SqliteStorageError> {
        let manager = self.conn.transaction_manager();
        manager
            .begin_transaction(&self.conn)
            .map_err(|err| SqliteStorageError::DieselError {
                source: err,
                operation: "begin transaction".to_string(),
            })?;
        Ok(SqliteTransaction::new(&self.conn))
    }
}

impl<'a> AtomicDb<'a> for SqliteStateStore {
    type Error = SqliteStorageError;
    type ReadAccess = SqliteTransaction<'a>;
    type WriteAccess = SqliteTransaction<'a>;

    fn read_access(&'a self) -> Result<Self::ReadAccess, Self::Error> {
        self.access()
    }

    fn write_access(&'a self) -> Result<Self::WriteAccess, Self::Error> {
        self.access()
    }

    fn commit(&self, tx: Self::WriteAccess) -> Result<(), Self::Error> {
        self.conn
            .transaction_manager()
            .commit_transaction(tx.conn)
            .map_err(|err| SqliteStorageError::DieselError {
                source: err,
                operation: "commit transaction".to_string(),
            })?;

        Ok(())
    }
}

pub struct SqliteTransaction<'a> {
    conn: &'a SqliteConnection,
}

impl<'a> SqliteTransaction<'a> {
    fn new(conn: &'a SqliteConnection) -> Self {
        Self { conn }
    }
}

impl<'a> StateReader for SqliteTransaction<'a> {
    fn get_state_raw(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StateStoreError> {
        use crate::schema::metadata::dsl;
        let val = dsl::metadata
            .select(metadata::value)
            .filter(metadata::key.eq(key))
            .first::<Vec<u8>>(self.conn)
            .optional()
            .map_err(|source| {
                StateStoreError::custom(SqliteStorageError::DieselError {
                    source,
                    operation: "get state".to_string(),
                })
            })?;

        Ok(val)
    }

    fn exists(&self, key: &[u8]) -> Result<bool, StateStoreError> {
        use crate::schema::metadata::dsl;
        let val = dsl::metadata
            .count()
            .filter(metadata::key.eq(key))
            .limit(1)
            .first::<i64>(self.conn)
            .map_err(|source| {
                StateStoreError::custom(SqliteStorageError::DieselError {
                    source,
                    operation: "get state".to_string(),
                })
            })?;

        Ok(val > 0)
    }
}

impl<'a> StateWriter for SqliteTransaction<'a> {
    fn set_state_raw(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), StateStoreError> {
        use crate::schema::metadata::dsl;

        // TODO: Check key exists without getting the data
        match self.get_state_raw(key) {
            Ok(Some(_)) => diesel::update(dsl::metadata.filter(metadata::key.eq(key)))
                .set(metadata::value.eq(value))
                .execute(self.conn)
                .map_err(|source| {
                    StateStoreError::custom(SqliteStorageError::DieselError {
                        source,
                        operation: "update::metadata".to_string(),
                    })
                })?,
            Ok(None) => diesel::insert_into(metadata::table)
                .values((metadata::key.eq(key), metadata::value.eq(value)))
                .execute(self.conn)
                .map_err(|source| {
                    StateStoreError::custom(SqliteStorageError::DieselError {
                        source,
                        operation: "insert::metadata".to_string(),
                    })
                })?,
            Err(e) => return Err(e),
        };

        Ok(())
    }
}

impl Drop for SqliteTransaction<'_> {
    fn drop(&mut self) {
        if let Err(err) = self.conn.transaction_manager().rollback_transaction(self.conn) {
            log::error!("Error rolling back transaction: {:?}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use borsh::{BorshDeserialize, BorshSerialize};

    use super::*;

    #[test]
    fn read_write_rollback_commit() {
        #[derive(Debug, BorshSerialize, BorshDeserialize, PartialEq, Eq, Clone)]
        struct UserData {
            name: String,
            age: u8,
        }

        let user_data = UserData {
            name: "Foo".to_string(),
            age: 99,
        };

        let store = SqliteStateStore::try_connect(":memory:").unwrap();
        store.migrate().unwrap();
        {
            let mut access = store.write_access().unwrap();
            access.set_state(b"abc", user_data.clone()).unwrap();
            let res = access.get_state(b"abc").unwrap();
            assert_eq!(res, Some(user_data.clone()));
            assert!(access.exists(b"abc").unwrap());
            let res = access.get_state::<_, UserData>(b"def").unwrap();
            assert_eq!(res, None);
            // Drop without commit rolls back
        }

        {
            let access = store.read_access().unwrap();
            let res = access.get_state::<_, UserData>(b"abc").unwrap();
            assert_eq!(res, None);
            assert!(!access.exists(b"abc").unwrap());
        }

        {
            let mut access = store.write_access().unwrap();
            access.set_state(b"abc", user_data.clone()).unwrap();
            store.commit(access).unwrap();
        }

        let access = store.read_access().unwrap();
        let res = access.get_state(b"abc").unwrap();
        assert_eq!(res, Some(user_data));
    }
}
