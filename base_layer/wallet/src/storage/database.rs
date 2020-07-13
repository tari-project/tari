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

use crate::error::WalletStorageError;
use log::*;
use std::{
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use tari_comms::types::CommsSecretKey;

const LOG_TARGET: &str = "wallet::database";

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait WalletBackend: Send + Sync {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    CommsSecretKey,
}

pub enum DbValue {
    CommsSecretKey(CommsSecretKey),
}

#[derive(Clone)]
pub enum DbKeyValuePair {
    CommsSecretKey(CommsSecretKey),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

pub struct WalletDatabase<T>
where T: WalletBackend + 'static
{
    db: Arc<T>,
}

impl<T> WalletDatabase<T>
where T: WalletBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub async fn get_comms_secret_key(&self) -> Result<Option<CommsSecretKey>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::CommsSecretKey) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsSecretKey(k))) => Ok(Some(k)),
            Err(e) => log_error(DbKey::CommsSecretKey, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn set_comms_secret_key(&self, key: CommsSecretKey) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CommsSecretKey(key)))
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn clear_comms_secret_key(&self) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.write(WriteOperation::Remove(DbKey::CommsSecretKey)))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::CommsSecretKey => f.write_str(&"CommsSecretKey".to_string()),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::CommsSecretKey(k) => f.write_str(&format!("CommsSecretKey: {:?}", k)),
        }
    }
}

fn log_error<T>(req: DbKey, err: WalletStorageError) -> Result<T, WalletStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req,
        err.to_string()
    );
    Err(err)
}

#[cfg(test)]
mod test {
    use crate::storage::{
        connection_manager::run_migration_and_create_sqlite_connection,
        database::{WalletBackend, WalletDatabase},
        memory_db::WalletMemoryDatabase,
        sqlite_db::WalletSqliteDatabase,
    };
    use rand::rngs::OsRng;
    use tari_comms::types::CommsSecretKey;
    use tari_crypto::keys::SecretKey;
    use tari_test_utils::random::string;
    use tempfile::tempdir;
    use tokio::runtime::Runtime;

    pub fn test_database_crud<T: WalletBackend + 'static>(backend: T) {
        let mut runtime = Runtime::new().unwrap();

        let db = WalletDatabase::new(backend);

        // Test wallet settings
        assert!(runtime.block_on(db.get_comms_secret_key()).unwrap().is_none());
        let secret_key = CommsSecretKey::random(&mut OsRng);
        runtime.block_on(db.set_comms_secret_key(secret_key.clone())).unwrap();
        let stored_key = runtime.block_on(db.get_comms_secret_key()).unwrap().unwrap();
        assert_eq!(secret_key, stored_key);
        runtime.block_on(db.clear_comms_secret_key()).unwrap();
        assert!(runtime.block_on(db.get_comms_secret_key()).unwrap().is_none());
    }

    #[test]
    fn test_database_crud_memory_db() {
        test_database_crud(WalletMemoryDatabase::new());
    }

    #[test]
    fn test_database_crud_sqlite_db() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = tempdir().unwrap().path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();

        test_database_crud(WalletSqliteDatabase::new(connection));
    }
}
