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

use std::{
    convert::TryFrom,
    sync::{Arc, RwLock},
};

use chacha20poly1305::XChaCha20Poly1305;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
pub use key_manager_state::{KeyManagerStateSql, NewKeyManagerStateSql};
use log::*;
use tari_common_sqlite::{error::SqliteStorageError, sqlite_connection_pool::PooledDbConnection};
use tari_common_types::{
    encryption::Encryptable,
    types::{CompressedPublicKey, PrivateKey},
};
use tari_utilities::acquire_read_lock;
use tokio::time::Instant;

use crate::{
    legacy_key_manager::{error::KeyManagerStorageError, KeyManagerState, TransactionKeyManagerBackend},
    storage::{
        database::ImportedKey,
        sqlite_db::imported_keys::{ImportedKeySql, NewImportedKeySql},
    },
};
mod imported_keys;
mod key_manager_state;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./src/migrations");
const LOG_TARGET: &str = "wallet::key_manager_service::database::wallet";

/// A Sqlite backend for the Output Manager Service. The Backend is accessed via a connection pool to the Sqlite file.
#[derive(Clone)]
pub struct TransactionKeyManagerSqliteDatabase<TTransactionKeyManagerDbConnection> {
    database_connection: Arc<TTransactionKeyManagerDbConnection>,
    cipher: Arc<RwLock<XChaCha20Poly1305>>,
}

impl<TTransactionKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Clone>
    TransactionKeyManagerSqliteDatabase<TTransactionKeyManagerDbConnection>
{
    /// Creates a new sql backend from provided wallet db connection
    /// * `cipher` is used to encrypt the sensitive fields in the database, a cipher is derived
    /// * from a provided password, which we enforce for class instantiation
    fn new(database_connection: TTransactionKeyManagerDbConnection, cipher: XChaCha20Poly1305) -> Self {
        Self {
            database_connection: Arc::new(database_connection),
            cipher: Arc::new(RwLock::new(cipher)),
        }
    }

    pub fn init(database_connection: TTransactionKeyManagerDbConnection, cipher: XChaCha20Poly1305) -> Self {
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
                        let m = format!("Running migration {b}");
                        m
                    })
                    .collect::<Vec<String>>()
            })
            .map_err(|e| SqliteStorageError::DieselR2d2Error(e.to_string()))
    }
}

enum TempError {
    KeyManagerError(KeyManagerStorageError),
    DieselError(diesel::result::Error),
}

impl TempError {
    fn into_key_manager_error(self) -> KeyManagerStorageError {
        match self {
            TempError::KeyManagerError(e) => e,
            TempError::DieselError(e) => KeyManagerStorageError::StorageError(e.to_string()),
        }
    }
}

impl From<diesel::result::Error> for TempError {
    fn from(e: diesel::result::Error) -> Self {
        TempError::DieselError(e)
    }
}

impl From<KeyManagerStorageError> for TempError {
    fn from(e: KeyManagerStorageError) -> Self {
        TempError::KeyManagerError(e)
    }
}

#[async_trait::async_trait]
impl<TTransactionKeyManagerDbConnection> TransactionKeyManagerBackend
    for TransactionKeyManagerSqliteDatabase<TTransactionKeyManagerDbConnection>
where TTransactionKeyManagerDbConnection: PooledDbConnection<Error = SqliteStorageError> + Send + Sync + Clone
{
    async fn get_key_manager(&self, branch: &str) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self
            .database_connection
            .get_pooled_connection()
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        let acquire_lock = start.elapsed();

        let result = match KeyManagerStateSql::get_state(branch, &mut conn).ok() {
            None => None,
            Some(km) => {
                let cipher = acquire_read_lock!(self.cipher);
                let km = km
                    .decrypt(&cipher)
                    .map_err(|e| KeyManagerStorageError::AeadError(format!("Decryption Error: {e}")))?;
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

    async fn add_key_manager(&self, key_manager: KeyManagerState) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self
            .database_connection
            .get_pooled_connection()
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);

        let km_sql = NewKeyManagerStateSql::from(key_manager);
        let km_sql = km_sql
            .encrypt(&cipher)
            .map_err(|e| KeyManagerStorageError::AeadError(format!("Encryption Error: {e}")))?;
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

    async fn increment_key_index(&self, branch: &str) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self
            .database_connection
            .get_pooled_connection()
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;

        let acquire_lock = start.elapsed();
        conn.immediate_transaction::<_, TempError, _>(|conn| {
            let cipher = acquire_read_lock!(self.cipher);
            let km = KeyManagerStateSql::get_state(branch, conn)?;
            let mut km = km
                .decrypt(&cipher)
                .map_err(|e| KeyManagerStorageError::AeadError(format!("Decryption Error: {e}")))?;
            let mut bytes: [u8; 8] = [0u8; 8];
            bytes.copy_from_slice(km.primary_key_index.get(..8).expect("Already checked"));
            let index = u64::from_le_bytes(bytes) + 1;
            km.primary_key_index = index.to_le_bytes().to_vec();
            let km = km
                .encrypt(&cipher)
                .map_err(|e| KeyManagerStorageError::AeadError(format!("Encryption Error: {e}")))?;
            KeyManagerStateSql::set_index(km.id, km.primary_key_index, conn)?;
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
        })
        .map_err(|e| e.into_key_manager_error())
    }

    async fn set_key_index(&self, branch: &str, index: u64) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self
            .database_connection
            .get_pooled_connection()
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        let acquire_lock = start.elapsed();
        conn.immediate_transaction::<_, TempError, _>(|conn| {
            let cipher = acquire_read_lock!(self.cipher);
            let km = KeyManagerStateSql::get_state(branch, conn)?;
            let mut km = km
                .decrypt(&cipher)
                .map_err(|e| KeyManagerStorageError::AeadError(format!("Decryption Error: {e}")))?;
            km.primary_key_index = index.to_le_bytes().to_vec();
            let km = km
                .encrypt(&cipher)
                .map_err(|e| KeyManagerStorageError::AeadError(format!("Encryption Error: {e}")))?;
            KeyManagerStateSql::set_index(km.id, km.primary_key_index, conn)?;
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
        })
        .map_err(|e| e.into_key_manager_error())
    }

    async fn insert_imported_key(
        &self,
        public_key: CompressedPublicKey,
        private_key: PrivateKey,
    ) -> Result<(), KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self
            .database_connection
            .get_pooled_connection()
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        // check if we already have the key:
        conn.immediate_transaction::<_, TempError, _>(|conn| {
            if ImportedKeySql::key_exists(&public_key, conn)? {
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
            encrypted_key.commit(conn)?;
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
        })
        .map_err(|e| e.into_key_manager_error())
    }

    async fn get_imported_key(&self, public_key: &CompressedPublicKey) -> Result<PrivateKey, KeyManagerStorageError> {
        let start = Instant::now();
        let mut conn = self
            .database_connection
            .get_pooled_connection()
            .map_err(|e| KeyManagerStorageError::StorageError(e.to_string()))?;
        let acquire_lock = start.elapsed();
        let cipher = acquire_read_lock!(self.cipher);
        let key = ImportedKeySql::get_key(public_key, &mut conn)?;
        let unencrypted_key = key.to_imported_key(&cipher)?;
        if start.elapsed().as_millis() > 0 {
            trace!(
                target: LOG_TARGET,
                "sqlite profile - get_imported_key: lock {} + db_op {} = {} ms",
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
    #![allow(clippy::indexing_slicing)]
    use diesel::{sql_query, Connection, RunQueryDsl, SqliteConnection};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_key_manager_crud() {
        let db_name = format!("{}.sqlite3", "test");
        let temp_dir = tempdir().unwrap();
        let db_folder = temp_dir.path().to_str().unwrap().to_string();
        let db_path = format!("{db_folder}{db_name}");

        let mut conn =
            SqliteConnection::establish(&db_path).unwrap_or_else(|_| panic!("Error connecting to {db_path}"));

        conn.run_pending_migrations(MIGRATIONS)
            .map(|v| {
                v.into_iter()
                    .map(|b| {
                        let m = format!("Running migration {b}");
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
