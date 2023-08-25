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

use chrono::NaiveDateTime;
use log::*;
use tari_common_types::{chain_metadata::ChainMetadata, wallet_types::WalletType};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{IdentitySignature, PeerFeatures},
    tor::TorIdentity,
};
use tari_key_manager::cipher_seed::CipherSeed;
use tari_utilities::SafePassword;

use crate::{error::WalletStorageError, utxo_scanner_service::service::ScannedBlock};

const LOG_TARGET: &str = "wallet::database";

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait WalletBackend: Send + Sync + Clone {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError>;

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

    /// Change the passphrase used to encrypt the database
    fn change_passphrase(&self, existing: &SafePassword, new: &SafePassword) -> Result<(), WalletStorageError>;

    fn create_burnt_proof(
        &self,
        id: u32,
        reciprocal_claim_public_key: String,
        payload: String,
    ) -> Result<(), WalletStorageError>;
    fn fetch_burnt_proof(&self, id: u32) -> Result<(u32, String, String, NaiveDateTime), WalletStorageError>;
    fn fetch_burnt_proofs(&self) -> Result<Vec<(u32, String, String, NaiveDateTime)>, WalletStorageError>;
    fn delete_burnt_proof(&self, id: u32) -> Result<(), WalletStorageError>;
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
    EncryptedMainKey,    // the database encryption key, itself encrypted with the secondary key
    SecondaryKeySalt,    // the salt used (with the user's passphrase) to derive the secondary derivation key
    SecondaryKeyVersion, // the parameter version for the secondary derivation key
    SecondaryKeyHash,    // a hash commitment to the secondary derivation key
    WalletBirthday,
    LastAccessedNetwork,
    LastAccessedVersion,
    WalletType,
}

impl DbKey {
    pub fn to_key_string(&self) -> String {
        match self {
            DbKey::MasterSeed => "MasterSeed".to_string(),
            DbKey::CommsAddress => "CommsAddress".to_string(),
            DbKey::CommsFeatures => "NodeFeatures".to_string(),
            DbKey::TorId => "TorId".to_string(),
            DbKey::ClientKey(k) => format!("ClientKey.{}", k),
            DbKey::BaseNodeChainMetadata => "BaseNodeChainMetadata".to_string(),
            DbKey::EncryptedMainKey => "EncryptedMainKey".to_string(),
            DbKey::SecondaryKeySalt => "SecondaryKeySalt".to_string(),
            DbKey::SecondaryKeyVersion => "SecondaryKeyVersion".to_string(),
            DbKey::SecondaryKeyHash => "SecondaryKeyHash".to_string(),
            DbKey::WalletBirthday => "WalletBirthday".to_string(),
            DbKey::CommsIdentitySignature => "CommsIdentitySignature".to_string(),
            DbKey::LastAccessedNetwork => "LastAccessedNetwork".to_string(),
            DbKey::LastAccessedVersion => "LastAccessedVersion".to_string(),
            DbKey::WalletType => "WalletType".to_string(),
        }
    }
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
    EncryptedMainKey(String),
    SecondaryKeySalt(String),
    SecondaryKeyVersion(String),
    SecondaryKeyHash(String),
    WalletBirthday(String),
    LastAccessedNetwork(String),
    LastAccessedVersion(String),
    WalletType(WalletType),
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
    NetworkAndVersion((String, String)),
    WalletType(WalletType),
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

    pub fn change_passphrase(&self, existing: &SafePassword, new: &SafePassword) -> Result<(), WalletStorageError> {
        self.db.change_passphrase(existing, new)?;
        Ok(())
    }

    pub fn get_master_seed(&self) -> Result<Option<CipherSeed>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::MasterSeed) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::MasterSeed(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::MasterSeed, other),
            Err(e) => log_error(DbKey::MasterSeed, e),
        }?;
        Ok(c)
    }

    pub fn set_master_seed(&self, seed: CipherSeed) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::MasterSeed(seed)))?;
        Ok(())
    }

    pub fn clear_master_seed(&self) -> Result<(), WalletStorageError> {
        self.db.write(WriteOperation::Remove(DbKey::MasterSeed))?;
        Ok(())
    }

    pub fn get_tor_id(&self) -> Result<Option<TorIdentity>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::TorId) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::TorId(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::TorId, other),
            Err(e) => log_error(DbKey::TorId, e),
        }?;
        Ok(c)
    }

    pub fn set_tor_identity(&self, id: TorIdentity) -> Result<(), WalletStorageError> {
        self.db.write(WriteOperation::Insert(DbKeyValuePair::TorId(id)))?;
        Ok(())
    }

    pub fn get_node_address(&self) -> Result<Option<Multiaddr>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::CommsAddress) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsAddress(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::CommsAddress, other),
            Err(e) => log_error(DbKey::CommsAddress, e),
        }?;
        Ok(c)
    }

    pub fn set_node_address(&self, address: Multiaddr) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::CommsAddress(address)))?;
        Ok(())
    }

    pub fn get_node_features(&self) -> Result<Option<PeerFeatures>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::CommsFeatures) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsFeatures(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::CommsFeatures, other),
            Err(e) => log_error(DbKey::CommsFeatures, e),
        }?;
        Ok(c)
    }

    pub fn set_node_features(&self, features: PeerFeatures) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::CommsFeatures(features)))?;
        Ok(())
    }

    pub fn get_comms_identity_signature(&self) -> Result<Option<IdentitySignature>, WalletStorageError> {
        let sig = match self.db.fetch(&DbKey::CommsIdentitySignature) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::CommsIdentitySignature(k))) => Ok(Some(*k)),
            Ok(Some(other)) => unexpected_result(DbKey::CommsIdentitySignature, other),
            Err(e) => log_error(DbKey::CommsIdentitySignature, e),
        }?;
        Ok(sig)
    }

    pub fn set_comms_identity_signature(&self, sig: IdentitySignature) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::CommsIdentitySignature(
                Box::new(sig),
            )))?;
        Ok(())
    }

    pub fn get_chain_metadata(&self) -> Result<Option<ChainMetadata>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::BaseNodeChainMetadata) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::BaseNodeChainMetadata(metadata))) => Ok(Some(metadata)),
            Ok(Some(other)) => unexpected_result(DbKey::BaseNodeChainMetadata, other),
            Err(e) => log_error(DbKey::BaseNodeChainMetadata, e),
        }?;
        Ok(c)
    }

    pub fn set_chain_metadata(&self, metadata: ChainMetadata) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::BaseNodeChainMetadata(metadata)))?;
        Ok(())
    }

    pub fn set_client_key_value(&self, key: String, value: String) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::ClientKeyValue(key, value)))?;
        Ok(())
    }

    pub fn set_last_network_and_version(&self, network: String, version: String) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::NetworkAndVersion((
                network, version,
            ))))?;
        Ok(())
    }

    pub fn get_client_key_value(&self, key: String) -> Result<Option<String>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::ClientKey(key.clone())) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::ClientValue(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::ClientKey(key), other),
            Err(e) => log_error(DbKey::ClientKey(key), e),
        }?;
        Ok(c)
    }

    pub fn get_client_key_from_str<V>(&self, key: String) -> Result<Option<V>, WalletStorageError>
    where
        V: std::str::FromStr,
        V::Err: ToString,
    {
        let value = match self.db.fetch(&DbKey::ClientKey(key.clone())) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::ClientValue(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::ClientKey(key), other),
            Err(e) => log_error(DbKey::ClientKey(key), e),
        }?;

        match value {
            Some(c) => {
                let a = V::from_str(&c).map_err(|err| WalletStorageError::ConversionError(err.to_string()))?;
                Ok(Some(a))
            },
            None => Ok(None),
        }
    }

    pub fn clear_client_value(&self, key: String) -> Result<bool, WalletStorageError> {
        let c = match self.db.write(WriteOperation::Remove(DbKey::ClientKey(key.clone()))) {
            Ok(None) => Ok(false),
            Ok(Some(DbValue::ValueCleared)) => Ok(true),
            Ok(Some(other)) => unexpected_result(DbKey::ClientKey(key), other),
            Err(e) => log_error(DbKey::ClientKey(key), e),
        }?;
        Ok(c)
    }

    pub fn get_wallet_birthday(&self) -> Result<u16, WalletStorageError> {
        let result = match self.db.fetch(&DbKey::WalletBirthday) {
            Ok(None) => Err(WalletStorageError::ValueNotFound(DbKey::WalletBirthday)),
            Ok(Some(DbValue::WalletBirthday(b))) => Ok(b
                .parse::<u16>()
                .map_err(|_| WalletStorageError::ConversionError("Could not parse wallet birthday".to_string()))?),
            Ok(Some(other)) => unexpected_result(DbKey::WalletBirthday, other),
            Err(e) => log_error(DbKey::WalletBirthday, e),
        }?;
        Ok(result)
    }

    pub fn get_scanned_blocks(&self) -> Result<Vec<ScannedBlock>, WalletStorageError> {
        let result = self.db.get_scanned_blocks()?;
        Ok(result)
    }

    pub fn save_scanned_block(&self, scanned_block: ScannedBlock) -> Result<(), WalletStorageError> {
        self.db.save_scanned_block(scanned_block)?;
        Ok(())
    }

    pub fn clear_scanned_blocks(&self) -> Result<(), WalletStorageError> {
        self.db.clear_scanned_blocks()?;
        Ok(())
    }

    pub fn clear_scanned_blocks_from_and_higher(&self, height: u64) -> Result<(), WalletStorageError> {
        self.db.clear_scanned_blocks_from_and_higher(height)?;
        Ok(())
    }

    pub fn clear_scanned_blocks_before_height(
        &self,
        height: u64,
        exclude_recovered: bool,
    ) -> Result<(), WalletStorageError> {
        self.db.clear_scanned_blocks_before_height(height, exclude_recovered)?;
        Ok(())
    }

    pub fn create_burnt_proof(
        &self,
        id: u32,
        reciprocal_claim_public_key: String,
        payload: String,
    ) -> Result<(), WalletStorageError> {
        self.db.create_burnt_proof(id, reciprocal_claim_public_key, payload)?;
        Ok(())
    }

    pub fn fetch_burnt_proof(&self, id: u32) -> Result<(u32, String, String, NaiveDateTime), WalletStorageError> {
        self.db.fetch_burnt_proof(id)
    }

    pub fn fetch_burnt_proofs(&self) -> Result<Vec<(u32, String, String, NaiveDateTime)>, WalletStorageError> {
        self.db.fetch_burnt_proofs()
    }

    pub fn delete_burnt_proof(&self, id: u32) -> Result<(), WalletStorageError> {
        self.db.delete_burnt_proof(id)
    }

    pub fn get_wallet_type(&self) -> Result<Option<WalletType>, WalletStorageError> {
        match self.db.fetch(&DbKey::WalletType) {
            Ok(None) => Ok(None),
            Ok(Some(DbValue::WalletType(k))) => Ok(Some(k)),
            Ok(Some(other)) => unexpected_result(DbKey::WalletType, other),
            Err(e) => log_error(DbKey::WalletType, e),
        }
    }

    pub fn set_wallet_type(&self, wallet_type: WalletType) -> Result<(), WalletStorageError> {
        self.db
            .write(WriteOperation::Insert(DbKeyValuePair::WalletType(wallet_type)))?;
        Ok(())
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
            DbValue::EncryptedMainKey(k) => f.write_str(&format!("EncryptedMainKey: {:?}", k)),
            DbValue::SecondaryKeySalt(s) => f.write_str(&format!("SecondaryKeySalt: {}", s)),
            DbValue::SecondaryKeyVersion(v) => f.write_str(&format!("SecondaryKeyVersion: {}", v)),
            DbValue::SecondaryKeyHash(h) => f.write_str(&format!("SecondaryKeyHash: {}", h)),
            DbValue::WalletBirthday(b) => f.write_str(&format!("WalletBirthday: {}", b)),
            DbValue::CommsIdentitySignature(_) => f.write_str("CommsIdentitySignature"),
            DbValue::LastAccessedNetwork(network) => f.write_str(&format!("LastAccessedNetwork: {}", network)),
            DbValue::LastAccessedVersion(version) => f.write_str(&format!("LastAccessedVersion: {}", version)),
            DbValue::WalletType(wallet_type) => f.write_str(&format!("WalletType: {:?}", wallet_type)),
        }
    }
}

fn log_error<T>(req: DbKey, err: WalletStorageError) -> Result<T, WalletStorageError> {
    error!(
        target: LOG_TARGET,
        "Database access error on request: {}: {}",
        req.to_key_string(),
        err.to_string()
    );
    Err(err)
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, WalletStorageError> {
    let msg = format!(
        "Unexpected result for database query {}. Response: {}",
        req.to_key_string(),
        res
    );
    error!(target: LOG_TARGET, "{}", msg);
    Err(WalletStorageError::UnexpectedResult(msg))
}

#[cfg(test)]
mod test {
    use tari_key_manager::cipher_seed::CipherSeed;
    use tari_test_utils::random::string;
    use tari_utilities::SafePassword;
    use tempfile::tempdir;

    use crate::storage::{
        database::WalletDatabase,
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::run_migration_and_create_sqlite_connection,
    };

    #[test]
    fn test_database_crud() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = tempdir().unwrap().path().to_str().unwrap().to_string();
        let connection = run_migration_and_create_sqlite_connection(format!("{}{}", db_folder, db_name), 16).unwrap();

        let passphrase = SafePassword::from("my secret lovely passphrase");
        let db = WalletDatabase::new(WalletSqliteDatabase::new(connection, passphrase).unwrap());

        // Test wallet settings
        assert!(db.get_master_seed().unwrap().is_none());
        let seed = CipherSeed::new();
        db.set_master_seed(seed.clone()).unwrap();
        let stored_seed = db.get_master_seed().unwrap().unwrap();
        assert_eq!(seed, stored_seed);
        db.clear_master_seed().unwrap();
        assert!(db.get_master_seed().unwrap().is_none());

        let client_key_values = vec![
            ("key1".to_string(), "value1".to_string()),
            ("key2".to_string(), "value2".to_string()),
            ("key3".to_string(), "value3".to_string()),
        ];

        for kv in &client_key_values {
            db.set_client_key_value(kv.0.clone(), kv.1.clone()).unwrap();
        }

        assert!(db.get_client_key_value("wrong".to_string()).unwrap().is_none());

        db.set_client_key_value(client_key_values[0].0.clone(), "updated".to_string())
            .unwrap();

        assert_eq!(
            db.get_client_key_value(client_key_values[0].0.clone())
                .unwrap()
                .unwrap(),
            "updated".to_string()
        );

        assert!(!db.clear_client_value("wrong".to_string()).unwrap());

        assert!(db.clear_client_value(client_key_values[0].0.clone()).unwrap());

        assert!(!db.clear_client_value(client_key_values[0].0.clone()).unwrap());
    }
}
