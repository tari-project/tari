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
use std::fmt::{Display, Error, Formatter};
use tari_comms::{peer_manager::Peer, types::CommsPublicKey};

const LOG_TARGET: &'static str = "wallet::contacts_service::database";

/// This trait defines the functionality that a database backend need to provide for the Contacts Service
pub trait WalletBackend: Send + Sync {
    /// Retrieve the record associated with the provided DbKey
    fn fetch(&self, key: &DbKey) -> Result<Option<DbValue>, WalletStorageError>;
    /// Modify the state the of the backend with a write operation
    fn write(&mut self, op: WriteOperation) -> Result<Option<DbValue>, WalletStorageError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbKey {
    Peer(CommsPublicKey),
    Peers,
}

pub enum DbValue {
    Peer(Box<Peer>),
    Peers(Vec<Peer>),
}

pub enum DbKeyValuePair {
    Peer(CommsPublicKey, Peer),
}

pub enum WriteOperation {
    Insert(DbKeyValuePair),
    Remove(DbKey),
}

// Private macro that pulls out all the boiler plate of extracting a DB query result from its variants
macro_rules! fetch {
    ($self:ident, $key_val:expr, $key_var:ident) => {{
        let key = DbKey::$key_var($key_val);
        match $self.db.fetch(&key) {
            Ok(None) => Err(WalletStorageError::ValueNotFound(key)),
            Ok(Some(DbValue::$key_var(k))) => Ok(*k),
            Ok(Some(other)) => unexpected_result(key, other),
            Err(e) => log_error(key, e),
        }
    }};
}

pub struct WalletDatabase<T>
where T: WalletBackend
{
    db: T,
}

impl<T> WalletDatabase<T>
where T: WalletBackend
{
    pub fn new(db: T) -> Self {
        Self { db }
    }

    pub fn get_peer(&self, pub_key: &CommsPublicKey) -> Result<Peer, WalletStorageError> {
        fetch!(self, pub_key.clone(), Peer)
    }

    pub fn get_peers(&self) -> Result<Vec<Peer>, WalletStorageError> {
        let c = match self.db.fetch(&DbKey::Peers) {
            Ok(None) => log_error(
                DbKey::Peers,
                WalletStorageError::UnexpectedResult("Could not retrieve peers".to_string()),
            ),
            Ok(Some(DbValue::Peers(c))) => Ok(c),
            Ok(Some(other)) => unexpected_result(DbKey::Peers, other),
            Err(e) => log_error(DbKey::Peers, e),
        }?;
        Ok(c)
    }

    pub fn save_peer(&mut self, peer: Peer) -> Result<(), WalletStorageError> {
        self.db.write(WriteOperation::Insert(DbKeyValuePair::Peer(
            peer.public_key.clone(),
            peer,
        )))?;
        Ok(())
    }

    pub fn remove_peer(&mut self, pub_key: &CommsPublicKey) -> Result<Peer, WalletStorageError> {
        match self
            .db
            .write(WriteOperation::Remove(DbKey::Peer(pub_key.clone())))?
            .ok_or(WalletStorageError::ValueNotFound(DbKey::Peer(pub_key.clone())))?
        {
            DbValue::Peer(c) => Ok(*c),
            DbValue::Peers(_) => Err(WalletStorageError::UnexpectedResult(
                "Incorrect response from backend.".to_string(),
            )),
        }
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
            DbKey::Peers => f.write_str(&format!("Peers")),
        }
    }
}

impl Display for DbValue {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            DbValue::Peer(_) => f.write_str(&format!("Peer")),
            DbValue::Peers(_) => f.write_str(&format!("Peers")),
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
            database::{DbKey, WalletDatabase},
            memory_db::WalletMemoryDatabase,
        },
    };
    use tari_comms::{
        connection::{net_address::NetAddressWithStats, NetAddressesWithStats},
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        types::{CommsPublicKey, CommsSecretKey},
    };
    use tari_crypto::keys::PublicKey;

    #[test]
    pub fn test_database_crud() {
        let mut rng = match rand::OsRng::new() {
            Ok(x) => x,
            Err(_) => unimplemented!(),
        };

        let mut db = WalletDatabase::new(WalletMemoryDatabase::new());
        let mut peers = Vec::new();
        for i in 0..5 {
            let (_secret_key, public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut rng);

            let peer = Peer::new(
                public_key.clone(),
                NodeId::from_key(&public_key).unwrap(),
                NetAddressesWithStats::new(vec![NetAddressWithStats::new("1.2.3.4:9000".parse().unwrap())]),
                PeerFlags::empty(),
                PeerFeatures::COMMUNICATION_NODE,
            );

            peers.push(peer);

            db.save_peer(peers[i].clone()).unwrap();
            assert_eq!(
                db.save_peer(peers[i].clone()),
                Err(WalletStorageError::DuplicateContact)
            );
        }

        let got_peers = db.get_peers().unwrap();
        assert_eq!(peers, got_peers);

        let peer = db.get_peer(&peers[0].public_key).unwrap();
        assert_eq!(peer, peers[0]);

        let (_secret_key, public_key) = PublicKey::random_keypair(&mut rng);

        let peer = db.get_peer(&public_key);
        assert_eq!(
            peer,
            Err(WalletStorageError::ValueNotFound(DbKey::Peer(public_key.clone())))
        );
        assert_eq!(
            db.remove_peer(&public_key),
            Err(WalletStorageError::ValueNotFound(DbKey::Peer(public_key.clone())))
        );

        let _ = db.remove_peer(&peers[0].public_key).unwrap();
        peers.remove(0);
        let got_peers = db.get_peers().unwrap();

        assert_eq!(peers, got_peers);
    }
}
