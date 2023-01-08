// Copyright 2022. The Tari Project
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

pub use key_manager_state::{KeyManagerStateSql, NewKeyManagerStateSql};
use log::*;
use tokio::time::Instant;

use crate::{
    key_manager_service::{
        error::KeyManagerStorageError,
        storage::database::{KeyManagerBackend, KeyManagerState},
    },
    storage::sqlite_utilities::wallet_db_connection::WalletDbConnection,
};

mod key_manager_state;

const LOG_TARGET: &str = "wallet::key_manager_service::database::wallet";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct KeyManagerSqliteDatabase {
    database_connection: WalletDbConnection,
}

impl KeyManagerSqliteDatabase {
    /// Creates a new sql backend from provided wallet db connection
    /// * `cipher` is used to encrypt the sensitive fields in the database, a cipher is derived
    /// from a provided password, which we enforce for class instantiation
    pub fn new(database_connection: WalletDbConnection) -> Result<Self, KeyManagerStorageError> {
        let db = Self { database_connection };
        Ok(db)
    }
}

impl KeyManagerBackend for KeyManagerSqliteDatabase {
    fn get_key_manager(&self, branch: String) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match KeyManagerStateSql::get_state(&branch, &conn).ok() {
            None => None,
            Some(km) => Some(KeyManagerState::try_from(km)?),
        };
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - fetch key_manager: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(result)
    }

    fn add_key_manager(&self, key_manager: KeyManagerState) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let km_sql = NewKeyManagerStateSql::from(key_manager);
        km_sql.commit(&conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - write Insert key manager: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn increment_key_index(&self, branch: String) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut km = KeyManagerStateSql::get_state(&branch, &conn)?;
        let mut bytes: [u8; 8] = [0u8; 8];
        bytes.copy_from_slice(&km.primary_key_index[..8]);
        let index = u64::from_le_bytes(bytes) + 1;
        km.primary_key_index = index.to_le_bytes().to_vec();
        KeyManagerStateSql::set_index(km.id, km.primary_key_index, &conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - increment_key_index: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn set_key_index(&self, branch: String, index: u64) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let mut km = KeyManagerStateSql::get_state(&branch, &conn)?;
        km.primary_key_index = index.to_le_bytes().to_vec();
        KeyManagerStateSql::set_index(km.id, km.primary_key_index, &conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - set_key_index: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use diesel::{Connection, SqliteConnection};
    use tari_test_utils::random;
    use tempfile::tempdir;

    use crate::key_manager_service::storage::sqlite_db::{KeyManagerState, KeyManagerStateSql, NewKeyManagerStateSql};

    #[test]
    fn test_key_manager_crud() {
        let db_name = format!("{}.sqlite3", random::string(8).as_str());
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        embed_migrations!("./migrations");
        let conn = SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

        embedded_migrations::run_with_output(&conn, &mut std::io::stdout()).expect("Migration failed");

        conn.execute("PRAGMA foreign_keys = ON").unwrap();
        let branch = random::string(8);
        assert!(KeyManagerStateSql::get_state(&branch, &conn).is_err());

        let state1 = KeyManagerState {
            branch_seed: branch.clone(),
            primary_key_index: 0,
        };

        NewKeyManagerStateSql::from(state1.clone()).commit(&conn).unwrap();
        let state1_read = KeyManagerStateSql::get_state(&branch, &conn).unwrap();
        let id = state1_read.id;

        assert_eq!(state1, KeyManagerState::try_from(state1_read).unwrap());

        let index: u64 = 2;
        KeyManagerStateSql::set_index(id, index.to_le_bytes().to_vec(), &conn).unwrap();

        let state3_read = KeyManagerStateSql::get_state(&branch, &conn).unwrap();
        let mut bytes: [u8; 8] = [0u8; 8];
        bytes.copy_from_slice(&state3_read.primary_key_index[..8]);
        assert_eq!(u64::from_le_bytes(bytes), 2);
    }
}
