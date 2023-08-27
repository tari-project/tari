// Copyright 2022. The Taiji Project
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

use std::{
    convert::TryFrom,
    sync::{Arc, RwLock},
};

use chacha20poly1305::XChaCha20Poly1305;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
pub use key_manager_state::{KeyManagerStateSql, NewKeyManagerStateSql};
use log::*;
use taiji_common_sqlite::{error::SqliteStorageError, sqlite_connection_pool::PooledDbConnection};
use taiji_common_types::encryption::Encryptable;
use tari_crypto::keys::PublicKey;
use tari_utilities::acquire_read_lock;
use tokio::time::Instant;

use crate::key_manager_service::{
    error::KeyManagerStorageError,
    storage::{
        database::{ImportedKey, KeyManagerBackend, KeyManagerState},
        sqlite_db::imported_keys::{ImportedKeySql, NewImportedKeySql},
    },
};

mod imported_keys;
mod key_manager_state;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
const LOG_TARGET: &str = "wallet::key_manager_service::database::wallet";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct KeyManagerSqliteDatabase<TKeyManagerDbConnection> {
    database_connection: Arc<TKeyManagerDbConnection>,
    cipher: Arc<RwLock<XChaCha20Poly1305>>,
}

impl<TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone>
    KeyManagerSqliteDatabase<TKeyManagerDbConnection>
{
    /// Creates a new sql backend from provided wallet db connection
    /// * `cipher` is used to encrypt the sensitive fields in the database, a cipher is derived
    /// from a provided password, which we enforce for class instantiation
    fn new(database_connection: TKeyManagerDbConnection, cipher: XChaCha20Poly1305) -> Self {
        Self {
            database_connection: Arc::new(database_connection),
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    pub fn init(database_connection: TKeyManagerDbConnection, cipher: XChaCha20Poly1305) -> Self {
        let db = Self::new(database_connection, cipher);
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

impl<TKeyManagerDbConnection, PK> KeyManagerBackend<PK> for KeyManagerSqliteDatabase<TKeyManagerDbConnection>
where
    TKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Send + Sync + Clone,
    PK: PublicKey,
{
    fn get_key_manager(&self, branch: &str) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();

        let result = match KeyManagerStateSql::get_state(branch, &mut conn).ok() {
            None => None,
            Some(km) => {
                let cipher = acquire_read_lock!(self.cipher);
                let km = km
                    .decrypt(&cipher)
                    .map_err(|e| KeyManagerStorageError::AeadError(format!("Decryption Error: {}", e)))?;
                Some(KeyManagerState::try_from(km)?)
            },
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
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let km_sql = NewKeyManagerStateSql::from(key_manager);
        let km_sql = km_sql
            .encrypt(&cipher)
            .map_err(|e| KeyManagerStorageError::AeadError(format!("Encryption Error: {}", e)))?;
        km_sql.commit(&mut conn)?;
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

    fn increment_key_index(&self, branch: &str) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        let km = KeyManagerStateSql::get_state(branch, &mut conn)?;
        let mut km = km
            .decrypt(&cipher)
            .map_err(|e| KeyManagerStorageError::AeadError(format!("Decryption Error: {}", e)))?;
        let mut bytes: [u8; 8] = [0u8; 8];
        bytes.copy_from_slice(&km.primary_key_index[..8]);
        let index = u64::from_le_bytes(bytes) + 1;
        km.primary_key_index = index.to_le_bytes().to_vec();
        let km = km
            .encrypt(&cipher)
            .map_err(|e| KeyManagerStorageError::AeadError(format!("Encryption Error: {}", e)))?;
        KeyManagerStateSql::set_index(km.id, km.primary_key_index, &mut conn)?;
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

    fn set_key_index(&self, branch: &str, index: u64) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        let km = KeyManagerStateSql::get_state(branch, &mut conn)?;
        let mut km = km
            .decrypt(&cipher)
            .map_err(|e| KeyManagerStorageError::AeadError(format!("Decryption Error: {}", e)))?;
        km.primary_key_index = index.to_le_bytes().to_vec();
        let km = km
            .encrypt(&cipher)
            .map_err(|e| KeyManagerStorageError::AeadError(format!("Encryption Error: {}", e)))?;
        KeyManagerStateSql::set_index(km.id, km.primary_key_index, &mut conn)?;
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

    fn insert_imported_key(&self, public_key: PK, private_key: PK::K) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        // check if we already have the key:
        if self.get_imported_key(&public_key).is_ok() {
            // we already have the key so we dont have to add it in
            return Ok(());
        }
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        let key = ImportedKey {
            public_key,
            private_key,
        };
        let encrypted_key = NewImportedKeySql::new_from_imported_key(key, &cipher)?;
        encrypted_key.commit(&mut conn)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - insert_imported_key: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(())
    }

    fn get_imported_key(&self, public_key: &PK) -> Result<PK::K, KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self.database_connection.get_pooled_connection()?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        let key = ImportedKeySql::get_key(public_key, &mut conn)?;
        let unencrypted_key = key.to_imported_key::<PK>(&cipher)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - insert_imported_key: lock {} + db_op {} = {} ms",
                acquire_lock.as_millis(),
                (start.elapsed() - acquire_lock).as_millis(),
                start.elapsed().as_millis()
            );
        }

        Ok(unencrypted_key.private_key)
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use diesel::{sql_query, Connection, RunQueryDsl, SqliteConnection};
    use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
    use tempfile::tempdir;

    use super::*;
    use crate::key_manager_service::storage::sqlite_db::{KeyManagerState, KeyManagerStateSql, NewKeyManagerStateSql};

    #[test]
    fn test_key_manager_crud() {
        let db_name = format!("{}.sqlite3", "test");
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{}{}", db_folder, db_name);

        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");
        let mut conn =
            SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {}", db_path));

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
            .expect("Migrations failed");

        sql_query("PRAGMA foreign_keys = ON").execute(&mut conn).unwrap();
        let branch = "branch_key".to_string();
        assert!(KeyManagerStateSql::get_state(&branch, &mut conn).is_err());

        let state1 = KeyManagerState {
            branch_seed: branch.clone(),
            primary_key_index: 0,
        };

        NewKeyManagerStateSql::from(state1.clone()).commit(&mut conn).unwrap();
        let state1_read = KeyManagerStateSql::get_state(&branch, &mut conn).unwrap();
        let id = state1_read.id;

        assert_eq!(state1, KeyManagerState::try_from(state1_read).unwrap());

        let index: u64 = 2;
        KeyManagerStateSql::set_index(id, index.to_le_bytes().to_vec(), &mut conn).unwrap();

        let state3_read = KeyManagerStateSql::get_state(&branch, &mut conn).unwrap();
        let mut bytes: [u8; 8] = [0u8; 8];
        bytes.copy_from_slice(&state3_read.primary_key_index[..8]);
        assert_eq!(u64::from_le_bytes(bytes), 2);
    }
}
