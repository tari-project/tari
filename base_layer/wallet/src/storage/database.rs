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
use tari_comms::{
    peer_manager::Peer,
    types::{CommsPublicKey, CommsSecretKey},
};

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
    Peer(CommsPublicKey),
    Peers,
    CommsSecretKey,
}

pub enum DbValue {
    Peer(Box<Peer>),
    Peers(Vec<Peer>),
    CommsSecretKey(CommsSecretKey),
}

#[derive(Clone)]
pub enum DbKeyValuePair {
    Peer(CommsPublicKey, Peer),
    CommsSecretKey(CommsSecretKey),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($db:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $db.fetch(&key) {
            Ok(None) => Err(WalletStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
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

    pub async fn get_peer(&self, pub_key: CommsPublicKey) -> Result<Peer, WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || fetch!(db_clone, pub_key.clone(), Peer))
            .await
            .or_else(|err| Err(WalletStorageError::BlockingTaskSpawnError(err.to_string())))
            .and_then(|inner_result| inner_result)
    }

    pub async fn get_peers(&self) -> Result<Vec<Peer>, WalletStorageError> {
        let db_clone = self.db.clone();

        let c = tokio::task::spawn_blocking(move || match db_clone.fetch(&DbKey::Peers) {
            Ok(None) => log_error(
                DbKey::Peers,
                WalletStorageError::UnexpectedResult("Could not retrieve peers".to_string()),
            ),
            Ok(Some(DbValue::Peers(c))) => Ok(c),
            Ok(Some(other)) => unexpected_result(DbKey::Peers, other),
            Err(e) => log_error(DbKey::Peers, e),
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(c)
    }

    pub async fn save_peer(&self, peer: Peer) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::Peer(
                peer.public_key.clone(),
                peer,
            )))
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }

    pub async fn remove_peer(&self, pub_key: CommsPublicKey) -> Result<Peer, WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            match db_clone
                .write(WriteOperation::Remove(DbKey::Peer(pub_key.clone())))?
                .ok_or_else(|| WalletStorageError::ValueNotFound(DbKey::Peer(pub_key.clone())))?
            {
                DbValue::Peer(c) => Ok(*c),
                _ => Err(WalletStorageError::UnexpectedResult(
                    "Incorrect response from backend.".to_string(),
                )),
            }
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))
        .and_then(|inner_result| inner_result)
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

    pub async fn set_comms_private_key(&self, key: CommsSecretKey) -> Result<(), WalletStorageError> {
        let db_clone = self.db.clone();

        tokio::task::spawn_blocking(move || {
            db_clone.write(WriteOperation::Insert(DbKeyValuePair::CommsSecretKey(key)))
        })
        .await
        .map_err(|err| WalletStorageError::BlockingTaskSpawnError(err.to_string()))??;
        Ok(())
    }
}

fn unexpected_result<T>(req: DbKey, res: DbValue) -> Result<T, WalletStorageError> {
    let msg = format!("Unexpected result for database query {}. Response: {}", req, res);
    error!(target: LOG_TARGET, "{}", msg);
    Err(WalletStorageError::UnexpectedResult(msg))
}

impl Display for DbKey {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbKey::Peer(c) => f.write_str(&format!("Peer: {:?}", c)),
            DbKey::Peers => f.write_str(&"Peers".to_string()),
            DbKey::CommsSecretKey => f.write_str(&"CommsSecretKey".to_string()),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Peer(p) => f.write_str(&format!("Peer: {:?}", p)),
            DbValue::Peers(_) => f.write_str(&"Peers".to_string()),
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
    use crate::{
        error::WalletStorageError,
        storage::{
            connection_manager::run_migration_and_create_sqlite_connection,
            database::{DbKey, WalletBackend, WalletDatabase},
            memory_db::WalletMemoryDatabase,
            sqlite_db::WalletSqliteDatabase,
        },
    };
    use rand::rngs::OsRng;
    use tari_comms::{
        multiaddr::Multiaddr,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        types::{CommsPublicKey, CommsSecretKey},
    };
    use tari_core::transactions::types::PublicKey;
    use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey};
    use tari_test_utils::random::string;
    use tempdir::TempDir;
    use tokio::runtime::Runtime;

    pub fn test_database_crud<T: WalletBackend + 'static>(backend: T) {
        let mut runtime = Runtime::new().unwrap();

        let db = WalletDatabase::new(backend);
        let mut peers = Vec::new();
        for i in 0..5 {
            let (_secret_key, public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut OsRng);

            let peer = Peer::new(
                public_key.clone(),
                NodeId::from_key(&public_key).unwrap(),
                "/ip4/1.2.3.4/tcp/9000".parse::<Multiaddr>().unwrap().into(),
                PeerFlags::empty(),
                PeerFeatures::COMMUNICATION_NODE,
                &[],
            );

            peers.push(peer);

            runtime.block_on(db.save_peer(peers[i].clone())).unwrap();

            match runtime.block_on(db.save_peer(peers[i].clone())) {
                Err(WalletStorageError::DuplicateContact) => (),
                _ => assert!(false),
            }
        }

        let got_peers = runtime.block_on(db.get_peers()).unwrap();
        assert_eq!(peers, got_peers);

        let peer = runtime.block_on(db.get_peer(peers[0].public_key.clone())).unwrap();
        assert_eq!(peer, peers[0]);

        let (_secret_key, public_key): (_, PublicKey) = PublicKeyTrait::random_keypair(&mut OsRng);

        match runtime.block_on(db.get_peer(public_key.clone())) {
            Err(WalletStorageError::ValueNotFound(DbKey::Peer(_p))) => (),
            _ => assert!(false),
        }

        match runtime.block_on(db.remove_peer(public_key.clone())) {
            Err(WalletStorageError::ValueNotFound(DbKey::Peer(_p))) => (),
            _ => assert!(false),
        }

        let _ = runtime.block_on(db.remove_peer(peers[0].public_key.clone())).unwrap();
        peers.remove(0);
        let got_peers = runtime.block_on(db.get_peers()).unwrap();

        assert_eq!(peers, got_peers);

        // Test wallet settings
        assert!(runtime.block_on(db.get_comms_secret_key()).unwrap().is_none());
        let secret_key = CommsSecretKey::random(&mut OsRng);
        runtime.block_on(db.set_comms_private_key(secret_key.clone())).unwrap();
        let stored_key = runtime.block_on(db.get_comms_secret_key()).unwrap().unwrap();
        assert_eq!(secret_key, stored_key);
    }

    #[test]
    fn test_database_crud_memory_db() {
        test_database_crud(WalletMemoryDatabase::new());
    }

    #[test]
    fn test_database_crud_sqlite_db() {
        let db_name = format!("{}.sqlite3", string(8).as_str());
        let db_folder = TempDir::new(string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string();
        let connection = run_migration_and_create_sqlite_connection(&format!("{}{}", db_folder, db_name)).unwrap();

        test_database_crud(WalletSqliteDatabase::new(connection));
    }
}
