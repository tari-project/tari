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

use std::{
    fmt::{Display, Error, Formatter},
    sync::Arc,
};

use aes_gcm::Aes256Gcm;
use log::*;
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{IdentitySignature, PeerFeatures},
    tor::TorIdentity,
};
use tari_key_manager::cipher_seed::CipherSeed;

use crate::{error::WalletStorageError, utxo_scanner_service::service::ScannedBlock};

const LOG_TARGET: &str = "wallet::database";

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait WalletBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError>;
    /// Apply encryption to the backend.
    fn apply_encryption(&self, passphrase: String) -> Result<Aes256Gcm, WalletStorageError>;
    /// Remove encryption from the backend.
    fn remove_encryption(&self) -> Result<(), WalletStorageError>;

    fn get_scanned_blocks(&self) -> Result<Vec<ScannedBlock>, WalletStorageError>;
    fn save_scanned_block(&self, scanned_block: ScannedBlock) -> Result<(), WalletStorageError>;
    fn clear_scanned_blocks(&self) -> Result<(), WalletStorageError>;
    /// Clear scanned blocks from the givne height and higher
    fn clear_scanned_blocks_from_and_higher(&self, height: u64) -> Result<(), WalletStorageError>;
    /// Clear scanned block history from before the specified height. Choice to exclude blocks that contained recovered
    /// outputs
    fn clear_scanned_blocks_before_height(
        &self,
        height: u64,
        exclude_recovered: bool,
    ) -> Result<(), WalletStorageError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    CommsAddress,
    CommsFeatures,
    CommsIdentitySignature,
    TorId,
    BaseNodeChainMetadata,
    ClientKey(String),
    MasterSeed,
    PassphraseHash,
    EncryptionSalt,
    WalletBirthday,
}

pub enum DbValue {
    CommsAddress(Multiaddr),
    CommsFeatures(PeerFeatures),
    CommsIdentitySignature(Box<IdentitySignature>),
    TorId(TorIdentity),
    ClientValue(String),
    ValueCleared,
    BaseNodeChainMetadata(ChainMetadata),
    MasterSeed(CipherSeed),
    PassphraseHash(String),
    EncryptionSalt(String),
    WalletBirthday(String),
}

#[derive(Clone)]
pub enum DbKeyValuePair {
    ClientKeyValue(String, String),
    TorId(TorIdentity),
    BaseNodeChainMetadata(ChainMetadata),
    MasterSeed(CipherSeed),
    CommsAddress(Multiaddr),
    CommsFeatures(PeerFeatures),
    CommsIdentitySignature(Box<IdentitySignature>),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

#[derive(Clone)]
pub struct WalletDatabase<T> {
    db: Arc<T>,
}

impl<T> WalletDatabase<T>
where T: WalletBackend + 'static
{
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    pub async fn get_master_seed(&self) -> Result<Option<CipherSeed>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::MasterSeed) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::MasterSeed(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::MasterSeed, other),
            Err(e) => log_error(DbKey::MasterSeed, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn set_master_seed(&self, seed: CipherSeed) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.write(WriteOperation::Insert(DbKeyValuePair::MasterSeed(seed))))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn clear_master_seed(&self) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.write(WriteOperation::Remove(DbKey::MasterSeed)))
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
            Err(e) => log_error(DbKey::TorId, e),
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

    pub async fn get_node_address(&self) -> Result<Option<Multiaddr>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::CommsAddress) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsAddress(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::CommsAddress, other),
            Err(e) => log_error(DbKey::CommsAddress, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn set_node_address(&self, address: Multiaddr) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CommsAddress(address)))
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn get_node_features(&self) -> Result<Option<PeerFeatures>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::CommsFeatures) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsFeatures(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::CommsFeatures, other),
            Err(e) => log_error(DbKey::CommsFeatures, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn set_node_features(&self, features: PeerFeatures) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CommsFeatures(features)))
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn get_comms_identity_signature(&self) -> Result<Option<IdentitySignature>, WalletStorageError> {
        let db = self.db.clone();

        let sig = tokio::task::spawn_blocking(move || match db.fetch(&DbKey::CommsIdentitySignature) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsIdentitySignature(k))) => Ok(Some(*k)),
            Ok(Some(other)) => unexpected_result(DbKey::CommsIdentitySignature, other),
            Err(e) => log_error(DbKey::CommsIdentitySignature, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(sig)
    }

    pub async fn set_comms_identity_signature(&self, sig: IdentitySignature) -> Result<(), WalletStorageError> {
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db.write(WriteOperation::Insert(DbKeyValuePair::CommsIdentitySignature(
                Box::new(sig),
            )))
        })
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

    pub async fn apply_encryption(&self, passphrase: String) -> Result<Aes256Gcm, WalletStorageError> {
        let db_clone = self.db.clone();
        tokio::task::spawn_blocking(move || db_clone.apply_encryption(passphrase))
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

    pub async fn get_wallet_birthday(&self) -> Result<u16, WalletStorageError> {
        let db_clone = self.db.clone();

        let result = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::WalletBirthday) {
            Ok(None) => Err(WalletStorageError::ValueNotFound(DbKey::WalletBirthday)),
            Ok(Some(DbValue::WalletBirthday(b))) => Ok(b
                .parse::<u16>()
                .map_err(|_| WalletStorageError::ConversionError("Could not parse wallet birthday".to_string()))?),
            Ok(Some(other)) => unexpected_result(DbKey::WalletBirthday, other),
            Err(e) => log_error(DbKey::WalletBirthday, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(result)
    }

    pub async fn get_scanned_blocks(&self) -> Result<Vec<ScannedBlock>, WalletStorageError> {
        let db_clone = self.db.clone();

        let result = tokio::task::spawn_blocking(move || db_clone.get_scanned_blocks())
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(result)
    }

    pub async fn save_scanned_block(&self, scanned_block: ScannedBlock) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.save_scanned_block(scanned_block))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn clear_scanned_blocks(&self) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.clear_scanned_blocks())
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn clear_scanned_blocks_from_and_higher(&self, height: u64) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.clear_scanned_blocks_from_and_higher(height))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }

    pub async fn clear_scanned_blocks_before_height(
        &self,
        height: u64,
        exclude_recovered: bool,
    ) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || db_clone.clear_scanned_blocks_before_height(height, exclude_recovered))
            .await
            .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;

        Ok(())
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::MasterSeed => f.write_str("MasterSeed"),
            DbKey::CommsAddress => f.write_str("CommsAddress"),
            DbKey::CommsFeatures => f.write_str("Nod features"),
            DbKey::TorId => f.write_str("TorId"),
            DbKey::ClientKey(k) => f.write_str(&format!("ClientKey: {:?}", k)),
            DbKey::BaseNodeChainMetadata => f.write_str("Last seen Chain metadata from basw node"),
            DbKey::PassphraseHash => f.write_str("PassphraseHash"),
            DbKey::EncryptionSalt => f.write_str("EncryptionSalt"),
            DbKey::WalletBirthday => f.write_str("WalletBirthday"),
            DbKey::CommsIdentitySignature => f.write_str("CommsIdentitySignature"),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::MasterSeed(k) => f.write_str(&format!("MasterSeed: {:?}", k)),
            DbValue::ClientValue(v) => f.write_str(&format!("ClientValue: {:?}", v)),
            DbValue::ValueCleared => f.write_str("ValueCleared"),
            DbValue::CommsFeatures(_) => f.write_str("Node features"),
            DbValue::CommsAddress(_) => f.write_str("Comms Address"),
            DbValue::TorId(v) => f.write_str(&format!("Tor ID: {}", v)),
            DbValue::BaseNodeChainMetadata(v) => f.write_str(&format!("Last seen Chain metadata from base node:{}", v)),
            DbValue::PassphraseHash(h) => f.write_str(&format!("PassphraseHash: {}", h)),
            DbValue::EncryptionSalt(s) => f.write_str(&format!("EncryptionSalt: {}", s)),
            DbValue::WalletBirthday(b) => f.write_str(&format!("WalletBirthday: {}", b)),
            DbValue::CommsIdentitySignature(_) => f.write_str("CommsIdentitySignature"),
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
    use tari_key_manager::cipher_seed::CipherSeed;
    use tari_test_utils::random::string;
    use tempfile::tempdir;
    use tokio::runtime::Runtime;

    use crate::storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::run_migration_and_create_sqlite_connection,
    };

    #[test]
    fn test_database_crud() {
        let runtime = Runtime::new().unwrap();

        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = tempdir().unwrap().path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name), 16).unwrap();

        let db = WalletDatabase::new(WalletSqliteDatabase::new(connection, None).unwrap());

        // Test wallet settings
        assert!(runtime.block_on(db.get_master_seed()).unwrap().is_none());
        let seed = CipherSeed::new();
        runtime.block_on(db.set_master_seed(seed.clone())).unwrap();
        let stored_seed = runtime.block_on(db.get_master_seed()).unwrap().unwrap();
        assert_eq!(seed, stored_seed);
        runtime.block_on(db.clear_master_seed()).unwrap();
        assert!(runtime.block_on(db.get_master_seed()).unwrap().is_none());

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
}
