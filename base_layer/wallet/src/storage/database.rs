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
use aes_gcm::Aes256Gcm;
use log::*;
use std::{
    fmt::{Display, Error, Formatter},
    sync::Arc,
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::NodeIdentity,
    tor::TorIdentity,
    types::{CommsPublicKey, CommsSecretKey},
};

const LOG_TARGET: &str = "wallet::database";

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait WalletBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), WalletStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), WalletStorageError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    CommsSecretKey,
    CommsPublicKey,
    CommsAddress,
    CommsFeatures,
    Identity,
    TorId,
    BaseNodeChainMetadata,
    ClientKey(String),
}

pub enum DbValue {
    CommsSecretKey(CommsSecretKey),
    CommsPublicKey(CommsPublicKey),
    CommsAddress(Multiaddr),
    CommsFeatures(u64),
    Identity(NodeIdentity),
    TorId(TorIdentity),
    ClientValue(String),
    ValueCleared,
    BaseNodeChainMetadata(ChainMetadata),
}

#[derive(Clone)]
pub enum DbKeyValuePair {
    CommsSecretKey(CommsSecretKey),
    ClientKeyValue(String, String),
    Identity(Box<NodeIdentity>),
    TorId(TorIdentity),
    BaseNodeChainMetadata(ChainMetadata),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

#[derive(Clone)]
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
            Ok(Some(other)) => unexpected_result(DbKey::CommsSecretKey, other),
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

    pub async fn get_tor_id(&self) -> Result<Option<TorIdentity>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::TorId) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::TorId(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::TorId, other),
            Err(e) => log_error(DbKey::CommsSecretKey, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn set_tor_identity(&self, id: TorIdentity) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.write(WriteOperation::Insert(DbKeyValuePair::TorId(id))))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn get_chain_metadata(&self) -> Result<Option<ChainMetadata>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::BaseNodeChainMetadata) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::BaseNodeChainMetadata(metadata))) => Ok(Some(metadata)),
            Ok(Some(other)) => unexpected_result(DbKey::BaseNodeChainMetadata, other),
            Err(e) => log_error(DbKey::BaseNodeChainMetadata, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn set_chain_metadata(&self, metadata: ChainMetadata) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::BaseNodeChainMetadata(metadata)))
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

    pub async fn apply_encryption(&self, cipher: Aes256Gcm) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.apply_encryption(cipher))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn remove_encryption(&self) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.remove_encryption())
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))
            .and_then(|inner_result| inner_result)
    }

    pub async fn set_client_key_value(&self, key: String, value: String) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::ClientKeyValue(key, value)))
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn get_client_key_value(&self, key: String) -> Result<Option<String>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::ClientKey(key.clone())) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::ClientValue(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::ClientKey(key), other),
            Err(e) => log_error(DbKey::ClientKey(key), e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn get_client_key_from_str<V>(&self, key: String) -> Result<Option<V>, WalletStorageError>
    where
        V: std::str::FromStr,
        V::Err: ToString,
    {
        let db = self.db.clone();

        let value = tokio::task::spawn_blocking(move || match db.fetch(&DbKey::ClientKey(key.clone())) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::ClientValue(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::ClientKey(key), other),
            Err(e) => log_error(DbKey::ClientKey(key), e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;

        match value {
            Some(c) => {
                let a = V::from_str(&c).map_err(|err| WalletStorageError::ConversionError(err.to_string()))?;
                Ok(Some(a))
            },
            None => Ok(None),
        }
    }

    pub async fn clear_client_value(&self, key: String) -> Result<bool, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || {
            match db_clone.write(WriteOperation::Remove(DbKey::ClientKey(key.clone()))) {
                Ok(None) => Ok(false),
                Ok(Some(DbValue::ValueCleared)) => Ok(true),
                Ok(Some(other)) => unexpected_result(DbKey::ClientKey(key), other),
                Err(e) => log_error(DbKey::ClientKey(key), e),
            }
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::CommsSecretKey => f.write_str(&"CommsSecretKey".to_string()),
            DbKey::CommsPublicKey => f.write_str(&"CommsPublicKey".to_string()),
            DbKey::CommsAddress => f.write_str(&"CommsAddress".to_string()),
            DbKey::CommsFeatures => f.write_str(&"Node features".to_string()),
            DbKey::Identity => f.write_str(&"NodeIdentity".to_string()),
            DbKey::TorId => f.write_str(&"TorId".to_string()),
            DbKey::ClientKey(k) => f.write_str(&format!("ClientKey: {:?}", k)),
            DbKey::BaseNodeChainMetadata => f.write_str(&"Last seen Chain metadata from base node".to_string()),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::CommsSecretKey(k) => f.write_str(&format!("CommsSecretKey: {:?}", k)),
            DbValue::CommsPublicKey(k) => f.write_str(&format!("CommsPublicKey: {:?}", k)),
            DbValue::ClientValue(v) => f.write_str(&format!("ClientValue: {:?}", v)),
            DbValue::ValueCleared => f.write_str(&"ValueCleared".to_string()),
            DbValue::CommsFeatures(_) => f.write_str(&"Node features".to_string()),
            DbValue::CommsAddress(_) => f.write_str(&"Comms Address".to_string()),
            DbValue::TorId(v) => f.write_str(&format!("Tor ID: {}", v)),
            DbValue::Identity(v) => f.write_str(&format!("Node Identity: {}", v)),
            DbValue::BaseNodeChainMetadata(v) => f.write_str(&format!("Last seen Chain metadata from base node:{}", v)),
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

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, WalletStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(WalletStorageError::UnexpectedResult(msg))
}

#[cfg(test)]
mod test {
    use crate::storage::{
        database::{WalletBackend, WalletDatabase},
        memory_db::WalletMemoryDatabase,
        sqlite_db::WalletSqliteDatabase,
        sqlite_utilities::run_migration_and_create_sqlite_connection,
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

        let client_key_values = vec![
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
            ("key3".to_string(), "value3".to_string()),
        ];

        for kv in client_key_values.iter() {
            runtime
                .block_on(db.set_client_key_value(kv.0.clone(), kv.1.clone()))
                .unwrap();
        }

        assert!(runtime
            .block_on(db.get_client_key_value("wrong".to_string()))
            .unwrap()
            .is_none());

        runtime
            .block_on(db.set_client_key_value(client_key_values[0].0.clone(), "updated".to_string()))
            .unwrap();

        assert_eq!(
            runtime
                .block_on(db.get_client_key_value(client_key_values[0].0.clone()))
                .unwrap()
                .unwrap(),
            "updated".to_string()
        );

        assert!(!runtime.block_on(db.clear_client_value("wrong".to_string())).unwrap());

        assert!(runtime
            .block_on(db.clear_client_value(client_key_values[0].0.clone()))
            .unwrap());

        assert!(!runtime
            .block_on(db.clear_client_value(client_key_values[0].0.clone()))
            .unwrap());
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

        test_database_crud(WalletSqliteDatabase::new(connection, None).unwrap());
    }
}
